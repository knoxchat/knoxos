/// Image Viewer — Displays BMP/TGA images from the VFS
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

use super::colors;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::window::{self, WindowContentType, WindowId};

// ─── State ──────────────────────────────────────────────────────────

struct ImageViewerState {
    window_id: WindowId,
    file_path: String,
    /// Decoded pixel data (RGBA)
    pixels: Vec<u32>,
    img_width: u32,
    img_height: u32,
    /// Zoom level: 100 = 1x
    zoom: u32,
    /// Pan offset
    pan_x: i32,
    pan_y: i32,
    /// Status message
    status: String,
}

lazy_static! {
    static ref STATES: Mutex<Vec<ImageViewerState>> = Mutex::new(Vec::new());
}

const TOOLBAR_HEIGHT: i32 = 32;

// ─── Public API ─────────────────────────────────────────────────────

pub fn open() {
    open_with_path("");
}

pub fn open_with_path(path: &str) {
    let mut win = window::Window::new("Image Viewer", 200, 80, 800, 600);
    win.content_type = WindowContentType::ImageViewer;
    let wid = win.id;

    let mut wm = window::WINDOW_MANAGER.lock();
    wm.add_window(win);
    drop(wm);

    let mut state = ImageViewerState {
        window_id: wid,
        file_path: String::from(path),
        pixels: Vec::new(),
        img_width: 0,
        img_height: 0,
        zoom: 100,
        pan_x: 0,
        pan_y: 0,
        status: String::from("No image loaded. Use File > Open to load an image."),
    };

    if !path.is_empty() {
        load_image(&mut state, path);
    }

    STATES.lock().push(state);

    super::taskbar::add_entry(wid, "Image Viewer");
    super::taskbar::set_active(wid);
    super::sounds::window_open();
}

pub fn close(wid: WindowId) {
    STATES.lock().retain(|s| s.window_id != wid);
}

fn load_image(state: &mut ImageViewerState, path: &str) {
    let vfs = crate::vfs::VFS.lock();
    let data = match vfs.read_file(path) {
        Some(d) => d.to_vec(),
        None => {
            state.status = format!("Failed to open: {}", path);
            return;
        }
    };
    drop(vfs);

    state.file_path = String::from(path);

    // Try BMP decode
    if data.len() > 54 && data[0] == b'B' && data[1] == b'M' {
        if let Some((w, h, pixels)) = decode_bmp(&data) {
            state.img_width = w;
            state.img_height = h;
            state.pixels = pixels;
            state.status = format!("{}  ({}×{} BMP)", path, w, h);
            state.zoom = 100;
            state.pan_x = 0;
            state.pan_y = 0;
            return;
        }
    }

    // Try TGA decode (uncompressed)
    if data.len() > 18 {
        if let Some((w, h, pixels)) = decode_tga(&data) {
            state.img_width = w;
            state.img_height = h;
            state.pixels = pixels;
            state.status = format!("{}  ({}×{} TGA)", path, w, h);
            state.zoom = 100;
            state.pan_x = 0;
            state.pan_y = 0;
            return;
        }
    }

    state.status = format!("Unsupported format: {}", path);
}

fn decode_bmp(data: &[u8]) -> Option<(u32, u32, Vec<u32>)> {
    if data.len() < 54 {
        return None;
    }
    let pixel_offset = u32::from_le_bytes([data[10], data[11], data[12], data[13]]) as usize;
    let width = i32::from_le_bytes([data[18], data[19], data[20], data[21]]);
    let height = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
    let bpp = u16::from_le_bytes([data[28], data[29]]);

    if width <= 0 || width > 8192 {
        return None;
    }
    let w = width as u32;
    let h = height.unsigned_abs();
    let bottom_up = height > 0;

    if bpp != 24 && bpp != 32 {
        return None;
    }
    let bytes_per_pixel = (bpp / 8) as usize;
    let row_size = (w as usize * bytes_per_pixel).div_ceil(4) * 4; // Padded to 4 bytes

    let mut pixels = Vec::with_capacity((w * h) as usize);

    for row in 0..h {
        let src_row = if bottom_up { h - 1 - row } else { row };
        let row_start = pixel_offset + src_row as usize * row_size;
        for col in 0..w {
            let idx = row_start + col as usize * bytes_per_pixel;
            if idx + bytes_per_pixel > data.len() {
                pixels.push(0xFF000000); // Black
                continue;
            }
            let b = data[idx] as u32;
            let g = data[idx + 1] as u32;
            let r = data[idx + 2] as u32;
            let a = if bpp == 32 { data[idx + 3] as u32 } else { 255 };
            pixels.push((a << 24) | (r << 16) | (g << 8) | b);
        }
    }

    Some((w, h, pixels))
}

