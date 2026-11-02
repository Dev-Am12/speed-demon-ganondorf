// Track C (Join): sequential AVX2 record parser, batched prefetch pipeline, shared name/country mask table.
use std::arch::x86_64::*;
use std::ffi::c_void;
use std::fs::{self, File};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::thread;

const THREADS: usize = 12;
const NAME_SLOTS: usize = 1 << 17;
const RMAP: usize = 1_000_000;
const BATCH: usize = 64;
const INIT_AGG: [i64; 9] = [0, 0, 0, i64::MAX, i64::MIN, 0, 0, 0, 0];

unsafe extern "C" {
    fn mmap(a: *mut c_void, l: usize, p: i32, f: i32, fd: i32, o: i64) -> *mut c_void;
}

// Maps a file read-only. Memory after the end stays readable (zero pages), so wide loads may overrun.
fn mmap_file(path: &str) -> &'static [u8] {
    let f = File::open(path).unwrap();
    let len = f.metadata().unwrap().len() as usize;
    unsafe {
        let p = mmap(std::ptr::null_mut(), len + (1 << 16), 1, 0x22, -1, 0);
        if len > 0 {
            mmap(p, len, 1, 0x12, f.as_raw_fd(), 0);
        }
        std::slice::from_raw_parts(p.cast::<u8>(), len)
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
        return hash_long(p, len);
    }
    let r8 = |q: *const u8| q.cast::<u64>().read_unaligned();
    let sbits = (8 - len.min(8)) << 3;
    let mut a = r8(p) & (!0u64 >> sbits);
    let mut b = r8(p.add(len).sub(8)) & (!0u64 << sbits);
    if len > 16 {
        a ^= wymix(r8(p.add(8)), len as u64);
        if len > 24 {
            b ^= wymix(r8(p.add(16)), len as u64);
        }
    }
    let c = b ^ ((len as u64) << 32) ^ 0x517c_c1b7_2722_0a95;
    wymix(a ^ 0x9e37_79b9_7f4a_7c15, c) | 1
}

