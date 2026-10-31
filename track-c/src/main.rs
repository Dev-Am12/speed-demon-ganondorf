// Track C (Join): Two-stage AVX2 + VPCLMULQDQ structural CSV join with SWAR integer parsing.
use std::arch::x86_64::*;
use std::ffi::c_void;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

const MAX_THREADS: usize = 12;
const NAME_SLOTS: usize = 1 << 17;
const ID_MASK: u64 = (NAME_SLOTS as u64) - 1;
const INIT_AGG: [i64; 8] = [0, 0, 0, i64::MAX, i64::MIN, 0, 0, 0];

// SAFETY: standard POSIX mmap ABI linked via libc.
unsafe extern "C" {
    fn mmap(addr: *mut c_void, len: usize, prot: i32, flags: i32, fd: i32, off: i64)
        -> *mut c_void;
}

fn mmap_file(path: &str) -> io::Result<&'static [u8]> {
    let f = File::open(path)?;
    let len = f.metadata()?.len() as usize;
    if len == 0 {
        return Ok(&[]);
    }
    // SAFETY: len > 0 and f is an open file descriptor mapped read-only.
    unsafe {
        let p = mmap(std::ptr::null_mut(), len, 1, 2, f.as_raw_fd(), 0);
        if p as isize == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(std::slice::from_raw_parts(p.cast::<u8>(), len))
    }
}

#[inline(always)]
fn wymix(a: u64, b: u64) -> u64 {
    let r = (a as u128).wrapping_mul(b as u128);
    (r as u64) ^ ((r >> 64) as u64)
}

#[inline(always)]
unsafe fn hash_name_fast(p: *const u8, len: usize) -> u64 {
    // SAFETY: caller guarantees [p, p + 8) and [p + len - 8, p + len) lie in mapped/padded memory.
    unsafe {
        let sbits = (8 - len.min(8)) << 3;
        let mut a = p.cast::<u64>().read_unaligned() & (!0u64 >> sbits);
        let mut b = p.add(len).sub(8).cast::<u64>().read_unaligned() & (!0u64 << sbits);
        if len > 16 {
            a ^= wymix(p.add(8).cast::<u64>().read_unaligned(), len as u64);
            if len > 24 {
                b ^= wymix(p.add(16).cast::<u64>().read_unaligned(), len as u64);
            }
        }
        wymix(
            a ^ 0x9e37_79b9_7f4a_7c15,
            b ^ ((len as u64) << 32) ^ 0x517c_c1b7_2722_0a95,
        ) | 1
    }
}

#[inline(always)]
fn insert_name(keys: &[AtomicU64], masks: &[AtomicU64], next_id: &AtomicU64, h: u64, c: usize) {
    let tag = (h & !ID_MASK).max(ID_MASK + 1);
    let (bit, w_off) = (1u64 << (c & 63), c >> 6);
    let mut idx = (h as usize) & (NAME_SLOTS - 1);
    loop {
        // SAFETY: idx < NAME_SLOTS and dense id * 4 + w_off < masks.len().
        let k_slot = unsafe { keys.get_unchecked(idx) };
        let mut cur = k_slot.load(Ordering::Relaxed);
        if cur == 0 {
            let new_val = tag | next_id.fetch_add(1, Ordering::Relaxed);
            cur = match k_slot.compare_exchange(0, new_val, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => new_val,
                Err(v) => v,
            };
        }
        if (cur & !ID_MASK) == tag {
            let id = (cur & ID_MASK) as usize;
            let m = unsafe { masks.get_unchecked((id << 2) + w_off) };
            if (m.load(Ordering::Relaxed) & bit) == 0 {
                m.fetch_or(bit, Ordering::Relaxed);
            }
            return;
        }
        idx = (idx + 1) & (NAME_SLOTS - 1);
    }
}

#[inline(always)]
unsafe fn parse_swar(end_ptr: *const u8, len: usize) -> i64 {
    // SAFETY: caller ensures end_ptr >= mmap_base + 8 and 1 <= len <= 8.
    let raw = unsafe { end_ptr.sub(8).cast::<u64>().read_unaligned() };
    let d0 = raw & (0x0F0F_0F0F_0F0F_0F0Fu64 << ((8 - len) << 3));
    let d1 = (d0.wrapping_mul(2561) >> 8) & 0x00FF_00FF_00FF_00FF;
    let d2 = (d1.wrapping_mul(6_553_601) >> 16) & 0x0000_FFFF_0000_FFFF;
    (d2.wrapping_mul(42_949_672_960_001) >> 32) as i64
}

