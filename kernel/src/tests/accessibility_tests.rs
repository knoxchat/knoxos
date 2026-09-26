// ═══════════════════════════════════════════════════════════════════════
// ACCESSIBILITY TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::gui::accessibility::{ElementType, FocusManager, FocusableElement};
use crate::gui::framebuffer::Rect;
use crate::gui::theme_switch::ThemeColors;
use alloc::string::String;

fn contrast_ratio(a: u32, b: u32) -> u32 {
    fn lum(c: u32) -> u32 {
        let r = ((c >> 16) & 0xFF) as u32;
        let g = ((c >> 8) & 0xFF) as u32;
        let b = (c & 0xFF) as u32;
        3 * r + 6 * g + b
    }
    let (l1, l2) = (lum(a), lum(b));
    let (hi, lo) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (hi + 10) * 10 / (lo + 10)
}

fn el(id: u32, tab: i32, label: &str) -> FocusableElement {
    FocusableElement {
        id,
        label: String::from(label),
        rect: Rect::new(0, 0, 10, 10),
        tab_index: tab,
        focusable: true,
        element_type: ElementType::Button,
    }
}

#[test_case]
fn test_high_contrast_theme_colors() {
    let t = ThemeColors::high_contrast();
    let ratio = contrast_ratio(t.bg_primary, t.text_primary);
    // WCAG AAA body text is 7:1; black/white is ~21:1.
    assert!(ratio >= 70, "contrast {} (tenths) below 7:1", ratio);
}

#[test_case]
fn test_screen_reader_element_traversal() {
    let mut fm = FocusManager::new();
    fm.register(el(1, 0, "one"));
    fm.register(el(2, 1, "two"));
    fm.register(el(3, 2, "three"));
    fm.focus_next();
    assert_eq!(fm.focused().unwrap().id, 1);
    fm.focus_next();
    assert_eq!(fm.focused().unwrap().id, 2);
    fm.focus_next();
    assert_eq!(fm.focused().unwrap().id, 3);
}

#[test_case]
fn test_keyboard_navigation() {
    let mut fm = FocusManager::new();
    fm.register(el(10, 0, "a"));
    fm.register(el(11, 1, "b"));
    fm.focus_next();
    fm.focus_next();
    assert_eq!(fm.focused().unwrap().id, 11);
    fm.focus_next(); // wrap
    assert_eq!(fm.focused().unwrap().id, 10);
    fm.focus_prev();
    assert_eq!(fm.focused().unwrap().id, 11);
}
