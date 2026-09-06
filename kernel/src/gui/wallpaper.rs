/// Custom Wallpaper — Load & decode images from the VFS
///
/// Supports loading wallpaper images from the virtual filesystem in these
/// formats:
///   - **BMP** (uncompressed 24-bit / 32-bit) — common desktop format
///   - **RAW BGRA** — pixel buffer with a small 8-byte header (w:u32 + h:u32)
///   - **PPM** (P6 binary) — simple image interchange format
///
/// The decoded pixel data is stored in a global buffer and the desktop
/// compositor queries it on each wallpaper cache invalidation. If no custom
/// wallpaper is loaded, the procedurally generated "Nebula Depth" is used.
extern crate alloc;
use super::framebuffer::Pixel;
use alloc::vec::Vec;
use spin::Mutex;

// ─── Wallpaper modes ─────────────────────────────────────────────────

/// How the wallpaper image is drawn onto the screen
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WallpaperMode {
    /// Stretch image to fill the entire screen
    Stretch,
    /// Scale proportionally and center (may have bars)
    Fit,
    /// Scale proportionally and crop to fill
    Fill,
    /// Display at native size, centered
    Center,
    /// Tile the image to cover the screen
    Tile,
}

// ─── State ───────────────────────────────────────────────────────────

struct WallpaperImage {
    pixels: Vec<Pixel>,
    width: usize,
    height: usize,
    mode: WallpaperMode,
    path: [u8; 256],
    path_len: usize,
}

static CUSTOM_WALLPAPER: Mutex<Option<WallpaperImage>> = Mutex::new(None);

/// Returns true if a custom wallpaper is currently loaded
pub fn has_custom_wallpaper() -> bool {
    CUSTOM_WALLPAPER.lock().is_some()
}

/// Clear the custom wallpaper (revert to procedural)
pub fn clear_wallpaper() {
    *CUSTOM_WALLPAPER.lock() = None;
    super::desktop::invalidate_wallpaper_cache();
}

/// Set the display mode for the current wallpaper
pub fn set_mode(mode: WallpaperMode) {
    if let Some(ref mut wp) = *CUSTOM_WALLPAPER.lock() {
        wp.mode = mode;
    }
    super::desktop::invalidate_wallpaper_cache();
}

/// Load a wallpaper image from the VFS
pub fn load_wallpaper(path: &str) -> Result<(), &'static str> {
    let data = crate::vfs::read_file_dispatch(path).ok_or("File not found")?;

    let (pixels, w, h) = decode_image(&data)?;

    let mut path_buf = [0u8; 256];
    let plen = path.len().min(256);
    path_buf[..plen].copy_from_slice(&path.as_bytes()[..plen]);

    *CUSTOM_WALLPAPER.lock() = Some(WallpaperImage {
        pixels,
        width: w,
        height: h,
        mode: WallpaperMode::Fill,
        path: path_buf,
        path_len: plen,
    });

    super::desktop::invalidate_wallpaper_cache();
    Ok(())
}

/// Blit the custom wallpaper into the framebuffer (called from desktop compositor)
pub fn render_to_framebuffer(fb: &mut super::framebuffer::FrameBuffer) {
    let guard = CUSTOM_WALLPAPER.lock();
    let wp = match guard.as_ref() {
        Some(w) => w,
        None => return,
    };

    let sw = fb.width;
    let sh = fb.height;
    let iw = wp.width;
    let ih = wp.height;

    if iw == 0 || ih == 0 {
        return;
    }

    match wp.mode {
        WallpaperMode::Stretch => blit_stretched(fb, &wp.pixels, iw, ih, sw, sh),
        WallpaperMode::Fit => blit_fit(fb, &wp.pixels, iw, ih, sw, sh),
        WallpaperMode::Fill => blit_fill(fb, &wp.pixels, iw, ih, sw, sh),
        WallpaperMode::Center => blit_center(fb, &wp.pixels, iw, ih, sw, sh),
        WallpaperMode::Tile => blit_tile(fb, &wp.pixels, iw, ih, sw, sh),
    }
}

// ─── Image Decoding ──────────────────────────────────────────────────

fn decode_image(data: &[u8]) -> Result<(Vec<Pixel>, usize, usize), &'static str> {
    // Detect format by header bytes
    if data.len() > 2 && data[0] == b'B' && data[1] == b'M' {
        return decode_bmp(data);
    }
    if data.len() > 2 && data[0] == b'P' && data[1] == b'6' {
        return decode_ppm(data);
    }
    if data.len() > 8 {
        // Try RAW BGRA format: header = [width: u32 LE, height: u32 LE], then pixel data
        return decode_raw_bgra(data);
    }
    Err("Unrecognized image format")
}

