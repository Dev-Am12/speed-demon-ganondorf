// Track D (Traverse): parallelized solution in safe standard-library Rust.
// Implements the shortest path contract defined in specs/d-traverse.md.
// Safe Rust, std only, no unsafe, no SIMD (GR-P2..P10, PLAN §6, §7.3, §11).

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::FileExt;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

const CHUNK_SIZE: usize = 64 * 1024;

/// 4-ary flat min-heap of (u64, u32) priority items.
/// Spec §3 & PLAN §7.3: shallower tree depth (log4 vs log2) and cache-line aligned child scanning.
struct QuadHeap {
    data: Vec<(u64, u32)>,
}

impl QuadHeap {
    fn with_capacity(cap: usize) -> Self {
        Self {
            data: Vec::with_capacity(cap),
        }
    }

    fn push(&mut self, item: (u64, u32)) {
        let mut idx = self.data.len();
        self.data.push(item);
        while idx > 0 {
            let parent = (idx - 1) >> 2;
            if self.data[idx].0 < self.data[parent].0 {
                self.data.swap(idx, parent);
                idx = parent;
            } else {
                break;
            }
        }
    }

    fn pop(&mut self) -> Option<(u64, u32)> {
        if self.data.is_empty() {
            return None;
        }
        let top = self.data.swap_remove(0);
        if !self.data.is_empty() {
            let mut idx = 0;
            let len = self.data.len();
            loop {
                let first_child = (idx << 2) + 1;
                if first_child >= len {
                    break;
                }
                let mut best_child = first_child;
                let mut best_cost = self.data[first_child].0;
                let last_child = (first_child + 4).min(len);
                for c in (first_child + 1)..last_child {
                    if self.data[c].0 < best_cost {
                        best_cost = self.data[c].0;
                        best_child = c;
                    }
                }
                if best_cost < self.data[idx].0 {
                    self.data.swap(idx, best_child);
                    idx = best_child;
                } else {
                    break;
                }
            }
        }
        Some(top)
    }

    fn clear(&mut self) {
        self.data.clear();
    }
}

/// Open-addressed hash table for visited nodes: key = node (u32), val = dist (u64).
/// Empty slot indicated by key == u32::MAX.
struct DistTable {
    keys: Vec<u32>,
    vals: Vec<u64>,
    shift: u32,
    mask: usize,
    count: usize,
    touched: Vec<usize>,
}

impl DistTable {
    fn with_power(pow: u32) -> Self {
        let cap = 1usize << pow;
        Self {
            keys: vec![u32::MAX; cap],
            vals: vec![u64::MAX; cap],
            shift: 64 - pow,
            mask: cap - 1,
            count: 0,
            touched: Vec::with_capacity(cap / 2),
        }
    }

    #[inline(always)]
    fn hash(&self, node: u32) -> usize {
        let h = (node as u64).wrapping_mul(0x9E3779B97F4A7C15);
        (h >> self.shift) as usize & self.mask
    }

    fn grow(&mut self) {
        let new_pow = (64 - self.shift) + 1;
        let new_cap = 1usize << new_pow;
        let mut new_keys = vec![u32::MAX; new_cap];
        let mut new_vals = vec![u64::MAX; new_cap];
        let new_shift = 64 - new_pow;
        let new_mask = new_cap - 1;
        let mut new_touched = Vec::with_capacity(self.count * 2);

        for &slot in &self.touched {
            let k = self.keys[slot];
            let v = self.vals[slot];
            let h = (k as u64).wrapping_mul(0x9E3779B97F4A7C15);
            let mut idx = (h >> new_shift) as usize & new_mask;
            while new_keys[idx] != u32::MAX {
                idx = (idx + 1) & new_mask;
            }
            new_keys[idx] = k;
            new_vals[idx] = v;
            new_touched.push(idx);
        }

        self.keys = new_keys;
        self.vals = new_vals;
        self.shift = new_shift;
        self.mask = new_mask;
        self.touched = new_touched;
    }

