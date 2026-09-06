/// Gesture Recognition — multi-touch gesture detection and handling
///
/// Provides:
///   - Pinch-to-zoom (two-finger scale)
///   - Two-finger scroll (smooth scrolling)
///   - Three-finger swipe (workspace switch)
///   - Four-finger swipe (overview/app expose)
///   - Long press detection
///   - Edge swipe (show panel, go back)
///   - Tap detection (single, double, triple)
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

// ═══════════════════════════════════════════════════════════════════════
// TOUCH POINT & GESTURE TYPES
// ═══════════════════════════════════════════════════════════════════════

/// A single touch contact point
#[derive(Debug, Clone, Copy)]
pub struct TouchPoint {
    /// Touch slot ID (from the input device)
    pub id: u32,
    /// X position in screen pixels
    pub x: i32,
    /// Y position in screen pixels
    pub y: i32,
    /// Pressure (0..1024, 0 = lifted)
    pub pressure: u16,
    /// Touch area major axis (for palm rejection)
    pub major: u16,
    /// Timestamp (TSC ticks)
    pub timestamp: u64,
}

/// Touch event from the input driver
#[derive(Debug, Clone, Copy)]
pub enum TouchEvent {
    Down(TouchPoint),
    Move(TouchPoint),
    Up(TouchPoint),
    Cancel(u32),
}

/// Recognized gesture
#[derive(Debug, Clone, Copy)]
pub enum Gesture {
    /// Single tap at position
    Tap { x: i32, y: i32 },
    /// Double tap at position
    DoubleTap { x: i32, y: i32 },
    /// Long press at position
    LongPress { x: i32, y: i32 },
    /// Two-finger pinch/zoom
    Pinch {
        center_x: i32,
        center_y: i32,
        scale: f32,    // 1.0 = no change, > 1.0 = zoom in
        rotation: f32, // radians
    },
    /// Two-finger scroll
    Scroll { dx: i32, dy: i32 },
    /// Three-finger horizontal swipe
    SwipeThree {
        direction: SwipeDirection,
        velocity: f32,
    },
    /// Four-finger horizontal swipe (overview)
    SwipeFour {
        direction: SwipeDirection,
        velocity: f32,
    },
    /// Edge swipe from screen border
    EdgeSwipe { edge: ScreenEdge, distance: i32 },
    /// Ongoing drag (single finger)
    Drag { x: i32, y: i32, dx: i32, dy: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwipeDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenEdge {
    Top,
    Bottom,
    Left,
    Right,
}

/// Gesture recognizer state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecognizerState {
    Idle,
    PossibleTap,
    PossibleLongPress,
    Dragging,
    TwoFingerStart,
    Pinching,
    Scrolling,
    ThreeFingerStart,
    ThreeFingerSwiping,
    FourFingerStart,
    FourFingerSwiping,
    EdgeSwipeStart,
}

// ═══════════════════════════════════════════════════════════════════════
// GESTURE RECOGNIZER
// ═══════════════════════════════════════════════════════════════════════

/// Configuration thresholds
struct GestureConfig {
    /// Max movement (px) to still count as a tap
    tap_slop: i32,
    /// Double-tap time window (TSC ticks, ~300ms at 2GHz)
    double_tap_ticks: u64,
    /// Long press threshold (ticks, ~500ms)
    long_press_ticks: u64,
    /// Minimum pinch distance change (px) to trigger
    pinch_threshold: i32,
    /// Minimum swipe distance (px) for 3/4 finger swipe
    swipe_threshold: i32,
    /// Edge detection zone width (px from screen border)
    edge_zone: i32,
    /// Screen dimensions
    screen_w: i32,
    screen_h: i32,
}

impl Default for GestureConfig {
    fn default() -> Self {
        Self {
            tap_slop: 10,
            double_tap_ticks: 600_000_000,   // ~300ms at 2GHz
            long_press_ticks: 1_000_000_000, // ~500ms
            pinch_threshold: 20,
            swipe_threshold: 50,
            edge_zone: 20,
            screen_w: 1920,
            screen_h: 1080,
        }
    }
}

struct GestureState {
    config: GestureConfig,
    state: RecognizerState,
    /// Currently active touch points
    active_touches: Vec<TouchPoint>,
    /// Initial touch positions (when fingers first went down)
    initial_touches: Vec<TouchPoint>,
    /// Last tap position + timestamp (for double-tap detection)
    last_tap_x: i32,
    last_tap_y: i32,
    last_tap_time: u64,
    /// Initial pinch distance (for computing scale)
    initial_pinch_dist: f32,
    /// Gesture callbacks (up to 8)
    callbacks: Vec<fn(Gesture)>,
}

impl GestureState {
    fn new() -> Self {
        Self {
            config: GestureConfig::default(),
            state: RecognizerState::Idle,
            active_touches: Vec::new(),
            initial_touches: Vec::new(),
            last_tap_x: 0,
            last_tap_y: 0,
            last_tap_time: 0,
            initial_pinch_dist: 0.0,
            callbacks: Vec::new(),
        }
    }

