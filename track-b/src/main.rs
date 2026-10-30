use std::ffi::c_void;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;
use std::thread;

const EMPTY_ID: u32 = u32::MAX;
const MAX_TERM_LEN: usize = 32;
const MAX_THREADS: usize = 12;
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

// The open-addressing slots contain only ids; term bytes live once in this
// compact vocabulary and are compared after a hash collision.
struct Vocabulary {
    slots: Vec<u32>,
    terms: Vec<Vec<u8>>,
}

impl Vocabulary {
    fn new() -> Self {
        Self {
            slots: vec![EMPTY_ID; 8],
            terms: Vec::new(),
        }
    }

    fn len(&self) -> usize {
        self.terms.len()
    }

    fn get(&self, term: &[u8]) -> Option<u32> {
        if term.is_empty() || term.len() > MAX_TERM_LEN {
            return None;
        }
        let mask = self.slots.len() - 1;
        let mut slot = (hash_term(term) as usize) & mask;
        loop {
            let id = self.slots[slot];
            if id == EMPTY_ID {
                return None;
            }
            if self.terms[id as usize].as_slice() == term {
                return Some(id);
            }
            slot = (slot + 1) & mask;
        }
    }

    fn intern(&mut self, term: &[u8]) -> u32 {
        if let Some(id) = self.get(term) {
            return id;
        }
        if self.terms.len().saturating_add(1) > self.slots.len() / 2 {
            self.grow();
        }
        let id = self.terms.len() as u32;
        self.terms.push(term.to_vec());
        self.insert_existing(id);
        id
    }

    fn grow(&mut self) {
        let new_capacity = self.slots.len() * 2;
        let old_slots = std::mem::replace(&mut self.slots, vec![EMPTY_ID; new_capacity]);
        for id in old_slots {
            if id != EMPTY_ID {
                self.insert_existing(id);
            }
        }
    }

    fn insert_existing(&mut self, id: u32) {
        let mask = self.slots.len() - 1;
        let mut slot = (hash_term(&self.terms[id as usize]) as usize) & mask;
        while self.slots[slot] != EMPTY_ID {
            slot = (slot + 1) & mask;
        }
        self.slots[slot] = id;
    }
}

// Process up to four 8-byte words per term; the only zero padding is the
// final partial word, avoiding a 32-byte clear for each token.
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

// Class 0 is a separator, 1 is a lowercase letter or digit, and 2 is an
// uppercase ASCII letter that needs only its ASCII lowercase bit set.
static BYTE_CLASS: OnceLock<[u8; 256]> = OnceLock::new();

fn byte_classes() -> &'static [u8; 256] {
    BYTE_CLASS.get_or_init(|| {
        let mut classes = [0u8; 256];
        for byte in b'a'..=b'z' {
            classes[byte as usize] = 1;
        }
        for byte in b'A'..=b'Z' {
            classes[byte as usize] = 2;
        }
        for byte in b'0'..=b'9' {
            classes[byte as usize] = 1;
        }
        classes
    })
}

