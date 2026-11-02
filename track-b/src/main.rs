use std::arch::x86_64::*;
use std::collections::HashMap;
use std::ffi::c_void;
use std::fs::{self, File};
use std::hash::{BuildHasherDefault, Hasher};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

const MAX_THREADS: usize = 12;
const MAX_DOCS: usize = 65_534; // local doc ids are u16 and 0 is never used as a sentinel here
const BLOCK: usize = 256; // queries per work item
const NONE: u32 = u32::MAX; // table code of a token that is not a query term
const ID_BITS: u32 = 18; // table payload is id + 1, so ids stay below 2^18 - 1
const ID_MASK: u64 = (1 << ID_BITS) - 1;
const TAG_MASK: u64 = (1 << 46) - 1; // 46-bit tag from the low bits of the slot hash
const SAMPLE_DIV: usize = 32; // dense = document frequency >= 1/32 (measured on a sample)
const MAX_DENSE: usize = 512; // bitset rows per shard, at most (the most frequent terms)
const BUCKET: usize = 256; // sparse term ids per log bucket
const K_MUL: u64 = 0x517c_c1b7_2722_0a95;
const POS_MASK: [u64; 6] = [
    0xAAAA_AAAA_AAAA_AAAA,
    0xCCCC_CCCC_CCCC_CCCC,
    0xF0F0_F0F0_F0F0_F0F0,
    0xFF00_FF00_FF00_FF00,
    0xFFFF_0000_FFFF_0000,
    0xFFFF_FFFF_0000_0000,
];

// SAFETY: same mmap declaration as the current track (POSIX ABI, fixed-width arguments).
unsafe extern "C" {
    fn mmap(a: *mut c_void, len: usize, prot: i32, flags: i32, fd: i32, off: i64) -> *mut c_void;
    fn prctl(option: i32, ...) -> i32;
}

#[derive(Clone, Copy)]
enum Op {
    And,
    Or,
    Not,
}

#[derive(Clone, Copy)]
enum Query {
    Bool { terms: [u32; 6], ops: [Op; 5], len: u8 },
    Phrase { plan: u32 },
}

/// Phrase plan. `keys` and `terms` follow phrase order; `dterms` are the distinct terms,
/// rarest first; `seed_positions` marks the phrase positions that hold the rarest term.
#[derive(Clone, Copy, Default)]
struct PhrasePlan {
    terms: [u32; 8],
    keys: [(u64, u64); 8],
    len: u8,
    dterms: [u32; 8],
    dn: u8,
    seed_positions: u8,
}

// ---------------------------------------------------------------- hashing

#[inline(always)]
fn mix64(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// Key of a query term: up to 16 bytes are stored verbatim (lo, hi); 17..=32 bytes are hashed
/// exactly like the tokenizer hashes document tokens; longer terms never match (key (0, 0)).
fn pack(term: &[u8]) -> (u64, u64) {
    if term.len() > 32 {
        return (0, 0);
    }
    let mut b = [0u8; 32];
    b[..term.len()].copy_from_slice(term);
    let w = |i: usize| u64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap());
    if term.len() <= 16 {
        return (w(0), w(1));
    }
    let lo = mix64(w(0) ^ w(2).rotate_left(17) ^ term.len() as u64).max(1);
    (lo, mix64(w(1) ^ w(3).rotate_left(37) ^ lo) | (1u64 << 63))
}

/// One multiply plus one finalizer. The seed enters through the multiplier input, so two keys
/// that collide under one seed (only possible with different `hi` words) rarely do under the next.
#[inline(always)]
fn slot_hash(k: (u64, u64), seed: u64) -> u64 {
    mix64(k.0 ^ (k.1 ^ seed).wrapping_mul(K_MUL))
}

#[derive(Default)]
struct FxHasher(u64);

impl Hasher for FxHasher {
    fn finish(&self) -> u64 {
        mix64(self.0)
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(5) ^ b as u64).wrapping_mul(K_MUL);
        }
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = (self.0.rotate_left(5) ^ v).wrapping_mul(K_MUL);
    }
}

type FxMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;

// ------------------------------------------------------------- query terms

/// Distinct query terms in first-seen order: exact ids, keys and the bytes of each term.
#[derive(Default)]
struct Terms {
    ids: FxMap<(u64, u64), u32>,
    keys: Vec<(u64, u64)>,
    texts: Vec<([u8; 32], u8)>, // length 0 for terms longer than 32 bytes (never matched)
}

impl Terms {
    fn intern(&mut self, term: &[u8]) -> u32 {
        let k = pack(term);
        if let Some(&id) = self.ids.get(&k) {
            return id;
        }
        let id = self.keys.len() as u32;
        self.ids.insert(k, id);
        self.keys.push(k);
        let mut text = [0u8; 32];
        let mut len = 0u8;
        if term.len() <= 32 {
            text[..term.len()].copy_from_slice(term);
            len = term.len() as u8;
        }
        self.texts.push((text, len));
        id
    }
}

// ------------------------------------------------------------------- table

/// Four slots in 32 aligned bytes, so a bucket never straddles a cache line.
#[derive(Clone, Copy)]
#[repr(C, align(32))]
struct Bucket([u64; 4]);

/// Tag-only bucketed table over the distinct query terms. A slot holds
/// (46-bit tag << 18) | (id + 1), and 0 is empty. A key goes into the first free slot of its home
/// bucket, or of the next buckets when that one is full. A lookup compares the four tags of the
/// home bucket with one vector compare and only walks on when the bucket is full and holds no
/// match, so an absent token can rarely match a term (about 1e-7 per dataset, as before).
/// Build keeps a seed only if every present term looks itself up.
struct Vocab {
    buckets: Vec<Bucket>,
    seed: u64,
    shift: u32,
}

