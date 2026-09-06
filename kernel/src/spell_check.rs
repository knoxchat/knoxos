use crate::serial_println;
/// Spell Checker
///
/// Provides spell checking using Hunspell-compatible dictionaries.
/// Supports suggestion ranking, personal dictionaries, and language detection.
use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// A dictionary for one language
pub struct Dictionary {
    pub language: String,
    pub words: BTreeSet<String>,
    pub affixes: Vec<AffixRule>,
}

/// Affix rule (prefix/suffix)
#[derive(Debug, Clone)]
pub struct AffixRule {
    pub strip: String,
    pub add: String,
    pub condition: String,
    pub is_prefix: bool,
}

/// Spell check result
#[derive(Debug, Clone)]
pub struct SpellResult {
    pub word: String,
    pub correct: bool,
    pub suggestions: Vec<String>,
}

/// Personal dictionary (user-added words)
pub struct PersonalDict {
    pub words: BTreeSet<String>,
}

pub struct SpellChecker {
    dictionaries: Vec<Dictionary>,
    personal: PersonalDict,
    active_lang: String,
}

lazy_static::lazy_static! {
    static ref CHECKER: Mutex<SpellChecker> = Mutex::new(SpellChecker {
        dictionaries: Vec::new(),
        personal: PersonalDict { words: BTreeSet::new() },
        active_lang: String::from("en_US"),
    });
}

impl SpellChecker {
    /// Check if a word is spelled correctly
    pub fn check(&self, word: &str) -> bool {
        let lower = word.to_lowercase();
        // Check personal dictionary first
        if self.personal.words.contains(&lower) {
            return true;
        }
        // Check active language dictionary
        if let Some(dict) = self
            .dictionaries
            .iter()
            .find(|d| d.language == self.active_lang)
        {
            if dict.words.contains(&lower) {
                return true;
            }
            // Try with affixes stripped
            for rule in &dict.affixes {
                if let Some(stem) = apply_affix_reverse(&lower, rule) {
                    if dict.words.contains(&stem) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Get spelling suggestions for a misspelled word
    pub fn suggest(&self, word: &str) -> Vec<String> {
        let lower = word.to_lowercase();
        let dict = match self
            .dictionaries
            .iter()
            .find(|d| d.language == self.active_lang)
        {
            Some(d) => d,
            None => return Vec::new(),
        };

        let mut candidates = Vec::new();
        // Edit distance 1
        for dict_word in &dict.words {
            let dist = edit_distance(&lower, dict_word);
            if dist <= 2 {
                candidates.push((dist, dict_word.clone()));
            }
        }
        candidates.sort_by_key(|(d, _)| *d);
        candidates.into_iter().take(8).map(|(_, w)| w).collect()
    }

    /// Check a block of text and return results for each word
    pub fn check_text(&self, text: &str) -> Vec<SpellResult> {
        let mut results = Vec::new();
        for word in text.split(|c: char| !c.is_alphabetic()) {
            if word.is_empty() || word.len() <= 1 {
                continue;
            }
            let correct = self.check(word);
            let suggestions = if correct {
                Vec::new()
            } else {
                self.suggest(word)
            };
            results.push(SpellResult {
                word: String::from(word),
                correct,
                suggestions,
            });
        }
        results
    }

    /// Add word to personal dictionary
    pub fn add_to_personal(&mut self, word: &str) {
        self.personal.words.insert(word.to_lowercase());
    }

    /// Set active language
    pub fn set_language(&mut self, lang: &str) {
        self.active_lang = String::from(lang);
    }
}

fn apply_affix_reverse(word: &str, rule: &AffixRule) -> Option<String> {
    if rule.is_prefix {
        if word.starts_with(rule.add.as_str()) {
            let stem = alloc::format!("{}{}", rule.strip, &word[rule.add.len()..]);
            return Some(stem);
        }
    } else if word.ends_with(rule.add.as_str()) {
        let stem = alloc::format!("{}{}", &word[..word.len() - rule.add.len()], rule.strip);
        return Some(stem);
    }
    None
}

/// Simple Levenshtein edit distance
fn edit_distance(a: &str, b: &str) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let m = a_bytes.len();
    let n = b_bytes.len();
    let mut dp = Vec::with_capacity(n + 1);
    for j in 0..=n {
        dp.push(j);
    }
    for i in 1..=m {
        let mut prev = dp[0];
        dp[0] = i;
        for j in 1..=n {
            let cost = if a_bytes[i - 1] == b_bytes[j - 1] {
                0
            } else {
                1
            };
            let val = (dp[j] + 1).min(dp[j - 1] + 1).min(prev + cost);
            prev = dp[j];
            dp[j] = val;
        }
    }
    dp[n]
}

pub fn init() {
    serial_println!("[SPELL] Spell checker loaded");
}
