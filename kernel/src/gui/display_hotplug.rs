/// Display Hot-plug — Monitor connect/disconnect detection and handling
///
/// Provides:
///   - Monitor connection state tracking
///   - Hot-plug event handling (connected, disconnected, mode changed)
///   - Automatic layout adjustment on monitor add/remove
///   - DRM connector status polling
///   - Wayland wl_output advertisement on connect
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

use super::framebuffer::Rect;

// ═══════════════════════════════════════════════════════════════════════
// MONITOR INFO
// ═══════════════════════════════════════════════════════════════════════

/// Unique monitor identifier
pub type MonitorId = u32;

static NEXT_MONITOR_ID: AtomicU32 = AtomicU32::new(1);

/// Physical connector type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorType {
    Unknown,
    VGA,
    DVID,
    DVII,
    HDMI,
    DisplayPort,
    EDP, // embedded DisplayPort (laptop)
    UsbC,
    Virtual, // QEMU/VirtIO
}

impl ConnectorType {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::VGA => "VGA",
            Self::DVID => "DVI-D",
            Self::DVII => "DVI-I",
            Self::HDMI => "HDMI",
            Self::DisplayPort => "DisplayPort",
            Self::EDP => "eDP",
            Self::UsbC => "USB-C",
            Self::Virtual => "Virtual",
        }
    }
}

/// Connection status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Unknown,
}

/// Display mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    pub refresh_mhz: u32, // millihertz (60000 = 60Hz)
    pub preferred: bool,
}

/// EDID-parsed monitor information
#[derive(Debug, Clone)]
pub struct MonitorInfo {
    pub id: MonitorId,
    pub connector_type: ConnectorType,
    pub status: ConnectionStatus,
    /// Connector index (e.g., HDMI-1, DP-2)
    pub connector_index: u8,
    /// EDID manufacturer name
    pub manufacturer: String,
    /// EDID product name
    pub model: String,
    /// EDID serial number
    pub serial: String,
    /// Physical size in millimeters
    pub physical_width_mm: u32,
    pub physical_height_mm: u32,
    /// Available display modes
    pub modes: Vec<DisplayMode>,
    /// Current active mode index
    pub active_mode: Option<usize>,
    /// Position in the virtual desktop canvas
    pub position_x: i32,
    pub position_y: i32,
    /// Scale factor (100 = 1x, 200 = 2x for HiDPI)
    pub scale_percent: u32,
    /// Whether this is the primary monitor
    pub primary: bool,
    /// Whether the DPMS state is on
    pub dpms_on: bool,
}

impl MonitorInfo {
    /// Get the effective resolution (current mode)
    pub fn resolution(&self) -> (u32, u32) {
        self.active_mode
            .and_then(|i| self.modes.get(i))
            .map(|m| (m.width, m.height))
            .unwrap_or((1920, 1080))
    }

    /// Get the desktop area this monitor covers
    pub fn desktop_rect(&self) -> Rect {
        let (w, h) = self.resolution();
        Rect::new(self.position_x, self.position_y, w, h)
    }

    /// Calculate DPI from physical size and resolution
    pub fn dpi(&self) -> u32 {
        if self.physical_width_mm == 0 {
            return 96; // default
        }
        let (w, _) = self.resolution();
        // dpi = pixels / inches = pixels / (mm / 25.4)
        (w as u64 * 254 / (self.physical_width_mm as u64 * 10)) as u32
    }

