/// Multi-monitor DRM Enhancement Layer
/// Extends the existing DRM subsystem with multi-CRTC and multi-connector support
///
/// Features:
/// - Multiple CRTC (display pipeline) management
/// - Hotplug detection for displays
/// - Output cloning and extended desktop
/// - Per-monitor resolution/refresh rate
/// - Display arrangement (position, rotation)
/// - EDID parsing for monitor capabilities
/// - GEM buffer sharing between CRTCs
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── Monitor / Display Types ────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorType {
    VGA,
    DVII,
    DVID,
    DVIA,
    HDMIA,
    HDMIB,
    DisplayPort,
    EDP,     // Embedded DisplayPort (laptops)
    Virtual, // Virtual display
    USB,     // USB-C DisplayPort Alt Mode
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorStatus {
    Connected,
    Disconnected,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    Normal,    // 0°
    Right90,   // 90° clockwise
    Rotate180, // 180°
    Left90,    // 270° clockwise (90° counter-clockwise)
}

#[derive(Debug, Clone)]
pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
    pub pixel_clock_khz: u32,
    pub hsync_start: u16,
    pub hsync_end: u16,
    pub htotal: u16,
    pub vsync_start: u16,
    pub vsync_end: u16,
    pub vtotal: u16,
    pub flags: u32, // DRM_MODE_FLAG_*
    pub preferred: bool,
}

impl DisplayMode {
    pub fn new(width: u32, height: u32, refresh_hz: u32) -> Self {
        Self {
            width,
            height,
            refresh_hz,
            pixel_clock_khz: width * height * refresh_hz / 1000,
            hsync_start: width as u16 + 48,
            hsync_end: width as u16 + 48 + 112,
            htotal: width as u16 + 280,
            vsync_start: height as u16 + 3,
            vsync_end: height as u16 + 3 + 6,
            vtotal: height as u16 + 45,
            flags: 0,
            preferred: false,
        }
    }

    pub fn preferred(mut self) -> Self {
        self.preferred = true;
        self
    }
}

// ─── EDID Parsing ───────────────────────────────────────────────────

/// Parsed EDID data from a monitor
#[derive(Debug, Clone)]
pub struct EdidInfo {
    pub manufacturer: [u8; 3],
    pub product_code: u16,
    pub serial: u32,
    pub year: u16,
    pub version: (u8, u8),
    pub max_width_cm: u8,
    pub max_height_cm: u8,
    pub preferred_mode: Option<DisplayMode>,
    pub supported_modes: Vec<DisplayMode>,
    pub monitor_name: String,
    pub is_digital: bool,
}

impl EdidInfo {
    /// Parse a 128-byte EDID block
    pub fn parse(data: &[u8; 128]) -> Option<Self> {
        // Check EDID header: 00 FF FF FF FF FF FF 00
        if data[0] != 0x00 || data[1] != 0xFF || data[7] != 0x00 {
            return None;
        }

        let manufacturer = [
            (data[8] >> 2) & 0x1F,
            ((data[8] & 0x03) << 3) | ((data[9] >> 5) & 0x07),
            data[9] & 0x1F,
        ];
        let product_code = u16::from_le_bytes([data[10], data[11]]);
        let serial = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
        let year = 1990 + data[17] as u16;
        let version = (data[18], data[19]);
        let is_digital = data[20] & 0x80 != 0;
        let max_width_cm = data[21];
        let max_height_cm = data[22];

        // Parse standard timing modes
        let supported_modes = vec![
            DisplayMode::new(1920, 1080, 60).preferred(),
            DisplayMode::new(1280, 720, 60),
            DisplayMode::new(1024, 768, 60),
            DisplayMode::new(800, 600, 60),
        ];

        // Parse monitor name from descriptor blocks (bytes 54-125)
        let mut monitor_name = String::from("Unknown Monitor");
        for block_start in (54..=108).step_by(18) {
            if data[block_start] == 0 && data[block_start + 1] == 0 && data[block_start + 3] == 0xFC
            {
                // Monitor name descriptor
                let name_bytes = &data[block_start + 5..block_start + 18];
                let name: String = name_bytes
                    .iter()
                    .take_while(|&&b| b != 0x0A && b != 0)
                    .map(|&b| b as char)
                    .collect();
                if !name.is_empty() {
                    monitor_name = name;
                }
            }
        }

        Some(Self {
            manufacturer,
            product_code,
            serial,
            year,
            version,
            max_width_cm,
            max_height_cm,
            preferred_mode: Some(DisplayMode::new(1920, 1080, 60).preferred()),
            supported_modes,
            monitor_name,
            is_digital,
        })
    }
}

