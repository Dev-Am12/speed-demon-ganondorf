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

// SAFETY: standard POSIX mmap ABI linked via libc.
unsafe extern "C" {
    fn mmap(addr: *mut c_void, len: usize, prot: i32, flags: i32, fd: i32, off: i64) -> *mut c_void;
}

#[derive(Clone, Copy, Default)]
struct CountryAgg {
    count: u64,
    id_sum: u64,
    amount_sum: i64,
    amount_min: i64,
    amount_max: i64,
    name_bytes: u64,
    note_bytes: u64,
    empty_name: u64,
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
    // SAFETY: caller guarantees [p, p + 8) and [p + len - 8, p + len) lie inside mapped/padded memory.
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
        wymix(a ^ 0x9e37_79b9_7f4a_7c15, b ^ ((len as u64) << 32) ^ 0x517c_c1b7_2722_0a95) | 1
    }
}

#[inline(always)]
fn insert_name(keys: &[AtomicU64], masks: &[AtomicU64], h: u64, country: usize) {
    let bit = 1u64 << (country & 63);
    let w_off = country >> 6;
    let mut idx = (h as usize) & (NAME_SLOTS - 1);
    loop {
        // SAFETY: idx < NAME_SLOTS and masks has length NAME_SLOTS * 4.
        let k_slot = unsafe { keys.get_unchecked(idx) };
        let cur = k_slot.load(Ordering::Relaxed);
        if cur == h {
            let m = unsafe { masks.get_unchecked((idx << 2) + w_off) };
            if (m.load(Ordering::Relaxed) & bit) == 0 {
                m.fetch_or(bit, Ordering::Relaxed);
            }
            return;
        }
        if cur == 0
            && k_slot.compare_exchange(0, h, Ordering::Relaxed, Ordering::Relaxed).is_ok()
        {
            unsafe { masks.get_unchecked((idx << 2) + w_off) }.fetch_or(bit, Ordering::Relaxed);
            return;
        }
        idx = (idx + 1) & (NAME_SLOTS - 1);
    }
}

#[inline(always)]
fn parse_u64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0u64, |v, &b| v * 10 + (b - b'0') as u64)
}

#[inline(always)]
unsafe fn parse_swar(end_ptr: *const u8, len: usize) -> u64 {
    // SAFETY: caller ensures end_ptr >= mmap_base + 8 and 1 <= len <= 8.
    let raw = unsafe { end_ptr.sub(8).cast::<u64>().read_unaligned() };
    let d0 = raw & (0x0F0F_0F0F_0F0F_0F0Fu64 << ((8 - len) << 3));
    let d1 = (d0.wrapping_mul(2561) >> 8) & 0x00FF_00FF_00FF_00FF;
    let d2 = (d1.wrapping_mul(6_553_601) >> 16) & 0x0000_FFFF_0000_FFFF;
    d2.wrapping_mul(42_949_672_960_001) >> 32
}

