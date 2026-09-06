//! Custom Wallpaper — Load wallpaper images from the filesystem
//!
//! Supports loading BMP images from the VFS and setting them as desktop
//! wallpaper. Includes scaling modes (stretch, center, tile, fit, fill).
//! Covers status.md item 7.28 (Custom wallpaper from filesystem).

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

/// Wallpaper scaling mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallpaperMode {
    /// Stretch to fill screen (may distort)
    Stretch,
    /// Center at original size (black bars)
    Center,
    /// Tile to fill screen
    Tile,
    /// Scale to fit within screen (preserves aspect ratio, may have bars)
    Fit,
    /// Scale to fill screen (preserves aspect ratio, may crop)
    Fill,
}

/// Loaded wallpaper image
#[derive(Debug, Clone)]
pub struct WallpaperImage {
    pub width: usize,
    pub height: usize,
    /// BGRA pixel data
    pub pixels: Vec<u32>,
    pub path: String,
}

/// Wallpaper state
struct WallpaperState {
    current_image: Option<WallpaperImage>,
    mode: WallpaperMode,
    /// Pre-scaled buffer for the current resolution
    scaled_cache: Vec<u32>,
    cache_width: usize,
    cache_height: usize,
}

lazy_static::lazy_static! {
    static ref STATE: Mutex<WallpaperState> = Mutex::new(WallpaperState {
        current_image: None,
        mode: WallpaperMode::Fill,
        scaled_cache: Vec::new(),
        cache_width: 0,
        cache_height: 0,
    });
}

static LOAD_COUNT: AtomicU64 = AtomicU64::new(0);

/// Parse a BMP file from raw bytes
/// Supports 24-bit and 32-bit uncompressed BMPs
pub fn parse_bmp(data: &[u8]) -> Option<WallpaperImage> {
    if data.len() < 54 {
        return None;
    }
    // BMP header check
    if data[0] != b'B' || data[1] != b'M' {
        return None;
    }

    let data_offset = u32::from_le_bytes([data[10], data[11], data[12], data[13]]) as usize;
    let width = i32::from_le_bytes([data[18], data[19], data[20], data[21]]) as usize;
    let height_raw = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
    let bpp = u16::from_le_bytes([data[28], data[29]]) as usize;

    let top_down = height_raw < 0;
    let height = if top_down {
        (-height_raw) as usize
    } else {
        height_raw as usize
    };

    if width == 0 || height == 0 || width > 8192 || height > 8192 {
        return None;
    }

    let bytes_per_pixel = bpp / 8;
    if bytes_per_pixel != 3 && bytes_per_pixel != 4 {
        return None;
    }

    let row_size = (bpp * width).div_ceil(32) * 4; // Rows are padded to 4 bytes
    let mut pixels = Vec::with_capacity(width * height);

    for y in 0..height {
        let src_y = if top_down { y } else { height - 1 - y };
        let row_start = data_offset + src_y * row_size;

        for x in 0..width {
            let px_start = row_start + x * bytes_per_pixel;
            if px_start + bytes_per_pixel > data.len() {
                pixels.push(0xFF000000); // Black fallback
                continue;
            }

            let b = data[px_start] as u32;
            let g = data[px_start + 1] as u32;
            let r = data[px_start + 2] as u32;
            let a = if bytes_per_pixel == 4 {
                data[px_start + 3] as u32
            } else {
                0xFF
            };
            pixels.push((a << 24) | (r << 16) | (g << 8) | b);
        }
    }

    Some(WallpaperImage {
        width,
        height,
        pixels,
        path: String::new(),
    })
}

/// Scale image to target dimensions using bilinear interpolation
pub fn scale_image(img: &WallpaperImage, target_w: usize, target_h: usize) -> Vec<u32> {
    let mut out = Vec::with_capacity(target_w * target_h);
    let x_ratio = img.width as f32 / target_w as f32;
    let y_ratio = img.height as f32 / target_h as f32;

    for y in 0..target_h {
        let sy = (y as f32 * y_ratio) as usize;
        let sy = sy.min(img.height.saturating_sub(1));

        for x in 0..target_w {
            let sx = (x as f32 * x_ratio) as usize;
            let sx = sx.min(img.width.saturating_sub(1));
            out.push(img.pixels[sy * img.width + sx]);
        }
    }
    out
}

