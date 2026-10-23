//! Native anti-aliased text via `fontdue` — same path as the `os` project.
//!
//! Embeds the Inter family (six weights) plus Hack Regular for mono.
//! Each `(face, size, glyph)` coverage bitmap is rasterized once and cached.
//! Metrics (ascent, advance, xmin, top) come from fontdue so layout and
//! drawing match `os/kernel/knox-kernel/src/display/font.rs`.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use fontdue::{Font, FontSettings};
use spin::Mutex;

use super::embedded_fonts;

/// Inter weight selector. Maps to the six embedded faces (same as `os`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Weight {
    Light,
    Regular,
    Medium,
    SemiBold,
    Bold,
    ExtraBold,
}

impl Weight {
    #[inline]
    fn index(self) -> usize {
        match self {
            Weight::Light => 0,
            Weight::Regular => 1,
            Weight::Medium => 2,
            Weight::SemiBold => 3,
            Weight::Bold => 4,
            Weight::ExtraBold => 5,
        }
    }

    /// Map a numeric CSS-style weight (100..900) to the nearest embedded face.
    pub fn from_numeric(w: u16) -> Self {
        match w {
            0..=349 => Weight::Light,
            350..=449 => Weight::Regular,
            450..=549 => Weight::Medium,
            550..=649 => Weight::SemiBold,
            650..=749 => Weight::Bold,
            _ => Weight::ExtraBold,
        }
    }
}

/// Raster face: Inter weight or Hack mono.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Face {
    Inter(Weight),
    Mono,
}

impl Face {
    #[inline]
    fn index(self) -> usize {
        match self {
            Face::Inter(w) => w.index(),
            Face::Mono => 6,
        }
    }
}

/// Cached glyph (fontdue metrics, KnoxOS + `os` field names).
pub struct Glyph {
    pub width: usize,
    pub height: usize,
    /// Horizontal offset from the pen position to the bitmap's left edge.
    pub xmin: i32,
    /// Vertical offset from the baseline to the bitmap's top edge
    /// (screen coordinates, positive = downward). Same as `os` `Glyph.top`.
    pub top: i32,
    /// Y-up bearing (distance from baseline to bitmap top).
    pub bearing_y: i16,
    /// Horizontal advance to the next pen position.
    pub advance: f32,
    /// Row-major 8-bit coverage (alpha) values.
    pub coverage: Vec<u8>,
}

struct FontSet {
    faces: Vec<Font>,
    /// (face << 48) | (size_px << 32) | codepoint  ->  rasterized glyph.
    cache: BTreeMap<u64, Glyph>,
    /// (face << 16) | size_px  ->  rounded ascent in pixels.
    ascent_cache: BTreeMap<u32, i32>,
}

static FONTS: Mutex<Option<FontSet>> = Mutex::new(None);

fn load_face(bytes: &[u8]) -> Option<Font> {
    Font::from_bytes(bytes, FontSettings::default()).ok()
}

/// Initialize the embedded Inter + Hack faces. Safe to call more than once.
pub fn init() {
    let mut guard = FONTS.lock();
    if guard.is_some() {
        return;
    }
    let sources: [(&[u8], &str); 7] = [
        (embedded_fonts::INTER_LIGHT, "Inter Light"),
        (embedded_fonts::INTER_REGULAR, "Inter Regular"),
        (embedded_fonts::INTER_MEDIUM, "Inter Medium"),
        (embedded_fonts::INTER_SEMIBOLD, "Inter SemiBold"),
        (embedded_fonts::INTER_BOLD, "Inter Bold"),
        (embedded_fonts::INTER_EXTRABOLD, "Inter ExtraBold"),
        (embedded_fonts::FONT_MONO_DATA, "Hack Regular"),
    ];
    let mut faces = Vec::with_capacity(7);
    for (bytes, name) in sources {
        match load_face(bytes) {
            Some(font) => faces.push(font),
            None => {
                crate::serial_println!("[fonts] {} failed to parse", name);
                return;
            }
        }
    }
    *guard = Some(FontSet {
        faces,
        cache: BTreeMap::new(),
        ascent_cache: BTreeMap::new(),
    });
    crate::serial_println!("[OK] Native Inter text renderer initialized (6 faces + Hack)");
}

