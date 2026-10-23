/// Custom wallpaper loaded from a file picker path (BMP).
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::gui::framebuffer::{FrameBuffer, Pixel};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Custom Wallpaper from File Picker   (8.30)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Custom wallpaper state — optionally stores raw RGBA pixel data
/// that replaces the procedural Aurora wallpaper.
pub struct CustomWallpaper {
    pub enabled: bool,
    /// RGBA pixel data (width × height × 4 bytes)
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub path: String,
}

static CUSTOM_WALLPAPER: Mutex<CustomWallpaper> = Mutex::new(CustomWallpaper {
    enabled: false,
    data: Vec::new(),
    width: 0,
    height: 0,
    path: String::new(),
});

/// Set a custom wallpaper from an image file path.
/// Reads the file, parses simple BMP/raw formats, and caches pixel data.
pub fn set_custom_wallpaper(path: &str) -> bool {
    let vfs = crate::vfs::VFS.lock();
    let file_data = match vfs.read_file(path) {
        Some(d) => d.to_vec(),
        None => return false,
    };
    drop(vfs);

    // Try to parse as a simple BMP (DIB header)
    if file_data.len() > 54 && file_data[0] == b'B' && file_data[1] == b'M' {
        let data_offset =
            u32::from_le_bytes([file_data[10], file_data[11], file_data[12], file_data[13]])
                as usize;
        let w =
            i32::from_le_bytes([file_data[18], file_data[19], file_data[20], file_data[21]]) as u32;
        let h = (i32::from_le_bytes([file_data[22], file_data[23], file_data[24], file_data[25]]))
            .unsigned_abs();
        let bpp = u16::from_le_bytes([file_data[28], file_data[29]]);

        if bpp == 24 || bpp == 32 {
            let bytes_per_pixel = (bpp / 8) as usize;
            let row_size = ((bpp as u32 * w).div_ceil(32) * 4) as usize;
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);

            for row in 0..h {
                let src_row = if i32::from_le_bytes([
                    file_data[22],
                    file_data[23],
                    file_data[24],
                    file_data[25],
                ]) > 0
                {
                    // Bottom-up BMP
                    (h - 1 - row) as usize
                } else {
                    row as usize
                };
                let row_start = data_offset + src_row * row_size;
                for col in 0..w as usize {
                    let off = row_start + col * bytes_per_pixel;
                    if off + 2 < file_data.len() {
                        let b = file_data[off];
                        let g = file_data[off + 1];
                        let r = file_data[off + 2];
                        let a = if bytes_per_pixel == 4 && off + 3 < file_data.len() {
                            file_data[off + 3]
                        } else {
                            255
                        };
                        rgba.push(r);
                        rgba.push(g);
                        rgba.push(b);
                        rgba.push(a);
                    }
                }
            }

            let mut cw = CUSTOM_WALLPAPER.lock();
            cw.enabled = true;
            cw.data = rgba;
            cw.width = w;
            cw.height = h;
            cw.path = String::from(path);
            drop(cw);
            crate::gui::desktop::invalidate_wallpaper_cache();
            crate::gui::request_redraw();
            return true;
        }
    }

    false
}

/// Clear custom wallpaper and revert to procedural Aurora wallpaper
pub fn clear_custom_wallpaper() {
    let mut cw = CUSTOM_WALLPAPER.lock();
    cw.enabled = false;
    cw.data.clear();
    cw.width = 0;
    cw.height = 0;
    cw.path.clear();
    drop(cw);
    crate::gui::desktop::invalidate_wallpaper_cache();
    crate::gui::request_redraw();
}

/// Check if a custom wallpaper is active
pub fn has_custom_wallpaper() -> bool {
    CUSTOM_WALLPAPER.lock().enabled
}

/// Blit the custom wallpaper onto the framebuffer, stretching to fit
pub fn draw_custom_wallpaper(fb: &mut FrameBuffer) {
    let cw = CUSTOM_WALLPAPER.lock();
    if !cw.enabled || cw.data.is_empty() || cw.width == 0 || cw.height == 0 {
        return;
    }
    let dst_w = fb.width;
    let dst_h = fb.height;
    let src_w = cw.width;
    let src_h = cw.height;

    // Nearest-neighbor scale
    for dy in 0..dst_h {
        let sy = (dy as u64 * src_h as u64 / dst_h as u64) as u32;
        for dx in 0..dst_w {
            let sx = (dx as u64 * src_w as u64 / dst_w as u64) as u32;
            let off = ((sy * src_w + sx) * 4) as usize;
            if off + 3 < cw.data.len() {
                let r = cw.data[off];
                let g = cw.data[off + 1];
                let b = cw.data[off + 2];
                fb.set_pixel(dx, dy, Pixel::rgb(r, g, b));
            }
        }
    }
}

/// Open file picker to select a wallpaper image
pub fn pick_wallpaper() {
    crate::gui::file_picker::open("/home/user/Pictures");
}
