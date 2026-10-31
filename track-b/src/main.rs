use std::ffi::c_void;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

const MAX_THREADS: usize = 12;
const MAX_DOCS_PER_SHARD: usize = 65_534;
const QUERY_BLOCK_SIZE: usize = 256;
const PROT_READ: i32 = 1;
const MAP_PRIVATE: i32 = 2;
const DENSE_TAG: u32 = 1 << 31;
const SKIP_TAG: u32 = u32::MAX;

// SAFETY: the declared mmap ABI uses only fixed-width POSIX arguments and returns an opaque pointer.
unsafe extern "C" {
    fn mmap(
        address: *mut c_void,
        length: usize,
        protection: i32,
        flags: i32,
        fd: i32,
        offset: i64,
    ) -> *mut c_void;
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
        plan_idx: u32,
    },
}

#[derive(Clone, Copy, Default)]
struct PhrasePlan {
    seed: u32,
    second: u32,
    keys: [(u64, u64); 8],
    len: u8,
}

#[repr(C)]
struct Vocabulary {
    slots: Vec<u32>,
    keys: Vec<(u64, u64)>,
}

impl Vocabulary {
    fn new() -> Self {
        Self {
            slots: vec![u32::MAX; 16],
            keys: Vec::new(),
        }
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.keys.len()
    }

    #[inline(always)]
    fn key_of(&self, id: u32) -> (u64, u64) {
        self.keys[id as usize]
    }

    #[inline(always)]
    fn get_key(&self, lo: u64, hi: u64) -> Option<u32> {
        let mask = self.slots.len() - 1;
        let mut idx = mix64(lo ^ hi.rotate_left(29)) as usize & mask;
        loop {
            // SAFETY: idx is masked by slots.len() - 1, and slots has power-of-two length.
            let id = unsafe { *self.slots.get_unchecked(idx) };
            if id == u32::MAX {
                return None;
            }
            if self.keys[id as usize] == (lo, hi) {
                return Some(id);
            }
            idx = (idx + 1) & mask;
        }
    }

    fn intern(&mut self, term: &[u8]) -> u32 {
        let (lo, hi) = pack_term_bytes(term);
        if let Some(id) = self.get_key(lo, hi) {
            return id;
        }
        if (self.len() + 1) * 2 > self.slots.len() {
            let mut next = vec![u32::MAX; self.slots.len() * 2];
            let mask = next.len() - 1;
            for &old_id in &self.slots {
                if old_id != u32::MAX {
                    let (old_lo, old_hi) = self.keys[old_id as usize];
                    let mut idx = mix64(old_lo ^ old_hi.rotate_left(29)) as usize & mask;
                    while next[idx] != u32::MAX {
                        idx = (idx + 1) & mask;
                    }
                    next[idx] = old_id;
                }
            }
            self.slots = next;
        }
        let id = self.len() as u32;
        self.keys.push((lo, hi));
        let mask = self.slots.len() - 1;
        let mut idx = mix64(lo ^ hi.rotate_left(29)) as usize & mask;
        while self.slots[idx] != u32::MAX {
            idx = (idx + 1) & mask;
        }
        self.slots[idx] = id;
        id
    }
}

