// Track D (Traverse): single-threaded baseline (v0) in safe standard-library Rust.
// Implements the shortest path contract defined in specs/d-traverse.md.
// No threads, no unsafe, no SIMD, no mmap, no custom hashers, no renumbering, no bidirectional search (GR-P2, PLAN §6, §7.3, §11).

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};

/// Parse the next unsigned decimal integer from line bytes starting at pos.
fn parse_u32_field(line: &[u8], pos: &mut usize) -> u32 {
    // Skip spaces and trailing line delimiters per Spec §2 ASCII formatting
    while *pos < line.len() && (line[*pos] == b' ' || line[*pos] == b'\r' || line[*pos] == b'\n') {
        *pos += 1;
    }
    let mut val: u64 = 0;
    // Accumulate digits into u64 to prevent intermediate overflow before u32 truncation
    while *pos < line.len() && line[*pos].is_ascii_digit() {
        val = val * 10 + (line[*pos] - b'0') as u64;
        *pos += 1;
    }
    val as u32
}

/// Run single-source Dijkstra search from s to t using forward CSR graph.
/// Spec §3: Returns shortest directed distance in u64, or -1 if unreachable.
fn shortest_path(
    offsets: &[usize],
    targets: &[u32],
    weights: &[u32],
    s: u32,
    t: u32,
    dist: &mut [u64],
    touched: &mut Vec<u32>,
    heap: &mut BinaryHeap<Reverse<(u64, u32)>>,
) -> i64 {
    // Spec §3: d(s, s) = 0 for every node, including isolated nodes and nodes with self-loops
    if s == t {
        return 0;
    }

    // Initialize source node distance to 0
    // Safe index: s < n guaranteed by Spec §2 query constraints
    dist[s as usize] = 0;
    touched.push(s);
    // Heap tracks (cost, node); cost is u64 per Spec §3 (exceeds 2^32), node is u32
    heap.push(Reverse((0, s)));

    let mut answer: i64 = -1;

    while let Some(Reverse((d, u))) = heap.pop() {
        // Safe index: u < n guaranteed by edge target bounds
        if d > dist[u as usize] {
            // Stale heap entry: u was already settled via a shorter path
            continue;
        }

        // Spec §3 & §7: early exit as soon as target node t is settled
        if u == t {
            answer = d as i64;
            break;
        }

        // Relax outgoing edges from node u using CSR slices
        // Safe indices: u and u + 1 within bounds of offsets array of size n + 1
        let start = offsets[u as usize];
        let end = offsets[u as usize + 1];

        for i in start..end {
            // Safe indices: i < m guaranteed by CSR construction
            let v = targets[i];
            let w = weights[i];
            let next_d = d + w as u64;

            // Safe index: v < n guaranteed by Spec §2 edge head bounds
            if next_d < dist[v as usize] {
                if dist[v as usize] == u64::MAX {
                    // Record touched node for O(visited) cleanup at end of query
                    touched.push(v);
                }
                dist[v as usize] = next_d;
                heap.push(Reverse((next_d, v)));
            }
        }
    }

    // Reset distance state for all touched nodes in O(visited) without O(n) array sweep
    for &node in touched.iter() {
        // Safe index: node < n guaranteed by touched node origin
        dist[node as usize] = u64::MAX;
    }
    touched.clear();
    heap.clear();

    answer
}