    /// Display name for UI (e.g., "HDMI-1 (Dell U2723QE)")
    pub fn display_name(&self) -> String {
        alloc::format!(
            "{}-{} ({})",
            self.connector_type.name(),
            self.connector_index,
            if self.model.is_empty() {
                "Unknown"
            } else {
                &self.model
            }
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════
// HOT-PLUG EVENTS
// ═══════════════════════════════════════════════════════════════════════

/// Hot-plug event type
#[derive(Debug, Clone)]
pub enum HotplugEvent {
    /// Monitor connected — contains EDID-parsed info
    Connected(MonitorInfo),
    /// Monitor disconnected
    Disconnected(MonitorId),
    /// Monitor mode changed (resolution or refresh rate)
    ModeChanged(MonitorId, DisplayMode),
    /// DPMS state changed
    DpmsChanged(MonitorId, bool),
}

/// Callback type for hot-plug listeners
type HotplugCallback = fn(event: &HotplugEvent);

// ═══════════════════════════════════════════════════════════════════════
// STATE
// ═══════════════════════════════════════════════════════════════════════

struct HotplugState {
    /// All known monitors (connected or disconnected)
    monitors: Vec<MonitorInfo>,
    /// Event queue for pending hot-plug events
    events: Vec<HotplugEvent>,
    /// Registered listeners
    listeners: Vec<HotplugCallback>,
    /// Whether polling is active
    polling_active: bool,
    /// Poll interval in TSC ticks (~2 seconds)
    poll_interval_ticks: u64,
    /// Last poll timestamp
    last_poll_tsc: u64,
}

impl HotplugState {
    fn new() -> Self {
        Self {
            monitors: Vec::new(),
            events: Vec::new(),
            listeners: Vec::new(),
            polling_active: false,
            poll_interval_ticks: 4_000_000_000, // ~2 seconds at 2GHz
            last_poll_tsc: 0,
        }
    }
}

lazy_static::lazy_static! {
    static ref STATE: Mutex<HotplugState> = Mutex::new(HotplugState::new());
}

static HOTPLUG_INITIALIZED: AtomicBool = AtomicBool::new(false);

// ═══════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════

/// Initialize the hot-plug subsystem
pub fn init() {
    let mut state = STATE.lock();

    // Register the primary (boot) display
    let (sw, sh) = crate::gui::cached_screen_size();
    let primary = MonitorInfo {
        id: NEXT_MONITOR_ID.fetch_add(1, Ordering::Relaxed),
        connector_type: ConnectorType::Virtual,
        status: ConnectionStatus::Connected,
        connector_index: 1,
        manufacturer: String::from("KnoxOS"),
        model: String::from("Primary Display"),
        serial: String::from("KNOX-001"),
        physical_width_mm: 530,
        physical_height_mm: 300,
        modes: alloc::vec![DisplayMode {
            width: sw as u32,
            height: sh as u32,
            refresh_mhz: 60000,
            preferred: true,
        }],
        active_mode: Some(0),
        position_x: 0,
        position_y: 0,
        scale_percent: 100,
        primary: true,
        dpms_on: true,
    };
    state.monitors.push(primary);
    state.polling_active = true;
    HOTPLUG_INITIALIZED.store(true, Ordering::Relaxed);

    crate::serial_println!("[Hotplug] Initialized with primary display {}x{}", sw, sh);
}

/// Poll for hot-plug events (called periodically from main loop)
pub fn poll() {
    if !HOTPLUG_INITIALIZED.load(Ordering::Relaxed) {
        return;
    }

    let now = crate::gui::read_tsc_public();
    let mut state = STATE.lock();

    if !state.polling_active {
        return;
    }

    // Rate-limit polling
    if now.saturating_sub(state.last_poll_tsc) < state.poll_interval_ticks {
        return;
    }
    state.last_poll_tsc = now;

    // Check DRM connector status changes
    // In a real implementation, this would read DRM connector properties
    // For now, this is a hook for when DRM driver is connected
    #[cfg(any())] // TODO: enable when DRM driver is ready
    poll_drm_connectors(&mut state);
}

/// Register a hot-plug event listener
pub fn register_listener(callback: HotplugCallback) {
    STATE.lock().listeners.push(callback);
}

/// Get all connected monitors
pub fn connected_monitors() -> Vec<MonitorInfo> {
    STATE
        .lock()
        .monitors
        .iter()
        .filter(|m| m.status == ConnectionStatus::Connected)
        .cloned()
        .collect()
}

/// Get monitor by ID
pub fn get_monitor(id: MonitorId) -> Option<MonitorInfo> {
    STATE.lock().monitors.iter().find(|m| m.id == id).cloned()
}

/// Get the primary monitor
pub fn primary_monitor() -> Option<MonitorInfo> {
    STATE.lock().monitors.iter().find(|m| m.primary).cloned()
}

/// Get monitor count (connected only)
pub fn monitor_count() -> usize {
    STATE
        .lock()
        .monitors
        .iter()
        .filter(|m| m.status == ConnectionStatus::Connected)
        .count()
}

/// Notify that a monitor has been connected (called from DRM/GPU driver)
pub fn notify_connected(info: MonitorInfo) {
    let mut state = STATE.lock();
    let event = HotplugEvent::Connected(info.clone());

    // Add to monitors list
    state.monitors.push(info);

    // Queue event
    state.events.push(event.clone());

    // Notify listeners
    for listener in &state.listeners {
        listener(&event);
    }

    // Update Wayland outputs
    // wayland_server::add_output(...)

    crate::serial_println!("[Hotplug] Monitor connected");
    crate::gui::request_redraw();
}

/// Notify that a monitor has been disconnected
pub fn notify_disconnected(id: MonitorId) {
    let mut state = STATE.lock();

    // Mark as disconnected
    if let Some(monitor) = state.monitors.iter_mut().find(|m| m.id == id) {
        monitor.status = ConnectionStatus::Disconnected;
    }

    let event = HotplugEvent::Disconnected(id);
    state.events.push(event.clone());

    for listener in &state.listeners {
        listener(&event);
    }

    // Move windows from disconnected monitor to primary
    // This would call wm_core to reposition windows

    crate::serial_println!("[Hotplug] Monitor {} disconnected", id);
    crate::gui::request_redraw();
}

/// Set monitor position in virtual desktop
pub fn set_position(id: MonitorId, x: i32, y: i32) {
    let mut state = STATE.lock();
    if let Some(monitor) = state.monitors.iter_mut().find(|m| m.id == id) {
        monitor.position_x = x;
        monitor.position_y = y;
    }
    crate::gui::request_redraw();
}

/// Set monitor scale factor
pub fn set_scale(id: MonitorId, scale_percent: u32) {
    let mut state = STATE.lock();
    if let Some(monitor) = state.monitors.iter_mut().find(|m| m.id == id) {
        monitor.scale_percent = scale_percent.clamp(100, 300);
    }
    crate::gui::request_redraw();
}

/// Set primary monitor
pub fn set_primary(id: MonitorId) {
    let mut state = STATE.lock();
    for monitor in &mut state.monitors {
        monitor.primary = monitor.id == id;
    }
}

/// Change display mode for a monitor
pub fn set_mode(id: MonitorId, mode_index: usize) -> bool {
    let mut state = STATE.lock();
    if let Some(monitor) = state.monitors.iter_mut().find(|m| m.id == id) {
        if mode_index < monitor.modes.len() {
            monitor.active_mode = Some(mode_index);
            let mode = monitor.modes[mode_index];
            let event = HotplugEvent::ModeChanged(id, mode);
            state.events.push(event.clone());
            for listener in &state.listeners {
                listener(&event);
            }
            crate::gui::request_redraw();
            return true;
        }
    }
    false
}

/// Set DPMS power state (on/off/standby)
pub fn set_dpms(id: MonitorId, on: bool) {
    let mut state = STATE.lock();
    if let Some(monitor) = state.monitors.iter_mut().find(|m| m.id == id) {
        monitor.dpms_on = on;
        let event = HotplugEvent::DpmsChanged(id, on);
        state.events.push(event);
    }
}

/// Get the total virtual desktop size (union of all monitor rects)
pub fn total_desktop_size() -> (i32, i32, u32, u32) {
    let state = STATE.lock();
    let connected: Vec<_> = state
        .monitors
        .iter()
        .filter(|m| m.status == ConnectionStatus::Connected)
        .collect();

    if connected.is_empty() {
        return (0, 0, 1920, 1080);
    }

    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;

    for m in &connected {
        let r = m.desktop_rect();
        min_x = min_x.min(r.x);
        min_y = min_y.min(r.y);
        max_x = max_x.max(r.x + r.width as i32);
        max_y = max_y.max(r.y + r.height as i32);
    }

    (min_x, min_y, (max_x - min_x) as u32, (max_y - min_y) as u32)
}

/// Drain pending events (for consumers that poll rather than use callbacks)
pub fn drain_events() -> Vec<HotplugEvent> {
    let mut state = STATE.lock();
    core::mem::take(&mut state.events)
}