impl Vocab {
    fn build(keys: &[(u64, u64)]) -> Vocab {
        assert!(keys.len() < ID_MASK as usize, "vocabulary too large for table ids");
        let mut bits = 4u32;
        while (4usize << bits) * 2 < keys.len() * 3 {
            bits += 1;
        }
        let mask = (1usize << bits) - 1;
        let mut seed = 1u64;
        loop {
            let mut buckets = vec![Bucket([0; 4]); mask + 1];
            for (id, &k) in keys.iter().enumerate() {
                let h = slot_hash(k, seed);
                let mut b = (h >> (64 - bits)) as usize;
                loop {
                    let bk = &mut buckets[b].0;
                    if let Some(s) = bk.iter().position(|&e| e == 0) {
                        bk[s] = ((h & TAG_MASK) << ID_BITS) | (id as u64 + 1);
                        break;
                    }
                    b = (b + 1) & mask;
                }
            }
            let v = Vocab {
                buckets,
                seed,
                shift: 64 - bits,
            };
            if keys.iter().enumerate().all(|(id, &k)| v.code(k) == id as u32) {
                return v;
            }
            seed += 1;
        }
    }

    /// Term id of a token key, or NONE. The common cases (hit, or miss in a bucket with room)
    /// contain no data-dependent branch.
    #[inline(always)]
    fn code(&self, k: (u64, u64)) -> u32 {
        let h = slot_hash(k, self.seed);
        let b = (h >> self.shift) as usize;
        // SAFETY: the shift keeps log2(bucket count) bits, so b < buckets.len().
        let bk = unsafe { self.buckets.get_unchecked(b) };
        let want = (h & TAG_MASK) << ID_BITS;
        // SAFETY: Bucket is 32-byte aligned; x86-64-v3 includes AVX2 (see the Dockerfile).
        let m = unsafe {
            let v = _mm256_load_si256(bk.0.as_ptr().cast::<__m256i>());
            let eq = _mm256_cmpeq_epi64(
                _mm256_and_si256(v, _mm256_set1_epi64x(!ID_MASK as i64)),
                _mm256_set1_epi64x(want as i64),
            );
            _mm256_movemask_pd(_mm256_castsi256_pd(eq)) as u32
        };
        if (m == 0) & (bk.0[3] != 0) {
            return self.spill(h, b);
        }
        let hit = (m != 0) as u64;
        // Lane 0 is read when nothing matched; `hit - 1` is all ones then, which makes NONE.
        let payload = bk.0[(m.trailing_zeros() & 3) as usize] & ID_MASK;
        (payload.wrapping_sub(1) | hit.wrapping_sub(1)) as u32
    }

    /// Continues a lookup in the buckets after a full home bucket without a match.
    #[cold]
    #[inline(never)]
    fn spill(&self, h: u64, mut b: usize) -> u32 {
        let want = (h & TAG_MASK) << ID_BITS;
        loop {
            b = (b + 1) & (self.buckets.len() - 1);
            let bk = &self.buckets[b].0;
            for &e in bk {
                if e & !ID_MASK == want {
                    return (e & ID_MASK).wrapping_sub(1) as u32;
                }
            }
            if bk[3] == 0 {
                return NONE;
            }
        }
    }

    /// Rewrites every payload id through `map` (old id -> new id). Tags are unchanged.
    fn remap(&mut self, map: &[u32]) {
        for e in self.buckets.iter_mut().flat_map(|bk| bk.0.iter_mut()) {
            if *e != 0 {
                *e = (*e & !ID_MASK) | (map[((*e & ID_MASK) - 1) as usize] as u64 + 1);
            }
        }
    }
}

// --------------------------------------------------------------- tokenizer

const fn make_byte_map() -> [u8; 256] {
    let mut map = [0u8; 256];
    let mut b = 0usize;
    while b < 256 {
        map[b] = match b as u8 {
            c @ (b'a'..=b'z' | b'0'..=b'9') => c,
            c @ b'A'..=b'Z' => c | 0x20,
            _ => 0,
        };
        b += 1;
    }
    map
}

/// Lower-cased token byte, or 0 for every separator byte.
const BYTE_MAP: [u8; 256] = make_byte_map();

#[inline(always)]
fn mapped(t: &[u8], i: usize) -> u8 {
    // SAFETY: every caller checks i < t.len() first.
    unsafe { BYTE_MAP[*t.get_unchecked(i) as usize] }
}

/// Tokenizes t[i..] up to the first LF (or the end). `f` receives each token's key (None for a
/// token longer than 32 bytes, which still occupies a position) plus its byte offset, and
/// returns false to stop early. Returns the index where the scan stopped: the LF, or t.len().
#[inline(always)]
fn scan<F: FnMut(usize, Option<(u64, u64)>) -> bool>(t: &[u8], mut i: usize, mut f: F) -> usize {
    let n = t.len();
    while i < n {
        let c = mapped(t, i);
        if c == 0 {
            if t[i] == b'\n' {
                return i;
            }
            i += 1;
            continue;
        }
        let token_start = i;
        let mut lo = c as u64;
        let mut shift = 8u32;
        i += 1;
        while i < n && shift < 64 {
            let b = mapped(t, i);
            if b == 0 {
                break;
            }
            lo |= (b as u64) << shift;
            shift += 8;
            i += 1;
        }
        let key = 'tok: {
            if i == n || mapped(t, i) == 0 {
                break 'tok Some((lo, 0));
            }
            let mut hi = 0u64;
            shift = 0;
            while i < n && shift < 64 {
                let b = mapped(t, i);
                if b == 0 {
                    break;
                }
                hi |= (b as u64) << shift;
                shift += 8;
                i += 1;
            }
            if i == n || mapped(t, i) == 0 {
                break 'tok Some((lo, hi));
            }
            let mut extra = [0u8; 16];
            let mut m = 0usize;
            while i < n {
                let b = mapped(t, i);
                if b == 0 {
                    break;
                }
                if m < 16 {
                    extra[m] = b;
                }
                m += 1;
                i += 1;
            }
            if m > 16 {
                break 'tok None;
            }
            let w2 = u64::from_le_bytes(extra[0..8].try_into().unwrap());
            let w3 = u64::from_le_bytes(extra[8..16].try_into().unwrap());
            let h_lo = mix64(lo ^ w2.rotate_left(17) ^ (16 + m) as u64).max(1);
            Some((h_lo, mix64(hi ^ w3.rotate_left(37) ^ h_lo) | (1u64 << 63)))
        };
        if !f(token_start, key) {
            return n;
        }
        if i < n && t[i] == b'\n' {
            return i;
        }
        i += 1;
    }
    n
}