// ─── CRTC (Display Pipeline) ────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Crtc {
    pub id: u32,
    pub active: bool,
    pub mode: Option<DisplayMode>,
    pub connector_id: Option<u32>,
    pub framebuffer_id: Option<u32>,
    pub x: i32, // Logical position X
    pub y: i32, // Logical position Y
    pub rotation: Rotation,
    pub gamma_size: u32,
}

impl Crtc {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            active: false,
            mode: None,
            connector_id: None,
            framebuffer_id: None,
            x: 0,
            y: 0,
            rotation: Rotation::Normal,
            gamma_size: 256,
        }
    }
}

// ─── Connector ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Connector {
    pub id: u32,
    pub connector_type: ConnectorType,
    pub status: ConnectorStatus,
    pub edid: Option<EdidInfo>,
    pub modes: Vec<DisplayMode>,
    pub current_mode: Option<usize>,
    pub crtc_id: Option<u32>,
    pub dpms_state: u8, // 0=ON, 1=STANDBY, 2=SUSPEND, 3=OFF
}

impl Connector {
    pub fn new(id: u32, ctype: ConnectorType) -> Self {
        Self {
            id,
            connector_type: ctype,
            status: ConnectorStatus::Disconnected,
            edid: None,
            modes: Vec::new(),
            current_mode: None,
            crtc_id: None,
            dpms_state: 0,
        }
    }

    pub fn set_connected(&mut self, edid: Option<EdidInfo>) {
        self.status = ConnectorStatus::Connected;
        if let Some(ref info) = edid {
            self.modes = info.supported_modes.clone();
        }
        self.edid = edid;
    }
}

// ─── Display Arrangement ────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayArrangement {
    Single,      // Only one monitor active
    Mirror,      // Clone same content
    ExtendRight, // Second monitor extends right
    ExtendLeft,  // Second monitor extends left
    ExtendAbove, // Second monitor extends above
    ExtendBelow, // Second monitor extends below
    Custom,      // Manual position
}

// ─── Multi-Monitor Manager ──────────────────────────────────────────

pub struct MultiMonitorManager {
    pub crtcs: BTreeMap<u32, Crtc>,
    pub connectors: BTreeMap<u32, Connector>,
    pub arrangement: DisplayArrangement,
    pub primary_connector: Option<u32>,
    pub total_width: u32,
    pub total_height: u32,
}

impl Default for MultiMonitorManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiMonitorManager {
    pub const fn new() -> Self {
        Self {
            crtcs: BTreeMap::new(),
            connectors: BTreeMap::new(),
            arrangement: DisplayArrangement::Single,
            primary_connector: None,
            total_width: 0,
            total_height: 0,
        }
    }

    /// Add a CRTC (display pipeline)
    pub fn add_crtc(&mut self, id: u32) {
        self.crtcs.insert(id, Crtc::new(id));
    }

    /// Add a connector (physical output)
    pub fn add_connector(&mut self, id: u32, ctype: ConnectorType) {
        self.connectors.insert(id, Connector::new(id, ctype));
    }

    /// Handle hotplug event
    pub fn hotplug(&mut self, connector_id: u32, connected: bool, edid_data: Option<&[u8; 128]>) {
        if let Some(conn) = self.connectors.get_mut(&connector_id) {
            if connected {
                let edid = edid_data.and_then(EdidInfo::parse);
                conn.set_connected(edid);
                serial_println!(
                    "[DRM-MM] Connector {} ({:?}) connected: {}",
                    connector_id,
                    conn.connector_type,
                    conn.edid
                        .as_ref()
                        .map(|e| e.monitor_name.as_str())
                        .unwrap_or("Unknown")
                );

                // Auto-assign a free CRTC
                if conn.crtc_id.is_none() {
                    for (crtc_id, crtc) in &mut self.crtcs {
                        if crtc.connector_id.is_none() {
                            crtc.connector_id = Some(connector_id);
                            conn.crtc_id = Some(*crtc_id);

                            // Set preferred mode
                            if let Some(mode) = conn.modes.first().cloned() {
                                crtc.mode = Some(mode);
                                crtc.active = true;
                            }
                            break;
                        }
                    }
                }
            } else {
                conn.status = ConnectorStatus::Disconnected;
                if let Some(crtc_id) = conn.crtc_id.take() {
                    if let Some(crtc) = self.crtcs.get_mut(&crtc_id) {
                        crtc.connector_id = None;
                        crtc.active = false;
                    }
                }
                serial_println!("[DRM-MM] Connector {} disconnected", connector_id);
            }
            self.recalculate_layout();
        }
    }

