/// Accessibility — Screen reader, high contrast, keyboard navigation, zoom
///
/// Provides accessibility features for KnoxOS:
///   - Screen reader (text-to-speech for UI elements)
///   - High contrast mode (increased visibility)
///   - Keyboard focus navigation (Tab/Shift+Tab)
///   - Focus ring rendering
///   - Screen magnification/zoom
///   - Reduced motion mode
///   - Large cursor option
///   - Sticky keys
///   - Slow keys (debounce)
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use spin::Mutex;

use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// ACCESSIBILITY STATE
// ═══════════════════════════════════════════════════════════════════════

static HIGH_CONTRAST: AtomicBool = AtomicBool::new(false);
static SCREEN_READER: AtomicBool = AtomicBool::new(false);
static REDUCED_MOTION: AtomicBool = AtomicBool::new(false);
static LARGE_CURSOR: AtomicBool = AtomicBool::new(false);
static STICKY_KEYS: AtomicBool = AtomicBool::new(false);
static ZOOM_LEVEL: AtomicU8 = AtomicU8::new(100); // 100 = 100% = no zoom
static SLOW_KEYS_DELAY_MS: AtomicU8 = AtomicU8::new(0); // 0 = disabled

// ═══════════════════════════════════════════════════════════════════════
// KEYBOARD FOCUS NAVIGATION
// ═══════════════════════════════════════════════════════════════════════

/// A focusable UI element
#[derive(Clone)]
pub struct FocusableElement {
    pub id: u32,
    pub label: String,
    pub rect: Rect,
    pub tab_index: i32,
    pub focusable: bool,
    pub element_type: ElementType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementType {
    Button,
    TextInput,
    Checkbox,
    RadioButton,
    Slider,
    MenuItem,
    Tab,
    Link,
    ListItem,
    Window,
}

/// Focus manager state
pub struct FocusManager {
    /// All registered focusable elements
    elements: Vec<FocusableElement>,
    /// Currently focused element index
    focused_index: Option<usize>,
    /// Whether keyboard navigation is active
    keyboard_nav_active: bool,
}

impl FocusManager {
    pub fn new() -> Self {
        Self {
            elements: Vec::new(),
            focused_index: None,
            keyboard_nav_active: false,
        }
    }

    /// Register a focusable element
    pub fn register(&mut self, element: FocusableElement) {
        self.elements.push(element);
        // Sort by tab_index
        self.elements.sort_by_key(|e| e.tab_index);
    }

    /// Clear all registered elements (called on redraw)
    pub fn clear(&mut self) {
        self.elements.clear();
        self.focused_index = None;
    }

    /// Move focus to the next element (Tab key)
    pub fn focus_next(&mut self) {
        self.keyboard_nav_active = true;
        if self.elements.is_empty() {
            return;
        }
        let next = match self.focused_index {
            Some(idx) => {
                let mut n = idx + 1;
                while n < self.elements.len() && !self.elements[n].focusable {
                    n += 1;
                }
                if n >= self.elements.len() { 0 } else { n }
            }
            None => 0,
        };
        self.focused_index = Some(next);
        // Screen reader: announce the newly focused element
        if let Some(elem) = self.elements.get(next) {
            announce_element(elem);
        }
    }

    /// Move focus to the previous element (Shift+Tab)
    pub fn focus_prev(&mut self) {
        self.keyboard_nav_active = true;
        if self.elements.is_empty() {
            return;
        }
        let prev = match self.focused_index {
            Some(idx) => {
                if idx == 0 {
                    self.elements.len() - 1
                } else {
                    let mut n = idx - 1;
                    while n > 0 && !self.elements[n].focusable {
                        n -= 1;
                    }
                    n
                }
            }
            None => self.elements.len() - 1,
        };
        self.focused_index = Some(prev);
        // Screen reader: announce the newly focused element
        if let Some(elem) = self.elements.get(prev) {
            announce_element(elem);
        }
    }