/// AVX2 block classifier: returns (alphanumeric mask, LF mask) for 64 bytes at p.
#[inline(always)]
unsafe fn classify64(p: *const u8) -> (u64, u64) {
    // SAFETY: caller guarantees 64 readable bytes at p.
    unsafe {
        let v0 = _mm256_loadu_si256(p.cast::<__m256i>());
        let v1 = _mm256_loadu_si256(p.add(32).cast::<__m256i>());
        let alnum = |v: __m256i| {
            let d = _mm256_and_si256(
                _mm256_cmpgt_epi8(v, _mm256_set1_epi8(0x2F)),
                _mm256_cmpgt_epi8(_mm256_set1_epi8(0x3A), v),
            );
            let u = _mm256_and_si256(
                _mm256_cmpgt_epi8(v, _mm256_set1_epi8(0x40)),
                _mm256_cmpgt_epi8(_mm256_set1_epi8(0x5B), v),
            );
            let l = _mm256_and_si256(
                _mm256_cmpgt_epi8(v, _mm256_set1_epi8(0x60)),
                _mm256_cmpgt_epi8(_mm256_set1_epi8(0x7B), v),
            );
            _mm256_or_si256(d, _mm256_or_si256(u, l))
        };
        let nl = _mm256_set1_epi8(b'\n' as i8);
        let a = (_mm256_movemask_epi8(alnum(v0)) as u32 as u64)
            | ((_mm256_movemask_epi8(alnum(v1)) as u32 as u64) << 32);
        let lf = (_mm256_movemask_epi8(_mm256_cmpeq_epi8(v0, nl)) as u32 as u64)
            | ((_mm256_movemask_epi8(_mm256_cmpeq_epi8(v1, nl)) as u32 as u64) << 32);
        (a, lf)
    }
}

/// Key of the token starting at s. Tokens of 1..=15 bytes take a 16-byte load; anything
/// longer (or too close to the end of the buffer) uses the exact scalar tokenizer.
#[inline(always)]
unsafe fn token_key(t: &[u8], s: usize) -> Option<(u64, u64)> {
    if s + 16 <= t.len() {
        // SAFETY: s + 16 <= t.len().
        unsafe {
            let v = _mm_loadu_si128(t.as_ptr().add(s).cast::<__m128i>());
            let up = _mm_and_si128(
                _mm_cmpgt_epi8(v, _mm_set1_epi8(0x40)),
                _mm_cmpgt_epi8(_mm_set1_epi8(0x5B), v),
            );
            let lo_c = _mm_and_si128(
                _mm_cmpgt_epi8(v, _mm_set1_epi8(0x60)),
                _mm_cmpgt_epi8(_mm_set1_epi8(0x7B), v),
            );
            let dg = _mm_and_si128(
                _mm_cmpgt_epi8(v, _mm_set1_epi8(0x2F)),
                _mm_cmpgt_epi8(_mm_set1_epi8(0x3A), v),
            );
            let alnum = (_mm_movemask_epi8(_mm_or_si128(up, _mm_or_si128(lo_c, dg))) as u32) & 0xFFFF;
            if alnum != 0xFFFF {
                let len = (!alnum).trailing_zeros() as usize; // 1..=15
                // Setting bit 5 lower-cases A-Z and leaves digits and a-z unchanged; bytes past
                // the token are masked off below.
                let lowered = _mm_or_si128(v, _mm_set1_epi8(0x20));
                let w0 = _mm_cvtsi128_si64(lowered) as u64;
                let w1 = _mm_cvtsi128_si64(_mm_unpackhi_epi64(lowered, lowered)) as u64;
                let lo = w0 & (u64::MAX >> (64 - 8 * len.min(8)));
                let hi = w1 & ((1u64 << (8 * (len.max(8) - 8))) - 1);
                return Some((lo, hi));
            }
        }
    }
    // Exact scalar path for long tokens and the last few bytes of the buffer.
    let end = s + t[s..].iter().take_while(|&&c| BYTE_MAP[c as usize] != 0).count();
    let mut key = None;
    scan(&t[s..end], 0, |_, k| {
        key = k;
        true
    });
    key
}

/// Same contract as `scan`, for text that starts at a token boundary: it classifies 64 bytes
/// per step and visits token starts found in the masks instead of every byte.
#[target_feature(enable = "avx2,bmi1,bmi2,popcnt")]
unsafe fn scan_avx2<F: FnMut(usize, Option<(u64, u64)>) -> bool>(t: &[u8], i0: usize, mut f: F) -> usize {
    let n = t.len();
    let mut prev = 0u64; // 1 when the byte before the current block is alphanumeric
    let mut p = i0;
    let mut tail = [0u8; 64];
    while p < n {
        let (alnum, lf) = if p + 64 <= n {
            // SAFETY: p + 64 <= n.
            unsafe { classify64(t.as_ptr().add(p)) }
        } else {
            let rem = n - p;
            tail[..rem].copy_from_slice(&t[p..]);
            tail[rem..].fill(0);
            // SAFETY: tail has 64 bytes.
            unsafe { classify64(tail.as_ptr()) }
        };
        let starts = alnum & !((alnum << 1) | prev);
        prev = alnum >> 63;
        let limit = if lf != 0 { lf.trailing_zeros() } else { 64 };
        let mut bits = if limit >= 64 { starts } else { starts & ((1u64 << limit) - 1) };
        while bits != 0 {
            let b = bits.trailing_zeros() as usize;
            bits &= bits - 1;
            let pos = p + b;
            // SAFETY: pos < n.
            if !f(pos, unsafe { token_key(t, pos) }) {
                return n;
            }
        }
        if lf != 0 {
            return p + limit as usize;
        }
        p += 64;
    }
    n
}

/// Safe entry point; the binary is built for x86-64-v3, so AVX2 and BMI are always present.
#[inline(always)]
fn scan_fast<F: FnMut(usize, Option<(u64, u64)>) -> bool>(t: &[u8], i0: usize, f: F) -> usize {
    // SAFETY: x86-64-v3 includes AVX2, BMI1, BMI2 and POPCNT (see the Dockerfile).
    unsafe { scan_avx2(t, i0, f) }
}

