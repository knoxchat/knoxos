/// Screenshot tool — capture full screen or region to PNG-like image file
/// Supports full screen, active window, and custom region capture modes.
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use super::framebuffer::Pixel;

/// Screenshot capture mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CaptureMode {
    /// Capture the entire screen
    FullScreen,
    /// Capture a specific region (user draws a selection rectangle)
    Region,
    /// Capture the currently focused window
    ActiveWindow,
}

/// State for region selection (rubber-band rectangle)
#[derive(Debug, Clone, Copy)]
pub struct RegionSelection {
    pub active: bool,
    pub start_x: i32,
    pub start_y: i32,
    pub end_x: i32,
    pub end_y: i32,
}

/// Screenshot result
#[derive(Clone)]
pub struct CapturedScreenshot {
    pub width: u32,
    pub height: u32,
    /// RGBA pixel data (packed u32: 0xAARRGGBB)
    pub pixels: Vec<u32>,
    pub timestamp: u64,
}

/// Screenshot tool state
pub struct ScreenshotTool {
    pub mode: CaptureMode,
    pub region: RegionSelection,
    pub last_screenshot: Option<CapturedScreenshot>,
    /// Countdown timer (ticks remaining, 0 = capture now)
    pub delay_ticks: u64,
    /// Whether a capture is pending (waiting for delay)
    pub capture_pending: bool,
    /// Copy to clipboard instead of saving to file
    pub copy_to_clipboard: bool,
    /// Total screenshots taken this session
    pub capture_count: u32,
}

lazy_static::lazy_static! {
    pub static ref SCREENSHOT: Mutex<ScreenshotTool> = Mutex::new(ScreenshotTool {
        mode: CaptureMode::FullScreen,
        region: RegionSelection {
            active: false,
            start_x: 0,
            start_y: 0,
            end_x: 0,
            end_y: 0,
        },
        last_screenshot: None,
        delay_ticks: 0,
        capture_pending: false,
        copy_to_clipboard: false,
        capture_count: 0,
    });
}

static SELECTING_REGION: AtomicBool = AtomicBool::new(false);

/// Check if the user is currently selecting a region
pub fn is_selecting_region() -> bool {
    SELECTING_REGION.load(Ordering::Relaxed)
}

/// Initiate a screenshot capture
pub fn take_screenshot(mode: CaptureMode) {
    let mut tool = SCREENSHOT.lock();
    tool.mode = mode;

    match mode {
        CaptureMode::FullScreen | CaptureMode::ActiveWindow => {
            // Capture immediately (or after delay)
            if tool.delay_ticks > 0 {
                tool.capture_pending = true;
            } else {
                drop(tool);
                do_capture();
            }
        }
        CaptureMode::Region => {
            // Enter region selection mode
            tool.region.active = true;
            tool.region.start_x = 0;
            tool.region.start_y = 0;
            tool.region.end_x = 0;
            tool.region.end_y = 0;
            SELECTING_REGION.store(true, Ordering::Relaxed);
        }
    }
}

/// Quick screenshot (full screen, no delay)
pub fn take_fullscreen() {
    take_screenshot(CaptureMode::FullScreen);
}

/// Handle mouse down during region selection
pub fn region_mouse_down(x: i32, y: i32) {
    let mut tool = SCREENSHOT.lock();
    if tool.region.active {
        tool.region.start_x = x;
        tool.region.start_y = y;
        tool.region.end_x = x;
        tool.region.end_y = y;
    }
}

/// Handle mouse move during region selection
pub fn region_mouse_move(x: i32, y: i32) {
    let mut tool = SCREENSHOT.lock();
    if tool.region.active {
        tool.region.end_x = x;
        tool.region.end_y = y;
    }
}

/// Handle mouse up — finalize region and capture
pub fn region_mouse_up(x: i32, y: i32) {
    let mut tool = SCREENSHOT.lock();
    if tool.region.active {
        tool.region.end_x = x;
        tool.region.end_y = y;
        tool.region.active = false;
        SELECTING_REGION.store(false, Ordering::Relaxed);
        drop(tool);
        do_capture();
    }
}

/// Cancel region selection
pub fn cancel_region_selection() {
    let mut tool = SCREENSHOT.lock();
    tool.region.active = false;
    SELECTING_REGION.store(false, Ordering::Relaxed);
}

/// Tick — handle delayed captures
pub fn tick() {
    let mut tool = SCREENSHOT.lock();
    if tool.capture_pending && tool.delay_ticks > 0 {
        tool.delay_ticks -= 1;
        if tool.delay_ticks == 0 {
            tool.capture_pending = false;
            drop(tool);
            do_capture();
        }
    }
}

