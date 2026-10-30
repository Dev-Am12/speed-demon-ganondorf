use std::ffi::c_void;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;

const MAX_TERM_LEN: usize = 32;
const MAX_THREADS: usize = 12;
const QUERY_BLOCK_SIZE: usize = 256;
const PROT_READ: i32 = 1;
const MAP_PRIVATE: i32 = 2;

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

struct Vocabulary {
    keys: Vec<u64>,
    ids: Vec<u32>,
    term_bytes: Vec<u8>,
    offsets: Vec<u32>,
    maybe_bits: Vec<u64>,
}

impl Vocabulary {
    fn new() -> Self {
        Self {
            keys: vec![0; 8],
            ids: vec![0; 8],
            term_bytes: Vec::new(),
            offsets: vec![0],
            maybe_bits: vec![0; 1 << 14],
        }
    }

    fn len(&self) -> usize {
        self.offsets.len() - 1
    }

    fn term(&self, id: u32) -> &[u8] {
        let index = id as usize;
        &self.term_bytes[self.offsets[index] as usize..self.offsets[index + 1] as usize]
    }

    fn maybe_present(&self, key: u64) -> bool {
        let bit = slot_hash(key) as usize & ((1 << 20) - 1);
        self.maybe_bits[bit / 64] & (1u64 << (bit % 64)) != 0
    }

    fn mark_maybe(&mut self, key: u64) {
        let bit = slot_hash(key) as usize & ((1 << 20) - 1);
        self.maybe_bits[bit / 64] |= 1u64 << (bit % 64);
    }

    fn get(&self, term: &[u8]) -> Option<u32> {
        if term.is_empty() || term.len() > MAX_TERM_LEN {
            return None;
        }
        let key = term_key(term);
        if !self.maybe_present(key) {
            return None;
        }
        let mask = self.keys.len() - 1;
        let mut slot = slot_hash(key) as usize & mask;
        loop {
            let candidate = self.keys[slot];
            if candidate == 0 {
                return None;
            }
            let id = self.ids[slot];
            if candidate == key && self.term(id) == term {
                return Some(id);
            }
            slot = (slot + 1) & mask;
        }
    }

    fn intern(&mut self, term: &[u8]) -> u32 {
        if let Some(id) = self.get(term) {
            return id;
        }
        if self.len() + 1 > self.keys.len() / 2 {
            self.grow();
        }
        let id = self.len() as u32;
        let key = term_key(term);
        self.term_bytes.extend_from_slice(term);
        self.offsets.push(self.term_bytes.len() as u32);
        self.mark_maybe(key);
        self.insert_existing(key, id);
        id
    }

    fn grow(&mut self) {
        let new_capacity = self.keys.len() * 2;
        let old_keys = std::mem::replace(&mut self.keys, vec![0; new_capacity]);
        let old_ids = std::mem::replace(&mut self.ids, vec![0; new_capacity]);
        for (key, id) in old_keys.into_iter().zip(old_ids) {
            if key != 0 {
                self.insert_existing(key, id);
            }
        }
    }

    fn insert_existing(&mut self, key: u64, id: u32) {
        let mask = self.keys.len() - 1;
        let mut slot = slot_hash(key) as usize & mask;
        while self.keys[slot] != 0 {
            slot = (slot + 1) & mask;
        }
        self.keys[slot] = key;
        self.ids[slot] = id;
    }
}

// Short terms are exact little-endian bytes. Long terms carry a hash marker;
// stored bytes confirm equality after any hash collision.
fn term_key(term: &[u8]) -> u64 {
    if term.len() <= 8 {
        let mut key = 0u64;
        for (index, &byte) in term.iter().enumerate() {
            key |= (byte as u64) << (index * 8);
        }
        key
    } else {
        hash_term(term) | (1u64 << 63)
    }
}

