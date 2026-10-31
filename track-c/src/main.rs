// Track C (Join): SIMD structural CSV parser + lock-free 256-bit country bitmask aggregation.
// Implements the hostile CSV join contract defined in specs/c-join.md.
// Rust standard library only, zero external dependencies (D = 1).

use std::arch::x86_64::{
    __m256i, _mm256_cmpeq_epi8, _mm256_loadu_si256, _mm256_movemask_epi8, _mm256_set1_epi8,
    _mm_clmulepi64_si128, _mm_cvtsi128_si64, _mm_set1_epi8, _mm_set_epi64x,
};
use std::ffi::c_void;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

const PROT_READ: i32 = 1;
const MAP_PRIVATE: i32 = 2;
const MAX_THREADS: usize = 12;
const NAME_TABLE_SLOTS: usize = 1 << 17; // 131,072 slots (~6.3 MB, fits in 24 MiB L3 cache)

// SAFETY: standard POSIX mmap signature linked via host libc.
unsafe extern "C" {
    fn mmap(
        addr: *mut c_void,
        len: usize,
        prot: i32,
        flags: i32,
        fd: i32,
        offset: i64,
    ) -> *mut c_void;
}

#[derive(Clone, Copy)]
struct CountryAgg {
    count: u64,
    id_sum: u64,
    amount_sum: i64,
    amount_min: i64,
    amount_max: i64,
    name_bytes: u64,
    note_bytes: u64,
}

impl Default for CountryAgg {
    fn default() -> Self {
        Self {
            count: 0,
            id_sum: 0,
            amount_sum: 0,
            amount_min: i64::MAX,
            amount_max: i64::MIN,
            name_bytes: 0,
            note_bytes: 0,
        }
    }
}

#[repr(C)]
struct NameSlot {
    h1: AtomicU64,
    h2: AtomicU64,
    mask: [AtomicU64; 4],
}

struct SharedNameTable {
    slots: Vec<NameSlot>,
}

impl SharedNameTable {
    fn new(cap: usize) -> Self {
        let mut slots = Vec::with_capacity(cap);
        for _ in 0..cap {
            slots.push(NameSlot {
                h1: AtomicU64::new(0),
                h2: AtomicU64::new(0),
                mask: [
                    AtomicU64::new(0),
                    AtomicU64::new(0),
                    AtomicU64::new(0),
                    AtomicU64::new(0),
                ],
            });
        }
        Self { slots }
    }

    #[inline(always)]
    fn insert(&self, h1: u64, h2: u64, country: usize) {
        let table_mask = self.slots.len() - 1;
        let word_idx = country >> 6;
        let bit = 1u64 << (country & 63);
        let mut idx = (h1 as usize) & table_mask;
        loop {
            // SAFETY: idx is masked by power-of-two slots.len() - 1.
            let slot = unsafe { self.slots.get_unchecked(idx) };
            let mut cur_h1 = slot.h1.load(Ordering::Acquire);
            if cur_h1 == 0 {
                match slot
                    .h1
                    .compare_exchange(0, h1, Ordering::AcqRel, Ordering::Acquire)
                {
                    Ok(_) => {
                        slot.h2.store(h2, Ordering::Release);
                        slot.mask[word_idx].fetch_or(bit, Ordering::Relaxed);
                        return;
                    }
                    Err(actual) => cur_h1 = actual,
                }
            }
            if cur_h1 == h1 {
                let mut cur_h2 = slot.h2.load(Ordering::Acquire);
                while cur_h2 == 0 {
                    std::hint::spin_loop();
                    cur_h2 = slot.h2.load(Ordering::Acquire);
                }
                if cur_h2 == h2 {
                    let cur_m = slot.mask[word_idx].load(Ordering::Relaxed);
                    if (cur_m & bit) == 0 {
                        slot.mask[word_idx].fetch_or(bit, Ordering::Relaxed);
                    }
                    return;
                }
            }
            idx = (idx + 1) & table_mask;
        }
    }
}