fn decode_tga(data: &[u8]) -> Option<(u32, u32, Vec<u32>)> {
    if data.len() < 18 {
        return None;
    }
    let id_len = data[0] as usize;
    let image_type = data[2];
    // Only support uncompressed true-color (type 2)
    if image_type != 2 {
        return None;
    }
    let w = u16::from_le_bytes([data[12], data[13]]) as u32;
    let h = u16::from_le_bytes([data[14], data[15]]) as u32;
    let bpp = data[16];
    let descriptor = data[17];
    let top_to_bottom = (descriptor & 0x20) != 0;

    if w == 0 || w > 8192 || h == 0 || h > 8192 {
        return None;
    }
    if bpp != 24 && bpp != 32 {
        return None;
    }

    let bytes_per_pixel = (bpp / 8) as usize;
    let pixel_start = 18 + id_len;

    let mut pixels = Vec::with_capacity((w * h) as usize);
    for row in 0..h {
        let src_row = if top_to_bottom { row } else { h - 1 - row };
        for col in 0..w {
            let idx = pixel_start + (src_row * w + col) as usize * bytes_per_pixel;
            if idx + bytes_per_pixel > data.len() {
                pixels.push(0xFF000000);
                continue;
            }
            let b = data[idx] as u32;
            let g = data[idx + 1] as u32;
            let r = data[idx + 2] as u32;
            let a = if bpp == 32 { data[idx + 3] as u32 } else { 255 };
            pixels.push((a << 24) | (r << 16) | (g << 8) | b);
        }
    }

    Some((w, h, pixels))
}

// ─── Drawing ────────────────────────────────────────────────────────

pub fn draw_content(fb: &mut FrameBuffer, wid: WindowId, area: Rect, _scroll_y: i32) {
    let states = STATES.lock();
    let state = match states.iter().find(|s| s.window_id == wid) {
        Some(s) => s,
        None => return,
    };

    // Background
    fb.fill_rect(area, Pixel::rgb(30, 30, 34));

    // ── Toolbar ──
    let tb_rect = Rect::new(area.x, area.y, area.width, TOOLBAR_HEIGHT as u32);
    fb.fill_rect(tb_rect, Pixel::rgb(38, 38, 44));
    fb.draw_hline(
        area.x,
        area.y + TOOLBAR_HEIGHT,
        area.width,
        Pixel::rgb(55, 55, 65),
    );

    // Toolbar buttons: Open | Zoom In | Zoom Out | Fit | 100%
    let buttons = &["Open", "+", "−", "Fit", "100%"];
    let mut bx = area.x + 8;
    for label in buttons {
        let bw = label.len() as i32 * 7 + 16;
        fb.fill_rounded_rect_aa(
            Rect::new(bx, area.y + 4, bw as u32, 24),
            Pixel::rgb(55, 55, 65),
            4,
        );
        fonts::draw_string_compact(fb, bx + 8, area.y + 10, label, Pixel::rgb(200, 200, 210), 1);
        bx += bw + 4;
    }

    // Zoom indicator
    let zoom_text = format!("{}%", state.zoom);
    fonts::draw_string_compact(
        fb,
        area.x + area.width as i32 - 60,
        area.y + 10,
        &zoom_text,
        Pixel::rgb(140, 140, 150),
        1,
    );

    // ── Image viewport ──
    let vp_y = area.y + TOOLBAR_HEIGHT + 1;
    let vp_h = area.height as i32 - TOOLBAR_HEIGHT - 1 - 24; // 24 for status bar

    if state.pixels.is_empty() {
        // No image loaded — show placeholder
        let msg = "No image loaded";
        let msg_w = msg.len() as i32 * 7;
        fonts::draw_string_compact(
            fb,
            area.x + (area.width as i32 - msg_w) / 2,
            vp_y + vp_h / 2 - 6,
            msg,
            Pixel::rgb(100, 100, 110),
            1,
        );
        // Hint
        fonts::draw_string_compact(
            fb,
            area.x + (area.width as i32 - 200) / 2,
            vp_y + vp_h / 2 + 12,
            "Click \"Open\" to browse for an image",
            Pixel::rgb(80, 80, 90),
            1,
        );
    } else {
        // Draw the image with zoom and pan
        let scale_num = state.zoom;
        let draw_w = (state.img_width * scale_num / 100) as i32;
        let draw_h = (state.img_height * scale_num / 100) as i32;

        // Center in viewport + pan offset
        let cx = area.x + (area.width as i32 - draw_w) / 2 + state.pan_x;
        let cy = vp_y + (vp_h - draw_h) / 2 + state.pan_y;

        // Checkerboard background for transparency
        let check_x0 = cx.max(area.x);
        let check_y0 = cy.max(vp_y);
        let check_x1 = (cx + draw_w).min(area.x + area.width as i32);
        let check_y1 = (cy + draw_h).min(vp_y + vp_h);

        if check_x1 > check_x0 && check_y1 > check_y0 {
            // Draw checkerboard
            let c1 = Pixel::rgb(50, 50, 50);
            let c2 = Pixel::rgb(60, 60, 60);
            for py in (check_y0..check_y1).step_by(8) {
                for px in (check_x0..check_x1).step_by(8) {
                    let cw = 8u32.min((check_x1 - px) as u32);
                    let ch = 8u32.min((check_y1 - py) as u32);
                    let checker = if ((px - check_x0) / 8 + (py - check_y0) / 8) % 2 == 0 {
                        c1
                    } else {
                        c2
                    };
                    fb.fill_rect(Rect::new(px, py, cw, ch), checker);
                }
            }
        }

        // Blit image pixels (nearest-neighbor scaling)
        for sy in 0..state.img_height {
            let dy = cy + (sy * scale_num / 100) as i32;
            if dy < vp_y || dy >= vp_y + vp_h {
                continue;
            }
            for sx in 0..state.img_width {
                let dx = cx + (sx * scale_num / 100) as i32;
                if dx < area.x || dx >= area.x + area.width as i32 {
                    continue;
                }
                let rgba = state.pixels[(sy * state.img_width + sx) as usize];
                let a = (rgba >> 24) & 0xFF;
                if a < 128 {
                    continue; // Skip mostly transparent pixels
                }
                let r = ((rgba >> 16) & 0xFF) as u8;
                let g = ((rgba >> 8) & 0xFF) as u8;
                let b = (rgba & 0xFF) as u8;
                fb.set_pixel(dx as usize, dy as usize, Pixel::rgb(r, g, b));
            }
        }
    }

    // ── Status bar ──
    let sb_y = area.y + area.height as i32 - 24;
    fb.fill_rect(
        Rect::new(area.x, sb_y, area.width, 24),
        Pixel::rgb(34, 34, 40),
    );
    fb.draw_hline(area.x, sb_y, area.width, Pixel::rgb(50, 50, 58));
    let status_display = if state.status.len() > 80 {
        &state.status[..80]
    } else {
        &state.status
    };
    fonts::draw_string_compact(
        fb,
        area.x + 8,
        sb_y + 6,
        status_display,
        Pixel::rgb(160, 160, 170),
        1,
    );

    drop(states);
}

