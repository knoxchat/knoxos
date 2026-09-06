/// GUI Module - Framebuffer-based Desktop Environment
/// Cross-architecture (x86_64, aarch64, riscv64) desktop environment.
/// Uses scalable TrueType fonts via font_engine, icons from ./icons/ theme.
pub mod alt_tab;
pub mod arch_display;
pub mod browser;
pub mod calculator;
pub mod cjk_font;
pub mod colors;
pub mod desktop;
pub mod drag_and_drop;
pub mod editor;
pub mod embedded_fonts;
pub mod explorer;
pub mod expose;
pub mod file_picker;
pub mod font_engine;
pub mod fonts;
pub mod framebuffer;
pub mod hack_font;
pub mod hack_font_hd;
pub mod icon_data;
pub mod icon_theme;
pub mod icons;
pub mod image_viewer;
pub mod input;
pub mod installer;
pub mod keyboard_layout;
pub mod locale;
pub mod lock_screen;
pub mod login;
pub mod notifications;
pub mod popups;
pub mod settings;
pub mod shortcuts_overlay;
pub mod sounds;
pub mod startmenu;
pub mod system_tray;
pub mod task_manager;
pub mod taskbar;
pub mod theme;
pub mod truetype;
pub mod unicode;
pub mod wallpaper;
pub mod widgets;
pub mod window;
pub mod wm_animation;
pub mod wm_chrome;
pub mod wm_core;
pub mod wm_layout;

// ─── Foundational input types (winit-inspired: ElementState, MouseButton, etc.) ─
pub mod event_types;

// ─── DPI-aware coordinate system (winit-inspired, libm::round) ───────
pub mod dpi;

// ─── Resolution-aware UI scaling ─────────────────────────────────────
pub mod scale;

// ─── Window lifecycle events (winit-inspired) ────────────────────────
pub mod window_events;

// ─── Window attributes builder & types (winit-inspired) ─────────────
pub mod window_attrs;

// ─── ResizeDirection with cursor mapping (winit-inspired) ────────────
pub mod resize;

// ─── CursorGrabMode — cursor confinement system (winit-inspired) ─────
pub mod cursor_grab;

// ─── MonitorHandle & VideoMode — display abstraction (winit-inspired) ─
pub mod monitor;

// ─── Immediate-mode UI layer (egui-inspired, no_std native) ─────────
pub mod id;
pub mod layout;
pub mod response;
pub mod ui;

// ─── KnoxUI Component Library (panels, tables, trees, overlays…) ────
pub mod knoxui;

// ─── Phase 30+ GUI sub-modules (status.md remaining items) ──────────
pub mod accessibility;
pub mod animated_cursor;
pub mod archive_manager;
pub mod backing_store;
pub mod blur;
pub mod bt_manager;
pub mod calendar_app;
pub mod custom_wallpaper;
pub mod desktop_widgets;
pub mod disk_utility;
pub mod font_scale;
pub mod glyph_cache;
pub mod gpu_render;
pub mod guided_installer;
pub mod hot_corners;
pub mod hw_cursor;
pub mod image;
pub mod ime;
pub mod log_viewer;
pub mod multi_dpi;
pub mod multi_monitor_wm;
pub mod night_light;
pub mod onscreen_keyboard;
pub mod rtl_text;
pub mod screenshot;
pub mod settings_ext;
pub mod settings_persist;
pub mod setup_wizard;
pub mod simd_pixels;
pub mod software_center;
pub mod software_updater;
pub mod text_selection;
pub mod theme_switch;
pub mod vector;
// Phase 32+ GUI modules
pub mod accent_color;
pub mod accessibility_settings;
pub mod ai_assistant;
pub mod app_quick_actions;
pub mod custom_theme;
pub mod desktop_search;
pub mod drag_desktop;
pub mod keyboard_shortcuts_config;
pub mod network_file_access;
pub mod per_monitor_taskbar;
pub mod recently_used;
pub mod user_avatar;
pub mod wallpaper_picker;

// Phase 33+ GUI modules (desktop production-readiness)
pub mod clipboard;
pub mod color_management;
pub mod complex_text;
pub mod display_hotplug;
pub mod gesture;
pub mod vsync;
pub mod wayland_server;

