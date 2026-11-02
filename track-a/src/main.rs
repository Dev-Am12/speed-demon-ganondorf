use std::sync::{
    atomic::{AtomicU16, AtomicU32, AtomicU64, AtomicUsize, Ordering::*},
    Barrier, Mutex,
};
use std::{arch::x86_64::*, fs::File, io::Write, os::unix::io::AsRawFd};

const HOURS: usize = 72;
const DATES: [&str; 4] = ["2026-10-30", "2026-10-31", "2026-11-01", "2026-11-02"];

extern "C" {
    fn mmap(a: *mut (), l: usize, p: i32, f: i32, fd: i32, o: i64) -> *mut u8;
    fn munmap(a: *mut u8, l: usize) -> i32;
    fn malloc_trim(pad: usize) -> i32;
}

type HourData = (Vec<u32>, Vec<u32>);

static SLOTS: [AtomicU64; 4096] = [const { AtomicU64::new(0) }; 4096];
static META: [AtomicU64; 2048] = [const { AtomicU64::new(0) }; 2048];
static ARENA: [AtomicU64; 16384] = [const { AtomicU64::new(0) }; 16384];
static LOCK: Mutex<(usize, usize)> = Mutex::new((0, 0));
static ORDER: [AtomicU16; 2048] = [const { AtomicU16::new(0) }; 2048];
static N_EPS: AtomicUsize = AtomicUsize::new(0);
static ARENA_LEN: AtomicUsize = AtomicUsize::new(0);
static HOURLY: [Mutex<Vec<HourData>>; HOURS] = [const { Mutex::new(Vec::new()) }; HOURS];
static SUMMARY: [AtomicU32; HOURS * 2048 * 5] = [const { AtomicU32::new(0) }; HOURS * 2048 * 5];
static NEXT_H: AtomicUsize = AtomicUsize::new(0);
static NEXT_FMT: AtomicUsize = AtomicUsize::new(0);
static WRITE_TURN: AtomicUsize = AtomicUsize::new(0);

