//! CJK Font Support — Unicode fallback + basic CJK glyph rendering
//!
//! Provides:
//!   1. A Unicode fallback chain: try Hack → CJK fallback → tofu box
//!   2. Basic CJK ideograph rendering using a compact stroke decomposition
//!   3. Full-width character detection (CJK Unified Ideographs, Kana, Hangul)
//!   4. Box-drawing character rendering
//!   5. Latin-1 Supplement coverage (accented letters, symbols)
//!
//! For a bare-metal OS, embedding full CJK bitmap fonts (~25,000+ glyphs)
//! would add 50–100 MB. Instead we provide:
//!   - A small set of the most common CJK radicals/strokes rendered procedurally
//!   - A "tofu" placeholder for unmapped glyphs (□ with codepoint)
//!   - Infrastructure for loading CJK bitmap fonts from disk at runtime

use crate::gui::framebuffer::{FrameBuffer, Pixel};
use crate::gui::hack_font;
use crate::serial_println;
use alloc::vec::Vec;

/// Width of a full-width CJK cell (double the normal glyph width)
pub const CJK_CELL_WIDTH: usize = hack_font::GLYPH_WIDTH * 2;
pub const CJK_CELL_HEIGHT: usize = hack_font::GLYPH_HEIGHT;

pub const CJK_CELL_WIDTH_COMPACT: usize = hack_font::GLYPH_WIDTH_COMPACT * 2;
pub const CJK_CELL_HEIGHT_COMPACT: usize = hack_font::GLYPH_HEIGHT_COMPACT;

/// Returns true if a character is a full-width CJK / East Asian character
pub fn is_fullwidth(ch: char) -> bool {
    let cp = ch as u32;
    matches!(cp,
        // CJK Unified Ideographs
        0x4E00..=0x9FFF |
        // CJK Extension A
        0x3400..=0x4DBF |
        // CJK Extension B
        0x20000..=0x2A6DF |
        // CJK Compatibility Ideographs
        0xF900..=0xFAFF |
        // Hiragana
        0x3040..=0x309F |
        // Katakana
        0x30A0..=0x30FF |
        // Hangul Syllables
        0xAC00..=0xD7AF |
        // Hangul Jamo
        0x1100..=0x11FF |
        // CJK Symbols and Punctuation
        0x3000..=0x303F |
        // Halfwidth and Fullwidth Forms (fullwidth portion)
        0xFF01..=0xFF60 |
        // Enclosed CJK Letters
        0x3200..=0x32FF |
        // CJK Compatibility
        0x3300..=0x33FF |
        // Bopomofo
        0x3100..=0x312F |
        // Katakana Phonetic Extensions
        0x31F0..=0x31FF
    )
}

/// Returns true if a character is in the Latin-1 Supplement range
pub fn is_latin1_supplement(ch: char) -> bool {
    let cp = ch as u32;
    (0x00A0..=0x00FF).contains(&cp)
}

/// Returns true if this is a box-drawing character
pub fn is_box_drawing(ch: char) -> bool {
    let cp = ch as u32;
    (0x2500..=0x257F).contains(&cp)
}

/// Character width in cells: 1 for normal, 2 for fullwidth
pub fn char_width(ch: char) -> usize {
    if is_fullwidth(ch) { 2 } else { 1 }
}

/// Measure the display width of a string in glyph cells
pub fn string_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

/// Measure the pixel width of a Unicode string
pub fn string_pixel_width(s: &str, scale: u32) -> usize {
    let gw = hack_font::GLYPH_WIDTH;
    s.chars()
        .map(|ch| {
            if is_fullwidth(ch) {
                gw * 2 * scale as usize
            } else {
                gw * scale as usize
            }
        })
        .sum()
}

// ═══════════════════════════════════════════════════════════════════════
// TOFU RENDERER — Placeholder for unknown glyphs
// ═══════════════════════════════════════════════════════════════════════