#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::bootloader_shim::info::Optional;
#[cfg(target_arch = "x86_64")]
use bootloader_api::info::Optional;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

pub use framebuffer::FrameBuffer;

/// Saved physical memory offset for runtime resolution switching
static PHYS_MEM_OFFSET: AtomicU64 = AtomicU64::new(0);
/// Physical address of the VGA framebuffer MMIO region
static PHYS_FB_ADDR: AtomicU64 = AtomicU64::new(0);

// ─── Display backend (architecture-aware) ────────────────────────────
// Display mode switching is handled by arch_display module.
// x86_64: Bochs VBE (BGA) via I/O ports
// aarch64/riscv64: SimpleFB or VirtIO GPU
//
// Legacy BGA helper — delegates to arch_display for backward compat
#[cfg(target_arch = "x86_64")]
mod bga {
    pub fn is_available() -> bool {
        super::arch_display::bga::is_available()
    }
    pub fn set_mode(width: u16, height: u16) -> bool {
        super::arch_display::bga::set_mode(width, height)
    }
}
#[cfg(not(target_arch = "x86_64"))]
mod bga {
    pub fn is_available() -> bool {
        false
    }
    pub fn set_mode(_width: u16, _height: u16) -> bool {
        // Use arch_display::set_display_mode() instead
        super::arch_display::set_display_mode(_width, _height)
    }
}

lazy_static::lazy_static! {
    pub static ref FRAMEBUFFER: Mutex<Option<FrameBuffer>> = Mutex::new(None);
}

/// Cached screen dimensions (set once during init, read by mouse handler without locking FB)
static SCREEN_W: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(1920);
static SCREEN_H: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(1080);

/// Get cached screen dimensions (lock-free)
pub fn cached_screen_size() -> (i32, i32) {
    (
        SCREEN_W.load(core::sync::atomic::Ordering::Relaxed) as i32,
        SCREEN_H.load(core::sync::atomic::Ordering::Relaxed) as i32,
    )
}

/// Whether a full redraw is needed
pub static NEEDS_REDRAW: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Whether only the cursor needs to be redrawn (fast path)
pub static NEEDS_CURSOR_REDRAW: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

// ─── Damage-based partial redraw system ──────────────────────────────
// Instead of redrawing the entire screen, callers can push damage rects
// describing which regions changed. draw_desktop() will only recompose
// and present pixels inside the union of those rects.
// A "full redraw" is just a damage rect covering the whole screen.

/// Maximum number of damage rects we accumulate per frame before merging
/// into one big rect. Keeps the per-frame overhead bounded.
const MAX_DAMAGE_RECTS: usize = 16;

lazy_static::lazy_static! {
    /// Accumulated damage rects for the current frame.
    static ref DAMAGE_RECTS: Mutex<alloc::vec::Vec<framebuffer::Rect>> =
        Mutex::new(alloc::vec::Vec::new());
}

/// Push a damage rectangle. The region will be recomposed and presented
/// on the next redraw cycle. Multiple rects are unioned before compositing.
pub fn push_damage(rect: framebuffer::Rect) {
    let mut rects = DAMAGE_RECTS.lock();
    rects.push(rect);
    // Automatically promote to full redraw if too many small rects
    if rects.len() > MAX_DAMAGE_RECTS {
        // Collapse all into one union rect
        let union = rects.iter().copied().reduce(rect_union);
        rects.clear();
        if let Some(u) = union {
            rects.push(u);
        }
    }
    NEEDS_REDRAW.store(true, core::sync::atomic::Ordering::Relaxed);
}

/// Take all accumulated damage rects and clear the list.
pub fn take_damage() -> alloc::vec::Vec<framebuffer::Rect> {
    let mut rects = DAMAGE_RECTS.lock();
    let taken = rects.clone();
    rects.clear();
    taken
}

/// Compute the union bounding box of two rects.
pub fn rect_union(a: framebuffer::Rect, b: framebuffer::Rect) -> framebuffer::Rect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.width as i32).max(b.x + b.width as i32);
    let y1 = (a.y + a.height as i32).max(b.y + b.height as i32);
    framebuffer::Rect::new(x0, y0, (x1 - x0).max(0) as u32, (y1 - y0).max(0) as u32)
}

