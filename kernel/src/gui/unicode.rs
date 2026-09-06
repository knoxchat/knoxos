use super::framebuffer::{FrameBuffer, Pixel};
/// Unicode & Emoji Rendering Module
///
/// Provides Unicode-aware text handling and emoji rendering support:
///   - Unicode character classification (letter, digit, emoji, CJK, etc.)
///   - Grapheme cluster segmentation (combining marks, ZWJ sequences)
///   - Emoji color bitmap rendering (placeholder bitmaps for common emoji)
///   - Fallback glyph rendering for characters outside the bitmap font range
///
/// Emoji are rendered as small 16×16 or 20×20 color bitmaps at standard
/// text positions. Missing glyphs show a replacement character (U+FFFD).
use alloc::vec;
use alloc::vec::Vec;

// ─── Unicode Categories ──────────────────────────────────────────────

/// Check if a codepoint is in the Emoji range
pub fn is_emoji(cp: u32) -> bool {
    matches!(cp,
        0x1F600..=0x1F64F |  // Emoticons
        0x1F300..=0x1F5FF |  // Misc Symbols & Pictographs
        0x1F680..=0x1F6FF |  // Transport & Map
        0x1F700..=0x1F77F |  // Alchemical
        0x1F780..=0x1F7FF |  // Geometric Shapes Extended
        0x1F900..=0x1F9FF |  // Supplemental Symbols
        0x1FA00..=0x1FA6F |  // Chess Symbols
        0x1FA70..=0x1FAFF |  // Symbols Extended-A
        0x2600..=0x26FF   |  // Misc Symbols
        0x2700..=0x27BF   |  // Dingbats
        0xFE00..=0xFE0F   |  // Variation selectors
        0x200D            |  // Zero-width joiner
        0x20E3            |  // Combining enclosing keycap
        0xE0020..=0xE007F    // Tags
    )
}

/// Check if a codepoint is CJK (Chinese/Japanese/Korean)
pub fn is_cjk(cp: u32) -> bool {
    matches!(cp,
        0x4E00..=0x9FFF   |  // CJK Unified Ideographs
        0x3400..=0x4DBF   |  // CJK Extension A
        0x20000..=0x2A6DF |  // CJK Extension B
        0x2A700..=0x2B73F |  // CJK Extension C
        0x2B740..=0x2B81F |  // CJK Extension D
        0x3000..=0x303F   |  // CJK Symbols & Punctuation
        0x3040..=0x309F   |  // Hiragana
        0x30A0..=0x30FF   |  // Katakana
        0xAC00..=0xD7AF   |  // Hangul Syllables
        0xFF00..=0xFFEF      // Halfwidth & Fullwidth Forms
    )
}

/// Check if a codepoint is a combining mark
pub fn is_combining_mark(cp: u32) -> bool {
    matches!(cp,
        0x0300..=0x036F |  // Combining Diacritical Marks
        0x1AB0..=0x1AFF |  // Combining Diacritical Marks Extended
        0x1DC0..=0x1DFF |  // Combining Diacritical Marks Supplement
        0x20D0..=0x20FF |  // Combining Diacritical Marks for Symbols
        0xFE20..=0xFE2F    // Combining Half Marks
    )
}

/// Check if a codepoint is RTL (Arabic, Hebrew, etc.)
pub fn is_rtl(cp: u32) -> bool {
    matches!(cp,
        0x0590..=0x05FF |  // Hebrew
        0x0600..=0x06FF |  // Arabic
        0x0700..=0x074F |  // Syriac
        0x0750..=0x077F |  // Arabic Supplement
        0x0780..=0x07BF |  // Thaana
        0x08A0..=0x08FF |  // Arabic Extended-A
        0xFB50..=0xFDFF |  // Arabic Presentation Forms-A
        0xFE70..=0xFEFF    // Arabic Presentation Forms-B
    )
}

/// Estimated display width of a character (1 for narrow, 2 for wide)
pub fn char_display_width(cp: u32) -> usize {
    if is_cjk(cp) || is_emoji(cp) {
        2
    } else if is_combining_mark(cp) {
        0
    } else {
        1
    }
}

/// Calculate display width of a string
pub fn string_display_width(text: &str) -> usize {
    text.chars().map(|c| char_display_width(c as u32)).sum()
}

// ─── Grapheme Cluster Iteration ──────────────────────────────────────

/// A grapheme cluster — one or more Unicode codepoints that form a
/// single user-perceived character (e.g., base + combining marks, or
/// emoji ZWJ sequence).
pub struct GraphemeCluster {
    pub chars: Vec<char>,
    pub display_width: usize,
}

