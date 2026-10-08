// ═══════════════════════════════════════════════════════════════════════
// GUI WIDGET UNIT TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::gui::widgets::{Button, Checkbox, Slider, TextInput, ToggleSwitch};

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

#[test_case]
fn test_button_pointer_click() {
    let mut b = Button::new(10, 10, 80, 24, "ok");
    assert!(!b.update_pointer(12, 12, true));
    assert!(b.hovered && b.pressed);
    assert!(b.update_pointer(12, 12, false));
    assert!(b.hovered && !b.pressed);
    assert!(!b.update_pointer(0, 0, false));
    assert!(!b.hovered);
}

#[test_case]
fn test_checkbox_toggle_on_press() {
    let mut c = Checkbox::new(0, 0, "enable", false);
    assert!(!c.update_pointer(100, 100, true));
    assert!(!c.checked);
    assert!(c.update_pointer(2, 2, true));
    assert!(c.checked);
    assert!(c.hovered);
}

#[test_case]
fn test_toggle_switch_click() {
    let mut t = ToggleSwitch::new(0, 0, "wifi", false);
    assert!(t.update_pointer(4, 4, true));
    assert!(t.on);
    assert!(t.update_pointer(4, 4, true));
    assert!(!t.on);
}

#[test_case]
fn test_text_input_focus_hit() {
    let mut t = TextInput::new(20, 20, 120, 24, "type…");
    t.update_pointer(24, 24, true);
    assert!(t.focused);
    t.update_pointer(0, 0, true);
    assert!(!t.focused);
    assert!(t.contains(50, 30));
}

#[test_case]
fn test_fill_rounded_rect_aa_center() {
    let mut fb = FrameBuffer::new(64, 64);
    fb.fill_rounded_rect_aa(Rect::new(8, 8, 40, 24), Pixel::rgb(80, 90, 100), 6);
    let p = fb.get_pixel(28, 20);
    assert_eq!(p.r, 80);
    assert_eq!(p.g, 90);
    assert_eq!(p.b, 100);
}

#[test_case]
fn test_damage_rect_union() {
    let a = Rect::new(0, 0, 10, 10);
    let b = Rect::new(5, 5, 10, 10);
    let u = crate::gui::rect_union(a, b);
    assert_eq!(u.x, 0);
    assert_eq!(u.y, 0);
    assert_eq!(u.width, 15);
    assert_eq!(u.height, 15);
}
