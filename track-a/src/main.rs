// Track A (Ingest): single-threaded baseline (v0) with fixed-offset parsing (A1).
// Implements the contract defined in specs/a-ingest.md (hackathon-raptors/speed-demon-2026).
// No threads, no unsafe, no SIMD, no mmap, no custom hashers (GR-P2, PLAN §6, §7.1, §11).

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};

/// Per-(hour, endpoint) statistics accumulator.
struct Group {
    latencies: Vec<u32>,
    errors: u32,
}

/// Compute the nearest-rank percentile on an ascending-sorted slice of latencies.
/// Spec §3 item 4: 1-indexed rank = ceil(p * n / 100) = (p * n + 99) / 100.
fn percentile(sorted_latencies: &[u32], p: u64) -> u32 {
    let n = sorted_latencies.len() as u64;
    let rank = (p * n + 99) / 100;
    let idx = (rank - 1) as usize;
    sorted_latencies[idx]
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input_path = if args.len() > 1 {
        &args[1]
    } else {
        "/data/requests.log"
    };
    let output_path = if args.len() > 2 {
        &args[2]
    } else {
        "/out/summary.txt"
    };

    let file = match File::open(input_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open input file {}: {}", input_path, e);
            std::process::exit(1);
        }
    };

    let mut reader = BufReader::with_capacity(256 * 1024, file);
    let mut line_buf = Vec::with_capacity(512);

    const NUM_HOURS: usize = 72;
    let hour_strings: Vec<String> = (0..NUM_HOURS)
        .map(|idx| {
            let total_hour = idx + 18;
            let day_offset = total_hour / 24;
            let hour = total_hour % 24;
            let (month, day) = match day_offset {
                0 => (10, 30),
                1 => (10, 31),
                2 => (11, 1),
                3 => (11, 2),
                _ => unreachable!("day_offset must be in 0..=3 for the 72-hour span"),
            };
            format!("2026-{:02}-{:02}T{:02}:00:00Z", month, day, hour)
        })
        .collect();

    let mut endpoint_to_id: HashMap<Vec<u8>, usize> = HashMap::new();
    let mut endpoints: Vec<Vec<u8>> = Vec::new();
    let mut groups: Vec<Vec<Group>> = (0..NUM_HOURS).map(|_| Vec::new()).collect();

    loop {
        line_buf.clear();
        let bytes_read = reader
            .read_until(b'\n', &mut line_buf)
            .expect("I/O error reading /data/requests.log");
        if bytes_read == 0 {
            break;
        }

        let mut line = &line_buf[..];
        if line.ends_with(b"\n") {
            line = &line[..line.len() - 1];
        }
        if line.ends_with(b"\r") {
            line = &line[..line.len() - 1];
        }
        if line.is_empty() {
            continue;
        }

        let hour_idx = hour_index(line);
        let method_end = 28 + line[28..].iter().position(|&b| b == b' ').unwrap();
        let ep_start = method_end + 1;
        let ep_end = ep_start + line[ep_start..].iter().position(|&b| b == b' ').unwrap();
        let endpoint = &line[ep_start..ep_end];

        let mut latency = 0u32;
        for &d in &line[ep_end + 5..] {
            latency = latency * 10 + (d - b'0') as u32;
        }
        let is_error = line[ep_end + 1] == b'5';

        let endpoint_id = match endpoint_to_id.get(endpoint) {
            Some(&id) => id,
            None => {
                let id = endpoints.len();
                endpoints.push(endpoint.to_vec());
                endpoint_to_id.insert(endpoint.to_vec(), id);
                for h in 0..NUM_HOURS {
                    groups[h].push(Group {
                        latencies: Vec::new(),
                        errors: 0,
                    });
                }
                id
            }
        };

        let grp = &mut groups[hour_idx][endpoint_id];
        grp.latencies.push(latency);
        if is_error {
            grp.errors += 1;
        }
    }

    let mut sorted_endpoint_ids: Vec<usize> = (0..endpoints.len()).collect();
    sorted_endpoint_ids.sort_unstable_by(|&a, &b| endpoints[a].cmp(&endpoints[b]));

    let mut out_buf: Vec<u8> = Vec::with_capacity(12 * 1024 * 1024);

    for hour_idx in 0..NUM_HOURS {
        let hour_str = &hour_strings[hour_idx];
        for &ep_id in &sorted_endpoint_ids {
            let grp = &mut groups[hour_idx][ep_id];
            let count = grp.latencies.len();
            if count == 0 {
                continue;
            }

            grp.latencies.sort_unstable();

            let p50 = percentile(&grp.latencies, 50);
            let p95 = percentile(&grp.latencies, 95);
            let p99 = percentile(&grp.latencies, 99);

            let errors = grp.errors;
            let error_ppm = (errors as u64 * 1_000_000 / count as u64) as u32;
            let ep_bytes = &endpoints[ep_id];

            write!(out_buf, "{} ", hour_str).expect("in-memory write to Vec<u8> cannot fail");
            out_buf.extend_from_slice(ep_bytes);
            writeln!(
                out_buf,
                " {} {} {} {} {} {}",
                count, p50, p95, p99, errors, error_ppm
            )
            .expect("in-memory write to Vec<u8> cannot fail");
        }
    }

    let mut out_file = File::create(output_path).expect("cannot create /out/summary.txt");
    out_file
        .write_all(&out_buf)
        .expect("cannot write to /out/summary.txt");
    out_file.flush().expect("cannot flush /out/summary.txt");
    drop(out_file);
}