fn load_regions(path: &str) -> io::Result<(Vec<&'static [u8]>, Vec<u8>)> {
    let raw = mmap_file(path)?;
    let mut countries: Vec<&'static [u8]> = vec![b"_UNMATCHED"];
    let (mut c_keys, mut c_ids, mut rmap) = ([0u64; 512], [0u8; 512], vec![0u8; 1_000_000]);
    let mut i = raw
        .iter()
        .position(|&b| b == b'\n')
        .map_or(raw.len(), |p| p + 1);
    while i < raw.len() {
        let mut rid = 0usize;
        while i < raw.len() && raw[i] >= b'0' {
            rid = rid * 10 + (raw[i] - b'0') as usize;
            i += 1;
        }
        if i >= raw.len() {
            break;
        }
        i += 1;
        let (cs, mut key) = (i, 0u64);
        while i < raw.len() && raw[i] > b'\r' {
            key = (key << 8) | raw[i] as u64;
            i += 1;
        }
        let ce = i;
        while i < raw.len() && raw[i] <= b'\r' {
            i += 1;
        }
        let mut s = (key.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 55) as usize;
        while c_keys[s] != 0 && c_keys[s] != key {
            s = (s + 1) & 511;
        }
        if c_keys[s] == 0 {
            c_keys[s] = key;
            c_ids[s] = countries.len() as u8;
            countries.push(&raw[cs..ce]);
        }
        if rid < rmap.len() {
            rmap[rid] = c_ids[s];
        }
    }
    Ok((countries, rmap))
}

fn is_record_end(b: &[u8], nl: usize) -> bool {
    let e = nl - usize::from(nl > 0 && b[nl - 1] == b'\r');
    if e < 6
        || b[e - 3] != b'.'
        || !b[e - 1].is_ascii_digit()
        || !b[e - 2].is_ascii_digit()
        || !b[e - 4].is_ascii_digit()
    {
        return false;
    }
    let mut p = e - 4;
    while p > 0 && b[p - 1].is_ascii_digit() {
        p -= 1;
    }
    p -= usize::from(p > 0 && b[p - 1] == b'-');
    let nxt = nl + 1 + 3 * usize::from(b.get(nl + 1..nl + 4) == Some(b"\xEF\xBB\xBF"));
    p >= 2
        && b[p - 1] == b','
        && b[p - 2].is_ascii_digit()
        && b.get(nxt).is_some_and(u8::is_ascii_digit)
}