#[inline(always)]
fn mix64(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

#[inline(always)]
fn hash_name_128(bytes: &[u8]) -> (u64, u64) {
    let len = bytes.len();
    let mut h1 = 0x9e37_79b9_7f4a_7c15u64 ^ (len as u64);
    let mut h2 = 0x517c_c1b7_2722_0a95u64 ^ ((len as u64) << 32);
    if len <= 16 {
        let mut buf = [0u8; 16];
        buf[..len].copy_from_slice(bytes);
        let w0 = u64::from_le_bytes(buf[0..8].try_into().unwrap());
        let w1 = u64::from_le_bytes(buf[8..16].try_into().unwrap());
        h1 = mix64(h1 ^ w0);
        h2 = mix64(h2 ^ w1 ^ h1.rotate_left(19));
    } else {
        for chunk in bytes.chunks(16) {
            let mut buf = [0u8; 16];
            buf[..chunk.len()].copy_from_slice(chunk);
            let w0 = u64::from_le_bytes(buf[0..8].try_into().unwrap());
            let w1 = u64::from_le_bytes(buf[8..16].try_into().unwrap());
            h1 = mix64(h1 ^ w0);
            h2 = mix64(h2 ^ w1);
        }
    }
    (h1 | 1, h2 | 1)
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
    let neg = bytes[0] == b'-';
    let start = neg as usize;
    let n = bytes.len();
    let frac = ((bytes[n - 2] - b'0') as i64) * 10 + (bytes[n - 1] - b'0') as i64;
    let mut whole = 0i64;
    for &b in &bytes[start..n - 3] {
        whole = whole * 10 + (b - b'0') as i64;
    }
    let cents = whole * 100 + frac;
    if neg {
        -cents
    } else {
        cents
    }
}

fn load_regions(path: &str) -> io::Result<(Vec<Vec<u8>>, Vec<u8>, u8)> {
    let raw = fs::read(path)?;
    let mut rows: Vec<(usize, Vec<u8>)> = Vec::with_capacity(200_000);
    let mut country_list: Vec<Vec<u8>> = Vec::with_capacity(256);
    country_list.push(b"_UNMATCHED".to_vec());
    let mut max_rid = 999_999usize;

    for (line_idx, mut line) in raw.split(|&b| b == b'\n').enumerate() {
        if line.ends_with(b"\r") {
            line = &line[..line.len() - 1];
        }
        if line_idx == 0 {
            if let Some(stripped) = line.strip_prefix(b"\xEF\xBB\xBF") {
                line = stripped;
            }
            continue;
        }
        if line.is_empty() {
            continue;
        }
        if let Some(comma) = line.iter().position(|&b| b == b',') {
            let rid = parse_u64(&line[..comma]) as usize;
            let country = line[comma + 1..].to_vec();
            if rid > max_rid {
                max_rid = rid;
            }
            country_list.push(country.clone());
            rows.push((rid, country));
        }
    }
    country_list.sort_unstable();
    country_list.dedup();

    let unmatched_idx = country_list
        .binary_search_by(|c| c.as_slice().cmp(b"_UNMATCHED"))
        .unwrap() as u8;
    let mut region_map = vec![unmatched_idx; max_rid + 1];
    for (rid, country) in rows {
        let c_idx = country_list.binary_search(&country).unwrap() as u8;
        region_map[rid] = c_idx;
    }
    Ok((country_list, region_map, unmatched_idx))
}

#[inline(always)]
unsafe fn block_masks_64(ptr: *const u8) -> (u64, u64, u64) {
    // SAFETY: caller guarantees at least 64 readable bytes at ptr.
    unsafe {
        let v0 = _mm256_loadu_si256(ptr.cast::<__m256i>());
        let v1 = _mm256_loadu_si256(ptr.add(32).cast::<__m256i>());
        let qv = _mm256_set1_epi8(b'"' as i8);
        let cv = _mm256_set1_epi8(b',' as i8);
        let nv = _mm256_set1_epi8(b'\n' as i8);

        let q0 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(v0, qv)) as u32 as u64;
        let q1 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(v1, qv)) as u32 as u64;
        let c0 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(v0, cv)) as u32 as u64;
        let c1 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(v1, cv)) as u32 as u64;
        let n0 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(v0, nv)) as u32 as u64;
        let n1 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(v1, nv)) as u32 as u64;

        (q0 | (q1 << 32), c0 | (c1 << 32), n0 | (n1 << 32))
    }
}