    #[inline(always)]
    fn get(&self, node: u32) -> u64 {
        let mut idx = self.hash(node);
        loop {
            let k = self.keys[idx];
            if k == node {
                return self.vals[idx];
            }
            if k == u32::MAX {
                return u64::MAX;
            }
            idx = (idx + 1) & self.mask;
        }
    }

    #[inline(always)]
    fn find_or_insert(&mut self, node: u32) -> usize {
        if self.count * 10 >= self.keys.len() * 7 {
            self.grow();
        }
        let mut idx = self.hash(node);
        loop {
            let k = self.keys[idx];
            if k == node {
                return idx;
            }
            if k == u32::MAX {
                self.keys[idx] = node;
                self.touched.push(idx);
                self.count += 1;
                return idx;
            }
            idx = (idx + 1) & self.mask;
        }
    }

    fn clear(&mut self) {
        for &slot in &self.touched {
            self.keys[slot] = u32::MAX;
            self.vals[slot] = u64::MAX;
        }
        self.touched.clear();
        self.count = 0;
    }
}

/// Parse unsigned decimal integer from byte buffer.
#[inline(always)]
fn parse_u32(buf: &[u8], pos: &mut usize) -> u32 {
    while *pos < buf.len() && (buf[*pos] == b' ' || buf[*pos] == b'\r' || buf[*pos] == b'\n') {
        *pos += 1;
    }
    let mut val = 0u64;
    while *pos < buf.len() && buf[*pos].is_ascii_digit() {
        val = val * 10 + (buf[*pos] - b'0') as u64;
        *pos += 1;
    }
    val as u32
}

