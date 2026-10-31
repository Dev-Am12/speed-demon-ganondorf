// Track A (Ingest): parallelized solution (v1) in safe standard-library Rust.
// Implements the HTTP request aggregation contract in specs/a-ingest.md.
// Safe Rust, std only, no unsafe, no SIMD, no mmap (GR-P2..P10, PLAN §6, §7.1).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::os::unix::fs::FileExt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;

const HOURS: usize = 72;
const CHUNK_SIZE: usize = 64 * 1024;
const PARSE_WORKERS: usize = 12;
const AGG_WORKERS: usize = 12;
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
            let first = u64::from_le_bytes(name[0..8].try_into().unwrap());
            let last = u64::from_le_bytes(name[len - 8..len].try_into().unwrap());
            let h = first.wrapping_mul(0x517cc1b727220a95) ^ last.rotate_left(17) ^ (len as u64);
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
        let mut i = Self::hash(name) & 4095;
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
            i = (i + 1) & 4095;
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
fn find_space_swar(buf: &[u8], mut i: usize) -> usize {
    loop {
        if i + 8 <= buf.len() {
            let w = u64::from_le_bytes(buf[i..i + 8].try_into().unwrap());
            let x = w ^ 0x2020202020202020;
            let m = x.wrapping_sub(0x0101010101010101) & !x & 0x8080808080808080;
            if m != 0 {
                return i + (m.trailing_zeros() as usize >> 3);
            }
            i += 8;
        } else {
            while buf[i] != b' ' {
                i += 1;
            }
            return i;
        }
    }
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
    let ep_end = find_space_swar(buf, ep_start);
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

    let parse_workers = std::env::var("THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(PARSE_WORKERS)
        .min(len.div_ceil(CHUNK_SIZE))
        .clamp(1, 12);
    let agg_workers = std::env::var("THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(AGG_WORKERS)
        .clamp(1, 12);

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
            let locals = &locals;
            let remaps = &remaps;
            let prefixes = &prefixes;
            let tx = tx.clone();
            scope.spawn(move || {
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

                    let mut out = Vec::with_capacity(160 * 1024);
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
                        out.extend_from_slice(&prefixes[hour]);
                        out.extend_from_slice(&names[ep]);
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