#[cold]
unsafe fn hash_long(p: *const u8, len: usize) -> u64 {
    let mut h = (len as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ 0x517c_c1b7_2722_0a95;
    let mut i = 0usize;
    while i + 8 <= len {
        h = wymix(h ^ p.add(i).cast::<u64>().read_unaligned(), 0xa076_1d64_78bd_642f);
        i += 8;
    }
    let tail = p.add(len - 8).cast::<u64>().read_unaligned();
    wymix(h ^ tail, 0xe703_7ed1_a0b4_28db) | 1
}

#[inline(always)]
fn find_slot(keys: &[AtomicU64], h: u64) -> usize {
    let mut idx = (h as usize) & (NAME_SLOTS - 1);
    loop {
        let k = unsafe { keys.get_unchecked(idx) };
        let mut cur = k.load(Relaxed);
        if cur == 0 {
            cur = k.compare_exchange(0, h, Relaxed, Relaxed).map_or_else(|v| v, |_| h);
        }
        if cur == h {
            return idx;
        }
        idx = (idx + 1) & (NAME_SLOTS - 1);
    }
}

// Sets the (name, country) bit; returns 1 for exactly one caller per pair.
#[inline(always)]
fn mark(masks: &[AtomicU64], idx: usize, c: usize) -> i64 {
    let bit = 1u64 << (c & 63);
    let m = unsafe { masks.get_unchecked((idx << 2) + (c >> 6)) };
    i64::from((m.load(Relaxed) & bit) == 0 && (m.fetch_or(bit, Relaxed) & bit) == 0)
}

// Number of leading ASCII digits in the next 8 bytes (8 when all are digits).
#[inline(always)]
unsafe fn ndig(p: *const u8) -> usize {
    let x = p.cast::<u64>().read_unaligned() ^ 0x3030_3030_3030_3030;
    (((x.wrapping_add(0x7676_7676_7676_7676)) & 0x8080_8080_8080_8080).trailing_zeros() >> 3) as usize
}

// Value of the `len` (1..=8) digits that end just before `end`.
#[inline(always)]
unsafe fn num(end: *const u8, len: usize) -> u64 {
    if len.wrapping_sub(1) >= 8 {
        return 0;
    }
    let sh = ((8 - len) << 3) as u32;
    let w = (end.sub(8).cast::<u64>().read_unaligned() & 0x0F0F_0F0F_0F0F_0F0F) >> sh << sh;
    let w = (w.wrapping_mul(2561) >> 8) & 0x00FF_00FF_00FF_00FF;
    let w = (w.wrapping_mul(6_553_601) >> 16) & 0x0000_FFFF_0000_FFFF;
    w.wrapping_mul(42_949_672_960_001) >> 32
}

// Bit mask of the double quotes in the 64 bytes at `s`.
#[inline(always)]
unsafe fn qmask(s: *const u8) -> u64 {
    let q = _mm256_set1_epi8(b'"' as i8);
    let m = |o: usize| _mm256_movemask_epi8(_mm256_cmpeq_epi8(_mm256_loadu_si256(s.add(o).cast()), q)) as u32 as u64;
    m(0) | (m(32) << 32)
}

// One CSV field at `q`: (first byte, raw length, unescaped length, start of the next field).
#[inline(always)]
unsafe fn field(q: *const u8) -> (*const u8, usize, usize, *const u8) {
    if *q != b'"' {
        let mut e = q;
        while *e != b',' {
            e = e.add(1);
        }
        let n = e.offset_from(q) as usize;
        return (q, n, n, e.add(1));
    }
    let (mut s, mut esc) = (q.add(1), 0);
    loop {
        let m = qmask(s);
        if m == 0 {
            s = s.add(64);
            continue;
        }
        let e = s.add(m.trailing_zeros() as usize);
        if *e.add(1) == b'"' {
            esc += 1;
            s = e.add(2);
            continue;
        }
        let raw = e.offset_from(q.add(1)) as usize;
        return (q.add(1), raw, raw - esc, e.add(2));
    }
}

fn load_regions(path: &str) -> (Vec<&'static [u8]>, Vec<u8>, Vec<(u32, u8)>) {
    let raw = mmap_file(path);
    let mut countries: Vec<&'static [u8]> = vec![b"_UNMATCHED"];
    let (mut c_ids, mut rmap) = ([0u8; 32768], vec![0u8; RMAP]);
    let mut far: Vec<(u32, u8)> = Vec::new(); // region ids at or above RMAP (valid up to 9,999,999)
    let mut i = raw.iter().position(|&b| b == b'\n').map_or(raw.len(), |p| p + 1);
    while i < raw.len() {
        let l = unsafe { ndig(raw.as_ptr().add(i)) };
        let rid = unsafe { num(raw.as_ptr().add(i + l), l) } as usize;
        let cs = i + l + 1;
        if cs + 3 > raw.len() {
            break;
        }
        let g = |k: usize| ((raw[cs + k] & 31) as usize) << (10 - 5 * k);
        let idx = g(0) | g(1) | g(2);
        if c_ids[idx] == 0 {
            c_ids[idx] = countries.len() as u8;
            countries.push(&raw[cs..cs + 3]);
        }
        if rid < RMAP {
            rmap[rid] = c_ids[idx];
        } else {
            far.push((rid as u32, c_ids[idx]));
        }
        i = cs + 3;
        while i < raw.len() && raw[i] < b'0' {
            i += 1;
        }
    }
    far.sort_by_key(|&(r, _)| r); // stable: a repeated id keeps its last line
    (countries, rmap, far)
}

// Country index of a region id at or above RMAP; 0 (_UNMATCHED) when the id is not listed.
#[inline(always)]
fn far_country(far: &[(u32, u8)], rid: u32) -> u8 {
    if far.is_empty() {
        return 0;
    }
    let i = far.partition_point(|&(r, _)| r <= rid);
    if i > 0 && far[i - 1].0 == rid {
        far[i - 1].1
    } else {
        0
    }
}

// True when the line feed at `nl` ends a record: the line ends in `,region_id,amount` and a digit follows.
fn is_record_end(b: &[u8], nl: usize) -> bool {
    let d = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    let e = nl - usize::from(nl > 0 && b[nl - 1] == b'\r');
    if e < 6 || b[e - 3] != b'.' || !d(e - 1) || !d(e - 2) || !d(e - 4) {
        return false;
    }
    let mut p = e - 4;
    while p > 0 && b[p - 1].is_ascii_digit() {
        p -= 1;
    }
    p -= usize::from(p > 0 && b[p - 1] == b'-');
    let nxt = nl + 1 + 3 * usize::from(b.get(nl + 1..nl + 4) == Some(b"\xEF\xBB\xBF"));
    p >= 2 && b[p - 1] == b',' && d(p - 2) && d(nxt)
}

// Parses the records in [p, stop) in batches: parse, then country lookup and aggregation, then distinct marking.
#[target_feature(enable = "avx2,bmi1,bmi2,lzcnt,popcnt")]
unsafe fn parse_chunk(
    mut p: *const u8,
    stop: *const u8,
    rmap: &[u8],
    far: &[(u32, u8)],
    km: (&[AtomicU64], &[AtomicU64]),
    aggs: &mut [[i64; 9]],
) {
    let (keys, masks) = km;
    // Per record: name hash (0 = empty name), id, region id, cents, name bytes, note bytes.
    let mut recs = [[0u64; 6]; BATCH];
    while p < stop {
        let mut n = 0;
        while n < BATCH && p < stop {
            p = p.add(3 * usize::from(*p == 0xEF));
            let il = ndig(p);
            let id = num(p.add(il), il);
            let (np, nraw, nlen, q) = field(p.add(il + 1));
            let (_, _, tlen, q) = field(q);
            let rl = ndig(q);
            let rid = num(q.add(rl), rl);
            let neg = usize::from(*q.add(rl + 1) == b'-');
            let a = q.add(rl + 1 + neg);
            let wl = ndig(a);
            let whole = num(a.add(wl), wl);
            let frac = u64::from(*a.add(wl + 1) - b'0') * 10 + u64::from(*a.add(wl + 2) - b'0');
            let cents = (((whole * 100 + frac) as i64) ^ -(neg as i64)) + neg as i64;
            p = a.add(wl + 3);
            p = p.add(1 + usize::from(*p == b'\r'));
            let h = if nlen == 0 { 0 } else { hash_name_fast(np, nraw) };
            _mm_prefetch(rmap.as_ptr().wrapping_add(rid as usize).cast::<i8>(), _MM_HINT_T0);
            _mm_prefetch(keys.as_ptr().add((h as usize) & (NAME_SLOTS - 1)).cast::<i8>(), _MM_HINT_T0);
            *recs.get_unchecked_mut(n) = [h, id, rid, cents as u64, nlen as u64, tlen as u64];
            n += 1;
        }
        for r in recs.iter_mut().take(n) {
            let c = if (r[2] as usize) < RMAP {
                *rmap.get_unchecked(r[2] as usize) as usize
            } else {
                far_country(far, r[2] as u32) as usize
            };
            let a = aggs.get_unchecked_mut(c);
            let cents = r[3] as i64;
            a[7] |= i64::from(r[0] == 0);
            a[0] += 1;
            a[1] += r[1] as i64;
            a[2] += cents;
            a[3] = a[3].min(cents);
            a[4] = a[4].max(cents);
            a[5] += r[4] as i64;
            a[6] += r[5] as i64;
            let s = if r[0] == 0 { 0 } else { find_slot(keys, r[0]) };
            let mp = masks.as_ptr().add((s << 2) + (c >> 6));
            core::arch::asm!("prefetchw [{0}]", in(reg) mp, options(nostack, preserves_flags));
            r[2] = c as u64;
            r[3] = s as u64;
        }
        for r in recs.iter().take(n).filter(|r| r[0] != 0) {
            aggs.get_unchecked_mut(r[2] as usize)[8] += mark(masks, r[3] as usize, r[2] as usize);
        }
    }
}

fn fmt_cents(c: i64) -> String {
    let a = c.unsigned_abs();
    format!("{}{}.{:02}", if c < 0 { "-" } else { "" }, a / 100, a % 100)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).filter(|a| !a.starts_with("--")).collect();
    let data_dir = args.first().map_or("/data", String::as_str);
    let out_dir = args.get(1).map_or("/out", String::as_str);
    let (countries, rmap, far) = load_regions(&format!("{data_dir}/regions.csv"));
    let nc = countries.len();
    let raw = mmap_file(&format!("{data_dir}/records.csv"));
    let body = &raw[raw.iter().position(|&b| b == b'\n').map_or(raw.len(), |p| p + 1)..];
    let n_chunks = (body.len() >> 19).max(1);
    let threads = if n_chunks == 1 { 1 } else { THREADS };
    // Start of chunk c, moved forward to a record start.
    let cut = |c: usize| {
        if c >= n_chunks {
            return body.len();
        }
        let mut pos = body.len() / n_chunks * c;
        while c > 0 && pos < body.len() && !(body[pos] == b'\n' && is_record_end(body, pos)) {
            pos += 1;
        }
        if c == 0 { 0 } else { (pos + 1).min(body.len()) }
    };
    let (raw_keys, raw_masks) = (vec![0u64; NAME_SLOTS], vec![0u64; NAME_SLOTS * 4]);
    let km = unsafe {
        (
            std::slice::from_raw_parts(raw_keys.as_ptr().cast::<AtomicU64>(), NAME_SLOTS),
            std::slice::from_raw_parts(raw_masks.as_ptr().cast::<AtomicU64>(), NAME_SLOTS * 4),
        )
    };
    let next_chunk = AtomicUsize::new(0);
    let thread_aggs: Vec<Vec<[i64; 9]>> = thread::scope(|s| {
        let hs: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let mut aggs = vec![INIT_AGG; nc];
                    loop {
                        let c = next_chunk.fetch_add(1, Relaxed);
                        if c >= n_chunks {
                            return aggs;
                        }
                        let (a, b) = (cut(c), cut(c + 1));
                        if a < b {
                            unsafe { parse_chunk(body.as_ptr().add(a), body.as_ptr().add(b), &rmap, &far, km, &mut aggs) };
                        }
                    }
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut final_aggs = vec![INIT_AGG; nc];
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
    let mut order: Vec<usize> = (1..nc).collect();
    order.sort_unstable_by(|&a, &b| countries[a].cmp(countries[b]));
    order.push(0);
    let mut out = Vec::with_capacity(32 * 1024);
    for c in order {
        let a = &final_aggs[c];
        if a[0] > 0 {
            let cn = std::str::from_utf8(countries[c]).unwrap();
            let (s, mn, mx) = (fmt_cents(a[2]), fmt_cents(a[3]), fmt_cents(a[4]));
            let _ = writeln!(out, "{cn} {} {} {s} {mn} {mx} {} {} {}", a[0], a[1], a[7] + a[8], a[5], a[6]);
        }
    }
    fs::create_dir_all(out_dir).unwrap();
    let mut file = File::create(format!("{out_dir}/aggregates.txt")).unwrap();
    file.write_all(&out).unwrap();
    file.flush().unwrap();
    std::process::exit(0);
}