pub fn is_ready() -> bool {
    FONTS.lock().is_some()
}

#[inline]
fn clamp_size(px: u16) -> u16 {
    px.clamp(1, 255)
}

#[inline]
fn glyph_key(face: Face, size: u16, ch: char) -> u64 {
    ((face.index() as u64) << 48) | ((size as u64) << 32) | (ch as u64)
}

#[inline]
fn ascent_key(face: Face, size: u16) -> u32 {
    ((face.index() as u32) << 16) | size as u32
}

impl FontSet {
    fn ascent(&mut self, face: Face, size: u16) -> i32 {
        let key = ascent_key(face, size);
        if let Some(a) = self.ascent_cache.get(&key) {
            return *a;
        }
        let idx = face.index();
        let ascent = self
            .faces
            .get(idx)
            .and_then(|f| f.horizontal_line_metrics(size as f32))
            .map(|m| libm::roundf(m.ascent) as i32)
            .unwrap_or((size as i32 * 4) / 5);
        self.ascent_cache.insert(key, ascent);
        ascent
    }

    fn glyph(&mut self, face: Face, size: u16, ch: char) -> &Glyph {
        let key = glyph_key(face, size, ch);
        if !self.cache.contains_key(&key) {
            let idx = face.index();
            let (metrics, mut coverage) = if size <= 20 {
                // 2× then box-filter: unhinted Inter bowls (a/o/e/s) drop out
                // of a 14–16px grid on dark chrome. Advance stays on the 1×
                // metric so buttons do not grow.
                let adv = self.faces[idx].metrics(ch, size as f32).advance_width;
                let (hi, cov) = self.faces[idx].rasterize(ch, size as f32 * 2.0);
                let (mut m, cov) = downsample_2x(hi, cov);
                m.advance_width = adv;
                (m, cov)
            } else {
                self.faces[idx].rasterize(ch, size as f32)
            };
            strengthen_coverage(&mut coverage, size);
            // Top of bitmap relative to the baseline, in screen coords:
            //   font-space top above baseline = ymin + height
            //   screen y grows downward, so top = -(ymin + height)
            let bearing_y = metrics.ymin + metrics.height as i32;
            let top = -bearing_y;
            let glyph = Glyph {
                width: metrics.width,
                height: metrics.height,
                xmin: metrics.xmin,
                top,
                bearing_y: bearing_y as i16,
                advance: metrics.advance_width,
                coverage,
            };
            self.cache.insert(key, glyph);
        }
        self.cache.get(&key).unwrap()
    }
}

/// Average a 2× fontdue bitmap back to em size. Metrics are halved so
/// `walk_text` / GPUI placement stay on the requested pixel grid.
fn downsample_2x(
    metrics: fontdue::Metrics,
    src: alloc::vec::Vec<u8>,
) -> (fontdue::Metrics, alloc::vec::Vec<u8>) {
    let w2 = metrics.width;
    let h2 = metrics.height;
    if w2 == 0 || h2 == 0 {
        let mut m = metrics;
        m.advance_width *= 0.5;
        m.xmin /= 2;
        m.ymin /= 2;
        return (m, src);
    }
    let w = w2.div_ceil(2);
    let h = h2.div_ceil(2);
    let mut out = alloc::vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0u32;
            let mut n = 0u32;
            for dy in 0..2 {
                for dx in 0..2 {
                    let sx = x * 2 + dx;
                    let sy = y * 2 + dy;
                    if sx < w2 && sy < h2 {
                        sum += src[sy * w2 + sx] as u32;
                        n += 1;
                    }
                }
            }
            out[y * w + x] = sum.checked_div(n).unwrap_or(0) as u8;
        }
    }
    let mut m = metrics;
    m.width = w;
    m.height = h;
    m.xmin = div_toward_neg(m.xmin, 2);
    m.ymin = div_toward_neg(m.ymin, 2);
    m.advance_width *= 0.5;
    (m, out)
}

