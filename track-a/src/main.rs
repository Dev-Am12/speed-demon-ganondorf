use std::arch::x86_64::*;
use std::fs::File;
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::sync::atomic::{AtomicU16, AtomicU32, AtomicU64, AtomicUsize, Ordering::*};
use std::sync::{Barrier, Mutex};

const H_SLOTS: usize = 128;
const BLOCK_CAP: usize = 4096;

extern "C" {
    fn mmap(a: *mut (), l: usize, p: i32, f: i32, fd: i32, o: i64) -> *mut u8;
    fn munmap(a: *mut u8, l: usize) -> i32;
    fn madvise(a: *mut u8, l: usize, adv: i32) -> i32;
    fn malloc_trim(pad: usize) -> i32;
}

type HourBlock = (Vec<u32>, Vec<u32>);

static SLOTS: [AtomicU64; 8192] = [const { AtomicU64::new(0) }; 8192];
static META: [AtomicU64; 4096] = [const { AtomicU64::new(0) }; 4096];
static ARENA: [AtomicU64; 32768] = [const { AtomicU64::new(0) }; 32768];
static LOCK: Mutex<(usize, usize)> = Mutex::new((0, 0));
static ORDER: [AtomicU16; 4096] = [const { AtomicU16::new(0) }; 4096];
static N_EPS: AtomicUsize = AtomicUsize::new(0);
static ARENA_BYTES: AtomicUsize = AtomicUsize::new(0);
static HOURLY: [Mutex<Vec<HourBlock>>; H_SLOTS] = [const { Mutex::new(Vec::new()) }; H_SLOTS];
static HOUR_EPOCH: [AtomicUsize; H_SLOTS] = [const { AtomicUsize::new(0) }; H_SLOTS];
static HOUR_PREF: [[AtomicU64; 3]; H_SLOTS] = [const { [const { AtomicU64::new(0) }; 3] }; H_SLOTS];
static ACTIVE_H: [AtomicUsize; H_SLOTS] = [const { AtomicUsize::new(0) }; H_SLOTS];
static N_HOURS: AtomicUsize = AtomicUsize::new(0);
static SUMMARY: [AtomicU32; H_SLOTS * 4096 * 5] = [const { AtomicU32::new(0) }; H_SLOTS * 4096 * 5];
static NEXT_H: AtomicUsize = AtomicUsize::new(0);
static NEXT_FMT: AtomicUsize = AtomicUsize::new(0);
static WRITE_TURN: AtomicUsize = AtomicUsize::new(0);

// SAFETY: All unaligned loads are bounds-checked by branch conditions on `n = s.len()`.
#[inline(always)]
unsafe fn hash(s: &[u8]) -> u64 {
    const K1: u64 = 0x9E3779B97F4A7C15;
    const K2: u64 = 0xBF58476D1CE4E5B9;
    let (n, p) = (s.len(), s.as_ptr());
    let h = if n >= 8 {
        if n <= 16 {
            let a = (p as *const u64).read_unaligned();
            let b = (p.add(n - 8) as *const u64).read_unaligned();
            (a ^ b.rotate_left(23) ^ n as u64).wrapping_mul(K1)
        } else if n <= 32 {
            let a = (p as *const u64).read_unaligned();
            let b = (p.add(8) as *const u64).read_unaligned();
            let c = (p.add(n - 16) as *const u64).read_unaligned();
            let d = (p.add(n - 8) as *const u64).read_unaligned();
            (a ^ b.rotate_left(17) ^ n as u64).wrapping_mul(K1)
                ^ (c ^ d.rotate_left(29)).wrapping_mul(K2)
        } else {
            let mut h = n as u64;
            let mut i = 0;
            while i + 8 < n {
                h = (h.rotate_left(5) ^ (p.add(i) as *const u64).read_unaligned()).wrapping_mul(K1);
                i += 8;
            }
            (h.rotate_left(5) ^ (p.add(n - 8) as *const u64).read_unaligned()).wrapping_mul(K1)
        }
    } else if n >= 4 {
        let a = (p as *const u32).read_unaligned() as u64;
        let b = (p.add(n - 4) as *const u32).read_unaligned() as u64;
        (n as u64 ^ a ^ (b << 32)).wrapping_mul(K1)
    } else {
        let (a, b, c) = (s[0] as u64, s[n >> 1] as u64, s[n - 1] as u64);
        (n as u64 ^ a ^ (b << 8) ^ (c << 16)).wrapping_mul(K1)
    };
    let h = (h ^ (h >> 30)).wrapping_mul(K2);
    h ^ (h >> 29)
}