/// Document-line start: the offset of the text after the TAB in the first 16 bytes, if any.
#[inline(always)]
fn text_start(docs: &[u8], pos: usize, b: usize) -> Option<usize> {
    let head = &docs[pos..(pos + 16).min(b)];
    head.iter()
        .position(|&x| x == b'\t' || x == b'\n')
        .filter(|&p| head[p] == b'\t')
        .map(|p| pos + p + 1)
}

// ------------------------------------------------------------------ threads

/// Runs `f` on `n` scoped threads and returns their results.
fn workers<T: Send>(n: usize, f: impl Fn() -> T + Sync) -> Vec<T> {
    let f = &f;
    thread::scope(|s| {
        let hs: Vec<_> = (0..n).map(|_| s.spawn(move || f())).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    })
}

/// Evaluates `f(i, state)` for i in 0..n on `threads` workers (dynamic scheduling through one
/// atomic counter; `init` makes each worker's reusable scratch). Results come back in index order.
fn par_map<S, T: Send>(
    threads: usize,
    n: usize,
    init: impl Fn() -> S + Sync,
    f: impl Fn(usize, &mut S) -> T + Sync,
) -> Vec<T> {
    let next = AtomicUsize::new(0);
    let parts = workers(threads, || {
        let mut state = init();
        let mut out = Vec::new();
        loop {
            let i = next.fetch_add(1, Ordering::Relaxed);
            if i >= n {
                break out;
            }
            out.push((i, f(i, &mut state)));
        }
    });
    let mut all: Vec<(usize, T)> = parts.into_iter().flatten().collect();
    all.sort_unstable_by_key(|p| p.0);
    all.into_iter().map(|p| p.1).collect()
}

// ------------------------------------------------------------------- shards

#[derive(Clone, Copy)]
enum SetView<'a> {
    List(&'a [u16]),
    Bits(&'a [u64]),
}

impl SetView<'_> {
    #[inline(always)]
    fn contains(self, id: u16) -> bool {
        match self {
            SetView::List(l) => l.binary_search(&id).is_ok(),
            SetView::Bits(w) => w[id as usize >> 6] & (1u64 << (id & 63)) != 0,
        }
    }

    /// Calls `f` with every member in ascending order.
    fn each(self, mut f: impl FnMut(u16)) {
        match self {
            SetView::List(l) => {
                for &d in l {
                    f(d);
                }
            }
            SetView::Bits(ws) => {
                for (i, &w) in ws.iter().enumerate() {
                    let mut r = w;
                    while r != 0 {
                        f((i * 64 + r.trailing_zeros() as usize) as u16);
                        r &= r - 1;
                    }
                }
            }
        }
    }
}

/// One contiguous run of at most MAX_DOCS documents with its own posting lists and bitsets.
/// Terms below `dense` are bitset rows; the others are sorted lists laid out back to back in the
/// arena in term order, so the list of term `dense + i` is arena[offs[i]..offs[i + 1]].
struct Shard {
    base: u32,              // global id of the shard's first document
    words: usize,           // u64 words per bitset row
    dense: usize,           // number of dense terms
    spans: Vec<(u32, u32)>, // per document: text start and end (absolute byte offsets)
    offs: Vec<u32>,         // sparse list boundaries, one more entry than sparse terms
    arena: Vec<u16>,        // sparse posting lists, ascending local ids
    bits: Vec<u64>,         // dense rows: row r is bits[r * words .. (r + 1) * words]
}

impl Shard {
    fn view(&self, t: u32) -> SetView<'_> {
        let t = t as usize;
        if t < self.dense {
            SetView::Bits(&self.bits[t * self.words..(t + 1) * self.words])
        } else {
            let i = t - self.dense;
            SetView::List(&self.arena[self.offs[i] as usize..self.offs[i + 1] as usize])
        }
    }
}

/// Copies the current 64-document window of every dense accumulator into its bitset row.
fn flush_window(acc: &mut [u64], bits: &mut [u64], words: usize, w: usize) {
    if words == 0 {
        return;
    }
    for (r, a) in acc.iter_mut().enumerate() {
        bits[r * words + w] = *a;
        *a = 0;
    }
}