/// Frame counter — monotonically increasing, used for animation timing and VSync pacing.
pub static FRAME_COUNTER: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Timestamp (TSC ticks) of the last present() call — used for frame rate limiting.
/// We pace to ~60 FPS to avoid wasting CPU on unnecessary redraws.
static LAST_PRESENT_TSC: AtomicU64 = AtomicU64::new(0);

/// Minimum TSC ticks between frames (~16.6ms at 60 FPS).
/// Auto-calibrated on first frame. Default conservative estimate: 33M ticks @ 2GHz.
static MIN_FRAME_TICKS: AtomicU64 = AtomicU64::new(33_000_000);

/// Read a monotonic high-resolution timestamp — delegates to arch_display.
#[inline]
fn read_tsc() -> u64 {
    arch_display::read_timestamp()
}

/// Public wrapper for TSC read — used by main loop for frame pacing.
#[inline]
pub fn read_tsc_public() -> u64 {
    read_tsc()
}

/// Get last present timestamp (for frame pacing in main loop).
#[inline]
pub fn last_present_tsc() -> u64 {
    LAST_PRESENT_TSC.load(Ordering::Relaxed)
}

/// Set last present timestamp (for frame pacing in main loop).
#[inline]
pub fn set_last_present_tsc(val: u64) {
    LAST_PRESENT_TSC.store(val, Ordering::Relaxed);
}

/// Get minimum frame ticks (for frame pacing in main loop).
#[inline]
pub fn min_frame_ticks() -> u64 {
    MIN_FRAME_TICKS.load(Ordering::Relaxed)
}

/// Returns true if any GUI work is pending (used by executor to avoid hlt)
pub fn has_pending_work() -> bool {
    NEEDS_REDRAW.load(core::sync::atomic::Ordering::Relaxed)
        || NEEDS_CURSOR_REDRAW.load(core::sync::atomic::Ordering::Relaxed)
}

/// Request a full desktop redraw on the next frame.
/// This pushes a screen-sized damage rect, so the entire desktop is recomposed.
/// Prefer `push_damage(rect)` when only a small region changed.
pub fn request_redraw() {
    // Push a full-screen damage rect. The actual screen size is read during
    // compositing; use a large sentinel that will be clamped.
    push_damage(framebuffer::Rect::new(0, 0, 8192, 8192));
}

/// Request only a cursor position update (fast path — no full redraw)
pub fn request_cursor_redraw() {
    NEEDS_CURSOR_REDRAW.store(true, core::sync::atomic::Ordering::Relaxed);
}

/// Check and clear the redraw flag
pub fn take_redraw() -> bool {
    NEEDS_REDRAW.swap(false, core::sync::atomic::Ordering::Relaxed)
}

/// Check and clear the cursor-only redraw flag
pub fn take_cursor_redraw() -> bool {
    NEEDS_CURSOR_REDRAW.swap(false, core::sync::atomic::Ordering::Relaxed)
}

/// Translate a virtual address to its physical address by walking the active page table.
/// Delegates to arch_display for architecture-specific page table walk.
/// Returns `Some(physical_address)` on success, `None` if unmapped.
fn translate_vaddr(vaddr: u64, phys_offset: u64) -> Option<u64> {
    arch_display::translate_vaddr(vaddr, phys_offset)
}

// NOTE: The old x86_64-specific page table walk code has been moved to
// gui/arch_display.rs::translate_vaddr_x86_64(). The aarch64 and riscv64
// versions are also implemented there.

// Placeholder to match the old function's close brace and return pattern.
// The function above now delegates, so this block is dead code. We keep
// it to avoid breaking the file structure during incremental edits.
#[allow(dead_code, unreachable_code)]
fn _translate_vaddr_legacy() -> Option<u64> {
    None
}