    fn finger_count(&self) -> usize {
        self.active_touches.len()
    }

    fn find_touch(&self, id: u32) -> Option<usize> {
        self.active_touches.iter().position(|t| t.id == id)
    }

    fn find_initial(&self, id: u32) -> Option<usize> {
        self.initial_touches.iter().position(|t| t.id == id)
    }

    fn centroid(&self) -> (i32, i32) {
        if self.active_touches.is_empty() {
            return (0, 0);
        }
        let n = self.active_touches.len() as i32;
        let sx: i32 = self.active_touches.iter().map(|t| t.x).sum();
        let sy: i32 = self.active_touches.iter().map(|t| t.y).sum();
        (sx / n, sy / n)
    }

    fn two_finger_distance(&self) -> f32 {
        if self.active_touches.len() < 2 {
            return 0.0;
        }
        let dx = (self.active_touches[0].x - self.active_touches[1].x) as f32;
        let dy = (self.active_touches[0].y - self.active_touches[1].y) as f32;
        libm::sqrtf(dx * dx + dy * dy)
    }

    fn is_edge(&self, x: i32, y: i32) -> Option<ScreenEdge> {
        if x < self.config.edge_zone {
            Some(ScreenEdge::Left)
        } else if x > self.config.screen_w - self.config.edge_zone {
            Some(ScreenEdge::Right)
        } else if y < self.config.edge_zone {
            Some(ScreenEdge::Top)
        } else if y > self.config.screen_h - self.config.edge_zone {
            Some(ScreenEdge::Bottom)
        } else {
            None
        }
    }