#[inline(always)]
fn mix64(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

#[inline(always)]
fn pack_term_bytes(term: &[u8]) -> (u64, u64) {
    let len = term.len();
    if len <= 16 {
        let mut buf = [0u8; 16];
        buf[..len].copy_from_slice(term);
        (
            u64::from_le_bytes([
                buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
            ]),
            u64::from_le_bytes([
                buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
            ]),
        )
    } else {
        let mut buf = [0u8; 32];
        buf[..len].copy_from_slice(term);
        let w0 = u64::from_le_bytes([
            buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
        ]);
        let w1 = u64::from_le_bytes([
            buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
        ]);
        let w2 = u64::from_le_bytes([
            buf[16], buf[17], buf[18], buf[19], buf[20], buf[21], buf[22], buf[23],
        ]);
        let w3 = u64::from_le_bytes([
            buf[24], buf[25], buf[26], buf[27], buf[28], buf[29], buf[30], buf[31],
        ]);
        let lo = mix64(w0 ^ w2.rotate_left(17) ^ (len as u64)).max(1);
        let hi = mix64(w1 ^ w3.rotate_left(37) ^ lo) | (1u64 << 63);
        (lo, hi)
    }
}

const fn make_byte_map() -> [u8; 256] {
    let mut map = [0u8; 256];
    let mut b = b'a';
    while b <= b'z' {
        map[b as usize] = b;
        b += 1;
    }
    b = b'A';
    while b <= b'Z' {
        map[b as usize] = b | 0x20;
        b += 1;
    }
    b = b'0';
    while b <= b'9' {
        map[b as usize] = b;
        b += 1;
    }
    map
}

const BYTE_MAP: [u8; 256] = make_byte_map();

#[inline(always)]
fn scan_doc_tokens<F>(text: &[u8], mut on_token: F)
where
    F: FnMut(Option<(u64, u64)>) -> bool,
{
    let mut i = 0usize;
    let n = text.len();
    while i < n {
        // SAFETY: the outer loop maintains i < text.len().
        let b0 = unsafe { BYTE_MAP[*text.get_unchecked(i) as usize] };
        if b0 == 0 {
            i += 1;
            continue;
        }
        let mut lo = b0 as u64;
        let mut shift = 8u32;
        i += 1;
        while i < n && shift < 64 {
            // SAFETY: the loop condition maintains i < text.len().
            let b = unsafe { BYTE_MAP[*text.get_unchecked(i) as usize] };
            if b == 0 {
                break;
            }
            lo |= (b as u64) << shift;
            shift += 8;
            i += 1;
        }
        // SAFETY: short-circuiting ensures i < n before indexing.
        if i == n || unsafe { BYTE_MAP[*text.get_unchecked(i) as usize] } == 0 {
            if !on_token(Some((lo, 0))) {
                return;
            }
            i += 1;
            continue;
        }
        let mut hi = 0u64;
        shift = 0;
        while i < n && shift < 64 {
            // SAFETY: the loop condition maintains i < text.len().
            let b = unsafe { BYTE_MAP[*text.get_unchecked(i) as usize] };
            if b == 0 {
                break;
            }
            hi |= (b as u64) << shift;
            shift += 8;
            i += 1;
        }
        // SAFETY: short-circuiting ensures i < n before indexing.
        if i == n || unsafe { BYTE_MAP[*text.get_unchecked(i) as usize] } == 0 {
            if !on_token(Some((lo, hi))) {
                return;
            }
            i += 1;
            continue;
        }
        let mut extra = [0u8; 16];
        let mut extra_len = 0usize;
        while i < n {
            // SAFETY: the loop condition maintains i < text.len().
            let b = unsafe { BYTE_MAP[*text.get_unchecked(i) as usize] };
            if b == 0 {
                break;
            }
            if extra_len < 16 {
                extra[extra_len] = b;
            }
            extra_len += 1;
            i += 1;
        }
        let keep = if extra_len <= 16 {
            let w2 = u64::from_le_bytes([
                extra[0], extra[1], extra[2], extra[3], extra[4], extra[5], extra[6], extra[7],
            ]);
            let w3 = u64::from_le_bytes([
                extra[8], extra[9], extra[10], extra[11], extra[12], extra[13], extra[14],
                extra[15],
            ]);
            let len = 16 + extra_len;
            let h_lo = mix64(lo ^ w2.rotate_left(17) ^ (len as u64)).max(1);
            let h_hi = mix64(hi ^ w3.rotate_left(37) ^ h_lo) | (1u64 << 63);
            on_token(Some((h_lo, h_hi)))
        } else {
            on_token(None)
        };
        if !keep {
            return;
        }
        i += 1;
    }
}

#[derive(Clone, Copy, Default)]
struct TermRef {
    start: u32,
    tagged_len: u32,
}

impl TermRef {
    #[inline(always)]
    fn skip() -> Self {
        Self {
            start: 0,
            tagged_len: SKIP_TAG,
        }
    }
    #[inline(always)]
    fn dense(start: u32, df: u32) -> Self {
        Self {
            start,
            tagged_len: DENSE_TAG | df,
        }
    }
    #[inline(always)]
    fn sparse(start: u32, df: u32) -> Self {
        Self {
            start,
            tagged_len: df,
        }
    }
    #[inline(always)]
    fn is_dense(self) -> bool {
        self.tagged_len != SKIP_TAG && (self.tagged_len & DENSE_TAG != 0)
    }
    #[inline(always)]
    fn df(self) -> usize {
        (self.tagged_len & !DENSE_TAG) as usize
    }
}

#[derive(Clone, Copy)]
enum SetView<'a> {
    List(&'a [u16]),
    Bits(&'a [u64]),
}

impl<'a> SetView<'a> {
    #[inline(always)]
    fn contains(self, id: u16) -> bool {
        match self {
            Self::List(ids) => ids.binary_search(&id).is_ok(),
            Self::Bits(words) => {
                let idx = id as usize;
                let word_idx = idx >> 6;
                if word_idx < words.len() {
                    (words[word_idx] & (1u64 << (idx & 63))) != 0
                } else {
                    false
                }
            }
        }
    }

    #[inline(always)]
    fn is_bits(self) -> bool {
        matches!(self, Self::Bits(_))
    }

    #[inline(always)]
    fn count(self) -> usize {
        match self {
            Self::List(ids) => ids.len(),
            Self::Bits(words) => words.iter().map(|w| w.count_ones() as usize).sum(),
        }
    }
}

struct Shard {
    byte_start: usize,
    byte_end: usize,
    base_id: u32,
    doc_count: usize,
    starts: Vec<u32>,
    count: Vec<u16>,
    refs: Vec<TermRef>,
    sparse_total: u32,
    dense_total: u32,
    arena: Vec<u16>,
    bitsets: Vec<u64>,
}

#[derive(Clone, Copy)]
struct ShardIndex<'a> {
    refs: &'a [TermRef],
    arena: &'a [u16],
    bitsets: &'a [u64],
    starts: &'a [u32],
    doc_count: usize,
    base_id: u32,
}

impl<'a> ShardIndex<'a> {
    #[inline(always)]
    fn view(&self, term_id: u32) -> SetView<'a> {
        let r = self.refs[term_id as usize];
        let start = r.start as usize;
        if r.is_dense() {
            let words = self.doc_count.div_ceil(64);
            SetView::Bits(&self.bitsets[start..start + words])
        } else {
            SetView::List(&self.arena[start..start + r.df()])
        }
    }
}

