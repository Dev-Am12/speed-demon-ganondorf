// Track C (Join): Single-pass AVX2 + PCLMULQDQ structural CSV join in std-only Rust (D = 1).
use std::arch::x86_64::{
    __m256i, _mm256_cmpeq_epi8, _mm256_loadu_si256, _mm256_movemask_epi8, _mm256_or_si256,
    _mm256_set1_epi8, _mm_clmulepi64_si128, _mm_cvtsi128_si64, _mm_set1_epi8, _mm_set_epi64x,
};
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
    fn mmap(addr: *mut c_void, len: usize, prot: i32, flags: i32, fd: i32, off: i64)
        -> *mut c_void;
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
    // SAFETY: len > 0 and f is a valid open file descriptor mapped read-only.
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
fn hash_name(s: &[u8]) -> u64 {
    let len = s.len();
    let p = s.as_ptr();
    // SAFETY: slice bounds are checked in each branch before unaligned reads.
    let (w0, w1) = unsafe {
        if len >= 8 {
            let mut a = p.cast::<u64>().read_unaligned();
            let mut b = p.add(len - 8).cast::<u64>().read_unaligned();
            if len > 16 {
                a ^= wymix(p.add(8).cast::<u64>().read_unaligned(), len as u64);
                if len > 24 {
                    b ^= wymix(p.add(16).cast::<u64>().read_unaligned(), len as u64);
                }
            }
            (a, b)
        } else if len >= 4 {
            (
                p.cast::<u32>().read_unaligned() as u64,
                p.add(len - 4).cast::<u32>().read_unaligned() as u64,
            )
        } else {
            (
                (s[0] as u64) | ((s[len >> 1] as u64) << 8) | ((s[len - 1] as u64) << 16),
                0,
            )
        }
    };
    wymix(
        w0 ^ 0x9e37_79b9_7f4a_7c15,
        w1 ^ ((len as u64) << 32) ^ 0x517c_c1b7_2722_0a95,
    ) | 1
}

