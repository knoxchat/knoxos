/// Window Lifecycle Events — winit-inspired event system for KnoxOS windows
///
/// Provides a structured event queue that window content handlers can consume.
/// This decouples the raw input processing (mouse clicks on buttons) from the
/// window content's response (closing, resizing, etc.), making the system
/// robust across resolution changes.
///
/// Enhanced with foundational types from winit's `WindowEvent` enum:
/// - ElementState, MouseButton, MouseScrollDelta for typed input
/// - KeyboardInput with KeyEvent for structured keyboard events
/// - PointerEntered/PointerLeft for hover detection
/// - ModifiersChanged for per-key modifier tracking
/// - ThemeChanged, Occluded, RedrawRequested for lifecycle events
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

use super::event_types::{
    ElementState, KeyEvent, Modifiers, MouseButton, MouseScrollDelta, PointerSource,
};
use super::framebuffer::Rect;
use super::window::WindowId;

/// Events that can be delivered to a window.
///
/// Expanded from the original 13 variants to cover the full range of winit-style
/// window lifecycle events. Content handlers can pattern-match on these to respond
/// to keyboard input, hover state, theme changes, and more.
#[derive(Debug, Clone)]
pub enum WindowEvent {
    // ── Window lifecycle ──────────────────────────────────────────────
    /// The window's close button was clicked. Content handler should clean up.
    CloseRequested,
    /// The window was destroyed and is no longer valid. Clean up all resources.
    Destroyed,
    /// The window was resized (new content size in physical pixels).
    Resized { width: u32, height: u32 },
    /// The window was moved to a new position.
    Moved { x: i32, y: i32 },
    /// The window gained or lost focus.
    Focused(bool),
    /// The window was maximized.
    Maximized,
    /// The window was restored from maximized/minimized/snapped.
    Restored,
    /// The window was minimized.
    Minimized,
    /// The window was snapped to left half.
    SnappedLeft,
    /// The window was snapped to right half.
    SnappedRight,
    /// The display resolution / DPI changed. Windows should re-layout.
    ScaleFactorChanged {
        new_scale_percent: u32,
        new_screen_width: u32,
        new_screen_height: u32,
    },
    /// The window should repaint its content. Sent when the system determines
    /// the window's content needs to be redrawn (e.g., after expose, resize).
    /// Content handlers should redraw on receiving this rather than on a timer.
    RedrawRequested,
    /// The window is fully occluded (covered) or revealed.
    /// `true` = occluded (skip rendering), `false` = visible.
    Occluded(bool),
    /// The system theme changed (Light/Dark).
    ThemeChanged(Theme),

    // ── Mouse / Pointer events ───────────────────────────────────────
    /// The window's content area was scrolled.
    Scrolled { delta: MouseScrollDelta },
    /// Mouse button press/release within the content area (content-relative coordinates).
    PointerButton {
        x: i32,
        y: i32,
        button: MouseButton,
        state: ElementState,
        source: PointerSource,
    },
    /// Mouse/pointer moved within the content area (content-relative coordinates).
    PointerMoved {
        x: i32,
        y: i32,
        source: PointerSource,
    },
    /// The pointer entered the window's content area.
    PointerEntered { source: PointerSource },
    /// The pointer left the window's content area.
    PointerLeft { source: PointerSource },

    // ── Keyboard events ──────────────────────────────────────────────
    /// A keyboard key was pressed or released while this window was focused.
    KeyboardInput(KeyEvent),
    /// The set of active keyboard modifiers changed.
    ModifiersChanged(Modifiers),

    // ── Drag & Drop ──────────────────────────────────────────────────
    /// A file/item drag entered the window area.
    DragEntered { x: i32, y: i32 },
    /// A file/item drag moved within the window area.
    DragMoved { x: i32, y: i32 },
    /// A file/item was dropped onto the window.
    DragDropped { x: i32, y: i32 },
    /// A drag operation left the window area without dropping.
    DragLeft,
}

/// System theme — Light or Dark.
/// Adapted from `winit::window::Theme`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
pub enum Theme {
    Light,
    #[default]
    Dark,
}

// ── Backward-compatible constructors for existing call sites ─────────

impl WindowEvent {
    /// Create a ContentClick event (backward-compatible helper).
    /// Produces a PointerButton with Left button Pressed.
    #[inline]
    pub fn content_click(x: i32, y: i32) -> Self {
        WindowEvent::PointerButton {
            x,
            y,
            button: MouseButton::Left,
            state: ElementState::Pressed,
            source: PointerSource::Mouse,
        }
    }

    /// Create a ContentPointerMoved event (backward-compatible helper).
    #[inline]
    pub fn content_pointer_moved(x: i32, y: i32) -> Self {
        WindowEvent::PointerMoved {
            x,
            y,
            source: PointerSource::Mouse,
        }
    }

    /// Create a Scrolled event from a raw i32 delta_y (backward-compatible helper).
    #[inline]
    pub fn scrolled_legacy(delta_y: i32) -> Self {
        WindowEvent::Scrolled {
            delta: MouseScrollDelta::LineDelta(0.0, delta_y as f32),
        }
    }
}

/// Per-window event queue.
struct WindowEventQueue {
    events: Vec<WindowEvent>,
}

impl WindowEventQueue {
    fn new() -> Self {
        Self { events: Vec::new() }
    }
}

lazy_static::lazy_static! {
    /// Global event queues keyed by WindowId.
    static ref EVENT_QUEUES: Mutex<BTreeMap<u32, WindowEventQueue>> =
        Mutex::new(BTreeMap::new());
}

/// Push an event to a specific window's queue.
pub fn push_event(window_id: WindowId, event: WindowEvent) {
    let mut queues = EVENT_QUEUES.lock();
    queues
        .entry(window_id)
        .or_insert_with(WindowEventQueue::new)
        .events
        .push(event);
}

/// Push an event to ALL open windows (e.g., ScaleFactorChanged).
pub fn broadcast_event(event: WindowEvent) {
    let mut queues = EVENT_QUEUES.lock();
    for (_id, queue) in queues.iter_mut() {
        queue.events.push(event.clone());
    }
}

/// Take all pending events for a window, clearing the queue.
pub fn take_events(window_id: WindowId) -> Vec<WindowEvent> {
    let mut queues = EVENT_QUEUES.lock();
    if let Some(queue) = queues.get_mut(&window_id) {
        let taken = queue.events.clone();
        queue.events.clear();
        taken
    } else {
        Vec::new()
    }
}

/// Peek at pending events without consuming them.
pub fn peek_events(window_id: WindowId) -> Vec<WindowEvent> {
    let queues = EVENT_QUEUES.lock();
    if let Some(queue) = queues.get(&window_id) {
        queue.events.clone()
    } else {
        Vec::new()
    }
}

/// Check if a window has any pending events.
pub fn has_events(window_id: WindowId) -> bool {
    let queues = EVENT_QUEUES.lock();
    queues.get(&window_id).is_some_and(|q| !q.events.is_empty())
}

/// Create event queue for a new window.
pub fn register_window(window_id: WindowId) {
    let mut queues = EVENT_QUEUES.lock();
    queues.insert(window_id, WindowEventQueue::new());
}

/// Remove event queue when window is closed.
pub fn unregister_window(window_id: WindowId) {
    let mut queues = EVENT_QUEUES.lock();
    queues.remove(&window_id);
}