/// Decode uncompressed BMP (24-bit or 32-bit)
fn decode_bmp(data: &[u8]) -> Result<(Vec<Pixel>, usize, usize), &'static str> {
    if data.len() < 54 {
        return Err("BMP too small");
    }

    let pixel_offset = u32_le(data, 10) as usize;
    let dib_size = u32_le(data, 14);
    if dib_size < 40 {
        return Err("Unsupported BMP header");
    }

    let width = i32_le(data, 18) as usize;
    let height_raw = i32_le(data, 22);
    let top_down = height_raw < 0;
    let height = if top_down {
        (-height_raw) as usize
    } else {
        height_raw as usize
    };

    let bpp = u16_le(data, 28) as usize;
    let compression = u32_le(data, 30);

    if compression != 0 && compression != 3 {
        return Err("Compressed BMP not supported");
    }
    if bpp != 24 && bpp != 32 {
        return Err("Only 24/32-bit BMP supported");
    }

    let bytes_per_pixel = bpp / 8;
    let row_size = (bpp * width).div_ceil(32) * 4; // BMP rows are 4-byte aligned

    let mut pixels = Vec::with_capacity(width * height);

    for row in 0..height {
        let src_row = if top_down { row } else { height - 1 - row };
        let row_offset = pixel_offset + src_row * row_size;

        for col in 0..width {
            let off = row_offset + col * bytes_per_pixel;
            if off + bytes_per_pixel > data.len() {
                pixels.push(Pixel::rgb(0, 0, 0));
                continue;
            }
            let b = data[off];
            let g = data[off + 1];
            let r = data[off + 2];
            let a = if bytes_per_pixel == 4 {
                data[off + 3]
            } else {
                255
            };
            pixels.push(Pixel::new(r, g, b, a));
        }
    }

    Ok((pixels, width, height))
}

/// Decode PPM P6 binary image
fn decode_ppm(data: &[u8]) -> Result<(Vec<Pixel>, usize, usize), &'static str> {
    // P6\n<width> <height>\n<maxval>\n<pixel data>
    if data.len() < 10 {
        return Err("PPM too small");
    }

    let text_part = &data[3..]; // skip "P6\n"
    let (width, rest) = parse_ppm_number(text_part)?;
    let rest = skip_whitespace(rest);
    let (height, rest) = parse_ppm_number(rest)?;
    let rest = skip_whitespace(rest);
    let (_maxval, rest) = parse_ppm_number(rest)?;
    // Skip exactly one whitespace after maxval
    let pixel_data = if !rest.is_empty() { &rest[1..] } else { rest };

    let mut pixels = Vec::with_capacity(width * height);
    let mut off = 0;
    for _ in 0..(width * height) {
        if off + 3 > pixel_data.len() {
            pixels.push(Pixel::rgb(0, 0, 0));
            off += 3;
            continue;
        }
        let r = pixel_data[off];
        let g = pixel_data[off + 1];
        let b = pixel_data[off + 2];
        pixels.push(Pixel::rgb(r, g, b));
        off += 3;
    }

    Ok((pixels, width, height))
}

fn parse_ppm_number(data: &[u8]) -> Result<(usize, &[u8]), &'static str> {
    let mut n: usize = 0;
    let mut i = 0;
    while i < data.len() && data[i] >= b'0' && data[i] <= b'9' {
        n = n * 10 + (data[i] - b'0') as usize;
        i += 1;
    }
    if i == 0 {
        return Err("PPM: expected number");
    }
    Ok((n, &data[i..]))
}

fn skip_whitespace(data: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < data.len()
        && (data[i] == b' ' || data[i] == b'\t' || data[i] == b'\n' || data[i] == b'\r')
    {
        i += 1;
    }
    &data[i..]
}

/// Decode RAW BGRA: header [w:u32 LE, h:u32 LE] followed by BGRA pixel data
fn decode_raw_bgra(data: &[u8]) -> Result<(Vec<Pixel>, usize, usize), &'static str> {
    if data.len() < 8 {
        return Err("RAW image too small");
    }
    let width = u32_le(data, 0) as usize;
    let height = u32_le(data, 4) as usize;

    // Sanity: reasonable dimensions
    if width == 0 || height == 0 || width > 16384 || height > 16384 {
        return Err("RAW: unreasonable dimensions");
    }

    let expected = 8 + width * height * 4;
    if data.len() < expected {
        return Err("RAW: truncated pixel data");
    }

    let mut pixels = Vec::with_capacity(width * height);
    let mut off = 8;
    for _ in 0..(width * height) {
        let b = data[off];
        let g = data[off + 1];
        let r = data[off + 2];
        let a = data[off + 3];
        pixels.push(Pixel::new(r, g, b, a));
        off += 4;
    }

    Ok((pixels, width, height))
}

// ─── Blit helpers ────────────────────────────────────────────────────

fn sample_pixel(pixels: &[Pixel], iw: usize, _ih: usize, x: usize, y: usize) -> Pixel {
    let idx = y * iw + x;
    if idx < pixels.len() {
        pixels[idx]
    } else {
        Pixel::rgb(0, 0, 0)
    }
}

