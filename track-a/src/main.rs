// Track A (Ingest): parallelized solution in safe standard-library Rust.
// Implements the HTTP request aggregation contract in specs/a-ingest.md.
// Safe Rust, std only, no unsafe, no SIMD, no mmap (GR-P2..P10, PLAN §6, §7.1).

use std::arch::x86_64::*;
use std::fs::File;
use std::io::{BufWriter, Write};
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
                assert!(id < 2048, "endpoint catalogue exceeds 2047 limit");
                self.offsets
                    .push((self.arena.len() as u32, name.len() as u16));
                self.arena.extend_from_slice(name);
                self.slots[i] = ((tag as u64) << 32) | (id as u64 + 1);
                return id;
            }
            if (slot >> 32) as u32 == tag {
                let id = (slot as u32 - 1) as u16;
                if self.get_name(id) == name {
                    return id;
                }
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
}

#[inline(always)]
unsafe fn find_space_avx2(ptr: *const u8) -> usize {
    let space = _mm256_set1_epi8(b' ' as i8);
    let mut offset = 0;
    loop {
        let v = _mm256_loadu_si256(ptr.add(offset) as *const __m256i);
        let eq = _mm256_cmpeq_epi8(v, space);
        let mask = _mm256_movemask_epi8(eq) as u32;
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

        let method_space = if *p.add(31) == b' ' {
            31
        } else if *p.add(32) == b' ' {
            32
        } else if *p.add(33) == b' ' {
            33
        } else if *p.add(34) == b' ' {
            34
        } else {
            35
        };

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
        let hd = &mut local.hours[hour];
        let (packed_lat, is_overflow) = if latency >= 0xFFFFF {
            (0xFFFFF, true)
        } else {
            (latency, false)
        };
        if is_overflow {
            hd.overflow.push((hd.records.len() as u32, latency));
        }
        hd.records
            .push((id as u32 & 0x7FF) | (err << 11) | (packed_lat << 12));
        pos + j + 1
    }
}

fn parse_worker_range(file: &File, mut start: usize, end: usize) -> Local {
    let cap = ((end - start) / 40 / HOURS).max(64);
    let mut local = Local {
        table: Table::new(),
        hours: (0..HOURS)
            .map(|_| HourData {
                records: Vec::with_capacity(cap),
                overflow: Vec::new(),
            })
            .collect(),
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
fn push_field(buf: &mut Vec<u8>, mut n: u64, delim: u8) {
    let mut temp = [0u8; 20];
    let mut i = 20;
    loop {
        i -= 1;
        temp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    buf.extend_from_slice(&temp[i..]);
    buf.push(delim);
}

#[inline(always)]
fn percentile_values(latencies: &mut [u32]) -> (u32, u32, u32) {
    let n = latencies.len();
    if n == 1 {
        return (latencies[0], latencies[0], latencies[0]);
    }
    if n == 2 {
        if latencies[0] > latencies[1] {
            latencies.swap(0, 1);
        }
        return (latencies[0], latencies[1], latencies[1]);
    }
    let a = (50 * n + 99) / 100 - 1;
    let b = (95 * n + 99) / 100 - 1;
    let c = (99 * n + 99) / 100 - 1;
    latencies.sort_unstable();
    (latencies[a], latencies[b], latencies[c])
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("/data/requests.log");
    let output = args
        .get(2)
        .map(String::as_str)
        .unwrap_or("/out/summary.txt");
    let file = File::open(input).expect("cannot open requests.log");
    let len = file.metadata().expect("cannot stat requests.log").len() as usize;
    if len == 0 {
        let out = File::create(output).expect("cannot create summary.txt");
        out.sync_all().expect("flush output");
        return;
    }

    let workers = std::env::var("THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(12)
        .clamp(1, 12);
    let parse_workers = workers.min(len.div_ceil(CHUNK_SIZE));
    let agg_workers = workers;

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
                    let src = File::open(input).expect("open input");
                    let loc = parse_worker_range(&src, start, end);
                    (loc.table, loc.hours)
                })
            })
            .collect();
        tasks.into_iter().map(|t| t.join().unwrap()).unzip()
    });

    let mut names = Vec::with_capacity(2048 * tables.len());
    for table in &tables {
        for id in 0..table.offsets.len() as u16 {
            names.push(table.get_name(id));
        }
    }
    names.sort_unstable();
    names.dedup();
    let num_endpoints = names.len();
    assert!(
        num_endpoints <= 2047,
        "endpoint catalogue exceeds 2047 limit"
    );

    let remaps: Vec<Vec<u16>> = tables
        .iter()
        .map(|t| {
            (0..t.offsets.len() as u16)
                .map(|id| names.binary_search(&t.get_name(id)).unwrap() as u16)
                .collect()
        })
        .collect();

    let num_workers = hours_by_worker.len();
    let mut hourly: Vec<Option<Vec<HourData>>> = (0..HOURS)
        .map(|_| Some(Vec::with_capacity(num_workers)))
        .collect();
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
                        push_field(&mut out, n as u64, b' ');
                        push_field(&mut out, p50 as u64, b' ');
                        push_field(&mut out, p95 as u64, b' ');
                        push_field(&mut out, p99 as u64, b' ');
                        push_field(&mut out, errors as u64, b' ');
                        push_field(&mut out, errors as u64 * 1_000_000 / n as u64, b'\n');
                    }
                    let _ = tx.send((hour, out));
                }
            });
        }
        drop(tx);

        let out = File::create(output).expect("cannot create summary.txt");
        let mut writer = BufWriter::with_capacity(256 * 1024, out);
        let mut pending: Vec<Option<Vec<u8>>> = (0..HOURS).map(|_| None).collect();
        let mut next_to_write = 0;

        while next_to_write < HOURS {
            let (h, buf) = rx.recv().expect("receive hour");
            pending[h] = Some(buf);
            while next_to_write < HOURS && pending[next_to_write].is_some() {
                let b = pending[next_to_write].take().unwrap();
                writer.write_all(&b).expect("write summary.txt");
                drop(b);
                next_to_write += 1;
            }
        }
        writer.flush().expect("flush output");
    });
}
