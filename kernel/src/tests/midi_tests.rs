// ═══════════════════════════════════════════════════════════════════════
// MIDI TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::midi;

#[test_case]
fn test_gm_instrument_names() {
    // GM instrument 0 should be "Acoustic Grand Piano"
    let name = midi::gm_instrument_name(0);
    assert!(!name.is_empty(), "GM instrument 0 should have a name");
}

#[test_case]
fn test_note_to_freq_a4() {
    // MIDI note 69 = A4 = 440 Hz
    let freq = midi::note_to_freq(69);
    assert!(
        freq >= 439 && freq <= 441,
        "A4 should be ~440 Hz, got {}",
        freq
    );
}
