// Track C (Join): AVX2 + VPCLMULQDQ batched CSV join with prefetchw ring and dynamic chunks.
use std::arch::x86_64::*;
use std::ffi::c_void;
use std::fs::{self, File};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::thread;

const MAX_THREADS: usize = 12;
const NAME_SLOTS: usize = 1 << 17;
const PF: usize = 32;
const INIT_AGG: [i64; 9] = [0, 0, 0, i64::MAX, i64::MIN, 0, 0, 0, 0];

unsafe extern "C" {
    fn mmap(a: *mut c_void, l: usize, p: i32, f: i32, fd: i32, o: i64) -> *mut c_void;
}

fn mmap_file(path: &str) -> &'static [u8] {
    let f = File::open(path).unwrap();
    let len = f.metadata().unwrap().len() as usize;
    if len == 0 {
        return &[];
    }
    let p = unsafe { mmap(std::ptr::null_mut(), len, 1, 2, f.as_raw_fd(), 0) };
    unsafe { std::slice::from_raw_parts(p.cast::<u8>(), len) }
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
        let w = p.add(i).cast::<u64>().read_unaligned();
        h = wymix(h ^ w, 0xa076_1d64_78bd_642f);
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
            cur = k
                .compare_exchange(0, h, Relaxed, Relaxed)
                .map_or_else(|v| v, |_| h);
        }
        if cur == h {
            return idx;
        }
        idx = (idx + 1) & (NAME_SLOTS - 1);
    }
}

#[inline(always)]
fn mark(masks: &[AtomicU64], idx: usize, c: usize) -> i64 {
    let bit = 1u64 << (c & 63);
    let m = unsafe { masks.get_unchecked((idx << 2) + (c >> 6)) };
    i64::from((m.load(Relaxed) & bit) == 0 && (m.fetch_or(bit, Relaxed) & bit) == 0)
}

#[inline(always)]
unsafe fn p_int(mut p: *const u8, end: *const u8, swar_ok: bool) -> i64 {
    let len = end.offset_from_unsigned(p);
    if swar_ok && len.wrapping_sub(1) < 8 {
        let sh = ((8 - len) << 3) as u32;
        let w = (end.sub(8).cast::<u64>().read_unaligned() & 0x0F0F_0F0F_0F0F_0F0F) >> sh << sh;
        let w = (w.wrapping_mul(2561) >> 8) & 0x00FF_00FF_00FF_00FF;
        let w = (w.wrapping_mul(6_553_601) >> 16) & 0x0000_FFFF_0000_FFFF;
        return (w.wrapping_mul(42_949_672_960_001) >> 32) as i64;
    }
    let mut v = 0i64;
    while p < end {
        v = v * 10 + (*p - b'0') as i64;
        p = p.add(1);
    }
    v
}

fn load_regions(path: &str) -> (Vec<&'static [u8]>, Vec<u8>, Vec<(usize, u8)>) {
    let raw = mmap_file(path);
    let mut countries: Vec<&'static [u8]> = vec![b"_UNMATCHED"];
    let (mut c_ids, mut rmap, mut extra) = ([0u8; 32768], vec![0u8; 1_000_000], Vec::new());
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
        if i + 3 >= raw.len() {
            break;
        }
        let cs = i + 1;
        let g = |k: usize| ((raw[cs + k] & 31) as usize) << (10 - 5 * k);
        let idx = g(0) | g(1) | g(2);
        if c_ids[idx] == 0 {
            c_ids[idx] = countries.len() as u8;
            countries.push(&raw[cs..cs + 3]);
        }
        let cid = c_ids[idx];
        if rid < rmap.len() {
            rmap[rid] = cid;
        } else {
            extra.push((rid, cid));
        }
        i = cs + 4 + usize::from(cs + 3 < raw.len() && raw[cs + 3] == b'\r');
    }
    (countries, rmap, extra)
}

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