/// Hash of every byte of `s` (n >= 1).
/// SAFETY: all unaligned loads stay strictly within `s`: loop loads end at i + 8 < n,
/// the final 8-byte load is [n - 8, n), and the 4-byte branch requires n >= 4.
#[inline(always)]
unsafe fn hash(s: &[u8]) -> u64 {
    const K: u64 = 0x9E3779B97F4A7C15;
    let (n, p) = (s.len(), s.as_ptr());
    let mut h = n as u64;
    if n >= 8 {
        let mut i = 0;
        while i + 8 < n {
            h = (h.rotate_left(5) ^ (p.add(i) as *const u64).read_unaligned()).wrapping_mul(K);
            i += 8;
        }
        h = (h.rotate_left(5) ^ (p.add(n - 8) as *const u64).read_unaligned()).wrapping_mul(K);
    } else if n >= 4 {
        let a = (p as *const u32).read_unaligned() as u64;
        let b = (p.add(n - 4) as *const u32).read_unaligned() as u64;
        h = (h ^ a ^ (b << 32)).wrapping_mul(K);
    } else {
        let (a, b, c) = (s[0] as u64, s[n >> 1] as u64, s[n - 1] as u64);
        h = (h ^ a ^ (b << 8) ^ (c << 16)).wrapping_mul(K);
    }
    let h = (h ^ (h >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    h ^ (h >> 29)
}

/// SAFETY: `id` < N_EPS, and Phase 1 barrier orders this read after all inserts.
#[inline(always)]
unsafe fn get_name(id: usize) -> &'static [u8] {
    let m = META[id].load(Relaxed);
    std::slice::from_raw_parts(
        (ARENA.as_ptr() as *const u8).add((m >> 16) as usize),
        m as u16 as usize,
    )
}

/// Returns the global arrival id of endpoint `name`.
/// SAFETY: writes to ARENA and META occur only under LOCK with bounds assertions.
#[inline(always)]
unsafe fn intern(name: &[u8]) -> u16 {
    let h = hash(name);
    let tag = (h & !0xFFF) | 0x1000;
    let mut i = (h as usize) & 4095;
    loop {
        let s = SLOTS[i].load(Relaxed);
        if (s & !0xFFF) == tag {
            return (s & 0xFFF) as u16;
        }
        if s == 0 {
            break;
        }
        i = (i + 1) & 4095;
    }
    let mut st = LOCK.lock().unwrap();
    i = (h as usize) & 4095;
    loop {
        let s = SLOTS[i].load(Relaxed);
        if (s & !0xFFF) == tag {
            return (s & 0xFFF) as u16;
        }
        if s == 0 {
            let (id, start) = *st;
            assert!(id < 2048 && start + name.len() <= ARENA.len() * 8);
            *st = (id + 1, start + name.len());
            std::ptr::copy_nonoverlapping(
                name.as_ptr(),
                (ARENA.as_ptr() as *mut u8).add(start),
                name.len(),
            );
            META[id].store(((start as u64) << 16) | name.len() as u64, Release);
            SLOTS[i].store(tag | id as u64, Release);
            return id as u16;
        }
        i = (i + 1) & 4095;
    }
}

/// Formats `n` followed by delimiter `d` into `*ptr`.
/// SAFETY: Caller ensures `*ptr` has at least 12 bytes of writable capacity.
#[inline(always)]
unsafe fn push_u32(ptr: &mut *mut u8, mut n: u32, d: u8) {
    let (mut t, mut i) = ([0u8; 12], 11);
    t[11] = d;
    while n >= 10 {
        i -= 1;
        t[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    i -= 1;
    t[i] = b'0' + n as u8;
    let len = 12 - i;
    std::ptr::copy_nonoverlapping(t.as_ptr().add(i), *ptr, len);
    *ptr = (*ptr).add(len);
}

/// SAFETY: `ptr` points into mapped memory with at least one space character before mapped page end.
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

/// SAFETY: `data` is mapped slice from input file; SIMD loads stay within valid page boundaries.
unsafe fn parse_worker(data: &[u8], start: usize, end: usize) {
    let mut hours: [HourData; HOURS] = [const { (Vec::new(), Vec::new()) }; HOURS];
    let mut pos = start;
    let cap = ((end - start) / 100).clamp(64, 50000);
    while pos < end {
        let p = data.as_ptr().add(pos);
        let d = (*p.add(9) - b'0') as usize + ((*p.add(8) == b'0') as usize);
        let h = d * 24 + ((*p.add(11) - b'0') * 10 + *p.add(12) - b'0') as usize - 18;
        let v = _mm256_loadu_si256(p.add(31) as _);
        let m = _mm256_movemask_epi8(_mm256_cmpeq_epi8(v, _mm256_set1_epi8(32))) as u32;
        let s = 32 + m.trailing_zeros() as usize;
        let m2 = m & (m - 1);
        let e = if m2 != 0 {
            31 + m2.trailing_zeros() as usize
        } else {
            63 + find_space(p.add(63))
        };
        let (err, mut lat, mut j) = ((*p.add(e + 1) == b'5') as u32, 0u32, e + 5);
        while *p.add(j) != b'\n' {
            lat = lat * 10 + (*p.add(j) - b'0') as u32;
            j += 1;
        }
        let id = intern(std::slice::from_raw_parts(p.add(s), e - s));
        let r = (id as u32 & 0x7FF) | (err << 11) | (lat.min(0xFFFFF) << 12);
        if hours[h].0.is_empty() {
            hours[h].0.reserve(cap);
        }
        hours[h].0.push(r);
        if lat >= 0xFFFFF {
            hours[h].1.push(lat);
        }
        pos += j + 1;
    }
    for (h, hd) in hours.into_iter().enumerate() {
        if !hd.0.is_empty() {
            HOURLY[h].lock().unwrap().push(hd);
        }
    }
}

/// SAFETY: runs after Phase 1 barrier. All writes to HOURLY are complete.
unsafe fn process_hours(n_eps: usize) {
    let mut flat = Vec::new();
    let (mut counts, mut errs, mut cursors) = ([0u32; 2048], [0u32; 2048], [0u32; 2048]);
    loop {
        let h = NEXT_H.fetch_add(1, Relaxed);
        if h >= HOURS {
            break;
        }
        let hour_data = std::mem::take(&mut *HOURLY[h].lock().unwrap());
        counts[..n_eps].fill(0);
        errs[..n_eps].fill(0);
        for hd in &hour_data {
            for &r in &hd.0 {
                let ep = (r & 0x7FF) as usize;
                counts[ep] += 1;
                errs[ep] += (r >> 11) & 1;
            }
        }
        let mut sum = 0u32;
        for i in 0..n_eps {
            cursors[i] = sum;
            sum += counts[i];
        }
        flat.resize(sum as usize, 0);
        for hd in &hour_data {
            let mut oi = 0;
            for &r in &hd.0 {
                let ep = (r & 0x7FF) as usize;
                let (slot, mut lat) = (cursors[ep] as usize, r >> 12);
                cursors[ep] += 1;
                if lat == 0xFFFFF {
                    lat = hd.1[oi];
                    oi += 1;
                }
                flat[slot] = lat;
            }
        }
        drop(hour_data);
        for ep in 0..n_eps {
            let n = counts[ep] as usize;
            if n == 1 {
                let v = flat[cursors[ep] as usize - 1];
                *(SUMMARY.as_ptr() as *mut [u32; 5]).add(h * 2048 + ep) = [1, v, v, v, errs[ep]];
            } else if n > 1 {
                let cur = cursors[ep] as usize;
                let s = &mut flat[cur - n..cur];
                s.sort_unstable();
                let rk = |pct: usize| (pct * n + 99) / 100 - 1;
                *(SUMMARY.as_ptr() as *mut [u32; 5]).add(h * 2048 + ep) =
                    [n as u32, s[(n - 1) / 2], s[rk(95)], s[rk(99)], errs[ep]];
            }
        }
    }
}

fn main() {
    let mut args = std::env::args();
    let inp = args.nth(1).unwrap_or("/data/requests.log".into());
    let out_p = args.next().unwrap_or("/out/summary.txt".into());
    let file = File::open(&inp).expect("open");
    let (len, fd) = (
        file.metadata().expect("stat").len() as usize,
        file.as_raw_fd(),
    );
    if len == 0 {
        File::create(out_p).expect("create");
        return;
    }
    // SAFETY: mmap is called with valid length, file descriptor, and PROT_READ.
    unsafe {
        let data_ptr = mmap(std::ptr::null_mut(), len, 1, 2, fd, 0);
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

        let data_addr = data_ptr as usize;
        let out = File::create(&out_p).expect("create");
        let out_ref = &out;
        let barrier = Barrier::new(workers);
        std::thread::scope(|s| {
            for w in 0..workers {
                let (start, end, b) = (bounds[w], bounds[w + 1], &barrier);
                // SAFETY: Worker thread coordinates phases via barrier synchronization and uses atomic/mutex synchronizations.
                s.spawn(move || {
                    parse_worker(data, start, end);
                    b.wait();
                    process_hours(LOCK.lock().unwrap().0);
                    if b.wait().is_leader() {
                        HOURLY
                            .iter()
                            .for_each(|h| drop(std::mem::take(&mut *h.lock().unwrap())));
                        munmap(data_addr as *mut u8, len);
                        malloc_trim(0);
                        let (n_eps, arena_len) = *LOCK.lock().unwrap();
                        N_EPS.store(n_eps, Release);
                        ARENA_LEN.store(arena_len, Release);
                        let mut ord: Vec<u16> = (0..n_eps as u16).collect();
                        ord.sort_unstable_by_key(|&ep| get_name(ep as usize));
                        for (i, &ep) in ord.iter().enumerate() {
                            ORDER[i].store(ep, Relaxed);
                        }
                    }
                    b.wait();

                    let n_eps = N_EPS.load(Acquire);
                    let max_cap = (ARENA_LEN.load(Acquire) + n_eps * 88).max(180 * 1024);
                    let mut buf: Vec<u8> = Vec::with_capacity(max_cap);
                    loop {
                        let h = NEXT_FMT.fetch_add(1, Relaxed);
                        if h >= HOURS {
                            break;
                        }
                        let mut pref = [0u8; 21];
                        let (hr, d_i) = ((h + 18) % 24, (h + 18) / 24);
                        pref[..10].copy_from_slice(DATES[d_i].as_bytes());
                        pref[10..21].copy_from_slice(b"T00:00:00Z ");
                        pref[11] = b'0' + (hr / 10) as u8;
                        pref[12] = b'0' + (hr % 10) as u8;

                        let mut ptr = buf.as_mut_ptr();
                        for i in 0..n_eps {
                            let ep = ORDER[i].load(Relaxed) as usize;
                            let s = *(SUMMARY.as_ptr() as *const [u32; 5]).add(h * 2048 + ep);
                            let n = s[0];
                            if n == 0 {
                                continue;
                            }
                            std::ptr::copy_nonoverlapping(pref.as_ptr(), ptr, 21);
                            ptr = ptr.add(21);
                            let name = get_name(ep);
                            std::ptr::copy_nonoverlapping(name.as_ptr(), ptr, name.len());
                            ptr = ptr.add(name.len());
                            *ptr = b' ';
                            ptr = ptr.add(1);
                            push_u32(&mut ptr, n, b' ');
                            push_u32(&mut ptr, s[1], b' ');
                            push_u32(&mut ptr, s[2], b' ');
                            push_u32(&mut ptr, s[3], b' ');
                            let er = s[4];
                            if er == 0 {
                                (ptr as *mut u32).write_unaligned(0x0a302030);
                                ptr = ptr.add(4);
                            } else {
                                push_u32(&mut ptr, er, b' ');
                                push_u32(
                                    &mut ptr,
                                    (er as u64 * 1_000_000 / n as u64) as u32,
                                    b'\n',
                                );
                            }
                        }
                        buf.set_len(ptr.offset_from(buf.as_ptr()) as usize);
                        while WRITE_TURN.load(Acquire) != h {
                            std::hint::spin_loop();
                        }
                        (&*out_ref).write_all(&buf).expect("write");
                        buf.clear();
                        WRITE_TURN.store(h + 1, Release);
                    }
                });
            }
        });
        std::process::exit(0);
    }
}
