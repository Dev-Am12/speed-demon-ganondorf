use std::fs;
use std::io::{self, Write};

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

// Query terms fit in 32 bytes. Padding makes equality independent of token length.
#[derive(Clone, Copy, PartialEq, Eq)]
struct TermKey([u64; 4]);

impl TermKey {
    fn from_bytes(term: &[u8]) -> Self {
        let mut bytes = [0u8; 32];
        let length = term.len().min(bytes.len());
        bytes[..length].copy_from_slice(&term[..length]);
        Self::from_padded(bytes)
    }

    fn from_padded(bytes: [u8; 32]) -> Self {
        let mut words = [0u64; 4];
        for (index, word) in words.iter_mut().enumerate() {
            let start = index * 8;
            // These fixed slices always contain exactly eight bytes.
            let chunk: [u8; 8] = bytes[start..start + 8].try_into().unwrap();
            *word = u64::from_le_bytes(chunk);
        }
        Self(words)
    }
}

#[derive(Clone, Copy)]
struct Entry {
    key: TermKey,
    id: u32,
}

const EMPTY_ID: u32 = u32::MAX;

impl Entry {
    const EMPTY: Self = Self {
        key: TermKey([0; 4]),
        id: EMPTY_ID,
    };
}

struct Vocabulary {
    entries: Vec<Entry>,
    len: usize,
}

impl Vocabulary {
    fn new() -> Self {
        Self {
            entries: vec![Entry::EMPTY; 8],
            len: 0,
        }
    }

    fn get_key(&self, key: TermKey) -> Option<u32> {
        let mask = self.entries.len() - 1;
        let mut slot = (hash_key(key) as usize) & mask;
        loop {
            let entry = self.entries[slot];
            if entry.id == EMPTY_ID {
                return None;
            }
            if entry.key == key {
                return Some(entry.id);
            }
            slot = (slot + 1) & mask;
        }
    }

    fn intern(&mut self, term: &[u8]) -> u32 {
        let key = TermKey::from_bytes(term);
        if let Some(id) = self.get_key(key) {
            return id;
        }
        if self.len.saturating_add(1) > self.entries.len() / 2 {
            self.grow();
        }
        let id = self.len as u32;
        self.insert_entry(Entry { key, id });
        self.len += 1;
        id
    }

    fn grow(&mut self) {
        let new_capacity = self.entries.len() * 2;
        let old_entries =
            std::mem::replace(&mut self.entries, vec![Entry::EMPTY; new_capacity]);
        self.len = 0;
        for entry in old_entries {
            if entry.id != EMPTY_ID {
                self.insert_entry(entry);
                self.len += 1;
            }
        }
    }

    fn insert_entry(&mut self, entry: Entry) {
        let mask = self.entries.len() - 1;
        let mut slot = (hash_key(entry.key) as usize) & mask;
        while self.entries[slot].id != EMPTY_ID {
            slot = (slot + 1) & mask;
        }
        self.entries[slot] = entry;
    }
}

