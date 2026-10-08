/// Per-frame pointer snapshot for immediate-mode widgets (KnoxUI / `Ui`).
///
/// Edge flags (`pressed` / `released`) latch until `end_frame()` after a
/// successful composite, so a click that arrives just before a paced redraw
/// is not dropped.
use crate::gui::ui::InputState;
use spin::Mutex;

struct PointerLatch {
    x: i32,
    y: i32,
    left: bool,
    right: bool,
    pressed: bool,
    released: bool,
    secondary_pressed: bool,
    secondary_released: bool,
    scroll: i32,
    double_click: bool,
}

impl PointerLatch {
    const fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            left: false,
            right: false,
            pressed: false,
            released: false,
            secondary_pressed: false,
            secondary_released: false,
            scroll: 0,
            double_click: false,
        }
    }
}

static LATCH: Mutex<PointerLatch> = Mutex::new(PointerLatch::new());

/// Sync pointer position and button edges from the live mouse device.
pub fn update_pointer(x: i32, y: i32, left: bool, right: bool) {
    let mut latch = LATCH.lock();
    if left && !latch.left {
        latch.pressed = true;
    }
    if !left && latch.left {
        latch.released = true;
    }
    if right && !latch.right {
        latch.secondary_pressed = true;
    }
    if !right && latch.right {
        latch.secondary_released = true;
    }
    latch.x = x;
    latch.y = y;
    latch.left = left;
    latch.right = right;
}

/// Accumulate a scroll-wheel tick for the next widget frame.
pub fn note_scroll(delta: i32) {
    let mut latch = LATCH.lock();
    latch.scroll = latch.scroll.saturating_add(delta);
}

/// Mark that this pointer-down is a double-click.
pub fn note_double_click() {
    LATCH.lock().double_click = true;
}

/// Snapshot input for the compositor / KnoxUI widgets.
pub fn snapshot() -> InputState {
    let mouse = super::mouse::MOUSE.lock();
    let latch = LATCH.lock();
    InputState {
        pointer_x: mouse.x,
        pointer_y: mouse.y,
        pointer_primary_down: mouse.left_button,
        pointer_secondary_down: mouse.right_button,
        pointer_primary_pressed: latch.pressed,
        pointer_primary_released: latch.released,
        pointer_secondary_pressed: latch.secondary_pressed,
        pointer_secondary_released: latch.secondary_released,
        scroll_delta: latch.scroll,
        char_typed: None,
        backspace_pressed: false,
        enter_pressed: false,
        tab_pressed: false,
        escape_pressed: false,
        left_pressed: false,
        right_pressed: false,
        up_pressed: false,
        down_pressed: false,
        home_pressed: false,
        end_pressed: false,
        delete_pressed: false,
        frame_tick: crate::gui::FRAME_COUNTER.load(core::sync::atomic::Ordering::Relaxed),
        double_click: latch.double_click,
    }
}

/// Clear one-frame edge flags after the desktop has composited.
pub fn end_frame() {
    let mut latch = LATCH.lock();
    latch.pressed = false;
    latch.released = false;
    latch.secondary_pressed = false;
    latch.secondary_released = false;
    latch.scroll = 0;
    latch.double_click = false;
}
