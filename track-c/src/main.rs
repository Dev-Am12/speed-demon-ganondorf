// Track C (Join): AVX2 + VPCLMULQDQ structural CSV join with L1 filter cache.
use std::arch::x86_64::*;
use std::ffi::c_void;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

const MAX_THREADS: usize = 12;
const NAME_SLOTS: usize = 1 << 17;
const INIT_AGG: [i64; 9] = [0, 0, 0, i64::MAX, i64::MIN, 0, 0, 0, 0];

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
    if len > 32 {
        // SAFETY: caller guarantees [p, p + len) is readable.
        return unsafe { hash_long(p, len) };
    }
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

/// Full-coverage hash for names longer than 32 bytes: every byte takes part.
/// Reads 8-byte words at offsets 0, 8, ... while they fit, then one overlapping
/// word at len-8 that covers the tail. Names of 32 bytes or less never use this.
#[cold]
#[inline(never)]
unsafe fn hash_long(p: *const u8, len: usize) -> u64 {
    // SAFETY: len > 32, so every read at i (i + 8 <= len) and at len - 8 is inside [p, p + len).
    unsafe {
        let mut h = (len as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ 0x517c_c1b7_2722_0a95;
        let mut i = 0usize;
        while i + 8 <= len {
            h = wymix(h ^ p.add(i).cast::<u64>().read_unaligned(), 0xa076_1d64_78bd_642f);
            i += 8;
        }
        let tail = p.add(len - 8).cast::<u64>().read_unaligned();
        wymix(h ^ tail, 0xe703_7ed1_a0b4_28db) | 1
    }
}

#[inline(always)]
fn insert_name(keys: &[AtomicU64], masks: &[AtomicU64], h: u64, c: usize) -> bool {
    let (bit, w_off) = (1u64 << (c & 63), c >> 6);
    let mut idx = (h as usize) & (NAME_SLOTS - 1);
    loop {
        // SAFETY: idx < NAME_SLOTS and masks has length NAME_SLOTS * 4.
        let k_slot = unsafe { keys.get_unchecked(idx) };
        let mut cur = k_slot.load(Ordering::Relaxed);
        if cur == 0 {
            cur = match k_slot.compare_exchange(0, h, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => h,
                Err(v) => v,
            };
        }
        if cur == h {
            let m = unsafe { masks.get_unchecked((idx << 2) + w_off) };
            if (m.load(Ordering::Relaxed) & bit) == 0 {
                // Exactly one thread can observe this bit as previously clear.
                return (m.fetch_or(bit, Ordering::Relaxed) & bit) == 0;
            }
            return false;
        }
        idx = (idx + 1) & (NAME_SLOTS - 1);
    }
}

#[inline(always)]
unsafe fn parse_uint(mut p: *const u8, end: *const u8) -> i64 {
    let mut v = 0i64;
    while p < end {
        // SAFETY: caller guarantees p..end lies within the mapped CSV slice.
        v = v * 10 + unsafe { (*p - b'0') as i64 };
        p = unsafe { p.add(1) };
    }
    v
}

fn load_regions(path: &str) -> io::Result<(Vec<&'static [u8]>, Vec<u8>)> {
    let raw = mmap_file(path)?;
    let mut countries: Vec<&'static [u8]> = vec![b"_UNMATCHED"];
    let (mut c_ids, mut rmap) = ([0u8; 32768], vec![0u8; 1_000_000]);
    let mut i = raw
        .iter()
        .position(|&b| b == b'\n')
        .map_or(raw.len(), |p| p + 1);
    let bp = raw.as_ptr();
    while i < raw.len() {
        let mut rid = 0usize;
        while i < raw.len() && unsafe { *bp.add(i) } >= b'0' {
            rid = rid * 10 + unsafe { (*bp.add(i) - b'0') as usize };
            i += 1;
        }
        if i + 3 >= raw.len() {
            break;
        }
        let cs = i + 1;
        let idx = unsafe {
            (((*bp.add(cs) & 31) as usize) << 10)
                | (((*bp.add(cs + 1) & 31) as usize) << 5)
                | ((*bp.add(cs + 2) & 31) as usize)
        };
        let mut cid = c_ids[idx];
        if cid == 0 {
            cid = countries.len() as u8;
            c_ids[idx] = cid;
            countries.push(&raw[cs..cs + 3]);
        }
        if rid < rmap.len() {
            unsafe { *rmap.get_unchecked_mut(rid) = cid };
        }
        i = cs + 4;
        if i <= raw.len() && unsafe { *bp.add(i - 1) } == b'\r' {
            i += 1;
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
) -> Vec<[i64; 9]> {
    let mut aggs = vec![INIT_AGG; num_countries];
    let mut ubuf = vec![0u8; 512];
    let (mut d_pos, mut d_q, mut tail) = ([0u32; 72], [0u32; 72], [0u8; 64]);
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
            // SAFETY: drained whenever n_delims >= 5, so n_delims <= 4 + 64 = 68 < 72.
            unsafe {
                *d_pos.get_unchecked_mut(n_delims) = (blk_start + bit as usize) as u32;
                *d_q.get_unchecked_mut(n_delims) = q_prefix + _bzhi_u64(q_mask, bit).count_ones();
            }
            n_delims += 1;
        }
        q_prefix += q_mask.count_ones();
        blk_start += 64;
        if n_delims >= 5 {
            let mut idx = 0;
            while idx + 5 <= n_delims {
                // SAFETY: idx + 4 < n_delims < 72 and all delimiter offsets lie in bytes.
                unsafe {
                    let pos = |k: usize| *d_pos.get_unchecked(idx + k) as usize;
                    let (c1, c2, c3, c4, nl) = (pos(0), pos(1), pos(2), pos(3), pos(4));
                    let (q1, q2, q3) = (
                        *d_q.get_unchecked(idx),
                        *d_q.get_unchecked(idx + 1),
                        *d_q.get_unchecked(idx + 2),
                    );
                    let rs = rec_start + 3 * usize::from(*bp.add(rec_start) == 0xEF);
                    let id = parse_uint(bp.add(rs), bp.add(c1));
                    let rid = parse_uint(bp.add(c3 + 1), bp.add(c4)) as usize;
                    let country = if rid < 1_000_000 {
                        *region_map.get_unchecked(rid) as usize
                    } else {
                        0
                    };
                    let a_end = nl - usize::from(*bp.add(nl - 1) == b'\r');
                    let neg = usize::from(*bp.add(c4 + 1) == b'-');
                    let whole = parse_uint(bp.add(c4 + 1 + neg), bp.add(a_end - 3));
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
                        let h = hash_name_fast(n_ptr, n_len);
                        if insert_name(keys, masks, h, country) {
                            agg[8] += 1;
                        }
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
            for k in 0..(n_delims - idx) {
                unsafe {
                    *d_pos.get_unchecked_mut(k) = *d_pos.get_unchecked(idx + k);
                    *d_q.get_unchecked_mut(k) = *d_q.get_unchecked(idx + k);
                }
            }
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
    let (raw_keys, raw_masks) = (vec![0u64; NAME_SLOTS], vec![0u64; NAME_SLOTS * 4]);
    // SAFETY: AtomicU64 has identical layout and alignment as u64.
    let (keys, masks) = unsafe {
        (
            std::slice::from_raw_parts(raw_keys.as_ptr().cast::<AtomicU64>(), NAME_SLOTS),
            std::slice::from_raw_parts(raw_masks.as_ptr().cast::<AtomicU64>(), NAME_SLOTS * 4),
        )
    };
    let thread_aggs: Vec<Vec<[i64; 9]>> = thread::scope(|s| {
        let mut hs = Vec::with_capacity(threads);
        for w in bounds.windows(2).filter(|w| w[0] < w[1]) {
            // SAFETY: x86-64-v3 + pclmulqdq is available on the target CPU.
            hs.push(s.spawn(|| unsafe {
                parse_chunk(&body[w[0]..w[1]], &region_map, num_countries, keys, masks)
            }));
        }
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut final_aggs = vec![INIT_AGG; num_countries];
    for t_agg in thread_aggs {
        for (dst, src) in final_aggs.iter_mut().zip(t_agg).filter(|(_, s)| s[0] > 0) {
            for k in [0, 1, 2, 5, 6, 8] {
                dst[k] += src[k];
            }
            dst[3] = dst[3].min(src[3]);
            dst[4] = dst[4].max(src[4]);
            dst[7] |= src[7];
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
                final_aggs[c][7] + final_aggs[c][8],
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
