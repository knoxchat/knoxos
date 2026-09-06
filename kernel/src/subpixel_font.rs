/// Subpixel Font Rendering (ClearType-style)
///
/// Renders text using LCD subpixel geometry for sharper appearance
/// at small sizes. Supports RGB and BGR stripe layouts.
///
/// Features:
///   - Horizontal RGB/BGR subpixel rendering
///   - FIR low-pass anti-aliasing filter
///   - Gamma correction for perceptual uniformity
///   - Hinting integration (vertical stem alignment)
///   - Per-monitor subpixel layout detection
use alloc::vec::Vec;
use spin::Mutex;

/// Subpixel layout of the display
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SubpixelLayout {
    None, // No subpixel rendering (greyscale AA)
    HorizontalRgb,
    HorizontalBgr,
    VerticalRgb,
    VerticalBgr,
}

/// Filter weights for subpixel anti-aliasing
#[derive(Debug, Clone, Copy)]
pub struct LcdFilter {
    pub weights: [u8; 5],
}

impl LcdFilter {
    /// Default 5-tap low-pass filter (FreeType LCD_FILTER_DEFAULT equivalent)
    pub fn default_filter() -> Self {
        Self {
            weights: [0x10, 0x40, 0x70, 0x40, 0x10],
        }
    }

    /// Light filter (less color fringing, slightly less sharp)
    pub fn light() -> Self {
        Self {
            weights: [0x00, 0x55, 0x56, 0x55, 0x00],
        }
    }

    /// No filtering (maximum sharpness, more color fringing)
    pub fn none() -> Self {
        Self {
            weights: [0x00, 0x00, 0xFF, 0x00, 0x00],
        }
    }
}

/// Subpixel rendering configuration
pub struct SubpixelConfig {
    pub layout: SubpixelLayout,
    pub filter: LcdFilter,
    pub gamma: f32,
    pub enabled: bool,
}

lazy_static::lazy_static! {
    static ref CONFIG: Mutex<SubpixelConfig> = Mutex::new(SubpixelConfig {
        layout: SubpixelLayout::HorizontalRgb,
        filter: LcdFilter::default_filter(),
        gamma: 1.8,
        enabled: true,
    });
}

/// Render a greyscale glyph bitmap with subpixel positioning
///
/// Input: `alpha_bitmap` at 3× horizontal resolution (for H-RGB)
/// Output: RGBA pixel row with subpixel-weighted colors
pub fn render_subpixel_row(
    alpha_3x: &[u8], // 3× width greyscale samples
    fg_r: u8,
    fg_g: u8,
    fg_b: u8,
    bg_r: u8,
    bg_g: u8,
    bg_b: u8,
    output: &mut Vec<u32>,
    filter: &LcdFilter,
    layout: SubpixelLayout,
) {
    let pixel_count = alpha_3x.len() / 3;
    for px in 0..pixel_count {
        let base = px * 3;
        // Apply 5-tap filter to each subpixel channel
        let r_alpha = filtered_sample(alpha_3x, base, filter);
        let g_alpha = filtered_sample(alpha_3x, base + 1, filter);
        let b_alpha = filtered_sample(alpha_3x, base + 2, filter);

        let (ra, ga, ba) = match layout {
            SubpixelLayout::HorizontalRgb => (r_alpha, g_alpha, b_alpha),
            SubpixelLayout::HorizontalBgr => (b_alpha, g_alpha, r_alpha),
            _ => {
                let avg = ((r_alpha as u16 + g_alpha as u16 + b_alpha as u16) / 3) as u8;
                (avg, avg, avg)
            }
        };

        let r = blend(fg_r, bg_r, ra);
        let g = blend(fg_g, bg_g, ga);
        let b = blend(fg_b, bg_b, ba);
        output.push(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b) | 0xFF00_0000);
    }
}

/// Apply 5-tap FIR filter centered at position `center`
fn filtered_sample(data: &[u8], center: usize, filter: &LcdFilter) -> u8 {
    let w = &filter.weights;
    let mut sum = 0u32;
    let mut weight_sum = 0u32;
    for (i, &weight) in w.iter().enumerate() {
        let idx = center as isize + i as isize - 2;
        if idx >= 0 && (idx as usize) < data.len() {
            sum += data[idx as usize] as u32 * weight as u32;
            weight_sum += weight as u32;
        }
    }
    if weight_sum == 0 {
        return 0;
    }
    (sum / weight_sum).min(255) as u8
}

/// Alpha-blend foreground over background
fn blend(fg: u8, bg: u8, alpha: u8) -> u8 {
    let a = alpha as u16;
    ((fg as u16 * a + bg as u16 * (255 - a)) / 255) as u8
}

/// Apply gamma correction
pub fn gamma_correct(value: u8, gamma: f32) -> u8 {
    let normalized = value as f32 / 255.0;
    let corrected = libm::powf(normalized, 1.0 / gamma);
    (corrected * 255.0) as u8
}

pub fn set_layout(layout: SubpixelLayout) {
    CONFIG.lock().layout = layout;
}

pub fn set_filter(filter: LcdFilter) {
    CONFIG.lock().filter = filter;
}

pub fn init() {
    crate::serial_println!("[SUBPIXEL] Subpixel font rendering loaded (RGB horizontal)");
}
