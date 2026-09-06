/// VSync — Vertical synchronization and frame pacing
///
/// Provides frame timing synchronization to prevent tearing:
///   - Software VSync via TSC-based frame pacing (always available)
///   - Hardware VSync via DRM page flip events (when GPU driver supports it)
///   - Adaptive sync / variable refresh rate (FreeSync/G-Sync)
///   - Frame statistics and latency tracking
///   - Per-window frame callbacks (Wayland wl_callback)
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use alloc::vec::Vec;

// ═══════════════════════════════════════════════════════════════════════
// VSYNC MODE
// ═══════════════════════════════════════════════════════════════════════

/// VSync operating mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VSyncMode {
    /// No synchronization (immediate present, may tear)
    Off,
    /// Software-paced to target refresh rate using TSC timer
    Software,
    /// Hardware VSync via DRM page flip completion IRQ
    Hardware,
    /// Adaptive: VSync on when FPS >= refresh rate, off when below
    Adaptive,
    /// Mailbox: always present latest frame, no queuing delay
    Mailbox,
}

/// Current VSync mode
static VSYNC_MODE: AtomicU32 = AtomicU32::new(1); // Software by default

/// Target refresh rate in millihertz (60000 = 60 Hz)
static TARGET_REFRESH_MHZ: AtomicU32 = AtomicU32::new(60000);

/// Frame interval in TSC ticks (auto-calibrated)
static FRAME_INTERVAL_TICKS: AtomicU64 = AtomicU64::new(33_000_000);

/// Whether hardware VSync is available
static HW_VSYNC_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Whether adaptive sync (VRR) is supported
static VRR_SUPPORTED: AtomicBool = AtomicBool::new(false);

/// Frame counter
static VSYNC_FRAME_COUNT: AtomicU64 = AtomicU64::new(0);

/// Timestamp of last VSync event
static LAST_VSYNC_TSC: AtomicU64 = AtomicU64::new(0);

/// Whether a page flip is pending (DRM)
static PAGE_FLIP_PENDING: AtomicBool = AtomicBool::new(false);

// ═══════════════════════════════════════════════════════════════════════
// FRAME STATISTICS
// ═══════════════════════════════════════════════════════════════════════

/// Frame timing statistics for performance monitoring
#[derive(Debug, Clone, Copy)]
pub struct FrameStats {
    /// Total frames presented
    pub total_frames: u64,
    /// Frames that missed the deadline (dropped)
    pub dropped_frames: u64,
    /// Average frame time in microseconds
    pub avg_frame_time_us: u32,
    /// Worst frame time in microseconds
    pub worst_frame_time_us: u32,
    /// Best frame time in microseconds
    pub best_frame_time_us: u32,
    /// Current FPS estimate
    pub current_fps: u32,
    /// Average GPU wait time (if hardware VSync)
    pub avg_gpu_wait_us: u32,
}

struct StatsTracker {
    frame_times: [u32; 64], // Circular buffer of frame times (microseconds)
    write_idx: usize,
    total_frames: u64,
    dropped_frames: u64,
    worst_time: u32,
    best_time: u32,
    last_fps_time: u64,
    fps_frame_count: u32,
    current_fps: u32,
}

impl StatsTracker {
    const fn new() -> Self {
        Self {
            frame_times: [0u32; 64],
            write_idx: 0,
            total_frames: 0,
            dropped_frames: 0,
            worst_time: 0,
            best_time: u32::MAX,
            last_fps_time: 0,
            fps_frame_count: 0,
            current_fps: 0,
        }
    }

    fn record_frame(&mut self, frame_time_us: u32, tsc_now: u64) {
        self.frame_times[self.write_idx] = frame_time_us;
        self.write_idx = (self.write_idx + 1) & 63;
        self.total_frames += 1;

        if frame_time_us > self.worst_time {
            self.worst_time = frame_time_us;
        }
        if frame_time_us < self.best_time && frame_time_us > 0 {
            self.best_time = frame_time_us;
        }

        // Check for deadline miss
        let target_us = 1_000_000_000 / TARGET_REFRESH_MHZ.load(Ordering::Relaxed).max(1);
        if frame_time_us > target_us {
            self.dropped_frames += 1;
        }

        // FPS calculation (once per second)
        self.fps_frame_count += 1;
        // Simple estimate: ~2GHz TSC, 1 second ≈ 2_000_000_000 ticks
        let elapsed = tsc_now.saturating_sub(self.last_fps_time);
        if elapsed > 2_000_000_000 {
            self.current_fps = self.fps_frame_count;
            self.fps_frame_count = 0;
            self.last_fps_time = tsc_now;
        }
    }

