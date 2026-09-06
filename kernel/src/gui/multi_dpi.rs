//! Multi-DPI Display Support — per-display scale factors
//!
//! Manages independent DPI scaling for each connected display,
//! allowing mixed-DPI setups (e.g., 4K main + 1080p secondary).
//! Covers status.md item 7.32 (Multi-DPI display support).

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

/// Display DPI information
#[derive(Debug, Clone, Copy)]
pub struct DisplayDpi {
    /// Display index (0 = primary)
    pub display_id: usize,
    /// Physical width in mm (if known)
    pub physical_width_mm: u32,
    /// Physical height in mm (if known)
    pub physical_height_mm: u32,
    /// Pixel width
    pub pixel_width: u32,
    /// Pixel height
    pub pixel_height: u32,
    /// Computed DPI (horizontal)
    pub dpi_x: f32,
    /// Computed DPI (vertical)
    pub dpi_y: f32,
    /// UI scale factor (1.0 = 96 DPI, 2.0 = 192 DPI)
    pub scale_factor: f32,
    /// User override scale (if set)
    pub user_scale_override: Option<f32>,
}

impl DisplayDpi {
    /// Compute DPI from physical dimensions
    pub fn compute_dpi(pixel_w: u32, pixel_h: u32, mm_w: u32, mm_h: u32) -> (f32, f32) {
        if mm_w == 0 || mm_h == 0 {
            return (96.0, 96.0); // Default
        }
        let dpi_x = (pixel_w as f32) / (mm_w as f32 / 25.4);
        let dpi_y = (pixel_h as f32) / (mm_h as f32 / 25.4);
        (dpi_x, dpi_y)
    }

    /// Get the effective scale factor for UI elements
    pub fn effective_scale(&self) -> f32 {
        self.user_scale_override.unwrap_or(self.scale_factor)
    }

    /// Compute recommended scale factor from DPI
    pub fn recommended_scale(dpi: f32) -> f32 {
        if dpi <= 120.0 {
            1.0
        } else if dpi <= 168.0 {
            1.25
        } else if dpi <= 216.0 {
            1.5
        } else if dpi <= 264.0 {
            2.0
        } else if dpi <= 360.0 {
            2.5
        } else {
            3.0
        }
    }
}

/// Multi-DPI state
struct MultiDpiState {
    displays: Vec<DisplayDpi>,
    global_scale_override: Option<f32>,
}

lazy_static::lazy_static! {
    static ref STATE: Mutex<MultiDpiState> = Mutex::new(MultiDpiState {
        displays: Vec::new(),
        global_scale_override: None,
    });
}

static SCALE_QUERIES: AtomicU64 = AtomicU64::new(0);

/// Register a display with its physical dimensions
pub fn register_display(display_id: usize, pixel_w: u32, pixel_h: u32, mm_w: u32, mm_h: u32) {
    let (dpi_x, dpi_y) = DisplayDpi::compute_dpi(pixel_w, pixel_h, mm_w, mm_h);
    let avg_dpi = (dpi_x + dpi_y) / 2.0;
    let scale = DisplayDpi::recommended_scale(avg_dpi);

    let info = DisplayDpi {
        display_id,
        physical_width_mm: mm_w,
        physical_height_mm: mm_h,
        pixel_width: pixel_w,
        pixel_height: pixel_h,
        dpi_x,
        dpi_y,
        scale_factor: scale,
        user_scale_override: None,
    };

    let mut state = STATE.lock();
    // Replace existing or add new
    if let Some(existing) = state
        .displays
        .iter_mut()
        .find(|d| d.display_id == display_id)
    {
        *existing = info;
    } else {
        state.displays.push(info);
    }

    crate::serial_println!(
        "[multi_dpi] Display {} registered: {}x{} @ {:.0} DPI, scale={:.2}x",
        display_id,
        pixel_w,
        pixel_h,
        avg_dpi,
        scale
    );
}

/// Get the scale factor for a specific display
pub fn get_scale(display_id: usize) -> f32 {
    SCALE_QUERIES.fetch_add(1, Ordering::Relaxed);
    let state = STATE.lock();

    if let Some(global) = state.global_scale_override {
        return global;
    }

    state
        .displays
        .iter()
        .find(|d| d.display_id == display_id)
        .map(|d| d.effective_scale())
        .unwrap_or(1.0)
}

/// Set a user override scale for a display
pub fn set_display_scale(display_id: usize, scale: f32) {
    let mut state = STATE.lock();
    if let Some(d) = state
        .displays
        .iter_mut()
        .find(|d| d.display_id == display_id)
    {
        d.user_scale_override = Some(scale.clamp(0.5, 4.0));
        crate::serial_println!(
            "[multi_dpi] Display {} scale override: {:.2}x",
            display_id,
            scale
        );
    }
}

/// Set a global scale override (applies to all displays)
pub fn set_global_scale(scale: Option<f32>) {
    STATE.lock().global_scale_override = scale.map(|s| s.clamp(0.5, 4.0));
}

/// Scale a logical pixel value to physical pixels for a given display
pub fn logical_to_physical(logical: f32, display_id: usize) -> f32 {
    logical * get_scale(display_id)
}

/// Scale a physical pixel value to logical pixels for a given display
pub fn physical_to_logical(physical: f32, display_id: usize) -> f32 {
    physical / get_scale(display_id)
}

/// Get display count
pub fn display_count() -> usize {
    STATE.lock().displays.len()
}

/// Initialize the multi-DPI subsystem
pub fn init() {
    // Register the primary display with default dimensions
    register_display(0, 1920, 1080, 527, 296); // ~96 DPI (24" 1080p)
    crate::serial_println!("[multi_dpi] Multi-DPI display support initialized");
}