/// Indexes documents [a, b) (`nd` of them) in one sequential scan. Dense terms (ids below
/// d_count) set bits in a 64-document accumulator that stays in L1. Sparse hits are appended to
/// a per-bucket log, and each bucket is then drained into exact, duplicate-free lists, so the
/// random writes of a whole-corpus pass are replaced by small, cache-local ones. Every token
/// performs the same stores whatever it is: dense and absent tokens land in a scratch
/// accumulator slot and a scratch log that is never advanced, so the loop has no branch that
/// depends on the token.
fn index_shard(docs: &[u8], a: usize, b: usize, nd: usize, vocab: &Vocab, d_count: usize, nterms: usize) -> Shard {
    let words = nd.div_ceil(64);
    let nb = (nterms - d_count).div_ceil(BUCKET);
    let cap = ((b - a) / 16 / nb.max(1)).max(16);
    let mut logs: Vec<Vec<u32>> = (0..=nb).map(|_| Vec::with_capacity(cap)).collect(); // logs[nb]: scratch
    let mut acc = vec![0u64; d_count + 1]; // acc[d_count]: scratch
    let mut bits = vec![0u64; d_count * words];
    let mut spans = Vec::with_capacity(nd);
    let mut window = 0usize;
    let mut pos = a;
    let mut d = 0usize;
    while pos < b {
        if d >> 6 != window {
            flush_window(&mut acc[..d_count], &mut bits, words, window);
            window = d >> 6;
        }
        let du = d as u32;
        let bit = 1u64 << (d & 63);
        let tab = text_start(docs, pos, b);
        let end = match tab {
            Some(ts) => scan_fast(&docs[..b], ts, |_, tok| {
                let c = tok.map_or(NONE, |k| vocab.code(k)) as usize;
                let r = c.wrapping_sub(d_count); // sparse term index; huge for dense and absent
                let kb = (r / BUCKET).min(nb); // nb for dense and absent tokens
                // SAFETY: c.min(d_count) <= d_count < acc.len() and kb <= nb < logs.len(); the
                // write lands inside the reserved capacity, and the length only grows for kb < nb.
                unsafe {
                    *acc.get_unchecked_mut(c.min(d_count)) |= bit;
                    let log = logs.get_unchecked_mut(kb);
                    if log.len() == log.capacity() {
                        log.reserve(1);
                    }
                    let len = log.len();
                    log.as_mut_ptr().add(len).write(((r % BUCKET) as u32) << 16 | du);
                    log.set_len(len + (kb < nb) as usize);
                }
                true
            }),
            None => docs[pos..b].iter().position(|&x| x == b'\n').map_or(b, |p| pos + p),
        };
        spans.push((tab.unwrap_or(end) as u32, end as u32));
        pos = end + 1;
        d += 1;
    }
    flush_window(&mut acc[..d_count], &mut bits, words, window);
    assert_eq!(d, nd, "line count changed between the cut pass and indexing");

    let mut arena: Vec<u16> = Vec::new();
    let mut offs: Vec<u32> = Vec::with_capacity(nterms - d_count + 1);
    let mut cnt = [0u32; BUCKET];
    let mut last = [u32::MAX; BUCKET];
    let mut start = [0usize; BUCKET];
    for (k, log) in logs.iter().take(nb).enumerate() {
        // Pass 1: exact per-term counts. Entries are in document order, so `last` removes
        // repeats of a term inside one document.
        cnt.fill(0);
        last.fill(u32::MAX);
        for &e in log {
            let (kk, doc) = ((e >> 16) as usize, e & 0xFFFF);
            if last[kk] != doc {
                last[kk] = doc;
                cnt[kk] += 1;
            }
        }
        // Pass 2: place every list at its offset in the shared arena.
        let mut run = arena.len();
        for kk in 0..BUCKET {
            start[kk] = run;
            run += cnt[kk] as usize;
        }
        arena.resize(run, 0);
        let mut fill = start;
        last.fill(u32::MAX);
        for &e in log {
            let (kk, doc) = ((e >> 16) as usize, e & 0xFFFF);
            if last[kk] != doc {
                last[kk] = doc;
                arena[fill[kk]] = doc as u16;
                fill[kk] += 1;
            }
        }
        for kk in 0..BUCKET {
            if d_count + k * BUCKET + kk < nterms {
                offs.push(start[kk] as u32);
            }
        }
    }
    offs.push(arena.len() as u32);
    drop(logs);
    Shard {
        base: 0,
        words,
        dense: d_count,
        spans,
        offs,
        arena,
        bits,
    }
}

/// Distinct-document counts of every term over all documents of `part`, plus the document count.
fn sample_counts(part: &[u8], vocab: &Vocab, nterms: usize) -> (Vec<u32>, usize) {
    let mut cnt = vec![0u32; nterms];
    let mut last = vec![u32::MAX; nterms];
    let mut pos = 0usize;
    let mut d = 0u32;
    while pos < part.len() {
        let du = d;
        let end = match text_start(part, pos, part.len()) {
            Some(ts) => scan_fast(part, ts, |_, tok| {
                if let Some(t) = tok.map(|k| vocab.code(k)).filter(|&c| c != NONE) {
                    let t = t as usize;
                    if last[t] != du {
                        last[t] = du;
                        cnt[t] += 1;
                    }
                }
                true
            }),
            None => part[pos..].iter().position(|&x| x == b'\n').map_or(part.len(), |p| pos + p),
        };
        pos = end + 1;
        d += 1;
    }
    (cnt, d as usize)
}

// ------------------------------------------------------------------ queries

fn parse_queries(qb: &[u8], terms: &mut Terms) -> (Vec<Query>, Vec<PhrasePlan>) {
    let mut queries = Vec::with_capacity(50_000);
    let mut plans = Vec::new();
    for mut line in qb.split(|&b| b == b'\n') {
        if line.ends_with(b"\r") {
            line = &line[..line.len() - 1];
        }
        if let Some(rest) = line.strip_prefix(b"BOOL ") {
            let mut it = rest.split(|&b| b == b' ');
            let mut tids = [0u32; 6];
            let mut ops = [Op::And; 5];
            tids[0] = terms.intern(it.next().unwrap());
            let mut len = 1;
            while let (Some(op), Some(term)) = (it.next(), it.next()) {
                ops[len - 1] = match op {
                    b"AND" => Op::And,
                    b"OR" => Op::Or,
                    _ => Op::Not,
                };
                tids[len] = terms.intern(term);
                len += 1;
            }
            queries.push(Query::Bool {
                terms: tids,
                ops,
                len: len as u8,
            });
        } else if line.starts_with(b"PHRASE \"") && line.len() >= 9 {
            let mut p = PhrasePlan::default();
            for term in line[8..line.len() - 1].split(|&b| b == b' ') {
                let id = terms.intern(term);
                p.terms[p.len as usize] = id;
                p.keys[p.len as usize] = terms.keys[id as usize];
                p.len += 1;
            }
            queries.push(Query::Phrase {
                plan: plans.len() as u32,
            });
            plans.push(p);
        }
    }
    (queries, plans)
}

// ------------------------------------------------------------------ phrases

/// Checks a phrase from one known token occurrence of its term at phrase position
/// `anchor_pos`: walks back to the phrase start, then compares every token forward.
fn phrase_at(text: &[u8], anchor: usize, anchor_pos: usize, keys: &[(u64, u64)]) -> bool {
    let mut first = anchor;
    for _ in 0..anchor_pos {
        // Move across separators, then across the preceding token. Every non-ASCII byte is
        // a separator under the frozen byte-tokenization contract, including UTF-8 bytes.
        while first > 0 && mapped(text, first - 1) == 0 {
            first -= 1;
        }
        if first == 0 {
            return false;
        }
        while first > 0 && mapped(text, first - 1) != 0 {
            first -= 1;
        }
    }
    let mut matched = 0usize;
    scan(text, first, |_, token| {
        if token != Some(keys[matched]) {
            return false;
        }
        matched += 1;
        matched < keys.len()
    });
    matched == keys.len()
}