/// Apply wallpaper mode to produce screen-sized buffer
pub fn apply_mode(
    img: &WallpaperImage,
    mode: WallpaperMode,
    screen_w: usize,
    screen_h: usize,
) -> Vec<u32> {
    match mode {
        WallpaperMode::Stretch => scale_image(img, screen_w, screen_h),
        WallpaperMode::Center => {
            let mut buf = alloc::vec![0xFF1A1A2E; screen_w * screen_h]; // Dark background
            let ox = screen_w.saturating_sub(img.width) / 2;
            let oy = screen_h.saturating_sub(img.height) / 2;
            for y in 0..img.height.min(screen_h) {
                for x in 0..img.width.min(screen_w) {
                    let dx = ox + x;
                    let dy = oy + y;
                    if dx < screen_w && dy < screen_h {
                        buf[dy * screen_w + dx] = img.pixels[y * img.width + x];
                    }
                }
            }
            buf
        }
        WallpaperMode::Tile => {
            let mut buf = Vec::with_capacity(screen_w * screen_h);
            for y in 0..screen_h {
                for x in 0..screen_w {
                    let tx = x % img.width;
                    let ty = y % img.height;
                    buf.push(img.pixels[ty * img.width + tx]);
                }
            }
            buf
        }
        WallpaperMode::Fit => {
            let scale_x = screen_w as f32 / img.width as f32;
            let scale_y = screen_h as f32 / img.height as f32;
            let scale = if scale_x < scale_y { scale_x } else { scale_y };
            let nw = (img.width as f32 * scale) as usize;
            let nh = (img.height as f32 * scale) as usize;
            let scaled = scale_image(img, nw, nh);
            let mut buf = alloc::vec![0xFF1A1A2E; screen_w * screen_h];
            let ox = (screen_w - nw) / 2;
            let oy = (screen_h - nh) / 2;
            for y in 0..nh {
                for x in 0..nw {
                    buf[(oy + y) * screen_w + (ox + x)] = scaled[y * nw + x];
                }
            }
            buf
        }
        WallpaperMode::Fill => {
            let scale_x = screen_w as f32 / img.width as f32;
            let scale_y = screen_h as f32 / img.height as f32;
            let scale = if scale_x > scale_y { scale_x } else { scale_y };
            let nw = (img.width as f32 * scale) as usize;
            let nh = (img.height as f32 * scale) as usize;
            let scaled = scale_image(img, nw, nh);
            let ox = nw.saturating_sub(screen_w) / 2;
            let oy = nh.saturating_sub(screen_h) / 2;
            let mut buf = Vec::with_capacity(screen_w * screen_h);
            for y in 0..screen_h {
                for x in 0..screen_w {
                    let sx = ox + x;
                    let sy = oy + y;
                    if sx < nw && sy < nh {
                        buf.push(scaled[sy * nw + sx]);
                    } else {
                        buf.push(0xFF1A1A2E);
                    }
                }
            }
            buf
        }
    }
}

/// Load a wallpaper from a VFS path
pub fn load_from_path(path: &str, screen_w: usize, screen_h: usize) -> bool {
    // Try to read the file from VFS
    if let Ok(data) = crate::file_manager::read_file(path) {
        if let Some(mut img) = parse_bmp(&data) {
            img.path = String::from(path);
            let mut state = STATE.lock();
            let scaled = apply_mode(&img, state.mode, screen_w, screen_h);
            state.scaled_cache = scaled;
            state.cache_width = screen_w;
            state.cache_height = screen_h;
            state.current_image = Some(img);
            LOAD_COUNT.fetch_add(1, Ordering::Relaxed);
            crate::serial_println!("[custom_wallpaper] Loaded: {}", path);
            return true;
        }
    }
    false
}

/// Set the scaling mode
pub fn set_mode(mode: WallpaperMode) {
    STATE.lock().mode = mode;
}

/// Get the pre-scaled wallpaper buffer (if loaded)
pub fn get_scaled_buffer() -> Option<Vec<u32>> {
    let state = STATE.lock();
    if state.scaled_cache.is_empty() {
        None
    } else {
        Some(state.scaled_cache.clone())
    }
}

/// Check if a custom wallpaper is loaded
pub fn has_custom_wallpaper() -> bool {
    STATE.lock().current_image.is_some()
}

/// Initialize the custom wallpaper subsystem
pub fn init() {
    crate::serial_println!("[custom_wallpaper] Custom wallpaper subsystem initialized");
}