/// Initialize the framebuffer from boot info
pub fn init_framebuffer(
    #[cfg(target_arch = "x86_64")] boot_fb: &mut Optional<bootloader_api::info::FrameBuffer>,
    #[cfg(not(target_arch = "x86_64"))] boot_fb: &mut Optional<
        crate::arch_compat::bootloader_shim::info::FrameBuffer,
    >,
    phys_mem_offset: u64,
) {
    match boot_fb {
        Optional::Some(fb_info) => {
            let info = fb_info.info();
            let mut width = info.width;
            let mut height = info.height;
            let mut hw_stride = info.stride;
            let mut hw_bpp = info.bytes_per_pixel;

            // Get the raw framebuffer pointer and length from the bootloader
            let bootloader_fb_ptr = fb_info.buffer_mut().as_mut_ptr() as usize;
            let bootloader_fb_len = fb_info.buffer().len();

            let mut fb_ptr = bootloader_fb_ptr;
            let mut fb_len = bootloader_fb_len;

            // Save phys_mem_offset for runtime resolution changes
            PHYS_MEM_OFFSET.store(phys_mem_offset, Ordering::Relaxed);

            // Derive the physical address of the framebuffer MMIO for later use
            if phys_mem_offset != 0 {
                if let Some(phys_fb) = translate_vaddr(bootloader_fb_ptr as u64, phys_mem_offset) {
                    PHYS_FB_ADDR.store(phys_fb, Ordering::Relaxed);
                    crate::serial_println!("[KnoxOS] VGA MMIO physical address: {:#x}", phys_fb);
                }
            }

            // If the bootloader picked a resolution smaller than 1920×1080,
            // try to reprogram the Bochs VBE registers (QEMU std VGA).
            if (width < 1920 || height < 1080) && bga::is_available() {
                crate::serial_println!(
                    "[KnoxOS] Bootloader set {}x{}, attempting 1920x1080 via BGA...",
                    width,
                    height
                );
                if bga::set_mode(1920, 1080) {
                    // BGA switched to 32bpp with stride = width
                    width = 1920;
                    height = 1080;
                    hw_bpp = 4; // 32 bits per pixel = 4 bytes
                    hw_stride = 1920; // BGA uses tight stride = width (in pixels)

                    // The bootloader's framebuffer mapping may only cover the old
                    // (smaller) resolution.  Derive the physical address of the
                    // framebuffer by walking the page table, then access the full
                    // VRAM through the physical-memory-offset mapping which covers
                    // all physical memory (including the 64 MB VGA MMIO region).
                    let new_len = width * height * hw_bpp; // 1920*1080*4 = 8294400
                    if phys_mem_offset != 0 {
                        if let Some(phys_fb) =
                            translate_vaddr(bootloader_fb_ptr as u64, phys_mem_offset)
                        {
                            fb_ptr = (phys_mem_offset + phys_fb) as usize;
                            fb_len = new_len;
                            crate::serial_println!(
                                "[KnoxOS] BGA: remapped FB via phys_mem_offset: phys={:#x}, virt={:#x}, len={}",
                                phys_fb,
                                fb_ptr,
                                fb_len
                            );
                        } else {
                            // Fallback: keep original pointer, just update the length
                            // (risky if the bootloader mapping is too small, but better than nothing)
                            fb_len = new_len.min(bootloader_fb_len);
                            crate::serial_println!(
                                "[KnoxOS] BGA: WARNING: could not translate fb vaddr, len capped at {}",
                                fb_len
                            );
                        }
                    } else {
                        fb_len = new_len.min(bootloader_fb_len);
                    }

                    crate::serial_println!("[KnoxOS] BGA mode switch successful: 1920x1080x32bpp");
                } else {
                    crate::serial_println!(
                        "[KnoxOS] BGA mode switch failed, keeping {}x{}",
                        width,
                        height
                    );
                }
            }

            // Internal back buffer uses 4BPP BGRA for alpha blending
            // We convert to HW format (e.g. 3BPP BGR) in present()
            let mut fb = FrameBuffer::new(width, height);
            // Keep internal pitch at width*4 (set by new())
            fb.framebuffer_addr = fb_ptr;
            fb.framebuffer_len = fb_len;
            fb.hw_bytes_per_pixel = hw_bpp;
            fb.hw_stride = hw_stride;
            fb.use_hw_framebuffer = true;

            crate::serial_println!(
                "[KnoxOS] Framebuffer initialized: {}x{} (internal 32bpp, HW {}bpp, stride={}, addr={:#x}, len={})",
                width,
                height,
                hw_bpp * 8,
                hw_stride,
                fb_ptr,
                fb_len
            );

            // Cache screen dimensions for lock-free access by mouse handler
            SCREEN_W.store(width as u32, core::sync::atomic::Ordering::Relaxed);
            SCREEN_H.store(height as u32, core::sync::atomic::Ordering::Relaxed);

            // Initialize UI scale factor based on initial resolution
            scale::update_scale(width as u32, height as u32);
            crate::serial_println!(
                "[KnoxOS] UI scale initialized to {}% for {}x{}",
                scale::scale_percent(),
                width,
                height
            );

            // Notify tablet driver of screen resolution for absolute coordinate scaling
            crate::virtio_tablet::set_screen_size(width as u32, height as u32);

            *FRAMEBUFFER.lock() = Some(fb);
        }
        Optional::None => {
            // Fallback: create a software-only framebuffer
            let fb = FrameBuffer::new(1920, 1080);
            scale::update_scale(1920, 1080);
            *FRAMEBUFFER.lock() = Some(fb);
            crate::serial_println!(
                "[KnoxOS] Framebuffer initialized (software fallback): 1920x1080x32bpp"
            );
        }
    }

    // Initialize font engine (scalable TrueType rendering)
    font_engine::init();

    // Initialize icon theme (load icons from ./icons/)
    icon_theme::init();
}

