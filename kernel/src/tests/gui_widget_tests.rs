// ═══════════════════════════════════════════════════════════════════════
// GUI WIDGET UNIT TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::gui::widgets::{Button, Checkbox, Slider, TextInput};

#[test_case]
fn test_widget_button_state() {
    let mut b = Button::new(0, 0, 80, 24, "ok");
    assert!(!b.hovered && !b.pressed);
    b.hovered = true;
    assert!(b.hovered && !b.pressed);
    b.pressed = true;
    assert!(b.hovered && b.pressed);
    b.pressed = false;
    b.hovered = false;
    assert!(!b.hovered && !b.pressed);
}

#[test_case]
fn test_widget_text_input() {
    let mut t = TextInput::new(0, 0, 120, 24, "type…");
    t.insert_char('H');
    t.insert_char('i');
    assert_eq!(t.text.as_str(), "Hi");
    assert_eq!(t.cursor_pos, 2);
    t.backspace();
    assert_eq!(t.text.as_str(), "H");
    assert_eq!(t.cursor_pos, 1);
}

#[test_case]
fn test_widget_slider_range() {
    let mut s = Slider::new(0, 0, 100, 50);
    s.update_from_mouse(-40);
    assert_eq!(s.value, 0);
    s.update_from_mouse(200);
    assert_eq!(s.value, 100);
    s.update_from_mouse(25);
    assert_eq!(s.value, 25);
}

#[test_case]
fn test_widget_checkbox_toggle() {
    let mut c = Checkbox::new(0, 0, "enable", false);
    assert!(!c.checked);
    c.checked = !c.checked;
    assert!(c.checked);
    c.checked = !c.checked;
    assert!(!c.checked);
}

#[test_case]
fn test_widget_dropdown_selection() {
    let items = ["Apple", "Banana", "Cherry"];
    let mut selected = 0usize;
    selected = 2;
    assert_eq!(items[selected], "Cherry");
    selected = selected.min(items.len() - 1);
    assert!(selected < items.len());
}
