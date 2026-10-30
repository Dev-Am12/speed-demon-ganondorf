# Track D: Traverse

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/d-traverse.md).

## Approach

The baseline implementation (v0) is written in safe, single-threaded Rust using only the standard library.

1. **CSR Graph Representation:** Reads `/data/graph.txt` via a 256 KiB `BufReader` using a streaming two-pass approach. The first pass counts outgoing node degrees to construct the index offset array; the second pass populates target nodes (`u32`) and edge weights (`u32`). This builds a compact forward Compressed Sparse Row (CSR) graph in memory (~47 MB) without allocating large raw edge arrays or buffering the full input file.
2. **Dijkstra Search:** Uses `std::collections::BinaryHeap<Reverse<(u64, u32)>>` tracking cumulative path length in `u64` to support distances exceeding $2^{32} - 1$.
3. **Early Termination:** The priority queue search terminates immediately once the target node $t$ is settled.
4. **Touched List State Reset:** Instead of performing an $O(n)$ clear over the 1.74M distance array for each query, a `touched` vector records only the visited node IDs during relaxation, resetting `dist[v] = u64::MAX` in $O(\text{visited})$ time per query.
5. **Special Cases:** Queries with $s == t$ return `0` immediately without searching, correctly handling isolated nodes and self-loops. Unreachable targets naturally return `-1` when the priority queue is exhausted. Parallel edges and zero-weight edges are handled correctly by Dijkstra edge relaxation.
6. **Buffered Output:** All distance results are formatted into an in-memory buffer and written in a single pass to `/out/distances.txt`, then flushed and closed before exit.

AI coding tools were used in drafting this solution, and human teammates reviewed, validated, and verified every line against the formal specification and test harness.

## Dependencies

None.

## With another 24 hours

1. **Parallel query processing:** Distribute independent queries across worker threads using `std::thread::scope` without shared lock contention.
2. **Bidirectional Dijkstra:** Maintain a reverse CSR graph and search simultaneously forward from $s$ and backward from $t$, terminating when search frontiers collide.
3. **1-byte Radix Heap:** Replace `BinaryHeap` with a cache-friendly radix heap or monotone bucket queue to reduce priority queue extraction overhead.
4. **Fast graph parsing:** Scan edge line integers using SIMD-assisted delimiter finding and direct byte accumulation.