// SAFETY: `id < N_EPS`, and `META[id]` is published with Release ordering before `SLOTS` entry.
#[inline(always)]
unsafe fn get_name_sp(id: usize) -> &'static [u8] {
    let m = META[id].load(Relaxed);
    std::slice::from_raw_parts(
        (ARENA.as_ptr() as *const u8).add((m >> 16) as usize),
        m as u16 as usize,
    )
}

// SAFETY: `ARENA` writes are serialized by `LOCK` and bounds-checked by `assert!`.
#[inline(always)]
unsafe fn intern(name: &[u8]) -> u16 {
    let h = hash(name);
    let len_sp = name.len() + 1;
    let n_tag = (h & 0xFFFF_FFFF_0000_0000) | ((len_sp as u64) << 16);
    let mut i = (h as usize) & 8191;
    loop {
        let s = SLOTS[i].load(Acquire);
        if s == 0 {
            break;
        }
        if (s & 0xFFFF_FFFF_FFFF_0000) == n_tag
            && &get_name_sp((s & 0xFFFF) as usize - 1)[..name.len()] == name
        {
            return (s & 0xFFFF) as u16 - 1;
        }
        i = (i + 1) & 8191;
    }
    let mut st = LOCK.lock().unwrap();
    i = (h as usize) & 8191;
    loop {
        let s = SLOTS[i].load(Relaxed);
        if s == 0 {
            let (id, start) = *st;
            assert!(id < 4096 && start + len_sp <= ARENA.len() * 8);
            *st = (id + 1, start + len_sp);
            let dst = (ARENA.as_ptr() as *mut u8).add(start);
            std::ptr::copy_nonoverlapping(name.as_ptr(), dst, name.len());
            *dst.add(name.len()) = b' ';
            META[id].store(((start as u64) << 16) | len_sp as u64, Release);
            SLOTS[i].store(n_tag | (id as u64 + 1), Release);
            return id as u16;
        }
        if (s & 0xFFFF_FFFF_FFFF_0000) == n_tag
            && &get_name_sp((s & 0xFFFF) as usize - 1)[..name.len()] == name
        {
            return (s & 0xFFFF) as u16 - 1;
        }
        i = (i + 1) & 8191;
    }
}

// SAFETY: `p` points to a valid 27-byte RFC 3339 timestamp `YYYY-MM-DDTHH:MM:SS.ffffffZ`.
#[inline(always)]
unsafe fn parse_hour_slot(p: *const u8) -> usize {
    let y = (*p - b'0') as i32 * 1000
        + (*p.add(1) - b'0') as i32 * 100
        + (*p.add(2) - b'0') as i32 * 10
        + (*p.add(3) - b'0') as i32;
    let m = (*p.add(5) - b'0') as i32 * 10 + (*p.add(6) - b'0') as i32;
    let d = (*p.add(8) - b'0') as i32 * 10 + (*p.add(9) - b'0') as i32;
    let hh = (*p.add(11) - b'0') as i32 * 10 + (*p.add(12) - b'0') as i32;
    let y = y - (m <= 2) as i32;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let days = 365 * y + y / 4 - y / 100 + y / 400 + (153 * mp + 2) / 5 + d;
    let eh = (days * 24 + hh) as usize;
    let slot = eh & (H_SLOTS - 1);
    if HOUR_EPOCH[slot].load(Relaxed) == 0 {
        let mut pref = [0u8; 24];
        std::ptr::copy_nonoverlapping(p, pref.as_mut_ptr(), 13);
        pref[13..21].copy_from_slice(b":00:00Z ");
        for (i, c) in pref.chunks_exact(8).enumerate() {
            HOUR_PREF[slot][i].store(u64::from_ne_bytes(c.try_into().unwrap()), Relaxed);
        }
        HOUR_EPOCH[slot].store(eh, Release);
    }
    slot
}