fn count_quotes_simd(bytes: &[u8]) -> u64 {
    let mut count = 0u64;
    let mut offset = 0usize;
    let qv = unsafe { _mm256_set1_epi8(b'"' as i8) };
    while offset + 64 <= bytes.len() {
        // SAFETY: offset + 64 <= bytes.len() checked by loop condition.
        unsafe {
            let ptr = bytes.as_ptr().add(offset);
            let v0 = _mm256_loadu_si256(ptr.cast::<__m256i>());
            let v1 = _mm256_loadu_si256(ptr.add(32).cast::<__m256i>());
            let m0 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(v0, qv)) as u32;
            let m1 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(v1, qv)) as u32;
            count += (m0.count_ones() + m1.count_ones()) as u64;
        }
        offset += 64;
    }
    for &b in &bytes[offset..] {
        if b == b'"' {
            count += 1;
        }
    }
    count
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn process_record(
    bytes: &[u8],
    mut rec_start: usize,
    c1: usize,
    c2: usize,
    c3: usize,
    c4: usize,
    nl: usize,
    q1: u32,
    q2: u32,
    q3: u32,
    region_map: &[u8],
    unmatched_idx: u8,
    aggs: &mut [CountryAgg],
    name_table: &SharedNameTable,
    unescape_buf: &mut Vec<u8>,
) {
    if rec_start + 3 <= c1
        && bytes[rec_start] == 0xEF
        && bytes[rec_start + 1] == 0xBB
        && bytes[rec_start + 2] == 0xBF
    {
        rec_start += 3;
    }
    let id = parse_u64(&bytes[rec_start..c1]);
    let rid = parse_u64(&bytes[c3 + 1..c4]) as usize;
    let country = region_map.get(rid).copied().unwrap_or(unmatched_idx) as usize;

    let amount_end = if nl > c4 + 1 && bytes[nl - 1] == b'\r' {
        nl - 1
    } else {
        nl
    };
    let cents = parse_cents(&bytes[c4 + 1..amount_end]);

    let q_note = (q3 - q2) as usize;
    let note_raw = c3 - (c2 + 1);
    let note_len = if q_note > 0 {
        note_raw - 1 - (q_note >> 1)
    } else {
        note_raw
    };

    let q_name = (q2 - q1) as usize;
    let name_start = c1 + 1;
    let name_raw = c2 - name_start;
    let (name_len, (h1, h2)) = if q_name == 0 {
        let slice = &bytes[name_start..c2];
        (name_raw, hash_name_128(slice))
    } else if q_name == 2 {
        let slice = &bytes[name_start + 1..c2 - 1];
        (name_raw - 2, hash_name_128(slice))
    } else {
        let inner = &bytes[name_start + 1..c2 - 1];
        unescape_buf.clear();
        let mut i = 0usize;
        while i < inner.len() {
            let b = inner[i];
            unescape_buf.push(b);
            i += 1 + usize::from(b == b'"');
        }
        (unescape_buf.len(), hash_name_128(unescape_buf))
    };

    name_table.insert(h1, h2, country);

    // SAFETY: country < aggs.len() guaranteed by region_map construction.
    let agg = unsafe { aggs.get_unchecked_mut(country) };
    agg.count += 1;
    agg.id_sum += id;
    agg.amount_sum += cents;
    if cents < agg.amount_min {
        agg.amount_min = cents;
    }
    if cents > agg.amount_max {
        agg.amount_max = cents;
    }
    agg.name_bytes += name_len as u64;
    agg.note_bytes += note_len as u64;
}