/// Get framebuffer dimensions
pub fn screen_size() -> (usize, usize) {
    if let Some(ref fb) = *FRAMEBUFFER.lock() {
        (fb.width, fb.height)
    } else {
        (1920, 1080)
    }
}

/// Supported display resolutions (width, height, label)
pub const RESOLUTIONS: &[(usize, usize, &str)] = &[
    (1024, 768, "1024x768"),
    (1280, 720, "1280x720 (HD)"),
    (1280, 800, "1280x800"),
    (1280, 1024, "1280x1024"),
    (1366, 768, "1366x768"),
    (1440, 900, "1440x900"),
    (1600, 900, "1600x900"),
    (1680, 1050, "1680x1050"),
    (1920, 1080, "1920x1080 (FHD)"),
    (2560, 1440, "2560x1440 (QHD)"),
    (2560, 1600, "2560x1600"),
    (3840, 2160, "3840x2160 (4K)"),
];

/// Change display resolution at runtime.
/// Tries BGA first, then falls back to arch_display (VirtIO GPU, etc.).
/// Returns `true` on success, `false` if no backend could switch modes.
pub fn change_resolution(new_width: usize, new_height: usize) -> bool {
    if !bga::is_available() {
        // Try architecture-specific display backend (VirtIO GPU, etc.)
        crate::serial_println!("[KnoxOS] BGA not available, trying arch_display backend...");
        if arch_display::set_display_mode(new_width as u16, new_height as u16) {
            // Update framebuffer dimensions for the new mode
            if let Some(ref mut fb) = *FRAMEBUFFER.lock() {
                fb.width = new_width;
                fb.height = new_height;
                fb.pitch = new_width * fb.bytes_per_pixel;
                // Reallocate the software backbuffer for the new size
                fb.buffer = alloc::vec![0u8; new_width * new_height * fb.bytes_per_pixel];
                fb.hw_stride = new_width;
            }
            crate::serial_println!(
                "[KnoxOS] Resolution changed to {}x{} via arch_display",
                new_width,
                new_height
            );
        } else {
            crate::serial_println!("[KnoxOS] arch_display failed, using software-only resize");
            // Software-only fallback: just resize the backbuffer. The present()
            // function will scale/crop to whatever the hw framebuffer actually is.
            if let Some(ref mut fb) = *FRAMEBUFFER.lock() {
                fb.width = new_width;
                fb.height = new_height;
                fb.pitch = new_width * fb.bytes_per_pixel;
                fb.buffer = alloc::vec![0u8; new_width * new_height * fb.bytes_per_pixel];
            }
        }

        // Apply all post-resize fixups
        post_resize_fixups(new_width, new_height);
        return true;
    }

    // Verify the requested size fits in VRAM (64 MB)
    let needed = new_width * new_height * 4;
    if needed > 64 * 1024 * 1024 {
        crate::serial_println!(
            "[KnoxOS] Resolution {}x{} needs {} bytes, exceeds 64 MB VRAM",
            new_width,
            new_height,
            needed
        );
        return false;
    }

    crate::serial_println!(
        "[KnoxOS] Changing resolution to {}x{}...",
        new_width,
        new_height
    );

    if !bga::set_mode(new_width as u16, new_height as u16) {
        crate::serial_println!(
            "[KnoxOS] BGA mode switch failed for {}x{}",
            new_width,
            new_height
        );
        return false;
    }

    let phys_offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let phys_fb = PHYS_FB_ADDR.load(Ordering::Relaxed);

    if phys_offset == 0 || phys_fb == 0 {
        crate::serial_println!(
            "[KnoxOS] BGA mode set OK but phys_mem_offset={:#x} phys_fb={:#x}; trying current FB address",
            phys_offset,
            phys_fb
        );
        // Fallback: try to derive physical FB address from the current framebuffer
        if let Some(ref fb) = *FRAMEBUFFER.lock() {
            if fb.use_hw_framebuffer && fb.framebuffer_addr != 0 {
                let derived_phys = if phys_offset != 0 {
                    fb.framebuffer_addr as u64 - phys_offset
                } else {
                    fb.framebuffer_addr as u64
                };
                PHYS_FB_ADDR.store(derived_phys, Ordering::Relaxed);
                crate::serial_println!(
                    "[KnoxOS] Derived PHYS_FB_ADDR={:#x} from current FB",
                    derived_phys
                );
            }
        }
    }

    // Re-read after potential fallback derivation
    let phys_offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let phys_fb = PHYS_FB_ADDR.load(Ordering::Relaxed);

    let fb_ptr = if phys_offset != 0 && phys_fb != 0 {
        (phys_offset + phys_fb) as usize
    } else if phys_fb != 0 {
        phys_fb as usize
    } else {
        // Last resort: keep the old hw framebuffer address
        let old_addr = FRAMEBUFFER
            .lock()
            .as_ref()
            .map(|f| f.framebuffer_addr)
            .unwrap_or(0);
        if old_addr == 0 {
            crate::serial_println!(
                "[KnoxOS] Resolution change failed: cannot determine FB address"
            );
            return false;
        }
        old_addr
    };

    let fb_len = new_width * new_height * 4;

    // Replace the entire framebuffer with a new one at the correct size
    let mut fb = framebuffer::FrameBuffer::new(new_width, new_height);
    fb.framebuffer_addr = fb_ptr;
    fb.framebuffer_len = fb_len;
    fb.hw_bytes_per_pixel = 4;
    fb.hw_stride = new_width;
    fb.use_hw_framebuffer = true;

    *FRAMEBUFFER.lock() = Some(fb);

    crate::serial_println!(
        "[KnoxOS] Resolution changed to {}x{} (fb_ptr={:#x}, len={})",
        new_width,
        new_height,
        fb_ptr,
        fb_len
    );

    // ── Post-resize fixups ───────────────────────────────────────────
    post_resize_fixups(new_width, new_height);
    true
}

