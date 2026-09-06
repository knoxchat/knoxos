use super::dpi::PhysicalSize;
use super::scale;
/// MonitorHandle & VideoMode — winit-inspired display abstraction
///
/// Provides structured types for querying the display's capabilities:
/// current resolution, scale factor, supported video modes with bit depth
/// and refresh rate.
///
/// Adapted from `winit/winit-core/src/monitor.rs`.
use alloc::string::String;
use alloc::vec::Vec;

// ═══════════════════════════════════════════════════════════════════════
// VideoMode — a supported display mode
// ═══════════════════════════════════════════════════════════════════════

/// Describes a fullscreen video mode of a display.
///
/// Adapted from `winit::monitor::VideoMode`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VideoMode {
    /// Resolution in physical pixels.
    pub size: PhysicalSize<u32>,
    /// Color depth in bits (typically 32 for modern displays).
    pub bit_depth: u16,
    /// Refresh rate in millihertz (60000 = 60 Hz, 144000 = 144 Hz).
    /// Using millihertz avoids floating-point for exact comparisons.
    pub refresh_rate_millihertz: u32,
}

impl VideoMode {
    /// Get the refresh rate in Hz as a floating-point value.
    #[inline]
    pub fn refresh_rate_hz(&self) -> f64 {
        self.refresh_rate_millihertz as f64 / 1000.0
    }

    /// Human-readable label, e.g. "1920×1080 @ 60Hz".
    pub fn label(&self) -> String {
        use alloc::format;
        format!(
            "{}x{} @ {}Hz",
            self.size.width,
            self.size.height,
            self.refresh_rate_millihertz / 1000,
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════
// MonitorHandle — an abstraction over the physical display
// ═══════════════════════════════════════════════════════════════════════

/// A handle to a physical monitor/display.
///
/// In KnoxOS (QEMU), there is typically one monitor. This abstraction
/// provides a clean API for querying display properties and available modes.
///
/// Adapted from `winit::monitor::MonitorHandle`.
pub struct MonitorHandle {
    /// Human-readable monitor name (e.g., "QEMU VGA", "Primary Display").
    name: Option<String>,
    /// Current resolution in physical pixels.
    current_size: PhysicalSize<u32>,
    /// Current scale factor (1.0 = 100%, 1.25 = 125%, etc.)
    current_scale_factor: f64,
    /// Monitor position in the virtual screen coordinate space.
    /// For single-monitor setups, this is always (0, 0).
    position: (i32, i32),
    /// Available video modes.
    video_modes: Vec<VideoMode>,
}

impl MonitorHandle {
    /// Query the current primary monitor.
    ///
    /// Reads the live framebuffer state and scale factor to build a handle.
    pub fn primary() -> Self {
        let (sw, sh) = super::cached_screen_size();
        let sf = scale::scale_factor();

        // Build available video modes from the RESOLUTIONS table
        let modes: Vec<VideoMode> = super::RESOLUTIONS
            .iter()
            .map(|&(w, h, _label)| VideoMode {
                size: PhysicalSize::new(w as u32, h as u32),
                bit_depth: 32,
                refresh_rate_millihertz: 60000, // QEMU standard VGA = 60 Hz
            })
            .collect();

        Self {
            name: Some(String::from("QEMU VGA Display")),
            current_size: PhysicalSize::new(sw as u32, sh as u32),
            current_scale_factor: sf,
            position: (0, 0),
            video_modes: modes,
        }
    }

    /// Get the monitor's name (if available).
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Get the current resolution in physical pixels.
    #[inline]
    pub fn size(&self) -> PhysicalSize<u32> {
        self.current_size
    }

    /// Get the width in physical pixels.
    #[inline]
    pub fn width(&self) -> u32 {
        self.current_size.width
    }

    /// Get the height in physical pixels.
    #[inline]
    pub fn height(&self) -> u32 {
        self.current_size.height
    }

    /// Get the monitor's scale factor.
    ///
    /// The scale factor relates logical coordinates to physical pixels:
    /// `physical = logical × scale_factor`.
    #[inline]
    pub fn scale_factor(&self) -> f64 {
        self.current_scale_factor
    }

    /// Get the monitor's position in the virtual screen space.
    ///
    /// For single-monitor setups, this is always `(0, 0)`.
    /// Multi-monitor layouts would have different offsets.
    #[inline]
    pub fn position(&self) -> (i32, i32) {
        self.position
    }

    /// Get an iterator over available video modes.
    pub fn video_modes(&self) -> &[VideoMode] {
        &self.video_modes
    }

    /// Find the "best" video mode for the given target resolution.
    /// Returns the mode that matches the resolution, preferring higher refresh rates.
    pub fn best_mode_for(&self, target_w: u32, target_h: u32) -> Option<&VideoMode> {
        self.video_modes
            .iter()
            .filter(|m| m.size.width == target_w && m.size.height == target_h)
            .max_by_key(|m| m.refresh_rate_millihertz)
    }

    /// Get the current video mode.
    pub fn current_video_mode(&self) -> VideoMode {
        VideoMode {
            size: self.current_size,
            bit_depth: 32,
            refresh_rate_millihertz: 60000,
        }
    }

    /// Refresh the handle with current live values.
    /// Call this after a resolution change.
    pub fn refresh(&mut self) {
        let (sw, sh) = super::cached_screen_size();
        self.current_size = PhysicalSize::new(sw as u32, sh as u32);
        self.current_scale_factor = scale::scale_factor();
    }
}