/// Occurrence of the rarest term at byte `p` as a whole token, plus an exact phrase match
/// for any of its phrase positions.
#[inline]
fn at(text: &[u8], p: usize, seed: &[u8], positions: u8, keys: &[(u64, u64)]) -> bool {
    let (n, l) = (text.len(), seed.len());
    if (p > 0 && text[p - 1].is_ascii_alphanumeric()) || p + l > n {
        return false;
    }
    if !text[p..p + l].iter().zip(seed).all(|(&x, &y)| x.to_ascii_lowercase() == y) {
        return false;
    }
    if p + l < n && text[p + l].is_ascii_alphanumeric() {
        return false;
    }
    let mut mask = positions;
    while mask != 0 {
        let at_pos = mask.trailing_zeros() as usize;
        mask &= mask - 1;
        if phrase_at(text, p, at_pos, keys) {
            return true;
        }
    }
    false
}

/// Finds the rarest term's occurrences in a candidate document with a 16-byte prefilter on its
/// first byte (case-folded), then checks each occurrence with `at`. The prefilter can pass
/// extra bytes but never misses a real occurrence.
fn verify(text: &[u8], seed: &[u8], positions: u8, keys: &[(u64, u64)]) -> bool {
    let (n, l) = (text.len(), seed.len());
    if l == 0 || l > n {
        return false;
    }
    let b0 = seed[0];
    let mut i = 0usize;
    // SAFETY: SSE2 is part of the x86-64 baseline; each load reads 16 bytes at i with i + 16 <= n.
    unsafe {
        let want = _mm_set1_epi8(b0 as i8);
        let fold = _mm_set1_epi8(0x20);
        while i + 16 <= n {
            let v = _mm_loadu_si128(text.as_ptr().add(i).cast::<__m128i>());
            let mut m = _mm_movemask_epi8(_mm_cmpeq_epi8(_mm_or_si128(v, fold), want)) as u32;
            while m != 0 {
                let j = m.trailing_zeros() as usize;
                m &= m - 1;
                if at(text, i + j, seed, positions, keys) {
                    return true;
                }
            }
            i += 16;
        }
    }
    while i < n {
        if (text[i] | 0x20) == b0 && at(text, i, seed, positions, keys) {
            return true;
        }
        i += 1;
    }
    false
}

// ------------------------------------------------------------- set algebra

fn merge_union(a: &[u16], b: &[u16], out: &mut Vec<u16>) {
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i] < b[j] {
            out.push(a[i]);
            i += 1;
        } else if a[i] > b[j] {
            out.push(b[j]);
            j += 1;
        } else {
            out.push(a[i]);
            i += 1;
            j += 1;
        }
    }
    out.extend_from_slice(&a[i..]);
    out.extend_from_slice(&b[j..]);
}

/// Sets (or ORs in) every member of `v` into the bitset `out`.
fn mark(v: SetView<'_>, out: &mut [u64]) {
    match v {
        SetView::List(l) => {
            for &x in l {
                out[x as usize >> 6] |= 1u64 << (x & 63);
            }
        }
        SetView::Bits(ws) => {
            for (o, &w) in out.iter_mut().zip(ws) {
                *o |= w;
            }
        }
    }
}

/// One binary BOOL operation (left op right). Writes a bitset into `ob` and returns true when
/// the result is a bitset, or writes a sorted list into `ol` and returns false.
fn combine(l: SetView<'_>, r: SetView<'_>, op: Op, w: usize, ob: &mut [u64], ol: &mut Vec<u16>) -> bool {
    match (op, l, r) {
        (Op::And, SetView::Bits(lw), SetView::Bits(rw)) => {
            for i in 0..w {
                ob[i] = lw[i] & rw[i];
            }
            true
        }
        (Op::And, SetView::Bits(_), SetView::List(rl)) => {
            ol.clear();
            ol.extend(rl.iter().copied().filter(|&x| l.contains(x)));
            false
        }
        (Op::And, SetView::List(ll), _) => {
            ol.clear();
            ol.extend(ll.iter().copied().filter(|&x| r.contains(x)));
            false
        }
        (Op::Or, SetView::List(a), SetView::List(b)) => {
            ol.clear();
            merge_union(a, b, ol);
            false
        }
        (Op::Or, _, _) => {
            ob.fill(0);
            mark(l, ob);
            mark(r, ob);
            true
        }
        (Op::Not, SetView::Bits(lw), SetView::Bits(rw)) => {
            for i in 0..w {
                ob[i] = lw[i] & !rw[i];
            }
            true
        }
        (Op::Not, SetView::Bits(lw), SetView::List(rl)) => {
            ob.copy_from_slice(lw);
            for &x in rl {
                ob[x as usize >> 6] &= !(1u64 << (x & 63));
            }
            true
        }
        (Op::Not, SetView::List(ll), _) => {
            ol.clear();
            ol.extend(ll.iter().copied().filter(|&x| !r.contains(x)));
            false
        }
    }
}

/// Reusable per-worker BOOL state: the current and the next result, as list or bitset.
struct Scratch {
    cl: Vec<u16>,
    tl: Vec<u16>,
    cb: Vec<u64>,
    tb: Vec<u64>,
}

#[derive(Clone, Copy)]
enum Cur {
    Term(u32),
    List,
    Bits,
}

/// One left-to-right BOOL step inside one shard.
fn step(cur: Cur, rhs: u32, op: Op, sh: &Shard, sc: &mut Scratch) -> Cur {
    let w = sh.words;
    let right = sh.view(rhs);
    let left = match cur {
        Cur::Term(t) => sh.view(t),
        Cur::List => SetView::List(&sc.cl),
        Cur::Bits => SetView::Bits(&sc.cb[..w]),
    };
    if combine(left, right, op, w, &mut sc.tb[..w], &mut sc.tl) {
        std::mem::swap(&mut sc.cb, &mut sc.tb);
        Cur::Bits
    } else {
        std::mem::swap(&mut sc.cl, &mut sc.tl);
        Cur::List
    }
}

// ------------------------------------------------------------------ results

/// Running result of one query across shards: count, id sum and the 10 smallest ids.
#[derive(Default)]
struct Acc10 {
    count: u64,
    sum: u64,
    len: usize,
    ids: [u32; 10],
}