fn parse_chunk(
    bytes: &[u8],
    region_map: &[u8],
    unmatched_idx: u8,
    num_countries: usize,
    name_table: &SharedNameTable,
) -> Vec<CountryAgg> {
    let mut aggs = vec![CountryAgg::default(); num_countries];
    let mut unescape_buf = Vec::with_capacity(256);
    let mut prev_in_quote = 0u64;
    let mut q_prefix = 0u32;
    let mut rec_start = 0usize;
    let mut field_idx = 0u8;
    let (mut c1, mut c2, mut c3, mut c4) = (0usize, 0usize, 0usize, 0usize);
    let (mut q1, mut q2, mut q3) = (0u32, 0u32, 0u32);

    let mut blk_start = 0usize;
    while blk_start < bytes.len() {
        let rem = bytes.len() - blk_start;
        let (q_mask, c_mask, n_mask) = if rem >= 64 {
            // SAFETY: rem >= 64 guarantees 64 readable bytes at blk_start.
            unsafe { block_masks_64(bytes.as_ptr().add(blk_start)) }
        } else {
            let mut tail = [0u8; 64];
            tail[..rem].copy_from_slice(&bytes[blk_start..]);
            let valid = (1u64 << rem) - 1;
            // SAFETY: tail is a 64-byte stack array.
            let (q, c, n) = unsafe { block_masks_64(tail.as_ptr()) };
            (q & valid, c & valid, n & valid)
        };

        // SAFETY: PCLMULQDQ is baseline on x86-64-v3.
        let in_quote = unsafe {
            let clmul =
                _mm_clmulepi64_si128(_mm_set_epi64x(0, q_mask as i64), _mm_set1_epi8(-1), 0);
            (_mm_cvtsi128_si64(clmul) as u64) ^ prev_in_quote
        };
        prev_in_quote = ((in_quote as i64) >> 63) as u64;

        let mut sep = (c_mask | n_mask) & !in_quote;
        while sep != 0 {
            let bit = sep.trailing_zeros() as usize;
            sep &= sep - 1;
            let pos = blk_start + bit;
            let q_here = q_prefix + (q_mask & ((1u64 << bit) - 1)).count_ones();
            match field_idx {
                0 => {
                    c1 = pos;
                    q1 = q_here;
                    field_idx = 1;
                }
                1 => {
                    c2 = pos;
                    q2 = q_here;
                    field_idx = 2;
                }
                2 => {
                    c3 = pos;
                    q3 = q_here;
                    field_idx = 3;
                }
                3 => {
                    c4 = pos;
                    field_idx = 4;
                }
                _ => {
                    process_record(
                        bytes,
                        rec_start,
                        c1,
                        c2,
                        c3,
                        c4,
                        pos,
                        q1,
                        q2,
                        q3,
                        region_map,
                        unmatched_idx,
                        &mut aggs,
                        name_table,
                        &mut unescape_buf,
                    );
                    rec_start = pos + 1;
                    field_idx = 0;
                }
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
    out.push(b'.');
    out.push(b'0' + rem / 10);
    out.push(b'0' + rem % 10);
}

fn run(data_dir: &str, out_dir: &str) -> io::Result<()> {
    let (countries, region_map, unmatched_idx) =
        load_regions(&format!("{}/regions.csv", data_dir))?;
    let num_countries = countries.len();

    let rec_file = File::open(format!("{}/records.csv", data_dir))?;
    let file_len = usize::try_from(rec_file.metadata()?.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "records.csv too large"))?;

    let mmap_slice: &[u8] = if file_len == 0 {
        &[]
    } else {
        // SAFETY: file_len > 0 and rec_file is open for reading.
        unsafe {
            let ptr = mmap(
                std::ptr::null_mut(),
                file_len,
                PROT_READ,
                MAP_PRIVATE,
                rec_file.as_raw_fd(),
                0,
            );
            if ptr as isize == -1 {
                return Err(io::Error::last_os_error());
            }
            std::slice::from_raw_parts(ptr.cast::<u8>(), file_len)
        }
    };

    // Skip the header line.
    let header_end = mmap_slice
        .iter()
        .position(|&b| b == b'\n')
        .map(|i| i + 1)
        .unwrap_or(mmap_slice.len());
    let body = &mmap_slice[header_end..];

    if body.is_empty() {
        fs::create_dir_all(out_dir)?;
        let mut out_file = File::create(format!("{}/aggregates.txt", out_dir))?;
        out_file.flush()?;
        return Ok(());
    }

    let threads = if body.len() < 64 * 1024 {
        1
    } else {
        thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .min(MAX_THREADS)
            .max(1)
    };

    let raw_bounds: Vec<usize> = (0..=threads).map(|i| body.len() * i / threads).collect();
    let quote_counts: Vec<u64> = thread::scope(|s| {
        let handles: Vec<_> = raw_bounds
            .windows(2)
            .map(|w| s.spawn(move || count_quotes_simd(&body[w[0]..w[1]])))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut chunk_bounds = vec![0usize; threads + 1];
    let mut prefix_quotes = 0u64;
    for i in 1..threads {
        prefix_quotes += quote_counts[i - 1];
        let mut in_quote = (prefix_quotes & 1) != 0;
        let mut pos = raw_bounds[i];
        while pos < body.len() {
            let b = body[pos];
            if b == b'"' {
                in_quote = !in_quote;
            } else if b == b'\n' && !in_quote {
                pos += 1;
                break;
            }
            pos += 1;
        }
        chunk_bounds[i] = pos;
    }
    chunk_bounds[threads] = body.len();

    let name_table = SharedNameTable::new(NAME_TABLE_SLOTS);
    let thread_aggs: Vec<Vec<CountryAgg>> = thread::scope(|s| {
        let handles: Vec<_> = chunk_bounds
            .windows(2)
            .filter(|w| w[0] < w[1])
            .map(|w| {
                let slice = &body[w[0]..w[1]];
                let rmap = &region_map;
                let ntable = &name_table;
                s.spawn(move || parse_chunk(slice, rmap, unmatched_idx, num_countries, ntable))
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut final_aggs = vec![CountryAgg::default(); num_countries];
    for t_agg in thread_aggs {
        for (c, src) in t_agg.into_iter().enumerate() {
            if src.count == 0 {
                continue;
            }
            let dst = &mut final_aggs[c];
            dst.count += src.count;
            dst.id_sum += src.id_sum;
            dst.amount_sum += src.amount_sum;
            if src.amount_min < dst.amount_min {
                dst.amount_min = src.amount_min;
            }
            if src.amount_max > dst.amount_max {
                dst.amount_max = src.amount_max;
            }
            dst.name_bytes += src.name_bytes;
            dst.note_bytes += src.note_bytes;
        }
    }

    let mut distinct_counts = vec![0u64; num_countries];
    for slot in &name_table.slots {
        if slot.h1.load(Ordering::Relaxed) != 0 {
            for (w_idx, word_atom) in slot.mask.iter().enumerate() {
                let mut w = word_atom.load(Ordering::Relaxed);
                while w != 0 {
                    let bit = w.trailing_zeros() as usize;
                    let c = w_idx * 64 + bit;
                    if c < num_countries {
                        distinct_counts[c] += 1;
                    }
                    w &= w - 1;
                }
            }
        }
    }

    let mut out = Vec::with_capacity(32 * 1024);
    for (c, agg) in final_aggs.iter().enumerate() {
        if agg.count == 0 {
            continue;
        }
        out.extend_from_slice(&countries[c]);
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
    drop(file);
    std::process::exit(0);
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut data_dir = "/data";
    let mut out_dir = "/out";
    let mut i = 1usize;
    while i < args.len() {
        if args[i] == "--data" && i + 1 < args.len() {
            data_dir = &args[i + 1];
            i += 2;
        } else if args[i] == "--out" && i + 1 < args.len() {
            out_dir = &args[i + 1];
            i += 2;
        } else if i == 1 {
            data_dir = &args[1];
            i += 1;
        } else if i == 2 {
            out_dir = &args[2];
            i += 1;
        } else {
            i += 1;
        }
    }
    run(data_dir, out_dir)
}
