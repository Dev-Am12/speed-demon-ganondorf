use std::ffi::c_void;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

const MAX_THREADS: usize = 12;
const MAX_DOCS: usize = 65_534; // documents per shard (ids are u16, 0 is reserved for "unset")
const BLOCK: usize = 256; // queries per work item
const DENSE: u32 = 1 << 31; // TermRef.len flag: bitset instead of u16 list
const SKIP: u32 = u32::MAX; // TermRef.len: term not needed by any query path

// SAFETY: the declared mmap ABI uses only fixed-width POSIX arguments and returns an opaque pointer.
unsafe extern "C" {
    fn mmap(a: *mut c_void, len: usize, prot: i32, flags: i32, fd: i32, off: i64) -> *mut c_void;
}

#[derive(Clone, Copy)]
enum Op {
    And,
    Or,
    Not,
}

#[derive(Clone, Copy)]
enum Query {
    Bool {
        terms: [u32; 6],
        ops: [Op; 5],
        len: u8,
    },
    Phrase {
        plan: u32,
    },
}

/// Per-phrase plan: `seed` is the rarest term, `second` the next rarest distinct one.
#[derive(Clone, Copy, Default)]
struct PhrasePlan {
    terms: [u32; 8],
    len: u8,
    seed: u32,
    second: u32,
    keys: [(u64, u64); 8],
}

// ---------------------------------------------------------------- vocabulary

