# Track D: Traverse

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/d-traverse.md).

## Approach

The implementation (Wave 1) is written in safe Rust using only the standard library:

1. **CSR Graph Representation:** Reads `/data/graph.txt` via a 256 KiB `BufReader` using a streaming two-pass approach. The first pass counts outgoing node degrees to construct the index offset array; the second pass populates target nodes (`u32`) and edge weights (`u32`). This builds a compact forward Compressed Sparse Row (CSR) graph in memory (~47 MB) without allocating large raw edge arrays or buffering the full input file.
2. **Parallel Query Processing (D5):** Loads all query pairs from `/data/queries.txt` into memory. An `AtomicUsize` query counter dynamically distributes queries across worker threads spawned within `std::thread::scope` (capped at 12 threads). Dynamic work stealing avoids load imbalance from variably sized Dijkstra searches.
3. **Dijkstra Search:** Each worker thread maintains its own preallocated distance array (`vec![u64::MAX; n]`), touched node tracker, and `BinaryHeap<Reverse<(u64, u32)>>`. Cumulative path length is tracked in `u64` to support distances exceeding $2^{32} - 1$.
4. **Early Termination:** The priority queue search terminates immediately once the target node $t$ is settled.
5. **Touched List State Reset:** Instead of performing an $O(n)$ clear over the 1.74M distance array for each query, a `touched` vector records only the visited node IDs during relaxation, resetting `dist[v] = u64::MAX` in $O(\text{visited})$ time per query.
6. **Special Cases:** Queries with $s == t$ return `0` immediately without searching, correctly handling isolated nodes and self-loops. Unreachable targets naturally return `-1` when the priority queue is exhausted. Parallel edges and zero-weight edges are handled correctly by Dijkstra edge relaxation.
7. **Thread Invariance & Ordered Output:** Worker threads write their query answers to preallocated index slots in a shared result vector. All results are written in single-pass buffered I/O to `/out/distances.txt`, guaranteeing byte-for-byte identical output regardless of thread count or execution order.

AI coding tools were used in drafting this solution, and human teammates reviewed, validated, and verified every line against the formal specification and test harness.

## Dependencies

None.

## With another 24 hours

1. **Parallel graph parsing & CSR build:** Read chunks of `graph.txt` concurrently with atomic degree accumulators to shave graph construction time.
2. **4-ary flat heap:** Replace `BinaryHeap` with a cache-aligned flat 4-ary heap to reduce comparison overhead and tree depth.
3. **Bidirectional Dijkstra:** Maintain a reverse CSR graph and search simultaneously forward from $s$ and backward from $t$, terminating when search frontiers collide.
4. **1-byte Radix Heap:** Replace `BinaryHeap` with a cache-friendly radix heap or monotone bucket queue to reduce priority queue extraction overhead.
