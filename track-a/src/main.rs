// Track A (Ingest): parallelized solution (v1) in safe standard-library Rust.
// Implements the HTTP request aggregation contract in specs/a-ingest.md.
// Safe Rust, std only, no unsafe, no SIMD, no mmap (GR-P2..P10, PLAN §6, §7.1).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::os::unix::fs::FileExt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

const HOURS: usize = 72;
const CHUNK_SIZE: usize = 64 * 1024;

struct Table {
    slots: Vec<usize>,
    names: Vec<Vec<u8>>,
}

impl Table {
    fn new() -> Self {
        Self {
            slots: vec![0; 4096],
            names: Vec::with_capacity(2048),
        }
    }

    fn hash(name: &[u8]) -> u64 {
        let mut h = 0x9e3779b97f4a7c15u64 ^ name.len() as u64;
        for part in name.chunks(8) {
            let mut word = [0u8; 8];
            word[..part.len()].copy_from_slice(part);
            let x = u64::from_le_bytes(word).wrapping_add(0x9e3779b97f4a7c15);
            h ^= (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            h = h.rotate_left(27).wrapping_mul(5).wrapping_add(0x52dce729);
        }
        h ^ (h >> 33)
    }

    fn intern(&mut self, name: &[u8]) -> u16 {
        if (self.names.len() + 1) * 10 >= self.slots.len() * 7 {
            let mut next = vec![0; self.slots.len() * 2];
            let mask = next.len() - 1;
            for (id, n) in self.names.iter().enumerate() {
                let mut i = Self::hash(n) as usize & mask;
                while next[i] != 0 {
                    i = (i + 1) & mask;
                }
                next[i] = id + 1;
            }
            self.slots = next;
        }
        let mask = self.slots.len() - 1;
        let mut i = Self::hash(name) as usize & mask;
        loop {
            let slot = self.slots[i];
            if slot == 0 {
                let id = self.names.len();
                self.names.push(name.to_vec());
                self.slots[i] = id + 1;
                return id as u16;
            }
            if self.names[slot - 1].as_slice() == name {
                return (slot - 1) as u16;
            }
            i = (i + 1) & mask;
        }
    }
}

#[derive(Clone, Copy)]
struct Record {
    latency: u32,
    id_err: u16,
}

struct Local {
    table: Table,
    hours: Vec<Vec<Record>>,
}

#[inline(always)]
fn hour_index(line: &[u8]) -> usize {
    let day_of_month = (line[8] - b'0') as usize * 10 + (line[9] - b'0') as usize;
    let day_idx = if line[6] == b'0' {
        day_of_month - 30
    } else {
        1 + day_of_month
    };
    let hour = ((line[11] - b'0') * 10 + line[12] - b'0') as usize;
    day_idx * 24 + hour - 18
}

#[inline(always)]
fn parse_line(line: &[u8], local: &mut Local) {
    let method_end = 28 + line[28..].iter().position(|&b| b == b' ').unwrap();
    let ep_start = method_end + 1;
    let ep_end = ep_start + line[ep_start..].iter().position(|&b| b == b' ').unwrap();
    let mut latency = 0u32;
    for &d in &line[ep_end + 5..] {
        latency = latency * 10 + (d - b'0') as u32;
    }
    let hour = hour_index(line);
    let id = local.table.intern(&line[ep_start..ep_end]);
    let err = (line[ep_end + 1] == b'5') as u16;
    local.hours[hour].push(Record {
        latency,
        id_err: (id & 0x7FFF) | (err << 15),
    });
}

fn parse_worker_range(file: &File, mut start: usize, end: usize) -> Local {
    let mut local = Local {
        table: Table::new(),
        hours: (0..HOURS).map(|_| Vec::with_capacity(1024)).collect(),
    };
    let mut buf = [0u8; CHUNK_SIZE];
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
        while pos <= last_nl {
            let nl = pos + buf[pos..=last_nl].iter().position(|&b| b == b'\n').unwrap();
            parse_line(&buf[pos..nl], &mut local);
            pos = nl + 1;
        }
        buf.copy_within(last_nl + 1..valid, 0);
        carried = valid - (last_nl + 1);
        start += n;
    }
    local
}

fn hour_text(index: usize, out: &mut Vec<u8>) {
    const DATES: [[u8; 10]; 4] = [
        *b"2026-10-30",
        *b"2026-10-31",
        *b"2026-11-01",
        *b"2026-11-02",
    ];
    out.extend_from_slice(&DATES[(index + 18) / 24]);
    out.push(b'T');
    let hour = (index + 18) % 24;
    out.push(b'0' + (hour / 10) as u8);
    out.push(b'0' + (hour % 10) as u8);
    out.extend_from_slice(b":00:00Z ");
}