// Mix all four words deterministically; table iteration never produces output.
fn hash_key(key: TermKey) -> u64 {
    let mut hash = 0x9e37_79b9_7f4a_7c15u64;
    for mut word in key.0 {
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

// Shared by indexing and phrase verification. Overlong runs consume a token
// position but cannot match; shorter keys have their unused bytes zeroed.
fn tokenize<F>(text: &[u8], mut visit: F)
where
    F: FnMut(Option<TermKey>),
{
    let mut token = [0u8; 32];
    let mut length = 0usize;

    for &byte in text {
        let lower = byte.to_ascii_lowercase();
        if lower.is_ascii_lowercase() || lower.is_ascii_digit() {
            if length < token.len() {
                token[length] = lower;
            }
            length += 1;
        } else if length != 0 {
            if length <= token.len() {
                token[length..].fill(0);
                visit(Some(TermKey::from_padded(token)));
            } else {
                visit(None);
            }
            token.fill(0);
            length = 0;
        }
    }

    if length != 0 {
        if length <= token.len() {
            token[length..].fill(0);
            visit(Some(TermKey::from_padded(token)));
        } else {
            visit(None);
        }
    }
}
fn intersect(left: &[u32], right: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(left.len().min(right.len()));
    let (mut i, mut j) = (0usize, 0usize);
    while i < left.len() && j < right.len() {
        match left[i].cmp(&right[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                out.push(left[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out
}

fn union(left: &[u32], right: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(left.len().saturating_add(right.len()));
    let (mut i, mut j) = (0usize, 0usize);
    while i < left.len() && j < right.len() {
        match left[i].cmp(&right[j]) {
            std::cmp::Ordering::Less => {
                out.push(left[i]);
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                out.push(right[j]);
                j += 1;
            }
            std::cmp::Ordering::Equal => {
                out.push(left[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out.extend_from_slice(&left[i..]);
    out.extend_from_slice(&right[j..]);
    out
}

fn difference(left: &[u32], right: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(left.len());
    let (mut i, mut j) = (0usize, 0usize);
    while i < left.len() {
        while j < right.len() && right[j] < left[i] {
            j += 1;
        }
        if j == right.len() || right[j] != left[i] {
            out.push(left[i]);
        }
        i += 1;
    }
    out
}

fn phrase_matches(text: &[u8], phrase: &[u32], vocabulary: &Vocabulary) -> bool {
    let width = phrase.len();
    let mut ring = [u32::MAX; 8];
    let mut position = 0usize;
    let mut matched = false;
    tokenize(text, |token| {
        let id = token
            .and_then(|key| vocabulary.get_key(key))
            .unwrap_or(u32::MAX);
        ring[position % width] = id;
        if position + 1 >= width {
            let first = (position + 1 - width) % width;
            let mut equal = true;
            for offset in 0..width {
                if ring[(first + offset) % width] != phrase[offset] { equal = false; break; }
            }
            if equal { matched = true; }
        }
        position += 1;
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

fn main() -> io::Result<()> {
    let query_bytes = fs::read("/data/queries.txt")?;
    let mut vocabulary = Vocabulary::new();
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

    let mut postings = (0..vocabulary.len)
        .map(|_| Vec::<u32>::new())
        .collect::<Vec<_>>();
    let docs = fs::read("/data/docs.txt")?;
    let mut ranges = Vec::<(usize, usize)>::new();
    let mut line_start = 0usize;
    while line_start < docs.len() {
        let line_end = docs[line_start..]
            .iter()
            .position(|&byte| byte == b'\n')
            .map(|offset| line_start + offset)
            .unwrap_or(docs.len());
        let line = &docs[line_start..line_end];
        let doc_id = ranges.len() as u32;
        if let Some(tab) = line.iter().position(|&byte| byte == b'\t') {
            let text_start = line_start + tab + 1;
            let text_end = line_end;
            ranges.push((text_start, text_end));
            tokenize(&docs[text_start..text_end], |token| {
                if let Some(key) = token {
                    if let Some(term_id) = vocabulary.get_key(key) {
                        if let Some(list) = postings.get_mut(term_id as usize) {
                            if list.last().copied() != Some(doc_id) {
                                list.push(doc_id);
                            }
                        }
                    }
                }
            });
        } else {
            // Even a line without a TAB occupies its document id.
            ranges.push((line_end, line_end));
        }
        if line_end == docs.len() {
            break;
        }
        line_start = line_end + 1;
    }

    let mut output = Vec::new();
    for query in &queries {
        match query {
            Query::Bool { terms, ops } => {
                let mut result = terms
                    .first()
                    .and_then(|&id| postings.get(id as usize))
                    .cloned()
                    .unwrap_or_default();
                for (index, op) in ops.iter().enumerate() {
                    let rhs = terms
                        .get(index + 1)
                        .and_then(|&id| postings.get(id as usize))
                        .map(Vec::as_slice)
                        .unwrap_or(&[]);
                    result = match op {
                        Op::And => intersect(&result, rhs),
                        Op::Or => union(&result, rhs),
                        Op::Not => difference(&result, rhs),
                    };
                }
                append_result(&mut output, &result);
            }
            Query::Phrase(phrase) => {
                let mut distinct = phrase.clone();
                distinct.sort_unstable();
                distinct.dedup();
                distinct.sort_by_key(|&id| {
                    postings
                        .get(id as usize)
                        .map(Vec::len)
                        .unwrap_or(usize::MAX)
                });

                let mut candidates = distinct
                    .first()
                    .and_then(|&id| postings.get(id as usize))
                    .cloned()
                    .unwrap_or_default();
                for &term_id in distinct.iter().skip(1) {
                    let list = postings
                        .get(term_id as usize)
                        .map(Vec::as_slice)
                        .unwrap_or(&[]);
                    candidates = intersect(&candidates, list);
                    if candidates.is_empty() {
                        break;
                    }
                }

                let mut matches = Vec::new();
                for doc_id in candidates {
                    if let Some(&(start, end)) = ranges.get(doc_id as usize) {
                        if phrase_matches(&docs[start..end], phrase, &vocabulary) {
                            matches.push(doc_id);
                        }
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