fn main() {
    // Resolve file paths: default to standard container paths /data and /out
    let args: Vec<String> = std::env::args().collect();
    let data_dir = if args.len() > 1 {
        // Safe index: args.len() > 1 verified
        &args[1]
    } else {
        "/data"
    };
    let out_dir = if args.len() > 2 {
        // Safe index: args.len() > 2 verified
        &args[2]
    } else {
        "/out"
    };

    let graph_path = format!("{}/graph.txt", data_dir);
    let queries_path = format!("{}/queries.txt", data_dir);
    let output_path = format!("{}/distances.txt", out_dir);

    // Open graph file for first pass; handle realistic failure mode with clear diagnostic (PLAN §6 item 9)
    let file = match File::open(&graph_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open graph file {}: {}", graph_path, e);
            std::process::exit(1);
        }
    };

    // Use 256 KiB buffered reader to stream graph without full-file allocation (PLAN §6 item 2, GR-P5)
    let mut reader = BufReader::with_capacity(256 * 1024, file);
    let mut line_buf = Vec::with_capacity(256);

    // Read header line: n SP m LF (Spec §2)
    line_buf.clear();
    reader
        .read_until(b'\n', &mut line_buf)
        .expect("failed to read graph.txt header");
    let mut pos = 0;
    let n = parse_u32_field(&line_buf, &mut pos) as usize;
    let m = parse_u32_field(&line_buf, &mut pos) as usize;

    // Pass 1: count outgoing degree per node into CSR offsets array of size n + 1
    let mut offsets = vec![0usize; n + 1];

    for _ in 0..m {
        line_buf.clear();
        let bytes_read = reader
            .read_until(b'\n', &mut line_buf)
            .expect("I/O error reading edge in graph.txt pass 1");
        if bytes_read == 0 {
            break;
        }
        let mut p = 0;
        let u = parse_u32_field(&line_buf, &mut p) as usize;
        // Safe index: u < n guaranteed by Spec §2; u + 1 < n + 1
        offsets[u + 1] += 1;
    }

    // Prefix sum over offsets: offsets[i] becomes starting index of node i's edges
    for i in 0..n {
        // Safe index: i + 1 < offsets.len() guaranteed by loop boundary
        offsets[i + 1] += offsets[i];
    }

    // Allocate CSR target and weight arrays of exact size m
    let mut targets = vec![0u32; m];
    let mut weights = vec![0u32; m];
    // Working cursor for edge slot placement
    let mut cur_offsets = offsets.clone();

    // Reopen graph file for pass 2 (cached in OS page cache; avoids storing raw edge tuples)
    let file2 = File::open(&graph_path).expect("failed to reopen graph.txt for pass 2");
    let mut reader2 = BufReader::with_capacity(256 * 1024, file2);

    // Skip header line in pass 2
    line_buf.clear();
    reader2
        .read_until(b'\n', &mut line_buf)
        .expect("failed to skip graph.txt header in pass 2");

    // Pass 2: populate targets and weights into CSR layout
    for _ in 0..m {
        line_buf.clear();
        let bytes_read = reader2
            .read_until(b'\n', &mut line_buf)
            .expect("I/O error reading edge in graph.txt pass 2");
        if bytes_read == 0 {
            break;
        }
        let mut p = 0;
        let u = parse_u32_field(&line_buf, &mut p) as usize;
        let v = parse_u32_field(&line_buf, &mut p);
        let w = parse_u32_field(&line_buf, &mut p);

        // Safe index: u < n guaranteed; slot < m guaranteed by degree prefix sums
        let slot = cur_offsets[u];
        cur_offsets[u] += 1;
        targets[slot] = v;
        weights[slot] = w;
    }
    drop(cur_offsets);

    // Pre-allocate query state buffers to eliminate per-query allocation
    let mut dist = vec![u64::MAX; n];
    let mut touched = Vec::with_capacity(16 * 1024);
    let mut heap = BinaryHeap::with_capacity(16 * 1024);

    // Open queries file
    let qfile = match File::open(&queries_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open queries file {}: {}", queries_path, e);
            std::process::exit(1);
        }
    };
    let mut qreader = BufReader::with_capacity(64 * 1024, qfile);

    // In-memory output buffer for single atomic write (PLAN §6 item 3)
    let mut out_buf = Vec::with_capacity(32 * 1024);

    // Process queries sequentially in input order (Spec §4)
    loop {
        line_buf.clear();
        let bytes_read = qreader
            .read_until(b'\n', &mut line_buf)
            .expect("I/O error reading queries.txt");
        if bytes_read == 0 {
            break;
        }
        if line_buf.iter().all(|&b| b.is_ascii_whitespace()) {
            continue;
        }

        let mut p = 0;
        let s = parse_u32_field(&line_buf, &mut p);
        let t = parse_u32_field(&line_buf, &mut p);

        let ans = shortest_path(
            &offsets,
            &targets,
            &weights,
            s,
            t,
            &mut dist,
            &mut touched,
            &mut heap,
        );

        // Write answer followed by LF (Spec §4: decimal integer or -1)
        writeln!(out_buf, "{}", ans).expect("writing to in-memory Vec<u8> cannot fail");
    }

    // Write output file once, flush, and close (Spec §4, PLAN §6 item 3, GR-C8)
    let mut out_file = File::create(&output_path).expect("cannot create /out/distances.txt");
    out_file
        .write_all(&out_buf)
        .expect("cannot write /out/distances.txt");
    out_file.flush().expect("cannot flush /out/distances.txt");
    drop(out_file);
}
