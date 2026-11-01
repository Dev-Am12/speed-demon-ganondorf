// Track A (Ingest): parallelized solution in safe standard-library Rust.
// Implements the HTTP request aggregation contract in specs/a-ingest.md.

use std::arch::x86_64::*;
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::FileExt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Mutex};
use std::thread;

const HOURS: usize = 72;
const CHUNK_SIZE: usize = 64 * 1024;
const DATES: [&str; 4] = ["2026-10-30", "2026-10-31", "2026-11-01", "2026-11-02"];

struct Table {
    slots: Vec<u64>,
    arena: Vec<u8>,
    offsets: Vec<(u32, u16)>,
}

impl Table {
    fn new() -> Self {
        Self {
            slots: vec![0u64; 4096],
            arena: Vec::with_capacity(65536),
            offsets: Vec::with_capacity(2048),
        }
    }

    #[inline(always)]
    fn hash(name: &[u8]) -> (u32, usize) {
        let len = name.len();
        let h = if len >= 8 {
            unsafe {
                let first = (name.as_ptr() as *const u64).read_unaligned();
                let last = (name.as_ptr().add(len - 8) as *const u64).read_unaligned();
                (first ^ last.rotate_left(32) ^ (len as u64)).wrapping_mul(0x517cc1b727220a95)
            }
        } else {
            let mut h = len as u64;
            for &b in name {
                h = (h ^ b as u64).wrapping_mul(0x100000001b3);
            }
            h
        };
        let tag = (h >> 32) as u32 ^ (h as u32);
        (tag, h as usize)
    }

    #[inline(always)]
    fn get_name(&self, id: u16) -> &[u8] {
        let (start, len) = self.offsets[id as usize];
        &self.arena[start as usize..start as usize + len as usize]
    }

    fn intern(&mut self, name: &[u8]) -> u16 {
        let (tag, h) = Self::hash(name);
        let mut i = h & 4095;
        loop {
            let slot = self.slots[i];
            if slot == 0 {
                let id = self.offsets.len() as u16;
                assert!(id < 2048, "endpoints > 2047");
                self.offsets
                    .push((self.arena.len() as u32, name.len() as u16));
                self.arena.extend_from_slice(name);
                self.slots[i] = ((tag as u64) << 32) | (id as u64 + 1);
                return id;
            }
            if (slot >> 32) as u32 == tag && self.get_name((slot as u32 - 1) as u16) == name {
                return (slot as u32 - 1) as u16;
            }
            i = (i + 1) & 4095;
        }
    }
}

struct HourData {
    records: Vec<u32>,
    overflow: Vec<(u32, u32)>,
}

struct Local {
    table: Table,
    hours: Vec<HourData>,
    cap: usize,
}

#[inline(always)]
unsafe fn find_space_avx2(ptr: *const u8) -> usize {
    let space = _mm256_set1_epi8(b' ' as i8);
    let mut offset = 0;
    loop {
        let v = _mm256_loadu_si256(ptr.add(offset) as *const __m256i);
        let mask = _mm256_movemask_epi8(_mm256_cmpeq_epi8(v, space)) as u32;
        if mask != 0 {
            return offset + mask.trailing_zeros() as usize;
        }
        offset += 32;
    }
}

#[inline(always)]
fn parse_line(buf: &[u8], pos: usize, local: &mut Local) -> usize {
    unsafe {
        let p = buf.as_ptr().add(pos);
        let day_idx = (*p.add(9) - b'0') as usize + ((*p.add(6) == b'1') as usize);
        let hour = day_idx * 24 + ((*p.add(11) - b'0') * 10 + *p.add(12) - b'0') as usize - 18;

        let mut method_space = 31;
        while *p.add(method_space) != b' ' {
            method_space += 1;
        }

        let ep_start = method_space + 1;
        let ep_len = find_space_avx2(p.add(ep_start));
        let ep_end = ep_start + ep_len;

        let err = (*p.add(ep_end + 1) == b'5') as u32;
        let mut latency = 0u32;
        let mut j = ep_end + 5;
        while *p.add(j) != b'\n' {
            latency = latency * 10 + (*p.add(j) - b'0') as u32;
            j += 1;
        }

        let name = std::slice::from_raw_parts(p.add(ep_start), ep_len);
        let id = local.table.intern(name);
        let cap = local.cap;
        let hd = &mut local.hours[hour];
        if hd.records.capacity() == 0 {
            hd.records.reserve(cap);
        }
        if latency >= 0xFFFFF {
            hd.overflow.push((hd.records.len() as u32, latency));
        }
        hd.records
            .push((id as u32 & 0x7FF) | (err << 11) | (latency.min(0xFFFFF) << 12));
        pos + j + 1
    }
}

