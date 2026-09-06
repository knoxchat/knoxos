use super::framebuffer::Rect;
/// Drag-and-Drop state machine for KnoxOS GUI
///
/// Inspired by egui's `DragAndDrop` plugin. Tracks the current drag payload,
/// provides lifecycle management (set → hover → release/cancel), and
/// automatically cleans up on Escape or mouse release.
///
/// This is a low-level API. Higher-level helpers can be built on top:
/// - Widget-level: `Response::dnd_set_drag_payload()` / `dnd_hover_payload()` / `dnd_release_payload()`
/// - Container-level: `Ui::dnd_drag_source()` / `dnd_drop_zone()`
///
/// # no_std compatible
/// Uses spin::Mutex instead of std::sync, and avoids `Any`/`Arc` by using
/// a concrete enum payload type.
use spin::Mutex;

// ─── Drag Payload Types ──────────────────────────────────────────────

/// What is being dragged. Concrete enum instead of `dyn Any` (no_std).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragPayload {
    /// A desktop icon is being dragged (index into Desktop::icons)
    DesktopIcon { index: usize },
    /// A window is being dragged (window ID) — tracks the visual "ghost"
    Window { id: u32 },
    /// A file/folder in the file explorer (window_id, entry index)
    FileEntry { window_id: u32, index: usize },
    /// A taskbar entry is being reordered (entry index)
    TaskbarEntry { index: usize },
}

/// Cursor icon hint for the current drag state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragCursor {
    /// Default grab cursor while dragging
    Grabbing,
    /// Drag target accepts the payload (e.g. folder accepts file drop)
    CanDrop,
    /// Drag target does NOT accept the payload
    NoDrop,
}

// ─── Drag State ──────────────────────────────────────────────────────

/// Internal drag-and-drop state
struct DragState {
    /// The current payload, or None if not dragging
    payload: Option<DragPayload>,
    /// Where the drag started (screen coords)
    start_x: i32,
    start_y: i32,
    /// Current drag position (updated every mouse move)
    current_x: i32,
    current_y: i32,
    /// The rect of the item being dragged (for visual ghost outline)
    source_rect: Option<Rect>,
    /// Cursor hint
    cursor: DragCursor,
    /// Whether the drag was cancelled (Escape key)
    cancelled: bool,
    /// Whether the payload was released this frame
    released_this_frame: bool,
}

lazy_static::lazy_static! {
    static ref DRAG: Mutex<DragState> = Mutex::new(DragState {
        payload: None,
        start_x: 0,
        start_y: 0,
        current_x: 0,
        current_y: 0,
        source_rect: None,
        cursor: DragCursor::Grabbing,
        cancelled: false,
        released_this_frame: false,
    });
}

// ─── Public API ──────────────────────────────────────────────────────

/// Begin a drag operation with the given payload.
/// `start_x`, `start_y`: the mouse position when the drag started.
/// `source_rect`: optional rect of the dragged item (for ghost outline).
pub fn set_payload(payload: DragPayload, start_x: i32, start_y: i32, source_rect: Option<Rect>) {
    let mut state = DRAG.lock();
    state.payload = Some(payload);
    state.start_x = start_x;
    state.start_y = start_y;
    state.current_x = start_x;
    state.current_y = start_y;
    state.source_rect = source_rect;
    state.cursor = DragCursor::Grabbing;
    state.cancelled = false;
    state.released_this_frame = false;
}

/// Clear the drag payload (cancel or complete).
pub fn clear_payload() {
    let mut state = DRAG.lock();
    state.payload = None;
    state.source_rect = None;
    state.cancelled = false;
    state.released_this_frame = false;
}

/// Update the current drag position (called on mouse move during drag).
pub fn update_position(x: i32, y: i32) {
    let mut state = DRAG.lock();
    if state.payload.is_some() {
        state.current_x = x;
        state.current_y = y;
    }
}

/// Set the cursor hint (called by drop zones to indicate acceptance).
pub fn set_cursor(cursor: DragCursor) {
    DRAG.lock().cursor = cursor;
}

/// Check if a drag operation is in progress.
pub fn has_payload() -> bool {
    DRAG.lock().payload.is_some()
}

/// Get the current payload (if any). Does not consume it.
pub fn payload() -> Option<DragPayload> {
    DRAG.lock().payload
}

/// Get the current drag position.
pub fn drag_position() -> (i32, i32) {
    let state = DRAG.lock();
    (state.current_x, state.current_y)
}

/// Get the delta from start position to current position.
pub fn drag_delta() -> (i32, i32) {
    let state = DRAG.lock();
    (
        state.current_x - state.start_x,
        state.current_y - state.start_y,
    )
}

/// Get the source rect of the dragged item (for ghost outline).
pub fn source_rect() -> Option<Rect> {
    DRAG.lock().source_rect
}

/// Get the ghost rect (source rect translated by drag delta).
pub fn ghost_rect() -> Option<Rect> {
    let state = DRAG.lock();
    if let (Some(payload), Some(src)) = (state.payload, state.source_rect) {
        let dx = state.current_x - state.start_x;
        let dy = state.current_y - state.start_y;
        Some(Rect::new(src.x + dx, src.y + dy, src.width, src.height))
    } else {
        None
    }
}

/// Check if the drag payload is of a specific type.
pub fn has_payload_of_type(check: fn(&DragPayload) -> bool) -> bool {
    DRAG.lock().payload.as_ref().is_some_and(check)
}

/// Check if a point is over a drop zone rect, AND we're carrying a payload.
/// Returns the payload if the point is inside the rect.
pub fn hover_payload(zone: &Rect, mouse_x: i32, mouse_y: i32) -> Option<DragPayload> {
    if zone.contains(mouse_x, mouse_y) {
        DRAG.lock().payload
    } else {
        None
    }
}

/// Check if the payload was released (mouse up) over a drop zone.
/// Returns and consumes the payload if mouse is inside the zone.
pub fn release_payload(zone: &Rect, mouse_x: i32, mouse_y: i32) -> Option<DragPayload> {
    if zone.contains(mouse_x, mouse_y) {
        let mut state = DRAG.lock();
        if state.released_this_frame {
            let payload = state.payload.take();
            state.source_rect = None;
            state.released_this_frame = false;
            payload
        } else {
            None
        }
    } else {
        None
    }
}

/// Cancel the drag (e.g. Escape key pressed).
pub fn cancel() {
    let mut state = DRAG.lock();
    state.payload = None;
    state.source_rect = None;
    state.cancelled = true;
}

/// Was the drag cancelled this frame?
pub fn was_cancelled() -> bool {
    DRAG.lock().cancelled
}

// ─── Frame lifecycle ─────────────────────────────────────────────────

/// Called at the beginning of each frame. Checks for Escape to cancel drag.
pub fn on_begin_frame() {
    // Escape key cancellation is handled by keyboard input dispatch.
    // This just resets per-frame flags.
    let mut state = DRAG.lock();
    state.released_this_frame = false;
    state.cancelled = false;
}

/// Called when the mouse button is released. Marks the payload as "released this frame"
/// so that drop zones can pick it up. If no drop zone consumes it, the payload
/// is automatically cleared at the end of the frame.
pub fn on_mouse_release() {
    let mut state = DRAG.lock();
    if state.payload.is_some() {
        state.released_this_frame = true;
    }
}

/// Called at the end of each frame. Cleans up if the payload wasn't consumed.
pub fn on_end_frame() {
    let mut state = DRAG.lock();
    if state.released_this_frame {
        // Nobody consumed the release — clear everything
        state.payload = None;
        state.source_rect = None;
        state.released_this_frame = false;
    }
}