#[inline(always)]
fn percentile_values(latencies: &mut [u32]) -> (u32, u32, u32) {
    let n = latencies.len();
    if n == 1 {
        return (latencies[0], latencies[0], latencies[0]);
    }
    let a = (50 * n + 99) / 100 - 1;
    let b = (95 * n + 99) / 100 - 1;
    let c = (99 * n + 99) / 100 - 1;
    latencies.sort_unstable();
    (latencies[a], latencies[b], latencies[c])
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input = args.get(1).map(String::as_str).unwrap_or("/data/requests.log");
    let output = args.get(2).map(String::as_str).unwrap_or("/out/summary.txt");
    let file = File::open(input).expect("cannot open requests.log");
    let len = file.metadata().expect("cannot stat requests.log").len() as usize;
    if len == 0 {
        let out = File::create(output).expect("cannot create summary.txt");
        out.sync_all().expect("flush output");
        return;
    }

    let default_workers = 6usize;
    let requested_workers = std::env::var("THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default_workers)
        .clamp(1, 12);
    let workers = requested_workers.min(len.div_ceil(CHUNK_SIZE)).max(1);

    let mut bounds = vec![0; workers + 1];
    let mut buf = [0u8; 1024];
    for w in 1..workers {
        let mut pos = len * w / workers;
        while let Ok(n) = file.read_at(&mut buf[..1024.min(len - pos)], pos as u64) {
            if let Some(i) = buf[..n].iter().position(|&b| b == b'\n') {
                pos += i + 1;
                break;
            }
            pos += n;
        }
        bounds[w] = pos;
    }
    bounds[workers] = len;
    bounds.dedup();

    let locals: Vec<Local> = thread::scope(|scope| {
        let mut tasks = Vec::new();
        for pair in bounds.windows(2) {
            let (start, end) = (pair[0], pair[1]);
            tasks.push(scope.spawn(move || {
                let source = File::open(input).expect("reopen requests.log");
                parse_worker_range(&source, start, end)
            }));
        }
        tasks
            .into_iter()
            .map(|t| t.join().expect("parser worker panicked"))
            .collect()
    });

    let mut names = Vec::new();
    for local in &locals {
        names.extend(local.table.names.iter().cloned());
    }
    names.sort_unstable();
    names.dedup();
    let num_endpoints = names.len();

    let remaps: Vec<Vec<u16>> = locals
        .iter()
        .map(|local| {
            local
                .table
                .names
                .iter()
                .map(|name| names.binary_search(name).unwrap() as u16)
                .collect()
        })
        .collect();

    let next_hour = AtomicUsize::new(0);
    let names = Arc::new(names);
    let locals = Arc::new(locals);
    let remaps = Arc::new(remaps);
    let mut hour_outputs: Vec<Vec<u8>> = (0..HOURS).map(|_| Vec::new()).collect();

    let thread_results: Vec<Vec<(usize, Vec<u8>)>> = thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..workers {
            let next = &next_hour;
            let names = Arc::clone(&names);
            let locals = Arc::clone(&locals);
            let remaps = Arc::clone(&remaps);
            handles.push(scope.spawn(move || {
                let mut my_hours = Vec::new();
                let mut hour_lats: Vec<Vec<u32>> = (0..num_endpoints).map(|_| Vec::new()).collect();
                let mut hour_errs = vec![0u32; num_endpoints];

                loop {
                    let hour = next.fetch_add(1, Ordering::Relaxed);
                    if hour >= HOURS {
                        break;
                    }
                    for lats in hour_lats.iter_mut() {
                        lats.clear();
                    }
                    hour_errs.fill(0);

                    for (w, local) in locals.iter().enumerate() {
                        let w_remap = &remaps[w];
                        for r in &local.hours[hour] {
                            let gid = w_remap[(r.id_err & 0x7FFF) as usize] as usize;
                            hour_lats[gid].push(r.latency);
                            if (r.id_err & 0x8000) != 0 {
                                hour_errs[gid] += 1;
                            }
                        }
                    }

                    let mut out = Vec::with_capacity(32 * 1024);
                    for ep in 0..num_endpoints {
                        let lats = &mut hour_lats[ep];
                        let n = lats.len();
                        if n == 0 {
                            continue;
                        }
                        let (p50, p95, p99) = percentile_values(lats);
                        let errors = hour_errs[ep];
                        hour_text(hour, &mut out);
                        out.extend_from_slice(&names[ep]);
                        let _ = write!(
                            out,
                            " {} {} {} {} {} {}\n",
                            n,
                            p50,
                            p95,
                            p99,
                            errors,
                            errors as u64 * 1_000_000 / n as u64
                        );
                    }
                    my_hours.push((hour, out));
                }
                my_hours
            }));
        }
        handles
            .into_iter()
            .map(|h| h.join().expect("formatter worker panicked"))
            .collect()
    });

    for my_hours in thread_results {
        for (hour, out) in my_hours {
            hour_outputs[hour] = out;
        }
    }
    let output_len: usize = hour_outputs.iter().map(Vec::len).sum();
    let mut output_bytes = Vec::with_capacity(output_len);
    for part in hour_outputs {
        output_bytes.extend_from_slice(&part);
    }

    let out = File::create(output).expect("cannot create summary.txt");
    let mut writer = BufWriter::with_capacity(256 * 1024, out);
    writer.write_all(&output_bytes).expect("write summary.txt");
    writer.flush().expect("flush output");
    drop(writer);
}