fn slot_hash(key: u64) -> u64 {
    let mut value = key ^ (key >> 30);
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn hash_term(term: &[u8]) -> u64 {
    let mut hash = 0x9e37_79b9_7f4a_7c15u64 ^ term.len() as u64;
    for chunk in term.chunks(8) {
        let mut bytes = [0u8; 8];
        bytes[..chunk.len()].copy_from_slice(chunk);
        let mut word = u64::from_le_bytes(bytes);
        word ^= word >> 30;
        word = word.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        word ^= word >> 27;
        word = word.wrapping_mul(0x94d0_49bb_1331_11eb);
        word ^= word >> 31;
        hash ^= word;
        hash = hash
            .rotate_left(27)
            .wrapping_mul(0x3c79_ac49_2ba7_b653)
            .wrapping_add(0x1c69_b3f7_4ac4_ae35);
    }
    hash ^ (hash >> 33)
}

const fn make_byte_map() -> [u8; 256] {
    let mut map = [0u8; 256];
    let mut byte = b'a';
    while byte <= b'z' {
        map[byte as usize] = byte;
        byte += 1;
    }
    byte = b'A';
    while byte <= b'Z' {
        map[byte as usize] = byte | 0x20;
        byte += 1;
    }
    byte = b'0';
    while byte <= b'9' {
        map[byte as usize] = byte;
        byte += 1;
    }
    map
}

const BYTE_MAP: [u8; 256] = make_byte_map();

fn finish_token<F>(
    token: &[u8; MAX_TERM_LEN],
    length: &mut usize,
    doc_id: u32,
    vocabulary: &Vocabulary,
    hit: &mut F,
) where
    F: FnMut(u32, u32),
{
    if *length != 0 && *length <= MAX_TERM_LEN {
        if let Some(id) = vocabulary.get(&token[..*length]) {
            hit(id, doc_id);
        }
    }
    *length = 0;
}

fn tokenize<F>(text: &[u8], map: &[u8; 256], mut visit: F)
where
    F: FnMut(Option<&[u8]>) -> bool,
{
    let mut token = [0u8; MAX_TERM_LEN];
    let mut length = 0usize;
    for &byte in text {
        let mapped = map[byte as usize];
        if mapped != 0 {
            token[length & (MAX_TERM_LEN - 1)] = mapped;
            length += 1;
        } else if length != 0 {
            let keep_going = if length <= MAX_TERM_LEN {
                visit(Some(&token[..length]))
            } else {
                visit(None)
            };
            length = 0;
            if !keep_going {
                return;
            }
        }
    }
    if length != 0 {
        if length <= MAX_TERM_LEN {
            let _ = visit(Some(&token[..length]));
        } else {
            let _ = visit(None);
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TermState {
    last_seen: u32,
    value: u32,
}

struct ChunkCount {
    states: Vec<TermState>,
    starts: Vec<usize>,
    documents: u32,
}

// The same byte loop is used for counting and posting writes. It recognizes
// only text after the first TAB, and emits a hit at the current local document.
fn scan_chunk<F>(
    bytes: &[u8],
    absolute_start: usize,
    vocabulary: &Vocabulary,
    map: &[u8; 256],
    mut starts: Option<&mut Vec<usize>>,
    mut hit: F,
) -> u32
where
    F: FnMut(u32, u32),
{
    if !bytes.is_empty() {
        if let Some(starts) = starts.as_deref_mut() {
            starts.push(absolute_start);
        }
    }
    let mut token = [0u8; MAX_TERM_LEN];
    let mut length = 0usize;
    let mut in_id = true;
    let mut local_doc = 0u32;
    let mut documents = 0u32;

    for (position, &byte) in bytes.iter().enumerate() {
        if byte == b'\n' {
            if !in_id {
                finish_token(&token, &mut length, local_doc, vocabulary, &mut hit);
            }
            local_doc += 1;
            documents += 1;
            in_id = true;
            length = 0;
            if position + 1 < bytes.len() {
                if let Some(starts) = starts.as_deref_mut() {
                    starts.push(absolute_start + position + 1);
                }
            }
        } else if in_id {
            if byte == b'\t' {
                in_id = false;
                length = 0;
            }
        } else {
            let mapped = map[byte as usize];
            if mapped != 0 {
                token[length & (MAX_TERM_LEN - 1)] = mapped;
                length += 1;
            } else if length != 0 {
                finish_token(&token, &mut length, local_doc, vocabulary, &mut hit);
            }
        }
    }

    if !in_id {
        finish_token(&token, &mut length, local_doc, vocabulary, &mut hit);
    }
    if !bytes.is_empty() && bytes[bytes.len() - 1] != b'\n' {
        documents += 1;
    }
    documents
}

fn scan_chunk_count(
    bytes: &[u8],
    absolute_start: usize,
    vocabulary: &Vocabulary,
    map: &[u8; 256],
) -> ChunkCount {
    let mut states = vec![
        TermState {
            last_seen: u32::MAX,
            value: 0
        };
        vocabulary.len()
    ];
    let mut starts = Vec::new();
    let documents = scan_chunk(
        bytes,
        absolute_start,
        vocabulary,
        map,
        Some(&mut starts),
        |id, local_doc| {
            let state = &mut states[id as usize];
            if state.last_seen != local_doc {
                state.last_seen = local_doc;
                state.value += 1;
            }
        },
    );
    ChunkCount {
        states,
        starts,
        documents,
    }
}

fn scan_chunk_write(
    bytes: &[u8],
    vocabulary: &Vocabulary,
    map: &[u8; 256],
    global_doc_start: u32,
    term_bases: &[u32],
    term_refs: &[TermRef],
    sparse: &[AtomicU32],
    bitsets: &[AtomicU64],
) {
    let mut states = vec![
        TermState {
            last_seen: u32::MAX,
            value: 0
        };
        vocabulary.len()
    ];
    scan_chunk(bytes, 0, vocabulary, map, None, |id, local_doc| {
        let index = id as usize;
        let state = &mut states[index];
        if state.last_seen != local_doc {
            state.last_seen = local_doc;
            let reference = term_refs[index];
            if reference.is_dense() {
                let doc_id = global_doc_start + local_doc;
                let slot = reference.start as usize + (doc_id as usize / 64);
                bitsets[slot].fetch_or(1u64 << (doc_id % 64), Ordering::Relaxed);
            } else {
                let slot = term_bases[index] + state.value;
                state.value += 1;
                sparse[slot as usize].store(global_doc_start + local_doc, Ordering::Relaxed);
            }
        }
    });
}

fn line_chunks(bytes: &[u8], count: usize) -> Vec<(usize, usize)> {
    let mut starts = vec![0usize; count + 1];
    for index in 1..count {
        let nominal = ((bytes.len() as u128 * index as u128) / count as u128) as usize;
        let boundary = if nominal == 0 {
            0
        } else {
            let search_from = nominal - 1;
            bytes[search_from..]
                .iter()
                .position(|&byte| byte == b'\n')
                .map(|offset| search_from + offset + 1)
                .unwrap_or(bytes.len())
        };
        starts[index] = boundary;
    }
    starts[count] = bytes.len();
    starts.windows(2).map(|pair| (pair[0], pair[1])).collect()
}

fn count_chunks(
    bytes: &[u8],
    ranges: &[(usize, usize)],
    vocabulary: &Vocabulary,
    map: &[u8; 256],
) -> io::Result<Vec<ChunkCount>> {
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(ranges.len());
        for &(start, end) in ranges {
            let slice = &bytes[start..end];
            handles.push(scope.spawn(move || scan_chunk_count(slice, start, vocabulary, map)));
        }
        let mut results = Vec::with_capacity(handles.len());
        for handle in handles {
            match handle.join() {
                Ok(result) => results.push(result),
                Err(_) => return Err(io::Error::other("document count worker panicked")),
            }
        }
        Ok(results)
    })
}

fn write_chunks(
    bytes: &[u8],
    ranges: &[(usize, usize)],
    doc_bases: &[u32],
    term_bases: &[Vec<u32>],
    term_refs: &[TermRef],
    vocabulary: &Vocabulary,
    map: &[u8; 256],
    sparse: &[AtomicU32],
    bitsets: &[AtomicU64],
) -> io::Result<()> {
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(ranges.len());
        for index in 0..ranges.len() {
            let (start, end) = ranges[index];
            let slice = &bytes[start..end];
            let base = &term_bases[index];
            let doc_base = doc_bases[index];
            handles.push(scope.spawn(move || {
                scan_chunk_write(
                    slice, vocabulary, map, doc_base, base, term_refs, sparse, bitsets,
                )
            }));
        }
        for handle in handles {
            if handle.join().is_err() {
                return Err(io::Error::other("posting write worker panicked"));
            }
        }
        Ok(())
    })
}