    fn stats(&self) -> FrameStats {
        let valid_count = self.frame_times.iter().filter(|&&t| t > 0).count();
        let avg = if valid_count > 0 {
            (self.frame_times.iter().map(|&t| t as u64).sum::<u64>() / valid_count as u64) as u32
        } else {
            0
        };
        FrameStats {
            total_frames: self.total_frames,
            dropped_frames: self.dropped_frames,
            avg_frame_time_us: avg,
            worst_frame_time_us: self.worst_time,
            best_frame_time_us: if self.best_time == u32::MAX {
                0
            } else {
                self.best_time
            },
            current_fps: self.current_fps,
            avg_gpu_wait_us: 0,
        }
    }
}

lazy_static::lazy_static! {
    static ref STATS: Mutex<StatsTracker> = Mutex::new(StatsTracker::new());
}

// ═══════════════════════════════════════════════════════════════════════
// FRAME CALLBACK SYSTEM
// ═══════════════════════════════════════════════════════════════════════

/// A pending frame callback (from Wayland clients or internal windows)
#[derive(Debug, Clone, Copy)]
struct FrameCallback {
    /// Window/surface ID requesting the callback
    surface_id: u32,
    /// Callback object ID
    callback_id: u32,
}

lazy_static::lazy_static! {
    static ref PENDING_CALLBACKS: Mutex<Vec<FrameCallback>> = Mutex::new(Vec::new());
}

/// Register a frame callback (called when next frame is presented)
pub fn request_frame_callback(surface_id: u32, callback_id: u32) {
    PENDING_CALLBACKS.lock().push(FrameCallback {
        surface_id,
        callback_id,
    });
}

/// Fire all pending frame callbacks with current timestamp
fn fire_callbacks() {
    let mut callbacks = PENDING_CALLBACKS.lock();
    if callbacks.is_empty() {
        return;
    }
    let _timestamp_ms = crate::clock::uptime_seconds() * 1000;
    // In full implementation: send wl_callback.done(timestamp) to each client
    callbacks.clear();
}

// ═══════════════════════════════════════════════════════════════════════
// DOUBLE BUFFERING / PAGE FLIP
// ═══════════════════════════════════════════════════════════════════════

/// Page flip state for double-buffered rendering
struct PageFlipState {
    /// Addresses of the two framebuffer pages
    front_buffer: usize,
    back_buffer: usize,
    /// Which buffer is currently displayed (0 or 1)
    current_page: u8,
    /// Whether double buffering is active
    enabled: bool,
}

impl PageFlipState {
    const fn new() -> Self {
        Self {
            front_buffer: 0,
            back_buffer: 0,
            current_page: 0,
            enabled: false,
        }
    }
}

lazy_static::lazy_static! {
    static ref PAGE_FLIP: Mutex<PageFlipState> = Mutex::new(PageFlipState::new());
}

/// Initialize double buffering with two framebuffer pages
pub fn init_double_buffer(fb_base: usize, fb_size: usize) {
    let mut pf = PAGE_FLIP.lock();
    pf.front_buffer = fb_base;
    pf.back_buffer = fb_base + fb_size;
    pf.current_page = 0;
    pf.enabled = true;
    crate::serial_println!(
        "[VSync] Double buffering initialized: page0={:#x} page1={:#x}",
        pf.front_buffer,
        pf.back_buffer
    );
}

/// Swap front and back buffers (called at VSync)
pub fn swap_buffers() -> usize {
    let mut pf = PAGE_FLIP.lock();
    if !pf.enabled {
        return pf.front_buffer;
    }
    pf.current_page = 1 - pf.current_page;
    if pf.current_page == 0 {
        pf.front_buffer
    } else {
        pf.back_buffer
    }
}