fn div_toward_neg(v: i32, d: i32) -> i32 {
    if v >= 0 { v / d } else { (v - (d - 1)) / d }
}

/// Lift mid-coverage so 14–18px Inter bowls stay ink on dark surfaces.
fn strengthen_coverage(coverage: &mut [u8], size: u16) {
    if size > 24 {
        return;
    }
    for p in coverage {
        if *p == 0 || *p == 255 {
            continue;
        }
        let c = *p as i32;
        let sharp = (c + (c - 128) * 80 / 256).clamp(0, 255) as u32;
        let boost = 72 * (255 - sharp) / 255;
        *p = (sharp + boost).min(255) as u8;
    }
}

/// Rounded ascent for a face/size (pixels).
pub fn ascent(face: Face, px: u16) -> i32 {
    let size = clamp_size(px);
    let mut guard = FONTS.lock();
    let Some(fonts) = guard.as_mut() else {
        return (size as i32 * 4) / 5;
    };
    fonts.ascent(face, size)
}

/// The natural line height (new-line advance) for a size/face, in pixels.
pub fn line_height(face: Face, px: u16) -> i32 {
    let size = clamp_size(px);
    let guard = FONTS.lock();
    let Some(fonts) = guard.as_ref() else {
        return libm::roundf(size as f32 * 1.2) as i32;
    };
    fonts
        .faces
        .get(face.index())
        .and_then(|f| f.horizontal_line_metrics(size as f32))
        .map(|m| libm::roundf(m.new_line_size) as i32)
        .unwrap_or(libm::roundf(size as f32 * 1.2) as i32)
}

/// Measure the advance width of a single line of text in pixels.
pub fn measure_text(text: &str, px: u16, face: Face) -> i32 {
    let size = clamp_size(px);
    let mut guard = FONTS.lock();
    let Some(fonts) = guard.as_mut() else {
        return 0;
    };
    let mut pen = 0.0f32;
    for ch in text.chars() {
        if ch == '\n' {
            break;
        }
        pen += fonts.glyph(face, size, ch).advance;
    }
    libm::roundf(pen) as i32
}

/// Walk a single line and invoke `visit` for each glyph.
///
/// `visit` receives `(gx, gy, glyph)` where `(gx, gy)` is the top-left of
/// the coverage bitmap. Returns the rounded advance width.
pub fn walk_text(
    text: &str,
    px: u16,
    face: Face,
    y: i32,
    mut visit: impl FnMut(i32, i32, &Glyph),
) -> i32 {
    let size = clamp_size(px);
    let mut guard = FONTS.lock();
    let Some(fonts) = guard.as_mut() else {
        return 0;
    };
    let baseline = y + fonts.ascent(face, size);
    let mut pen = 0.0f32;
    for ch in text.chars() {
        if ch == '\n' {
            break;
        }
        let glyph = fonts.glyph(face, size, ch);
        let gx = libm::roundf(pen) as i32 + glyph.xmin;
        let gy = baseline + glyph.top;
        let advance = glyph.advance;
        visit(gx, gy, glyph);
        pen += advance;
    }
    libm::roundf(pen) as i32
}

/// Rasterize one glyph; copies the cached coverage out of the lock.
pub fn rasterize(face: Face, ch: char, px: u16) -> Option<Glyph> {
    let size = clamp_size(px);
    let mut guard = FONTS.lock();
    let fonts = guard.as_mut()?;
    let g = fonts.glyph(face, size, ch);
    Some(Glyph {
        width: g.width,
        height: g.height,
        xmin: g.xmin,
        top: g.top,
        bearing_y: g.bearing_y,
        advance: g.advance,
        coverage: g.coverage.clone(),
    })
}