/// Nearest-neighbor stretch to fill screen
fn blit_stretched(
    fb: &mut super::framebuffer::FrameBuffer,
    pixels: &[Pixel],
    iw: usize,
    ih: usize,
    sw: usize,
    sh: usize,
) {
    for sy in 0..sh {
        let iy = sy * ih / sh;
        for sx in 0..sw {
            let ix = sx * iw / sw;
            fb.set_pixel(sx, sy, sample_pixel(pixels, iw, ih, ix, iy));
        }
    }
}

/// Scale proportionally and center (letterbox / pillarbox)
fn blit_fit(
    fb: &mut super::framebuffer::FrameBuffer,
    pixels: &[Pixel],
    iw: usize,
    ih: usize,
    sw: usize,
    sh: usize,
) {
    // Fill background black
    for y in 0..sh {
        for x in 0..sw {
            fb.set_pixel(x, y, Pixel::rgb(0, 0, 0));
        }
    }

    let scale_x = (sw * 1024) / iw;
    let scale_y = (sh * 1024) / ih;
    let scale = if scale_x < scale_y { scale_x } else { scale_y };
    let dw = (iw * scale) / 1024;
    let dh = (ih * scale) / 1024;
    let ox = (sw.saturating_sub(dw)) / 2;
    let oy = (sh.saturating_sub(dh)) / 2;

    for dy in 0..dh {
        let iy = dy * ih / dh;
        for dx in 0..dw {
            let ix = dx * iw / dw;
            fb.set_pixel(ox + dx, oy + dy, sample_pixel(pixels, iw, ih, ix, iy));
        }
    }
}

/// Scale proportionally and crop to fill
fn blit_fill(
    fb: &mut super::framebuffer::FrameBuffer,
    pixels: &[Pixel],
    iw: usize,
    ih: usize,
    sw: usize,
    sh: usize,
) {
    let scale_x = (sw * 1024) / iw;
    let scale_y = (sh * 1024) / ih;
    let scale = if scale_x > scale_y { scale_x } else { scale_y };
    let dw = (iw * scale) / 1024;
    let dh = (ih * scale) / 1024;
    // Source offset for centering the crop
    let crop_x = dw.saturating_sub(sw) / 2;
    let crop_y = dh.saturating_sub(sh) / 2;

    for sy in 0..sh {
        let src_y = ((crop_y + sy) * ih) / dh;
        for sx in 0..sw {
            let src_x = ((crop_x + sx) * iw) / dw;
            fb.set_pixel(
                sx,
                sy,
                sample_pixel(pixels, iw, ih, src_x.min(iw - 1), src_y.min(ih - 1)),
            );
        }
    }
}

/// Display at native size, centered on black
fn blit_center(
    fb: &mut super::framebuffer::FrameBuffer,
    pixels: &[Pixel],
    iw: usize,
    ih: usize,
    sw: usize,
    sh: usize,
) {
    // Fill background black
    for y in 0..sh {
        for x in 0..sw {
            fb.set_pixel(x, y, Pixel::rgb(0, 0, 0));
        }
    }

    let ox = sw.saturating_sub(iw) / 2;
    let oy = sh.saturating_sub(ih) / 2;
    let copy_w = iw.min(sw);
    let copy_h = ih.min(sh);
    let src_ox = if iw > sw { (iw - sw) / 2 } else { 0 };
    let src_oy = if ih > sh { (ih - sh) / 2 } else { 0 };

    for dy in 0..copy_h {
        for dx in 0..copy_w {
            let px = sample_pixel(pixels, iw, ih, src_ox + dx, src_oy + dy);
            fb.set_pixel(ox + dx, oy + dy, px);
        }
    }
}

/// Tile the image to cover the screen
fn blit_tile(
    fb: &mut super::framebuffer::FrameBuffer,
    pixels: &[Pixel],
    iw: usize,
    ih: usize,
    sw: usize,
    sh: usize,
) {
    for sy in 0..sh {
        let iy = sy % ih;
        for sx in 0..sw {
            let ix = sx % iw;
            fb.set_pixel(sx, sy, sample_pixel(pixels, iw, ih, ix, iy));
        }
    }
}

// ─── Little-endian byte helpers ──────────────────────────────────────

fn u16_le(data: &[u8], off: usize) -> u16 {
    (data[off] as u16) | ((data[off + 1] as u16) << 8)
}

fn u32_le(data: &[u8], off: usize) -> u32 {
    (data[off] as u32)
        | ((data[off + 1] as u32) << 8)
        | ((data[off + 2] as u32) << 16)
        | ((data[off + 3] as u32) << 24)
}

fn i32_le(data: &[u8], off: usize) -> i32 {
    u32_le(data, off) as i32
}
