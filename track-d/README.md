# Track D: Traverse

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/d-traverse.md).

## Approach

The implementation (Wave 2) is written in safe Rust using only the standard library:

1. **Parallel Line-Snapped Parse & Atomic CSR Build (D1, D2, D9):** Reads `/data/graph.txt` in parallel chunks using `FileExt::read_at` into small 64 KiB per-thread buffers. Pass 1 counts node out-degrees in `Vec<AtomicU32>` and populates an incoming-edge bitset `has_incoming: Vec<AtomicU64>`. Pass 2 uses per-node atomic cursor slots to concurrently populate a packed 64-bit edge array `(w << 32) | v` in `AtomicU64`. Graph build time dropped from 1,290 ms to 289 ms (4.4x faster) with zero heap reallocation.
2. **Degree-Zero Short-Circuit Optimization (D6a):** Queries where target $t$ has zero incoming edges or source $s$ has zero outgoing edges ($s \ne t$) are provably unreachable. The 217.5 KiB incoming-edge bitset enables instant $O(1)$ detection, eliminating catastrophic full-component Dijkstra sweeps on unreachable queries and cutting query time by over 50%.
3. **Dynamic Open-Addressed Visited Table (D8):** Replaces the previous 12 dense 14 MB per-thread distance vectors with a cache-local, open-addressed hash table (`DistTable`, initial $2^{16}$ slots, Fibonacci multiplicative hashing `>> shift`, linear probing). Touched slot tracking clears only populated entries between queries. This collapses peak RSS from 219.5 MB down to 129.7 MB (12 threads) and 89.1 MB (6 threads).
4. **Parallel Query Processing (D5):** Loads all query pairs from `/data/queries.txt` into memory. An `AtomicUsize` query counter dynamically distributes queries across worker threads spawned within `std::thread::scope` (capped at 12 threads). Dynamic work stealing avoids load imbalance from variably sized Dijkstra searches.
5. **4-ary Flat Min-Heap (D4):** Each worker thread maintains a custom flat 4-ary min-heap (`QuadHeap`) of `(u64, u32)` items over a single contiguous `Vec`. Tree depth is halved compared to `BinaryHeap`, and 4 sibling elements occupy a single cache line during sift-down operations.
6. **Early Termination:** The priority queue search terminates immediately once the target node $t$ is settled.
7. **Special Cases:** Queries with $s == t$ return `0` immediately without searching, correctly handling isolated nodes and self-loops. Unreachable targets return `-1` when the priority queue is exhausted or via degree-zero short-circuit. Parallel edges and zero-weight edges are handled correctly by edge relaxation.
8. **Thread Invariance & Ordered Output:** Worker threads write their query answers to preallocated index slots in a shared result vector. All results are written in single-pass buffered I/O to `/out/distances.txt`, guaranteeing byte-for-byte identical output regardless of thread count or execution order.

AI coding tools were used in drafting this solution, and human teammates reviewed, validated, and verified every line against the formal specification and test harness.

## Dependencies

None.

## Evaluated Alternatives & Next Steps

1. **Bidirectional Search (D6):** Evaluated and rejected. On directed scale-free road graphs, maintaining forward and reverse CSRs doubles graph memory to 456 MB and requires tracking dual frontiers with collision checks. Unidirectional Dijkstra with D6a degree shortcuts proved 1.56x faster while using 71% less memory.
2. **BFS Node Renumbering (D7):** Evaluated. Renumbering nodes in BFS order cost ~200 ms in permutation passes, exceeding the ~60 ms cache-locality gain in query traversal.
3. **Monotone Bucket Queue / 1-byte Radix Heap:** With another 24 hours, replacing `QuadHeap` with an integer radix heap could shave priority queue overhead for small integer edge weights.
4. **SIMD Line Scanning:** SIMD-accelerated whitespace and integer parsing for further graph ingestion speedups.