const DENSE_TAG: u32 = 1 << 31;

#[derive(Clone, Copy)]
struct TermRef {
    start: u32,
    tagged_len: u32,
}

impl TermRef {
    fn is_dense(self) -> bool {
        self.tagged_len & DENSE_TAG != 0
    }

    fn len(self) -> usize {
        (self.tagged_len & !DENSE_TAG) as usize
    }

    fn df(self) -> usize {
        self.len()
    }
}

#[derive(Clone, Copy)]
enum SetView<'a> {
    List(&'a [u32]),
    Bits(&'a [u64]),
}

impl SetView<'_> {
    fn contains(self, id: u32) -> bool {
        match self {
            Self::List(ids) => ids.binary_search(&id).is_ok(),
            Self::Bits(words) => {
                let index = id as usize;
                words
                    .get(index / 64)
                    .is_some_and(|word| word & (1u64 << (index % 64)) != 0)
            }
        }
    }

    fn is_bits(self) -> bool {
        matches!(self, Self::Bits(_))
    }

    fn count(self) -> usize {
        match self {
            Self::List(ids) => ids.len(),
            Self::Bits(words) => words.iter().map(|word| word.count_ones() as usize).sum(),
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
    fn from_arenas(
        sparse: Vec<u32>,
        bitsets: Vec<u64>,
        sets: Vec<TermRef>,
        doc_count: usize,
    ) -> Self {
        Self {
            sparse,
            bitsets,
            sets,
            doc_count,
        }
    }

    fn reference(&self, term_id: u32) -> TermRef {
        self.sets[term_id as usize]
    }

    fn df(&self, term_id: u32) -> usize {
        self.reference(term_id).df()
    }

    fn view(&self, term_id: u32) -> SetView<'_> {
        let reference = self.reference(term_id);
        let start = reference.start as usize;
        if reference.is_dense() {
            let words = self.doc_count.div_ceil(64);
            SetView::Bits(&self.bitsets[start..start + words])
        } else {
            SetView::List(&self.sparse[start..start + reference.len()])
        }
    }

    fn contains(&self, term_id: u32, doc_id: u32) -> bool {
        self.view(term_id).contains(doc_id)
    }
}

#[derive(Clone, Copy)]
enum Accumulator {
    Term(u32),
    List(usize),
    Bits(usize),
}

struct Scratch {
    lists: [Vec<u32>; 2],
    bits: [Vec<u64>; 2],
}

impl Scratch {
    fn new() -> Self {
        Self {
            lists: std::array::from_fn(|_| Vec::new()),
            bits: std::array::from_fn(|_| Vec::new()),
        }
    }
}

fn accumulator_view<'a>(
    accumulator: Accumulator,
    postings: &'a PostingIndex,
    scratch: &'a Scratch,
) -> SetView<'a> {
    match accumulator {
        Accumulator::Term(id) => postings.view(id),
        Accumulator::List(slot) => SetView::List(&scratch.lists[slot]),
        Accumulator::Bits(slot) => SetView::Bits(&scratch.bits[slot]),
    }
}