/// Perform the actual capture from the framebuffer
fn do_capture() {
    let tool = SCREENSHOT.lock();
    let mode = tool.mode;
    let region = tool.region;
    drop(tool);

    let screenshot = match mode {
        CaptureMode::FullScreen => capture_full_screen(),
        CaptureMode::Region => {
            let x1 = region.start_x.min(region.end_x).max(0) as u32;
            let y1 = region.start_y.min(region.end_y).max(0) as u32;
            let x2 = region.start_x.max(region.end_x).max(0) as u32;
            let y2 = region.start_y.max(region.end_y).max(0) as u32;
            let w = x2.saturating_sub(x1).max(1);
            let h = y2.saturating_sub(y1).max(1);
            capture_region(x1, y1, w, h)
        }
        CaptureMode::ActiveWindow => capture_active_window(),
    };

    if let Some(shot) = screenshot {
        let mut tool = SCREENSHOT.lock();
        tool.capture_count += 1;
        let count = tool.capture_count;
        tool.last_screenshot = Some(shot.clone());
        drop(tool);

        // Save to file
        let filename = alloc::format!("/home/user/Pictures/screenshot_{}.ppm", count);
        save_screenshot_ppm(&shot, &filename);

        // Show notification
        super::notifications::success(
            "Screenshot Captured",
            &alloc::format!("Saved to {}", filename),
        );

        // Play screenshot sound
        super::sounds::screenshot();

        crate::serial_println!(
            "[screenshot] Captured {}x{} → {}",
            shot.width,
            shot.height,
            filename
        );
    }
}

/// Capture the full screen from the framebuffer
fn capture_full_screen() -> Option<CapturedScreenshot> {
    let fb_guard = super::FRAMEBUFFER.lock();
    let fb = fb_guard.as_ref()?;
    let width = fb.width as u32;
    let height = fb.height as u32;
    let bpp = fb.bytes_per_pixel;
    let mut pixels = Vec::with_capacity((width * height) as usize);

    for y in 0..height as usize {
        for x in 0..width as usize {
            let offset = y * fb.pitch + x * bpp;
            if offset + 3 < fb.buffer.len() {
                let b = fb.buffer[offset] as u32;
                let g = fb.buffer[offset + 1] as u32;
                let r = fb.buffer[offset + 2] as u32;
                let a = if bpp >= 4 {
                    fb.buffer[offset + 3] as u32
                } else {
                    0xFF
                };
                pixels.push((a << 24) | (r << 16) | (g << 8) | b);
            } else {
                pixels.push(0);
            }
        }
    }

    Some(CapturedScreenshot {
        width,
        height,
        pixels,
        timestamp: crate::interrupts::get_ticks(),
    })
}

/// Capture a sub-region of the framebuffer
fn capture_region(x: u32, y: u32, w: u32, h: u32) -> Option<CapturedScreenshot> {
    let fb_guard = super::FRAMEBUFFER.lock();
    let fb = fb_guard.as_ref()?;
    let fb_w = fb.width as u32;
    let fb_h = fb.height as u32;
    let bpp = fb.bytes_per_pixel;

    let x1 = x.min(fb_w);
    let y1 = y.min(fb_h);
    let x2 = (x + w).min(fb_w);
    let y2 = (y + h).min(fb_h);
    let cw = x2 - x1;
    let ch = y2 - y1;

    if cw == 0 || ch == 0 {
        return None;
    }

    let mut pixels = Vec::with_capacity((cw * ch) as usize);
    for row in y1..y2 {
        for col in x1..x2 {
            let offset = row as usize * fb.pitch + col as usize * bpp;
            if offset + 3 < fb.buffer.len() {
                let b = fb.buffer[offset] as u32;
                let g = fb.buffer[offset + 1] as u32;
                let r = fb.buffer[offset + 2] as u32;
                let a = if bpp >= 4 {
                    fb.buffer[offset + 3] as u32
                } else {
                    0xFF
                };
                pixels.push((a << 24) | (r << 16) | (g << 8) | b);
            } else {
                pixels.push(0);
            }
        }
    }

    Some(CapturedScreenshot {
        width: cw,
        height: ch,
        pixels,
        timestamp: crate::interrupts::get_ticks(),
    })
}

/// Capture the active window (falls back to full screen if no window manager)
fn capture_active_window() -> Option<CapturedScreenshot> {
    // Try to get the focused window's bounds from the window manager
    let wm = super::window::WINDOW_MANAGER.lock();
    if let Some(focused_id) = wm.focused_window {
        if let Some(win) = wm.windows.iter().find(|w| w.id == focused_id) {
            let x = win.rect.x.max(0) as u32;
            let y = win.rect.y.max(0) as u32;
            let w = win.rect.width;
            let h = win.rect.height;
            drop(wm);
            return capture_region(x, y, w, h);
        }
    }
    drop(wm);
    // Fallback to full screen
    capture_full_screen()
}