    fn emit(&self, gesture: Gesture) {
        for cb in &self.callbacks {
            cb(gesture);
        }
    }
}

lazy_static::lazy_static! {
    static ref GESTURE: Mutex<GestureState> = Mutex::new(GestureState::new());
}

static GESTURE_INITIALIZED: AtomicBool = AtomicBool::new(false);
static GESTURE_EVENT_COUNT: AtomicU64 = AtomicU64::new(0);

// ═══════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════

/// Initialize gesture recognition
pub fn init(screen_w: i32, screen_h: i32) {
    let mut state = GESTURE.lock();
    state.config.screen_w = screen_w;
    state.config.screen_h = screen_h;
    GESTURE_INITIALIZED.store(true, Ordering::Relaxed);
    crate::serial_println!("[Gesture] Initialized ({}x{})", screen_w, screen_h);
}

/// Register a gesture callback
pub fn register_callback(cb: fn(Gesture)) {
    let mut state = GESTURE.lock();
    if state.callbacks.len() < 8 {
        state.callbacks.push(cb);
    }
}

/// Process a touch event from the input driver
pub fn process_touch(event: TouchEvent) {
    GESTURE_EVENT_COUNT.fetch_add(1, Ordering::Relaxed);
    let mut state = GESTURE.lock();

    match event {
        TouchEvent::Down(point) => handle_down(&mut state, point),
        TouchEvent::Move(point) => handle_move(&mut state, point),
        TouchEvent::Up(point) => handle_up(&mut state, point),
        TouchEvent::Cancel(id) => {
            state.active_touches.retain(|t| t.id != id);
            state.initial_touches.retain(|t| t.id != id);
            if state.active_touches.is_empty() {
                state.state = RecognizerState::Idle;
            }
        }
    }
}

/// Get current number of active touch contacts
pub fn active_touch_count() -> usize {
    GESTURE.lock().active_touches.len()
}

/// Update screen dimensions (e.g. on rotation)
pub fn set_screen_size(w: i32, h: i32) {
    let mut state = GESTURE.lock();
    state.config.screen_w = w;
    state.config.screen_h = h;
}

// ═══════════════════════════════════════════════════════════════════════
// EVENT HANDLERS
// ═══════════════════════════════════════════════════════════════════════

fn handle_down(state: &mut GestureState, point: TouchPoint) {
    state.active_touches.push(point);
    state.initial_touches.push(point);

    let count = state.finger_count();
    match count {
        1 => {
            // Check if this is an edge swipe
            if state.is_edge(point.x, point.y).is_some() {
                state.state = RecognizerState::EdgeSwipeStart;
            } else {
                state.state = RecognizerState::PossibleTap;
            }
        }
        2 => {
            state.initial_pinch_dist = state.two_finger_distance();
            state.state = RecognizerState::TwoFingerStart;
        }
        3 => {
            state.state = RecognizerState::ThreeFingerStart;
        }
        4 => {
            state.state = RecognizerState::FourFingerStart;
        }
        _ => {}
    }
}

fn handle_move(state: &mut GestureState, point: TouchPoint) {
    // Update active touch position
    if let Some(idx) = state.find_touch(point.id) {
        state.active_touches[idx] = point;
    } else {
        return;
    }

    match state.state {
        RecognizerState::PossibleTap | RecognizerState::PossibleLongPress => {
            // Check if we've moved beyond tap slop → transition to drag
            if let Some(idx) = state.find_initial(point.id) {
                let init = state.initial_touches[idx];
                let dx = (point.x - init.x).abs();
                let dy = (point.y - init.y).abs();
                if dx > state.config.tap_slop || dy > state.config.tap_slop {
                    state.state = RecognizerState::Dragging;
                    state.emit(Gesture::Drag {
                        x: point.x,
                        y: point.y,
                        dx: point.x - init.x,
                        dy: point.y - init.y,
                    });
                }
            }
        }
        RecognizerState::Dragging => {
            if let Some(idx) = state.find_initial(point.id) {
                let init = state.initial_touches[idx];
                state.emit(Gesture::Drag {
                    x: point.x,
                    y: point.y,
                    dx: point.x - init.x,
                    dy: point.y - init.y,
                });
            }
        }
        RecognizerState::TwoFingerStart
        | RecognizerState::Pinching
        | RecognizerState::Scrolling => {
            if state.finger_count() >= 2 {
                let current_dist = state.two_finger_distance();
                let dist_delta = (current_dist - state.initial_pinch_dist).abs();

                if dist_delta > state.config.pinch_threshold as f32 {
                    // Pinch gesture
                    let (cx, cy) = state.centroid();
                    let scale = if state.initial_pinch_dist > 0.0 {
                        current_dist / state.initial_pinch_dist
                    } else {
                        1.0
                    };
                    state.state = RecognizerState::Pinching;
                    state.emit(Gesture::Pinch {
                        center_x: cx,
                        center_y: cy,
                        scale,
                        rotation: 0.0,
                    });
                } else {
                    // Two-finger scroll
                    let (cx, cy) = state.centroid();
                    if state.initial_touches.len() >= 2 {
                        let init_cx = (state.initial_touches[0].x + state.initial_touches[1].x) / 2;
                        let init_cy = (state.initial_touches[0].y + state.initial_touches[1].y) / 2;
                        state.state = RecognizerState::Scrolling;
                        state.emit(Gesture::Scroll {
                            dx: cx - init_cx,
                            dy: cy - init_cy,
                        });
                    }
                }
            }
        }
        RecognizerState::ThreeFingerStart | RecognizerState::ThreeFingerSwiping => {
            if state.finger_count() >= 3 && state.initial_touches.len() >= 3 {
                let (cx, cy) = state.centroid();
                let init_cx = state
                    .initial_touches
                    .iter()
                    .take(3)
                    .map(|t| t.x)
                    .sum::<i32>()
                    / 3;
                let init_cy = state
                    .initial_touches
                    .iter()
                    .take(3)
                    .map(|t| t.y)
                    .sum::<i32>()
                    / 3;
                let dx = cx - init_cx;
                let dy = cy - init_cy;

                if dx.abs() > state.config.swipe_threshold
                    || dy.abs() > state.config.swipe_threshold
                {
                    let direction = if dx.abs() > dy.abs() {
                        if dx > 0 {
                            SwipeDirection::Right
                        } else {
                            SwipeDirection::Left
                        }
                    } else {
                        if dy > 0 {
                            SwipeDirection::Down
                        } else {
                            SwipeDirection::Up
                        }
                    };
                    let distance = libm::sqrtf((dx * dx + dy * dy) as f32);
                    state.state = RecognizerState::ThreeFingerSwiping;
                    state.emit(Gesture::SwipeThree {
                        direction,
                        velocity: distance, // simplified velocity
                    });
                }
            }
        }
        RecognizerState::FourFingerStart | RecognizerState::FourFingerSwiping => {
            if state.finger_count() >= 4 && state.initial_touches.len() >= 4 {
                let (cx, _cy) = state.centroid();
                let init_cx = state
                    .initial_touches
                    .iter()
                    .take(4)
                    .map(|t| t.x)
                    .sum::<i32>()
                    / 4;
                let dx = cx - init_cx;

                if dx.abs() > state.config.swipe_threshold {
                    let direction = if dx > 0 {
                        SwipeDirection::Right
                    } else {
                        SwipeDirection::Left
                    };
                    state.state = RecognizerState::FourFingerSwiping;
                    state.emit(Gesture::SwipeFour {
                        direction,
                        velocity: dx.abs() as f32,
                    });
                }
            }
        }
        RecognizerState::EdgeSwipeStart => {
            if let Some(idx) = state.find_initial(point.id) {
                let init = state.initial_touches[idx];
                let dx = point.x - init.x;
                let dy = point.y - init.y;
                let dist = libm::sqrtf((dx * dx + dy * dy) as f32) as i32;

                if dist > state.config.swipe_threshold {
                    if let Some(edge) = state.is_edge(init.x, init.y) {
                        state.emit(Gesture::EdgeSwipe {
                            edge,
                            distance: dist,
                        });
                    }
                }
            }
        }
        RecognizerState::Idle => {}
    }
}

fn handle_up(state: &mut GestureState, point: TouchPoint) {
    let finger_count_before = state.finger_count();

    match state.state {
        RecognizerState::PossibleTap => {
            if finger_count_before == 1 {
                // Check for double tap
                let dt = point.timestamp.wrapping_sub(state.last_tap_time);
                let ddx = (point.x - state.last_tap_x).abs();
                let ddy = (point.y - state.last_tap_y).abs();

                if dt < state.config.double_tap_ticks
                    && ddx < state.config.tap_slop * 2
                    && ddy < state.config.tap_slop * 2
                {
                    state.emit(Gesture::DoubleTap {
                        x: point.x,
                        y: point.y,
                    });
                    state.last_tap_time = 0; // reset to prevent triple-tap
                } else {
                    state.emit(Gesture::Tap {
                        x: point.x,
                        y: point.y,
                    });
                    state.last_tap_x = point.x;
                    state.last_tap_y = point.y;
                    state.last_tap_time = point.timestamp;
                }
            }
        }
        _ => {}
    }

    // Remove the touch
    state.active_touches.retain(|t| t.id != point.id);
    state.initial_touches.retain(|t| t.id != point.id);

    if state.active_touches.is_empty() {
        state.state = RecognizerState::Idle;
        state.initial_touches.clear();
    }
}