/// Run single-source Dijkstra search from s to t using packed edge CSR.
/// Spec §3: Returns shortest directed distance in u64, or -1 if unreachable.
fn shortest_path(
    offsets: &[u32],
    edges: &[AtomicU64],
    has_incoming: &[u64],
    s: u32,
    t: u32,
    dist_table: &mut DistTable,
    heap: &mut QuadHeap,
) -> i64 {
    if s == t {
        return 0;
    }

    // D6a: degree-zero short-circuit check.
    // If s has out-degree 0 or t has in-degree 0 (and s != t), t is provably unreachable.
    if offsets[s as usize] == offsets[s as usize + 1] {
        return -1;
    }
    let t_word = (t as usize) >> 6;
    let t_bit = 1u64 << ((t as usize) & 63);
    if (has_incoming[t_word] & t_bit) == 0 {
        return -1;
    }

    let s_slot = dist_table.find_or_insert(s);
    dist_table.vals[s_slot] = 0;
    heap.push((0, s));

    let mut answer: i64 = -1;

    while let Some((d, u)) = heap.pop() {
        let cur_d = dist_table.get(u);
        if d > cur_d {
            continue;
        }

        if u == t {
            answer = d as i64;
            break;
        }

        let start = offsets[u as usize] as usize;
        let end = offsets[u as usize + 1] as usize;

        for i in start..end {
            let packed = edges[i].load(Ordering::Relaxed);
            let v = (packed & 0xFFFF_FFFF) as u32;
            let w = (packed >> 32) as u64;
            let next_d = d + w;

            let slot = dist_table.find_or_insert(v);
            if next_d < dist_table.vals[slot] {
                dist_table.vals[slot] = next_d;
                heap.push((next_d, v));
            }
        }
    }

    dist_table.clear();
    heap.clear();

    answer
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let data_dir = if args.len() > 1 { &args[1] } else { "/data" };
    let out_dir = if args.len() > 2 { &args[2] } else { "/out" };

    let graph_path = format!("{}/graph.txt", data_dir);
    let queries_path = format!("{}/queries.txt", data_dir);
    let output_path = format!("{}/distances.txt", out_dir);

    let file = match File::open(&graph_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open graph file {}: {}", graph_path, e);
            std::process::exit(1);
        }
    };
    let file_len = file.metadata().expect("cannot stat graph.txt").len() as usize;

    let mut header_buf = [0u8; 128];
    let n_read = file.read_at(&mut header_buf, 0).expect("read header");
    let nl_pos = header_buf[..n_read]
        .iter()
        .position(|&b| b == b'\n')
        .expect("newline in header");
    let mut p = 0;
    let n = parse_u32(&header_buf[..nl_pos], &mut p) as usize;
    let m = parse_u32(&header_buf[..nl_pos], &mut p) as usize;
    let data_start = nl_pos + 1;

    let default_threads = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1)
        .min(12);
    let num_threads = std::env::var("THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default_threads)
        .clamp(1, 12);

    let workers = num_threads.min(m.div_ceil(100_000)).max(1);
    let data_len = file_len - data_start;

    let mut bounds = vec![data_start; workers + 1];
    let mut snap_buf = [0u8; 1024];
    for w in 1..workers {
        let mut pos = data_start + data_len * w / workers;
        while let Ok(n) = file.read_at(&mut snap_buf[..1024.min(file_len - pos)], pos as u64) {
            if n == 0 {
                break;
            }
            if let Some(i) = snap_buf[..n].iter().position(|&b| b == b'\n') {
                pos += i + 1;
                break;
            }
            pos += n;
        }
        bounds[w] = pos;
    }
    bounds[workers] = file_len;
    bounds.dedup();
    let actual_workers = bounds.len() - 1;

    // Pass 1 (D1/D2): parallel count of outgoing degrees and D6a incoming presence bitset
    let out_degrees: Vec<AtomicU32> = (0..n).map(|_| AtomicU32::new(0)).collect();
    let has_incoming_words = (n + 63) / 64;
    let has_incoming: Vec<AtomicU64> = (0..has_incoming_words).map(|_| AtomicU64::new(0)).collect();

    std::thread::scope(|scope| {
        for w in 0..actual_workers {
            let start = bounds[w];
            let end = bounds[w + 1];
            let out_deg = &out_degrees;
            let has_in = &has_incoming;
            let f = &file;
            scope.spawn(move || {
                let mut buf = [0u8; CHUNK_SIZE];
                let mut carried = 0;
                let mut cur = start;
                while cur < end {
                    let to_read = (CHUNK_SIZE - carried).min(end - cur);
                    let n = f
                        .read_at(&mut buf[carried..carried + to_read], cur as u64)
                        .unwrap();
                    if n == 0 {
                        break;
                    }
                    let valid = carried + n;
                    let last_nl = match buf[..valid].iter().rposition(|&b| b == b'\n') {
                        Some(nl) => nl,
                        None => {
                            carried = valid;
                            cur += n;
                            continue;
                        }
                    };
                    let mut pos = 0;
                    while pos <= last_nl {
                        let u = parse_u32(&buf, &mut pos);
                        let v = parse_u32(&buf, &mut pos);
                        let _w = parse_u32(&buf, &mut pos);
                        out_deg[u as usize].fetch_add(1, Ordering::Relaxed);
                        has_in[(v as usize) >> 6]
                            .fetch_or(1 << ((v as usize) & 63), Ordering::Relaxed);
                        while pos <= last_nl && buf[pos] != b'\n' {
                            pos += 1;
                        }
                        pos += 1;
                    }
                    buf.copy_within(last_nl + 1..valid, 0);
                    carried = valid - (last_nl + 1);
                    cur += n;
                }
            });
        }
    });

    let has_incoming_bits: Vec<u64> = has_incoming.into_iter().map(|a| a.into_inner()).collect();

    // Prefix sum to compute offsets and cursor slots
    let mut offsets = vec![0u32; n + 1];
    let mut cursors: Vec<AtomicU32> = Vec::with_capacity(n);
    let mut sum = 0u32;
    for i in 0..n {
        offsets[i] = sum;
        cursors.push(AtomicU32::new(sum));
        sum += out_degrees[i].load(Ordering::Relaxed);
    }
    offsets[n] = sum;
    drop(out_degrees);

    assert_eq!(offsets[0], 0);
    assert_eq!(offsets[n] as usize, m);

    // Pass 2 (D1/D2/D9): parallel fill of packed edges (w << 32 | v)
    let edges: Vec<AtomicU64> = (0..m).map(|_| AtomicU64::new(0)).collect();

    std::thread::scope(|scope| {
        for w in 0..actual_workers {
            let start = bounds[w];
            let end = bounds[w + 1];
            let cur_slots = &cursors;
            let edg = &edges;
            let f = &file;
            scope.spawn(move || {
                let mut buf = [0u8; CHUNK_SIZE];
                let mut carried = 0;
                let mut cur = start;
                while cur < end {
                    let to_read = (CHUNK_SIZE - carried).min(end - cur);
                    let n = f
                        .read_at(&mut buf[carried..carried + to_read], cur as u64)
                        .unwrap();
                    if n == 0 {
                        break;
                    }
                    let valid = carried + n;
                    let last_nl = match buf[..valid].iter().rposition(|&b| b == b'\n') {
                        Some(nl) => nl,
                        None => {
                            carried = valid;
                            cur += n;
                            continue;
                        }
                    };
                    let mut pos = 0;
                    while pos <= last_nl {
                        let u = parse_u32(&buf, &mut pos);
                        let v = parse_u32(&buf, &mut pos);
                        let w = parse_u32(&buf, &mut pos);
                        let slot = cur_slots[u as usize].fetch_add(1, Ordering::Relaxed);
                        edg[slot as usize]
                            .store(((w as u64) << 32) | (v as u64), Ordering::Relaxed);
                        while pos <= last_nl && buf[pos] != b'\n' {
                            pos += 1;
                        }
                        pos += 1;
                    }
                    buf.copy_within(last_nl + 1..valid, 0);
                    carried = valid - (last_nl + 1);
                    cur += n;
                }
            });
        }
    });
    drop(cursors);

    // Read queries
    let qfile = match File::open(&queries_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open queries file {}: {}", queries_path, e);
            std::process::exit(1);
        }
    };
    let mut qreader = BufReader::with_capacity(64 * 1024, qfile);
    let mut queries = Vec::with_capacity(2048);
    let mut line_buf = Vec::new();

    loop {
        line_buf.clear();
        let bytes_read = qreader
            .read_until(b'\n', &mut line_buf)
            .expect("read queries.txt");
        if bytes_read == 0 {
            break;
        }
        if line_buf.iter().all(|&b| b.is_ascii_whitespace()) {
            continue;
        }
        let mut p = 0;
        let s = parse_u32(&line_buf, &mut p);
        let t = parse_u32(&line_buf, &mut p);
        queries.push((s, t));
    }

    let total_queries = queries.len();
    let mut results = vec![-1i64; total_queries];

    if total_queries > 0 {
        let query_counter = AtomicUsize::new(0);

        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(num_threads);
            for _ in 0..num_threads {
                handles.push(scope.spawn(|| {
                    let mut dist_table = DistTable::with_power(16);
                    let mut heap = QuadHeap::with_capacity(16 * 1024);
                    let mut local_results = Vec::with_capacity(total_queries / num_threads + 16);

                    loop {
                        let idx = query_counter.fetch_add(1, Ordering::Relaxed);
                        if idx >= total_queries {
                            break;
                        }
                        let (s, t) = queries[idx];
                        let ans = shortest_path(
                            &offsets,
                            &edges,
                            &has_incoming_bits,
                            s,
                            t,
                            &mut dist_table,
                            &mut heap,
                        );
                        local_results.push((idx, ans));
                    }
                    local_results
                }));
            }

            for handle in handles {
                for (idx, ans) in handle.join().expect("worker thread panicked") {
                    results[idx] = ans;
                }
            }
        });
    }

    let mut out_buf = Vec::with_capacity(32 * 1024);
    for ans in results {
        writeln!(out_buf, "{}", ans).expect("formatting into Vec<u8> cannot fail");
    }

    let mut out_file = File::create(&output_path).expect("cannot create /out/distances.txt");
    out_file
        .write_all(&out_buf)
        .expect("cannot write /out/distances.txt");
    out_file.flush().expect("cannot flush /out/distances.txt");
    drop(out_file);
}