#[inline(always)]
fn insert_name(table: &[AtomicU64], h: u64, country: usize) {
    let word_off = 1 + (country >> 6);
    let bit = 1u64 << (country & 63);
    let mut idx = (h as usize) & (NAME_SLOTS - 1);
    loop {
        let base = idx * 5;
        // SAFETY: idx is masked by NAME_SLOTS - 1 and table has length NAME_SLOTS * 5.
        let cur = unsafe { table.get_unchecked(base).load(Ordering::Relaxed) };
        if cur == h {
            let m = unsafe { table.get_unchecked(base + word_off) };
            if (m.load(Ordering::Relaxed) & bit) == 0 {
                m.fetch_or(bit, Ordering::Relaxed);
            }
            return;
        }
        if cur == 0 {
            if unsafe {
                table
                    .get_unchecked(base)
                    .compare_exchange(0, h, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
            } {
                unsafe { table.get_unchecked(base + word_off) }.fetch_or(bit, Ordering::Relaxed);
                return;
            }
            continue;
        }
        idx = (idx + 1) & (NAME_SLOTS - 1);
    }
}

#[inline(always)]
fn parse_u64(bytes: &[u8]) -> u64 {
    let mut v = 0u64;
    for &b in bytes {
        v = v * 10 + (b - b'0') as u64;
    }
    v
}

#[inline(always)]
fn parse_cents(bytes: &[u8]) -> i64 {
    let neg = (bytes[0] == b'-') as usize;
    let n = bytes.len();
    let mut whole = 0i64;
    for &b in &bytes[neg..n - 3] {
        whole = whole * 10 + (b - b'0') as i64;
    }
    let cents = whole * 100 + (bytes[n - 2] - b'0') as i64 * 10 + (bytes[n - 1] - b'0') as i64;
    if neg != 0 { -cents } else { cents }
}

fn load_regions(path: &str) -> io::Result<(Vec<&'static [u8]>, Vec<u8>)> {
    let raw = mmap_file(path)?;
    let mut countries: Vec<&'static [u8]> = Vec::with_capacity(256);
    countries.push(b"_UNMATCHED");
    let mut c_keys = [0u64; 512];
    let mut c_ids = [0u8; 512];
    let mut region_map = vec![0u8; 1_000_000];
    let mut i = raw.iter().position(|&b| b == b'\n').map_or(raw.len(), |p| p + 1);
    while i < raw.len() {
        let mut rid = 0usize;
        while i < raw.len() {
            let b = raw[i];
            i += 1;
            if b == b',' {
                break;
            }
            if b.is_ascii_digit() {
                rid = rid * 10 + (b - b'0') as usize;
            }
        }
        let c_start = i;
        while i < raw.len() && raw[i] != b'\n' {
            i += 1;
        }
        let c_end = i - (i > c_start && raw[i - 1] == b'\r') as usize;
        i += 1;
        if c_end > c_start {
            let c_slice = &raw[c_start..c_end];
            let mut key = 0u64;
            for &b in c_slice {
                key = (key << 8) | (b as u64);
            }
            let mut s = (key.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 55) as usize;
            let cid = loop {
                if c_keys[s] == key {
                    break c_ids[s];
                }
                if c_keys[s] == 0 {
                    let id = countries.len() as u8;
                    c_keys[s] = key;
                    c_ids[s] = id;
                    countries.push(c_slice);
                    break id;
                }
                s = (s + 1) & 511;
            };
            if rid >= region_map.len() {
                region_map.resize(rid + 1, 0);
            }
            region_map[rid] = cid;
        }
    }
    Ok((countries, region_map))
}

fn is_record_end(body: &[u8], nl: usize) -> bool {
    let end = nl - (nl > 0 && body[nl - 1] == b'\r') as usize;
    if end < 6
        || body[end - 3] != b'.'
        || !body[end - 1].is_ascii_digit()
        || !body[end - 2].is_ascii_digit()
        || !body[end - 4].is_ascii_digit()
    {
        return false;
    }
    let mut p = end - 4;
    while p > 0 && body[p - 1].is_ascii_digit() {
        p -= 1;
    }
    if p > 0 && body[p - 1] == b'-' {
        p -= 1;
    }
    if p < 2 || body[p - 1] != b',' || !body[p - 2].is_ascii_digit() {
        return false;
    }
    let mut next = nl + 1;
    if next >= body.len() {
        return true;
    }
    if next + 3 <= body.len() && &body[next..next + 3] == b"\xEF\xBB\xBF" {
        next += 3;
    }
    next < body.len() && body[next].is_ascii_digit()
}

#[inline(always)]
unsafe fn block_masks_64(ptr: *const u8) -> (u64, u64) {
    // SAFETY: caller ensures at least 64 readable bytes at ptr.
    unsafe {
        let v0 = _mm256_loadu_si256(ptr.cast::<__m256i>());
        let v1 = _mm256_loadu_si256(ptr.add(32).cast::<__m256i>());
        let qv = _mm256_set1_epi8(b'"' as i8);
        let cv = _mm256_set1_epi8(b',' as i8);
        let nv = _mm256_set1_epi8(b'\n' as i8);
        let q0 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(v0, qv)) as u32 as u64;
        let q1 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(v1, qv)) as u32 as u64;
        let cn0 = _mm256_or_si256(_mm256_cmpeq_epi8(v0, cv), _mm256_cmpeq_epi8(v0, nv));
        let cn1 = _mm256_or_si256(_mm256_cmpeq_epi8(v1, cv), _mm256_cmpeq_epi8(v1, nv));
        let s0 = _mm256_movemask_epi8(cn0) as u32 as u64;
        let s1 = _mm256_movemask_epi8(cn1) as u32 as u64;
        (q0 | (q1 << 32), s0 | (s1 << 32))
    }
}