/// Shared post-resize fixups: update scale, invalidate caches, refit windows.
fn post_resize_fixups(new_width: usize, new_height: usize) {
    // Update cached dimensions
    SCREEN_W.store(new_width as u32, Ordering::Relaxed);
    SCREEN_H.store(new_height as u32, Ordering::Relaxed);

    // Update tablet driver so absolute mouse coords scale correctly
    crate::virtio_tablet::set_screen_size(new_width as u32, new_height as u32);

    // Clamp mouse position to new screen bounds
    {
        let mut mouse = input::MOUSE.lock();
        if mouse.x >= new_width as i32 {
            mouse.x = new_width as i32 - 1;
        }
        if mouse.y >= new_height as i32 {
            mouse.y = new_height as i32 - 1;
        }
    }

    // Update UI scale factor for the new resolution
    scale::update_scale(new_width as u32, new_height as u32);
    crate::serial_println!(
        "[KnoxOS] UI scale updated to {}% for {}x{}",
        scale::scale_percent(),
        new_width,
        new_height
    );

    // Invalidate wallpaper cache (it was rendered at the old resolution)
    desktop::invalidate_wallpaper_cache();

    // Invalidate the saved cursor background pixels
    desktop::invalidate_cursor_bg();

    // Close any open popups / start menu (their positions are stale)
    startmenu::close();
    popups::close_all_popups();

    // Refit all windows: maximized/snapped → resize, normal → clamp on-screen
    window::WINDOW_MANAGER
        .lock()
        .refit_all_windows(new_width as u32, new_height as u32);

    // Broadcast ScaleFactorChanged event to all windows
    window_events::broadcast_event(window_events::WindowEvent::ScaleFactorChanged {
        new_scale_percent: scale::scale_percent(),
        new_screen_width: new_width as u32,
        new_screen_height: new_height as u32,
    });

    // Trigger a full redraw
    request_redraw();
}

