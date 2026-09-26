// ═══════════════════════════════════════════════════════════════════════
// RTL TEXT TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::gui::rtl_text::BidiClass;

#[test_case]
fn test_bidi_class_variants() {
    // Verify BidiClass types can be constructed
    let classes = [
        BidiClass::L,
        BidiClass::R,
        BidiClass::AL,
        BidiClass::EN,
        BidiClass::AN,
        BidiClass::ON,
        BidiClass::WS,
    ];
    assert_eq!(classes.len(), 7);
}