impl Acc10 {
    fn push(&mut self, id: u32) {
        self.count += 1;
        self.sum += id as u64;
        if self.len < 10 {
            self.ids[self.len] = id;
            self.len += 1;
        }
    }

    /// Adds a shard's members (already offset by `base`). Shards are visited in id order, so
    /// the kept ids stay ascending. Bitset sums use popcounts: the position sum of a word is
    /// the sum over k of 2^k times the popcount of the positions whose bit k is set.
    fn add(&mut self, view: SetView<'_>, base: u32) {
        match view {
            SetView::List(l) => {
                for &x in l.iter().take(10 - self.len) {
                    self.ids[self.len] = base + x as u32;
                    self.len += 1;
                }
                self.count += l.len() as u64;
                self.sum += l.iter().map(|&x| x as u64).sum::<u64>() + base as u64 * l.len() as u64;
            }
            SetView::Bits(ws) => {
                for (i, &w) in ws.iter().enumerate() {
                    if w == 0 {
                        continue;
                    }
                    let first = base + 64 * i as u32;
                    let mut r = w;
                    while r != 0 && self.len < 10 {
                        self.ids[self.len] = first + r.trailing_zeros();
                        self.len += 1;
                        r &= r - 1;
                    }
                    let c = w.count_ones() as u64;
                    self.count += c;
                    self.sum += c * first as u64;
                    for (k, &m) in POS_MASK.iter().enumerate() {
                        self.sum += ((w & m).count_ones() as u64) << k;
                    }
                }
            }
        }
    }
}

fn write_acc(out: &mut Vec<u8>, a: &Acc10) {
    write!(out, "{} {}", a.count, a.sum).unwrap();
    for id in &a.ids[..a.len] {
        write!(out, " {id}").unwrap();
    }
    out.push(b'\n');
}

fn eval(q: &Query, plans: &[PhrasePlan], docs: &[u8], texts: &[([u8; 32], u8)], sh: &Shard, sc: &mut Scratch, a: &mut Acc10) {
    match *q {
        Query::Bool { terms, ops, len } => {
            let m = len as usize;
            let mut cur = Cur::Term(terms[0]);
            for i in 1..m {
                cur = step(cur, terms[i], ops[i - 1], sh, sc);
            }
            let view = match cur {
                Cur::Term(t) => sh.view(t),
                Cur::List => SetView::List(&sc.cl),
                Cur::Bits => SetView::Bits(&sc.cb[..sh.words]),
            };
            a.add(view, sh.base);
        }
        Query::Phrase { plan } => {
            let p = &plans[plan as usize];
            if p.dn == 0 {
                return;
            }
            let seed = p.dterms[0];
            let (seed_text, seed_len) = texts[seed as usize];
            let seed_bytes = &seed_text[..seed_len as usize];
            let keys = &p.keys[..p.len as usize];
            let others = &p.dterms[1..p.dn as usize];
            // Candidates: documents holding every distinct term, visited rarest-term first.
            sh.view(seed).each(|d| {
                if others.iter().all(|&t| sh.view(t).contains(d)) {
                    let (s, e) = sh.spans[d as usize];
                    if verify(&docs[s as usize..e as usize], seed_bytes, p.seed_positions, keys) {
                        a.push(sh.base + d as u32);
                    }
                }
            });
        }
    }
}

// --------------------------------------------------------------------- main

/// Splits the file into line-snapped byte chunks, one per worker (doubling the chunk count only
/// while a chunk holds more than MAX_DOCS documents). Returns the cuts and the document counts.
fn cut_and_count(docs: &[u8], threads: usize) -> (Vec<usize>, Vec<usize>) {
    let mut c = threads;
    loop {
        let mut cuts: Vec<usize> = (0..c)
            .map(|k| {
                let raw = docs.len() * k / c;
                match k {
                    0 => 0,
                    _ => docs[raw..]
                        .iter()
                        .position(|&b| b == b'\n')
                        .map_or(docs.len(), |p| raw + p + 1),
                }
            })
            .collect();
        cuts.push(docs.len());
        cuts.dedup();
        let counts = par_map(threads, cuts.len() - 1, || (), |i, _| {
            let part = &docs[cuts[i]..cuts[i + 1]];
            let mut n = part.iter().filter(|&&b| b == b'\n').count();
            if cuts[i + 1] == docs.len() && part.last() != Some(&b'\n') {
                n += 1; // final line without LF
            }
            n
        });
        if counts.iter().all(|&n| n <= MAX_DOCS) {
            return (cuts, counts);
        }
        c *= 2;
    }
}

fn load_queries() -> io::Result<(Terms, Vec<Query>, Vec<PhrasePlan>)> {
    let qb = fs::read("/data/queries.txt")?;
    let mut terms = Terms::default();
    let (queries, plans) = parse_queries(&qb, &mut terms);
    Ok((terms, queries, plans))
}