#[target_feature(enable = "avx2,bmi1,bmi2,lzcnt,popcnt,pclmulqdq")]
unsafe fn parse_chunk(
    bytes: &[u8],
    region_map: &[u8],
    num_countries: usize,
    keys: &[AtomicU64],
    masks: &[AtomicU64],
    next_id: &AtomicU64,
) -> Vec<[i64; 8]> {
    let mut aggs = vec![INIT_AGG; num_countries];
    let mut ubuf = vec![0u8; 512];
    let (mut d_pos, mut d_q, mut tail) = ([0u32; 320], [0u32; 320], [0u8; 64]);
    let (mut n_delims, mut prev_in_quote, mut q_prefix, mut rec_start, mut blk_start) =
        (0usize, 0u64, 0u32, 0usize, 0usize);
    let (qv, cv) = (_mm256_set1_epi8(b'"' as i8), _mm256_set1_epi8(b',' as i8));
    let (nv, ones) = (_mm256_set1_epi8(b'\n' as i8), _mm_set1_epi8(-1));
    let bp = bytes.as_ptr();
    while blk_start < bytes.len() {
        let rem = bytes.len() - blk_start;
        let ptr = if rem >= 64 {
            unsafe { bp.add(blk_start) }
        } else {
            tail[..rem].copy_from_slice(&bytes[blk_start..]);
            tail[rem..].fill(0);
            tail.as_ptr()
        };
        let v0 = unsafe { _mm256_loadu_si256(ptr.cast::<__m256i>()) };
        let v1 = unsafe { _mm256_loadu_si256(ptr.add(32).cast::<__m256i>()) };
        let q0 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(v0, qv)) as u32 as u64;
        let q1 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(v1, qv)) as u32 as u64;
        let cn0 = _mm256_or_si256(_mm256_cmpeq_epi8(v0, cv), _mm256_cmpeq_epi8(v0, nv));
        let cn1 = _mm256_or_si256(_mm256_cmpeq_epi8(v1, cv), _mm256_cmpeq_epi8(v1, nv));
        let q_mask = q0 | (q1 << 32);
        let cn_mask = (_mm256_movemask_epi8(cn0) as u32 as u64)
            | ((_mm256_movemask_epi8(cn1) as u32 as u64) << 32);
        let clmul = _mm_clmulepi64_si128(_mm_cvtsi64_si128(q_mask as i64), ones, 0);
        let in_quote = (_mm_cvtsi128_si64(clmul) as u64) ^ prev_in_quote;
        prev_in_quote = ((in_quote as i64) >> 63) as u64;
        let mut sep = cn_mask & !in_quote;
        while sep != 0 {
            let bit = sep.trailing_zeros();
            sep &= sep - 1;
            // SAFETY: Stage 2 drains whenever n_delims >= 250, so n_delims + 64 <= 313 < 320.
            unsafe {
                *d_pos.get_unchecked_mut(n_delims) = (blk_start + bit as usize) as u32;
                *d_q.get_unchecked_mut(n_delims) = q_prefix + _bzhi_u64(q_mask, bit).count_ones();
            }
            n_delims += 1;
        }
        q_prefix += q_mask.count_ones();
        blk_start += 64;
        if n_delims >= 250 || blk_start >= bytes.len() {
            let mut idx = 0;
            while idx + 5 <= n_delims {
                // SAFETY: idx + 4 < n_delims < 320 and all delimiter positions lie inside bytes.
                unsafe {
                    let pos = |k: usize| *d_pos.get_unchecked(idx + k) as usize;
                    let (c1, c2, c3, c4, nl) = (pos(0), pos(1), pos(2), pos(3), pos(4));
                    let (q1, q2, q3) = (
                        *d_q.get_unchecked(idx),
                        *d_q.get_unchecked(idx + 1),
                        *d_q.get_unchecked(idx + 2),
                    );
                    let rs = rec_start + 3 * usize::from(*bp.add(rec_start) == 0xEF);
                    let id = parse_swar(bp.add(c1), c1 - rs);
                    let rid = parse_swar(bp.add(c4), c4 - c3 - 1) as usize;
                    let country = if rid < 1_000_000 {
                        *region_map.get_unchecked(rid) as usize
                    } else {
                        0
                    };
                    let a_end = nl - usize::from(*bp.add(nl - 1) == b'\r');
                    let neg = usize::from(*bp.add(c4 + 1) == b'-');
                    let whole = parse_swar(bp.add(a_end - 3), a_end - c4 - 4 - neg);
                    let frac = (*bp.add(a_end - 2) - b'0') as i64 * 10
                        + (*bp.add(a_end - 1) - b'0') as i64;
                    let cents = if neg != 0 {
                        -(whole * 100 + frac)
                    } else {
                        whole * 100 + frac
                    };
                    let q_note = (q3 - q2) as usize;
                    let note_len = ((c3 - c2 - 1) - (q_note >> 1) - usize::from(q_note > 0)) as i64;
                    let q_name = (q2 - q1) as usize;
                    let (n_ptr, n_len) = if q_name <= 2 {
                        (bp.add(c1 + 1 + (q_name >> 1)), c2 - c1 - 1 - q_name)
                    } else {
                        let inner = &bytes[c1 + 2..c2 - 1];
                        let (mut i, mut out_len) = (0, 8);
                        if ubuf.len() < inner.len() + 16 {
                            ubuf.resize(inner.len() + 16, 0);
                        }
                        while i < inner.len() {
                            let b = *inner.get_unchecked(i);
                            *ubuf.get_unchecked_mut(out_len) = b;
                            out_len += 1;
                            i += 1 + usize::from(b == b'"');
                        }
                        (ubuf.as_ptr().add(8), out_len - 8)
                    };
                    let agg = aggs.get_unchecked_mut(country);
                    if n_len == 0 {
                        agg[7] = 1;
                    } else {
                        insert_name(keys, masks, next_id, hash_name_fast(n_ptr, n_len), country);
                    }
                    agg[0] += 1;
                    agg[1] += id;
                    agg[2] += cents;
                    agg[3] = agg[3].min(cents);
                    agg[4] = agg[4].max(cents);
                    agg[5] += n_len as i64;
                    agg[6] += note_len;
                    rec_start = nl + 1;
                    idx += 5;
                }
            }
            d_pos.copy_within(idx..n_delims, 0);
            d_q.copy_within(idx..n_delims, 0);
            n_delims -= idx;
        }
    }
    aggs
}