/// Get the address of the current back buffer (for rendering)
pub fn back_buffer_addr() -> usize {
    let pf = PAGE_FLIP.lock();
    if pf.current_page == 0 {
        pf.back_buffer
    } else {
        pf.front_buffer
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════

/// Initialize VSync subsystem
pub fn init() {
    // Calibrate frame interval from TSC
    // Assume ~2GHz TSC for now; refine later from HPET
    let refresh = TARGET_REFRESH_MHZ.load(Ordering::Relaxed);
    if refresh > 0 {
        // frame_interval = 2_000_000_000 / (refresh / 1000)
        // = 2_000_000_000_000 / refresh
        let interval = 2_000_000_000_000u64 / refresh as u64;
        FRAME_INTERVAL_TICKS.store(interval, Ordering::Relaxed);
    }
    crate::serial_println!(
        "[VSync] Initialized: mode=Software, target={}mHz, interval={} ticks",
        refresh,
        FRAME_INTERVAL_TICKS.load(Ordering::Relaxed)
    );
}

/// Set the VSync mode
pub fn set_mode(mode: VSyncMode) {
    let val = match mode {
        VSyncMode::Off => 0,
        VSyncMode::Software => 1,
        VSyncMode::Hardware => 2,
        VSyncMode::Adaptive => 3,
        VSyncMode::Mailbox => 4,
    };
    VSYNC_MODE.store(val, Ordering::Relaxed);
}

/// Get the current VSync mode
pub fn mode() -> VSyncMode {
    match VSYNC_MODE.load(Ordering::Relaxed) {
        0 => VSyncMode::Off,
        2 => VSyncMode::Hardware,
        3 => VSyncMode::Adaptive,
        4 => VSyncMode::Mailbox,
        _ => VSyncMode::Software,
    }
}

/// Set target refresh rate in millihertz
pub fn set_refresh_rate(mhz: u32) {
    TARGET_REFRESH_MHZ.store(mhz, Ordering::Relaxed);
    if mhz > 0 {
        let interval = 2_000_000_000_000u64 / mhz as u64;
        FRAME_INTERVAL_TICKS.store(interval, Ordering::Relaxed);
    }
}

/// Get target refresh rate in millihertz
pub fn refresh_rate() -> u32 {
    TARGET_REFRESH_MHZ.load(Ordering::Relaxed)
}

/// Check if it's time to present a new frame (frame pacing)
/// Returns true if enough time has elapsed since the last VSync
pub fn should_present() -> bool {
    let mode = VSYNC_MODE.load(Ordering::Relaxed);
    if mode == 0 {
        return true; // VSync off: always present
    }

    let now = crate::gui::read_tsc_public();
    let last = LAST_VSYNC_TSC.load(Ordering::Relaxed);
    let interval = FRAME_INTERVAL_TICKS.load(Ordering::Relaxed);

    if last == 0 {
        // First frame
        LAST_VSYNC_TSC.store(now, Ordering::Relaxed);
        return true;
    }

    let elapsed = now.saturating_sub(last);
    if mode == 3 {
        // Adaptive: present immediately if we're behind schedule
        return elapsed >= interval || elapsed >= interval * 3 / 2;
    }

    elapsed >= interval
}

/// Signal that a frame has been presented
pub fn frame_presented() {
    let now = crate::gui::read_tsc_public();
    let last = LAST_VSYNC_TSC.load(Ordering::Relaxed);
    LAST_VSYNC_TSC.store(now, Ordering::Relaxed);
    VSYNC_FRAME_COUNT.fetch_add(1, Ordering::Relaxed);

    // Record frame timing
    if last > 0 {
        let elapsed_ticks = now.saturating_sub(last);
        // Convert to microseconds (~2GHz TSC → 1 tick ≈ 0.5 ns)
        let elapsed_us = (elapsed_ticks / 2000) as u32;
        STATS.lock().record_frame(elapsed_us, now);
    }

    // Fire Wayland frame callbacks
    fire_callbacks();
}

/// Notify that a hardware page flip completed (called from DRM IRQ handler)
pub fn page_flip_complete() {
    PAGE_FLIP_PENDING.store(false, Ordering::Relaxed);
    frame_presented();
}

/// Check if a page flip is pending
pub fn is_page_flip_pending() -> bool {
    PAGE_FLIP_PENDING.load(Ordering::Relaxed)
}

/// Get frame statistics
pub fn stats() -> FrameStats {
    STATS.lock().stats()
}

/// Get total frame count
pub fn frame_count() -> u64 {
    VSYNC_FRAME_COUNT.load(Ordering::Relaxed)
}

/// Enable hardware VSync (called when DRM driver supports page flip events)
pub fn enable_hw_vsync() {
    HW_VSYNC_AVAILABLE.store(true, Ordering::Relaxed);
    VSYNC_MODE.store(2, Ordering::Relaxed);
    crate::serial_println!("[VSync] Hardware VSync enabled");
}

/// Enable variable refresh rate (FreeSync/G-Sync)
pub fn enable_vrr() {
    VRR_SUPPORTED.store(true, Ordering::Relaxed);
    crate::serial_println!("[VSync] Variable refresh rate (VRR) enabled");
}

/// Check if hardware VSync is available
pub fn is_hw_vsync_available() -> bool {
    HW_VSYNC_AVAILABLE.load(Ordering::Relaxed)
}

/// Check if VRR is supported
pub fn is_vrr_supported() -> bool {
    VRR_SUPPORTED.load(Ordering::Relaxed)
}
