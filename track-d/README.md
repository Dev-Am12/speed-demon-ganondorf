# Track D: Traverse

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/d-traverse.md).

## Approach

The implementation (Wave 3) is written in safe Rust using only the standard library:

1. **Parallel Line-Snapped Parse & Atomic CSR Build (D1, D2, D9):** Reads `/data/graph.txt` in parallel line-snapped chunks using `FileExt::read_at` through a unified `for_each_edge` helper. Pass 1 counts node out-degrees in `Vec<AtomicU32>` and populates an incoming-edge bitset `has_incoming: Vec<AtomicU64>`. Pass 2 uses per-node atomic cursor slots to concurrently populate a packed 64-bit edge array `(w << 32) | v` in `AtomicU64`. Graph build time dropped from 1,290 ms to 289 ms (4.4x faster) with zero heap reallocation.
2. **Degree-Zero Short-Circuit Optimization (D6a):** Queries where target $t$ has zero incoming edges or source $s$ has zero outgoing edges ($s \ne t$) are provably unreachable. The 217.5 KiB incoming-edge bitset enables instant $O(1)$ detection, eliminating catastrophic full-component Dijkstra sweeps on unreachable queries.
3. **Single-Probe Dynamic Visited Table (D8, D10, D12):** Replaces 12 dense 14 MB per-thread distance vectors with a cache-local open-addressed hash table (`DistTable`, initial $2^{16}$ slots, Fibonacci multiplicative hashing `>> shift`, linear probing). Edge relaxation uses a consolidated `find_or_insert` probe (D10) that returns the slot directly, eliminating double hash lookups and probe walks. Load factor threshold tuned to 0.75 (D12) prevents premature doubling. Touched slot tracking clears only populated entries between queries, keeping peak RSS at ~130 MB.
4. **Parallel Query Processing (D5, D13):** Loads all query pairs from `/data/queries.txt`. An `AtomicUsize` query counter dynamically distributes queries across worker threads spawned within `std::thread::scope` (capped at 12 threads). Worker threads write directly to an atomic results vector (`Vec<AtomicI64>`), eliminating thread-local result vector allocations and join collection loops.
5. **4-ary Flat Min-Heap (D4):** Each worker thread maintains a custom flat 4-ary min-heap (`QuadHeap`) of `(u64, u32)` items over a single contiguous `Vec`. Tree depth is halved compared to `BinaryHeap`, and 4 sibling elements occupy a single cache line during sift-down operations.
6. **Early Termination:** The priority queue search terminates immediately once the target node $t$ is settled.
7. **Special Cases:** Queries with $s == t$ return `0` immediately without searching, correctly handling isolated nodes and self-loops. Unreachable targets return `-1` when the priority queue is exhausted or via degree-zero short-circuit. Parallel edges and zero-weight edges are handled correctly by edge relaxation.
8. **Thread Invariance & Ordered Output:** Worker threads write their query answers to preallocated index slots in the shared atomic result vector. All results are written in single-pass buffered I/O to `/out/distances.txt`, guaranteeing byte-for-byte identical output regardless of thread count or execution order.

AI coding tools were used in drafting this solution, and human teammates reviewed, validated, and verified every line against the formal specification and test harness.

## Dependencies

None.

## Evaluated Alternatives & Rejections

1. **Bidirectional Search Retry (D6r):** Evaluated and rejected. Implemented dual CSRs and dual priority queues with PLAN §7.3 termination (`min_fwd + min_bwd >= best`). While settled node count dropped 38% (23.2M -> 14.3M), cross-table lookups on every relaxed edge doubled inner-loop hashing and branch mispredictions, slowing queries by 528 ms (1562 ms -> 2090 ms) while peak RSS exploded from 130 MB to 472.9 MB.
2. **BFS Node Renumbering & Locality (D7r):** Evaluated. Offline upper-bound simulation showed renumbering forward node IDs in BFS order yielded a 249 ms query gain with hash tables. However, a locality-preserving low-bits table `(node & mask)` suffered severe primary clustering (+770 ms slowdown). In-binary forward BFS traversal (~130 ms) plus parallel permuted CSR rebuild (~180 ms) totaled ~310 ms, exceeding the query gain while adding +100 lines of code ($S$).
3. **Early Process Exit (D11):** Tested `std::process::exit(0)` after flushing `/out/distances.txt`. Yielded only ~13 ms improvement (within noise margin); reverted per task card discipline.