    /// Get the currently focused element
    pub fn focused(&self) -> Option<&FocusableElement> {
        self.focused_index.and_then(|idx| self.elements.get(idx))
    }

    /// Activate the focused element (Enter/Space)
    pub fn activate_focused(&self) -> Option<u32> {
        self.focused().map(|e| e.id)
    }

    /// Is keyboard navigation active?
    pub fn is_keyboard_nav_active(&self) -> bool {
        self.keyboard_nav_active
    }

    /// Set focus by element ID
    pub fn set_focus(&mut self, id: u32) {
        self.focused_index = self.elements.iter().position(|e| e.id == id);
        self.keyboard_nav_active = true;
    }
}

lazy_static::lazy_static! {
    pub static ref FOCUS_MANAGER: Mutex<FocusManager> = Mutex::new(FocusManager::new());
}

// ═══════════════════════════════════════════════════════════════════════
// FOCUS RING RENDERING
// ═══════════════════════════════════════════════════════════════════════

/// Focus ring color (bright cyan with animation)
const FOCUS_RING_COLOR: Pixel = Pixel::new(0, 200, 255, 200);
const FOCUS_RING_WIDTH: i32 = 2;

/// Draw a focus ring around the given rect
pub fn draw_focus_ring(fb: &mut FrameBuffer, rect: Rect) {
    let x = rect.x - FOCUS_RING_WIDTH - 1;
    let y = rect.y - FOCUS_RING_WIDTH - 1;
    let w = rect.width + (FOCUS_RING_WIDTH * 2 + 2) as u32;
    let h = rect.height + (FOCUS_RING_WIDTH * 2 + 2) as u32;

    // Draw rounded focus ring
    for i in 0..FOCUS_RING_WIDTH {
        let offset = i;
        // Top
        fb.fill_rect(
            Rect::new(x + offset + 3, y + offset, w - 6, 1),
            FOCUS_RING_COLOR,
        );
        // Bottom
        fb.fill_rect(
            Rect::new(x + offset + 3, y + h as i32 - 1 - offset, w - 6, 1),
            FOCUS_RING_COLOR,
        );
        // Left
        fb.fill_rect(
            Rect::new(x + offset, y + offset + 3, 1, h - 6),
            FOCUS_RING_COLOR,
        );
        // Right
        fb.fill_rect(
            Rect::new(x + w as i32 - 1 - offset, y + offset + 3, 1, h - 6),
            FOCUS_RING_COLOR,
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// HIGH CONTRAST MODE
// ═══════════════════════════════════════════════════════════════════════

/// Toggle high contrast mode
pub fn toggle_high_contrast() {
    let prev = HIGH_CONTRAST.fetch_xor(true, Ordering::Relaxed);
    serial_println!("[Accessibility] High contrast: {}", !prev);
}

/// Check if high contrast mode is enabled
pub fn is_high_contrast() -> bool {
    HIGH_CONTRAST.load(Ordering::Relaxed)
}

/// Get high-contrast version of a color
pub fn high_contrast_color(color: Pixel) -> Pixel {
    if !is_high_contrast() {
        return color;
    }
    // Convert to grayscale and threshold
    let gray = ((color.r as u16 * 77 + color.g as u16 * 150 + color.b as u16 * 29) >> 8) as u8;
    if gray > 128 {
        Pixel::rgb(255, 255, 255) // White
    } else {
        Pixel::rgb(0, 0, 0) // Black
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SCREEN MAGNIFICATION
// ═══════════════════════════════════════════════════════════════════════

/// Set zoom level (100 = 100%, 200 = 200%, etc.)
pub fn set_zoom(level: u8) {
    ZOOM_LEVEL.store(level.max(100), Ordering::Relaxed);
    serial_println!("[Accessibility] Zoom: {}%", level);
}

/// Get current zoom level
pub fn zoom_level() -> u8 {
    ZOOM_LEVEL.load(Ordering::Relaxed)
}

/// Check if zoom is active (> 100%)
pub fn is_zoomed() -> bool {
    zoom_level() > 100
}

/// Increase zoom by 25%
pub fn zoom_in() {
    let current = zoom_level();
    let new_level = (current as u16 + 25).min(250) as u8;
    set_zoom(new_level);
}

/// Decrease zoom by 25%
pub fn zoom_out() {
    let current = zoom_level();
    let new_level = if current <= 125 { 100 } else { current - 25 };
    set_zoom(new_level);
}

/// Reset zoom to 100%
pub fn zoom_reset() {
    set_zoom(100);
    serial_println!("[Accessibility] Zoom reset to 100%");
}

// ═══════════════════════════════════════════════════════════════════════
// SCREEN READER (simplified)
// ═══════════════════════════════════════════════════════════════════════

/// Screen reader announcement buffer
lazy_static::lazy_static! {
    static ref SR_BUFFER: Mutex<Vec<String>> = Mutex::new(Vec::new());
}

/// Toggle screen reader
pub fn toggle_screen_reader() {
    let prev = SCREEN_READER.fetch_xor(true, Ordering::Relaxed);
    serial_println!("[Accessibility] Screen reader: {}", !prev);
}

/// Check if screen reader is active
pub fn is_screen_reader_active() -> bool {
    SCREEN_READER.load(Ordering::Relaxed)
}

/// Announce text to the screen reader
pub fn announce(text: &str) {
    if is_screen_reader_active() {
        SR_BUFFER.lock().push(String::from(text));
        serial_println!("[SR] {}", text);
    }
}

/// Announce a focused UI element to the screen reader (via serial output)
pub fn announce_element(elem: &FocusableElement) {
    if !is_screen_reader_active() {
        return;
    }
    let type_name = match elem.element_type {
        ElementType::Button => "Button",
        ElementType::TextInput => "Text input",
        ElementType::Checkbox => "Checkbox",
        ElementType::RadioButton => "Radio button",
        ElementType::Slider => "Slider",
        ElementType::MenuItem => "Menu item",
        ElementType::Tab => "Tab",
        ElementType::Link => "Link",
        ElementType::ListItem => "List item",
        ElementType::Window => "Window",
    };
    let announcement = alloc::format!("{}: {}", type_name, elem.label);
    announce(&announcement);
}

/// Announce a window focus change
pub fn announce_window_focus(title: &str) {
    if is_screen_reader_active() {
        let msg = alloc::format!("Window focused: {}", title);
        announce(&msg);
    }
}

/// Announce a notification
pub fn announce_notification(title: &str, body: &str) {
    if is_screen_reader_active() {
        let msg = alloc::format!("Notification: {} - {}", title, body);
        announce(&msg);
    }
}

/// Announce a desktop icon focus
pub fn announce_desktop_icon(label: &str) {
    if is_screen_reader_active() {
        let msg = alloc::format!("Desktop icon: {}", label);
        announce(&msg);
    }
}

/// Get pending screen reader announcements
pub fn drain_announcements() -> Vec<String> {
    let mut buf = SR_BUFFER.lock();
    let announcements = buf.clone();
    buf.clear();
    announcements
}

// ═══════════════════════════════════════════════════════════════════════
// STICKY KEYS
// ═══════════════════════════════════════════════════════════════════════

/// Toggle sticky keys
pub fn toggle_sticky_keys() {
    let prev = STICKY_KEYS.fetch_xor(true, Ordering::Relaxed);
    serial_println!("[Accessibility] Sticky keys: {}", !prev);
}

/// Check if sticky keys is enabled
pub fn is_sticky_keys() -> bool {
    STICKY_KEYS.load(Ordering::Relaxed)
}

// ═══════════════════════════════════════════════════════════════════════
// OTHER SETTINGS
// ═══════════════════════════════════════════════════════════════════════

/// Toggle reduced motion
pub fn toggle_reduced_motion() {
    let prev = REDUCED_MOTION.fetch_xor(true, Ordering::Relaxed);
    serial_println!("[Accessibility] Reduced motion: {}", !prev);
}

/// Check if reduced motion is enabled
pub fn is_reduced_motion() -> bool {
    REDUCED_MOTION.load(Ordering::Relaxed)
}

/// Toggle large cursor
pub fn toggle_large_cursor() {
    let prev = LARGE_CURSOR.fetch_xor(true, Ordering::Relaxed);
    serial_println!("[Accessibility] Large cursor: {}", !prev);
}

/// Is large cursor enabled?
pub fn is_large_cursor() -> bool {
    LARGE_CURSOR.load(Ordering::Relaxed)
}

/// Set slow keys delay (0 = disabled)
pub fn set_slow_keys_delay(ms: u8) {
    SLOW_KEYS_DELAY_MS.store(ms, Ordering::Relaxed);
}

/// Get slow keys delay
pub fn slow_keys_delay() -> u8 {
    SLOW_KEYS_DELAY_MS.load(Ordering::Relaxed)
}

/// Initialize accessibility subsystem
pub fn init() {
    serial_println!("[KnoxOS] Accessibility subsystem initialized");
}

// ═══════════════════════════════════════════════════════════════════════
// SCREEN READER — TTS output via audio subsystem
// ═══════════════════════════════════════════════════════════════════════

/// TTS engine state
pub struct TtsEngine {
    /// Speech rate (words per minute)
    pub rate: u16,
    /// Volume (0-100)
    pub volume: u8,
    /// Pitch multiplier (50-200, 100 = normal)
    pub pitch: u8,
    /// Whether speech is currently being output
    pub speaking: bool,
    /// Queue of text to speak
    pub queue: Vec<alloc::string::String>,
}

lazy_static::lazy_static! {
    static ref TTS: Mutex<TtsEngine> = Mutex::new(TtsEngine {
        rate: 150,
        volume: 80,
        pitch: 100,
        speaking: false,
        queue: Vec::new(),
    });
}

/// Speak text through the TTS engine
pub fn speak(text: &str) {
    if !is_screen_reader_active() {
        return;
    }

    let mut tts = TTS.lock();
    tts.queue.push(alloc::string::String::from(text));

    // Output to serial for now, plus send to audio subsystem
    crate::serial_println!("[TTS] Speaking: {}", text);

    // Generate simple sine-wave speech indication via PC speaker
    // Real TTS would use formant synthesis or concatenative synthesis
    if !tts.speaking {
        tts.speaking = true;
        // Brief tone to indicate speech start
        super::sounds::notification();
    }
}

/// Stop TTS output
pub fn stop_speaking() {
    let mut tts = TTS.lock();
    tts.queue.clear();
    tts.speaking = false;
}

/// Set TTS speech rate
pub fn set_tts_rate(wpm: u16) {
    TTS.lock().rate = wpm.clamp(50, 500);
}

/// Set TTS volume
pub fn set_tts_volume(vol: u8) {
    TTS.lock().volume = vol.min(100);
}

// ═══════════════════════════════════════════════════════════════════════
// SCREEN MAGNIFIER — Smooth follow-cursor lens
// ═══════════════════════════════════════════════════════════════════════

/// Magnifier configuration
#[derive(Clone, Copy, Debug)]
pub struct MagnifierConfig {
    /// Magnification factor (2x, 4x, 8x)
    pub factor: u8,
    /// Lens radius in pixels
    pub radius: u16,
    /// Whether to follow cursor smoothly
    pub follow_cursor: bool,
    /// Whether to use full-screen magnification instead of lens
    pub fullscreen_mode: bool,
    /// Crosshair at center of lens
    pub show_crosshair: bool,
}

static MAGNIFIER_CONFIG: Mutex<MagnifierConfig> = Mutex::new(MagnifierConfig {
    factor: 2,
    radius: 120,
    follow_cursor: true,
    fullscreen_mode: false,
    show_crosshair: false,
});

/// Set magnifier configuration
pub fn set_magnifier_config(config: MagnifierConfig) {
    *MAGNIFIER_CONFIG.lock() = config;
    crate::serial_println!(
        "[a11y] Magnifier: {}x, radius={}, fullscreen={}",
        config.factor,
        config.radius,
        config.fullscreen_mode
    );
}

/// Get magnifier configuration
pub fn magnifier_config() -> MagnifierConfig {
    *MAGNIFIER_CONFIG.lock()
}

// ═══════════════════════════════════════════════════════════════════════
// VOICE COMMANDS
// ═══════════════════════════════════════════════════════════════════════

static VOICE_CONTROL_ENABLED: AtomicBool = AtomicBool::new(false);

/// Voice command action
#[derive(Clone, Debug)]
pub enum VoiceAction {
    Click,
    ScrollUp,
    ScrollDown,
    GoBack,
    GoHome,
    Close,
    Minimize,
    Maximize,
    NextWindow,
    TypeText(alloc::string::String),
}

/// Enable/disable voice control
pub fn set_voice_control(enabled: bool) {
    VOICE_CONTROL_ENABLED.store(enabled, Ordering::Relaxed);
    crate::serial_println!(
        "[a11y] Voice control: {}",
        if enabled { "enabled" } else { "disabled" }
    );
}

/// Is voice control enabled?
pub fn is_voice_control_enabled() -> bool {
    VOICE_CONTROL_ENABLED.load(Ordering::Relaxed)
}

/// Parse a voice command string into an action
pub fn parse_voice_command(text: &str) -> Option<VoiceAction> {
    let lower = text.to_ascii_lowercase();
    match lower.as_str() {
        "click" | "press" | "select" => Some(VoiceAction::Click),
        "scroll up" | "up" => Some(VoiceAction::ScrollUp),
        "scroll down" | "down" => Some(VoiceAction::ScrollDown),
        "go back" | "back" => Some(VoiceAction::GoBack),
        "go home" | "home" => Some(VoiceAction::GoHome),
        "close" | "close window" => Some(VoiceAction::Close),
        "minimize" => Some(VoiceAction::Minimize),
        "maximize" => Some(VoiceAction::Maximize),
        "next window" | "switch" => Some(VoiceAction::NextWindow),
        _ => {
            if lower.starts_with("type ") {
                Some(VoiceAction::TypeText(alloc::string::String::from(
                    &text[5..],
                )))
            } else {
                None
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SWITCH ACCESS — Single-button navigation
// ═══════════════════════════════════════════════════════════════════════

static SWITCH_ACCESS_ENABLED: AtomicBool = AtomicBool::new(false);

/// Switch access scanning state
#[derive(Clone, Copy, Debug)]
pub struct SwitchAccessState {
    /// Current highlighted element index
    pub current_index: usize,
    /// Total number of interactive elements
    pub total_elements: usize,
    /// Scanning speed (ms between moves)
    pub scan_interval_ms: u32,
    /// Whether auto-scanning is active
    pub auto_scan: bool,
}

static SWITCH_STATE: Mutex<SwitchAccessState> = Mutex::new(SwitchAccessState {
    current_index: 0,
    total_elements: 0,
    scan_interval_ms: 1000,
    auto_scan: false,
});

/// Enable switch access mode
pub fn enable_switch_access(auto_scan: bool, interval_ms: u32) {
    SWITCH_ACCESS_ENABLED.store(true, Ordering::Relaxed);
    let mut state = SWITCH_STATE.lock();
    state.auto_scan = auto_scan;
    state.scan_interval_ms = interval_ms;
    state.current_index = 0;
    crate::serial_println!(
        "[a11y] Switch access enabled (auto_scan={}, interval={}ms)",
        auto_scan,
        interval_ms
    );
}

/// Disable switch access mode
pub fn disable_switch_access() {
    SWITCH_ACCESS_ENABLED.store(false, Ordering::Relaxed);
}

/// Is switch access enabled?
pub fn is_switch_access_enabled() -> bool {
    SWITCH_ACCESS_ENABLED.load(Ordering::Relaxed)
}

/// Advance to next element (single switch press)
pub fn switch_access_next() -> usize {
    let mut state = SWITCH_STATE.lock();
    if state.total_elements > 0 {
        state.current_index = (state.current_index + 1) % state.total_elements;
    }
    state.current_index
}

/// Select the current element (second switch or long press)
pub fn switch_access_select() -> usize {
    let state = SWITCH_STATE.lock();
    crate::serial_println!(
        "[a11y] Switch access: selected element {}",
        state.current_index
    );
    state.current_index
}

// ═══════════════════════════════════════════════════════════════════════
// CAPTION / SUBTITLE SUPPORT
// ═══════════════════════════════════════════════════════════════════════

static CAPTIONS_ENABLED: AtomicBool = AtomicBool::new(false);

/// Caption display configuration
#[derive(Clone, Debug)]
pub struct CaptionConfig {
    pub enabled: bool,
    pub font_size: u8,
    pub background_opacity: u8,
    pub text_color: super::framebuffer::Pixel,
    pub position: CaptionPosition,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CaptionPosition {
    Bottom,
    Top,
}

lazy_static::lazy_static! {
    static ref CAPTION_CONFIG: Mutex<CaptionConfig> = Mutex::new(CaptionConfig {
        enabled: false,
        font_size: 18,
        background_opacity: 180,
        text_color: super::framebuffer::Pixel::new(255, 255, 255, 255),
        position: CaptionPosition::Bottom,
    });
    static ref CURRENT_CAPTION: Mutex<alloc::string::String> = Mutex::new(alloc::string::String::new());
}

/// Enable/disable captions
pub fn set_captions_enabled(enabled: bool) {
    CAPTIONS_ENABLED.store(enabled, Ordering::Relaxed);
    CAPTION_CONFIG.lock().enabled = enabled;
}

/// Set caption text (called by media player)
pub fn set_caption_text(text: &str) {
    *CURRENT_CAPTION.lock() = alloc::string::String::from(text);
}

/// Get current caption text for rendering
pub fn current_caption() -> alloc::string::String {
    CURRENT_CAPTION.lock().clone()
}

/// Draw caption overlay on framebuffer
pub fn draw_captions(fb: &mut super::framebuffer::FrameBuffer) {
    if !CAPTIONS_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let text = CURRENT_CAPTION.lock().clone();
    if text.is_empty() {
        return;
    }

    let config = CAPTION_CONFIG.lock();
    let y = if config.position == CaptionPosition::Bottom {
        fb.height.saturating_sub(60)
    } else {
        20
    };

    // Draw semi-transparent background
    let bg = super::framebuffer::Pixel::new(0, 0, 0, config.background_opacity);
    let rect = super::framebuffer::Rect {
        x: 0,
        y: y as i32,
        width: fb.width as u32,
        height: 40,
    };
    fb.fill_rect(rect, bg);

    // Draw caption text centered
    let text_width = text.len() * 8;
    let x = (fb.width.saturating_sub(text_width)) / 2;
    super::fonts::draw_string_compact(fb, x as i32, (y + 12) as i32, &text, config.text_color, 1);
}

// ═══════════════════════════════════════════════════════════════════════
// ACCESSIBILITY SETTINGS PANEL DATA
// ═══════════════════════════════════════════════════════════════════════

/// Get summary of all accessibility settings for settings panel
pub fn get_settings_summary() -> Vec<(&'static str, bool)> {
    alloc::vec![
        ("Screen Reader", is_screen_reader_active()),
        ("High Contrast", is_high_contrast()),
        ("Large Cursor", is_large_cursor()),
        ("Sticky Keys", is_sticky_keys()),
        ("Reduced Motion", is_reduced_motion()),
        ("Zoom", is_zoomed()),
        ("Voice Control", is_voice_control_enabled()),
        ("Switch Access", is_switch_access_enabled()),
        ("Captions", CAPTIONS_ENABLED.load(Ordering::Relaxed)),
    ]
}