fn result_is_bits(left: SetView<'_>, right: SetView<'_>, op: Op, threshold: usize) -> bool {
    match op {
        Op::And => left.is_bits() && right.is_bits(),
        Op::Or => {
            left.is_bits()
                || right.is_bits()
                || left.count().saturating_add(right.count()) >= threshold
        }
        Op::Not => left.is_bits(),
    }
}

fn add_to_bits(view: SetView<'_>, out: &mut [u64]) {
    match view {
        SetView::List(ids) => {
            for &id in ids {
                let index = id as usize;
                out[index / 64] |= 1u64 << (index % 64);
            }
        }
        SetView::Bits(words) => {
            for (dst, &src) in out.iter_mut().zip(words) {
                *dst |= src;
            }
        }
    }
}

fn remove_from_bits(view: SetView<'_>, out: &mut [u64]) {
    match view {
        SetView::List(ids) => {
            for &id in ids {
                let index = id as usize;
                out[index / 64] &= !(1u64 << (index % 64));
            }
        }
        SetView::Bits(words) => {
            for (dst, &src) in out.iter_mut().zip(words) {
                *dst &= !src;
            }
        }
    }
}

fn apply_to_bits(left: SetView<'_>, right: SetView<'_>, op: Op, out: &mut [u64]) {
    out.fill(0);
    match op {
        Op::And => match (left, right) {
            (SetView::Bits(a), SetView::Bits(b)) => {
                for (index, dst) in out.iter_mut().enumerate() {
                    *dst = a[index] & b[index];
                }
            }
            (SetView::List(ids), bits) | (bits, SetView::List(ids)) => {
                for &id in ids {
                    if bits.contains(id) {
                        let index = id as usize;
                        out[index / 64] |= 1u64 << (index % 64);
                    }
                }
            }
        },
        Op::Or => {
            add_to_bits(left, out);
            add_to_bits(right, out);
        }
        Op::Not => {
            add_to_bits(left, out);
            remove_from_bits(right, out);
        }
    }
}