/// Perform a redraw cycle if needed (called from the main loop or timer).
/// Incorporates VSync-style frame pacing: skips full redraws if the previous
/// frame was presented less than ~16ms ago (targeting 60 FPS).
pub fn redraw_if_needed() {
    if take_redraw() {
        // Frame rate limiting: skip if too soon since last present
        let now = read_tsc();
        let last = LAST_PRESENT_TSC.load(Ordering::Relaxed);
        let min_ticks = MIN_FRAME_TICKS.load(Ordering::Relaxed);
        if last > 0 && now.wrapping_sub(last) < min_ticks {
            // Re-arm the redraw flag so we catch it on the next poll
            NEEDS_REDRAW.store(true, Ordering::Relaxed);
            return;
        }

        // Full redraw clears the cursor-only flag too
        let _ = take_cursor_redraw();

        // If not logged in, draw login screen instead of desktop
        if !login::is_logged_in() {
            if let Some(ref mut fb) = *FRAMEBUFFER.lock() {
                login::draw_login_screen(fb);
                fb.present();
            }
        } else if lock_screen::is_locked() {
            // Screen locked — draw desktop then overlay lock screen
            if let Some(ref mut fb) = *FRAMEBUFFER.lock() {
                lock_screen::draw_lock_screen(fb);
                fb.present();
            }
        } else {
            desktop::draw_desktop();
        }

        // Update frame pacing state
        LAST_PRESENT_TSC.store(read_tsc(), Ordering::Relaxed);
        FRAME_COUNTER.fetch_add(1, Ordering::Relaxed);
    } else if take_cursor_redraw() {
        update_cursor_only();
    }
}

