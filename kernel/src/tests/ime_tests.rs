// ═══════════════════════════════════════════════════════════════════════
// IME TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::gui::ime::{ImeMode, ImeState};

#[test_case]
fn test_ime_off_passthrough() {
    let mut ime = ImeState::new();
    assert_eq!(ime.mode, ImeMode::Off);
    let result = ime.process_key('a');
    assert_eq!(result, Some(alloc::string::String::from("a")));
}

#[test_case]
fn test_ime_pinyin_mode() {
    let mut ime = ImeState::new();
    ime.mode = ImeMode::Pinyin;
    // Type 'w' + 'o' → should generate candidates for "我"
    let _ = ime.process_key('w');
    let _ = ime.process_key('o');
    assert!(!ime.candidates.is_empty() || !ime.preedit.is_empty());
}

#[test_case]
fn test_ime_cancel() {
    let mut ime = ImeState::new();
    ime.mode = ImeMode::Pinyin;
    let _ = ime.process_key('n');
    let _ = ime.process_key('i');
    ime.cancel();
    assert!(ime.input_buffer.is_empty());
    assert!(ime.candidates.is_empty());
    assert!(!ime.visible);
}