fn apply_to_list(
    left: SetView<'_>,
    right: SetView<'_>,
    op: Op,
    doc_count: usize,
    out: &mut Vec<u32>,
) {
    out.clear();
    match op {
        Op::And => match (left, right) {
            (SetView::List(a), SetView::List(b)) => {
                let (mut i, mut j) = (0usize, 0usize);
                while i < a.len() && j < b.len() {
                    match a[i].cmp(&b[j]) {
                        std::cmp::Ordering::Less => i += 1,
                        std::cmp::Ordering::Greater => j += 1,
                        std::cmp::Ordering::Equal => {
                            out.push(a[i]);
                            i += 1;
                            j += 1;
                        }
                    }
                }
            }
            (SetView::List(ids), bits) | (bits, SetView::List(ids)) => {
                out.extend(ids.iter().copied().filter(|&id| bits.contains(id)));
            }
            (SetView::Bits(_), SetView::Bits(_)) => {
                for id in 0..doc_count as u32 {
                    if left.contains(id) && right.contains(id) {
                        out.push(id);
                    }
                }
            }
        },
        Op::Or => match (left, right) {
            (SetView::List(a), SetView::List(b)) => {
                let (mut i, mut j) = (0usize, 0usize);
                while i < a.len() && j < b.len() {
                    match a[i].cmp(&b[j]) {
                        std::cmp::Ordering::Less => {
                            out.push(a[i]);
                            i += 1;
                        }
                        std::cmp::Ordering::Greater => {
                            out.push(b[j]);
                            j += 1;
                        }
                        std::cmp::Ordering::Equal => {
                            out.push(a[i]);
                            i += 1;
                            j += 1;
                        }
                    }
                }
                out.extend_from_slice(&a[i..]);
                out.extend_from_slice(&b[j..]);
            }
            _ => {
                for id in 0..doc_count as u32 {
                    if left.contains(id) || right.contains(id) {
                        out.push(id);
                    }
                }
            }
        },
        Op::Not => match left {
            SetView::List(ids) => out.extend(ids.iter().copied().filter(|&id| !right.contains(id))),
            SetView::Bits(_) => {
                for id in 0..doc_count as u32 {
                    if left.contains(id) && !right.contains(id) {
                        out.push(id);
                    }
                }
            }
        },
    }
}

fn apply_operator(
    accumulator: Accumulator,
    rhs_id: u32,
    op: Op,
    postings: &PostingIndex,
    scratch: &mut Scratch,
    threshold: usize,
) -> Accumulator {
    let right = postings.view(rhs_id);
    let left_view = accumulator_view(accumulator, postings, scratch);
    let output_bits = result_is_bits(left_view, right, op, threshold);
    let word_count = postings.doc_count.div_ceil(64);

    match (accumulator, output_bits) {
        (Accumulator::Term(id), true) => {
            scratch.bits[0].resize(word_count, 0);
            apply_to_bits(postings.view(id), right, op, &mut scratch.bits[0]);
            Accumulator::Bits(0)
        }
        (Accumulator::List(source), true) => {
            let Scratch { lists, bits } = scratch;
            bits[0].resize(word_count, 0);
            apply_to_bits(SetView::List(&lists[source]), right, op, &mut bits[0]);
            Accumulator::Bits(0)
        }
        (Accumulator::Bits(source), true) => {
            let slot = 1 - source;
            let (first, second) = scratch.bits.split_at_mut(1);
            let (left, out) = if source == 0 {
                (&first[0][..], &mut second[0])
            } else {
                (&second[0][..], &mut first[0])
            };
            out.resize(word_count, 0);
            apply_to_bits(SetView::Bits(left), right, op, out);
            Accumulator::Bits(slot)
        }
        (Accumulator::Term(id), false) => {
            apply_to_list(
                postings.view(id),
                right,
                op,
                postings.doc_count,
                &mut scratch.lists[0],
            );
            Accumulator::List(0)
        }
        (Accumulator::List(source), false) => {
            let slot = 1 - source;
            let (first, second) = scratch.lists.split_at_mut(1);
            let (left, out) = if source == 0 {
                (&first[0][..], &mut second[0])
            } else {
                (&second[0][..], &mut first[0])
            };
            apply_to_list(SetView::List(left), right, op, postings.doc_count, out);
            Accumulator::List(slot)
        }
        (Accumulator::Bits(source), false) => {
            let Scratch { lists, bits } = scratch;
            apply_to_list(
                SetView::Bits(&bits[source]),
                right,
                op,
                postings.doc_count,
                &mut lists[0],
            );
            Accumulator::List(0)
        }
    }
}