/// Segment text into grapheme clusters (simplified)
pub fn grapheme_clusters(text: &str) -> Vec<GraphemeCluster> {
    let mut clusters = Vec::new();
    let mut iter = text.chars().peekable();

    while let Some(ch) = iter.next() {
        let mut chars = vec![ch];
        let mut width = char_display_width(ch as u32);

        // Absorb combining marks and variation selectors
        while let Some(&next) = iter.peek() {
            let ncp = next as u32;
            if is_combining_mark(ncp) || (0xFE00..=0xFE0F).contains(&ncp) || ncp == 0x200D {
                chars.push(iter.next().unwrap());
                if ncp == 0x200D {
                    // ZWJ — absorb the next character too
                    if let Some(joined) = iter.next() {
                        chars.push(joined);
                    }
                }
            } else {
                break;
            }
        }

        // Emoji ZWJ sequences are double-width
        if chars.len() > 1 && chars.iter().any(|c| is_emoji(*c as u32)) {
            width = 2;
        }

        clusters.push(GraphemeCluster {
            chars,
            display_width: width,
        });
    }

    clusters
}

// ─── Emoji Bitmap Rendering ──────────────────────────────────────────

/// Render an emoji at the given position. Returns advance width in pixels.
/// Uses simple 16×16 color bitmaps for common emoji.
pub fn draw_emoji(fb: &mut FrameBuffer, x: usize, y: usize, codepoint: u32) -> usize {
    let bitmap = get_emoji_bitmap(codepoint);
    let size = 16usize;

    for row in 0..size {
        for col in 0..size {
            let px = bitmap[row * size + col];
            if px.a > 0 {
                let fx = x + col;
                let fy = y + row;
                if fx < fb.width && fy < fb.height {
                    fb.blend_pixel(fx, fy, px);
                }
            }
        }
    }
    size + 2 // advance = size + small gap
}

/// Get a 16×16 emoji bitmap for a given codepoint.
/// Returns a solid colored placeholder for known emoji categories.
fn get_emoji_bitmap(codepoint: u32) -> Vec<Pixel> {
    let size = 16 * 16;
    let mut bitmap = vec![Pixel::new(0, 0, 0, 0); size];

    // Determine emoji color category
    let (base_color, shape) = match codepoint {
        // Smiley faces — yellow circle
        0x1F600..=0x1F64F => (Pixel::new(255, 220, 50, 255), EmojiShape::Circle),
        // Hearts — red/pink
        0x2764 | 0x1F493..=0x1F49F => (Pixel::new(255, 50, 80, 255), EmojiShape::Heart),
        // Stars — gold
        0x2B50 | 0x1F31F | 0x2728 => (Pixel::new(255, 215, 0, 255), EmojiShape::Star),
        // Nature — green
        0x1F330..=0x1F37F => (Pixel::new(80, 200, 80, 255), EmojiShape::Circle),
        // Weather — blue/cyan
        0x1F324..=0x1F32B | 0x2600..=0x2602 => (Pixel::new(100, 180, 255, 255), EmojiShape::Circle),
        // Fire — orange
        0x1F525 => (Pixel::new(255, 140, 0, 255), EmojiShape::Flame),
        // Hands/Gestures — skin-tone-ish
        0x1F44D..=0x1F44F | 0x270B | 0x270C | 0x1F91A..=0x1F91F => {
            (Pixel::new(240, 200, 150, 255), EmojiShape::Circle)
        }
        // Default — gray diamond
        _ => (Pixel::new(150, 150, 170, 255), EmojiShape::Diamond),
    };

    draw_emoji_shape(&mut bitmap, 16, base_color, shape);
    bitmap
}

#[derive(Clone, Copy)]
enum EmojiShape {
    Circle,
    Heart,
    Star,
    Diamond,
    Flame,
}