fn parse_chunk(
    bytes: &[u8],
    region_map: &[u8],
    num_countries: usize,
    name_table: &[AtomicU64],
) -> Vec<CountryAgg> {
    let mut aggs = vec![
        CountryAgg {
            amount_min: i64::MAX,
            amount_max: i64::MIN,
            .. CountryAgg::default()
        };
        num_countries
    ];
    let mut unescape_buf = Vec::with_capacity(256);
    let (mut prev_in_quote, mut q_prefix, mut rec_start, mut field_idx) = (0u64, 0u32, 0usize, 0usize);
    let (mut d, mut q) = ([0usize; 4], [0u32; 4]);
    let mut blk_start = 0usize;
    while blk_start < bytes.len() {
        let rem = bytes.len() - blk_start;
        let (q_mask, cn_mask) = if rem >= 64 {
            // SAFETY: rem >= 64 guarantees 64 readable bytes at blk_start.
            unsafe { block_masks_64(bytes.as_ptr().add(blk_start)) }
        } else {
            let mut tail = [0u8; 64];
            tail[..rem].copy_from_slice(&bytes[blk_start..]);
            let valid = (1u64 << rem) - 1;
            // SAFETY: tail is a 64-byte stack buffer.
            let (qm, sm) = unsafe { block_masks_64(tail.as_ptr()) };
            (qm & valid, sm & valid)
        };
        // SAFETY: PCLMULQDQ is baseline on x86-64-v3.
        let in_quote = unsafe {
            let clmul = _mm_clmulepi64_si128(_mm_set_epi64x(0, q_mask as i64), _mm_set1_epi8(-1), 0);
            (_mm_cvtsi128_si64(clmul) as u64) ^ prev_in_quote
        };
        prev_in_quote = ((in_quote as i64) >> 63) as u64;
        let mut sep = cn_mask & !in_quote;
        while sep != 0 {
            let bit = sep.trailing_zeros() as usize;
            sep &= sep - 1;
            let pos = blk_start + bit;
            if field_idx < 4 {
                d[field_idx] = pos;
                q[field_idx] = q_prefix + (q_mask & ((1u64 << bit) - 1)).count_ones();
                field_idx += 1;
            } else {
                let mut rs = rec_start;
                if bytes[rs] == 0xEF {
                    rs += 3;
                }
                let id = parse_u64(&bytes[rs..d[0]]);
                let rid = parse_u64(&bytes[d[2] + 1..d[3]]) as usize;
                let country = region_map.get(rid).copied().unwrap_or(0) as usize;
                let amount_end = pos - (bytes[pos - 1] == b'\r') as usize;
                let cents = parse_cents(&bytes[d[3] + 1..amount_end]);
                let q_note = (q[2] - q[1]) as usize;
                let note_raw = d[2] - (d[1] + 1);
                let note_len = if q_note > 0 { note_raw - 1 - (q_note >> 1) } else { note_raw };
                let q_name = (q[1] - q[0]) as usize;
                let name_start = d[0] + 1;
                let name_slice = if q_name == 0 {
                    &bytes[name_start..d[1]]
                } else if q_name == 2 {
                    &bytes[name_start + 1..d[1] - 1]
                } else {
                    let inner = &bytes[name_start + 1..d[1] - 1];
                    unescape_buf.clear();
                    let mut i = 0;
                    while i < inner.len() {
                        let b = inner[i];
                        unescape_buf.push(b);
                        i += 1 + usize::from(b == b'"');
                    }
                    &unescape_buf[..]
                };
                // SAFETY: country < num_countries is guaranteed by region_map values.
                let agg = unsafe { aggs.get_unchecked_mut(country) };
                if name_slice.is_empty() {
                    agg.empty_name = 1;
                } else {
                    insert_name(name_table, hash_name(name_slice), country);
                }
                agg.count += 1;
                agg.id_sum += id;
                agg.amount_sum += cents;
                if cents < agg.amount_min {
                    agg.amount_min = cents;
                }
                if cents > agg.amount_max {
                    agg.amount_max = cents;
                }
                agg.name_bytes += name_slice.len() as u64;
                agg.note_bytes += note_len as u64;
                rec_start = pos + 1;
                field_idx = 0;
            }
        }
        q_prefix += q_mask.count_ones();
        blk_start += 64;
    }
    aggs
}