fn append_view_result(out: &mut Vec<u8>, view: SetView<'_>, doc_count: usize) {
    let mut sum = 0u64;
    let mut first = [0u32; 10];
    let mut first_count = 0usize;
    let count = view.count();
    match view {
        SetView::List(ids) => {
            for &id in ids {
                sum += id as u64;
                if first_count < first.len() {
                    first[first_count] = id;
                    first_count += 1;
                }
            }
        }
        SetView::Bits(words) => {
            for (word_index, &word) in words.iter().enumerate() {
                let mut remaining = word;
                while remaining != 0 {
                    let id = word_index * 64 + remaining.trailing_zeros() as usize;
                    if id < doc_count {
                        sum += id as u64;
                        if first_count < first.len() {
                            first[first_count] = id as u32;
                            first_count += 1;
                        }
                    }
                    remaining &= remaining - 1;
                }
            }
        }
    }
    push_number(out, count as u64);
    out.push(b' ');
    push_number(out, sum);
    for &id in &first[..first_count] {
        out.push(b' ');
        push_number(out, id as u64);
    }
    out.push(b'\n');
}

fn phrase_matches(text: &[u8], phrase: &[u32], vocabulary: &Vocabulary, map: &[u8; 256]) -> bool {
    let width = phrase.len();
    let mut ring = [u32::MAX; 8];
    let mut position = 0usize;
    let mut matched = false;
    tokenize(text, map, |token| {
        let id = token
            .and_then(|term| vocabulary.get(term))
            .unwrap_or(u32::MAX);
        ring[position % width] = id;
        if position + 1 >= width {
            let first = (position + 1 - width) % width;
            let mut equal = true;
            for offset in 0..width {
                if ring[(first + offset) % width] != phrase[offset] {
                    equal = false;
                    break;
                }
            }
            if equal {
                matched = true;
                return false;
            }
        }
        position += 1;
        true
    });
    matched
}