fn draw_emoji_shape(bitmap: &mut [Pixel], size: usize, color: Pixel, shape: EmojiShape) {
    let cx = size / 2;
    let cy = size / 2;
    let r = size as i32 / 2 - 1;

    match shape {
        EmojiShape::Circle => {
            for y in 0..size {
                for x in 0..size {
                    let dx = x as i32 - cx as i32;
                    let dy = y as i32 - cy as i32;
                    if dx * dx + dy * dy <= r * r {
                        bitmap[y * size + x] = color;
                    }
                }
            }
        }
        EmojiShape::Heart => {
            for y in 0..size {
                for x in 0..size {
                    let fx = (x as f32 - cx as f32) / (size as f32 / 2.0);
                    let fy = (y as f32 - cy as f32) / (size as f32 / 2.0);
                    // Heart curve approximation
                    let fy2 = fy + 0.3;
                    let val = fx * fx + fy2 * fy2 - 0.6;
                    if val < fx.abs() * fy2.abs() * 0.5
                        || (fy2 < 0.0 && fx * fx + (fy2 + 0.3) * (fy2 + 0.3) < 0.35)
                    {
                        bitmap[y * size + x] = color;
                    }
                }
            }
        }
        EmojiShape::Star => {
            // Simple 5-pointed star
            for y in 0..size {
                for x in 0..size {
                    let dx = x as i32 - cx as i32;
                    let dy = y as i32 - cy as i32;
                    let dist = libm::sqrtf((dx * dx + dy * dy) as f32);
                    let angle = libm::atan2f(dy as f32, dx as f32);
                    let star_r = r as f32 * (0.5 + 0.5 * libm::cosf(angle * 5.0 / 2.0).abs());
                    if dist < star_r {
                        bitmap[y * size + x] = color;
                    }
                }
            }
        }
        EmojiShape::Diamond => {
            for y in 0..size {
                for x in 0..size {
                    let dx = (x as i32 - cx as i32).unsigned_abs() as i32;
                    let dy = (y as i32 - cy as i32).unsigned_abs() as i32;
                    if dx + dy <= r {
                        bitmap[y * size + x] = color;
                    }
                }
            }
        }
        EmojiShape::Flame => {
            for y in 0..size {
                for x in 0..size {
                    let fx = (x as f32 - cx as f32) / (size as f32 / 3.0);
                    let fy = (y as f32) / size as f32;
                    // Flame shape: wider at bottom, narrow at top
                    let max_width = 1.0 - fy * 0.8;
                    if fx.abs() < max_width && fy > 0.1 {
                        let intensity = 1.0 - fy;
                        let r = (255.0 * intensity.min(1.0)) as u8;
                        let g = (140.0 * intensity * intensity) as u8;
                        bitmap[y * size + x] = Pixel::new(r, g, 0, 255);
                    }
                }
            }
        }
    }
}

/// Replacement character glyph (U+FFFD) — drawn as a diamond with "?"
pub fn draw_replacement_char(fb: &mut FrameBuffer, x: usize, y: usize, color: Pixel) {
    let size = 10usize;
    let cx = size / 2;
    let cy = size / 2;
    // Diamond outline
    for i in 0..=cx {
        let top = cy.saturating_sub(i);
        let bot = cy + i;
        let left = cx - i;
        let right = cx + i;
        if x + left < fb.width && y + top < fb.height {
            fb.set_pixel(x + left, y + top, color);
        }
        if x + right < fb.width && y + top < fb.height {
            fb.set_pixel(x + right, y + top, color);
        }
        if x + left < fb.width && y + bot < fb.height {
            fb.set_pixel(x + left, y + bot, color);
        }
        if x + right < fb.width && y + bot < fb.height {
            fb.set_pixel(x + right, y + bot, color);
        }
    }
}

/// Draw a Unicode-aware string with emoji support.
/// Falls back to the bitmap font for ASCII, renders emoji bitmaps for emoji
/// codepoints, and shows replacement chars for unsupported glyphs.
pub fn draw_unicode_string(
    fb: &mut FrameBuffer,
    x: usize,
    y: usize,
    text: &str,
    color: Pixel,
) -> usize {
    let clusters = grapheme_clusters(text);
    let mut pen_x = x;

    for cluster in &clusters {
        let base_cp = cluster.chars[0] as u32;

        if base_cp < 0x80 {
            // ASCII — use standard bitmap font
            super::fonts::draw_string(
                fb,
                pen_x as i32,
                y as i32,
                &alloc::string::String::from(cluster.chars[0]),
                color,
                1,
            );
            pen_x += 10; // FONT_WIDTH
        } else if is_emoji(base_cp) {
            pen_x += draw_emoji(fb, pen_x, y, base_cp);
        } else if (0x80..0x0300).contains(&base_cp) {
            // Latin Extended — try bitmap font (may show as replacement)
            super::fonts::draw_string(
                fb,
                pen_x as i32,
                y as i32,
                &alloc::string::String::from(cluster.chars[0]),
                color,
                1,
            );
            pen_x += 10;
        } else {
            // Unknown — replacement character
            draw_replacement_char(fb, pen_x, y, color);
            pen_x += 12;
        }
    }

    pen_x - x
}
