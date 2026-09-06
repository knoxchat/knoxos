/// Tokenizer — BPE (Byte Pair Encoding) tokenizer for AI inference
///
/// Provides:
///   - BPE merge rules application
///   - Vocabulary lookup (token ↔ ID)
///   - Special token handling ([PAD], [UNK], [CLS], [SEP], [BOS], [EOS])
///   - Pre-tokenization (whitespace split, byte-level)
///   - Decode token IDs back to text
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// VOCABULARY
// ═══════════════════════════════════════════════════════════════════════

/// A token in the vocabulary
#[derive(Debug, Clone)]
pub struct Token {
    pub id: u32,
    pub text: String,
    pub score: f32,
    pub is_special: bool,
}

/// BPE merge rule: merge pair (a, b) → merged
#[derive(Debug, Clone)]
pub struct MergeRule {
    pub a: String,
    pub b: String,
    pub merged: String,
    pub priority: u32,
}

/// The tokenizer
pub struct Tokenizer {
    /// Token ID → text
    id_to_token: BTreeMap<u32, Token>,
    /// Text → token ID
    token_to_id: BTreeMap<String, u32>,
    /// BPE merge rules (ordered by priority)
    merges: Vec<MergeRule>,
    /// Special tokens
    pub pad_id: u32,
    pub unk_id: u32,
    pub bos_id: u32,
    pub eos_id: u32,
    /// Vocabulary size
    pub vocab_size: u32,
}

impl Tokenizer {
    pub fn new() -> Self {
        let mut tok = Self {
            id_to_token: BTreeMap::new(),
            token_to_id: BTreeMap::new(),
            merges: Vec::new(),
            pad_id: 0,
            unk_id: 1,
            bos_id: 2,
            eos_id: 3,
            vocab_size: 0,
        };

        // Add special tokens
        tok.add_token(0, "[PAD]", 0.0, true);
        tok.add_token(1, "[UNK]", 0.0, true);
        tok.add_token(2, "[BOS]", 0.0, true);
        tok.add_token(3, "[EOS]", 0.0, true);

        // Add byte-level tokens (256 bytes)
        for b in 0u8..=255 {
            let text = alloc::format!("<0x{:02X}>", b);
            tok.add_token(4 + b as u32, &text, -1.0, false);
        }

        tok.vocab_size = 260;
        tok
    }

    /// Add a token to the vocabulary
    pub fn add_token(&mut self, id: u32, text: &str, score: f32, is_special: bool) {
        let token = Token {
            id,
            text: String::from(text),
            score,
            is_special,
        };
        self.token_to_id.insert(String::from(text), id);
        self.id_to_token.insert(id, token);
    }

    /// Add a BPE merge rule
    pub fn add_merge(&mut self, a: &str, b: &str) {
        let priority = self.merges.len() as u32;
        let merged = alloc::format!("{}{}", a, b);
        self.merges.push(MergeRule {
            a: String::from(a),
            b: String::from(b),
            merged: merged.clone(),
            priority,
        });

        // Add the merged token if not already in vocab
        if !self.token_to_id.contains_key(&merged) {
            let id = self.vocab_size;
            self.add_token(id, &merged, -(priority as f32), false);
            self.vocab_size += 1;
        }
    }

    /// Tokenize text into token IDs
    pub fn encode(&self, text: &str) -> Vec<u32> {
        if text.is_empty() {
            return Vec::new();
        }

        // Pre-tokenization: split on whitespace, keep whitespace as prefix
        let words = self.pre_tokenize(text);

        let mut all_ids = Vec::new();
        for word in &words {
            let ids = self.encode_word(word);
            all_ids.extend(ids);
        }

        all_ids
    }

    /// Encode with special tokens (BOS + tokens + EOS)
    pub fn encode_with_special(&self, text: &str) -> Vec<u32> {
        let mut ids = Vec::with_capacity(text.len() + 2);
        ids.push(self.bos_id);
        ids.extend(self.encode(text));
        ids.push(self.eos_id);
        ids
    }

    /// Pre-tokenize: split text into words
    fn pre_tokenize(&self, text: &str) -> Vec<String> {
        let mut words = Vec::new();
        let mut current = String::new();

        for c in text.chars() {
            if c.is_whitespace() {
                if !current.is_empty() {
                    words.push(current.clone());
                    current.clear();
                }
                // Whitespace becomes a token prefix for the next word
                current.push(c);
            } else {
                current.push(c);
            }
        }
        if !current.is_empty() {
            words.push(current);
        }

        words
    }

    /// Encode a single word using BPE
    fn encode_word(&self, word: &str) -> Vec<u32> {
        // Start with individual characters (or bytes)
        let mut symbols: Vec<String> = word
            .chars()
            .map(|c| String::from(c.encode_utf8(&mut [0u8; 4])))
            .collect();

        // Apply BPE merges iteratively
        loop {
            if symbols.len() < 2 {
                break;
            }

            // Find the highest-priority merge that applies
            let mut best_merge: Option<(usize, &MergeRule)> = None;

            for i in 0..symbols.len() - 1 {
                for merge in &self.merges {
                    if symbols[i] == merge.a && symbols[i + 1] == merge.b {
                        match best_merge {
                            None => best_merge = Some((i, merge)),
                            Some((_, best)) => {
                                if merge.priority < best.priority {
                                    best_merge = Some((i, merge));
                                }
                            }
                        }
                        break;
                    }
                }
            }

            match best_merge {
                Some((idx, merge)) => {
                    let merged = merge.merged.clone();
                    symbols[idx] = merged;
                    symbols.remove(idx + 1);
                }
                None => break,
            }
        }

        // Convert symbols to token IDs
        symbols
            .iter()
            .map(|s| self.token_to_id.get(s).copied().unwrap_or(self.unk_id))
            .collect()
    }

    /// Decode token IDs back to text
    pub fn decode(&self, ids: &[u32]) -> String {
        let mut result = String::new();
        for &id in ids {
            if let Some(token) = self.id_to_token.get(&id) {
                if !token.is_special {
                    result.push_str(&token.text);
                }
            }
        }
        result
    }

    /// Get token text by ID
    pub fn token_text(&self, id: u32) -> Option<&str> {
        self.id_to_token.get(&id).map(|t| t.text.as_str())
    }

    /// Get token ID by text
    pub fn token_id(&self, text: &str) -> Option<u32> {
        self.token_to_id.get(text).copied()
    }
}

lazy_static::lazy_static! {
    pub static ref TOKENIZER: Mutex<Tokenizer> = Mutex::new(Tokenizer::new());
}

/// Initialize tokenizer with basic English merges
pub fn init() {
    let mut tok = TOKENIZER.lock();

    // Add common English character pairs as merges
    let common_merges: &[(&str, &str)] = &[
        ("t", "h"),
        ("th", "e"),
        ("i", "n"),
        ("a", "n"),
        ("e", "r"),
        ("o", "n"),
        ("r", "e"),
        ("e", "d"),
        ("i", "s"),
        ("e", "s"),
        ("o", "r"),
        ("t", "i"),
        ("ti", "on"),
        ("a", "t"),
        ("e", "n"),
        ("a", "l"),
        ("h", "e"),
        ("s", "t"),
        ("i", "t"),
        ("a", "r"),
        ("l", "e"),
        ("o", "u"),
        ("n", "d"),
        ("o", "f"),
    ];

    for &(a, b) in common_merges {
        tok.add_merge(a, b);
    }

    drop(tok);
    serial_println!(
        "[KnoxOS] BPE Tokenizer initialized (vocab_size={})",
        TOKENIZER.lock().vocab_size
    );
}