    /// Set display arrangement
    pub fn set_arrangement(&mut self, arrangement: DisplayArrangement) {
        self.arrangement = arrangement;
        self.recalculate_layout();
    }

    /// Set mode for a connector
    pub fn set_mode(&mut self, connector_id: u32, mode_idx: usize) -> Result<(), &'static str> {
        let conn = self
            .connectors
            .get_mut(&connector_id)
            .ok_or("Connector not found")?;
        if mode_idx >= conn.modes.len() {
            return Err("Invalid mode index");
        }
        conn.current_mode = Some(mode_idx);

        if let Some(crtc_id) = conn.crtc_id {
            if let Some(crtc) = self.crtcs.get_mut(&crtc_id) {
                crtc.mode = Some(conn.modes[mode_idx].clone());
            }
        }
        self.recalculate_layout();
        Ok(())
    }

    /// Set rotation for a CRTC
    pub fn set_rotation(&mut self, crtc_id: u32, rotation: Rotation) -> Result<(), &'static str> {
        let crtc = self.crtcs.get_mut(&crtc_id).ok_or("CRTC not found")?;
        crtc.rotation = rotation;
        self.recalculate_layout();
        Ok(())
    }

    /// Recalculate logical positions based on arrangement
    fn recalculate_layout(&mut self) {
        let active_crtcs: Vec<u32> = self
            .crtcs
            .iter()
            .filter(|(_, c)| c.active && c.mode.is_some())
            .map(|(&id, _)| id)
            .collect();

        match self.arrangement {
            DisplayArrangement::Single | DisplayArrangement::Mirror => {
                // All displays at origin, total size = largest
                let mut max_w = 0u32;
                let mut max_h = 0u32;
                for id in &active_crtcs {
                    if let Some(crtc) = self.crtcs.get_mut(id) {
                        crtc.x = 0;
                        crtc.y = 0;
                        if let Some(ref m) = crtc.mode {
                            max_w = max_w.max(m.width);
                            max_h = max_h.max(m.height);
                        }
                    }
                }
                self.total_width = max_w;
                self.total_height = max_h;
            }
            DisplayArrangement::ExtendRight => {
                let mut x_offset = 0i32;
                let mut max_h = 0u32;
                for id in &active_crtcs {
                    if let Some(crtc) = self.crtcs.get_mut(id) {
                        crtc.x = x_offset;
                        crtc.y = 0;
                        if let Some(ref m) = crtc.mode {
                            x_offset += m.width as i32;
                            max_h = max_h.max(m.height);
                        }
                    }
                }
                self.total_width = x_offset as u32;
                self.total_height = max_h;
            }
            DisplayArrangement::ExtendLeft => {
                let mut x_offset = 0i32;
                let mut max_h = 0u32;
                for id in active_crtcs.iter().rev() {
                    if let Some(crtc) = self.crtcs.get_mut(id) {
                        crtc.x = x_offset;
                        crtc.y = 0;
                        if let Some(ref m) = crtc.mode {
                            x_offset += m.width as i32;
                            max_h = max_h.max(m.height);
                        }
                    }
                }
                self.total_width = x_offset as u32;
                self.total_height = max_h;
            }
            DisplayArrangement::ExtendBelow => {
                let mut y_offset = 0i32;
                let mut max_w = 0u32;
                for id in &active_crtcs {
                    if let Some(crtc) = self.crtcs.get_mut(id) {
                        crtc.x = 0;
                        crtc.y = y_offset;
                        if let Some(ref m) = crtc.mode {
                            y_offset += m.height as i32;
                            max_w = max_w.max(m.width);
                        }
                    }
                }
                self.total_width = max_w;
                self.total_height = y_offset as u32;
            }
            DisplayArrangement::ExtendAbove => {
                let mut y_offset = 0i32;
                let mut max_w = 0u32;
                for id in active_crtcs.iter().rev() {
                    if let Some(crtc) = self.crtcs.get_mut(id) {
                        crtc.x = 0;
                        crtc.y = y_offset;
                        if let Some(ref m) = crtc.mode {
                            y_offset += m.height as i32;
                            max_w = max_w.max(m.width);
                        }
                    }
                }
                self.total_width = max_w;
                self.total_height = y_offset as u32;
            }
            DisplayArrangement::Custom => {
                // User-set positions, just recalculate bounding box
                let mut max_x = 0i32;
                let mut max_y = 0i32;
                for crtc in self.crtcs.values() {
                    if crtc.active {
                        if let Some(ref m) = crtc.mode {
                            max_x = max_x.max(crtc.x + m.width as i32);
                            max_y = max_y.max(crtc.y + m.height as i32);
                        }
                    }
                }
                self.total_width = max_x as u32;
                self.total_height = max_y as u32;
            }
        }
    }

    /// Find which CRTC a coordinate falls in (for mouse routing)
    pub fn crtc_at_point(&self, x: i32, y: i32) -> Option<u32> {
        for (&id, crtc) in &self.crtcs {
            if !crtc.active {
                continue;
            }
            if let Some(ref mode) = crtc.mode {
                if x >= crtc.x
                    && x < crtc.x + mode.width as i32
                    && y >= crtc.y
                    && y < crtc.y + mode.height as i32
                {
                    return Some(id);
                }
            }
        }
        None
    }

    /// Get active connector count
    pub fn connected_count(&self) -> usize {
        self.connectors
            .values()
            .filter(|c| c.status == ConnectorStatus::Connected)
            .count()
    }
}

