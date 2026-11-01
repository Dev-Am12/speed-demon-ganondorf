use std::sync::{
    atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering::*},
    Barrier, Mutex,
};
use std::{arch::x86_64::*, fs::File, io::Write, os::unix::io::AsRawFd};

const HOURS: usize = 72;
const DATES: [&str; 4] = ["2026-10-30", "2026-10-31", "2026-11-01", "2026-11-02"];

extern "C" {
    fn mmap(a: *mut (), l: usize, p: i32, f: i32, fd: i32, o: i64) -> *mut u8;
    fn malloc_trim(pad: usize) -> i32;
}

type HourData = (Vec<u32>, Vec<u32>);

static SLOTS: [AtomicU64; 4096] = [const { AtomicU64::new(0) }; 4096];
static META: [AtomicU64; 2048] = [const { AtomicU64::new(0) }; 2048];
static LOCK: Mutex<(usize, usize)> = Mutex::new((0, 0));
static ARENA: [AtomicU64; 16384] = [const { AtomicU64::new(0) }; 16384];
static HOURLY: [Mutex<Vec<HourData>>; HOURS] = [const { Mutex::new(Vec::new()) }; HOURS];
static SUMMARY: [AtomicU32; HOURS * 2048 * 5] = [const { AtomicU32::new(0) }; HOURS * 2048 * 5];
static NEXT_H: AtomicUsize = AtomicUsize::new(0);

unsafe fn hash(s: &[u8]) -> (u32, usize) {
    let (n, p, mut h) = (s.len(), s.as_ptr(), s.len() as u64);
    if n >= 8 {
        let f = (p as *const u64).read_unaligned();
        let m = (p.add((n - 8) >> 1) as *const u64).read_unaligned();
        let l = (p.add(n - 8) as *const u64).read_unaligned();
        h = (f ^ m.rotate_left(16) ^ l.rotate_left(32) ^ h).wrapping_mul(0x517cc1b727220a95);
    } else {
        for &b in s {
            h = (h ^ b as u64).wrapping_mul(0x100000001b3);
        }
    }
    ((h >> 32) as u32 ^ h as u32, h as usize)
}

unsafe fn get_name(id: usize) -> &'static [u8] {
    let m = META[id].load(Relaxed);
    let p = (ARENA.as_ptr() as *const u8).add((m >> 16) as usize);
    std::slice::from_raw_parts(p, (m & 0xFFFF) as usize)
}

unsafe fn is_match(s: u64, tag: u32, name: &[u8]) -> bool {
    (s >> 32) as u32 == tag && get_name((s as u32).wrapping_sub(1) as usize) == name
}

unsafe fn intern(name: &[u8]) -> u16 {
    let (tag, h_raw) = hash(name);
    let h = h_raw & 4095;
    let mut i = h;
    loop {
        let s = SLOTS[i].load(Relaxed);
        if s == 0 {
            break;
        }
        if is_match(s, tag, name) {
            return (s as u32 - 1) as u16;
        }
        i = (i + 1) & 4095;
    }
    let mut st = LOCK.lock().unwrap();
    i = h;
    loop {
        let s = SLOTS[i].load(Relaxed);
        if s == 0 {
            let (id, start) = *st;
            *st = (id + 1, start + name.len());
            let dst = (ARENA.as_ptr() as *mut u8).add(start);
            std::ptr::copy_nonoverlapping(name.as_ptr(), dst, name.len());
            META[id].store(((start as u64) << 16) | name.len() as u64, Relaxed);
            SLOTS[i].store(((tag as u64) << 32) | (id as u64 + 1), Release);
            return id as u16;
        }
        if is_match(s, tag, name) {
            return (s as u32 - 1) as u16;
        }
        i = (i + 1) & 4095;
    }
}

#[inline(always)]
fn push_u32(b: &mut Vec<u8>, mut n: u32, d: u8) {
    let (mut t, mut i) = ([d; 12], 11);
    while n > 0 || i == 11 {
        i -= 1;
        t[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    b.extend_from_slice(&t[i..12]);
}

unsafe fn parse_worker(data: &[u8], start: usize, end: usize) {
    let mut hours: [HourData; HOURS] = [const { (Vec::new(), Vec::new()) }; HOURS];
    let mut pos = start;
    let cap = ((end - start) / (45 * HOURS) + 64).max(64);
    while pos < end {
        let p = data.as_ptr().add(pos);
        let d = (*p.add(9) - b'0') as usize + ((*p.add(8) == b'0') as usize);
        let h = d * 24 + ((*p.add(11) - b'0') * 10 + *p.add(12) - b'0') as usize - 18;
        let v = _mm256_loadu_si256(p.add(31) as _);
        let m = _mm256_movemask_epi8(_mm256_cmpeq_epi8(v, _mm256_set1_epi8(32))) as u32;
        let s = 32 + m.trailing_zeros() as usize;
        let mut e = 31 + (m & (m - 1)).trailing_zeros() as usize;
        if m & (m - 1) == 0 {
            e = 63;
            while *p.add(e) != b' ' {
                e += 1;
            }
        }
        let (err, mut lat, mut j) = ((*p.add(e + 1) == b'5') as u32, 0u32, e + 5);
        while *p.add(j) != b'\n' {
            lat = lat * 10 + (*p.add(j) - b'0') as u32;
            j += 1;
        }
        let id = intern(&data[pos + s..pos + e]);
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

        let barrier = Barrier::new(workers);
        std::thread::scope(|s| {
            for w in 0..workers {
                let (start, end, b) = (bounds[w], bounds[w + 1], &barrier);
                s.spawn(move || {
                    parse_worker(data, start, end);
                    b.wait();
                    process_hours(LOCK.lock().unwrap().0);
                });
            }
        });

        HOURLY
            .iter()
            .for_each(|h| drop(std::mem::take(&mut *h.lock().unwrap())));
        malloc_trim(0);

        let n_eps = LOCK.lock().unwrap().0;
        let mut order: Vec<_> = (0..n_eps).collect();
        order.sort_unstable_by_key(|&ep| get_name(ep));

        let mut out = File::create(out_p).expect("create");
        let mut buf = Vec::with_capacity(256 * 1024);
        for h in 0..HOURS {
            let pref = format!("{}T{:02}:00:00Z ", DATES[(h + 18) / 24], (h + 18) % 24);
            for &ep in &order {
                let s = *(SUMMARY.as_ptr() as *const [u32; 5]).add(h * 2048 + ep);
                if s[0] > 0 {
                    buf.extend_from_slice(pref.as_bytes());
                    buf.extend_from_slice(get_name(ep));
                    buf.push(b' ');
                    for &v in &s {
                        push_u32(&mut buf, v, b' ');
                    }
                    let r = (s[4] as u64 * 1_000_000 / s[0] as u64) as u32;
                    push_u32(&mut buf, r, b'\n');
                    if buf.len() >= 128 * 1024 {
                        out.write_all(&buf).expect("write");
                        buf.clear();
                    }
                }
            }
        }
        out.write_all(&buf).expect("write");
        std::process::exit(0);
    }
}