fn push_number(out: &mut Vec<u8>, mut value: u64) {
    let mut digits = [0u8; 20];
    let mut used = 0usize;
    loop {
        digits[used] = b'0' + (value % 10) as u8;
        used += 1;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    while used != 0 {
        used -= 1;
        out.push(digits[used]);
    }
}

fn append_result(out: &mut Vec<u8>, ids: &[u32]) {
    push_number(out, ids.len() as u64);
    out.push(b' ');
    let mut sum = 0u64;
    for &id in ids {
        sum += id as u64;
    }
    push_number(out, sum);
    for &id in ids.iter().take(10) {
        out.push(b' ');
        push_number(out, id as u64);
    }
    out.push(b'\n');
}

fn parse_queries(query_bytes: &[u8], vocabulary: &mut Vocabulary) -> Vec<Query> {
    let mut queries = Vec::new();
    for line in query_bytes.split(|&byte| byte == b'\n') {
        if line.is_empty() {
            continue;
        }
        if line.starts_with(b"BOOL ") {
            let mut parts = line[5..].split(|&byte| byte == b' ');
            let mut terms = Vec::new();
            let mut ops = Vec::new();
            if let Some(first) = parts.next() {
                terms.push(vocabulary.intern(first));
            }
            while let (Some(op), Some(term)) = (parts.next(), parts.next()) {
                let parsed_op = match op {
                    b"AND" => Op::And,
                    b"OR" => Op::Or,
                    _ => Op::Not,
                };
                ops.push(parsed_op);
                terms.push(vocabulary.intern(term));
            }
            queries.push(Query::Bool { terms, ops });
        } else if line.starts_with(b"PHRASE \"") && line.len() >= 9 {
            let body = &line[8..line.len() - 1];
            let phrase = body
                .split(|&byte| byte == b' ')
                .map(|term| vocabulary.intern(term))
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
    document_starts: &[usize],
    postings: &PostingIndex,
    vocabulary: &Vocabulary,
    map: &[u8; 256],
    scratch: &mut Scratch,
    threshold: usize,
) {
    match query {
        Query::Bool { terms, ops } => {
            let mut result = terms
                .first()
                .copied()
                .map(Accumulator::Term)
                .unwrap_or(Accumulator::List(0));
            for (index, &op) in ops.iter().enumerate() {
                if let Some(&rhs) = terms.get(index + 1) {
                    result = apply_operator(result, rhs, op, postings, scratch, threshold);
                }
            }
            let view = accumulator_view(result, postings, scratch);
            append_view_result(output, view, postings.doc_count);
        }
        Query::Phrase(phrase) => {
            let mut distinct = phrase.clone();
            distinct.sort_unstable();
            distinct.dedup();
            distinct.sort_by_key(|&id| postings.df(id));
            let mut candidates = Vec::new();
            if let Some(&seed) = distinct.first() {
                match postings.view(seed) {
                    SetView::List(ids) => candidates.extend_from_slice(ids),
                    SetView::Bits(words) => {
                        for (word_index, &word) in words.iter().enumerate() {
                            let mut remaining = word;
                            while remaining != 0 {
                                let id = word_index * 64 + remaining.trailing_zeros() as usize;
                                if id < postings.doc_count {
                                    candidates.push(id as u32);
                                }
                                remaining &= remaining - 1;
                            }
                        }
                    }
                }
            }
            let mut matches = Vec::new();
            for doc_id in candidates {
                if distinct
                    .iter()
                    .skip(1)
                    .any(|&term_id| !postings.contains(term_id, doc_id))
                {
                    continue;
                }
                let index = doc_id as usize;
                let start = document_starts[index];
                let mut end = document_starts[index + 1];
                if end > start && docs[end - 1] == b'\n' {
                    end -= 1;
                }
                let line = &docs[start..end];
                let text_start = line
                    .iter()
                    .position(|&byte| byte == b'\t')
                    .map(|tab| start + tab + 1)
                    .unwrap_or(end);
                if phrase_matches(&docs[text_start..end], phrase, vocabulary, map) {
                    matches.push(doc_id);
                }
            }
            append_result(output, &matches);
        }
    }
}

fn run(docs: &[u8]) -> io::Result<()> {
    let query_bytes = fs::read("/data/queries.txt")?;
    let mut vocabulary = Vocabulary::new();
    let queries = parse_queries(&query_bytes, &mut vocabulary);
    let map = &BYTE_MAP;
    let thread_count = thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .min(MAX_THREADS)
        .max(1);
    let ranges = line_chunks(docs, thread_count);

    let chunk_counts = count_chunks(docs, &ranges, &vocabulary, map)?;
    let mut df = vec![0u32; vocabulary.len()];
    let mut document_starts = Vec::new();
    let mut thread_doc_bases = Vec::with_capacity(chunk_counts.len());
    let mut thread_term_bases = Vec::with_capacity(chunk_counts.len());
    let mut documents_before = 0usize;
    let mut per_term_before = vec![0u32; vocabulary.len()];

    for chunk in &chunk_counts {
        thread_doc_bases.push(documents_before as u32);
        let mut bases = vec![0u32; vocabulary.len()];
        for term in 0..vocabulary.len() {
            let frequency = chunk.states[term].value;
            df[term] += frequency;
            bases[term] = per_term_before[term];
            per_term_before[term] += frequency;
        }
        thread_term_bases.push(bases);
        documents_before += chunk.documents as usize;
        document_starts.extend_from_slice(&chunk.starts);
    }
    document_starts.push(docs.len());

    let threshold = (documents_before / 32).max(1);
    let word_count = documents_before.div_ceil(64) as u32;
    let mut term_refs = vec![
        TermRef {
            start: 0,
            tagged_len: 0
        };
        vocabulary.len()
    ];
    let mut sparse_total = 0u32;
    let mut bitset_total = 0u32;

    // Replace each total df with that term's sparse arena start. Dense terms
    // receive a separate bitset range; per-thread prefixes are added below.
    for term in 0..vocabulary.len() {
        let frequency = df[term];
        if frequency as usize >= threshold {
            term_refs[term] = TermRef {
                start: bitset_total,
                tagged_len: DENSE_TAG | frequency,
            };
            bitset_total += word_count;
            df[term] = 0;
        } else {
            term_refs[term] = TermRef {
                start: sparse_total,
                tagged_len: frequency,
            };
            df[term] = sparse_total;
            sparse_total += frequency;
        }
    }
    for bases in &mut thread_term_bases {
        for term in 0..vocabulary.len() {
            if !term_refs[term].is_dense() {
                bases[term] += term_refs[term].start;
            }
        }
    }
    // Pass-one per-thread counts are no longer needed; only line starts and
    // the u32 pass-two bases remain.
    drop(chunk_counts);
    drop(per_term_before);

    let sparse_atomic: Vec<AtomicU32> = (0..sparse_total).map(|_| AtomicU32::new(0)).collect();
    let bitset_atomic: Vec<AtomicU64> = (0..bitset_total).map(|_| AtomicU64::new(0)).collect();
    write_chunks(
        docs,
        &ranges,
        &thread_doc_bases,
        &thread_term_bases,
        &term_refs,
        &vocabulary,
        map,
        &sparse_atomic,
        &bitset_atomic,
    )?;

    // Consume the atomic arenas into ordinary vectors; no second per-term
    // posting copy or per-term heap allocation is made.
    let sparse: Vec<u32> = sparse_atomic
        .into_iter()
        .map(AtomicU32::into_inner)
        .collect();
    let bitsets: Vec<u64> = bitset_atomic
        .into_iter()
        .map(AtomicU64::into_inner)
        .collect();
    let postings = PostingIndex::from_arenas(sparse, bitsets, term_refs, documents_before);

    let block_count = queries.len().div_ceil(QUERY_BLOCK_SIZE);
    let worker_count = thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .min(MAX_THREADS)
        .min(block_count.max(1));
    let next_block = AtomicUsize::new(0);
    let (sender, receiver) = mpsc::channel();

    thread::scope(|scope| {
        for _ in 0..worker_count {
            let sender = sender.clone();
            let queries = &queries;
            let document_starts = &document_starts;
            let postings = &postings;
            let vocabulary = &vocabulary;
            let next_block = &next_block;
            scope.spawn(move || {
                let mut scratch = Scratch::new();
                loop {
                    let block = next_block.fetch_add(1, Ordering::Relaxed);
                    if block >= block_count {
                        break;
                    }
                    let start = block * QUERY_BLOCK_SIZE;
                    let end = (start + QUERY_BLOCK_SIZE).min(queries.len());
                    let mut output = Vec::new();
                    for query in &queries[start..end] {
                        evaluate_query(
                            &mut output,
                            query,
                            docs,
                            document_starts,
                            postings,
                            vocabulary,
                            &map,
                            &mut scratch,
                            threshold,
                        );
                    }
                    if sender.send((block, output)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(sender);
    });

    // Workers finish blocks in arbitrary order; index them before writing so
    // thread scheduling never changes the result byte order.
    let mut blocks: Vec<Option<Vec<u8>>> =
        std::iter::repeat_with(|| None).take(block_count).collect();
    for (block, output) in receiver {
        blocks[block] = Some(output);
    }

    fs::create_dir_all("/out")?;
    let mut file = fs::File::create("/out/results.txt")?;
    for output in blocks.into_iter().flatten() {
        file.write_all(&output)?;
    }
    file.flush()?;
    drop(file);
    std::process::exit(0)
}

fn main() -> io::Result<()> {
    let file = File::open("/data/docs.txt")?;
    let length = usize::try_from(file.metadata()?.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "docs.txt is too large to map"))?;
    if length == 0 {
        return run(&[]);
    }
    if length > isize::MAX as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "docs.txt is too large to map",
        ));
    }

    // The file stays open and the mapping is read-only for this entire call.
    // Length is nonzero and fits isize; MAP_FAILED is checked before forming a slice.
    unsafe {
        let mapping = mmap(
            std::ptr::null_mut(),
            length,
            PROT_READ,
            MAP_PRIVATE,
            file.as_raw_fd(),
            0,
        );
        if mapping as isize == -1 {
            return Err(io::Error::last_os_error());
        }
        let docs = std::slice::from_raw_parts(mapping.cast::<u8>(), length);
        run(docs)
    }
}