fn parse_worker_range(file: &File, mut start: usize, end: usize) -> Local {
    let cap = ((end - start) / 100).clamp(64, 50000);
    let mut local = Local {
        table: Table::new(),
        hours: (0..HOURS)
            .map(|_| HourData {
                records: Vec::new(),
                overflow: Vec::new(),
            })
            .collect(),
        cap,
    };
    let mut buf = [0u8; CHUNK_SIZE + 64];
    let mut carried = 0;
    while start < end {
        let to_read = (CHUNK_SIZE - carried).min(end - start);
        let n = file
            .read_at(&mut buf[carried..carried + to_read], start as u64)
            .expect("read_at");
        if n == 0 {
            break;
        }
        let valid = carried + n;
        let last_nl = match buf[..valid].iter().rposition(|&b| b == b'\n') {
            Some(nl) => nl,
            None => {
                carried = valid;
                start += n;
                continue;
            }
        };
        let mut pos = 0;
        while pos < last_nl {
            pos = parse_line(&buf, pos, &mut local);
        }
        buf.copy_within(last_nl + 1..valid, 0);
        carried = valid - (last_nl + 1);
        start += n;
    }
    local
}

#[inline(always)]
fn push_field(buf: &mut Vec<u8>, mut n: u32, delim: u8) {
    let mut temp = [0u8; 12];
    let mut i = 11;
    temp[11] = delim;
    loop {
        i -= 1;
        temp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    buf.extend_from_slice(&temp[i..12]);
}

#[inline(always)]
fn percentile_values(latencies: &mut [u32]) -> (u32, u32, u32) {
    let n = latencies.len();
    latencies.sort_unstable();
    (
        latencies[(50 * n + 99) / 100 - 1],
        latencies[(95 * n + 99) / 100 - 1],
        latencies[(99 * n + 99) / 100 - 1],
    )
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (input, output) = (
        args.get(1).map_or("/data/requests.log", |s| s.as_str()),
        args.get(2).map_or("/out/summary.txt", |s| s.as_str()),
    );
    let file = File::open(input).expect("open input");
    let len = file.metadata().expect("stat").len() as usize;
    if len == 0 {
        File::create(output).expect("create output");
        return;
    }

    let workers = std::env::var("THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(12)
        .clamp(1, 12);
    let (parse_workers, agg_workers) = (workers.min(len.div_ceil(CHUNK_SIZE)), workers);

    let mut bounds = vec![0; parse_workers + 1];
    let mut buf = [0u8; 1024];
    for w in 1..parse_workers {
        let mut pos = len * w / parse_workers;
        while let Ok(n) = file.read_at(&mut buf[..1024.min(len - pos)], pos as u64) {
            if let Some(i) = buf[..n].iter().position(|&b| b == b'\n') {
                pos += i + 1;
                break;
            }
            pos += n;
        }
        bounds[w] = pos;
    }
    bounds[parse_workers] = len;
    bounds.dedup();

    let (tables, hours_by_worker): (Vec<Table>, Vec<Vec<HourData>>) = thread::scope(|s| {
        let tasks: Vec<_> = bounds
            .windows(2)
            .map(|w| {
                let (start, end) = (w[0], w[1]);
                s.spawn(move || {
                    let loc = parse_worker_range(&File::open(input).expect("open"), start, end);
                    (loc.table, loc.hours)
                })
            })
            .collect();
        tasks.into_iter().map(|t| t.join().unwrap()).unzip()
    });

    let mut names: Vec<&[u8]> = Vec::with_capacity(2048);
    let mut slots = [0u64; 4096];
    for t in &tables {
        for id in 0..t.offsets.len() as u16 {
            let name = t.get_name(id);
            let (tag, h) = Table::hash(name);
            let mut i = h & 4095;
            loop {
                let slot = slots[i];
                if slot == 0 {
                    assert!(names.len() < 2048, "endpoints > 2047");
                    slots[i] = ((tag as u64) << 32) | ((names.len() + 1) as u64);
                    names.push(name);
                    break;
                }
                if (slot >> 32) as u32 == tag && names[(slot as u32 - 1) as usize] == name {
                    break;
                }
                i = (i + 1) & 4095;
            }
        }
    }
    names.sort_unstable();
    let num_endpoints = names.len();

    slots.fill(0);
    for (gid, &name) in names.iter().enumerate() {
        let (tag, h) = Table::hash(name);
        let mut i = h & 4095;
        loop {
            if slots[i] == 0 {
                slots[i] = ((tag as u64) << 32) | ((gid + 1) as u64);
                break;
            }
            i = (i + 1) & 4095;
        }
    }

    let remaps: Vec<Vec<u16>> = tables
        .iter()
        .map(|t| {
            (0..t.offsets.len() as u16)
                .map(|id| {
                    let name = t.get_name(id);
                    let (tag, h) = Table::hash(name);
                    let mut i = h & 4095;
                    loop {
                        let slot = slots[i];
                        if (slot >> 32) as u32 == tag {
                            let cand = (slot as u32 - 1) as usize;
                            if names[cand] == name {
                                return cand as u16;
                            }
                        }
                        i = (i + 1) & 4095;
                    }
                })
                .collect()
        })
        .collect();

    let mut hourly: Vec<_> = (0..HOURS).map(|_| Some(Vec::new())).collect();
    for hours in hours_by_worker {
        for (h, hd) in hours.into_iter().enumerate() {
            hourly[h].as_mut().unwrap().push(hd);
        }
    }
    let hourly = Mutex::new(hourly);

    let mut prefixes = [[0u8; 21]; HOURS];
    for (hour, p) in prefixes.iter_mut().enumerate() {
        let (h, d) = ((hour + 18) % 24, (hour + 18) / 24);
        p[0..10].copy_from_slice(DATES[d].as_bytes());
        p[10] = b'T';
        p[11] = b'0' + (h / 10) as u8;
        p[12] = b'0' + (h % 10) as u8;
        p[13..21].copy_from_slice(b":00:00Z ");
    }

    let next_hour = AtomicUsize::new(0);
    let (tx, rx) = mpsc::channel();

    thread::scope(|scope| {
        for _ in 0..agg_workers {
            let next = &next_hour;
            let names = &names;
            let hourly = &hourly;
            let remaps = &remaps;
            let prefixes = &prefixes;
            let tx = tx.clone();
            scope.spawn(move || {
                let mut flat_lats: Vec<u32> = Vec::with_capacity(65536);
                let mut counts = vec![0u32; num_endpoints];
                let mut err_counts = vec![0u32; num_endpoints];
                let mut cursors = vec![0u32; num_endpoints];

                loop {
                    let hour = next.fetch_add(1, Ordering::Relaxed);
                    if hour >= HOURS {
                        break;
                    }
                    let hour_data = hourly.lock().unwrap()[hour].take().unwrap();
                    counts.fill(0);
                    err_counts.fill(0);

                    for (w, hd) in hour_data.iter().enumerate() {
                        let w_remap = &remaps[w];
                        for &r in &hd.records {
                            let gid = w_remap[(r & 0x7FF) as usize] as usize;
                            counts[gid] += 1;
                            if (r & 0x800) != 0 {
                                err_counts[gid] += 1;
                            }
                        }
                    }

                    let mut sum = 0u32;
                    for i in 0..num_endpoints {
                        cursors[i] = sum;
                        sum += counts[i];
                    }
                    if flat_lats.len() < sum as usize {
                        flat_lats.resize(sum as usize, 0);
                    }

                    for (w, hd) in hour_data.iter().enumerate() {
                        let w_remap = &remaps[w];
                        let mut of_idx = 0;
                        for &r in &hd.records {
                            let gid = w_remap[(r & 0x7FF) as usize] as usize;
                            let slot = cursors[gid] as usize;
                            cursors[gid] += 1;
                            let lat = r >> 12;
                            let lat = if lat == 0xFFFFF {
                                let full = hd.overflow[of_idx].1;
                                of_idx += 1;
                                full
                            } else {
                                lat
                            };
                            flat_lats[slot] = lat;
                        }
                    }
                    drop(hour_data);

                    let mut out = Vec::with_capacity(160 * 1024);
                    for ep in 0..num_endpoints {
                        let n = counts[ep] as usize;
                        if n == 0 {
                            continue;
                        }
                        let end = cursors[ep] as usize;
                        let start = end - n;
                        let ep_lats = &mut flat_lats[start..end];
                        let (p50, p95, p99) = percentile_values(ep_lats);
                        let errors = err_counts[ep];
                        out.extend_from_slice(&prefixes[hour]);
                        out.extend_from_slice(names[ep]);
                        out.push(b' ');
                        push_field(&mut out, n as u32, b' ');
                        push_field(&mut out, p50, b' ');
                        push_field(&mut out, p95, b' ');
                        push_field(&mut out, p99, b' ');
                        push_field(&mut out, errors, b' ');
                        push_field(
                            &mut out,
                            (errors as u64 * 1_000_000 / n as u64) as u32,
                            b'\n',
                        );
                    }
                    let _ = tx.send((hour, out));
                }
            });
        }
        drop(tx);

        let mut out = File::create(output).expect("create output");
        let mut pending = vec![None; HOURS];
        let mut next_to_write = 0;

        while next_to_write < HOURS {
            let (h, buf) = rx.recv().expect("receive hour");
            pending[h] = Some(buf);
            while next_to_write < HOURS && pending[next_to_write].is_some() {
                let b = pending[next_to_write].take().unwrap();
                out.write_all(&b).expect("write summary.txt");
                drop(b);
                next_to_write += 1;
            }
        }
    });
}