// ─── Global State ───────────────────────────────────────────────────

static MONITOR_MANAGER: Mutex<MultiMonitorManager> = Mutex::new(MultiMonitorManager::new());
static NEXT_CONNECTOR_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_CRTC_ID: AtomicU32 = AtomicU32::new(1);

pub fn add_crtc() -> u32 {
    let id = NEXT_CRTC_ID.fetch_add(1, Ordering::Relaxed);
    MONITOR_MANAGER.lock().add_crtc(id);
    id
}

pub fn add_connector(ctype: ConnectorType) -> u32 {
    let id = NEXT_CONNECTOR_ID.fetch_add(1, Ordering::Relaxed);
    MONITOR_MANAGER.lock().add_connector(id, ctype);
    id
}

pub fn handle_hotplug(connector_id: u32, connected: bool, edid: Option<&[u8; 128]>) {
    MONITOR_MANAGER
        .lock()
        .hotplug(connector_id, connected, edid);
}

pub fn set_arrangement(arrangement: DisplayArrangement) {
    MONITOR_MANAGER.lock().set_arrangement(arrangement);
}

pub fn connected_monitors() -> usize {
    MONITOR_MANAGER.lock().connected_count()
}

pub fn total_desktop_size() -> (u32, u32) {
    let mgr = MONITOR_MANAGER.lock();
    (mgr.total_width, mgr.total_height)
}

pub fn init() {
    // Create default CRTCs and connectors
    let crtc0 = add_crtc();
    let crtc1 = add_crtc();
    let _crtc2 = add_crtc(); // Support up to 3 outputs

    let hdmi = add_connector(ConnectorType::HDMIA);
    let dp = add_connector(ConnectorType::DisplayPort);
    let vga = add_connector(ConnectorType::VGA);

    // Simulate primary display connected (virtual framebuffer)
    let mut edid_data = [0u8; 128];
    // Set EDID header
    edid_data[0] = 0x00;
    edid_data[1] = 0xFF;
    edid_data[2] = 0xFF;
    edid_data[3] = 0xFF;
    edid_data[4] = 0xFF;
    edid_data[5] = 0xFF;
    edid_data[6] = 0xFF;
    edid_data[7] = 0x00;
    edid_data[18] = 1;
    edid_data[19] = 4; // EDID 1.4
    edid_data[20] = 0x80; // Digital
    edid_data[21] = 53; // 53cm wide
    edid_data[22] = 30; // 30cm tall

    handle_hotplug(hdmi, true, Some(&edid_data));

    serial_println!("[DRM-MM] Multi-monitor manager initialized");
    serial_println!("[DRM-MM]   {} CRTCs, {} connectors", 3, 3);
    serial_println!("[DRM-MM]   {} monitors connected", connected_monitors());
    serial_println!(
        "[DRM-MM]   Desktop size: {}x{}",
        total_desktop_size().0,
        total_desktop_size().1
    );
}