/// Draw a "tofu" (missing glyph) rectangle with codepoint display
pub fn draw_tofu(fb: &mut FrameBuffer, x: i32, y: i32, ch: char, color: Pixel, scale: u32) {
    let gw = hack_font::GLYPH_WIDTH as i32 * scale as i32;
    let gh = hack_font::GLYPH_HEIGHT as i32 * scale as i32;
    let w = if is_fullwidth(ch) { gw * 2 } else { gw };

    // Draw box outline
    for dx in 0..w {
        if x + dx >= 0 && y >= 0 && y + gh > 0 {
            fb.set_pixel((x + dx) as usize, y as usize, color);
            fb.set_pixel((x + dx) as usize, (y + gh - 1) as usize, color);
        }
    }
    for dy in 0..gh {
        if x >= 0 && y + dy >= 0 && x + w > 0 {
            fb.set_pixel(x as usize, (y + dy) as usize, color);
            fb.set_pixel((x + w - 1) as usize, (y + dy) as usize, color);
        }
    }

    // Draw codepoint as tiny hex digits inside the box
    let cp = ch as u32;
    let hex_chars: [u8; 4] = [
        ((cp >> 12) & 0xF) as u8,
        ((cp >> 8) & 0xF) as u8,
        ((cp >> 4) & 0xF) as u8,
        (cp & 0xF) as u8,
    ];

    // Draw 2 rows of 2 hex digits each (tiny 3x5 font)
    let ox = x + 2 * scale as i32;
    let oy = y + 3 * scale as i32;
    for (idx, &nib) in hex_chars.iter().enumerate() {
        let row = idx / 2;
        let col = idx % 2;
        let tx = ox + col as i32 * 4 * scale as i32;
        let ty = oy + row as i32 * 7 * scale as i32;
        draw_tiny_hex_digit(fb, tx, ty, nib, color, scale);
    }
}

