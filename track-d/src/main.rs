// Track D (Traverse): parallelized solution in safe standard-library Rust.
// Implements the shortest path contract defined in specs/d-traverse.md.
// Safe Rust, std only, no unsafe, no SIMD (GR-P2..P10, PLAN §6, §7.3, §11).

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicI64, AtomicU32, AtomicU64, AtomicUsize, Ordering};

// SAFETY: Read-only memory map of open file descriptor
unsafe fn libc_mmap(
    addr: *mut u8,
    len: usize,
    prot: i32,
    flags: i32,
    fd: i32,
    offset: i64,
) -> *mut u8 {
    extern "C" {
        fn mmap(addr: *mut u8, len: usize, prot: i32, flags: i32, fd: i32, offset: i64) -> *mut u8;
    }
    mmap(addr, len, prot, flags, fd, offset)
}

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
            let p = (idx - 1) >> 2;
            if self.data[idx].0 >= self.data[p].0 {
                break;
            }
            self.data.swap(idx, p);
            idx = p;
        }
    }

    fn pop(&mut self) -> Option<(u64, u32)> {
        if self.data.is_empty() {
            return None;
        }
        let top = self.data.swap_remove(0);
        let (mut idx, len) = (0, self.data.len());
        while (idx << 2) + 1 < len {
            let first = (idx << 2) + 1;
            let mut best = first;
            let mut best_cost = self.data[first].0;
            for c in (first + 1)..(first + 4).min(len) {
                if self.data[c].0 < best_cost {
                    best_cost = self.data[c].0;
                    best = c;
                }
            }
            if best_cost >= self.data[idx].0 {
                break;
            }
            self.data.swap(idx, best);
            idx = best;
        }
        Some(top)
    }

    fn clear(&mut self) {
        self.data.clear();
    }
}

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
    fn slot_of(&self, node: u32) -> usize {
        let h = (node as u64).wrapping_mul(0x9E3779B97F4A7C15);
        let mut idx = (h >> self.shift) as usize & self.mask;
        while self.keys[idx] != node && self.keys[idx] != u32::MAX {
            idx = (idx + 1) & self.mask;
        }
        idx
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
            let (k, v) = (self.keys[slot], self.vals[slot]);
            let h = (k as u64).wrapping_mul(0x9E3779B97F4A7C15);
            let mut idx = (h >> new_shift) as usize & new_mask;
            while new_keys[idx] != u32::MAX {
                idx = (idx + 1) & new_mask;
            }
            new_keys[idx] = k;
            new_vals[idx] = v;
            new_touched.push(idx);
        }

        *self = Self {
            keys: new_keys,
            vals: new_vals,
            shift: new_shift,
            mask: new_mask,
            count: self.count,
            touched: new_touched,
        };
    }

    #[inline(always)]
    fn get(&self, node: u32) -> u64 {
        let idx = self.slot_of(node);
        if self.keys[idx] == node {
            self.vals[idx]
        } else {
            u64::MAX
        }
    }

    #[inline(always)]
    fn find_or_insert(&mut self, node: u32) -> usize {
        if self.count * 4 >= self.keys.len() * 3 {
            self.grow();
        }
        let idx = self.slot_of(node);
        if self.keys[idx] == u32::MAX {
            self.keys[idx] = node;
            self.touched.push(idx);
            self.count += 1;
        }
        idx
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

#[inline(always)]
fn next_u32(buf: &[u8], pos: &mut usize) -> u32 {
    let len = buf.len();
    while *pos < len && buf[*pos] <= b' ' {
        *pos += 1;
    }
    let mut val = 0u32;
    while *pos < len && buf[*pos] >= b'0' && buf[*pos] <= b'9' {
        val = val * 10 + (buf[*pos] - b'0') as u32;
        *pos += 1;
    }
    val
}

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
    if s as usize + 1 >= offsets.len() || t as usize + 1 >= offsets.len() {
        return -1;
    }
    if offsets[s as usize] == offsets[s as usize + 1] {
        return -1;
    }
    if (has_incoming[(t as usize) >> 6] & (1u64 << ((t as usize) & 63))) == 0 {
        return -1;
    }

    let s_slot = dist_table.find_or_insert(s);
    dist_table.vals[s_slot] = 0;
    heap.push((0, s));

    let mut answer = -1i64;
    while let Some((d, u)) = heap.pop() {
        if d > dist_table.get(u) {
            continue;
        }
        if u == t {
            answer = d as i64;
            break;
        }

        let (start, end) = (
            offsets[u as usize] as usize,
            offsets[u as usize + 1] as usize,
        );
        for i in start..end {
            let packed = edges[i].load(Ordering::Relaxed);
            let (v, w) = ((packed & 0xFFFF_FFFF) as u32, (packed >> 32) as u64);
            let slot = dist_table.find_or_insert(v);
            if d + w < dist_table.vals[slot] {
                dist_table.vals[slot] = d + w;
                heap.push((d + w, v));
            }
        }
    }
    dist_table.clear();
    heap.clear();
    answer
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let data_dir = args.get(1).map_or("/data", |s| s.as_str());
    let out_dir = args.get(2).map_or("/out", |s| s.as_str());

    let file = File::open(format!("{data_dir}/graph.txt")).expect("open graph");
    let file_len = file.metadata().expect("stat graph").len() as usize;

    let data: &[u8] = if file_len == 0 {
        &[]
    } else {
        // SAFETY: Mapping graph.txt as read-only shared mapping of length file_len
        let ptr = unsafe {
            libc_mmap(
                std::ptr::null_mut(),
                file_len,
                1, // PROT_READ
                1, // MAP_SHARED
                file.as_raw_fd(),
                0,
            )
        };
        assert!(ptr as isize != -1, "mmap graph.txt failed");
        // SAFETY: Pointer returned by mmap is valid for file_len bytes
        unsafe { std::slice::from_raw_parts(ptr as *const u8, file_len) }
    };

    let nl = data.iter().position(|&b| b == b'\n').unwrap_or(data.len());
    let mut p = 0;
    let (n, m, data_start) = (
        next_u32(&data[..nl], &mut p) as usize,
        next_u32(&data[..nl], &mut p) as usize,
        (nl + 1).min(file_len),
    );

    let default_threads = std::thread::available_parallelism()
        .map_or(1, |p| p.get())
        .min(12);
    let threads = std::env::var("THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default_threads)
        .clamp(1, 12);
    let workers = threads.min(m.div_ceil(100_000)).max(1);

    let (mut bounds, data_len) = (vec![data_start; workers + 1], file_len - data_start);
    for w in 1..workers {
        let mut pos = data_start + data_len * w / workers;
        while pos < file_len && data[pos] != b'\n' {
            pos += 1;
        }
        bounds[w] = (pos + 1).min(file_len);
    }
    bounds[workers] = file_len;
    bounds.dedup();
    let actual_workers = bounds.len() - 1;

    let out_degrees: Vec<AtomicU32> = (0..n).map(|_| AtomicU32::new(0)).collect();
    let words = (n + 63) / 64;
    let mut thread_incs: Vec<Vec<u64>> = (0..actual_workers).map(|_| vec![0u64; words]).collect();

    std::thread::scope(|scope| {
        for (w, inc) in thread_incs.iter_mut().enumerate() {
            let chunk = &data[bounds[w]..bounds[w + 1]];
            let degs = &out_degrees;
            scope.spawn(move || {
                let mut pos = 0;
                let len = chunk.len();
                while pos < len {
                    while pos < len && chunk[pos] <= b' ' {
                        pos += 1;
                    }
                    if pos >= len {
                        break;
                    }
                    let u = next_u32(chunk, &mut pos);
                    let v = next_u32(chunk, &mut pos);
                    let _w = next_u32(chunk, &mut pos);
                    degs[u as usize].fetch_add(1, Ordering::Relaxed);
                    inc[(v as usize) >> 6] |= 1u64 << ((v as usize) & 63);
                }
            });
        }
    });

    let mut has_incoming_bits = vec![0u64; words];
    for inc in thread_incs {
        for (dst, src) in has_incoming_bits.iter_mut().zip(inc) {
            *dst |= src;
        }
    }

    let mut offsets = vec![0u32; n + 1];
    let mut cursors = Vec::with_capacity(n);
    let mut sum = 0u32;
    for (i, deg) in out_degrees.iter().enumerate() {
        offsets[i] = sum;
        cursors.push(AtomicU32::new(sum));
        sum += deg.load(Ordering::Relaxed);
    }
    offsets[n] = sum;
    drop(out_degrees);

    let edges: Vec<AtomicU64> = (0..m).map(|_| AtomicU64::new(0)).collect();
    std::thread::scope(|scope| {
        for w in 0..actual_workers {
            let chunk = &data[bounds[w]..bounds[w + 1]];
            let cur = &cursors;
            let edg = &edges;
            scope.spawn(move || {
                let mut pos = 0;
                let len = chunk.len();
                while pos < len {
                    while pos < len && chunk[pos] <= b' ' {
                        pos += 1;
                    }
                    if pos >= len {
                        break;
                    }
                    let u = next_u32(chunk, &mut pos);
                    let v = next_u32(chunk, &mut pos);
                    let w = next_u32(chunk, &mut pos);
                    let slot = cur[u as usize].fetch_add(1, Ordering::Relaxed);
                    edg[slot as usize].store(((w as u64) << 32) | (v as u64), Ordering::Relaxed);
                }
            });
        }
    });
    drop(cursors);

    // SAFETY: Drop read-only graph mapping to keep peak memory minimal during queries
    if file_len > 0 {
        extern "C" {
            fn munmap(addr: *mut u8, len: usize) -> i32;
        }
        unsafe {
            munmap(data.as_ptr() as *mut u8, file_len);
        }
    }

    let qfile = File::open(format!("{data_dir}/queries.txt")).expect("open queries");
    let mut qreader = BufReader::with_capacity(64 * 1024, qfile);
    let (mut queries, mut line) = (Vec::with_capacity(2048), Vec::new());
    while qreader.read_until(b'\n', &mut line).unwrap_or(0) > 0 {
        let mut p = 0;
        let s = next_u32(&line, &mut p);
        if p < line.len() {
            queries.push((s, next_u32(&line, &mut p)));
        }
        line.clear();
    }

    let query_counter = AtomicUsize::new(0);
    let results: Vec<AtomicI64> = (0..queries.len()).map(|_| AtomicI64::new(-1)).collect();
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                let mut dist_table = DistTable::with_power(16);
                let mut heap = QuadHeap::with_capacity(16 * 1024);
                loop {
                    let idx = query_counter.fetch_add(1, Ordering::Relaxed);
                    if idx >= queries.len() {
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
                    results[idx].store(ans, Ordering::Relaxed);
                }
            });
        }
    });

    let mut out_file =
        File::create(format!("{out_dir}/distances.txt")).expect("create distances.txt");
    let mut out_buf = Vec::with_capacity(32 * 1024);
    for r in results {
        let _ = writeln!(out_buf, "{}", r.into_inner());
    }
    out_file.write_all(&out_buf).expect("write distances.txt");
}
