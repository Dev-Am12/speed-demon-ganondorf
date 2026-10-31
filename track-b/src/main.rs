use std::ffi::c_void;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::thread;

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

#[derive(Clone, Copy)]
enum Query {
    Bool {
        terms: [u32; 6],
        ops: [Op; 5],
        len: u8,
    },
    Phrase {
        terms: [u32; 8],
        len: u8,
    },
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

#[repr(C)]
#[derive(Clone, Copy)]
struct WideState {
    last_seen: u32,
    value: u32,
}

enum TermStates {
    Packed(Vec<u32>),
    Wide(Vec<WideState>),
}

impl TermStates {
    fn new(vocab_len: usize, packed: bool) -> Self {
        if packed {
            Self::Packed(vec![u16::MAX as u32; vocab_len])
        } else {
            Self::Wide(vec![
                WideState {
                    last_seen: u32::MAX,
                    value: 0
                };
                vocab_len
            ])
        }
    }
    fn value(&self, i: usize) -> u32 {
        match self {
            Self::Packed(v) => v[i] >> 16,
            Self::Wide(v) => v[i].value,
        }
    }
    fn record(&mut self, i: usize, doc: u32) -> Option<u32> {
        match self {
            Self::Packed(v) => {
                let x = &mut v[i];
                let local = doc as u16;
                if *x as u16 == local {
                    None
                } else {
                    let old = *x >> 16;
                    *x = ((old + 1) << 16) | local as u32;
                    Some(old)
                }
            }
            Self::Wide(v) => {
                let x = &mut v[i];
                if x.last_seen == doc {
                    None
                } else {
                    x.last_seen = doc;
                    let old = x.value;
                    x.value += 1;
                    Some(old)
                }
            }
        }
    }
    fn reset(&mut self) {
        match self {
            Self::Packed(v) => v.fill(u16::MAX as u32),
            Self::Wide(v) => v.fill(WideState {
                last_seen: u32::MAX,
                value: 0,
            }),
        }
    }
}

struct ChunkCount {
    states: TermStates,
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

fn document_ranges(doc_count: usize, workers: usize) -> Vec<(u32, u32)> {
    (0..workers)
        .map(|i| {
            (
                (doc_count * i / workers) as u32,
                (doc_count * (i + 1) / workers) as u32,
            )
        })
        .collect()
}

fn build_document_starts(docs: &[u8], workers: usize) -> io::Result<Vec<u32>> {
    if docs.len() > u32::MAX as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "docs.txt exceeds 4 GiB",
        ));
    }
    if docs.is_empty() {
        return Ok(vec![0]);
    }
    let ranges: Vec<_> = (0..workers)
        .map(|i| (docs.len() * i / workers, docs.len() * (i + 1) / workers))
        .collect();
    let counts: Vec<usize> = thread::scope(|scope| {
        let handles: Vec<_> = ranges
            .iter()
            .map(|&(a, b)| scope.spawn(move || docs[a..b].iter().filter(|&&x| x == b'\n').count()))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let lf: usize = counts.iter().sum();
    let n = lf + usize::from(*docs.last().unwrap() != b'\n');
    let mut starts = vec![0u32; n + 1];
    let mut rem = &mut starts[1..];
    let mut outputs = Vec::with_capacity(counts.len());
    for &count in &counts {
        let (head, tail) = rem.split_at_mut(count);
        outputs.push(head);
        rem = tail;
    }
    thread::scope(|scope| {
        let handles: Vec<_> = ranges
            .into_iter()
            .zip(outputs)
            .map(|((a, b), out)| {
                scope.spawn(move || {
                    let mut j = 0;
                    for (i, &byte) in docs[a..b].iter().enumerate() {
                        if byte == b'\n' {
                            out[j] = (a + i + 1) as u32;
                            j += 1;
                        }
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
    });
    starts[n] = docs.len() as u32;
    Ok(starts)
}

#[inline]
fn text_bounds(docs: &[u8], starts: &[u32], id: usize) -> (usize, usize) {
    let start = starts[id] as usize;
    let next = starts[id + 1] as usize;
    let end = if next > start && docs[next - 1] == b'\n' {
        next - 1
    } else {
        next
    };
    let text_start = docs[start..end]
        .iter()
        .position(|&b| b == b'\t')
        .map(|i| start + i + 1)
        .unwrap_or(end);
    (text_start, end)
}

fn scan_chunk_count(
    docs: &[u8],
    starts: &[u32],
    doc_start: u32,
    doc_end: u32,
    vocab: &Vocabulary,
    packed: bool,
) -> ChunkCount {
    let mut states = TermStates::new(vocab.len(), packed);
    for doc_id in doc_start..doc_end {
        let (a, b) = text_bounds(docs, starts, doc_id as usize);
        if a < b {
            scan_doc_tokens(&docs[a..b], |tok| {
                if let Some((lo, hi)) = tok {
                    if let Some(id) = vocab.get_key(lo, hi) {
                        states.record(id as usize, doc_id - doc_start);
                    }
                }
                true
            });
        }
    }
    ChunkCount { states }
}

fn scan_chunk_write(
    docs: &[u8],
    starts: &[u32],
    doc_start: u32,
    doc_end: u32,
    vocab: &Vocabulary,
    states: &mut TermStates,
    bases: &[u32],
    refs: &[TermRef],
    sparse: &[AtomicU32],
    bits: &[AtomicU64],
) {
    for doc_id in doc_start..doc_end {
        let local = doc_id - doc_start;
        let (a, b) = text_bounds(docs, starts, doc_id as usize);
        if a < b {
            scan_doc_tokens(&docs[a..b], |tok| {
                if let Some((lo, hi)) = tok {
                    if let Some(id) = vocab.get_key(lo, hi) {
                        let i = id as usize;
                        if let Some(cursor) = states.record(i, local) {
                            let r = unsafe { *refs.get_unchecked(i) };
                            if r.is_skip() {
                                return true;
                            }
                            if r.is_dense() {
                                let slot = r.start as usize + (doc_id as usize >> 6);
                                unsafe {
                                    bits.get_unchecked(slot)
                                        .fetch_or(1u64 << (doc_id & 63), Ordering::Relaxed);
                                }
                            } else {
                                let slot = unsafe { *bases.get_unchecked(i) } + cursor;
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

fn eval_step(
    acc: Acc,
    rhs_id: u32,
    op: Op,
    postings: &PostingIndex,
    scratch: &mut Scratch,
    threshold: usize,
) -> Acc {
    let right = postings.view(rhs_id);
    let left = match acc {
        Acc::Term(id) => postings.view(id),
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

fn parse_queries(
    query_bytes: &[u8],
    vocab: &mut Vocabulary,
    is_bool: &mut Vec<bool>,
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
            queries.push(Query::Phrase {
                terms,
                len: len as u8,
            });
        }
    }
    queries
}

fn evaluate_query(
    output: &mut Vec<u8>,
    query: &Query,
    docs: &[u8],
    doc_starts: &[u32],
    postings: &PostingIndex,
    vocab: &Vocabulary,
    scratch: &mut Scratch,
    threshold: usize,
) {
    match query {
        Query::Bool { terms, ops, len } => {
            let count = *len as usize;
            let mut acc = Acc::Term(terms[0]);
            for i in 0..count - 1 {
                acc = eval_step(acc, terms[i + 1], ops[i], postings, scratch, threshold);
            }
            let view = match acc {
                Acc::Term(id) => postings.view(id),
                Acc::List => SetView::List(&scratch.cur_list),
                Acc::Bits => SetView::Bits(&scratch.cur_bits),
            };
            append_view_result(output, view, postings.doc_count);
        }
        Query::Phrase { terms, len } => {
            let len = *len as usize;
            let phrase = &terms[..len];
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
                let (ts, te) = text_bounds(docs, doc_starts, doc_id as usize);
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
    let doc_starts = build_document_starts(docs, threads)?;
    let total_docs = doc_starts.len() - 1;
    let scan_workers = threads
        .max(total_docs.div_ceil(u16::MAX as usize))
        .min(MAX_THREADS);
    let ranges = document_ranges(total_docs, scan_workers);
    let packed_states = ranges.iter().all(|(a, b)| b - a <= u16::MAX as u32);
    let mut chunk_counts: Vec<ChunkCount> = thread::scope(|scope| {
        let handles: Vec<_> = ranges
            .iter()
            .map(|&(a, b)| {
                let v = &vocab;
                let starts = &doc_starts;
                scope.spawn(move || scan_chunk_count(docs, starts, a, b, v, packed_states))
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut df = vec![0u32; vocab.len()];
    for c in &chunk_counts {
        for t in 0..vocab.len() {
            df[t] += c.states.value(t);
        }
    }

    for q in &queries {
        if let Query::Phrase { terms, len } = q {
            let len = *len as usize;
            let mut ord = *terms;
            ord[..len].sort_unstable_by_key(|&tid| (df[tid as usize], tid));
            let mut distinct = 0usize;
            let mut previous = u32::MAX;
            for &tid in &ord[..len] {
                if tid != previous {
                    if distinct < 2 {
                        keep_posting[tid as usize] = true;
                    }
                    distinct += 1;
                    previous = tid;
                }
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
        for t in 0..vocab.len() {
            bases[t] = running[t];
            if !term_refs[t].is_dense() && !term_refs[t].is_skip() {
                running[t] += c.states.value(t);
            }
        }
        thread_term_bases.push(bases);
    }
    drop(running);

    let mut sparse = vec![0u32; sparse_total as usize];
    let mut bitsets = vec![0u64; bitset_total as usize];
    {
        let sparse_atom: &[AtomicU32] = AtomicU32::from_mut_slice(&mut sparse);
        let bitset_atom: &[AtomicU64] = AtomicU64::from_mut_slice(&mut bitsets);
        for chunk in &mut chunk_counts {
            chunk.states.reset();
        }
        thread::scope(|s| {
            for (i, chunk) in chunk_counts.iter_mut().enumerate() {
                let (d_base, d_end) = ranges[i];
                let starts = &doc_starts;
                let t_base = &thread_term_bases[i];
                let v = &vocab;
                let t_refs = &term_refs;
                let states = &mut chunk.states;
                s.spawn(move || {
                    scan_chunk_write(
                        docs,
                        starts,
                        d_base,
                        d_end,
                        v,
                        states,
                        t_base,
                        t_refs,
                        sparse_atom,
                        bitset_atom,
                    )
                });
            }
        });
    }
    drop(thread_term_bases);
    drop(chunk_counts);

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
                let starts_ref = &doc_starts;
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
                            evaluate_query(
                                &mut out,
                                q,
                                docs,
                                starts_ref,
                                p_ref,
                                v_ref,
                                &mut scratch,
                                threshold,
                            );
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