// ═══════════════════════════════════════════════════════════════════════
// WALLPAPER SLIDESHOW / ROTATION
// ═══════════════════════════════════════════════════════════════════════

/// Slideshow configuration
pub struct SlideshowConfig {
    /// Ordered list of wallpaper file paths
    pub images: Vec<String>,
    /// Interval between transitions (in seconds)
    pub interval_secs: u64,
    /// Whether to shuffle order
    pub shuffle: bool,
    /// Current index in the slideshow
    pub current_index: usize,
    /// Whether slideshow is active
    pub active: bool,
    /// Last transition timestamp (tick count)
    pub last_transition: u64,
}

lazy_static::lazy_static! {
    static ref SLIDESHOW: Mutex<SlideshowConfig> = Mutex::new(SlideshowConfig {
        images: Vec::new(),
        interval_secs: 300, // 5 minutes default
        shuffle: false,
        current_index: 0,
        active: false,
        last_transition: 0,
    });
}

/// Configure a wallpaper slideshow from a directory
pub fn slideshow_from_dir(dir_path: &str, interval_secs: u64, shuffle: bool) {
    let mut ss = SLIDESHOW.lock();
    ss.images.clear();

    // List image files in directory
    if let Some(entries) = crate::vfs::list_dir_dispatch(dir_path) {
        for entry in entries {
            let name = entry.to_lowercase();
            if name.ends_with(".bmp") || name.ends_with(".png") || name.ends_with(".jpg") {
                let mut full = String::from(dir_path);
                if !full.ends_with('/') {
                    full.push('/');
                }
                full.push_str(&entry);
                ss.images.push(full);
            }
        }
    }

    if shuffle && !ss.images.is_empty() {
        // Simple shuffle using tick-based seed
        let seed = crate::interrupts::get_ticks();
        let len = ss.images.len();
        for i in (1..len).rev() {
            let j = ((seed.wrapping_mul(6364136223846793005).wrapping_add(1)) as usize) % (i + 1);
            ss.images.swap(i, j);
        }
    }

    ss.interval_secs = interval_secs;
    ss.shuffle = shuffle;
    ss.current_index = 0;
    ss.active = !ss.images.is_empty();
    ss.last_transition = crate::interrupts::get_ticks();

    if ss.active {
        let first = ss.images[0].clone();
        drop(ss);
        load_from_path(&first, 1920, 1080);
        crate::serial_println!(
            "[wallpaper] Slideshow started: {} images, {}s interval",
            SLIDESHOW.lock().images.len(),
            interval_secs
        );
    }
}

/// Manually add an image path to the slideshow
pub fn slideshow_add(path: &str) {
    SLIDESHOW.lock().images.push(String::from(path));
}

/// Advance to the next wallpaper in the slideshow
pub fn slideshow_next() {
    let mut ss = SLIDESHOW.lock();
    if ss.images.is_empty() {
        return;
    }

    ss.current_index = (ss.current_index + 1) % ss.images.len();
    let path = ss.images[ss.current_index].clone();
    ss.last_transition = crate::interrupts::get_ticks();
    drop(ss);

    load_from_path(&path, 1920, 1080);
}

/// Go back to the previous wallpaper
pub fn slideshow_prev() {
    let mut ss = SLIDESHOW.lock();
    if ss.images.is_empty() {
        return;
    }

    ss.current_index = if ss.current_index == 0 {
        ss.images.len() - 1
    } else {
        ss.current_index - 1
    };
    let path = ss.images[ss.current_index].clone();
    ss.last_transition = crate::interrupts::get_ticks();
    drop(ss);

    load_from_path(&path, 1920, 1080);
}

/// Stop the slideshow
pub fn slideshow_stop() {
    SLIDESHOW.lock().active = false;
}

/// Check if slideshow should advance (call from periodic tick)
pub fn slideshow_tick() {
    let ss = SLIDESHOW.lock();
    if !ss.active || ss.images.is_empty() {
        return;
    }

    let now = crate::interrupts::get_ticks();
    // ~18 ticks per second
    let elapsed_secs = (now.wrapping_sub(ss.last_transition)) / 18;
    if elapsed_secs >= ss.interval_secs {
        drop(ss);
        slideshow_next();
    }
}

/// Get slideshow status
pub fn slideshow_status() -> (bool, usize, usize, u64) {
    let ss = SLIDESHOW.lock();
    (
        ss.active,
        ss.current_index,
        ss.images.len(),
        ss.interval_secs,
    )
}