/// Save screenshot in PPM format (simple, no compression, universally readable)
fn save_screenshot_ppm(shot: &CapturedScreenshot, path: &str) {
    // PPM P6 binary format: simple RGB, no external dependencies
    let header = alloc::format!("P6\n{} {}\n255\n", shot.width, shot.height);
    let pixel_bytes = (shot.width * shot.height * 3) as usize;
    let mut data = Vec::with_capacity(header.len() + pixel_bytes);

    // Write header
    data.extend_from_slice(header.as_bytes());

    // Write RGB pixel data (convert from ARGB packed u32)
    for &pixel in &shot.pixels {
        let r = ((pixel >> 16) & 0xFF) as u8;
        let g = ((pixel >> 8) & 0xFF) as u8;
        let b = (pixel & 0xFF) as u8;
        data.push(r);
        data.push(g);
        data.push(b);
    }

    let _ = crate::file_manager::write_file(path, &data);
}

/// Draw the region selection overlay (rubber-band rectangle)
pub fn draw_region_overlay(fb: &mut super::framebuffer::FrameBuffer) {
    let tool = SCREENSHOT.lock();
    if !tool.region.active {
        return;
    }

    let x1 = tool.region.start_x.min(tool.region.end_x);
    let y1 = tool.region.start_y.min(tool.region.end_y);
    let x2 = tool.region.start_x.max(tool.region.end_x);
    let y2 = tool.region.start_y.max(tool.region.end_y);

    // Dim the entire screen outside the selection
    let screen_w = fb.width as i32;
    let screen_h = fb.height as i32;
    let dim = Pixel::new(0, 0, 0, 120);

    // Top bar
    if y1 > 0 {
        fb.fill_rect(
            super::framebuffer::Rect::new(0, 0, screen_w as u32, y1 as u32),
            dim,
        );
    }
    // Bottom bar
    if y2 < screen_h {
        fb.fill_rect(
            super::framebuffer::Rect::new(0, y2, screen_w as u32, (screen_h - y2) as u32),
            dim,
        );
    }
    // Left bar
    if x1 > 0 {
        fb.fill_rect(
            super::framebuffer::Rect::new(0, y1, x1 as u32, (y2 - y1) as u32),
            dim,
        );
    }
    // Right bar
    if x2 < screen_w {
        fb.fill_rect(
            super::framebuffer::Rect::new(x2, y1, (screen_w - x2) as u32, (y2 - y1) as u32),
            dim,
        );
    }

    // Selection border (dashed white)
    let border_color = Pixel::rgb(255, 255, 255);
    fb.draw_hline(x1, y1, (x2 - x1) as u32, border_color);
    fb.draw_hline(x1, y2, (x2 - x1) as u32, border_color);
    fb.draw_vline(x1, y1, (y2 - y1) as u32, border_color);
    fb.draw_vline(x2, y1, (y2 - y1) as u32, border_color);

    // Size indicator text
    let w = x2 - x1;
    let h = y2 - y1;
    if w > 0 && h > 0 {
        let size_text = alloc::format!("{}x{}", w, h);
        let text_x = x1 + (w - size_text.len() as i32 * 8) / 2;
        let text_y = y2 + 4;
        if text_y + 12 < screen_h {
            // Background pill for text
            let pill_w = (size_text.len() as u32 * 8) + 12;
            fb.fill_rounded_rect_aa(
                super::framebuffer::Rect::new(text_x - 6, text_y - 2, pill_w, 16),
                Pixel::new(0, 0, 0, 180),
                4,
            );
            super::fonts::draw_string_compact(
                fb,
                text_x,
                text_y,
                &size_text,
                Pixel::rgb(255, 255, 255),
                1,
            );
        }
    }

    // Instruction text at top
    let msg = "Click and drag to select region. Press Escape to cancel.";
    let msg_x = (screen_w - msg.len() as i32 * 8) / 2;
    let pill_w = (msg.len() as u32 * 8) + 16;
    fb.fill_rounded_rect_aa(
        super::framebuffer::Rect::new(msg_x - 8, 8, pill_w, 20),
        Pixel::new(0, 0, 0, 200),
        6,
    );
    super::fonts::draw_string_compact(fb, msg_x, 12, msg, Pixel::rgb(200, 200, 200), 1);
}

/// Initialize the screenshot subsystem
pub fn init() {
    crate::serial_println!(
        "[screenshot] Screenshot tool initialized (fullscreen, region, active window)"
    );
}