#[derive(Clone, Copy)]
enum Acc {
    Term(u32),
    List,
    Bits,
}

struct Scratch {
    cur_list: Vec<u16>,
    tmp_list: Vec<u16>,
    cur_bits: Vec<u64>,
    tmp_bits: Vec<u64>,
}

impl Scratch {
    fn new(max_words: usize) -> Self {
        Self {
            cur_list: Vec::with_capacity(1024),
            tmp_list: Vec::with_capacity(1024),
            cur_bits: vec![0u64; max_words.max(1)],
            tmp_bits: vec![0u64; max_words.max(1)],
        }
    }
}

fn add_to_bits(view: SetView<'_>, out: &mut [u64]) {
    match view {
        SetView::List(ids) => {
            for &id in ids {
                out[id as usize >> 6] |= 1u64 << (id & 63);
            }
        }
        SetView::Bits(words) => {
            for (d, &s) in out.iter_mut().zip(words) {
                *d |= s;
            }
        }
    }
}

fn remove_from_bits(view: SetView<'_>, out: &mut [u64]) {
    match view {
        SetView::List(ids) => {
            for &id in ids {
                out[id as usize >> 6] &= !(1u64 << (id & 63));
            }
        }
        SetView::Bits(words) => {
            for (d, &s) in out.iter_mut().zip(words) {
                *d &= !s;
            }
        }
    }
}

