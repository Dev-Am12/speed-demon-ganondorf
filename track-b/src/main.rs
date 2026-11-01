// Track B (Index): High-throughput query-indexed search engine.
// Optimized for Falkenstein i5-13500 (AVX2/BMI2, 12 worker threads, std-only).
use std::arch::x86_64::*;
use std::ffi::c_void;
use std::fs::{self, File};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::thread;

const MAX_THREADS: usize = 12;
const MAX_DOCS: usize = 65_534;
const BLOCK: usize = 512;
const DENSE: u32 = 1 << 31;
const SKIP: u32 = u32::MAX;

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
    Bool { terms: [u32; 6], ops: [Op; 5], len: u8 },
    Phrase { plan: u32 },
}

#[derive(Clone, Copy, Default)]
struct PhrasePlan {
    terms: [u32; 8],
    len: u8,
    seed: u32,
    second: u32,
    positional: bool,
    seed_positions: u8,
    keys: [(u64, u64); 8],
}

#[inline(always)]
fn mix64(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

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

struct Vocab {
    slots: Vec<u32>,
    keys: Vec<(u64, u64)>,
}

impl Vocab {
    #[inline(always)]
    fn home(k: (u64, u64), mask: usize) -> (usize, u32) {
        let mut x = k.0 ^ k.1.rotate_left(29);
        x ^= x >> 30;
        x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x ^= x >> 27;
        ((x as usize) & mask, (x >> 51) as u32)
    }

    fn place(slots: &mut [u32], keys: &[(u64, u64)], id: u32) {
        let mask = slots.len() - 1;
        let (mut i, tag) = Self::home(keys[id as usize], mask);
        while slots[i] != 0 {
            i = (i + 1) & mask;
        }
        slots[i] = (tag << 19) | (id + 1);
    }

    #[inline(always)]
    fn get(&self, k: (u64, u64)) -> Option<u32> {
        let mask = self.slots.len() - 1;
        let (mut i, tag) = Self::home(k, mask);
        loop {
            let e = unsafe { *self.slots.get_unchecked(i) };
            if e == 0 {
                return None;
            }
            if e >> 19 == tag {
                let id = (e & 0x7_FFFF) - 1;
                if unsafe { *self.keys.get_unchecked(id as usize) } == k {
                    return Some(id);
                }
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
            let mut next = vec![0u32; self.slots.len() * 2];
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
const BYTE_MAP: [u8; 256] = make_byte_map();

#[inline(always)]
fn mapped(t: &[u8], i: usize) -> u8 {
    unsafe { *BYTE_MAP.get_unchecked(*t.get_unchecked(i) as usize) }
}

#[inline(always)]
fn scan<F: FnMut(usize, Option<(u64, u64)>) -> bool>(t: &[u8], mut i: usize, mut f: F) -> usize {
    let n = t.len();
    while i < n {
        let c = mapped(t, i);
        if c == 0 {
            if unsafe { *t.get_unchecked(i) } == b'\n' {
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
        if i < n && unsafe { *t.get_unchecked(i) } == b'\n' {
            return i;
        }
        i += 1;
    }
    n
}

#[inline(always)]
unsafe fn classify64(p: *const u8) -> (u64, u64) {
    let v0 = _mm256_loadu_si256(p.cast());
    let v1 = _mm256_loadu_si256(p.add(32).cast());
    let alnum = |v| {
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

#[inline(always)]
unsafe fn token_key(t: &[u8], s: usize) -> Option<(u64, u64)> {
    if s + 16 <= t.len() {
        let v = _mm_loadu_si128(t.as_ptr().add(s).cast());
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
            let len = (!alnum).trailing_zeros() as usize;
            let lowered = _mm_or_si128(v, _mm_and_si128(up, _mm_set1_epi8(0x20)));
            let mut b = [0u8; 16];
            _mm_storeu_si128(b.as_mut_ptr().cast(), lowered);
            let w0 = u64::from_le_bytes(b[0..8].try_into().unwrap());
            let w1 = u64::from_le_bytes(b[8..16].try_into().unwrap());
            let lo = if len >= 8 { w0 } else { w0 & ((1u64 << (8 * len)) - 1) };
            let hi = if len <= 8 { 0 } else { w1 & ((1u64 << (8 * (len - 8))) - 1) };
            return Some((lo, hi));
        }
    }
    let end = s + t[s..].iter().take_while(|&&c| BYTE_MAP[c as usize] != 0).count();
    let mut key = None;
    scan(&t[s..end], 0, |_, k| {
        key = k;
        true
    });
    key
}

#[target_feature(enable = "avx2,bmi1,bmi2,popcnt")]
unsafe fn scan_avx2<F: FnMut(usize, Option<(u64, u64)>) -> bool>(
    t: &[u8],
    i0: usize,
    mut f: F,
) -> usize {
    let n = t.len();
    let mut prev = 0u64;
    let mut p = i0;
    let mut tail = [0u8; 64];
    while p < n {
        let (alnum, lf) = if p + 64 <= n {
            classify64(t.as_ptr().add(p))
        } else {
            let rem = n - p;
            tail[..rem].copy_from_slice(&t[p..]);
            tail[rem..].fill(0);
            classify64(tail.as_ptr())
        };
        let starts = alnum & !((alnum << 1) | prev);
        prev = alnum >> 63;
        let limit = if lf != 0 { lf.trailing_zeros() } else { 64 };
        let mut bits = if limit >= 64 { starts } else { starts & ((1u64 << limit) - 1) };
        while bits != 0 {
            let b = bits.trailing_zeros() as usize;
            bits &= bits - 1;
            let pos = p + b;
            if !f(pos, token_key(t, pos)) {
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

#[inline(always)]
fn scan_fast<F: FnMut(usize, Option<(u64, u64)>) -> bool>(t: &[u8], i0: usize, f: F) -> usize {
    unsafe { scan_avx2(t, i0, f) }
}

fn par_map<S, T: Send>(
    threads: usize,
    n: usize,
    init: impl Fn() -> S + Sync,
    f: impl Fn(usize, &mut S) -> T + Sync,
) -> Vec<T> {
    let next = AtomicUsize::new(0);
    let f = &f;
    let init = &init;
    let parts: Vec<Vec<(usize, T)>> = thread::scope(|s| {
        let hs: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let mut state = init();
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Relaxed);
                        if i >= n {
                            break out;
                        }
                        out.push((i, f(i, &mut state)));
                    }
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut all: Vec<(usize, T)> = parts.into_iter().flatten().collect();
    all.sort_unstable_by_key(|p| p.0);
    all.into_iter().map(|p| p.1).collect()
}

#[derive(Clone, Copy)]
struct TermRef {
    start: u32,
    len: u32,
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
            Self::Bits(w) => unsafe { w.get_unchecked(id as usize >> 6) } & (1u64 << (id & 63)) != 0,
        }
    }
    #[inline(always)]
    fn is_bits(self) -> bool {
        matches!(self, Self::Bits(_))
    }
    #[inline(always)]
    fn list_len(self) -> usize {
        match self {
            Self::List(l) => l.len(),
            Self::Bits(_) => 0,
        }
    }
    #[inline(always)]
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

#[derive(Clone, Copy)]
struct PosRef {
    term: u32,
    start: u32,
    len: u32,
}

struct Shard {
    base: u32,
    spans: Vec<(u32, u32)>,
    refs: Vec<TermRef>,
    arena: Vec<u16>,
    bits: Vec<u64>,
    pos_refs: Vec<PosRef>,
    positions: Vec<u64>,
}

impl Shard {
    #[inline(always)]
    fn words(&self) -> usize {
        self.spans.len().div_ceil(64)
    }
    #[inline(always)]
    fn view(&self, t: u32) -> SetView<'_> {
        let r = unsafe { *self.refs.get_unchecked(t as usize) };
        let s = r.start as usize;
        if r.len & DENSE != 0 {
            SetView::Bits(&self.bits[s..s + self.words()])
        } else {
            SetView::List(&self.arena[s..s + (r.len as usize)])
        }
    }
}

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
        let doc = spans.len() as u16 + 1;
        let head = &docs[pos..(pos + 16).min(b)];
        let tab = head
            .iter()
            .position(|&x| x == b'\t' || x == b'\n')
            .filter(|&p| head[p] == b'\t')
            .map(|p| pos + p + 1);
        let end = match tab {
            Some(ts) => scan_fast(&docs[..b], ts, |_, tok| {
                if let Some(t) = tok.and_then(|k| vocab.get(k)) {
                    let t = t as usize;
                    if unsafe { *last.get_unchecked(t) } != doc {
                        unsafe {
                            *last.get_unchecked_mut(t) = doc;
                            *count.get_unchecked_mut(t) += 1;
                        }
                    }
                }
                true
            }),
            None => docs[pos..b].iter().position(|&x| x == b'\n').map_or(b, |p| pos + p),
        };
        spans.push((tab.unwrap_or(end) as u32, end as u32));
        pos = end + 1;
    }
    (spans, count)
}

fn pass2(
    docs: &[u8],
    spans: &[(u32, u32)],
    vocab: &Vocab,
    refs: &[TermRef],
    arena: &mut [u16],
    bits: &mut [u64],
    cursor: &mut [u16],
    positional: &[bool],
    positions: &mut Vec<u64>,
) {
    cursor.fill(0);
    for (d, &(s, e)) in spans.iter().enumerate() {
        let d = d as u16;
        scan_fast(&docs[s as usize..e as usize], 0, |offset, tok| {
            if let Some(t) = tok.and_then(|k| vocab.get(k)) {
                let t = t as usize;
                if unsafe { *positional.get_unchecked(t) } {
                    positions.push(((t as u64) << 40) | ((d as u64) << 24) | offset as u64);
                }
                let r = unsafe { *refs.get_unchecked(t) };
                if r.len == SKIP {
                    return true;
                }
                if r.len & DENSE != 0 {
                    unsafe {
                        *bits.get_unchecked_mut(r.start as usize + (d as usize >> 6)) |= 1u64 << (d & 63);
                    }
                } else {
                    let c = unsafe { *cursor.get_unchecked(t) } as usize;
                    let p = r.start as usize + c;
                    if c == 0 || unsafe { *arena.get_unchecked(p - 1) } != d {
                        unsafe {
                            *arena.get_unchecked_mut(p) = d;
                            *cursor.get_unchecked_mut(t) += 1;
                        }
                    }
                }
            }
            true
        });
    }
}

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
            queries.push(Query::Bool { terms, ops, len: len as u8 });
        } else if line.starts_with(b"PHRASE \"") && line.len() >= 9 {
            let mut p = PhrasePlan::default();
            for term in line[8..line.len() - 1].split(|&b| b == b' ') {
                p.terms[p.len as usize] = vocab.intern(term);
                p.len += 1;
            }
            queries.push(Query::Phrase { plan: plans.len() as u32 });
            plans.push(p);
        }
    }
    (queries, plans)
}

fn phrase_at(text: &[u8], anchor: usize, anchor_pos: usize, keys: &[(u64, u64)]) -> bool {
    let mut first = anchor;
    for _ in 0..anchor_pos {
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

fn phrase_fallback(text: &[u8], keys: &[(u64, u64)]) -> bool {
    let mut state = 0u8;
    let target = 1u8 << (keys.len() - 1);
    let mut hit = false;
    scan(text, 0, |_, token| {
        let Some(key) = token else {
            state = 0;
            return true;
        };
        if state == 0 && key != keys[0] {
            return true;
        }
        let mut mask = 0u8;
        for (i, &expected) in keys.iter().enumerate() {
            if key == expected {
                mask |= 1u8 << i;
            }
        }
        state = ((state << 1) | 1) & mask;
        hit = state & target != 0;
        !hit
    });
    hit
}

fn make_pos_refs(events: &[u64]) -> Vec<PosRef> {
    let mut refs = Vec::new();
    let mut i = 0usize;
    while i < events.len() {
        let term = (events[i] >> 40) as u32;
        let start = i;
        while i < events.len() && (events[i] >> 40) as u32 == term {
            i += 1;
        }
        refs.push(PosRef {
            term,
            start: start as u32,
            len: (i - start) as u32,
        });
    }
    refs
}

#[derive(Default)]
struct Acc10 {
    count: u64,
    sum: u64,
    len: usize,
    ids: [u32; 10],
}

impl Acc10 {
    #[inline(always)]
    fn push(&mut self, id: u32) {
        self.count += 1;
        self.sum += id as u64;
        if self.len < 10 {
            unsafe { *self.ids.get_unchecked_mut(self.len) = id };
            self.len += 1;
        }
    }

    #[inline(always)]
    fn push_word(&mut self, mut w: u64, base: u32) {
        while w != 0 {
            self.push(base + w.trailing_zeros());
            w &= w - 1;
        }
    }

    fn add(&mut self, view: SetView<'_>, base: u32) {
        match view {
            SetView::List(l) => {
                for &id in l.iter().take(10 - self.len) {
                    unsafe { *self.ids.get_unchecked_mut(self.len) = base + id as u32 };
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
    let _ = write!(out, "{} {}", a.count, a.sum);
    for id in &a.ids[..a.len] {
        let _ = write!(out, " {id}");
    }
    out.push(b'\n');
}

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
                    unsafe { *out.get_unchecked_mut(w) |= b };
                } else {
                    unsafe { *out.get_unchecked_mut(w) &= !b };
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

fn merge(a: &[u16], b: &[u16], out: &mut Vec<u16>, union: bool) {
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        let (x, y) = (unsafe { *a.get_unchecked(i) }, unsafe { *b.get_unchecked(j) });
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
        Op::Or => left.is_bits() || right.is_bits() || left.list_len() + right.list_len() >= words * 4,
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
            (Op::Not, SetView::Bits(a), SetView::Bits(b)) => {
                for ((d, &x), &y) in t.iter_mut().zip(a).zip(b) {
                    *d = x & !y;
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
            let (s, o) = match (left, right) {
                (SetView::List(a), SetView::List(b)) if a.len() > b.len() => (b, left),
                (SetView::List(a), _) => (a, right),
                (_, SetView::List(b)) => (b, left),
                _ => unreachable!(),
            };
            match o {
                SetView::List(b) => merge(s, b, out, false),
                SetView::Bits(_) => out.extend(s.iter().copied().filter(|&x| o.contains(x))),
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

fn eval(q: &Query, plans: &[PhrasePlan], docs: &[u8], sh: &Shard, sc: &mut Scratch, a: &mut Acc10) {
    match q {
        Query::Bool { terms, ops, len } => {
            let m = *len as usize;
            let mut cur = Cur::Term(terms[0]);
            let mut i = 1usize;
            while i < m {
                cur = step(cur, terms[i], ops[i - 1], sh, sc);
                i += 1;
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
            if p.positional {
                let Ok(ridx) = sh.pos_refs.binary_search_by_key(&p.seed, |r| r.term) else {
                    return;
                };
                let r = unsafe { *sh.pos_refs.get_unchecked(ridx) };
                let events = &sh.positions[r.start as usize..(r.start + r.len) as usize];
                let mut i = 0usize;
                while i < events.len() {
                    let doc = ((events[i] >> 24) & 0xffff) as u16;
                    let mut end = i + 1;
                    while end < events.len() && ((events[end] >> 24) & 0xffff) as u16 == doc {
                        end += 1;
                    }
                    if second.map_or(true, |set| set.contains(doc)) {
                        let (s, e) = unsafe { *sh.spans.get_unchecked(doc as usize) };
                        let text = &docs[s as usize..e as usize];
                        let mut found = false;
                        for &event in &events[i..end] {
                            let offset = (event & 0x00ff_ffff) as usize;
                            let mut mask = p.seed_positions;
                            while mask != 0 {
                                let at = mask.trailing_zeros() as usize;
                                if phrase_at(text, offset, at, keys) {
                                    found = true;
                                    break;
                                }
                                mask &= mask - 1;
                            }
                            if found {
                                break;
                            }
                        }
                        if found {
                            a.push(sh.base + doc as u32);
                        }
                    }
                    i = end;
                }
            } else {
                sh.view(p.seed).each(|d| {
                    if second.map_or(true, |set| set.contains(d)) {
                        let (s, e) = unsafe { *sh.spans.get_unchecked(d as usize) };
                        if phrase_fallback(&docs[s as usize..e as usize], keys) {
                            a.push(sh.base + d as u32);
                        }
                    }
                });
            }
        }
    }
}

fn run(docs: &[u8]) -> std::io::Result<()> {
    let qb = fs::read("/data/queries.txt")?;
    let mut vocab = Vocab {
        slots: vec![0u32; 16],
        keys: Vec::new(),
    };
    let (mut queries, mut plans) = parse_queries(&qb, &mut vocab);
    drop(qb);
    let v = vocab.keys.len();
    let threads = thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(MAX_THREADS)
        .max(1);

    let mut c = threads;
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
                    n += 1;
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

    let (mut spans, counts1): (Vec<_>, Vec<_>) = par_map(
        threads,
        n,
        || vec![0u16; v],
        |i, last| pass1(docs, (cuts[i], cuts[i + 1]), &vocab, last, counts[i]),
    )
    .into_iter()
    .unzip();

    let mut df = vec![0u32; v];
    for cnt in &counts1 {
        for (d, &x) in df.iter_mut().zip(cnt) {
            *d += x as u32;
        }
    }

    let mut keep = vec![false; v];
    let mut positional = vec![false; v];
    for q in &mut queries {
        if let Query::Bool { terms, ops, len } = q {
            let m = *len as usize;
            for &t in &terms[..m] {
                keep[t as usize] = true;
            }
            let mut start = 0usize;
            while start < m {
                let mut end = start;
                while end + 1 < m && matches!(ops[end], Op::And) {
                    end += 1;
                }
                let sort_from = if start == 0 { start } else { start + 1 };
                if sort_from < end {
                    terms[sort_from..=end].sort_unstable_by_key(|&t| (df[t as usize], t));
                } else if start == 0 && sort_from == end {
                    terms[start..=end].sort_unstable_by_key(|&t| (df[t as usize], t));
                }
                start = end + 1;
            }
        }
    }

    let rare_anchor_limit = total as usize / 1000;
    for p in &mut plans {
        let m = p.len as usize;
        let mut ord = p.terms;
        ord[..m].sort_unstable_by_key(|&t| (df[t as usize], t));
        let seed = ord[0];
        let second = ord[1..m].iter().copied().find(|&t| t != seed);
        p.seed = seed;
        p.second = second.unwrap_or(u32::MAX);
        p.seed_positions = 0;
        for i in 0..m {
            if p.terms[i] == seed {
                p.seed_positions |= 1u8 << i;
            }
        }
        p.positional = df[seed as usize] <= rare_anchor_limit as u32;
        if p.positional {
            positional[seed as usize] = true;
        } else {
            keep[seed as usize] = true;
        }
        if let Some(s) = second {
            keep[s as usize] = true;
        }
        for i in 0..m {
            p.keys[i] = vocab.keys[p.terms[i] as usize];
        }
    }

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
                        TermRef { start: 0, len: SKIP }
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

    let bodies = par_map(
        threads,
        n,
        || vec![0u16; v],
        |i, cursor| {
            let (refs, sp, dn) = &layouts[i];
            let mut arena = vec![0u16; *sp];
            let mut bits = vec![0u64; *dn];
            let mut positions = Vec::new();
            pass2(
                docs,
                &spans[i],
                &vocab,
                refs,
                &mut arena,
                &mut bits,
                cursor,
                &positional,
                &mut positions,
            );
            positions.sort_unstable();
            (arena, bits, positions)
        },
    );

    let mut shards = Vec::with_capacity(n);
    for (i, ((arena, bits, positions), (refs, _, _))) in bodies.into_iter().zip(layouts).enumerate() {
        let pos_refs = make_pos_refs(&positions);
        shards.push(Shard {
            base: bases[i],
            spans: std::mem::take(&mut spans[i]),
            refs,
            arena,
            bits,
            pos_refs,
            positions,
        });
    }

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

fn main() -> std::io::Result<()> {
    let file = File::open("/data/docs.txt")?;
    let len = usize::try_from(file.metadata()?.len())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "docs.txt too large"))?;
    if len == 0 {
        return run(&[]);
    }
    unsafe {
        let ptr = mmap(std::ptr::null_mut(), len, 1, 2, file.as_raw_fd(), 0);
        if ptr as isize == -1 {
            return Err(std::io::Error::last_os_error());
        }
        run(std::slice::from_raw_parts(ptr.cast::<u8>(), len))
    }
}