fn run(docs: &[u8]) -> io::Result<()> {
    let threads = thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(MAX_THREADS)
        .max(1);
    // The cuts only read the documents, so they run while this thread parses the queries.
    let (layout, loaded) = thread::scope(|s| {
        let cut = s.spawn(|| cut_and_count(docs, threads));
        let loaded = load_queries();
        (cut.join().unwrap(), loaded)
    });
    let (cuts, counts) = layout;
    let (mut terms, mut queries, mut plans) = loaded?;
    drop(std::mem::take(&mut terms.ids));
    let nterms = terms.keys.len();
    let n = counts.len();
    let mut bases = Vec::with_capacity(n);
    let mut total = 0u32;
    for &k in &counts {
        bases.push(total);
        total += k as u32;
    }

    // Vocabulary table, then the dense/sparse split from the first 1/32 of the file (as bytes,
    // snapped to line starts and cut into one piece per thread, so the sample is the same
    // contiguous prefix as before, counted in parallel). Dense terms get the lowest ids, so the
    // sample's ids are still the table's ids here.
    let mut vocab = Vocab::build(&terms.keys);
    let snap = |raw: usize| match raw {
        0 => 0,
        _ => docs[raw..].iter().position(|&b| b == b'\n').map_or(docs.len(), |p| raw + p + 1),
    };
    let pieces: Vec<usize> = (0..=threads).map(|k| snap(docs.len() / SAMPLE_DIV * k / threads)).collect();
    let sampled = par_map(threads, threads, || (), |i, _| {
        sample_counts(&docs[pieces[i]..pieces[i + 1]], &vocab, nterms)
    });
    let sample_n: usize = sampled.iter().map(|p| p.1).sum();
    let mut scount = vec![0u32; nterms];
    for (part, _) in &sampled {
        for (s, &c) in scount.iter_mut().zip(part) {
            *s += c;
        }
    }
    drop(sampled);
    let is_dense = |t: usize| scount[t] > 0 && scount[t] as usize * SAMPLE_DIV >= sample_n;
    let mut order: Vec<usize> = (0..nterms).collect();
    order.sort_unstable_by_key(|&t| (!is_dense(t), std::cmp::Reverse(scount[t]), t));
    let d_count = (0..nterms).filter(|&t| is_dense(t)).count().min(MAX_DENSE);

    // Renumber: order[new] = old. Table payloads, query terms and text all follow.
    let mut map = vec![0u32; nterms];
    for (new, &old) in order.iter().enumerate() {
        map[old] = new as u32;
    }
    vocab.remap(&map);
    let texts: Vec<([u8; 32], u8)> = order.iter().map(|&old| terms.texts[old]).collect();
    drop(terms);
    for q in &mut queries {
        if let Query::Bool { terms: t, len, .. } = q {
            for x in &mut t[..*len as usize] {
                *x = map[*x as usize];
            }
        }
    }
    for p in &mut plans {
        for x in &mut p.terms[..p.len as usize] {
            *x = map[*x as usize];
        }
    }

    // Index every shard on its own thread (no shared writes, no barrier).
    let mut shards: Vec<Shard> = par_map(threads, n, || (), |i, _| {
        index_shard(docs, cuts[i], cuts[i + 1], counts[i], &vocab, d_count, nterms)
    });
    drop(vocab);
    for (i, sh) in shards.iter_mut().enumerate() {
        sh.base = bases[i];
    }

    // Global document frequencies (exact; used for AND ordering and phrase seeds).
    let mut df = vec![0u32; nterms];
    for sh in &shards {
        for (t, d) in df.iter_mut().take(d_count).enumerate() {
            let row = &sh.bits[t * sh.words..(t + 1) * sh.words];
            *d += row.iter().map(|w| w.count_ones()).sum::<u32>();
        }
        for (d, w) in df[d_count..].iter_mut().zip(sh.offs.windows(2)) {
            *d += w[1] - w[0];
        }
    }

    // Reorder operands inside contiguous AND runs only. Intersection is commutative, and the
    // first operand of a run that follows OR/NOT stays where it is.
    for q in &mut queries {
        if let Query::Bool { terms: t, ops, len } = q {
            let m = *len as usize;
            let mut start = 0usize;
            while start < m {
                let mut end = start;
                while end + 1 < m && matches!(ops[end], Op::And) {
                    end += 1;
                }
                let lo = if start == 0 { 0 } else { start + 1 };
                if lo < end {
                    t[lo..=end].sort_unstable_by_key(|&x| (df[x as usize], x));
                }
                start = end + 1;
            }
        }
    }

    // Phrase plans: distinct terms rarest first; the seed is the rarest one.
    for p in &mut plans {
        let m = p.len as usize;
        let mut ds = p.terms;
        ds[..m].sort_unstable_by_key(|&t| (df[t as usize], t));
        let mut dn = 0usize;
        for i in 0..m {
            if i == 0 || ds[i] != ds[i - 1] {
                p.dterms[dn] = ds[i];
                dn += 1;
            }
        }
        p.dn = dn as u8;
        if dn == 0 {
            continue;
        }
        let seed = p.dterms[0];
        p.seed_positions = 0;
        for i in 0..m {
            if p.terms[i] == seed {
                p.seed_positions |= 1u8 << i;
            }
        }
    }

    // Queries: blocks of BLOCK queries, each evaluated on every shard in document order.
    let max_words = shards.iter().map(|s| s.words).max().unwrap_or(0).max(1);
    let nblocks = queries.len().div_ceil(BLOCK);
    let blocks = par_map(
        threads,
        nblocks,
        || Scratch {
            cl: Vec::new(),
            tl: Vec::new(),
            cb: vec![0u64; max_words],
            tb: vec![0u64; max_words],
        },
        |b, sc| {
            // Shard-major inside a block: one shard's lists stay cache-hot for all its queries.
            let qs = &queries[b * BLOCK..((b + 1) * BLOCK).min(queries.len())];
            let mut accs: Vec<Acc10> = qs.iter().map(|_| Acc10::default()).collect();
            for sh in &shards {
                for (q, a) in qs.iter().zip(accs.iter_mut()) {
                    eval(q, &plans, docs, &texts, sh, sc, a);
                }
            }
            let mut out = Vec::with_capacity(BLOCK * 48);
            for a in &accs {
                write_acc(&mut out, a);
            }
            out
        },
    );
    fs::create_dir_all("/out")?;
    let mut file = File::create("/out/results.txt")?;
    for b in &blocks {
        file.write_all(b)?;
    }
    file.flush()?;
    std::process::exit(0);
}

fn main() -> io::Result<()> {
    // SAFETY: PR_SET_THP_DISABLE (41) only changes this process's page-size policy, so scattered
    // small buffers stay as small as their contents; a failure is harmless and ignored.
    unsafe {
        prctl(41, 1u64, 0u64, 0u64, 0u64);
    }
    let file = File::open("/data/docs.txt")?;
    let len = usize::try_from(file.metadata()?.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "docs.txt too large"))?;
    if len == 0 {
        return run(&[]);
    }
    // SAFETY: len is nonzero and <= isize::MAX; the live input fd maps this range and MAP_FAILED is checked before forming the immutable slice.
    unsafe {
        // prot = PROT_READ (1), flags = MAP_PRIVATE (2)
        let ptr = mmap(std::ptr::null_mut(), len, 1, 2, file.as_raw_fd(), 0);
        if ptr as isize == -1 {
            return Err(io::Error::last_os_error());
        }
        run(std::slice::from_raw_parts(ptr.cast::<u8>(), len))
    }
}