fn eval_step(
    acc: Acc,
    rhs_id: u32,
    op: Op,
    shard: &ShardIndex<'_>,
    scratch: &mut Scratch,
    threshold: usize,
) -> Acc {
    let right = shard.view(rhs_id);
    let left = match acc {
        Acc::Term(id) => shard.view(id),
        Acc::List => SetView::List(&scratch.cur_list),
        Acc::Bits => SetView::Bits(&scratch.cur_bits),
    };
    let out_bits = match op {
        Op::And => left.is_bits() && right.is_bits(),
        Op::Or => {
            left.is_bits()
                || right.is_bits()
                || left.count().saturating_add(right.count()) >= threshold
        }
        Op::Not => left.is_bits(),
    };
    let words = shard.doc_count.div_ceil(64);
    if out_bits {
        match op {
            Op::And => {
                if let (SetView::Bits(a), SetView::Bits(b)) = (left, right) {
                    for (d, (&x, &y)) in scratch.tmp_bits[..words]
                        .iter_mut()
                        .zip(a[..words].iter().zip(&b[..words]))
                    {
                        *d = x & y;
                    }
                }
            }
            Op::Or => {
                scratch.tmp_bits[..words].fill(0);
                add_to_bits(left, &mut scratch.tmp_bits[..words]);
                add_to_bits(right, &mut scratch.tmp_bits[..words]);
            }
            Op::Not => {
                scratch.tmp_bits[..words].fill(0);
                add_to_bits(left, &mut scratch.tmp_bits[..words]);
                remove_from_bits(right, &mut scratch.tmp_bits[..words]);
            }
        }
        let valid_bits = shard.doc_count & 63;
        if valid_bits != 0 {
            scratch.tmp_bits[words - 1] &= (1u64 << valid_bits) - 1;
        }
        std::mem::swap(&mut scratch.cur_bits, &mut scratch.tmp_bits);
        Acc::Bits
    } else {
        let out = &mut scratch.tmp_list;
        out.clear();
        match op {
            Op::And => {
                if let (SetView::List(a), SetView::List(b)) = (left, right) {
                    let (small, large) = if a.len() <= b.len() { (a, b) } else { (b, a) };
                    if !small.is_empty() && large.len() > 16 * small.len() {
                        let mut lo = 0usize;
                        for &x in small {
                            match large[lo..].binary_search(&x) {
                                Ok(p) => {
                                    out.push(x);
                                    lo += p + 1;
                                }
                                Err(p) => {
                                    lo += p;
                                    if lo == large.len() {
                                        break;
                                    }
                                }
                            }
                        }
                    } else {
                        let (mut i, mut j) = (0, 0);
                        while i < a.len() && j < b.len() {
                            if a[i] < b[j] {
                                i += 1;
                            } else if a[i] > b[j] {
                                j += 1;
                            } else {
                                out.push(a[i]);
                                i += 1;
                                j += 1;
                            }
                        }
                    }
                } else if let (SetView::List(ids), bits) | (bits, SetView::List(ids)) =
                    (left, right)
                {
                    out.extend(ids.iter().copied().filter(|&id| bits.contains(id)));
                }
            }
            Op::Or => {
                if let (SetView::List(a), SetView::List(b)) = (left, right) {
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
            }
            Op::Not => {
                if let SetView::List(ids) = left {
                    out.extend(ids.iter().copied().filter(|&id| !right.contains(id)));
                }
            }
        }
        std::mem::swap(&mut scratch.cur_list, &mut scratch.tmp_list);
        Acc::List
    }
}

fn push_u64(out: &mut Vec<u8>, mut v: u64) {
    let mut buf = [0u8; 20];
    let mut i = 20;
    loop {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    out.extend_from_slice(&buf[i..]);
}

#[derive(Clone, Copy, Default)]
struct QueryPart {
    count: u32,
    sum: u64,
    ids: [u16; 10],
    len: u8,
}

#[derive(Default)]
struct QueryAccum {
    count: u64,
    sum: u64,
    ids: [u32; 10],
    len: u8,
}

impl QueryAccum {
    fn add_shard(&mut self, part: QueryPart, base: u32) {
        self.count += part.count as u64;
        self.sum += part.sum + (base as u64) * (part.count as u64);
        let needed = (10 - self.len as usize).min(part.len as usize);
        for &id in &part.ids[..needed] {
            self.ids[self.len as usize] = base + id as u32;
            self.len += 1;
        }
    }
}

#[inline(always)]
fn word_bits_sum(w: u64, base: u64) -> u64 {
    const M0: u64 = 0xAAAA_AAAA_AAAA_AAAA;
    const M1: u64 = 0xCCCC_CCCC_CCCC_CCCC;
    const M2: u64 = 0xF0F0_F0F0_F0F0_F0F0;
    const M3: u64 = 0xFF00_FF00_FF00_FF00;
    const M4: u64 = 0xFFFF_0000_FFFF_0000;
    const M5: u64 = 0xFFFF_FFFF_0000_0000;

    let cnt = w.count_ones() as u64;
    let intra = (w & M0).count_ones() as u64
        + ((w & M1).count_ones() as u64) * 2
        + ((w & M2).count_ones() as u64) * 4
        + ((w & M3).count_ones() as u64) * 8
        + ((w & M4).count_ones() as u64) * 16
        + ((w & M5).count_ones() as u64) * 32;
    base * cnt + intra
}

fn result_from_view(view: SetView<'_>, doc_count: usize) -> QueryPart {
    let mut result = QueryPart::default();
    match view {
        SetView::List(ids) => {
            result.count = ids.len() as u32;
            let take = ids.len().min(10);
            result.ids[..take].copy_from_slice(&ids[..take]);
            result.len = take as u8;
            for &id in ids {
                result.sum += id as u64;
            }
        }
        SetView::Bits(words) => {
            let limit_words = doc_count.div_ceil(64);
            let active = &words[..limit_words.min(words.len())];
            for (wi, &word) in active.iter().enumerate() {
                if word == 0 {
                    continue;
                }
                let base = (wi * 64) as u64;
                if result.len < 10 {
                    let mut rem = word;
                    while rem != 0 {
                        let id = wi * 64 + rem.trailing_zeros() as usize;
                        if id < doc_count {
                            result.count += 1;
                            result.sum += id as u64;
                            if result.len < 10 {
                                result.ids[result.len as usize] = id as u16;
                                result.len += 1;
                            }
                        }
                        rem &= rem - 1;
                    }
                } else {
                    result.count += word.count_ones();
                    result.sum += word_bits_sum(word, base);
                }
            }
        }
    }
    result
}

fn write_accum(out: &mut Vec<u8>, result: &QueryAccum) {
    push_u64(out, result.count);
    out.push(b' ');
    push_u64(out, result.sum);
    for &id in &result.ids[..result.len as usize] {
        out.push(b' ');
        push_u64(out, id as u64);
    }
    out.push(b'\n');
}

#[inline(always)]
fn phrase_matches_text(text: &[u8], phrase_keys: &[(u64, u64)]) -> bool {
    let width = phrase_keys.len();
    let target_bit = 1u8 << (width - 1);
    let mut state = 0u8;
    let mut matched = false;
    let k0 = phrase_keys[0];

    scan_doc_tokens(text, |tok| {
        if let Some(k) = tok {
            if state == 0 {
                if k == k0 {
                    state = 1;
                    if width == 1 {
                        matched = true;
                        return false;
                    }
                }
                return true;
            }
            let mut mask = 0u8;
            for (off, &pk) in phrase_keys.iter().enumerate() {
                if k == pk {
                    mask |= 1u8 << off;
                }
            }
            state = ((state << 1) | 1) & mask;
            if (state & target_bit) != 0 {
                matched = true;
                return false;
            }
        } else {
            state = 0;
        }
        true
    });
    matched
}

fn parse_queries(
    query_bytes: &[u8],
    vocab: &mut Vocabulary,
    is_bool: &mut Vec<bool>,
    phrase_raw: &mut Vec<([u32; 8], u8)>,
) -> Vec<Query> {
    let mut queries = Vec::with_capacity(50_000);
    for mut line in query_bytes.split(|&b| b == b'\n') {
        if line.ends_with(b"\r") {
            line = &line[..line.len() - 1];
        }
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix(b"BOOL ") {
            let mut parts = rest.split(|&b| b == b' ');
            let mut terms = [0u32; 6];
            let mut ops = [Op::And; 5];
            let mut len = 0usize;
            if let Some(first) = parts.next() {
                let id = vocab.intern(first);
                if id as usize >= is_bool.len() {
                    is_bool.resize(vocab.len(), false);
                }
                is_bool[id as usize] = true;
                terms[len] = id;
                len += 1;
            }
            while let (Some(op), Some(term)) = (parts.next(), parts.next()) {
                ops[len - 1] = match op {
                    b"AND" => Op::And,
                    b"OR" => Op::Or,
                    _ => Op::Not,
                };
                let id = vocab.intern(term);
                if id as usize >= is_bool.len() {
                    is_bool.resize(vocab.len(), false);
                }
                is_bool[id as usize] = true;
                terms[len] = id;
                len += 1;
            }
            queries.push(Query::Bool {
                terms,
                ops,
                len: len as u8,
            });
        } else if line.starts_with(b"PHRASE \"") && line.len() >= 9 {
            let body = &line[8..line.len() - 1];
            let mut terms = [0u32; 8];
            let mut len = 0usize;
            for term in body.split(|&b| b == b' ') {
                let id = vocab.intern(term);
                if id as usize >= is_bool.len() {
                    is_bool.resize(vocab.len(), false);
                }
                terms[len] = id;
                len += 1;
            }
            let plan_idx = phrase_raw.len() as u32;
            phrase_raw.push((terms, len as u8));
            queries.push(Query::Phrase { plan_idx });
        }
    }
    queries
}

fn evaluate_query_shard(
    query: &Query,
    phrase_plans: &[PhrasePlan],
    docs: &[u8],
    shard: &ShardIndex<'_>,
    scratch: &mut Scratch,
    threshold: usize,
) -> QueryPart {
    match query {
        Query::Bool { terms, ops, len } => {
            let count = *len as usize;
            let mut acc = Acc::Term(terms[0]);
            for i in 0..count - 1 {
                acc = eval_step(acc, terms[i + 1], ops[i], shard, scratch, threshold);
            }
            let view = match acc {
                Acc::Term(id) => shard.view(id),
                Acc::List => SetView::List(&scratch.cur_list),
                Acc::Bits => SetView::Bits(&scratch.cur_bits),
            };
            result_from_view(view, shard.doc_count)
        }
        Query::Phrase { plan_idx } => {
            let plan = &phrase_plans[*plan_idx as usize];
            let seed_view = shard.view(plan.seed);
            if seed_view.count() == 0 {
                return QueryPart::default();
            }
            let second_view = if plan.second != u32::MAX {
                let sv = shard.view(plan.second);
                if sv.count() == 0 {
                    return QueryPart::default();
                }
                Some(sv)
            } else {
                None
            };
            let phrase_len = plan.len as usize;
            let mut part = QueryPart::default();

            let check_doc = |doc_id: u16| -> bool {
                let line_start = shard.starts[doc_id as usize] as usize;
                let next_start = shard.starts[doc_id as usize + 1] as usize;
                let actual_end = if next_start > line_start && docs[next_start - 1] == b'\n' {
                    next_start - 1
                } else {
                    next_start
                };
                let tab_limit = (line_start + 16).min(actual_end);
                let text_start = match docs[line_start..tab_limit]
                    .iter()
                    .position(|&byte| byte == b'\t')
                {
                    Some(p) => line_start + p + 1,
                    None => actual_end,
                };
                phrase_matches_text(&docs[text_start..actual_end], &plan.keys[..phrase_len])
            };

            let mut add_match = |doc_id: u16| {
                if check_doc(doc_id) {
                    part.count += 1;
                    part.sum += doc_id as u64;
                    if part.len < 10 {
                        part.ids[part.len as usize] = doc_id;
                        part.len += 1;
                    }
                }
            };

            match (seed_view, second_view) {
                (SetView::List(s1), None) => {
                    for &doc_id in s1 {
                        add_match(doc_id);
                    }
                }
                (SetView::Bits(words), None) => {
                    for (wi, &w0) in words.iter().enumerate() {
                        let mut rem = w0;
                        while rem != 0 {
                            let doc_id = (wi * 64 + rem.trailing_zeros() as usize) as u16;
                            if (doc_id as usize) < shard.doc_count {
                                add_match(doc_id);
                            }
                            rem &= rem - 1;
                        }
                    }
                }
                (SetView::List(s1), Some(SetView::List(s2))) => {
                    let mut i = 0;
                    let mut j = 0;
                    while i < s1.len() && j < s2.len() {
                        let d1 = s1[i];
                        let d2 = s2[j];
                        if d1 == d2 {
                            add_match(d1);
                            i += 1;
                            j += 1;
                        } else if d1 < d2 {
                            i += 1;
                        } else {
                            j += 1;
                        }
                    }
                }
                (SetView::List(s1), Some(SetView::Bits(s2_words))) => {
                    for &doc_id in s1 {
                        let idx = doc_id as usize;
                        if (s2_words[idx >> 6] & (1u64 << (idx & 63))) != 0 {
                            add_match(doc_id);
                        }
                    }
                }
                (SetView::Bits(s1_words), Some(SetView::List(s2))) => {
                    for &doc_id in s2 {
                        let idx = doc_id as usize;
                        if (s1_words[idx >> 6] & (1u64 << (idx & 63))) != 0 {
                            add_match(doc_id);
                        }
                    }
                }
                (SetView::Bits(s1_words), Some(SetView::Bits(s2_words))) => {
                    for (wi, (&w1, &w2)) in s1_words.iter().zip(s2_words).enumerate() {
                        let mut rem = w1 & w2;
                        while rem != 0 {
                            let doc_id = (wi * 64 + rem.trailing_zeros() as usize) as u16;
                            if (doc_id as usize) < shard.doc_count {
                                add_match(doc_id);
                            }
                            rem &= rem - 1;
                        }
                    }
                }
            }
            part
        }
    }
}

fn run(docs: &[u8]) -> io::Result<()> {
    let query_bytes = fs::read("/data/queries.txt")?;
    let mut vocab = Vocabulary::new();
    let mut bool_terms = Vec::new();
    let mut phrase_raw = Vec::new();
    let queries = parse_queries(&query_bytes, &mut vocab, &mut bool_terms, &mut phrase_raw);
    drop(query_bytes);

    let threads = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(MAX_THREADS)
        .max(1);

    // Scope 1: Newline count & shard determination
    let mut c = threads.max(12);
    let (chunk_ranges, chunk_doc_counts) = loop {
        let mut nominal_starts = Vec::with_capacity(c + 1);
        for k in 0..c {
            let raw = docs.len() * k / c;
            if k == 0 {
                nominal_starts.push(0);
            } else {
                let snap = match docs[raw..].iter().position(|&b| b == b'\n') {
                    Some(p) => raw + p + 1,
                    None => docs.len(),
                };
                nominal_starts.push(snap);
            }
        }
        nominal_starts.push(docs.len());
        nominal_starts.dedup();

        let num_spans = nominal_starts.len() - 1;
        let counts: Vec<usize> = thread::scope(|scope| {
            let handles: Vec<_> = (0..num_spans)
                .map(|i| {
                    let a = nominal_starts[i];
                    let b = nominal_starts[i + 1];
                    scope.spawn(move || {
                        if a >= b {
                            return 0;
                        }
                        let slice = &docs[a..b];
                        let mut cnt = slice.iter().filter(|&&byte| byte == b'\n').count();
                        if b == docs.len() && !docs.is_empty() && docs[docs.len() - 1] != b'\n' {
                            cnt += 1;
                        }
                        cnt
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });

        if counts.iter().any(|&cnt| cnt > MAX_DOCS_PER_SHARD) {
            c *= 2;
        } else {
            let mut ranges = Vec::new();
            let mut valid_counts = Vec::new();
            for i in 0..num_spans {
                if counts[i] > 0 {
                    ranges.push((nominal_starts[i], nominal_starts[i + 1]));
                    valid_counts.push(counts[i]);
                }
            }
            break (ranges, valid_counts);
        }
    };

    let mut shards: Vec<Shard> = Vec::with_capacity(chunk_ranges.len());
    let mut cur_base = 0u32;
    for (&(a, b), &cnt) in chunk_ranges.iter().zip(&chunk_doc_counts) {
        shards.push(Shard {
            byte_start: a,
            byte_end: b,
            base_id: cur_base,
            doc_count: cnt,
            starts: Vec::new(),
            count: Vec::new(),
            refs: Vec::new(),
            sparse_total: 0,
            dense_total: 0,
            arena: Vec::new(),
            bitsets: Vec::new(),
        });
        cur_base += cnt as u32;
    }

    // Scope 2: Pass 1
    let num_shards = shards.len();
    let next_shard = AtomicUsize::new(0);
    let pass1_results: Vec<Vec<(usize, Vec<u32>, Vec<u16>)>> = thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                let next_ref = &next_shard;
                let shards_ref = &shards;
                let vocab_ref = &vocab;
                scope.spawn(move || {
                    let mut my_results = Vec::new();
                    let mut last = vec![0u16; vocab_ref.len()];
                    loop {
                        let s_idx = next_ref.fetch_add(1, Ordering::Relaxed);
                        if s_idx >= num_shards {
                            break;
                        }
                        let shard = &shards_ref[s_idx];
                        let a = shard.byte_start;
                        let b = shard.byte_end;
                        let mut starts = Vec::with_capacity(shard.doc_count + 1);
                        let mut count = vec![0u16; vocab_ref.len()];
                        last.fill(0);

                        let mut pos = a;
                        let mut local_doc = 0u16;
                        while pos < b {
                            starts.push(pos as u32);
                            let line_len = docs[pos..b]
                                .iter()
                                .position(|&byte| byte == b'\n')
                                .map(|p| p + 1)
                                .unwrap_or(b - pos);
                            let next_pos = pos + line_len;
                            let actual_end = if next_pos > pos && docs[next_pos - 1] == b'\n' {
                                next_pos - 1
                            } else {
                                next_pos
                            };
                            let tab_limit = (pos + 16).min(actual_end);
                            let text_start =
                                match docs[pos..tab_limit].iter().position(|&byte| byte == b'\t') {
                                    Some(p) => pos + p + 1,
                                    None => actual_end,
                                };

                            scan_doc_tokens(&docs[text_start..actual_end], |tok| {
                                if let Some((lo, hi)) = tok {
                                    if let Some(id) = vocab_ref.get_key(lo, hi) {
                                        let tid = id as usize;
                                        let expected = local_doc + 1;
                                        if last[tid] != expected {
                                            last[tid] = expected;
                                            count[tid] += 1;
                                        }
                                    }
                                }
                                true
                            });
                            local_doc += 1;
                            pos = next_pos;
                        }
                        starts.push(b as u32);
                        my_results.push((s_idx, starts, count));
                    }
                    my_results
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    for thread_res in pass1_results {
        for (s_idx, starts, count) in thread_res {
            shards[s_idx].starts = starts;
            shards[s_idx].count = count;
        }
    }

    // Step C: Global decisions (serial)
    let mut df = vec![0u32; vocab.len()];
    for shard in &shards {
        for (t, &cnt) in shard.count.iter().enumerate() {
            df[t] += cnt as u32;
        }
    }

    let mut keep = bool_terms;
    keep.resize(vocab.len(), false);

    let mut phrase_plans = Vec::with_capacity(phrase_raw.len());
    for &(terms, len) in &phrase_raw {
        let flen = len as usize;
        let mut ord = terms;
        ord[..flen].sort_unstable_by_key(|&tid| (df[tid as usize], tid));
        let seed = ord[0];
        let mut second = u32::MAX;
        for &tid in &ord[1..flen] {
            if tid != seed {
                second = tid;
                break;
            }
        }
        keep[seed as usize] = true;
        if second != u32::MAX {
            keep[second as usize] = true;
        }
        let mut keys = [(0u64, 0u64); 8];
        for i in 0..flen {
            keys[i] = vocab.key_of(terms[i]);
        }
        phrase_plans.push(PhrasePlan {
            seed,
            second,
            keys,
            len,
        });
    }
    drop(phrase_raw);

    // Kept mask for fast L1 lookup in Pass 2
    let mut kept_mask = vec![0u64; vocab.len().div_ceil(64)];
    for (t, &k) in keep.iter().enumerate() {
        if k {
            kept_mask[t >> 6] |= 1u64 << (t & 63);
        }
    }

    // Scope 3: Per-shard layout
    thread::scope(|scope| {
        for shard in &mut shards {
            let keep_ref = &keep;
            let vlen = vocab.len();
            scope.spawn(move || {
                let words = shard.doc_count.div_ceil(64);
                let mut refs = vec![TermRef::default(); vlen];
                let mut sparse_offset = 0u32;
                let mut dense_offset = 0u32;
                for t in 0..vlen {
                    let cnt = shard.count[t];
                    if !keep_ref[t] {
                        refs[t] = TermRef::skip();
                    } else if (cnt as usize) * 2 >= words * 8 {
                        refs[t] = TermRef::dense(dense_offset, cnt as u32);
                        dense_offset += words as u32;
                    } else {
                        refs[t] = TermRef::sparse(sparse_offset, cnt as u32);
                        sparse_offset += cnt as u32;
                    }
                }
                shard.refs = refs;
                shard.sparse_total = sparse_offset;
                shard.dense_total = dense_offset;
                shard.count = Vec::new(); // Free pass-1 count memory
            });
        }
    });

    // Scope 4: Pass 2
    let next_shard2 = AtomicUsize::new(0);
    let pass2_results: Vec<Vec<(usize, Vec<u16>, Vec<u64>)>> = thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                let next_ref = &next_shard2;
                let shards_ref = &shards;
                let vocab_ref = &vocab;
                let mask_ref = &kept_mask;
                scope.spawn(move || {
                    let mut my_results = Vec::new();
                    let mut last = vec![0u16; vocab_ref.len()];
                    let mut cursor = vec![0u16; vocab_ref.len()];
                    loop {
                        let s_idx = next_ref.fetch_add(1, Ordering::Relaxed);
                        if s_idx >= num_shards {
                            break;
                        }
                        let shard = &shards_ref[s_idx];
                        let mut arena = vec![0u16; shard.sparse_total as usize];
                        let mut bitsets = vec![0u64; shard.dense_total as usize];
                        last.fill(0);
                        cursor.fill(0);

                        for local_doc in 0..shard.doc_count as u16 {
                            let line_start = shard.starts[local_doc as usize] as usize;
                            let next_start = shard.starts[local_doc as usize + 1] as usize;
                            let actual_end =
                                if next_start > line_start && docs[next_start - 1] == b'\n' {
                                    next_start - 1
                                } else {
                                    next_start
                                };
                            let tab_limit = (line_start + 16).min(actual_end);
                            let text_start = match docs[line_start..tab_limit]
                                .iter()
                                .position(|&byte| byte == b'\t')
                            {
                                Some(p) => line_start + p + 1,
                                None => actual_end,
                            };

                            scan_doc_tokens(&docs[text_start..actual_end], |tok| {
                                if let Some((lo, hi)) = tok {
                                    if let Some(id) = vocab_ref.get_key(lo, hi) {
                                        let tid = id as usize;
                                        if (mask_ref[tid >> 6] & (1u64 << (tid & 63))) != 0 {
                                            let expected = local_doc + 1;
                                            if last[tid] != expected {
                                                last[tid] = expected;
                                                let r = shard.refs[tid];
                                                if r.is_dense() {
                                                    let word_idx = (local_doc as usize) >> 6;
                                                    let bit = 1u64 << ((local_doc as usize) & 63);
                                                    bitsets[r.start as usize + word_idx] |= bit;
                                                } else {
                                                    let off = cursor[tid] as usize;
                                                    arena[r.start as usize + off] = local_doc;
                                                    cursor[tid] += 1;
                                                }
                                            }
                                        }
                                    }
                                }
                                true
                            });
                        }
                        my_results.push((s_idx, arena, bitsets));
                    }
                    my_results
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    for thread_res in pass2_results {
        for (s_idx, arena, bitsets) in thread_res {
            shards[s_idx].arena = arena;
            shards[s_idx].bitsets = bitsets;
        }
    }

    // Scope 5: Queries
    let shard_indexes: Vec<ShardIndex> = shards
        .iter()
        .map(|s| ShardIndex {
            refs: &s.refs,
            arena: &s.arena,
            bitsets: &s.bitsets,
            starts: &s.starts,
            doc_count: s.doc_count,
            base_id: s.base_id,
        })
        .collect();

    let num_queries = queries.len();
    let num_blocks = num_queries.div_ceil(QUERY_BLOCK_SIZE);
    let next_block = AtomicUsize::new(0);

    let max_words = shards
        .iter()
        .map(|s| s.doc_count.div_ceil(64))
        .max()
        .unwrap_or(0);

    let thread_blocks: Vec<Vec<(usize, Vec<u8>)>> = thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                let next_ref = &next_block;
                let queries_ref = &queries;
                let plans_ref = &phrase_plans;
                let shards_ref = &shard_indexes;
                scope.spawn(move || {
                    let mut my_blocks = Vec::new();
                    let mut scratch = Scratch::new(max_words);
                    loop {
                        let b = next_ref.fetch_add(1, Ordering::Relaxed);
                        if b >= num_blocks {
                            break;
                        }
                        let q_start = b * QUERY_BLOCK_SIZE;
                        let q_end = (q_start + QUERY_BLOCK_SIZE).min(num_queries);
                        let mut block_buf = Vec::with_capacity((q_end - q_start) * 48);

                        for q in &queries_ref[q_start..q_end] {
                            let mut accum = QueryAccum::default();
                            for s in shards_ref {
                                let threshold = s.doc_count.div_ceil(64) * 4;
                                let part = evaluate_query_shard(
                                    q,
                                    plans_ref,
                                    docs,
                                    s,
                                    &mut scratch,
                                    threshold,
                                );
                                accum.add_shard(part, s.base_id);
                            }
                            write_accum(&mut block_buf, &accum);
                        }
                        my_blocks.push((b, block_buf));
                    }
                    my_blocks
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut ordered_blocks: Vec<Vec<u8>> = vec![Vec::new(); num_blocks];
    for blocks in thread_blocks {
        for (b, buf) in blocks {
            ordered_blocks[b] = buf;
        }
    }

    fs::create_dir_all("/out")?;
    let mut file = File::create("/out/results.txt")?;
    for buf in &ordered_blocks {
        file.write_all(buf)?;
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
        let ptr = mmap(
            std::ptr::null_mut(),
            len,
            PROT_READ,
            MAP_PRIVATE,
            file.as_raw_fd(),
            0,
        );
        if ptr as isize == -1 {
            return Err(io::Error::last_os_error());
        }
        run(std::slice::from_raw_parts(ptr.cast::<u8>(), len))
    }
}