// Shared by both scans and phrase verification. It never truncates an
// overlong run and lets a verifier stop as soon as its phrase is found.
fn tokenize<F>(text: &[u8], classes: &[u8; 256], mut visit: F)
where
    F: FnMut(Option<&[u8]>) -> bool,
{
    let mut token = [0u8; MAX_TERM_LEN];
    let mut length = 0usize;
    for &byte in text {
        let class = classes[byte as usize];
        if class != 0 {
            if length < MAX_TERM_LEN {
                token[length] = if class == 2 { byte | 0x20 } else { byte };
            }
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

struct ChunkCount {
    df: Vec<u32>,
    starts: Vec<usize>,
}

fn scan_chunk_count(
    bytes: &[u8],
    absolute_start: usize,
    vocabulary: &Vocabulary,
    classes: &[u8; 256],
) -> ChunkCount {
    let mut df = vec![0u32; vocabulary.len()];
    let mut last_seen = vec![u32::MAX; vocabulary.len()];
    let mut starts = Vec::new();
    let mut position = 0usize;
    let mut local_doc = 0u32;

    while position < bytes.len() {
        starts.push(absolute_start + position);
        let line_end = bytes[position..]
            .iter()
            .position(|&byte| byte == b'\n')
            .map(|offset| position + offset)
            .unwrap_or(bytes.len());
        let line = &bytes[position..line_end];
        if let Some(tab) = line.iter().position(|&byte| byte == b'\t') {
            tokenize(&line[tab + 1..], classes, |token| {
                if let Some(term) = token {
                    if let Some(id) = vocabulary.get(term) {
                        let index = id as usize;
                        if last_seen[index] != local_doc {
                            last_seen[index] = local_doc;
                            df[index] += 1;
                        }
                    }
                }
                true
            });
        }
        local_doc += 1;
        if line_end == bytes.len() {
            break;
        }
        position = line_end + 1;
    }

    ChunkCount { df, starts }
}

fn scan_chunk_write(
    bytes: &[u8],
    vocabulary: &Vocabulary,
    classes: &[u8; 256],
    global_doc_start: u32,
    term_bases: &[usize],
    postings: &[AtomicU32],
) {
    let mut last_seen = vec![u32::MAX; vocabulary.len()];
    let mut cursors = vec![0usize; vocabulary.len()];
    let mut position = 0usize;
    let mut local_doc = 0u32;

    while position < bytes.len() {
        let line_end = bytes[position..]
            .iter()
            .position(|&byte| byte == b'\n')
            .map(|offset| position + offset)
            .unwrap_or(bytes.len());
        let line = &bytes[position..line_end];
        if let Some(tab) = line.iter().position(|&byte| byte == b'\t') {
            tokenize(&line[tab + 1..], classes, |token| {
                if let Some(term) = token {
                    if let Some(id) = vocabulary.get(term) {
                        let index = id as usize;
                        if last_seen[index] != local_doc {
                            last_seen[index] = local_doc;
                            let slot = term_bases[index] + cursors[index];
                            cursors[index] += 1;
                            postings[slot].store(global_doc_start + local_doc, Ordering::Relaxed);
                        }
                    }
                }
                true
            });
        }
        local_doc += 1;
        if line_end == bytes.len() {
            break;
        }
        position = line_end + 1;
    }
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
    classes: &[u8; 256],
) -> io::Result<Vec<ChunkCount>> {
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(ranges.len());
        for &(start, end) in ranges {
            let slice = &bytes[start..end];
            handles.push(scope.spawn(move || scan_chunk_count(slice, start, vocabulary, classes)));
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
    term_bases: &[Vec<usize>],
    vocabulary: &Vocabulary,
    classes: &[u8; 256],
    postings: &[AtomicU32],
) -> io::Result<()> {
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(ranges.len());
        for index in 0..ranges.len() {
            let (start, end) = ranges[index];
            let slice = &bytes[start..end];
            let base = &term_bases[index];
            let doc_base = doc_bases[index];
            handles.push(scope.spawn(move || {
                scan_chunk_write(slice, vocabulary, classes, doc_base, base, postings)
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

enum TermSet {
    Empty,
    List { ids: Vec<u32>, df: usize },
    Bits { words: Vec<u64>, df: usize },
}

impl TermSet {
    fn view(&self) -> SetView<'_> {
        match self {
            Self::Empty => SetView::List(&[]),
            Self::List { ids, .. } => SetView::List(ids),
            Self::Bits { words, .. } => SetView::Bits(words),
        }
    }

    fn df(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::List { df, .. } | Self::Bits { df, .. } => *df,
        }
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
    sets: Vec<TermSet>,
    doc_count: usize,
}

impl PostingIndex {
    fn from_flat(
        ids: Vec<u32>,
        offsets: &[usize],
        df: &[u32],
        doc_count: usize,
        threshold: usize,
    ) -> Self {
        let mut sets = Vec::with_capacity(df.len());
        let word_count = doc_count.div_ceil(64);
        for term in 0..df.len() {
            let frequency = df[term] as usize;
            if frequency == 0 {
                sets.push(TermSet::Empty);
            } else if frequency >= threshold {
                let mut words = vec![0u64; word_count];
                for &doc_id in &ids[offsets[term]..offsets[term + 1]] {
                    let index = doc_id as usize;
                    words[index / 64] |= 1u64 << (index % 64);
                }
                sets.push(TermSet::Bits {
                    words,
                    df: frequency,
                });
            } else {
                sets.push(TermSet::List {
                    ids: ids[offsets[term]..offsets[term + 1]].to_vec(),
                    df: frequency,
                });
            }
        }
        Self { sets, doc_count }
    }

    fn set(&self, term_id: u32) -> &TermSet {
        &self.sets[term_id as usize]
    }

    fn view(&self, term_id: u32) -> SetView<'_> {
        self.set(term_id).view()
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

fn phrase_matches(
    text: &[u8],
    phrase: &[u32],
    vocabulary: &Vocabulary,
    classes: &[u8; 256],
) -> bool {
    let width = phrase.len();
    let mut ring = [u32::MAX; 8];
    let mut position = 0usize;
    let mut matched = false;
    tokenize(text, classes, |token| {
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

fn run(docs: &[u8]) -> io::Result<()> {
    let query_bytes = fs::read("/data/queries.txt")?;
    let mut vocabulary = Vocabulary::new();
    let queries = parse_queries(&query_bytes, &mut vocabulary);
    let classes = byte_classes();
    let thread_count = thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .min(MAX_THREADS)
        .max(1);
    let ranges = line_chunks(docs, thread_count);

    let chunk_counts = count_chunks(docs, &ranges, &vocabulary, classes)?;
    let mut df = vec![0u32; vocabulary.len()];
    let mut document_starts = Vec::new();
    let mut thread_doc_bases = Vec::with_capacity(chunk_counts.len());
    let mut thread_term_bases = Vec::with_capacity(chunk_counts.len());
    let mut documents_before = 0usize;
    let mut per_term_before = vec![0usize; vocabulary.len()];

    for chunk in &chunk_counts {
        thread_doc_bases.push(documents_before as u32);
        let mut bases = vec![0usize; vocabulary.len()];
        for term in 0..vocabulary.len() {
            df[term] += chunk.df[term];
            bases[term] = per_term_before[term];
            per_term_before[term] += chunk.df[term] as usize;
        }
        thread_term_bases.push(bases);
        documents_before += chunk.starts.len();
        document_starts.extend_from_slice(&chunk.starts);
    }
    document_starts.push(docs.len());

    let mut offsets = Vec::with_capacity(vocabulary.len() + 1);
    offsets.push(0usize);
    for &frequency in &df {
        let next = offsets.last().copied().unwrap_or(0) + frequency as usize;
        offsets.push(next);
    }
    for bases in &mut thread_term_bases {
        for term in 0..vocabulary.len() {
            bases[term] += offsets[term];
        }
    }

    // The first-pass counts assign each thread a disjoint segment of each
    // term's sorted posting range; atomics make those parallel stores safe.
    let atomic_ids: Vec<AtomicU32> = (0..*offsets.last().unwrap_or(&0))
        .map(|_| AtomicU32::new(0))
        .collect();
    write_chunks(
        docs,
        &ranges,
        &thread_doc_bases,
        &thread_term_bases,
        &vocabulary,
        classes,
        &atomic_ids,
    )?;
    let ids = atomic_ids
        .iter()
        .map(|value| value.load(Ordering::Relaxed))
        .collect();
    // A set at or above N/32 uses a bitset. Scratch copies can change this
    // threshold expression to force either representation during the gate.
    let threshold = (documents_before / 32).max(1);
    let postings = PostingIndex::from_flat(ids, &offsets, &df, documents_before, threshold);
    let mut scratch = Scratch::new();

    let mut output = Vec::new();
    for query in &queries {
        match query {
            Query::Bool { terms, ops } => {
                let mut result = terms
                    .first()
                    .copied()
                    .map(Accumulator::Term)
                    .unwrap_or(Accumulator::List(0));
                for (index, &op) in ops.iter().enumerate() {
                    if let Some(&rhs) = terms.get(index + 1) {
                        result =
                            apply_operator(result, rhs, op, &postings, &mut scratch, threshold);
                    }
                }
                let view = accumulator_view(result, &postings, &scratch);
                append_view_result(&mut output, view, documents_before);
            }
            Query::Phrase(phrase) => {
                let mut distinct = phrase.clone();
                distinct.sort_unstable();
                distinct.dedup();
                distinct.sort_by_key(|&id| postings.set(id).df());
                let mut candidates = Vec::new();
                if let Some(&seed) = distinct.first() {
                    match postings.view(seed) {
                        SetView::List(ids) => candidates.extend_from_slice(ids),
                        SetView::Bits(words) => {
                            for (word_index, &word) in words.iter().enumerate() {
                                let mut remaining = word;
                                while remaining != 0 {
                                    let id = word_index * 64 + remaining.trailing_zeros() as usize;
                                    if id < documents_before {
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
                    if phrase_matches(&docs[text_start..end], phrase, &vocabulary, classes) {
                        matches.push(doc_id);
                    }
                }
                append_result(&mut output, &matches);
            }
        }
    }

    fs::create_dir_all("/out")?;
    let mut file = fs::File::create("/out/results.txt")?;
    file.write_all(&output)?;
    file.flush()?;
    drop(file);
    Ok(())
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