#[target_feature(enable = "avx2,bmi1,bmi2,lzcnt,popcnt,pclmulqdq")]
unsafe fn parse_chunk(
    bytes: &[u8],
    rmap: &[u8],
    extra: &[(usize, u8)],
    km: (&[AtomicU64], &[AtomicU64]),
    aggs: &mut [[i64; 9]],
) {
    let (keys, masks) = km;
    let mut ubuf = vec![0u8; 512];
    let (mut d_pos, mut d_q, mut tail) = ([0u32; 336], [0u32; 336], [0u8; 64]);
    let (mut ring, mut rn) = ([(0u64, 0usize, 0usize); PF], 0usize);
    let (mut n_delims, mut prev_iq, mut q_pref) = (0usize, 0u64, 0u32);
    let (mut rec_start, mut blk_start) = (0usize, 0usize);
    let (qv, cv) = (_mm256_set1_epi8(b'"' as i8), _mm256_set1_epi8(b',' as i8));
    let (nv, ones, bp) = (
        _mm256_set1_epi8(b'\n' as i8),
        _mm_set1_epi8(-1),
        bytes.as_ptr(),
    );
    let m64 = |v0: __m256i, v1: __m256i, c: __m256i| {
        let m = |v| _mm256_movemask_epi8(_mm256_cmpeq_epi8(v, c)) as u32 as u64;
        m(v0) | (m(v1) << 32)
    };
    while blk_start < bytes.len() {
        while blk_start < bytes.len() && n_delims <= 256 {
            let rem = bytes.len() - blk_start;
            let ptr = if rem >= 64 {
                bp.add(blk_start)
            } else {
                tail[..rem].copy_from_slice(&bytes[blk_start..]);
                tail[rem..].fill(0);
                tail.as_ptr()
            };
            let v0 = _mm256_loadu_si256(ptr.cast());
            let v1 = _mm256_loadu_si256(ptr.add(32).cast());
            let q_mask = m64(v0, v1, qv);
            let cn_mask = m64(v0, v1, cv) | m64(v0, v1, nv);
            let clmul = _mm_clmulepi64_si128(_mm_cvtsi64_si128(q_mask as i64), ones, 0);
            let iq = (_mm_cvtsi128_si64(clmul) as u64) ^ prev_iq;
            prev_iq = ((iq as i64) >> 63) as u64;
            let mut sep = cn_mask & !iq;
            let mut put = |i: usize, s: &mut u64| {
                let bit = s.trailing_zeros();
                *s &= s.wrapping_sub(1);
                *d_pos.get_unchecked_mut(i) = (blk_start + bit as usize) as u32;
                *d_q.get_unchecked_mut(i) = q_pref + _bzhi_u64(q_mask, bit).count_ones();
            };
            let c = sep.count_ones() as usize;
            for k in 0..8 {
                put(n_delims + k, &mut sep);
            }
            n_delims += c.min(8);
            while sep != 0 {
                put(n_delims, &mut sep);
                n_delims += 1;
            }
            q_pref += q_mask.count_ones();
            blk_start += 64;
        }
        if blk_start >= bytes.len() && n_delims % 5 == 4 {
            *d_pos.get_unchecked_mut(n_delims) = bytes.len() as u32;
            *d_q.get_unchecked_mut(n_delims) = q_pref;
            n_delims += 1;
        }
        let nr = n_delims / 5;
        for r in 0..nr {
            let idx = r * 5;
            let pos = |k: usize| *d_pos.get_unchecked(idx + k) as usize;
            let (c1, c2, c3, c4, nl) = (pos(0), pos(1), pos(2), pos(3), pos(4));
            let rs = rec_start + 3 * usize::from(*bp.add(rec_start) == 0xEF);
            let a_end = nl - usize::from(nl > 0 && *bp.add(nl - 1) == b'\r');
            let neg = usize::from(*bp.add(c4 + 1) == b'-');
            let id = p_int(bp.add(rs), bp.add(c1), c1 >= 8);
            let rid = p_int(bp.add(c3 + 1), bp.add(c4), c4 >= 8) as usize;
            let whole = p_int(bp.add(c4 + 1 + neg), bp.add(a_end - 3), a_end >= 11);
            let country = if rid < 1_000_000 {
                *rmap.get_unchecked(rid) as usize
            } else {
                extra
                    .iter()
                    .find(|x| x.0 == rid)
                    .map_or(0, |x| x.1 as usize)
            };
            let frac = (*bp.add(a_end - 2) - b'0') as i64 * 10 + (*bp.add(a_end - 1) - b'0') as i64;
            let (x, m) = (whole * 100 + frac, -(neg as i64));
            let cents = (x ^ m) - m;
            let q_note = (*d_q.get_unchecked(idx + 2) - *d_q.get_unchecked(idx + 1)) as usize;
            let note_len = ((c3 - c2 - 1) - (q_note >> 1) - usize::from(q_note > 0)) as i64;
            let q_name = (*d_q.get_unchecked(idx + 1) - *d_q.get_unchecked(idx)) as usize;
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
            if n_len != 0 {
                let h = hash_name_fast(n_ptr, n_len);
                let s = rn & (PF - 1);
                if rn >= PF {
                    let (_, c_old, i_old) = ring[s];
                    aggs.get_unchecked_mut(c_old)[8] += mark(masks, i_old, c_old);
                }
                if rn >= PF / 2 {
                    let mid = (rn - PF / 2) & (PF - 1);
                    let i_mid = find_slot(keys, ring[mid].0);
                    ring[mid].2 = i_mid;
                    let mp = masks.as_ptr().add((i_mid << 2) + (ring[mid].1 >> 6));
                    core::arch::asm!("prefetchw [{0}]", in(reg) mp, options(nostack, preserves_flags));
                }
                ring[s] = (h, country, 0);
                let kp = keys
                    .as_ptr()
                    .add((h as usize) & (NAME_SLOTS - 1))
                    .cast::<i8>();
                _mm_prefetch(kp, _MM_HINT_T0);
                rn += 1;
            }
            let agg = aggs.get_unchecked_mut(country);
            agg[7] |= i64::from(n_len == 0);
            agg[0] += 1;
            agg[1] += id;
            agg[2] += cents;
            agg[3] = agg[3].min(cents);
            agg[4] = agg[4].max(cents);
            agg[5] += n_len as i64;
            agg[6] += note_len;
            rec_start = nl + 1;
        }
        let rem_d = n_delims - nr * 5;
        for k in 0..rem_d {
            *d_pos.get_unchecked_mut(k) = *d_pos.get_unchecked(nr * 5 + k);
            *d_q.get_unchecked_mut(k) = *d_q.get_unchecked(nr * 5 + k);
        }
        n_delims = rem_d;
    }
    for r in rn.saturating_sub(PF)..rn {
        let (h, c, _) = ring[r & (PF - 1)];
        aggs[c][8] += mark(masks, find_slot(keys, h), c);
    }
}

