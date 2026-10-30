use std::ffi::c_void;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::thread;

const MAX_TERM_LEN: usize = 32;
const MAX_THREADS: usize = 12;
const QUERY_BLOCK_SIZE: usize = 256;
const PROT_READ: i32 = 1;
const MAP_PRIVATE: i32 = 2;
const DENSE_TAG: u32 = 1 << 31;
const SKIP_TAG: u32 = u32::MAX;

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

enum Query {
    Bool { terms: Vec<u32>, ops: Vec<Op> },
    Phrase(Vec<u32>),
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct VocabSlot {
    lo: u64,
    hi: u64,
    id: u32,
    pad: u32,
}

struct Vocabulary {
    slots: Vec<VocabSlot>,
    keys: Vec<(u64, u64)>,
}

impl Vocabulary {
    fn new() -> Self {
        Self {
            slots: vec![VocabSlot::default(); 16],
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
            let s = unsafe { *self.slots.get_unchecked(idx) };
            if s.lo == 0 {
                return None;
            }
            if s.lo == lo && s.hi == hi {
                return Some(s.id);
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
            let mut next = vec![VocabSlot::default(); self.slots.len() * 2];
            let mask = next.len() - 1;
            for s in &self.slots {
                if s.lo != 0 {
                    let mut idx = mix64(s.lo ^ s.hi.rotate_left(29)) as usize & mask;
                    while next[idx].lo != 0 {
                        idx = (idx + 1) & mask;
                    }
                    next[idx] = *s;
                }
            }
            self.slots = next;
        }
        let id = self.len() as u32;
        self.keys.push((lo, hi));
        let mask = self.slots.len() - 1;
        let mut idx = mix64(lo ^ hi.rotate_left(29)) as usize & mask;
        while self.slots[idx].lo != 0 {
            idx = (idx + 1) & mask;
        }
        self.slots[idx] = VocabSlot { lo, hi, id, pad: 0 };
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
            u64::from_le_bytes([buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]]),
            u64::from_le_bytes([buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15]]),
        )
    } else {
        let mut buf = [0u8; 32];
        buf[..len].copy_from_slice(term);
        let w0 = u64::from_le_bytes([buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]]);
        let w1 = u64::from_le_bytes([buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15]]);
        let w2 = u64::from_le_bytes([buf[16], buf[17], buf[18], buf[19], buf[20], buf[21], buf[22], buf[23]]);
        let w3 = u64::from_le_bytes([buf[24], buf[25], buf[26], buf[27], buf[28], buf[29], buf[30], buf[31]]);
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

#[repr(C)]
#[derive(Clone, Copy)]
struct TermState {
    last_seen: u32,
    value: u32,
}

struct ChunkCount {
    states: Vec<TermState>,
    text_ranges: Vec<(u32, u32)>,
}