// ─── Click Handling ─────────────────────────────────────────────────

pub fn handle_click(wid: WindowId, area: Rect, click_x: i32, click_y: i32) -> bool {
    // Check toolbar button clicks
    if click_y >= area.y && click_y < area.y + TOOLBAR_HEIGHT {
        let buttons = &["Open", "+", "−", "Fit", "100%"];
        let mut bx = area.x + 8;
        for (i, label) in buttons.iter().enumerate() {
            let bw = label.len() as i32 * 7 + 16;
            if click_x >= bx && click_x < bx + bw {
                match i {
                    0 => {
                        // Open: Show file picker
                        super::file_picker::open("/home/user/Pictures");
                    }
                    1 => zoom_in(wid),
                    2 => zoom_out(wid),
                    3 => fit_to_window(wid, area),
                    4 => set_zoom(wid, 100),
                    _ => {}
                }
                return true;
            }
            bx += bw + 4;
        }
    }
    false
}

fn zoom_in(wid: WindowId) {
    let mut states = STATES.lock();
    if let Some(state) = states.iter_mut().find(|s| s.window_id == wid) {
        state.zoom = (state.zoom + 25).min(800);
    }
}

fn zoom_out(wid: WindowId) {
    let mut states = STATES.lock();
    if let Some(state) = states.iter_mut().find(|s| s.window_id == wid) {
        state.zoom = state.zoom.saturating_sub(25).max(10);
    }
}

fn set_zoom(wid: WindowId, zoom: u32) {
    let mut states = STATES.lock();
    if let Some(state) = states.iter_mut().find(|s| s.window_id == wid) {
        state.zoom = zoom;
    }
}

fn fit_to_window(wid: WindowId, area: Rect) {
    let mut states = STATES.lock();
    if let Some(state) = states.iter_mut().find(|s| s.window_id == wid) {
        if state.img_width > 0 && state.img_height > 0 {
            let vp_w = area.width;
            let vp_h = (area.height as i32 - TOOLBAR_HEIGHT - 24) as u32;
            let scale_x = vp_w * 100 / state.img_width;
            let scale_y = vp_h * 100 / state.img_height;
            state.zoom = scale_x.min(scale_y).clamp(10, 800);
            state.pan_x = 0;
            state.pan_y = 0;
        }
    }
}

/// Load a file into the image viewer (called from file picker callback etc.)
pub fn load_file(wid: WindowId, path: &str) {
    let mut states = STATES.lock();
    if let Some(state) = states.iter_mut().find(|s| s.window_id == wid) {
        load_image(state, path);
    }
}
