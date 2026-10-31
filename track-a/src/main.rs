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
const DATES: [&str; 4] = ["2026-10-30", "2026-10-31", "2026-11-01", "2026-11-02"];

struct Table {
    slots: Vec<u16>,
    names: Vec<Vec<u8>>,
}

impl Table {
    fn new() -> Self {
        Self {
            slots: vec![0u16; 4096],
            names: Vec::with_capacity(2048),
        }
    }

    #[inline(always)]
    fn hash(name: &[u8]) -> usize {
        let len = name.len();
        if len >= 8 {
            let first = u64::from_ne_bytes(name[..8].try_into().unwrap());
            let last = u64::from_ne_bytes(name[len - 8..].try_into().unwrap());
            let mut h = first ^ last.rotate_left(31) ^ (len as u64);
            h = (h ^ (h >> 33)).wrapping_mul(0xff51afd7ed558ccd);
            h as usize
        } else {
            let mut h = len as u64;
            for &b in name {
                h = (h ^ b as u64).wrapping_mul(0x100000001b3);
            }
            h as usize
        }
    }

    fn intern(&mut self, name: &[u8]) -> u16 {
        if (self.names.len() + 1) * 2 >= self.slots.len() {
            let mut next = vec![0u16; self.slots.len() * 2];
            let mask = next.len() - 1;
            for (id, n) in self.names.iter().enumerate() {
                let mut i = Self::hash(n) & mask;
                while next[i] != 0 {
                    i = (i + 1) & mask;
                }
                next[i] = (id + 1) as u16;
            }
            self.slots = next;
        }
        let mask = self.slots.len() - 1;
        let mut i = Self::hash(name) & mask;
        loop {
            let slot = self.slots[i];
            if slot == 0 {
                let id = self.names.len();
                self.names.push(name.to_vec());
                self.slots[i] = (id + 1) as u16;
                return id as u16;
            }
            if self.names[(slot - 1) as usize] == name {
                return slot - 1;
            }
            i = (i + 1) & mask;
        }
    }
}

struct HourData {
    lats: Vec<u32>,
    id_errs: Vec<u16>,
}

struct Local {
    table: Table,
    hours: Vec<HourData>,
}

#[inline(always)]
fn parse_line(buf: &[u8], pos: usize, local: &mut Local) -> usize {
    let day = (buf[pos + 8] - b'0') as usize * 10 + (buf[pos + 9] - b'0') as usize;
    let day_idx = if buf[pos + 6] == b'0' {
        day - 30
    } else {
        1 + day
    };
    let hour = day_idx * 24 + ((buf[pos + 11] - b'0') * 10 + buf[pos + 12] - b'0') as usize - 18;

    let mut i = pos + 28;
    while buf[i] != b' ' {
        i += 1;
    }
    let ep_start = i + 1;
    i = ep_start;
    while buf[i] != b' ' {
        i += 1;
    }
    let ep_end = i;
    let err = (buf[ep_end + 1] == b'5') as u16;
    let mut latency = 0u32;
    i = ep_end + 5;
    while buf[i] != b'\n' {
        latency = latency * 10 + (buf[i] - b'0') as u32;
        i += 1;
    }
    let id = local.table.intern(&buf[ep_start..ep_end]);
    let hd = &mut local.hours[hour];
    hd.lats.push(latency);
    // Spec §2: catalogue has exactly 1,842 endpoints (<= 32,767); fits in 15 bits, bit 15 is 5xx flag.
    hd.id_errs.push((id & 0x7FFF) | (err << 15));
    i + 1
}

fn parse_worker_range(file: &File, mut start: usize, end: usize) -> Local {
    let mut local = Local {
        table: Table::new(),
        hours: (0..HOURS)
            .map(|_| HourData {
                lats: Vec::with_capacity(4096),
                id_errs: Vec::with_capacity(4096),
            })
            .collect(),
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
fn percentile_values(latencies: &mut [u32]) -> (u32, u32, u32) {
    let n = latencies.len();
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

    let default_workers = 12usize;
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
        names.extend_from_slice(&local.table.names);
    }
    names.sort_unstable();
    names.dedup();
    let num_endpoints = names.len();

    let remaps: Vec<Vec<u16>> = locals
        .iter()
        .map(|l| {
            l.table
                .names
                .iter()
                .map(|n| names.binary_search(n).unwrap() as u16)
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
                let mut flat_lats: Vec<u32> = Vec::with_capacity(65536);
                let mut counts = vec![0u32; num_endpoints + 1];
                let mut offsets = vec![0u32; num_endpoints + 1];
                let mut err_counts = vec![0u32; num_endpoints];
                let mut cursors = vec![0u32; num_endpoints];

                loop {
                    let hour = next.fetch_add(1, Ordering::Relaxed);
                    if hour >= HOURS {
                        break;
                    }
                    counts.fill(0);
                    err_counts.fill(0);

                    for (w, local) in locals.iter().enumerate() {
                        let w_remap = &remaps[w];
                        for &r in &local.hours[hour].id_errs {
                            let gid = w_remap[(r & 0x7FFF) as usize] as usize;
                            counts[gid] += 1;
                            if (r & 0x8000) != 0 {
                                err_counts[gid] += 1;
                            }
                        }
                    }

                    let mut sum = 0u32;
                    for i in 0..num_endpoints {
                        offsets[i] = sum;
                        cursors[i] = sum;
                        sum += counts[i];
                    }
                    offsets[num_endpoints] = sum;
                    if flat_lats.len() < sum as usize {
                        flat_lats.resize(sum as usize, 0);
                    }

                    for (w, local) in locals.iter().enumerate() {
                        let w_remap = &remaps[w];
                        let hd = &local.hours[hour];
                        for i in 0..hd.lats.len() {
                            let r = hd.id_errs[i];
                            let gid = w_remap[(r & 0x7FFF) as usize] as usize;
                            let slot = cursors[gid] as usize;
                            cursors[gid] += 1;
                            flat_lats[slot] = hd.lats[i];
                        }
                    }

                    let mut out = Vec::with_capacity(32 * 1024);
                    let (h, d) = ((hour + 18) % 24, (hour + 18) / 24);
                    let date_str = DATES[d];
                    for ep in 0..num_endpoints {
                        let start = offsets[ep] as usize;
                        let end = offsets[ep + 1] as usize;
                        let n = end - start;
                        if n == 0 {
                            continue;
                        }
                        let ep_lats = &mut flat_lats[start..end];
                        let (p50, p95, p99) = percentile_values(ep_lats);
                        let errors = err_counts[ep];
                        let ep_name = std::str::from_utf8(&names[ep]).unwrap();
                        let _ = write!(
                            out,
                            "{date_str}T{h:02}:00:00Z {ep_name} {n} {p50} {p95} {p99} {errors} {}\n",
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

    let out = File::create(output).expect("cannot create summary.txt");
    let mut writer = BufWriter::with_capacity(256 * 1024, out);
    for h in 0..HOURS {
        writer
            .write_all(&hour_outputs[h])
            .expect("write summary.txt");
    }
    writer.flush().expect("flush output");
    drop(writer);
}