// SAFETY: Called only when `pos + 160 <= end`, so at least 96 bytes from `ptr` (`p.add(63)`) are readable.
#[inline(always)]
unsafe fn find_space(ptr: *const u8) -> usize {
    let sp = _mm256_set1_epi8(32);
    let mut off = 0;
    loop {
        let v = _mm256_loadu_si256(ptr.add(off) as _);
        let m = _mm256_movemask_epi8(_mm256_cmpeq_epi8(v, sp)) as u32;
        if m != 0 {
            return off + m.trailing_zeros() as usize;
        }
        off += 32;
    }
}

// SAFETY: Fast SIMD path runs only while `pos + 160 <= end`; tail lines use bounds-checked indexing.
unsafe fn parse_worker(data: &[u8], mut pos: usize, end: usize) {
    let mut hours: [HourBlock; H_SLOTS] = [const { (Vec::new(), Vec::new()) }; H_SLOTS];
    while pos < end {
        let p = data.as_ptr().add(pos);
        let h = parse_hour_slot(p);
        let (s, e) = if pos + 160 <= end {
            let v = _mm256_loadu_si256(p.add(31) as _);
            let m = _mm256_movemask_epi8(_mm256_cmpeq_epi8(v, _mm256_set1_epi8(32))) as u32;
            let s = 32 + m.trailing_zeros() as usize;
            let m2 = m & (m - 1);
            let e = if m2 != 0 {
                31 + m2.trailing_zeros() as usize
            } else {
                63 + find_space(p.add(63))
            };
            (s, e)
        } else {
            let mut s = 31;
            while *p.add(s) != b' ' {
                s += 1;
            }
            s += 1;
            let mut e = s;
            while *p.add(e) != b' ' {
                e += 1;
            }
            (s, e)
        };
        let (err, mut lat, mut j) = ((*p.add(e + 1) == b'5') as u32, 0u32, e + 5);
        while *p.add(j) >= b'0' {
            lat = lat * 10 + (*p.add(j) - b'0') as u32;
            j += 1;
        }
        if *p.add(j) == b'\r' {
            j += 1;
        }
        let id = intern(&data[pos + s..pos + e]);
        let r = (id as u32 & 0xFFF) | (err << 12) | (lat.min(0x7FFFF) << 13);
        let hb = &mut hours[h];
        if hb.0.is_empty() {
            hb.0.reserve_exact(BLOCK_CAP);
        }
        hb.0.push(r);
        if lat >= 0x7FFFF {
            hb.1.push(lat);
        }
        if hb.0.len() == BLOCK_CAP {
            let full = std::mem::replace(hb, (Vec::new(), Vec::new()));
            HOURLY[h].lock().unwrap().push(full);
        }
        pos += j + 1;
    }
    for (h, hb) in hours.into_iter().enumerate() {
        if !hb.0.is_empty() {
            HOURLY[h].lock().unwrap().push(hb);
        }
    }
}