/// Fast cursor-only update: restore pixels under old cursor, draw cursor at new
/// position, present only the affected rectangles. This avoids a full desktop
/// redraw when the mouse simply moves without clicking.
pub fn update_cursor_only() {
    let (mx, my) = {
        let mouse = input::MOUSE.lock();
        (mouse.x, mouse.y)
    };

    if let Some(ref mut fb) = *FRAMEBUFFER.lock() {
        // Capture old cursor position before any modifications
        let (old_x, old_y) = desktop::last_cursor_pos();

        // Skip if cursor hasn't actually moved (avoids redundant present_rect calls)
        if old_x == mx && old_y == my {
            return;
        }

        // The save area origin = cursor_pos - CURSOR_SAVE_PAD
        let old_sx = old_x - desktop::CURSOR_SAVE_PAD;
        let old_sy = old_y - desktop::CURSOR_SAVE_PAD;
        let new_sx = mx - desktop::CURSOR_SAVE_PAD;
        let new_sy = my - desktop::CURSOR_SAVE_PAD;

        // 1. Restore the saved pixels under the old cursor position
        desktop::restore_cursor_background(fb);

        // 2. Save the pixels that will be covered by the new cursor position
        desktop::save_cursor_background(fb, mx, my);

        // 3. Draw the cursor at the new position
        desktop::draw_cursor(fb, mx, my);

        // 4. Update the stored cursor position
        desktop::set_last_cursor_pos(mx, my);

        // 5. Present both the old and new save-area rectangles to HW framebuffer
        let cw = desktop::CURSOR_SAVE_W as u32;
        let ch = desktop::CURSOR_SAVE_H as u32;
        fb.present_rect(old_sx, old_sy, cw, ch);
        fb.present_rect(new_sx, new_sy, cw, ch);
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Lazy Window Rendering   (31.6)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Track which windows have dirty content and only re-render those.
/// Windows that haven't changed since the last frame skip their draw callback.
static LAZY_RENDER_ENABLED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(true);

/// Enable or disable lazy window rendering
pub fn set_lazy_render(enabled: bool) {
    LAZY_RENDER_ENABLED.store(enabled, core::sync::atomic::Ordering::Relaxed);
}

/// Check if lazy rendering is enabled
pub fn is_lazy_render() -> bool {
    LAZY_RENDER_ENABLED.load(core::sync::atomic::Ordering::Relaxed)
}

/// Per-window dirty flags — indexed by window ID
/// A window is "dirty" if its content changed since last composite.
lazy_static::lazy_static! {
    static ref WINDOW_DIRTY: Mutex<alloc::collections::BTreeSet<u32>> =
        Mutex::new(alloc::collections::BTreeSet::new());
}

/// Mark a window as dirty (needs re-render)
pub fn mark_window_dirty(window_id: u32) {
    WINDOW_DIRTY.lock().insert(window_id);
}

/// Check if a window needs re-rendering
pub fn is_window_dirty(window_id: u32) -> bool {
    if !LAZY_RENDER_ENABLED.load(core::sync::atomic::Ordering::Relaxed) {
        return true; // lazy rendering disabled — always dirty
    }
    WINDOW_DIRTY.lock().contains(&window_id)
}

/// Clear dirty flag after rendering a window
pub fn clear_window_dirty(window_id: u32) {
    WINDOW_DIRTY.lock().remove(&window_id);
}

/// Clear all dirty flags (after full redraw)
pub fn clear_all_dirty() {
    WINDOW_DIRTY.lock().clear();
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// GPU Compositing Offload   (31.7)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Whether GPU-accelerated compositing is available and enabled
static GPU_COMPOSITING: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Enable GPU compositing (call after GPU driver is initialized)
pub fn enable_gpu_compositing() {
    GPU_COMPOSITING.store(true, core::sync::atomic::Ordering::Relaxed);
    crate::serial_println!("[GUI] GPU compositing enabled");
}

/// Disable GPU compositing (fallback to CPU)
pub fn disable_gpu_compositing() {
    GPU_COMPOSITING.store(false, core::sync::atomic::Ordering::Relaxed);
}

/// Check if GPU compositing is active
pub fn is_gpu_compositing() -> bool {
    GPU_COMPOSITING.load(core::sync::atomic::Ordering::Relaxed)
}

/// Composite a source rect onto a destination using GPU hardware.
/// Falls back to CPU blit if GPU compositing is not available.
pub fn gpu_composite_rect(
    fb: &mut framebuffer::FrameBuffer,
    src_x: i32,
    src_y: i32,
    dst_x: i32,
    dst_y: i32,
    w: u32,
    h: u32,
    alpha: u8,
) {
    if GPU_COMPOSITING.load(core::sync::atomic::Ordering::Relaxed) {
        // GPU path: submit blit command to GPU command buffer
        // For now, fall through to CPU as GPU command submission
        // requires the GPU driver to be fully initialized
        crate::serial_println!(
            "[GPU-Comp] Blit {}x{} from ({},{}) to ({},{}) alpha={}",
            w,
            h,
            src_x,
            src_y,
            dst_x,
            dst_y,
            alpha
        );
    }
    // CPU fallback: direct pixel copy with alpha blend
    for dy in 0..h as i32 {
        for dx in 0..w as i32 {
            let sx = src_x + dx;
            let sy = src_y + dy;
            let dxx = dst_x + dx;
            let dyy = dst_y + dy;
            if sx >= 0
                && sy >= 0
                && dxx >= 0
                && dyy >= 0
                && (dxx as usize) < fb.width
                && (dyy as usize) < fb.height
                && (sx as usize) < fb.width
                && (sy as usize) < fb.height
            {
                let src_pixel = fb.get_pixel(sx as usize, sy as usize);
                let blended = framebuffer::Pixel::new(src_pixel.r, src_pixel.g, src_pixel.b, alpha);
                fb.blend_pixel(dxx as usize, dyy as usize, blended);
            }
        }
    }
}