fn load_regions(path: &str) -> io::Result<(Vec<&'static [u8]>, Vec<u8>)> {
    let raw = mmap_file(path)?;
    let mut countries: Vec<&'static [u8]> = vec![b"_UNMATCHED"];
    let (mut c_keys, mut c_ids, mut rmap) = ([0u64; 512], [0u8; 512], vec![0u8; 1_000_000]);
    for line in raw.split(|&b| b == b'\n').skip(1) {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if let Some(c) = line.iter().position(|&b| b == b',') {
            let (rid, c_slice) = (parse_u64(&line[..c]) as usize, &line[c + 1..]);
            let key = c_slice.iter().fold(0u64, |acc, &b| (acc << 8) | b as u64);
            let mut s = (key.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 55) as usize;
            while c_keys[s] != 0 && c_keys[s] != key {
                s = (s + 1) & 511;
            }
            if c_keys[s] == 0 {
                c_keys[s] = key;
                c_ids[s] = countries.len() as u8;
                countries.push(c_slice);
            }
            if rid >= rmap.len() {
                rmap.resize(rid + 1, 0);
            }
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
    p >= 2 && b[p - 1] == b',' && b[p - 2].is_ascii_digit() && b.get(nxt).is_some_and(u8::is_ascii_digit)
}

#[target_feature(enable = "avx2,bmi1,bmi2,lzcnt,popcnt,pclmul")]
unsafe fn parse_chunk(
    bytes: &[u8],
    region_map: &[u8],
    num_countries: usize,
    keys: &[AtomicU64],
    masks: &[AtomicU64],
) -> Vec<CountryAgg> {
    let mut aggs = vec![CountryAgg::default(); num_countries];
    for a in &mut aggs {
        a.amount_min = i64::MAX;
        a.amount_max = i64::MIN;
    }
    let mut ubuf = vec![0u8; 512];
    let (mut d_pos, mut d_q) = ([0u32; 320], [0u32; 320]);
    let (mut n_delims, mut prev_in_quote, mut q_prefix, mut rec_start, mut blk_start) =
        (0usize, 0u64, 0u32, 0usize, 0usize);
    let qv = _mm256_set1_epi8(b'"' as i8);
    let cv = _mm256_set1_epi8(b',' as i8);
    let nv = _mm256_set1_epi8(b'\n' as i8);

    while blk_start < bytes.len() {
        let rem = bytes.len() - blk_start;
        let (ptr, valid) = if rem >= 64 {
            (unsafe { bytes.as_ptr().add(blk_start) }, !0u64)
        } else {
            let mut tail = [0u8; 64];
            tail[..rem].copy_from_slice(&bytes[blk_start..]);
            (tail.as_ptr(), (1u64 << rem) - 1)
        };
        let v0 = unsafe { _mm256_loadu_si256(ptr.cast::<__m256i>()) };
        let v1 = unsafe { _mm256_loadu_si256(ptr.add(32).cast::<__m256i>()) };
        let q0 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(v0, qv)) as u32 as u64;
        let q1 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(v1, qv)) as u32 as u64;
        let cn0 = _mm256_or_si256(_mm256_cmpeq_epi8(v0, cv), _mm256_cmpeq_epi8(v0, nv));
        let cn1 = _mm256_or_si256(_mm256_cmpeq_epi8(v1, cv), _mm256_cmpeq_epi8(v1, nv));
        let s0 = _mm256_movemask_epi8(cn0) as u32 as u64;
        let s1 = _mm256_movemask_epi8(cn1) as u32 as u64;
        let (q_mask, cn_mask) = ((q0 | (q1 << 32)) & valid, (s0 | (s1 << 32)) & valid);

        let clmul = _mm_clmulepi64_si128(_mm_set_epi64x(0, q_mask as i64), _mm_set1_epi8(-1), 0);
        let in_quote = (_mm_cvtsi128_si64(clmul) as u64) ^ prev_in_quote;
        prev_in_quote = ((in_quote as i64) >> 63) as u64;

        let mut sep = cn_mask & !in_quote;
        while sep != 0 {
            let bit = sep.trailing_zeros();
            sep &= sep - 1;
            d_pos[n_delims] = (blk_start + bit as usize) as u32;
            d_q[n_delims] = q_prefix + (q_mask & ((1u64 << bit) - 1)).count_ones();
            n_delims += 1;
        }
        q_prefix += q_mask.count_ones();
        blk_start += 64;

        if n_delims >= 250 || blk_start >= bytes.len() {
            let bp = bytes.as_ptr();
            let mut idx = 0;
            while idx + 5 <= n_delims {
                let (c1, c2, c3, c4, nl) = (
                    d_pos[idx] as usize,
                    d_pos[idx + 1] as usize,
                    d_pos[idx + 2] as usize,
                    d_pos[idx + 3] as usize,
                    d_pos[idx + 4] as usize,
                );
                let (q1, q2, q3) = (d_q[idx], d_q[idx + 1], d_q[idx + 2]);
                let rs = rec_start + 3 * usize::from(bytes[rec_start] == 0xEF);
                // SAFETY: body starts after a >= 29-byte header in mmap_slice, so >= 8 bytes precede c1.
                let id = unsafe { parse_swar(bp.add(c1), c1 - rs) };
                let rid = unsafe { parse_swar(bp.add(c4), c4 - c3 - 1) } as usize;
                let country = region_map.get(rid).copied().unwrap_or(0) as usize;
                let a_end = nl - usize::from(bytes[nl - 1] == b'\r');
                let neg = usize::from(bytes[c4 + 1] == b'-');
                let whole = unsafe { parse_swar(bp.add(a_end - 3), a_end - c4 - 4 - neg) } as i64;
                let frac = (bytes[a_end - 2] - b'0') as i64 * 10 + (bytes[a_end - 1] - b'0') as i64;
                let cents = if neg != 0 { -(whole * 100 + frac) } else { whole * 100 + frac };

                let q_note = (q3 - q2) as usize;
                let note_len = (c3 - c2 - 1) - (q_note >> 1) - usize::from(q_note > 0);
                let q_name = (q2 - q1) as usize;
                let (n_ptr, n_len) = if q_name == 0 {
                    (unsafe { bp.add(c1 + 1) }, c2 - c1 - 1)
                } else if q_name == 2 {
                    (unsafe { bp.add(c1 + 2) }, c2 - c1 - 3)
                } else {
                    let inner = &bytes[c1 + 2..c2 - 1];
                    let (mut i, mut out_len) = (0, 8);
                    if ubuf.len() < inner.len() + 16 {
                        ubuf.resize(inner.len() + 16, 0);
                    }
                    while i < inner.len() {
                        let b = inner[i];
                        ubuf[out_len] = b;
                        out_len += 1;
                        i += 1 + usize::from(b == b'"');
                    }
                    (unsafe { ubuf.as_ptr().add(8) }, out_len - 8)
                };

                // SAFETY: country < num_countries is guaranteed by load_regions.
                let agg = unsafe { aggs.get_unchecked_mut(country) };
                if n_len == 0 {
                    agg.empty_name = 1;
                } else {
                    insert_name(keys, masks, unsafe { hash_name_fast(n_ptr, n_len) }, country);
                }
                agg.count += 1;
                agg.id_sum += id;
                agg.amount_sum += cents;
                agg.amount_min = agg.amount_min.min(cents);
                agg.amount_max = agg.amount_max.max(cents);
                agg.name_bytes += n_len as u64;
                agg.note_bytes += note_len as u64;
                rec_start = nl + 1;
                idx += 5;
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
    let args: Vec<String> = std::env::args().skip(1).filter(|a| !a.starts_with("--")).collect();
    let data_dir = args.first().map_or("/data", String::as_str);
    let out_dir = args.get(1).map_or("/out", String::as_str);

    let (countries, region_map) = load_regions(&format!("{}/regions.csv", data_dir))?;
    let num_countries = countries.len();
    let mmap_slice = mmap_file(&format!("{}/records.csv", data_dir))?;
    let header_end = mmap_slice.iter().position(|&b| b == b'\n').map_or(mmap_slice.len(), |p| p + 1);
    let body = &mmap_slice[header_end..];
    if body.is_empty() {
        fs::create_dir_all(out_dir)?;
        File::create(format!("{}/aggregates.txt", out_dir))?;
        return Ok(());
    }
    let threads = if body.len() < 512 * 1024 {
        1
    } else {
        thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, MAX_THREADS)
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

    let thread_aggs: Vec<Vec<CountryAgg>> = thread::scope(|s| {
        let handles: Vec<_> = bounds
            .windows(2)
            .filter(|w| w[0] < w[1])
            .map(|w| {
                // SAFETY: x86-64-v3 + pclmul is available on the target CPU.
                s.spawn(|| unsafe { parse_chunk(&body[w[0]..w[1]], &region_map, num_countries, keys, masks) })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut final_aggs = vec![CountryAgg::default(); num_countries];
    for a in &mut final_aggs {
        a.amount_min = i64::MAX;
        a.amount_max = i64::MIN;
    }
    for t_agg in thread_aggs {
        for (c, src) in t_agg.into_iter().enumerate().filter(|(_, s)| s.count > 0) {
            let dst = &mut final_aggs[c];
            dst.count += src.count;
            dst.id_sum += src.id_sum;
            dst.amount_sum += src.amount_sum;
            dst.amount_min = dst.amount_min.min(src.amount_min);
            dst.amount_max = dst.amount_max.max(src.amount_max);
            dst.name_bytes += src.name_bytes;
            dst.note_bytes += src.note_bytes;
            dst.empty_name |= src.empty_name;
        }
    }

    let mut distinct_counts: Vec<u64> = final_aggs.iter().map(|a| a.empty_name).collect();
    for (idx, k) in keys.iter().enumerate() {
        if k.load(Ordering::Relaxed) != 0 {
            for w_idx in 0..4 {
                let mut w = masks[(idx << 2) + w_idx].load(Ordering::Relaxed);
                while w != 0 {
                    let c = w_idx * 64 + w.trailing_zeros() as usize;
                    if c < num_countries {
                        distinct_counts[c] += 1;
                    }
                    w &= w - 1;
                }
            }
        }
    }

    let mut order: Vec<usize> = (1..num_countries).collect();
    order.sort_unstable_by(|&a, &b| countries[a].cmp(countries[b]));
    order.push(0);

    let mut out = Vec::with_capacity(32 * 1024);
    for c in order {
        let agg = &final_aggs[c];
        if agg.count > 0 {
            let cname = std::str::from_utf8(countries[c]).unwrap();
            let _ = writeln!(
                out,
                "{} {} {} {} {} {} {} {} {}",
                cname,
                agg.count,
                agg.id_sum,
                fmt_cents(agg.amount_sum),
                fmt_cents(agg.amount_min),
                fmt_cents(agg.amount_max),
                distinct_counts[c],
                agg.name_bytes,
                agg.note_bytes
            );
        }
    }
    fs::create_dir_all(out_dir)?;
    let mut file = File::create(format!("{}/aggregates.txt", out_dir))?;
    file.write_all(&out)?;
    file.flush()?;
    std::process::exit(0);
}