//! Tablet & 2-in-1 Device Integration
//!
//! Provides comprehensive support for convertible laptops, tablets, and 2-in-1 devices:
//!   - Screen auto-rotation via accelerometer data
//!   - Tablet mode detection (hinge angle sensor / EC events)
//!   - Touch-first UI mode switching (larger targets, on-screen keyboard)
//!   - Palm rejection coordination with touchscreen driver
//!   - Stylus proximity detection
//!   - Virtual keyboard auto-show in tablet mode
//!
//! Integrates with:
//!   - touchscreen.rs (multi-touch input)
//!   - trackpad.rs (disable in tablet mode)
//!   - wacom.rs (stylus input)
//!   - gui/desktop.rs (UI mode switching)
//!   - depthcharge.rs (Chromebook EC accelerometer)
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// SCREEN ORIENTATION
// ═══════════════════════════════════════════════════════════════════════

/// Screen orientation states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    /// Normal landscape (0°)
    Landscape,
    /// Portrait, rotated 90° clockwise (right edge is now top)
    PortraitRight,
    /// Inverted landscape (180°)
    LandscapeInverted,
    /// Portrait, rotated 90° counter-clockwise (left edge is now top)
    PortraitLeft,
}

impl Orientation {
    /// Rotation angle in degrees (clockwise)
    pub fn degrees(self) -> u32 {
        match self {
            Self::Landscape => 0,
            Self::PortraitRight => 90,
            Self::LandscapeInverted => 180,
            Self::PortraitLeft => 270,
        }
    }

    /// Whether this is a portrait orientation
    pub fn is_portrait(self) -> bool {
        matches!(self, Self::PortraitRight | Self::PortraitLeft)
    }

