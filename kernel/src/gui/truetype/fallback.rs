//! Configurable font fallback priority chain (Latin / CJK / emoji).
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

// ═══════════════════════════════════════════════════════════════════════
// FONT FALLBACK SYSTEM — Configurable priority chain
// ═══════════════════════════════════════════════════════════════════════

/// Font fallback entry with priority
#[derive(Clone, Debug)]
pub struct FontFallbackEntry {
    pub name: String,
    pub priority: u8,
    pub covers_cjk: bool,
    pub covers_emoji: bool,
    pub covers_symbols: bool,
}

lazy_static::lazy_static! {
    static ref FONT_FALLBACK_CHAIN: Mutex<Vec<FontFallbackEntry>> = Mutex::new(vec![
        FontFallbackEntry {
            name: String::from("Inter"),
            priority: 0,
            covers_cjk: false,
            covers_emoji: false,
            covers_symbols: true,
        },
        FontFallbackEntry {
            name: String::from("Hack"),
            priority: 1,
            covers_cjk: false,
            covers_emoji: false,
            covers_symbols: true,
        },
        FontFallbackEntry {
            name: String::from("CJK-Bitmap"),
            priority: 2,
            covers_cjk: true,
            covers_emoji: false,
            covers_symbols: false,
        },
        FontFallbackEntry {
            name: String::from("Emoji-Color"),
            priority: 3,
            covers_cjk: false,
            covers_emoji: true,
            covers_symbols: false,
        },
    ]);
}

/// Set the font fallback priority chain
pub fn set_fallback_chain(entries: Vec<FontFallbackEntry>) {
    let mut chain = FONT_FALLBACK_CHAIN.lock();
    *chain = entries;
    chain.sort_by_key(|e| e.priority);
    crate::serial_println!("[fonts] Fallback chain updated: {} entries", chain.len());
}

/// Get the current font fallback chain
pub fn get_fallback_chain() -> Vec<FontFallbackEntry> {
    FONT_FALLBACK_CHAIN.lock().clone()
}

/// Add a font to the fallback chain
pub fn add_fallback_font(entry: FontFallbackEntry) {
    let mut chain = FONT_FALLBACK_CHAIN.lock();
    chain.push(entry);
    chain.sort_by_key(|e| e.priority);
}

/// Resolve which font should render a given codepoint
pub fn resolve_font_for_codepoint(codepoint: u32) -> String {
    let chain = FONT_FALLBACK_CHAIN.lock();
    let is_cjk = crate::gui::unicode::is_cjk(codepoint);
    let is_emoji = crate::gui::unicode::is_emoji(codepoint);

    for entry in chain.iter() {
        if is_cjk {
            if entry.covers_cjk {
                return entry.name.clone();
            }
            continue;
        }
        if is_emoji {
            if entry.covers_emoji {
                return entry.name.clone();
            }
            continue;
        }
        // Latin / symbols: skip CJK-only and emoji-only faces.
        if entry.covers_cjk && !entry.covers_symbols {
            continue;
        }
        if entry.covers_emoji && !entry.covers_symbols {
            continue;
        }
        return entry.name.clone();
    }
    // Ultimate fallback
    String::from("Inter")
}