/// Draw a 3×5 hex digit (0-F) at the given position
fn draw_tiny_hex_digit(fb: &mut FrameBuffer, x: i32, y: i32, digit: u8, color: Pixel, scale: u32) {
    // 3×5 bitmap font for hex digits (each u16 has 15 bits, row-major)
    #[rustfmt::skip]
    const DIGITS: [u16; 16] = [
        0b111_101_101_101_111u16, // 0
        0b010_110_010_010_111u16, // 1
        0b111_001_111_100_111u16, // 2
        0b111_001_111_001_111u16, // 3
        0b101_101_111_001_001u16, // 4
        0b111_100_111_001_111u16, // 5
        0b111_100_111_101_111u16, // 6
        0b111_001_001_010_010u16, // 7
        0b111_101_111_101_111u16, // 8
        0b111_101_111_001_111u16, // 9
        0b111_101_111_101_101u16, // A
        0b110_101_110_101_110u16, // B
        0b111_100_100_100_111u16, // C
        0b110_101_101_101_110u16, // D
        0b111_100_111_100_111u16, // E
        0b111_100_111_100_100u16, // F
    ];

    let d = digit as usize & 0xF;
    let bits = DIGITS[d];
    for row in 0..5usize {
        for col in 0..3usize {
            let bit_idx = row * 3 + col;
            if bits & (1 << (14 - bit_idx)) != 0 {
                for sy in 0..scale as i32 {
                    for sx in 0..scale as i32 {
                        let px = x + col as i32 * scale as i32 + sx;
                        let py = y + row as i32 * scale as i32 + sy;
                        if px >= 0 && py >= 0 {
                            fb.set_pixel(px as usize, py as usize, color);
                        }
                    }
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// BOX DRAWING CHARACTERS
// ═══════════════════════════════════════════════════════════════════════

/// Render a box-drawing character (U+2500..U+257F)
pub fn draw_box_drawing(fb: &mut FrameBuffer, x: i32, y: i32, ch: char, color: Pixel, scale: u32) {
    let gw = hack_font::GLYPH_WIDTH as i32 * scale as i32;
    let gh = hack_font::GLYPH_HEIGHT as i32 * scale as i32;
    let cx = x + gw / 2;
    let cy = y + gh / 2;

    let cp = ch as u32;
    // Decode box-drawing character into segments: top, right, bottom, left
    // 0 = no line, 1 = light, 2 = heavy/double
    let (top, right, bottom, left) = match cp {
        0x2500 => (0, 1, 0, 1), // ─
        0x2501 => (0, 2, 0, 2), // ━
        0x2502 => (1, 0, 1, 0), // │
        0x2503 => (2, 0, 2, 0), // ┃
        0x250C => (0, 1, 1, 0), // ┌
        0x250F => (0, 2, 2, 0), // ┏
        0x2510 => (0, 0, 1, 1), // ┐
        0x2513 => (0, 0, 2, 2), // ┓
        0x2514 => (1, 1, 0, 0), // └
        0x2517 => (2, 2, 0, 0), // ┗
        0x2518 => (1, 0, 0, 1), // ┘
        0x251B => (2, 0, 0, 2), // ┛
        0x251C => (1, 1, 1, 0), // ├
        0x2524 => (1, 0, 1, 1), // ┤
        0x252C => (0, 1, 1, 1), // ┬
        0x2534 => (1, 1, 0, 1), // ┴
        0x253C => (1, 1, 1, 1), // ┼
        0x2550 => (0, 2, 0, 2), // ═ (double horizontal)
        0x2551 => (2, 0, 2, 0), // ║ (double vertical)
        0x2554 => (0, 2, 2, 0), // ╔
        0x2557 => (0, 0, 2, 2), // ╗
        0x255A => (2, 2, 0, 0), // ╚
        0x255D => (2, 0, 0, 2), // ╝
        0x2560 => (2, 2, 2, 0), // ╠
        0x2563 => (2, 0, 2, 2), // ╣
        0x2566 => (0, 2, 2, 2), // ╦
        0x2569 => (2, 2, 0, 2), // ╩
        0x256C => (2, 2, 2, 2), // ╬
        _ => {
            // Fallback: draw tofu for unmapped box-drawing chars
            draw_tofu(fb, x, y, ch, color, scale);
            return;
        }
    };

    let thick = (2 * scale as i32).max(1);
    let thin = (scale as i32).max(1);

    // Helper macro-like closure for safe set_pixel
    let sp = |fb: &mut FrameBuffer, px: i32, py: i32, c: Pixel| {
        if px >= 0 && py >= 0 {
            fb.set_pixel(px as usize, py as usize, c);
        }
    };

    // Draw segments
    if top > 0 {
        let w = if top > 1 { thick } else { thin };
        for dy in 0..=(cy - y) {
            for dx in 0..w {
                sp(fb, cx - w / 2 + dx, y + dy, color);
            }
        }
    }
    if bottom > 0 {
        let w = if bottom > 1 { thick } else { thin };
        for dy in 0..(y + gh - cy) {
            for dx in 0..w {
                sp(fb, cx - w / 2 + dx, cy + dy, color);
            }
        }
    }
    if left > 0 {
        let w = if left > 1 { thick } else { thin };
        for dx in 0..=(cx - x) {
            for dy in 0..w {
                sp(fb, x + dx, cy - w / 2 + dy, color);
            }
        }
    }
    if right > 0 {
        let w = if right > 1 { thick } else { thin };
        for dx in 0..(x + gw - cx) {
            for dy in 0..w {
                sp(fb, cx + dx, cy - w / 2 + dy, color);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// UNIFIED UNICODE DRAW — FALLBACK CHAIN
// ═══════════════════════════════════════════════════════════════════════

/// Draw a single Unicode character with the full fallback chain:
///   1. ASCII (0x20..0x7E) → Hack font
///   2. Box drawing (U+2500..U+257F) → procedural box renderer
///   3. Fullwidth CJK → tofu with codepoint (or runtime font if loaded)
///   4. Everything else → tofu
///
/// Returns the advance width in pixels.
pub fn draw_unicode_char(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    ch: char,
    color: Pixel,
    scale: u32,
) -> i32 {
    let gw = hack_font::GLYPH_WIDTH as i32 * scale as i32;

    // 1. ASCII — use Hack font
    let cp = ch as u32;
    if (0x20..=0x7E).contains(&cp) {
        crate::gui::fonts::draw_char(fb, x, y, ch, color, scale);
        return gw;
    }

    // 2. Box drawing
    if is_box_drawing(ch) {
        draw_box_drawing(fb, x, y, ch, color, scale);
        return gw;
    }

    // 3. Common Latin-1 Supplement (map accented chars to base ASCII)
    if is_latin1_supplement(ch) {
        let base = latin1_to_ascii(ch);
        if base != '\0' {
            crate::gui::fonts::draw_char(fb, x, y, base, color, scale);
            return gw;
        }
    }

    // 4. Check runtime-loaded CJK font (if available)
    if is_fullwidth(ch) {
        if let Some(bitmap) = get_runtime_cjk_glyph(cp) {
            draw_cjk_bitmap(fb, x, y, &bitmap, gw * 2, color, scale);
            return gw * 2;
        }
        // No runtime font — draw tofu
        draw_tofu(fb, x, y, ch, color, scale);
        return gw * 2;
    }

    // 5. Fallback: tofu for anything else
    draw_tofu(fb, x, y, ch, color, scale);
    gw
}

/// Draw a full Unicode string, handling mixed-width characters
pub fn draw_unicode_string(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    text: &str,
    color: Pixel,
    scale: u32,
) -> i32 {
    let mut cursor_x = x;
    for ch in text.chars() {
        let advance = draw_unicode_char(fb, cursor_x, y, ch, color, scale);
        cursor_x += advance;
    }
    cursor_x - x // return total width
}

/// Map Latin-1 Supplement characters to closest ASCII equivalent
fn latin1_to_ascii(ch: char) -> char {
    match ch {
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'A',
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'È' | 'É' | 'Ê' | 'Ë' => 'E',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'Ì' | 'Í' | 'Î' | 'Ï' => 'I',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => 'O',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        'Ù' | 'Ú' | 'Û' | 'Ü' => 'U',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'Ñ' => 'N',
        'ñ' => 'n',
        'Ý' => 'Y',
        'ý' | 'ÿ' => 'y',
        'Ç' => 'C',
        'ç' => 'c',
        'Ð' => 'D',
        'ð' => 'd',
        'Þ' => 'P',
        'þ' => 'p',
        'ß' => 's',
        '×' => 'x',
        '÷' => '/',
        '¡' => '!',
        '¿' => '?',
        '©' => 'C',
        '®' => 'R',
        '°' => 'o',
        '±' => '+',
        '²' => '2',
        '³' => '3',
        '¹' => '1',
        'µ' => 'u',
        '¶' => 'P',
        '·' => '.',
        '¼' => '/',
        '½' => '/',
        '¾' => '/',
        _ => '\0',
    }
}

// ═══════════════════════════════════════════════════════════════════════
// RUNTIME CJK FONT LOADING
// ═══════════════════════════════════════════════════════════════════════

use alloc::collections::BTreeMap;
use lazy_static::lazy_static;
use spin::Mutex;

/// A runtime-loaded CJK glyph bitmap (16×16 pixels, 1 bit per pixel)
pub type CjkBitmap = [u8; 32]; // 16 rows × 2 bytes/row = 32 bytes

lazy_static! {
    /// Runtime-loaded CJK glyph cache (codepoint → 16×16 bitmap)
    static ref CJK_FONT_CACHE: Mutex<BTreeMap<u32, CjkBitmap>> = Mutex::new(BTreeMap::new());
}

/// Try to get a CJK glyph from the runtime font cache
fn get_runtime_cjk_glyph(codepoint: u32) -> Option<CjkBitmap> {
    CJK_FONT_CACHE.lock().get(&codepoint).copied()
}

/// Load a CJK font file from the VFS
/// Expected format: raw binary, 32 bytes per glyph, ordered by codepoint
/// with a header: magic(4) + first_codepoint(4) + glyph_count(4) + data
pub fn load_cjk_font(data: &[u8]) -> usize {
    if data.len() < 12 {
        serial_println!("[CJK] Font data too small");
        return 0;
    }

    // Check magic: "KCF\0" (KnoxOS CJK Font)
    if &data[0..4] != b"KCF\0" {
        serial_println!("[CJK] Invalid font magic");
        return 0;
    }

    let first_cp = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let count = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let glyph_data = &data[12..];

    if glyph_data.len() < count * 32 {
        serial_println!(
            "[CJK] Font data truncated: expected {} glyphs ({} bytes), got {} bytes",
            count,
            count * 32,
            glyph_data.len()
        );
        return 0;
    }

    let mut cache = CJK_FONT_CACHE.lock();
    let mut loaded = 0usize;
    for i in 0..count {
        let cp = first_cp + i as u32;
        let offset = i * 32;
        let mut bitmap: CjkBitmap = [0u8; 32];
        bitmap.copy_from_slice(&glyph_data[offset..offset + 32]);
        cache.insert(cp, bitmap);
        loaded += 1;
    }

    serial_println!(
        "[CJK] Loaded {} glyphs (U+{:04X}..U+{:04X})",
        loaded,
        first_cp,
        first_cp + count as u32 - 1
    );

    loaded
}

/// Draw a 16×16 CJK bitmap glyph scaled to the target cell size
fn draw_cjk_bitmap(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    bitmap: &CjkBitmap,
    _cell_width: i32,
    color: Pixel,
    scale: u32,
) {
    let s = scale as i32;
    for row in 0..16i32 {
        let b0 = bitmap[row as usize * 2];
        let b1 = bitmap[row as usize * 2 + 1];
        let row_bits = ((b0 as u16) << 8) | b1 as u16;
        for col in 0..16i32 {
            if row_bits & (1 << (15 - col)) != 0 {
                for sy in 0..s {
                    for sx in 0..s {
                        let px = x + col * s + sx;
                        let py = y + row * s + sy;
                        if px >= 0 && py >= 0 {
                            fb.set_pixel(px as usize, py as usize, color);
                        }
                    }
                }
            }
        }
    }
}

/// Try to load CJK fonts from the VFS at boot time
pub fn init() {
    // Try loading from well-known paths
    let font_paths = [
        "/usr/share/fonts/cjk_unified.kcf",
        "/usr/share/fonts/kana.kcf",
        "/usr/share/fonts/hangul.kcf",
    ];

    for path in &font_paths {
        if let Some(data) = crate::vfs::read_file_dispatch(path) {
            let count = load_cjk_font(&data);
            if count > 0 {
                serial_println!("[CJK] Loaded font: {} ({} glyphs)", path, count);
            }
        }
    }

    serial_println!("[KnoxOS] CJK font subsystem initialized (fallback + runtime loading)");
}