#[inline(always)]
fn mix64(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// Key of a query term: <=16 bytes are stored verbatim (lo, hi); 17..=32 bytes are hashed
/// exactly like the tokenizer hashes document tokens (hi always has bit 63 set then).
fn pack(term: &[u8]) -> (u64, u64) {
    if term.len() > 32 {
        return (0, 0); // can never equal a token key (lo of a real token is never 0)
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

/// Open-addressed table of the distinct query terms; the term id is the insertion index.
struct Vocab {
    slots: Vec<u32>,
    keys: Vec<(u64, u64)>,
}

impl Vocab {
    fn home(k: (u64, u64), mask: usize) -> usize {
        mix64(k.0 ^ k.1.rotate_left(29)) as usize & mask
    }

    fn place(slots: &mut [u32], keys: &[(u64, u64)], id: u32) {
        let mask = slots.len() - 1;
        let mut i = Self::home(keys[id as usize], mask);
        while slots[i] != u32::MAX {
            i = (i + 1) & mask;
        }
        slots[i] = id;
    }

    #[inline(always)]
    fn get(&self, k: (u64, u64)) -> Option<u32> {
        let mask = self.slots.len() - 1;
        let mut i = Self::home(k, mask);
        loop {
            // SAFETY: i is masked by slots.len() - 1 and slots has a power-of-two length.
            let id = unsafe { *self.slots.get_unchecked(i) };
            if id == u32::MAX {
                return None;
            }
            if self.keys[id as usize] == k {
                return Some(id);
            }
            i = (i + 1) & mask;
        }
    }

    fn intern(&mut self, term: &[u8]) -> u32 {
        let k = pack(term);
        if let Some(id) = self.get(k) {
            return id;
        }
        if (self.keys.len() + 1) * 2 > self.slots.len() {
            let mut next = vec![u32::MAX; self.slots.len() * 2];
            for id in 0..self.keys.len() as u32 {
                Self::place(&mut next, &self.keys, id);
            }
            self.slots = next;
        }
        self.keys.push(k);
        let id = self.keys.len() as u32 - 1;
        Self::place(&mut self.slots, &self.keys, id);
        id
    }
}

// ----------------------------------------------------------------- tokenizer

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
/// token longer than 32 bytes, which still occupies a position) and returns false to stop early.
/// Returns the index where the scan stopped: the LF position, or t.len().
#[inline(always)]
fn scan<F: FnMut(Option<(u64, u64)>) -> bool>(t: &[u8], mut i: usize, mut f: F) -> usize {
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
        if !f(key) {
            return n;
        }
        if i < n && t[i] == b'\n' {
            return i;
        }
        i += 1;
    }
    n
}

// ------------------------------------------------------------------- threads

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

// -------------------------------------------------------------------- shards

#[derive(Clone, Copy)]
struct TermRef {
    start: u32, // offset into the shard's arena (u16 units) or bitset storage (u64 words)
    len: u32,   // list length, DENSE | list length, or SKIP
}

#[derive(Clone, Copy)]
enum SetView<'a> {
    List(&'a [u16]),
    Bits(&'a [u64]),
}

impl SetView<'_> {
    #[inline(always)]
    fn contains(self, id: u16) -> bool {
        match self {
            Self::List(l) => l.binary_search(&id).is_ok(),
            Self::Bits(w) => w[id as usize >> 6] & (1u64 << (id & 63)) != 0,
        }
    }

    fn is_bits(self) -> bool {
        matches!(self, Self::Bits(_))
    }

    fn list_len(self) -> usize {
        match self {
            Self::List(l) => l.len(),
            Self::Bits(_) => 0,
        }
    }

    /// Calls `f` with every member in ascending order.
    fn each(self, mut f: impl FnMut(u16)) {
        match self {
            Self::List(l) => l.iter().for_each(|&d| f(d)),
            Self::Bits(ws) => {
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

/// One contiguous run of documents (at most MAX_DOCS) with its own posting lists and bitsets.
struct Shard {
    base: u32,              // global id of the shard's first document
    spans: Vec<(u32, u32)>, // per document: text start and end (absolute byte offsets)
    refs: Vec<TermRef>,
    arena: Vec<u16>,
    bits: Vec<u64>,
}

impl Shard {
    fn words(&self) -> usize {
        self.spans.len().div_ceil(64)
    }

    fn view(&self, t: u32) -> SetView<'_> {
        let r = self.refs[t as usize];
        let s = r.start as usize;
        if r.len & DENSE != 0 {
            SetView::Bits(&self.bits[s..s + self.words()])
        } else {
            SetView::List(&self.arena[s..s + r.len as usize])
        }
    }
}

/// Pass 1 over one shard's byte range: records document text spans and per-term document counts.
fn pass1(
    docs: &[u8],
    (a, b): (usize, usize),
    vocab: &Vocab,
    last: &mut [u16],
    ndocs: usize,
) -> (Vec<(u32, u32)>, Vec<u16>) {
    let mut spans = Vec::with_capacity(ndocs);
    let mut count = vec![0u16; vocab.keys.len()];
    last.fill(0);
    let mut pos = a;
    while pos < b {
        let doc = spans.len() as u16 + 1; // local id + 1; 0 means "not seen"
        let head = &docs[pos..(pos + 16).min(b)];
        let tab = head
            .iter()
            .position(|&x| x == b'\t' || x == b'\n')
            .filter(|&p| head[p] == b'\t')
            .map(|p| pos + p + 1);
        let end = match tab {
            Some(ts) => scan(&docs[..b], ts, |tok| {
                if let Some(t) = tok.and_then(|k| vocab.get(k)) {
                    let t = t as usize;
                    if last[t] != doc {
                        last[t] = doc;
                        count[t] += 1;
                    }
                }
                true
            }),
            None => docs[pos..b]
                .iter()
                .position(|&x| x == b'\n')
                .map_or(b, |p| pos + p),
        };
        spans.push((tab.unwrap_or(end) as u32, end as u32));
        pos = end + 1;
    }
    (spans, count)
}

/// Pass 2 over one shard: fills the exact-size arena (sparse terms) and bitsets (dense terms).
fn pass2(
    docs: &[u8],
    spans: &[(u32, u32)],
    vocab: &Vocab,
    refs: &[TermRef],
    arena: &mut [u16],
    bits: &mut [u64],
    cursor: &mut [u16],
) {
    cursor.fill(0);
    for (d, &(s, e)) in spans.iter().enumerate() {
        let d = d as u16;
        scan(&docs[s as usize..e as usize], 0, |tok| {
            if let Some(t) = tok.and_then(|k| vocab.get(k)) {
                let r = refs[t as usize];
                if r.len == SKIP {
                    return true;
                }
                if r.len & DENSE != 0 {
                    bits[r.start as usize + (d as usize >> 6)] |= 1u64 << (d & 63);
                } else {
                    let c = cursor[t as usize] as usize;
                    let p = r.start as usize + c;
                    if c == 0 || arena[p - 1] != d {
                        arena[p] = d;
                        cursor[t as usize] += 1;
                    }
                }
            }
            true
        });
    }
}

// ------------------------------------------------------------------- queries

fn parse_queries(qb: &[u8], vocab: &mut Vocab) -> (Vec<Query>, Vec<PhrasePlan>) {
    let mut queries = Vec::with_capacity(50_000);
    let mut plans = Vec::new();
    for mut line in qb.split(|&b| b == b'\n') {
        if line.ends_with(b"\r") {
            line = &line[..line.len() - 1];
        }
        if let Some(rest) = line.strip_prefix(b"BOOL ") {
            let mut it = rest.split(|&b| b == b' ');
            let mut terms = [0u32; 6];
            let mut ops = [Op::And; 5];
            terms[0] = vocab.intern(it.next().unwrap());
            let mut len = 1;
            while let (Some(op), Some(term)) = (it.next(), it.next()) {
                ops[len - 1] = match op {
                    b"AND" => Op::And,
                    b"OR" => Op::Or,
                    _ => Op::Not,
                };
                terms[len] = vocab.intern(term);
                len += 1;
            }
            queries.push(Query::Bool {
                terms,
                ops,
                len: len as u8,
            });
        } else if line.starts_with(b"PHRASE \"") && line.len() >= 9 {
            let mut p = PhrasePlan::default();
            for term in line[8..line.len() - 1].split(|&b| b == b' ') {
                p.terms[p.len as usize] = vocab.intern(term);
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

/// True if `text` contains the phrase keys as consecutive tokens (shift-and over token keys).
fn phrase_matches(text: &[u8], keys: &[(u64, u64)]) -> bool {
    let target = 1u8 << (keys.len() - 1);
    let mut state = 0u8;
    let mut hit = false;
    scan(text, 0, |tok| {
        let Some(k) = tok else {
            state = 0; // an over-long token occupies a position and breaks adjacency
            return true;
        };
        if state == 0 && k != keys[0] {
            return true;
        }
        let mut mask = 0u8;
        for (i, &pk) in keys.iter().enumerate() {
            if k == pk {
                mask |= 1u8 << i;
            }
        }
        state = ((state << 1) | 1) & mask;
        hit = state & target != 0;
        !hit
    });
    hit
}

/// Running result of one query across shards: count, id sum and the 10 smallest ids.
#[derive(Default)]
struct Acc10 {
    count: u64,
    sum: u64,
    len: usize,
    ids: [u32; 10],
}

/// Position weights: POS[k] has bit p set when bit k of p is set.
const POS: [u64; 6] = {
    let mut m = [0u64; 6];
    let mut k = 0usize;
    while k < 6 {
        let mut p = 0usize;
        while p < 64 {
            if (p >> k) & 1 == 1 {
                m[k] |= 1u64 << p;
            }
            p += 1;
        }
        k += 1;
    }
    m
};

impl Acc10 {
    #[inline(always)]
    fn push(&mut self, id: u32) {
        self.count += 1;
        self.sum += id as u64;
        if self.len < 10 {
            self.ids[self.len] = id;
            self.len += 1;
        }
    }

    /// Adds every set bit p of `w` as id base + p.
    fn push_word(&mut self, mut w: u64, base: u32) {
        if self.len < 10 {
            while w != 0 {
                self.push(base + w.trailing_zeros());
                w &= w - 1;
            }
        } else {
            self.count += w.count_ones() as u64;
            let mut s = base as u64 * w.count_ones() as u64;
            for (k, m) in POS.iter().enumerate() {
                s += ((w & m).count_ones() as u64) << k;
            }
            self.sum += s;
        }
    }

    fn add(&mut self, view: SetView<'_>, base: u32) {
        match view {
            SetView::List(l) => {
                for &id in l.iter().take(10 - self.len) {
                    self.ids[self.len] = base + id as u32;
                    self.len += 1;
                }
                self.count += l.len() as u64;
                self.sum += l.iter().map(|&x| x as u64).sum::<u64>() + base as u64 * l.len() as u64;
            }
            SetView::Bits(ws) => {
                for (i, &w) in ws.iter().enumerate() {
                    if w != 0 {
                        self.push_word(w, base + 64 * i as u32);
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

/// Reusable per-worker BOOL state: current and next list / bitset.
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

fn mark(view: SetView<'_>, out: &mut [u64], set: bool) {
    match view {
        SetView::List(l) => {
            for &id in l {
                let (w, b) = (id as usize >> 6, 1u64 << (id & 63));
                if set {
                    out[w] |= b;
                } else {
                    out[w] &= !b;
                }
            }
        }
        SetView::Bits(ws) => {
            for (d, &s) in out.iter_mut().zip(ws) {
                if set {
                    *d |= s;
                } else {
                    *d &= !s;
                }
            }
        }
    }
}

/// Merge of two sorted lists: intersection, or union when `union` is set.
fn merge(a: &[u16], b: &[u16], out: &mut Vec<u16>, union: bool) {
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        let (x, y) = (a[i], b[j]);
        if x == y {
            out.push(x);
        } else if union {
            out.push(x.min(y));
        }
        i += (x <= y) as usize;
        j += (y <= x) as usize;
    }
    if union {
        out.extend_from_slice(&a[i..]);
        out.extend_from_slice(&b[j..]);
    }
}

/// One left-to-right BOOL step: (current result) op (term rhs), inside one shard.
fn step(cur: Cur, rhs: u32, op: Op, sh: &Shard, sc: &mut Scratch) -> Cur {
    let words = sh.words();
    let right = sh.view(rhs);
    let left = match cur {
        Cur::Term(t) => sh.view(t),
        Cur::List => SetView::List(&sc.cl),
        Cur::Bits => SetView::Bits(&sc.cb[..words]),
    };
    let bits = match op {
        Op::And => left.is_bits() && right.is_bits(),
        Op::Or => {
            left.is_bits() || right.is_bits() || left.list_len() + right.list_len() >= words * 4
        }
        Op::Not => left.is_bits(),
    };
    if bits {
        let t = &mut sc.tb[..words];
        match (op, left, right) {
            (Op::And, SetView::Bits(a), SetView::Bits(b)) => {
                for ((d, &x), &y) in t.iter_mut().zip(a).zip(b) {
                    *d = x & y;
                }
            }
            _ => {
                t.fill(0);
                mark(left, t, true);
                mark(right, t, matches!(op, Op::Or));
            }
        }
        std::mem::swap(&mut sc.cb, &mut sc.tb);
        return Cur::Bits;
    }
    let out = &mut sc.tl;
    out.clear();
    match (op, left) {
        (Op::And, _) => {
            // At least one operand is a list here; filter the shorter one (or merge equals).
            let (s, o) = match (left, right) {
                (SetView::List(a), SetView::List(b)) if a.len() > b.len() => (b, left),
                (SetView::List(a), _) => (a, right),
                (_, SetView::List(b)) => (b, left),
                _ => unreachable!(),
            };
            match o {
                SetView::List(b) if b.len() <= 16 * s.len() => merge(s, b, out, false),
                _ => out.extend(s.iter().copied().filter(|&x| o.contains(x))),
            }
        }
        (Op::Or, SetView::List(a)) => {
            if let SetView::List(b) = right {
                merge(a, b, out, true);
            }
        }
        (_, SetView::List(a)) => out.extend(a.iter().copied().filter(|&x| !right.contains(x))),
        _ => {}
    }
    std::mem::swap(&mut sc.cl, &mut sc.tl);
    Cur::List
}

/// Evaluates one query on one shard and folds the result into `a`.
fn eval(q: &Query, plans: &[PhrasePlan], docs: &[u8], sh: &Shard, sc: &mut Scratch, a: &mut Acc10) {
    match q {
        Query::Bool { terms, ops, len } => {
            let mut cur = Cur::Term(terms[0]);
            for i in 1..*len as usize {
                cur = step(cur, terms[i], ops[i - 1], sh, sc);
            }
            let view = match cur {
                Cur::Term(t) => sh.view(t),
                Cur::List => SetView::List(&sc.cl),
                Cur::Bits => SetView::Bits(&sc.cb[..sh.words()]),
            };
            a.add(view, sh.base);
        }
        Query::Phrase { plan } => {
            let p = &plans[*plan as usize];
            let second = (p.second != u32::MAX).then(|| sh.view(p.second));
            let keys = &p.keys[..p.len as usize];
            sh.view(p.seed).each(|d| {
                if second.map_or(true, |s| s.contains(d)) {
                    let (s, e) = sh.spans[d as usize];
                    if phrase_matches(&docs[s as usize..e as usize], keys) {
                        a.push(sh.base + d as u32);
                    }
                }
            });
        }
    }
}

// ---------------------------------------------------------------------- main

fn run(docs: &[u8]) -> io::Result<()> {
    let qb = fs::read("/data/queries.txt")?;
    let mut vocab = Vocab {
        slots: vec![u32::MAX; 16],
        keys: Vec::new(),
    };
    let (queries, mut plans) = parse_queries(&qb, &mut vocab);
    drop(qb);
    let v = vocab.keys.len();
    let threads = thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(MAX_THREADS)
        .max(1);

    // Shards: byte chunks that start at line starts, doubled until each holds <= MAX_DOCS documents.
    let mut c = threads * 2;
    let (cuts, counts) = loop {
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
        let counts = par_map(
            threads,
            cuts.len() - 1,
            || (),
            |i, _| {
                let part = &docs[cuts[i]..cuts[i + 1]];
                let mut n = part.iter().filter(|&&b| b == b'\n').count();
                if cuts[i + 1] == docs.len() && part.last() != Some(&b'\n') {
                    n += 1; // final line without LF
                }
                n
            },
        );
        if counts.iter().all(|&n| n <= MAX_DOCS) {
            break (cuts, counts);
        }
        c *= 2;
    };
    let n = counts.len();
    let mut bases = Vec::with_capacity(n);
    let mut total = 0u32;
    for &k in &counts {
        bases.push(total);
        total += k as u32;
    }

    // Pass 1: spans and per-shard document frequencies.
    let (mut spans, counts1): (Vec<_>, Vec<_>) = par_map(
        threads,
        n,
        || vec![0u16; v],
        |i, last| pass1(docs, (cuts[i], cuts[i + 1]), &vocab, last, counts[i]),
    )
    .into_iter()
    .unzip();

    // Global decisions (serial, tiny): df, which terms to keep, phrase plans.
    let mut df = vec![0u32; v];
    for cnt in &counts1 {
        for (d, &x) in df.iter_mut().zip(cnt) {
            *d += x as u32;
        }
    }
    let mut keep = vec![false; v];
    for q in &queries {
        if let Query::Bool { terms, len, .. } = q {
            for &t in &terms[..*len as usize] {
                keep[t as usize] = true;
            }
        }
    }
    for p in &mut plans {
        let m = p.len as usize;
        let mut ord = p.terms;
        ord[..m].sort_unstable_by_key(|&t| (df[t as usize], t));
        let seed = ord[0];
        let second = ord[1..m].iter().copied().find(|&t| t != seed);
        p.seed = seed;
        p.second = second.unwrap_or(u32::MAX);
        keep[seed as usize] = true;
        if let Some(s) = second {
            keep[s as usize] = true;
        }
        for i in 0..m {
            p.keys[i] = vocab.keys[p.terms[i] as usize];
        }
    }

    // Layout: skip unneeded terms, bitset when dense (count*2 >= words*8 bytes), else u16 list.
    let layouts = par_map(
        threads,
        n,
        || (),
        |i, _| {
            let words = spans[i].len().div_ceil(64);
            let (mut sp, mut dn) = (0usize, 0usize);
            let refs: Vec<TermRef> = counts1[i]
                .iter()
                .zip(&keep)
                .map(|(&c, &k)| {
                    if !k {
                        TermRef {
                            start: 0,
                            len: SKIP,
                        }
                    } else if c as usize * 2 >= words * 8 {
                        dn += words;
                        TermRef {
                            start: (dn - words) as u32,
                            len: DENSE | c as u32,
                        }
                    } else {
                        sp += c as usize;
                        TermRef {
                            start: (sp - c as usize) as u32,
                            len: c as u32,
                        }
                    }
                })
                .collect();
            (refs, sp, dn)
        },
    );
    drop(counts1);

    // Pass 2: fill each shard's arena and bitsets (one thread per shard, no sharing).
    let bodies = par_map(
        threads,
        n,
        || vec![0u16; v],
        |i, cursor| {
            let (refs, sp, dn) = &layouts[i];
            let mut arena = vec![0u16; *sp];
            let mut bits = vec![0u64; *dn];
            pass2(docs, &spans[i], &vocab, refs, &mut arena, &mut bits, cursor);
            (arena, bits)
        },
    );
    let mut shards = Vec::with_capacity(n);
    for (i, ((arena, bits), (refs, _, _))) in bodies.into_iter().zip(layouts).enumerate() {
        shards.push(Shard {
            base: bases[i],
            spans: std::mem::take(&mut spans[i]),
            refs,
            arena,
            bits,
        });
    }

    // Queries: blocks of BLOCK queries, each evaluated on every shard in document order.
    let max_words = shards.iter().map(Shard::words).max().unwrap_or(0).max(1);
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
            let mut out = Vec::with_capacity(BLOCK * 48);
            for q in &queries[b * BLOCK..((b + 1) * BLOCK).min(queries.len())] {
                let mut a = Acc10::default();
                for sh in &shards {
                    eval(q, &plans, docs, sh, sc, &mut a);
                }
                write_acc(&mut out, &a);
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
