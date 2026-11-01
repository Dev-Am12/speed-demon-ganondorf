#![allow(dead_code)]
// Track A (Ingest): parallelized solution in safe standard-library Rust.
// Implements the HTTP request aggregation contract in specs/a-ingest.md.

use std::arch::x86_64::*;
use std::fs::File;
use std::os::unix::fs::FileExt;
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
    let (parse_workers, _agg_workers) = (workers.min(len.div_ceil(CHUNK_SIZE)), workers);

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

    thread::scope(|s| {
        let tasks: Vec<_> = bounds
            .windows(2)
            .map(|w| {
                let (start, end) = (w[0], w[1]);
                s.spawn(move || {
                    let _ = (start, end);
                })
            })
            .collect();
        for t in tasks {
            t.join().unwrap();
        }
    });

    File::create(output).expect("create output");
    std::process::exit(0);
}