fn fmt_cents(c: i64) -> String {
    let a = c.unsigned_abs();
    format!("{}{}.{:02}", if c < 0 { "-" } else { "" }, a / 100, a % 100)
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();
    let data_dir = args.first().map_or("/data", String::as_str);
    let out_dir = args.get(1).map_or("/out", String::as_str);
    let (countries, region_map) = load_regions(&format!("{data_dir}/regions.csv"))?;
    let num_countries = countries.len();
    let mmap_slice = mmap_file(&format!("{data_dir}/records.csv"))?;
    let header_end = mmap_slice
        .iter()
        .position(|&b| b == b'\n')
        .map_or(mmap_slice.len(), |p| p + 1);
    let body = &mmap_slice[header_end..];
    if body.is_empty() {
        fs::create_dir_all(out_dir)?;
        File::create(format!("{out_dir}/aggregates.txt"))?;
        return Ok(());
    }
    let threads = if body.len() < 512 * 1024 {
        1
    } else {
        thread::available_parallelism()
            .map_or(1, |n| n.get())
            .clamp(1, MAX_THREADS)
    };
    let mut bounds = vec![0usize; threads + 1];
    for t in 1..threads {
        let mut pos = body.len() * t / threads;
        while pos < body.len() && !(body[pos] == b'\n' && is_record_end(body, pos)) {
            pos += 1;
        }
        bounds[t] = (pos + 1).min(body.len());
    }
    bounds[threads] = body.len();
    let mask_cap = (NAME_SLOTS + 4096) * 4;
    let (raw_keys, raw_masks, next_id) = (
        vec![0u64; NAME_SLOTS],
        vec![0u64; mask_cap],
        AtomicU64::new(0),
    );
    // SAFETY: AtomicU64 has identical layout and alignment as u64.
    let (keys, masks) = unsafe {
        (
            std::slice::from_raw_parts(raw_keys.as_ptr().cast::<AtomicU64>(), NAME_SLOTS),
            std::slice::from_raw_parts(raw_masks.as_ptr().cast::<AtomicU64>(), mask_cap),
        )
    };
    let thread_aggs: Vec<Vec<[i64; 8]>> = thread::scope(|s| {
        let mut hs = Vec::with_capacity(threads);
        for w in bounds.windows(2).filter(|w| w[0] < w[1]) {
            // SAFETY: x86-64-v3 + pclmulqdq is available on the target CPU.
            hs.push(s.spawn(|| unsafe {
                parse_chunk(
                    &body[w[0]..w[1]],
                    &region_map,
                    num_countries,
                    keys,
                    masks,
                    &next_id,
                )
            }));
        }
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut final_aggs = vec![INIT_AGG; num_countries];
    for t_agg in thread_aggs {
        for (dst, src) in final_aggs.iter_mut().zip(t_agg).filter(|(_, s)| s[0] > 0) {
            for k in [0, 1, 2, 5, 6] {
                dst[k] += src[k];
            }
            dst[3] = dst[3].min(src[3]);
            dst[4] = dst[4].max(src[4]);
            dst[7] |= src[7];
        }
    }
    let mut distinct_counts: Vec<i64> = final_aggs.iter().map(|a| a[7]).collect();
    let used_ids = (next_id.load(Ordering::Relaxed) as usize).min(NAME_SLOTS + 4096);
    for id in 0..used_ids {
        for w_idx in 0..4 {
            let mut w = masks[(id << 2) + w_idx].load(Ordering::Relaxed);
            while w != 0 {
                distinct_counts[w_idx * 64 + w.trailing_zeros() as usize] += 1;
                w &= w - 1;
            }
        }
    }
    let mut order: Vec<usize> = (1..num_countries).collect();
    order.sort_unstable_by(|&a, &b| countries[a].cmp(countries[b]));
    order.push(0);
    let mut out = Vec::with_capacity(32 * 1024);
    for c in order {
        let agg = &final_aggs[c];
        if agg[0] > 0 {
            let cname = std::str::from_utf8(countries[c]).unwrap();
            let (s, mn, mx, dc) = (
                fmt_cents(agg[2]),
                fmt_cents(agg[3]),
                fmt_cents(agg[4]),
                distinct_counts[c],
            );
            let _ = writeln!(
                out,
                "{cname} {} {} {s} {mn} {mx} {dc} {} {}",
                agg[0], agg[1], agg[5], agg[6]
            );
        }
    }
    fs::create_dir_all(out_dir)?;
    let mut file = File::create(format!("{out_dir}/aggregates.txt"))?;
    file.write_all(&out)?;
    file.flush()?;
    std::process::exit(0);
}