fn fmt_cents(c: i64) -> String {
    let a = c.unsigned_abs();
    format!("{}{}.{:02}", if c < 0 { "-" } else { "" }, a / 100, a % 100)
}

fn main() {
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();
    let data_dir = args.first().map_or("/data", String::as_str);
    let out_dir = args.get(1).map_or("/out", String::as_str);
    let (countries, rmap, extra) = load_regions(&format!("{data_dir}/regions.csv"));
    let nc = countries.len();
    let raw = mmap_file(&format!("{data_dir}/records.csv"));
    let body = &raw[raw
        .iter()
        .position(|&b| b == b'\n')
        .map_or(raw.len(), |p| p + 1)..];
    let threads = if body.len() < 512 * 1024 {
        1
    } else {
        thread::available_parallelism()
            .map_or(1, |n| n.get())
            .clamp(1, MAX_THREADS)
    };
    let n_chunks = if threads == 1 { 1 } else { threads * 4 };
    let mut bounds = vec![0usize; n_chunks + 1];
    for c in 1..n_chunks {
        let mut pos = body.len() * c / n_chunks;
        while pos < body.len() && !(body[pos] == b'\n' && is_record_end(body, pos)) {
            pos += 1;
        }
        bounds[c] = (pos + 1).min(body.len());
    }
    bounds[n_chunks] = body.len();
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
                        if bounds[c] < bounds[c + 1] {
                            unsafe {
                                parse_chunk(
                                    &body[bounds[c]..bounds[c + 1]],
                                    &rmap,
                                    &extra,
                                    km,
                                    &mut aggs,
                                );
                            }
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
            let _ = writeln!(
                out,
                "{cn} {} {} {s} {mn} {mx} {} {} {}",
                a[0],
                a[1],
                a[7] + a[8],
                a[5],
                a[6]
            );
        }
    }
    fs::create_dir_all(out_dir).unwrap();
    let mut file = File::create(format!("{out_dir}/aggregates.txt")).unwrap();
    file.write_all(&out).unwrap();
    file.flush().unwrap();
    std::process::exit(0);
}