// SAFETY: `i` is in `1..=10`, so `i + 12 <= 22 < 24`, and destination has >= 16 bytes slack.
#[inline(always)]
unsafe fn push_u32(ptr: *mut u8, mut n: u32, d: u8) -> *mut u8 {
    let (mut t, mut i) = ([0u8; 24], 11);
    t[11] = d;
    while n >= 10 {
        i -= 1;
        t[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    i -= 1;
    t[i] = b'0' + n as u8;
    let src = t.as_ptr().add(i);
    (ptr as *mut u64).write_unaligned((src as *const u64).read_unaligned());
    (ptr.add(8) as *mut u32).write_unaligned((src.add(8) as *const u32).read_unaligned());
    ptr.add(12 - i)
}

// SAFETY: `N_HOURS` and `N_EPS` are published before Barrier 1 completes; each worker claims disjoint `idx`.
unsafe fn process_hours() {
    let (n_hours, n_eps) = (N_HOURS.load(Acquire), N_EPS.load(Acquire));
    let mut flat = Vec::new();
    let (mut cnts, mut errs, mut curs) = ([0u32; 4096], [0u32; 4096], [0u32; 4096]);
    loop {
        let idx = NEXT_H.fetch_add(1, Relaxed);
        if idx >= n_hours {
            break;
        }
        let h = ACTIVE_H[idx].load(Relaxed);
        let h_data = std::mem::take(&mut *HOURLY[h].lock().unwrap());
        cnts[..n_eps].fill(0);
        errs[..n_eps].fill(0);
        for hb in &h_data {
            for &r in &hb.0 {
                let ep = (r & 0xFFF) as usize;
                cnts[ep] += 1;
                errs[ep] += (r >> 12) & 1;
            }
        }
        let mut sum = 0u32;
        for i in 0..n_eps {
            curs[i] = sum;
            sum += cnts[i];
        }
        flat.resize(sum as usize, 0);
        for hb in &h_data {
            let mut oi = 0;
            for &r in &hb.0 {
                let ep = (r & 0xFFF) as usize;
                let (slot, mut lat) = (curs[ep] as usize, r >> 13);
                curs[ep] += 1;
                if lat == 0x7FFFF {
                    lat = hb.1[oi];
                    oi += 1;
                }
                flat[slot] = lat;
            }
        }
        drop(h_data);
        for ep in 0..n_eps {
            let n = cnts[ep] as usize;
            if n == 0 {
                continue;
            }
            let (p50, p95, p99) = if n == 1 {
                let v = flat[curs[ep] as usize - 1];
                (v, v, v)
            } else {
                let cur = curs[ep] as usize;
                let s = &mut flat[cur - n..cur];
                let a = (n - 1) / 2;
                let b = (95 * n + 99) / 100 - 1;
                let c = (99 * n + 99) / 100 - 1;
                if n <= 128 {
                    s.sort_unstable();
                } else {
                    s.select_nth_unstable(b);
                    if c > b {
                        s[b + 1..].select_nth_unstable(c - b - 1);
                    }
                    if a < b {
                        s[..b].select_nth_unstable(a);
                    }
                }
                (s[a], s[b], s[c])
            };
            *(SUMMARY.as_ptr() as *mut [u32; 5]).add(idx * 4096 + ep) =
                [n as u32, p50, p95, p99, errs[ep]];
        }
    }
}

// SAFETY: Runs after Barrier 2 (`munmap` + `malloc_trim` + `ORDER` sort). `WRITE_TURN` serializes file writes.
unsafe fn format_hours(out_f: &File) {
    let (n_hours, n_eps) = (N_HOURS.load(Acquire), N_EPS.load(Acquire));
    let cap = (ARENA_BYTES.load(Acquire) + n_eps * 88).max(180 * 1024);
    let mut buf: Vec<u8> = vec![0u8; cap];
    loop {
        let idx = NEXT_FMT.fetch_add(1, Relaxed);
        if idx >= n_hours {
            break;
        }
        let h = ACTIVE_H[idx].load(Relaxed);
        let mut pref = [0u8; 24];
        for (i, chunk) in pref.chunks_exact_mut(8).enumerate() {
            chunk.copy_from_slice(&HOUR_PREF[h][i].load(Relaxed).to_ne_bytes());
        }
        let mut ptr = buf.as_mut_ptr();
        for i in 0..n_eps {
            let ep = ORDER[i].load(Relaxed) as usize;
            let s = *(SUMMARY.as_ptr() as *const [u32; 5]).add(idx * 4096 + ep);
            if s[0] == 0 {
                continue;
            }
            std::ptr::copy_nonoverlapping(pref.as_ptr(), ptr, 21);
            ptr = ptr.add(21);
            let name = get_name_sp(ep);
            std::ptr::copy_nonoverlapping(name.as_ptr(), ptr, name.len());
            ptr = ptr.add(name.len());
            ptr = push_u32(ptr, s[0], b' ');
            ptr = push_u32(ptr, s[1], b' ');
            ptr = push_u32(ptr, s[2], b' ');
            ptr = push_u32(ptr, s[3], b' ');
            if s[4] == 0 {
                (ptr as *mut u32).write_unaligned(0x0a302030);
                ptr = ptr.add(4);
            } else {
                ptr = push_u32(ptr, s[4], b' ');
                ptr = push_u32(ptr, (s[4] as u64 * 1_000_000 / s[0] as u64) as u32, b'\n');
            }
        }
        let written = ptr.offset_from(buf.as_ptr()) as usize;
        while WRITE_TURN.load(Acquire) != idx {
            std::hint::spin_loop();
        }
        (&*out_f).write_all(&buf[..written]).expect("write");
        WRITE_TURN.store(idx + 1, Release);
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let inp = args.next().unwrap_or("/data/requests.log".into());
    let out_p = args.next().unwrap_or("/out/summary.txt".into());
    let file = File::open(&inp).expect("open");
    let (len, fd) = (
        file.metadata().expect("stat").len() as usize,
        file.as_raw_fd(),
    );
    let out_f = File::create(out_p).expect("create");
    if len == 0 {
        return;
    }
    // SAFETY: Read-only private mapping of immutable input log file; unmapped at Barrier 2 before output writes.
    unsafe {
        let data_ptr = mmap(std::ptr::null_mut(), len, 1, 2, fd, 0);
        madvise(data_ptr, len, 3);
        let data = std::slice::from_raw_parts(data_ptr, len);
        let workers = std::env::var("THREADS")
            .map_or(12, |v| v.parse().unwrap_or(12))
            .clamp(1, 12)
            .min(len.div_ceil(64 * 1024));
        let mut bounds = vec![0; workers + 1];
        bounds[workers] = len;
        for w in 1..workers {
            let mut p = len * w / workers;
            while p < len && data[p] != b'\n' {
                p += 1;
            }
            bounds[w] = (p + 1).min(len);
        }
        bounds.dedup();
        let nw = bounds.len() - 1;
        let barrier = Barrier::new(nw);
        let addr = data_ptr as usize;
        std::thread::scope(|s| {
            for w in 0..nw {
                let (st, en, b, out_r) = (bounds[w], bounds[w + 1], &barrier, &out_f);
                s.spawn(move || {
                    parse_worker(data, st, en);
                    if b.wait().is_leader() {
                        let (n_eps, a_bytes) = *LOCK.lock().unwrap();
                        N_EPS.store(n_eps, Release);
                        ARENA_BYTES.store(a_bytes, Release);
                        let mut hrs: Vec<usize> = (0..H_SLOTS)
                            .filter(|&h| HOUR_EPOCH[h].load(Relaxed) != 0)
                            .collect();
                        hrs.sort_unstable_by_key(|&h| HOUR_EPOCH[h].load(Relaxed));
                        for (idx, &h) in hrs.iter().enumerate() {
                            ACTIVE_H[idx].store(h, Relaxed);
                        }
                        N_HOURS.store(hrs.len(), Release);
                    }
                    b.wait();
                    process_hours();
                    if b.wait().is_leader() {
                        munmap(addr as *mut u8, len);
                        malloc_trim(0);
                        let n_eps = N_EPS.load(Relaxed);
                        let mut ord: Vec<u16> = (0..n_eps as u16).collect();
                        ord.sort_unstable_by_key(|&id| get_name_sp(id as usize));
                        for (i, &id) in ord.iter().enumerate() {
                            ORDER[i].store(id, Relaxed);
                        }
                    }
                    b.wait();
                    format_hours(out_r);
                });
            }
        });
        (&out_f).flush().expect("flush");
        std::process::exit(0);
    }
}