fn push_u64(out: &mut Vec<u8>, mut v: u64) {
    let mut buf = [0u8; 20];
    let mut i = 20;
    loop {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    out.extend_from_slice(&buf[i..]);
}

fn push_cents(out: &mut Vec<u8>, cents: i64) {
    if cents < 0 {
        out.push(b'-');
    }
    let abs = cents.unsigned_abs();
    push_u64(out, abs / 100);
    let rem = (abs % 100) as u8;
    out.extend_from_slice(&[b'.', b'0' + rem / 10, b'0' + rem % 10]);
}

fn main() -> io::Result<()> {
    let mut data_dir = "/data";
    let mut out_dir = "/out";
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--data" && i + 1 < args.len() {
            data_dir = &args[i + 1];
            i += 2;
        } else if args[i] == "--out" && i + 1 < args.len() {
            out_dir = &args[i + 1];
            i += 2;
        } else {
            if i == 1 {
                data_dir = &args[1];
            } else if i == 2 {
                out_dir = &args[2];
            }
            i += 1;
        }
    }
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
        while pos < body.len() {
            if body[pos] == b'\n' && is_record_end(body, pos) {
                pos += 1;
                break;
            }
            pos += 1;
        }
        bounds[t] = pos;
    }
    bounds[threads] = body.len();

    let raw_table: Vec<u64> = vec![0u64; NAME_SLOTS * 5];
    // SAFETY: AtomicU64 has identical size, alignment, and bit validity as u64.
    let name_table: &[AtomicU64] =
        unsafe { std::slice::from_raw_parts(raw_table.as_ptr().cast::<AtomicU64>(), raw_table.len()) };

    let thread_aggs: Vec<Vec<CountryAgg>> = thread::scope(|s| {
        let handles: Vec<_> = bounds
            .windows(2)
            .filter(|w| w[0] < w[1])
            .map(|w| s.spawn(|| parse_chunk(&body[w[0]..w[1]], &region_map, num_countries, name_table)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut final_aggs = vec![
        CountryAgg {
            amount_min: i64::MAX,
            amount_max: i64::MIN,
            ..CountryAgg::default()
        };
        num_countries
    ];
    for t_agg in thread_aggs {
        for (c, src) in t_agg.into_iter().enumerate() {
            if src.count == 0 {
                continue;
            }
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
    for slot in name_table.chunks_exact(5) {
        if slot[0].load(Ordering::Relaxed) != 0 {
            for w_idx in 0..4 {
                let mut w = slot[1 + w_idx].load(Ordering::Relaxed);
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
        if agg.count == 0 {
            continue;
        }
        out.extend_from_slice(countries[c]);
        out.push(b' ');
        push_u64(&mut out, agg.count);
        out.push(b' ');
        push_u64(&mut out, agg.id_sum);
        out.push(b' ');
        push_cents(&mut out, agg.amount_sum);
        out.push(b' ');
        push_cents(&mut out, agg.amount_min);
        out.push(b' ');
        push_cents(&mut out, agg.amount_max);
        out.push(b' ');
        push_u64(&mut out, distinct_counts[c]);
        out.push(b' ');
        push_u64(&mut out, agg.name_bytes);
        out.push(b' ');
        push_u64(&mut out, agg.note_bytes);
        out.push(b'\n');
    }
    fs::create_dir_all(out_dir)?;
    let mut file = File::create(format!("{}/aggregates.txt", out_dir))?;
    file.write_all(&out)?;
    file.flush()?;
    std::process::exit(0);
}