#[inline(always)]
fn scan_doc_tokens<F>(text: &[u8], mut on_token: F)
where
    F: FnMut(Option<(u64, u64)>) -> bool,
{
    let mut i = 0usize;
    let n = text.len();
    while i < n {
        let b0 = unsafe { BYTE_MAP[*text.get_unchecked(i) as usize] };
        if b0 == 0 {
            i += 1;
            continue;
        }
        let mut lo = b0 as u64;
        let mut shift = 8u32;
        i += 1;
        while i < n && shift < 64 {
            let b = unsafe { BYTE_MAP[*text.get_unchecked(i) as usize] };
            if b == 0 {
                break;
            }
            lo |= (b as u64) << shift;
            shift += 8;
            i += 1;
        }
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
            let b = unsafe { BYTE_MAP[*text.get_unchecked(i) as usize] };
            if b == 0 {
                break;
            }
            hi |= (b as u64) << shift;
            shift += 8;
            i += 1;
        }
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
                extra[8], extra[9], extra[10], extra[11], extra[12], extra[13], extra[14], extra[15],
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

fn scan_chunk_count(bytes: &[u8], abs_start: usize, vocab: &Vocabulary) -> ChunkCount {
    let mut states = vec![
        TermState {
            last_seen: u32::MAX,
            value: 0,
        };
        vocab.len()
    ];
    let mut text_ranges = Vec::with_capacity(bytes.len() / 900 + 16);
    let mut pos = 0usize;
    let mut local_doc = 0u32;
    while pos < bytes.len() {
        let mut tab_pos = None;
        while pos < bytes.len() && bytes[pos] != b'\n' {
            if tab_pos.is_none() && bytes[pos] == b'\t' {
                tab_pos = Some(pos);
            }
            pos += 1;
        }
        let line_end = pos;
        if pos < bytes.len() {
            pos += 1;
        }
        let text_start = tab_pos.map(|t| t + 1).unwrap_or(line_end);
        text_ranges.push(((abs_start + text_start) as u32, (abs_start + line_end) as u32));
        if text_start < line_end {
            scan_doc_tokens(&bytes[text_start..line_end], |tok| {
                if let Some((lo, hi)) = tok {
                    if let Some(id) = vocab.get_key(lo, hi) {
                        let st = unsafe { states.get_unchecked_mut(id as usize) };
                        if st.last_seen != local_doc {
                            st.last_seen = local_doc;
                            st.value += 1;
                        }
                    }
                }
                true
            });
        }
        local_doc += 1;
    }
    ChunkCount { states, text_ranges }
}

fn scan_chunk_write(
    docs: &[u8],
    text_ranges: &[(u32, u32)],
    vocab: &Vocabulary,
    global_doc_start: u32,
    term_bases: &[u32],
    term_refs: &[TermRef],
    sparse: &[AtomicU32],
    bitsets: &[AtomicU64],
) {
    let mut states = vec![
        TermState {
            last_seen: u32::MAX,
            value: 0,
        };
        vocab.len()
    ];
    for (idx, &(t_start, t_end)) in text_ranges.iter().enumerate() {
        let local_doc = idx as u32;
        let doc_id = global_doc_start + local_doc;
        if t_start < t_end {
            scan_doc_tokens(&docs[t_start as usize..t_end as usize], |tok| {
                if let Some((lo, hi)) = tok {
                    if let Some(id) = vocab.get_key(lo, hi) {
                        let t_idx = id as usize;
                        let st = unsafe { states.get_unchecked_mut(t_idx) };
                        if st.last_seen != local_doc {
                            st.last_seen = local_doc;
                            let tref = unsafe { *term_refs.get_unchecked(t_idx) };
                            if tref.is_skip() {
                                return true;
                            }
                            if tref.is_dense() {
                                let slot = tref.start as usize + (doc_id as usize >> 6);
                                unsafe {
                                    bitsets
                                        .get_unchecked(slot)
                                        .fetch_or(1u64 << (doc_id & 63), Ordering::Relaxed);
                                }
                            } else {
                                let slot = unsafe { *term_bases.get_unchecked(t_idx) } + st.value;
                                st.value += 1;
                                unsafe {
                                    sparse
                                        .get_unchecked(slot as usize)
                                        .store(doc_id, Ordering::Relaxed);
                                }
                            }
                        }
                    }
                }
                true
            });
        }
    }
}

fn line_chunks(bytes: &[u8], count: usize) -> Vec<(usize, usize)> {
    let mut starts = vec![0usize; count + 1];
    for i in 1..count {
        let nominal = ((bytes.len() as u128 * i as u128) / count as u128) as usize;
        starts[i] = if nominal == 0 {
            0
        } else {
            bytes[nominal - 1..]
                .iter()
                .position(|&b| b == b'\n')
                .map(|off| nominal + off)
                .unwrap_or(bytes.len())
        };
    }
    starts[count] = bytes.len();
    starts.windows(2).map(|w| (w[0], w[1])).collect()
}

#[derive(Clone, Copy)]
struct TermRef {
    start: u32,
    tagged_len: u32,
}

impl TermRef {
    #[inline(always)]
    fn is_skip(self) -> bool {
        self.tagged_len == SKIP_TAG
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
    List(&'a [u32]),
    Bits(&'a [u64]),
}

impl SetView<'_> {
    #[inline(always)]
    fn contains(self, id: u32) -> bool {
        match self {
            Self::List(ids) => ids.binary_search(&id).is_ok(),
            Self::Bits(words) => {
                let idx = id as usize;
                (unsafe { *words.get_unchecked(idx >> 6) } & (1u64 << (idx & 63))) != 0
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

struct PostingIndex {
    sparse: Vec<u32>,
    bitsets: Vec<u64>,
    sets: Vec<TermRef>,
    doc_count: usize,
}

impl PostingIndex {
    #[inline(always)]
    fn view(&self, term_id: u32) -> SetView<'_> {
        let r = self.sets[term_id as usize];
        let start = r.start as usize;
        if r.is_dense() {
            let words = self.doc_count.div_ceil(64);
            SetView::Bits(&self.bitsets[start..start + words])
        } else {
            SetView::List(&self.sparse[start..start + r.df()])
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
    cur_list: Vec<u32>,
    tmp_list: Vec<u32>,
    cur_bits: Vec<u64>,
    tmp_bits: Vec<u64>,
}

impl Scratch {
    fn new(words: usize) -> Self {
        Self {
            cur_list: Vec::with_capacity(1024),
            tmp_list: Vec::with_capacity(1024),
            cur_bits: vec![0u64; words],
            tmp_bits: vec![0u64; words],
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

fn eval_step(acc: Acc, rhs_id: u32, op: Op, postings: &PostingIndex, scratch: &mut Scratch, threshold: usize) -> Acc {
    let right = postings.view(rhs_id);
    let left = match acc {
        Acc::Term(id) => postings.view(id),
        Acc::List => SetView::List(&scratch.cur_list),
        Acc::Bits => SetView::Bits(&scratch.cur_bits),
    };
    let out_bits = match op {
        Op::And => left.is_bits() && right.is_bits(),
        Op::Or => left.is_bits() || right.is_bits() || left.count().saturating_add(right.count()) >= threshold,
        Op::Not => left.is_bits(),
    };
    if out_bits {
        let out = &mut scratch.tmp_bits;
        match op {
            Op::And => {
                if let (SetView::Bits(a), SetView::Bits(b)) = (left, right) {
                    for (d, (&x, &y)) in out.iter_mut().zip(a.iter().zip(b)) {
                        *d = x & y;
                    }
                }
            }
            Op::Or => {
                out.fill(0);
                add_to_bits(left, out);
                add_to_bits(right, out);
            }
            Op::Not => {
                out.fill(0);
                add_to_bits(left, out);
                remove_from_bits(right, out);
            }
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
                } else if let (SetView::List(ids), bits) | (bits, SetView::List(ids)) = (left, right) {
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

fn append_view_result(out: &mut Vec<u8>, view: SetView<'_>, doc_count: usize) {
    let mut sum = 0u64;
    let mut first = [0u32; 10];
    let mut fc = 0usize;
    let count = match view {
        SetView::List(ids) => {
            for &id in ids {
                sum += id as u64;
                if fc < 10 {
                    first[fc] = id;
                    fc += 1;
                }
            }
            ids.len()
        }
        SetView::Bits(words) => {
            let mut c = 0usize;
            for (wi, &w) in words.iter().enumerate() {
                let mut rem = w;
                while rem != 0 {
                    let id = wi * 64 + rem.trailing_zeros() as usize;
                    if id < doc_count {
                        c += 1;
                        sum += id as u64;
                        if fc < 10 {
                            first[fc] = id as u32;
                            fc += 1;
                        }
                    }
                    rem &= rem - 1;
                }
            }
            c
        }
    };
    push_u64(out, count as u64);
    out.push(b' ');
    push_u64(out, sum);
    for &id in &first[..fc] {
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
    scan_doc_tokens(text, |tok| {
        let mut mask = 0u8;
        if let Some(k) = tok {
            for (off, &pk) in phrase_keys.iter().enumerate() {
                if k == pk {
                    mask |= 1u8 << off;
                }
            }
        }
        state = ((state << 1) | 1) & mask;
        if (state & target_bit) != 0 {
            matched = true;
            return false;
        }
        true
    });
    matched
}

fn parse_queries(query_bytes: &[u8], vocab: &mut Vocabulary, is_bool: &mut Vec<bool>) -> Vec<Query> {
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
            let mut terms = Vec::with_capacity(6);
            let mut ops = Vec::with_capacity(5);
            if let Some(first) = parts.next() {
                let id = vocab.intern(first);
                if id as usize >= is_bool.len() {
                    is_bool.resize(vocab.len(), false);
                }
                is_bool[id as usize] = true;
                terms.push(id);
            }
            while let (Some(op), Some(term)) = (parts.next(), parts.next()) {
                ops.push(match op {
                    b"AND" => Op::And,
                    b"OR" => Op::Or,
                    _ => Op::Not,
                });
                let id = vocab.intern(term);
                if id as usize >= is_bool.len() {
                    is_bool.resize(vocab.len(), false);
                }
                is_bool[id as usize] = true;
                terms.push(id);
            }
            queries.push(Query::Bool { terms, ops });
        } else if line.starts_with(b"PHRASE \"") && line.len() >= 9 {
            let body = &line[8..line.len() - 1];
            let phrase: Vec<u32> = body
                .split(|&b| b == b' ')
                .map(|t| {
                    let id = vocab.intern(t);
                    if id as usize >= is_bool.len() {
                        is_bool.resize(vocab.len(), false);
                    }
                    id
                })
                .collect();
            queries.push(Query::Phrase(phrase));
        }
    }
    queries
}

fn evaluate_query(
    output: &mut Vec<u8>,
    query: &Query,
    docs: &[u8],
    text_ranges: &[(u32, u32)],
    postings: &PostingIndex,
    vocab: &Vocabulary,
    scratch: &mut Scratch,
    threshold: usize,
) {
    match query {
        Query::Bool { terms, ops } => {
            let mut acc = Acc::Term(terms[0]);
            for (i, &op) in ops.iter().enumerate() {
                acc = eval_step(acc, terms[i + 1], op, postings, scratch, threshold);
            }
            let view = match acc {
                Acc::Term(id) => postings.view(id),
                Acc::List => SetView::List(&scratch.cur_list),
                Acc::Bits => SetView::Bits(&scratch.cur_bits),
            };
            append_view_result(output, view, postings.doc_count);
        }
        Query::Phrase(phrase) => {
            let len = phrase.len();
            let mut order = [0usize; 8];
            let mut pkeys = [(0u64, 0u64); 8];
            for i in 0..len {
                order[i] = i;
                pkeys[i] = vocab.key_of(phrase[i]);
            }
            order[..len].sort_unstable_by_key(|&i| postings.sets[phrase[i] as usize].df());
            let seed = phrase[order[0]];
            if postings.sets[seed as usize].df() == 0 {
                output.extend_from_slice(b"0 0\n");
                return;
            }
            let mut filters = [0u32; 8];
            let mut fcount = 0usize;
            for &pos in &order[1..len] {
                let tid = phrase[pos];
                if !postings.sets[tid as usize].is_skip() && tid != seed {
                    filters[fcount] = tid;
                    fcount += 1;
                }
            }
            let matches = &mut scratch.cur_list;
            matches.clear();
            let check_doc = |doc_id: u32| -> bool {
                for &fid in &filters[..fcount] {
                    if !postings.view(fid).contains(doc_id) {
                        return false;
                    }
                }
                let (ts, te) = unsafe { *text_ranges.get_unchecked(doc_id as usize) };
                phrase_matches_text(&docs[ts as usize..te as usize], &pkeys[..len])
            };
            match postings.view(seed) {
                SetView::List(ids) => {
                    for &doc_id in ids {
                        if check_doc(doc_id) {
                            matches.push(doc_id);
                        }
                    }
                }
                SetView::Bits(words) => {
                    for (wi, &w0) in words.iter().enumerate() {
                        let mut rem = w0;
                        for &fid in &filters[..fcount] {
                            if let SetView::Bits(other) = postings.view(fid) {
                                rem &= other[wi];
                            }
                        }
                        while rem != 0 {
                            let doc_id = wi * 64 + rem.trailing_zeros() as usize;
                            if doc_id < postings.doc_count && check_doc(doc_id as u32) {
                                matches.push(doc_id as u32);
                            }
                            rem &= rem - 1;
                        }
                    }
                }
            }
            append_view_result(output, SetView::List(matches), postings.doc_count);
        }
    }
}

fn run(docs: &[u8]) -> io::Result<()> {
    let query_bytes = fs::read("/data/queries.txt")?;
    let mut vocab = Vocabulary::new();
    let mut keep_posting = Vec::new();
    let queries = parse_queries(&query_bytes, &mut vocab, &mut keep_posting);
    drop(query_bytes);

    let threads = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(MAX_THREADS)
        .max(1);
    let ranges = line_chunks(docs, threads);

    let chunk_counts: Vec<ChunkCount> = thread::scope(|s| {
        let handles: Vec<_> = ranges
            .iter()
            .map(|&(start, end)| {
                let v = &vocab;
                s.spawn(move || scan_chunk_count(&docs[start..end], start, v))
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut df = vec![0u32; vocab.len()];
    let total_docs: usize = chunk_counts.iter().map(|c| c.text_ranges.len()).sum();
    let mut text_ranges = Vec::with_capacity(total_docs);
    let mut thread_doc_bases = Vec::with_capacity(chunk_counts.len());
    for c in &chunk_counts {
        thread_doc_bases.push(text_ranges.len() as u32);
        text_ranges.extend_from_slice(&c.text_ranges);
        for (t, st) in c.states.iter().enumerate() {
            df[t] += st.value;
        }
    }

    for q in &queries {
        if let Query::Phrase(p) = q {
            let mut ord: Vec<u32> = p.clone();
            ord.sort_unstable_by_key(|&tid| df[tid as usize]);
            ord.dedup();
            for &tid in ord.iter().take(2) {
                keep_posting[tid as usize] = true;
            }
            for &tid in &ord {
                if df[tid as usize] < 2048 {
                    keep_posting[tid as usize] = true;
                }
            }
        }
    }

    let threshold = (total_docs / 32).max(1);
    let word_count = total_docs.div_ceil(64) as u32;
    let mut term_refs = vec![
        TermRef {
            start: 0,
            tagged_len: 0,
        };
        vocab.len()
    ];
    let mut sparse_total = 0u32;
    let mut bitset_total = 0u32;
    for t in 0..vocab.len() {
        let f = df[t];
        if !keep_posting[t] {
            term_refs[t] = TermRef {
                start: 0,
                tagged_len: SKIP_TAG,
            };
        } else if f as usize >= threshold {
            term_refs[t] = TermRef {
                start: bitset_total,
                tagged_len: DENSE_TAG | f,
            };
            bitset_total += word_count;
        } else {
            term_refs[t] = TermRef {
                start: sparse_total,
                tagged_len: f,
            };
            sparse_total += f;
        }
    }

    let mut thread_term_bases = Vec::with_capacity(chunk_counts.len());
    let mut running = term_refs.iter().map(|r| r.start).collect::<Vec<u32>>();
    for c in &chunk_counts {
        let mut bases = vec![0u32; vocab.len()];
        for (t, st) in c.states.iter().enumerate() {
            bases[t] = running[t];
            if !term_refs[t].is_dense() && !term_refs[t].is_skip() {
                running[t] += st.value;
            }
        }
        thread_term_bases.push(bases);
    }
    drop(chunk_counts);
    drop(running);

    let mut sparse = vec![0u32; sparse_total as usize];
    let mut bitsets = vec![0u64; bitset_total as usize];
    {
        let sparse_atom: &[AtomicU32] = AtomicU32::from_mut_slice(&mut sparse);
        let bitset_atom: &[AtomicU64] = AtomicU64::from_mut_slice(&mut bitsets);
        thread::scope(|s| {
            for i in 0..thread_doc_bases.len() {
                let d_base = thread_doc_bases[i];
                let d_end = thread_doc_bases.get(i + 1).copied().unwrap_or(total_docs as u32);
                let tr_slice = &text_ranges[d_base as usize..d_end as usize];
                let t_base = &thread_term_bases[i];
                let v = &vocab;
                let t_refs = &term_refs;
                s.spawn(move || {
                    scan_chunk_write(docs, tr_slice, v, d_base, t_base, t_refs, sparse_atom, bitset_atom);
                });
            }
        });
    }
    drop(thread_term_bases);

    let postings = PostingIndex {
        sparse,
        bitsets,
        sets: term_refs,
        doc_count: total_docs,
    };

    let block_count = queries.len().div_ceil(QUERY_BLOCK_SIZE);
    let next_block = AtomicUsize::new(0);
    let worker_results: Vec<Vec<(usize, Vec<u8>)>> = thread::scope(|s| {
        let handles: Vec<_> = (0..threads.min(block_count.max(1)))
            .map(|_| {
                let q_ref = &queries;
                let tr_ref = &text_ranges;
                let p_ref = &postings;
                let v_ref = &vocab;
                let nb_ref = &next_block;
                s.spawn(move || {
                    let mut scratch = Scratch::new(word_count as usize);
                    let mut local_blocks = Vec::new();
                    loop {
                        let b = nb_ref.fetch_add(1, Ordering::Relaxed);
                        if b >= block_count {
                            break;
                        }
                        let start = b * QUERY_BLOCK_SIZE;
                        let end = (start + QUERY_BLOCK_SIZE).min(q_ref.len());
                        let mut out = Vec::with_capacity((end - start) * 32);
                        for q in &q_ref[start..end] {
                            evaluate_query(&mut out, q, docs, tr_ref, p_ref, v_ref, &mut scratch, threshold);
                        }
                        local_blocks.push((b, out));
                    }
                    local_blocks
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut ordered = vec![Vec::new(); block_count];
    for list in worker_results {
        for (b, out) in list {
            ordered[b] = out;
        }
    }
    fs::create_dir_all("/out")?;
    let mut file = File::create("/out/results.txt")?;
    for out in &ordered {
        file.write_all(out)?;
    }
    file.flush()?;
    drop(file);
    std::process::exit(0);
}

fn main() -> io::Result<()> {
    let file = File::open("/data/docs.txt")?;
    let len = usize::try_from(file.metadata()?.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "docs.txt too large"))?;
    if len == 0 {
        return run(&[]);
    }
    unsafe {
        let ptr = mmap(std::ptr::null_mut(), len, PROT_READ, MAP_PRIVATE, file.as_raw_fd(), 0);
        if ptr as isize == -1 {
            return Err(io::Error::last_os_error());
        }
        run(std::slice::from_raw_parts(ptr.cast::<u8>(), len))
    }
}