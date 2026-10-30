// Track A (Ingest): single-threaded baseline (v0) in safe standard-library Rust.
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
    // Integer arithmetic for ceil(p * n / 100) per Spec §3 item 4 and GR-C2
    let rank = (p * n + 99) / 100;
    // 0-indexed offset: 1 <= rank <= n guaranteed for p in 1..=100 and n >= 1
    let idx = (rank - 1) as usize;
    // Index within bounds guaranteed by 0 <= idx < n
    sorted_latencies[idx]
}

/// Parse ASCII decimal digits into u32 without allocation.
fn parse_u32(bytes: &[u8]) -> u32 {
    let mut val: u32 = 0;
    for &b in bytes {
        if b.is_ascii_digit() {
            val = val * 10 + (b - b'0') as u32;
        }
    }
    val
}

/// Parse ASCII decimal digits into u64 without allocation (prevents intermediate overflow).
fn parse_u64(bytes: &[u8]) -> u64 {
    let mut val: u64 = 0;
    for &b in bytes {
        if b.is_ascii_digit() {
            val = val * 10 + (b - b'0') as u64;
        }
    }
    val
}

fn main() {
    // Contract paths: /data/requests.log -> /out/summary.txt (spec §2, §4).
    // CLI arguments allow local host testing without container mount when arguments are passed.
    let args: Vec<String> = std::env::args().collect();
    let input_path = if args.len() > 1 {
        // Safe index: args.len() > 1 verified
        &args[1]
    } else {
        "/data/requests.log"
    };
    let output_path = if args.len() > 2 {
        // Safe index: args.len() > 2 verified
        &args[2]
    } else {
        "/out/summary.txt"
    };

    // Open input log file; handle realistic failure mode with clear diagnostic (PLAN §6 item 9)
    let file = match File::open(input_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open input file {}: {}", input_path, e);
            std::process::exit(1);
        }
    };

    // Use 256 KiB buffered reader to minimize syscall overhead (PLAN §6 item 2, §7.1 v0)
    let mut reader = BufReader::with_capacity(256 * 1024, file);
    let mut line_buf = Vec::with_capacity(512);

    // Pre-generate the 72 fixed hour strings (Spec §2: 2026-10-30T18:00:00Z to 2026-11-02T17:00:00Z)
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
                // Exhaustive over 0..NUM_HOURS since (71 + 18) / 24 = 3
                _ => unreachable!("day_offset must be in 0..=3 for the 72-hour span"),
            };
            format!("2026-{:02}-{:02}T{:02}:00:00Z", month, day, hour)
        })
        .collect();

    // Dense mapping: endpoint bytes -> dense usize ID (PLAN §7.1 v0)
    let mut endpoint_to_id: HashMap<Vec<u8>, usize> = HashMap::new();
    let mut endpoints: Vec<Vec<u8>> = Vec::new();

    // 2D grid: groups[hour_idx][endpoint_id] gives O(1) indexed access per request line
    let mut groups: Vec<Vec<Group>> = (0..NUM_HOURS).map(|_| Vec::new()).collect();

    // Ingest loop: process line-by-line reusing line_buf to eliminate per-line heap allocation
    loop {
        line_buf.clear();
        // Read until LF; safe I/O expect with explicit contract message
        let bytes_read = reader
            .read_until(b'\n', &mut line_buf)
            .expect("I/O error reading /data/requests.log");
        if bytes_read == 0 {
            // EOF reached (Spec §3 item 7)
            break;
        }

        let mut raw_line = &line_buf[..];
        if raw_line.ends_with(b"\n") {
            // Safe slice: ends_with verified non-empty and ending with LF
            raw_line = &raw_line[..raw_line.len() - 1];
        }
        if raw_line.ends_with(b"\r") {
            // Safe slice: ends_with verified non-empty and ending with CR
            raw_line = &raw_line[..raw_line.len() - 1];
        }
        if raw_line.is_empty() {
            continue;
        }

        // Split line into fields: ts SP method SP endpoint SP status SP latency (Spec §2 grammar)
        let mut parts = raw_line.split(|&b| b == b' ');
        // Spec §2 guarantees all 5 fields are present on each well-formed line
        let ts = parts.next().expect("malformed line: missing timestamp");
        let _method = parts.next().expect("malformed line: missing method");
        let endpoint = parts.next().expect("malformed line: missing endpoint");
        let status = parts.next().expect("malformed line: missing status");
        let latency = parts.next().expect("malformed line: missing latency");

        // Spec §2 guarantees ts is exactly 27 bytes: YYYY-MM-DDTHH:MM:SS.uuuuuuZ
        if ts.len() != 27 {
            panic!("timestamp field must be exactly 27 bytes per spec §2");
        }

        // Parse month, day, hour directly from ASCII digits per Spec §2 calendar guarantee
        // Safe indexing: ts.len() == 27 verified above
        let month = (ts[5] - b'0') as usize * 10 + (ts[6] - b'0') as usize;
        let day = (ts[8] - b'0') as usize * 10 + (ts[9] - b'0') as usize;
        let hour = (ts[11] - b'0') as usize * 10 + (ts[12] - b'0') as usize;

        let day_offset = match (month, day) {
            (10, 30) => 0,
            (10, 31) => 1,
            (11, 1) => 2,
            (11, 2) => 3,
            _ => panic!("timestamp date outside guaranteed 72-hour window (spec §2)"),
        };

        let hour_offset = day_offset * 24 + hour;
        // Spec §2 guarantees hour_offset is within 18..=89 (72 hours total)
        let hour_idx = hour_offset
            .checked_sub(18)
            .expect("timestamp hour before 2026-10-30T18:00:00Z");
        if hour_idx >= NUM_HOURS {
            panic!("timestamp hour after 72-hour window (spec §2)");
        }

        // Dense endpoint mapping: borrows &[u8] for lookup, allocates Vec only on first sighting
        let endpoint_id = match endpoint_to_id.get(endpoint) {
            Some(&id) => id,
            None => {
                let id = endpoints.len();
                endpoints.push(endpoint.to_vec());
                endpoint_to_id.insert(endpoint.to_vec(), id);
                // Maintain parallel groups for all 72 hours for the new endpoint
                for h in 0..NUM_HOURS {
                    // Safe index: h < NUM_HOURS guaranteed by loop range
                    groups[h].push(Group {
                        latencies: Vec::new(),
                        errors: 0,
                    });
                }
                id
            }
        };

        // Parse status: Spec §2 guarantees 3 digits (100..599); Spec §3 item 5 defines 500..=599 as errors
        let status_num = parse_u32(status);
        let is_error = status_num >= 500 && status_num <= 599;

        // Parse latency: Spec §2 guarantees unsigned 32-bit decimal microseconds (0..=4294967295)
        let lat = parse_u64(latency) as u32;

        // Record latency and error into the dense group slot
        // Safe indexing: hour_idx < NUM_HOURS and endpoint_id < endpoints.len() guaranteed above
        let grp = &mut groups[hour_idx][endpoint_id];
        grp.latencies.push(lat);
        if is_error {
            grp.errors += 1;
        }
    }

    // Sort endpoints lexicographically by unsigned byte order once (Spec §4: prefix first)
    let mut sorted_endpoint_ids: Vec<usize> = (0..endpoints.len()).collect();
    sorted_endpoint_ids.sort_unstable_by(|&a, &b| {
        // Safe indexing: a, b < endpoints.len() guaranteed by (0..endpoints.len())
        endpoints[a].cmp(&endpoints[b])
    });

    // Build the complete output in memory buffer (PLAN §6 item 3)
    let mut out_buf: Vec<u8> = Vec::with_capacity(12 * 1024 * 1024);

    // Emit groups hour-major, then endpoint bytes ascending (Spec §4 total ordering)
    for hour_idx in 0..NUM_HOURS {
        // Safe index: hour_idx < NUM_HOURS guaranteed by loop range
        let hour_str = &hour_strings[hour_idx];
        for &ep_id in &sorted_endpoint_ids {
            // Safe indexing: hour_idx < NUM_HOURS and ep_id < endpoints.len() guaranteed
            let grp = &mut groups[hour_idx][ep_id];
            let count = grp.latencies.len();
            if count == 0 {
                // Spec §3 item 7: empty groups omitted from output
                continue;
            }

            // Spec §3 item 4: nearest rank requires sorting latencies ascending, keeping duplicates
            grp.latencies.sort_unstable();

            let p50 = percentile(&grp.latencies, 50);
            let p95 = percentile(&grp.latencies, 95);
            let p99 = percentile(&grp.latencies, 99);

            let errors = grp.errors;
            // Spec §3 item 6: error_ppm = floor(errors * 1_000_000 / count) via integer arithmetic
            let error_ppm = (errors as u64 * 1_000_000 / count as u64) as u32;

            // Safe index: ep_id < endpoints.len() guaranteed by sorted_endpoint_ids construction
            let ep_bytes = &endpoints[ep_id];

            // Append line: hour endpoint count p50 p95 p99 errors error_ppm\n (Spec §4 format)
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

    // Write output file once, flush, and close (Spec §4, PLAN §6 item 3, GR-C8)
    let mut out_file = File::create(output_path).expect("cannot create /out/summary.txt");
    out_file
        .write_all(&out_buf)
        .expect("cannot write to /out/summary.txt");
    out_file.flush().expect("cannot flush /out/summary.txt");
    drop(out_file);
}