    /// Transform screen coordinates from physical to logical
    pub fn transform_coords(self, x: u32, y: u32, screen_w: u32, screen_h: u32) -> (u32, u32) {
        match self {
            Self::Landscape => (x, y),
            Self::PortraitRight => (y, screen_w.saturating_sub(x)),
            Self::LandscapeInverted => (screen_w.saturating_sub(x), screen_h.saturating_sub(y)),
            Self::PortraitLeft => (screen_h.saturating_sub(y), x),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ACCELEROMETER DATA PROCESSING
// ═══════════════════════════════════════════════════════════════════════

/// Raw accelerometer reading (in milli-g, 1g = 9806 mg)
#[derive(Debug, Clone, Copy)]
pub struct AccelReading {
    pub x: i32, // Left-right tilt
    pub y: i32, // Forward-backward tilt
    pub z: i32, // Up-down (gravity)
    pub timestamp_ms: u64,
}

/// Accelerometer data source
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccelSource {
    /// IIO (Industrial I/O) accelerometer (Linux-style)
    Iio,
    /// Chrome EC accelerometer (via EC commands)
    ChromeEc,
    /// ACPI sensor hub (Windows-style tablets)
    AcpiSensorHub,
    /// USB HID sensor
    UsbHid,
}

/// Accelerometer reading filter (low-pass to reduce jitter)
pub struct AccelFilter {
    /// Filtered x, y, z values (fixed-point, scaled by 1000)
    filtered_x: i64,
    filtered_y: i64,
    filtered_z: i64,
    /// Filter coefficient (0.0 to 1.0, stored as 0-1000)
    /// Higher = more smoothing, more lag
    alpha: i64,
    /// Whether we have a previous reading
    initialized: bool,
}

impl AccelFilter {
    pub fn new(alpha: f32) -> Self {
        Self {
            filtered_x: 0,
            filtered_y: 0,
            filtered_z: 0,
            alpha: (alpha * 1000.0) as i64,
            initialized: false,
        }
    }

    /// Feed a new reading and get filtered output
    pub fn update(&mut self, reading: &AccelReading) -> AccelReading {
        if !self.initialized {
            self.filtered_x = reading.x as i64 * 1000;
            self.filtered_y = reading.y as i64 * 1000;
            self.filtered_z = reading.z as i64 * 1000;
            self.initialized = true;
            return *reading;
        }

        // Exponential moving average: filtered = alpha * filtered + (1-alpha) * new
        let one_minus_alpha = 1000 - self.alpha;
        self.filtered_x =
            (self.alpha * self.filtered_x + one_minus_alpha * (reading.x as i64 * 1000)) / 1000;
        self.filtered_y =
            (self.alpha * self.filtered_y + one_minus_alpha * (reading.y as i64 * 1000)) / 1000;
        self.filtered_z =
            (self.alpha * self.filtered_z + one_minus_alpha * (reading.z as i64 * 1000)) / 1000;

        AccelReading {
            x: (self.filtered_x / 1000) as i32,
            y: (self.filtered_y / 1000) as i32,
            z: (self.filtered_z / 1000) as i32,
            timestamp_ms: reading.timestamp_ms,
        }
    }
}

/// Determine screen orientation from accelerometer data
pub fn orientation_from_accel(reading: &AccelReading) -> Orientation {
    let x = reading.x;
    let y = reading.y;

    // Threshold to avoid jitter near axis boundaries (2g = 2000 milli-g)
    let threshold = 4000;

    // Dominant axis determines orientation
    let abs_x = if x < 0 { -x } else { x };
    let abs_y = if y < 0 { -y } else { y };

    if abs_x > abs_y {
        if abs_x < threshold {
            Orientation::Landscape // Near flat, keep landscape
        } else if x > 0 {
            Orientation::PortraitRight
        } else {
            Orientation::PortraitLeft
        }
    } else if abs_y < threshold {
        Orientation::Landscape
    } else if y > 0 {
        Orientation::LandscapeInverted
    } else {
        Orientation::Landscape
    }
}

// ═══════════════════════════════════════════════════════════════════════
// TABLET MODE DETECTION
// ═══════════════════════════════════════════════════════════════════════

/// Device form factor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormFactor {
    /// Standard laptop / clamshell
    Laptop,
    /// Tablet (no physical keyboard attached)
    Tablet,
    /// 2-in-1 convertible in tent mode (screen faces away from keyboard)
    Tent,
    /// 2-in-1 in tablet mode (folded 360°)
    Convertible360,
    /// Detachable tablet (keyboard detached)
    Detachable,
}

impl FormFactor {
    /// Whether touch input should be the primary input method
    pub fn is_touch_primary(self) -> bool {
        !matches!(self, Self::Laptop)
    }

    /// Whether the physical keyboard should be disabled
    pub fn disable_physical_keyboard(self) -> bool {
        matches!(self, Self::Tablet | Self::Convertible360 | Self::Detachable)
    }

    /// Whether the trackpad should be disabled
    pub fn disable_trackpad(self) -> bool {
        matches!(
            self,
            Self::Tablet | Self::Convertible360 | Self::Detachable | Self::Tent
        )
    }
}

/// Hinge angle (for convertible devices)
#[derive(Debug, Clone, Copy)]
pub struct HingeAngle {
    /// Angle in degrees (0 = closed, 180 = flat, 360 = tablet mode)
    pub degrees: u32,
}

impl HingeAngle {
    pub fn form_factor(self) -> FormFactor {
        match self.degrees {
            0..=5 => FormFactor::Laptop,             // Closed (sleeping)
            6..=200 => FormFactor::Laptop,           // Normal laptop use
            201..=300 => FormFactor::Tent,           // Tent mode
            301..=360 => FormFactor::Convertible360, // Full tablet
            _ => FormFactor::Laptop,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// TABLET MODE MANAGER
// ═══════════════════════════════════════════════════════════════════════

/// Central tablet mode state machine
pub struct TabletModeManager {
    /// Current device form factor
    pub form_factor: FormFactor,
    /// Current screen orientation
    pub orientation: Orientation,
    /// Whether auto-rotation is enabled
    pub auto_rotate_enabled: bool,
    /// Whether rotation lock is on (user override)
    pub rotation_locked: bool,
    /// Locked orientation (when rotation_locked is true)
    pub locked_orientation: Orientation,
    /// Accelerometer filter
    pub accel_filter: AccelFilter,
    /// Accelerometer source
    pub accel_source: AccelSource,
    /// Hinge angle (if available)
    pub hinge_angle: Option<HingeAngle>,
    /// Screen dimensions (physical, pre-rotation)
    pub screen_width: u32,
    pub screen_height: u32,
    /// On-screen keyboard visible
    pub osk_visible: bool,
    /// Palm rejection threshold (mm)
    pub palm_rejection_size_mm: u32,
    /// Debounce counter for orientation changes
    orientation_debounce: u32,
    pending_orientation: Orientation,
}

/// Debounce count before accepting orientation change (at ~60Hz ≈ 500ms)
const ORIENTATION_DEBOUNCE_THRESHOLD: u32 = 30;

impl TabletModeManager {
    pub fn new(screen_width: u32, screen_height: u32) -> Self {
        Self {
            form_factor: FormFactor::Laptop,
            orientation: Orientation::Landscape,
            auto_rotate_enabled: true,
            rotation_locked: false,
            locked_orientation: Orientation::Landscape,
            accel_filter: AccelFilter::new(0.8),
            accel_source: AccelSource::Iio,
            hinge_angle: None,
            screen_width,
            screen_height,
            osk_visible: false,
            palm_rejection_size_mm: 20,
            orientation_debounce: 0,
            pending_orientation: Orientation::Landscape,
        }
    }

    /// Process a new accelerometer reading
    /// Returns true if orientation changed
    pub fn process_accel(&mut self, raw: &AccelReading) -> bool {
        if self.rotation_locked || !self.auto_rotate_enabled {
            return false;
        }

        let filtered = self.accel_filter.update(raw);
        let new_orientation = orientation_from_accel(&filtered);

        if new_orientation == self.pending_orientation {
            self.orientation_debounce += 1;
        } else {
            self.pending_orientation = new_orientation;
            self.orientation_debounce = 0;
        }

        if self.orientation_debounce >= ORIENTATION_DEBOUNCE_THRESHOLD
            && self.pending_orientation != self.orientation
        {
            let old = self.orientation;
            self.orientation = self.pending_orientation;
            self.orientation_debounce = 0;
            serial_println!(
                "[Tablet] Orientation changed: {:?} → {:?} ({}°)",
                old,
                self.orientation,
                self.orientation.degrees()
            );
            return true;
        }

        false
    }

    /// Process hinge angle update
    /// Returns true if form factor changed
    pub fn process_hinge_angle(&mut self, degrees: u32) -> bool {
        let angle = HingeAngle { degrees };
        let new_form = angle.form_factor();
        self.hinge_angle = Some(angle);

        if new_form != self.form_factor {
            let old = self.form_factor;
            self.form_factor = new_form;
            serial_println!(
                "[Tablet] Form factor changed: {:?} → {:?} (hinge={}°)",
                old,
                self.form_factor,
                degrees
            );

            // Auto-show/hide on-screen keyboard
            if self.form_factor.is_touch_primary() && !self.osk_visible {
                serial_println!("[Tablet] Enabling on-screen keyboard");
                self.osk_visible = true;
            } else if !self.form_factor.is_touch_primary() && self.osk_visible {
                serial_println!("[Tablet] Disabling on-screen keyboard");
                self.osk_visible = false;
            }

            return true;
        }

        false
    }

    /// Toggle rotation lock
    pub fn toggle_rotation_lock(&mut self) {
        self.rotation_locked = !self.rotation_locked;
        if self.rotation_locked {
            self.locked_orientation = self.orientation;
            serial_println!("[Tablet] Rotation locked at {:?}", self.orientation);
        } else {
            serial_println!("[Tablet] Rotation unlocked");
        }
    }

    /// Get the effective screen dimensions (after rotation)
    pub fn effective_dimensions(&self) -> (u32, u32) {
        if self.orientation.is_portrait() {
            (self.screen_height, self.screen_width)
        } else {
            (self.screen_width, self.screen_height)
        }
    }

    /// Transform a touch coordinate from physical panel space to logical screen space
    pub fn transform_touch(&self, phys_x: u32, phys_y: u32) -> (u32, u32) {
        self.orientation
            .transform_coords(phys_x, phys_y, self.screen_width, self.screen_height)
    }

    /// Get UI scaling factor for tablet mode (larger touch targets)
    pub fn ui_scale_factor(&self) -> f32 {
        if self.form_factor.is_touch_primary() {
            1.25 // 25% larger touch targets
        } else {
            1.0
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PALM REJECTION
// ═══════════════════════════════════════════════════════════════════════

/// Touch contact classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchClassification {
    /// Normal finger touch
    Finger,
    /// Palm (should be rejected)
    Palm,
    /// Stylus tip
    Stylus,
    /// Unknown / unclassified
    Unknown,
}

/// Classify a touch contact based on size and pressure
pub fn classify_touch(
    major_axis_mm: u32,
    minor_axis_mm: u32,
    pressure: u32,
    palm_threshold_mm: u32,
) -> TouchClassification {
    // Palm detection: large contact area with low-medium pressure
    if major_axis_mm > palm_threshold_mm || minor_axis_mm > palm_threshold_mm {
        return TouchClassification::Palm;
    }

    // Stylus: very small contact with high pressure
    if major_axis_mm < 3 && minor_axis_mm < 3 && pressure > 200 {
        return TouchClassification::Stylus;
    }

    // Normal finger
    if major_axis_mm >= 3 && major_axis_mm <= palm_threshold_mm {
        return TouchClassification::Finger;
    }

    TouchClassification::Unknown
}

// ═══════════════════════════════════════════════════════════════════════
// GLOBAL STATE
// ═══════════════════════════════════════════════════════════════════════

static TABLET_MODE_ACTIVE: AtomicBool = AtomicBool::new(false);
static CURRENT_ORIENTATION: AtomicU32 = AtomicU32::new(0); // degrees
static TABLET_MANAGER: Mutex<Option<TabletModeManager>> = Mutex::new(None);

/// Initialize tablet mode support
pub fn init() {
    serial_println!("[Tablet] 2-in-1 / tablet device support initialized");
    serial_println!("[Tablet]   Auto-rotation: accelerometer-based (IIO/EC/ACPI)");
    serial_println!("[Tablet]   Tablet mode: hinge angle detection (0°-360°)");
    serial_println!("[Tablet]   Palm rejection: contact area classification");
    serial_println!("[Tablet]   Touch coordinate transform: per-orientation mapping");
    serial_println!("[Tablet]   Form factors: Laptop, Tablet, Tent, 360°, Detachable");

    let manager = TabletModeManager::new(1920, 1080);
    *TABLET_MANAGER.lock() = Some(manager);

    serial_println!("[Tablet]   Default: Landscape orientation, auto-rotate enabled");
}

/// Check if device is in tablet mode
pub fn is_tablet_mode() -> bool {
    TABLET_MODE_ACTIVE.load(Ordering::Relaxed)
}

/// Get current screen orientation in degrees
pub fn current_rotation() -> u32 {
    CURRENT_ORIENTATION.load(Ordering::Relaxed)
}

/// Feed an accelerometer reading to the tablet manager
pub fn feed_accel(x: i32, y: i32, z: i32, timestamp_ms: u64) -> bool {
    let reading = AccelReading {
        x,
        y,
        z,
        timestamp_ms,
    };
    let mut guard = TABLET_MANAGER.lock();
    if let Some(ref mut mgr) = *guard {
        let changed = mgr.process_accel(&reading);
        if changed {
            CURRENT_ORIENTATION.store(mgr.orientation.degrees(), Ordering::Relaxed);
        }
        changed
    } else {
        false
    }
}

/// Feed a hinge angle update
pub fn feed_hinge_angle(degrees: u32) -> bool {
    let mut guard = TABLET_MANAGER.lock();
    if let Some(ref mut mgr) = *guard {
        let changed = mgr.process_hinge_angle(degrees);
        if changed {
            TABLET_MODE_ACTIVE.store(mgr.form_factor.is_touch_primary(), Ordering::Relaxed);
        }
        changed
    } else {
        false
    }
}
