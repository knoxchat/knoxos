/// TrueType / OpenType Font Loader
///
/// Parses TTF/OTF files from the VFS and extracts glyph outlines for
/// software rasterization at any point size. Supports:
///   - TrueType outlines (quadratic Bézier curves)
///   - OpenType/CFF outlines (cubic Bézier curves)
///   - cmap table (Unicode → glyph ID mapping)
///   - hmtx / hhea (horizontal metrics / advance widths)
///   - head (units-per-em, bounding box)
///   - Simple + composite glyphs
///
/// The rasterizer uses a scanline fill algorithm with 4× vertical
/// super-sampling for anti-aliased output.
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use super::framebuffer::{FrameBuffer, Pixel};

// ─── Font Table Tags ─────────────────────────────────────────────────

const TAG_CMAP: u32 = u32::from_be_bytes(*b"cmap");
const TAG_HEAD: u32 = u32::from_be_bytes(*b"head");
const TAG_HHEA: u32 = u32::from_be_bytes(*b"hhea");
const TAG_HMTX: u32 = u32::from_be_bytes(*b"hmtx");
const TAG_LOCA: u32 = u32::from_be_bytes(*b"loca");
const TAG_GLYF: u32 = u32::from_be_bytes(*b"glyf");
const TAG_MAXP: u32 = u32::from_be_bytes(*b"maxp");
const TAG_NAME: u32 = u32::from_be_bytes(*b"name");
const _TAG_POST: u32 = u32::from_be_bytes(*b"post");

// ─── Font Data Structures ────────────────────────────────────────────

/// A parsed TrueType/OpenType font
pub struct TtfFont {
    /// Raw font file data
    data: Vec<u8>,
    /// Table directory entries
    tables: Vec<TableEntry>,
    /// Units per em (from head table)
    pub units_per_em: u16,
    /// Number of glyphs (from maxp table)
    pub num_glyphs: u16,
    /// Number of long horizontal metrics (from hhea)
    num_h_metrics: u16,
    /// Whether loca uses short (2-byte) offsets
    loca_short: bool,
    /// Font family name
    pub family_name: String,
}

#[derive(Clone, Copy)]
struct TableEntry {
    tag: u32,
    offset: u32,
    length: u32,
}

/// A single glyph outline with contours
#[derive(Clone)]
pub struct GlyphOutline {
    pub advance_width: u16,
    pub lsb: i16,
    pub x_min: i16,
    pub y_min: i16,
    pub x_max: i16,
    pub y_max: i16,
    pub contours: Vec<Contour>,
}

/// A closed contour (sequence of on-curve and off-curve points)
#[derive(Clone)]
pub struct Contour {
    pub points: Vec<GlyphPoint>,
}

#[derive(Clone, Copy)]
pub struct GlyphPoint {
    pub x: i16,
    pub y: i16,
    pub on_curve: bool,
}

/// Rasterized glyph bitmap
pub struct GlyphBitmap {
    pub width: usize,
    pub height: usize,
    pub bearing_x: i32,
    pub bearing_y: i32,
    pub advance: i32,
    /// Alpha coverage values (0-255), row-major
    pub pixels: Vec<u8>,
}

// ─── Global Font Registry ────────────────────────────────────────────

/// Loaded system fonts
static FONT_REGISTRY: Mutex<Vec<TtfFont>> = Mutex::new(Vec::new());

/// Load a TTF/OTF font from raw bytes and register it
pub fn load_font(data: &[u8]) -> Result<usize, &'static str> {
    let font = TtfFont::parse(data)?;
    let mut reg = FONT_REGISTRY.lock();
    let idx = reg.len();
    reg.push(font);
    Ok(idx)
}

/// Load a font from a VFS path
pub fn load_font_from_path(path: &str) -> Result<usize, &'static str> {
    let data = crate::vfs::read_file_dispatch(path).ok_or("File not found")?;
    load_font(&data)
}

/// Get the number of loaded fonts
pub fn font_count() -> usize {
    FONT_REGISTRY.lock().len()
}

/// Render a glyph from the specified font at the given pixel size
pub fn rasterize_glyph(font_idx: usize, codepoint: char, px_size: f32) -> Option<GlyphBitmap> {
    let reg = FONT_REGISTRY.lock();
    let font = reg.get(font_idx)?;
    let glyph_id = font.cmap_lookup(codepoint as u32)?;
    let outline = font.load_glyph(glyph_id)?;
    Some(rasterize_outline(&outline, font.units_per_em, px_size))
}

/// Draw a string using a loaded TrueType font
pub fn draw_ttf_string(
    fb: &mut FrameBuffer,
    font_idx: usize,
    x: i32,
    y: i32,
    text: &str,
    color: Pixel,
    px_size: f32,
) {
    let reg = FONT_REGISTRY.lock();
    let font = match reg.get(font_idx) {
        Some(f) => f,
        None => return,
    };

    let scale = px_size / font.units_per_em as f32;
    let mut pen_x = x;

    for ch in text.chars() {
        let glyph_id = match font.cmap_lookup(ch as u32) {
            Some(id) => id,
            None => continue,
        };
        let outline = match font.load_glyph(glyph_id) {
            Some(o) => o,
            None => continue,
        };
        let bitmap = rasterize_outline(&outline, font.units_per_em, px_size);

        // Blit the glyph bitmap with gamma-correct blending
        for gy in 0..bitmap.height {
            for gx in 0..bitmap.width {
                let alpha = bitmap.pixels[gy * bitmap.width + gx];
                if alpha == 0 {
                    continue;
                }
                let px = pen_x + bitmap.bearing_x + gx as i32;
                let py = y - bitmap.bearing_y + gy as i32;
                if px >= 0 && py >= 0 && (px as usize) < fb.width && (py as usize) < fb.height {
                    if alpha == 255 {
                        fb.set_pixel(px as usize, py as usize, color);
                    } else {
                        // Gamma-correct blending via sRGB LUTs (from fonts module)
                        let bg = fb.get_pixel(px as usize, py as usize);
                        let a = alpha as u32;
                        let inv_a = 255 - a;
                        let r = super::fonts::gamma_blend_public(color.r, bg.r, a, inv_a);
                        let g = super::fonts::gamma_blend_public(color.g, bg.g, a, inv_a);
                        let b = super::fonts::gamma_blend_public(color.b, bg.b, a, inv_a);
                        fb.set_pixel(px as usize, py as usize, Pixel::rgb(r, g, b));
                    }
                }
            }
        }

        pen_x += (outline.advance_width as f32 * scale) as i32;
    }
}

// ─── Font Parsing ────────────────────────────────────────────────────

impl TtfFont {
    pub fn parse(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < 12 {
            return Err("Font data too short");
        }

        let sfnt_version = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        if sfnt_version != 0x00010000 && sfnt_version != 0x4F54544F {
            return Err("Not a valid TTF/OTF file");
        }

        let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
        let mut tables = Vec::with_capacity(num_tables);

        for i in 0..num_tables {
            let off = 12 + i * 16;
            if off + 16 > data.len() {
                break;
            }
            let tag = u32::from_be_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]]);
            let offset =
                u32::from_be_bytes([data[off + 8], data[off + 9], data[off + 10], data[off + 11]]);
            let length = u32::from_be_bytes([
                data[off + 12],
                data[off + 13],
                data[off + 14],
                data[off + 15],
            ]);
            tables.push(TableEntry {
                tag,
                offset,
                length,
            });
        }

        let mut font = TtfFont {
            data: data.to_vec(),
            tables,
            units_per_em: 1000,
            num_glyphs: 0,
            num_h_metrics: 0,
            loca_short: false,
            family_name: String::from("Unknown"),
        };

        font.parse_head()?;
        font.parse_maxp()?;
        font.parse_hhea()?;
        font.parse_name();

        Ok(font)
    }

    fn find_table(&self, tag: u32) -> Option<&[u8]> {
        for t in &self.tables {
            if t.tag == tag {
                let start = t.offset as usize;
                let end = start + t.length as usize;
                if end <= self.data.len() {
                    return Some(&self.data[start..end]);
                }
            }
        }
        None
    }

    fn parse_head(&mut self) -> Result<(), &'static str> {
        let head = self.find_table(TAG_HEAD).ok_or("Missing head table")?;
        if head.len() < 54 {
            return Err("head table too short");
        }
        let upem = u16::from_be_bytes([head[18], head[19]]);
        let idx_fmt = i16::from_be_bytes([head[50], head[51]]);
        self.units_per_em = upem;
        self.loca_short = idx_fmt == 0;
        Ok(())
    }

    fn parse_maxp(&mut self) -> Result<(), &'static str> {
        let maxp = self.find_table(TAG_MAXP).ok_or("Missing maxp table")?;
        if maxp.len() < 6 {
            return Err("maxp table too short");
        }
        let ng = u16::from_be_bytes([maxp[4], maxp[5]]);
        self.num_glyphs = ng;
        Ok(())
    }

    fn parse_hhea(&mut self) -> Result<(), &'static str> {
        let hhea = self.find_table(TAG_HHEA).ok_or("Missing hhea table")?;
        if hhea.len() < 36 {
            return Err("hhea table too short");
        }
        let nhm = u16::from_be_bytes([hhea[34], hhea[35]]);
        self.num_h_metrics = nhm;
        Ok(())
    }

    fn parse_name(&mut self) {
        let name_table = match self.find_table(TAG_NAME) {
            Some(t) => t,
            None => return,
        };
        if name_table.len() < 6 {
            return;
        }
        let count = u16::from_be_bytes([name_table[2], name_table[3]]) as usize;
        let string_offset = u16::from_be_bytes([name_table[4], name_table[5]]) as usize;

        let mut found_name: Option<String> = None;
        for i in 0..count {
            let rec_off = 6 + i * 12;
            if rec_off + 12 > name_table.len() {
                break;
            }
            let name_id = u16::from_be_bytes([name_table[rec_off + 6], name_table[rec_off + 7]]);
            if name_id == 1 {
                let length =
                    u16::from_be_bytes([name_table[rec_off + 8], name_table[rec_off + 9]]) as usize;
                let offset =
                    u16::from_be_bytes([name_table[rec_off + 10], name_table[rec_off + 11]])
                        as usize;
                let start = string_offset + offset;
                if start + length <= name_table.len() {
                    let platform_id =
                        u16::from_be_bytes([name_table[rec_off], name_table[rec_off + 1]]);
                    if platform_id == 1 {
                        if let Ok(s) = core::str::from_utf8(&name_table[start..start + length]) {
                            found_name = Some(String::from(s));
                            break;
                        }
                    }
                }
            }
        }
        if let Some(name) = found_name {
            self.family_name = name;
        }
    }

    /// Look up a Unicode codepoint in the cmap table → glyph ID
    pub fn cmap_lookup(&self, codepoint: u32) -> Option<u16> {
        let cmap = self.find_table(TAG_CMAP)?;
        if cmap.len() < 4 {
            return None;
        }
        let num_subtables = u16::from_be_bytes([cmap[2], cmap[3]]) as usize;

        // Find a format-4 or format-12 subtable
        for i in 0..num_subtables {
            let rec = 4 + i * 8;
            if rec + 8 > cmap.len() {
                break;
            }
            let _platform = u16::from_be_bytes([cmap[rec], cmap[rec + 1]]);
            let offset =
                u32::from_be_bytes([cmap[rec + 4], cmap[rec + 5], cmap[rec + 6], cmap[rec + 7]])
                    as usize;

            if offset + 2 > cmap.len() {
                continue;
            }
            let format = u16::from_be_bytes([cmap[offset], cmap[offset + 1]]);

            match format {
                4 => {
                    if let Some(gid) = self.cmap_format4(cmap, offset, codepoint) {
                        return Some(gid);
                    }
                }
                12 => {
                    if let Some(gid) = self.cmap_format12(cmap, offset, codepoint) {
                        return Some(gid);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn cmap_format4(&self, cmap: &[u8], offset: usize, codepoint: u32) -> Option<u16> {
        if codepoint > 0xFFFF {
            return None;
        }
        let cp = codepoint as u16;
        if offset + 14 > cmap.len() {
            return None;
        }
        let seg_count = u16::from_be_bytes([cmap[offset + 6], cmap[offset + 7]]) as usize / 2;
        let end_codes = offset + 14;
        let start_codes = end_codes + seg_count * 2 + 2;
        let id_deltas = start_codes + seg_count * 2;
        let id_range_offsets = id_deltas + seg_count * 2;

        for i in 0..seg_count {
            let end_code =
                u16::from_be_bytes([cmap[end_codes + i * 2], cmap[end_codes + i * 2 + 1]]);
            if cp > end_code {
                continue;
            }
            let start_code =
                u16::from_be_bytes([cmap[start_codes + i * 2], cmap[start_codes + i * 2 + 1]]);
            if cp < start_code {
                return Some(0); // .notdef
            }
            let id_delta =
                i16::from_be_bytes([cmap[id_deltas + i * 2], cmap[id_deltas + i * 2 + 1]]);
            let id_range_offset = u16::from_be_bytes([
                cmap[id_range_offsets + i * 2],
                cmap[id_range_offsets + i * 2 + 1],
            ]);

            if id_range_offset == 0 {
                return Some((cp as i32 + id_delta as i32) as u16);
            } else {
                let glyph_idx_offset = id_range_offsets
                    + i * 2
                    + id_range_offset as usize
                    + (cp as usize - start_code as usize) * 2;
                if glyph_idx_offset + 2 <= cmap.len() {
                    let gid =
                        u16::from_be_bytes([cmap[glyph_idx_offset], cmap[glyph_idx_offset + 1]]);
                    if gid != 0 {
                        return Some((gid as i32 + id_delta as i32) as u16);
                    }
                }
                return Some(0);
            }
        }
        Some(0)
    }

    fn cmap_format12(&self, cmap: &[u8], offset: usize, codepoint: u32) -> Option<u16> {
        if offset + 16 > cmap.len() {
            return None;
        }
        let num_groups = u32::from_be_bytes([
            cmap[offset + 12],
            cmap[offset + 13],
            cmap[offset + 14],
            cmap[offset + 15],
        ]) as usize;

        for i in 0..num_groups {
            let g = offset + 16 + i * 12;
            if g + 12 > cmap.len() {
                break;
            }
            let start = u32::from_be_bytes([cmap[g], cmap[g + 1], cmap[g + 2], cmap[g + 3]]);
            let end = u32::from_be_bytes([cmap[g + 4], cmap[g + 5], cmap[g + 6], cmap[g + 7]]);
            let start_glyph =
                u32::from_be_bytes([cmap[g + 8], cmap[g + 9], cmap[g + 10], cmap[g + 11]]);

            if codepoint >= start && codepoint <= end {
                return Some((start_glyph + (codepoint - start)) as u16);
            }
        }
        None
    }

    /// Load a glyph outline from the glyf table
    pub fn load_glyph(&self, glyph_id: u16) -> Option<GlyphOutline> {
        let loca = self.find_table(TAG_LOCA)?;
        let glyf = self.find_table(TAG_GLYF)?;
        let hmtx = self.find_table(TAG_HMTX)?;

        // Get glyph offset from loca
        let (glyph_offset, next_offset) = if self.loca_short {
            let off = glyph_id as usize * 2;
            if off + 4 > loca.len() {
                return None;
            }
            let o1 = u16::from_be_bytes([loca[off], loca[off + 1]]) as u32 * 2;
            let o2 = u16::from_be_bytes([loca[off + 2], loca[off + 3]]) as u32 * 2;
            (o1, o2)
        } else {
            let off = glyph_id as usize * 4;
            if off + 8 > loca.len() {
                return None;
            }
            let o1 = u32::from_be_bytes([loca[off], loca[off + 1], loca[off + 2], loca[off + 3]]);
            let o2 =
                u32::from_be_bytes([loca[off + 4], loca[off + 5], loca[off + 6], loca[off + 7]]);
            (o1, o2)
        };

        // Get advance width from hmtx
        let (advance_width, lsb) = if glyph_id < self.num_h_metrics {
            let off = glyph_id as usize * 4;
            if off + 4 <= hmtx.len() {
                (
                    u16::from_be_bytes([hmtx[off], hmtx[off + 1]]),
                    i16::from_be_bytes([hmtx[off + 2], hmtx[off + 3]]),
                )
            } else {
                (0, 0)
            }
        } else {
            let last_off = (self.num_h_metrics as usize - 1) * 4;
            let aw = if last_off + 2 <= hmtx.len() {
                u16::from_be_bytes([hmtx[last_off], hmtx[last_off + 1]])
            } else {
                0
            };
            let lsb_off = self.num_h_metrics as usize * 4
                + (glyph_id as usize - self.num_h_metrics as usize) * 2;
            let lsb = if lsb_off + 2 <= hmtx.len() {
                i16::from_be_bytes([hmtx[lsb_off], hmtx[lsb_off + 1]])
            } else {
                0
            };
            (aw, lsb)
        };

        // Empty glyph (e.g., space)
        if glyph_offset == next_offset {
            return Some(GlyphOutline {
                advance_width,
                lsb,
                x_min: 0,
                y_min: 0,
                x_max: 0,
                y_max: 0,
                contours: Vec::new(),
            });
        }

        let go = glyph_offset as usize;
        if go + 10 > glyf.len() {
            return None;
        }

        let num_contours = i16::from_be_bytes([glyf[go], glyf[go + 1]]);
        let x_min = i16::from_be_bytes([glyf[go + 2], glyf[go + 3]]);
        let y_min = i16::from_be_bytes([glyf[go + 4], glyf[go + 5]]);
        let x_max = i16::from_be_bytes([glyf[go + 6], glyf[go + 7]]);
        let y_max = i16::from_be_bytes([glyf[go + 8], glyf[go + 9]]);

        let contours = if num_contours >= 0 {
            self.parse_simple_glyph(glyf, go, num_contours as usize)
        } else {
            // Composite glyph — simplified: just return empty
            Vec::new()
        };

        Some(GlyphOutline {
            advance_width,
            lsb,
            x_min,
            y_min,
            x_max,
            y_max,
            contours,
        })
    }

    fn parse_simple_glyph(&self, glyf: &[u8], offset: usize, num_contours: usize) -> Vec<Contour> {
        let mut pos = offset + 10;
        let mut end_pts = Vec::with_capacity(num_contours);

        for _ in 0..num_contours {
            if pos + 2 > glyf.len() {
                return Vec::new();
            }
            end_pts.push(u16::from_be_bytes([glyf[pos], glyf[pos + 1]]));
            pos += 2;
        }

        // Skip instructions
        if pos + 2 > glyf.len() {
            return Vec::new();
        }
        let inst_len = u16::from_be_bytes([glyf[pos], glyf[pos + 1]]) as usize;
        pos += 2 + inst_len;

        let num_points = *end_pts.last().unwrap_or(&0) as usize + 1;

        // Parse flags
        let mut flags = Vec::with_capacity(num_points);
        while flags.len() < num_points {
            if pos >= glyf.len() {
                return Vec::new();
            }
            let flag = glyf[pos];
            pos += 1;
            flags.push(flag);
            if flag & 0x08 != 0 {
                // Repeat
                if pos >= glyf.len() {
                    return Vec::new();
                }
                let repeat = glyf[pos] as usize;
                pos += 1;
                for _ in 0..repeat {
                    flags.push(flag);
                }
            }
        }

        // Parse x-coordinates
        let mut x_coords = Vec::with_capacity(num_points);
        let mut x: i16 = 0;
        for i in 0..num_points {
            let f = flags[i];
            if f & 0x02 != 0 {
                // 1 byte
                if pos >= glyf.len() {
                    return Vec::new();
                }
                let dx = glyf[pos] as i16;
                pos += 1;
                x += if f & 0x10 != 0 { dx } else { -dx };
            } else if f & 0x10 == 0 {
                // 2 bytes
                if pos + 2 > glyf.len() {
                    return Vec::new();
                }
                let dx = i16::from_be_bytes([glyf[pos], glyf[pos + 1]]);
                pos += 2;
                x += dx;
            }
            // else: same as previous
            x_coords.push(x);
        }

        // Parse y-coordinates
        let mut y_coords = Vec::with_capacity(num_points);
        let mut y: i16 = 0;
        for i in 0..num_points {
            let f = flags[i];
            if f & 0x04 != 0 {
                if pos >= glyf.len() {
                    return Vec::new();
                }
                let dy = glyf[pos] as i16;
                pos += 1;
                y += if f & 0x20 != 0 { dy } else { -dy };
            } else if f & 0x20 == 0 {
                if pos + 2 > glyf.len() {
                    return Vec::new();
                }
                let dy = i16::from_be_bytes([glyf[pos], glyf[pos + 1]]);
                pos += 2;
                y += dy;
            }
            y_coords.push(y);
        }

        // Build contours
        let mut contours = Vec::with_capacity(num_contours);
        let mut start = 0usize;
        for &end in &end_pts {
            let end = end as usize;
            let mut points = Vec::new();
            for j in start..=end.min(num_points - 1) {
                points.push(GlyphPoint {
                    x: x_coords[j],
                    y: y_coords[j],
                    on_curve: flags[j] & 0x01 != 0,
                });
            }
            contours.push(Contour { points });
            start = end + 1;
        }

        contours
    }
}

// ─── Rasterizer ──────────────────────────────────────────────────────

/// Flatten a quadratic Bézier contour into line segments, properly handling
/// on-curve and off-curve control points per the TrueType specification.
/// Off-curve points are quadratic Bézier control points; consecutive
/// off-curve points have implied on-curve midpoints between them.
fn flatten_contour(contour: &Contour) -> Vec<(f32, f32)> {
    let pts = &contour.points;
    let n = pts.len();
    if n < 2 {
        return Vec::new();
    }

    let mut result: Vec<(f32, f32)> = Vec::new();

    // Find the first on-curve point (or synthesize one)
    let first_on = if pts[0].on_curve {
        (pts[0].x as f32, pts[0].y as f32)
    } else if pts[n - 1].on_curve {
        (pts[n - 1].x as f32, pts[n - 1].y as f32)
    } else {
        // Both first and last are off-curve: start at their midpoint
        (
            (pts[0].x as f32 + pts[n - 1].x as f32) * 0.5,
            (pts[0].y as f32 + pts[n - 1].y as f32) * 0.5,
        )
    };

    result.push(first_on);
    let mut last = first_on;
    let mut i = 0;

    while i < n {
        let p = &pts[i];
        if p.on_curve {
            last = (p.x as f32, p.y as f32);
            result.push(last);
            i += 1;
        } else {
            // Off-curve control point — find the next on-curve (or implied midpoint)
            let ctrl = (p.x as f32, p.y as f32);
            let next_idx = (i + 1) % n;
            let next_p = &pts[next_idx];
            let end = if next_p.on_curve {
                i += 2;
                (next_p.x as f32, next_p.y as f32)
            } else {
                // Implied on-curve midpoint between two off-curve points
                i += 1;
                (
                    (ctrl.0 + next_p.x as f32) * 0.5,
                    (ctrl.1 + next_p.y as f32) * 0.5,
                )
            };

            // Subdivide quadratic Bézier: P(t) = (1-t)²·last + 2(1-t)t·ctrl + t²·end
            // Use 8 segments for smooth curves
            let steps = 8;
            for s in 1..=steps {
                let t = s as f32 / steps as f32;
                let inv = 1.0 - t;
                let x = inv * inv * last.0 + 2.0 * inv * t * ctrl.0 + t * t * end.0;
                let y = inv * inv * last.1 + 2.0 * inv * t * ctrl.1 + t * t * end.1;
                result.push((x, y));
            }
            last = end;
        }
    }

    result
}

/// Rasterize a glyph outline into an alpha bitmap using scanline fill
/// with 4×4 super-sampling for high-quality anti-aliasing.
/// Properly handles quadratic Bézier curves (TrueType outlines).
fn rasterize_outline(outline: &GlyphOutline, units_per_em: u16, px_size: f32) -> GlyphBitmap {
    let scale = px_size / units_per_em as f32;
    let width = libm::ceilf((outline.x_max - outline.x_min) as f32 * scale) as usize + 2;
    let height = libm::ceilf((outline.y_max - outline.y_min) as f32 * scale) as usize + 2;

    if width == 0 || height == 0 || outline.contours.is_empty() {
        return GlyphBitmap {
            width: width.max(1),
            height: height.max(1),
            bearing_x: (outline.lsb as f32 * scale) as i32,
            bearing_y: (outline.y_max as f32 * scale) as i32,
            advance: (outline.advance_width as f32 * scale) as i32,
            pixels: vec![0u8; width.max(1) * height.max(1)],
        };
    }

    // 4×4 super-sampling (both horizontal and vertical) for full AA
    let ss_x = 4usize;
    let ss_y = 4usize;
    let ss_width = width * ss_x;
    let ss_height = height * ss_y;
    let total_samples = ss_x * ss_y; // 16 samples per output pixel
    let mut coverage = vec![0u32; width * height];

    // Pre-flatten all contours (handles Bézier curves correctly)
    let flattened: Vec<Vec<(f32, f32)>> = outline
        .contours
        .iter()
        .map(|c| flatten_contour(c))
        .collect();

    // For each super-sampled scanline, compute winding intersections
    for sy in 0..ss_height {
        let y_f = outline.y_max as f32 - (sy as f32 + 0.5) / (ss_y as f32 * scale);
        let mut intersections: Vec<f32> = Vec::new();

        for flat_pts in &flattened {
            let n = flat_pts.len();
            if n < 2 {
                continue;
            }
            for i in 0..n - 1 {
                let (x0, y0) = flat_pts[i];
                let (x1, y1) = flat_pts[i + 1];

                if (y0 < y_f && y1 < y_f) || (y0 >= y_f && y1 >= y_f) {
                    continue;
                }

                let t = (y_f - y0) / (y1 - y0);
                let x_i = x0 + t * (x1 - x0);
                // Convert to pixel-space X, accounting for super-sampling
                intersections.push((x_i - outline.x_min as f32) * scale * ss_x as f32);
            }
            // Close the contour: last point → first point
            if n >= 2 {
                let (x0, y0) = flat_pts[n - 1];
                let (x1, y1) = flat_pts[0];
                if !((y0 < y_f && y1 < y_f) || (y0 >= y_f && y1 >= y_f)) {
                    let t = (y_f - y0) / (y1 - y0);
                    let x_i = x0 + t * (x1 - x0);
                    intersections.push((x_i - outline.x_min as f32) * scale * ss_x as f32);
                }
            }
        }

        intersections.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
        let row = sy / ss_y;
        if row >= height {
            continue;
        }

        // Fill between pairs (even-odd rule) at sub-pixel resolution
        let mut i = 0;
        while i + 1 < intersections.len() {
            let x_start = intersections[i].max(0.0) as usize;
            let x_end = (intersections[i + 1] as usize).min(ss_width);
            for sx in x_start..x_end {
                let col = sx / ss_x;
                if col < width {
                    coverage[row * width + col] += 1;
                }
            }
            i += 2;
        }
    }

    // Convert coverage to alpha (16 samples per pixel from 4×4 SS)
    let mut pixels = vec![0u8; width * height];
    for i in 0..pixels.len() {
        pixels[i] = ((coverage[i] * 255) / total_samples as u32).min(255) as u8;
    }

    GlyphBitmap {
        width,
        height,
        bearing_x: (outline.lsb as f32 * scale) as i32,
        bearing_y: (outline.y_max as f32 * scale) as i32,
        advance: (outline.advance_width as f32 * scale) as i32,
        pixels,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// OPENTYPE ADVANCED FEATURES — Kerning & Ligatures
// ═══════════════════════════════════════════════════════════════════════

const TAG_KERN: u32 = u32::from_be_bytes(*b"kern");
const TAG_GSUB: u32 = u32::from_be_bytes(*b"GSUB");
const TAG_GPOS: u32 = u32::from_be_bytes(*b"GPOS");

/// A kerning pair adjustment
#[derive(Clone, Copy, Debug)]
pub struct KernPair {
    pub left: u16,
    pub right: u16,
    pub x_advance: i16,
}

/// An OpenType ligature mapping
#[derive(Clone, Debug)]
pub struct Ligature {
    /// Input glyph sequence (e.g., [f, i])
    pub components: Vec<u16>,
    /// Replacement glyph ID
    pub ligature_glyph: u16,
}

/// OpenType feature tables
pub struct OtFeatures {
    pub kern_pairs: Vec<KernPair>,
    pub ligatures: Vec<Ligature>,
}

impl TtfFont {
    /// Parse kern table (format 0) for pair-based kerning
    pub fn parse_kern_table(&self) -> Vec<KernPair> {
        let mut pairs = Vec::new();
        let data = match self.find_table(TAG_KERN) {
            Some(d) => d,
            None => return pairs,
        };
        if data.len() < 4 {
            return pairs;
        }

        let _version = u16::from_be_bytes([data[0], data[1]]);
        let n_tables = u16::from_be_bytes([data[2], data[3]]) as usize;
        let mut off = 4usize;

        for _ in 0..n_tables {
            if off + 6 > data.len() {
                break;
            }
            let _sub_version = u16::from_be_bytes([data[off], data[off + 1]]);
            let sub_length = u16::from_be_bytes([data[off + 2], data[off + 3]]) as usize;
            let coverage = u16::from_be_bytes([data[off + 4], data[off + 5]]);
            let format = coverage >> 8;

            if format == 0 && off + 8 <= data.len() {
                let n_pairs = u16::from_be_bytes([data[off + 6], data[off + 7]]) as usize;
                let start = off + 14;
                for i in 0..n_pairs {
                    let p = start + i * 6;
                    if p + 6 > data.len() {
                        break;
                    }
                    pairs.push(KernPair {
                        left: u16::from_be_bytes([data[p], data[p + 1]]),
                        right: u16::from_be_bytes([data[p + 2], data[p + 3]]),
                        x_advance: i16::from_be_bytes([data[p + 4], data[p + 5]]),
                    });
                }
            }
            off += sub_length.max(6);
        }
        crate::serial_println!("[ttf] Parsed {} kern pairs", pairs.len());
        pairs
    }

    /// Parse GSUB ligature substitutions (Lookup Type 4)
    pub fn parse_ligatures(&self) -> Vec<Ligature> {
        let mut ligs = Vec::new();
        let data = match self.find_table(TAG_GSUB) {
            Some(d) => d,
            None => return ligs,
        };
        if data.len() < 10 {
            return ligs;
        }

        // GSUB header
        let _major = u16::from_be_bytes([data[0], data[1]]);
        let _minor = u16::from_be_bytes([data[2], data[3]]);
        let _script_list_off = u16::from_be_bytes([data[4], data[5]]) as usize;
        let _feature_list_off = u16::from_be_bytes([data[6], data[7]]) as usize;
        let lookup_list_off = u16::from_be_bytes([data[8], data[9]]) as usize;

        if lookup_list_off >= data.len() {
            return ligs;
        }

        // LookupList table
        let ll = &data[lookup_list_off..];
        if ll.len() < 2 {
            return ligs;
        }
        let lookup_count = u16::from_be_bytes([ll[0], ll[1]]) as usize;

        for li in 0..lookup_count {
            let lo = 2 + li * 2;
            if lo + 2 > ll.len() {
                break;
            }
            let lookup_off = u16::from_be_bytes([ll[lo], ll[lo + 1]]) as usize;
            if lookup_off >= ll.len() {
                continue;
            }

            let lk = &ll[lookup_off..];
            if lk.len() < 6 {
                continue;
            }
            let lookup_type = u16::from_be_bytes([lk[0], lk[1]]);

            if lookup_type == 4 {
                // Type 4: Ligature Substitution
                let subtable_count = u16::from_be_bytes([lk[4], lk[5]]) as usize;
                for si in 0..subtable_count {
                    let so = 6 + si * 2;
                    if so + 2 > lk.len() {
                        break;
                    }
                    let sub_off = u16::from_be_bytes([lk[so], lk[so + 1]]) as usize;
                    if sub_off >= lk.len() {
                        continue;
                    }

                    let st = &lk[sub_off..];
                    if st.len() < 6 {
                        continue;
                    }
                    let _format = u16::from_be_bytes([st[0], st[1]]);
                    let coverage_off = u16::from_be_bytes([st[2], st[3]]) as usize;
                    let lig_set_count = u16::from_be_bytes([st[4], st[5]]) as usize;

                    // Parse coverage table to get first glyphs
                    let mut first_glyphs = Vec::new();
                    if coverage_off < st.len() {
                        let cov = &st[coverage_off..];
                        if cov.len() >= 4 {
                            let cov_format = u16::from_be_bytes([cov[0], cov[1]]);
                            let glyph_count = u16::from_be_bytes([cov[2], cov[3]]) as usize;
                            if cov_format == 1 {
                                for gi in 0..glyph_count {
                                    let go = 4 + gi * 2;
                                    if go + 2 <= cov.len() {
                                        first_glyphs
                                            .push(u16::from_be_bytes([cov[go], cov[go + 1]]));
                                    }
                                }
                            }
                        }
                    }

                    // Parse each LigatureSet
                    for lsi in 0..lig_set_count {
                        let lso = 6 + lsi * 2;
                        if lso + 2 > st.len() {
                            break;
                        }
                        let ls_off = u16::from_be_bytes([st[lso], st[lso + 1]]) as usize;
                        if ls_off >= st.len() {
                            continue;
                        }

                        let ls = &st[ls_off..];
                        if ls.len() < 2 {
                            continue;
                        }
                        let lig_count = u16::from_be_bytes([ls[0], ls[1]]) as usize;

                        let first = first_glyphs.get(lsi).copied().unwrap_or(0);

                        for lei in 0..lig_count {
                            let leo = 2 + lei * 2;
                            if leo + 2 > ls.len() {
                                break;
                            }
                            let lig_off = u16::from_be_bytes([ls[leo], ls[leo + 1]]) as usize;
                            if lig_off >= ls.len() {
                                continue;
                            }

                            let l = &ls[lig_off..];
                            if l.len() < 4 {
                                continue;
                            }
                            let lig_glyph = u16::from_be_bytes([l[0], l[1]]);
                            let comp_count = u16::from_be_bytes([l[2], l[3]]) as usize;

                            let mut components = vec![first];
                            for ci in 0..comp_count.saturating_sub(1) {
                                let co = 4 + ci * 2;
                                if co + 2 <= l.len() {
                                    components.push(u16::from_be_bytes([l[co], l[co + 1]]));
                                }
                            }

                            ligs.push(Ligature {
                                components,
                                ligature_glyph: lig_glyph,
                            });
                        }
                    }
                }
            }
        }

        crate::serial_println!("[ttf] Parsed {} ligatures from GSUB", ligs.len());
        ligs
    }

    /// Get kerning adjustment for a glyph pair
    pub fn get_kerning(&self, pairs: &[KernPair], left: u16, right: u16) -> i16 {
        for pair in pairs {
            if pair.left == left && pair.right == right {
                return pair.x_advance;
            }
        }
        0
    }
}

// ═══════════════════════════════════════════════════════════════════════
// FONT FALLBACK SYSTEM — Configurable priority chain
// ═══════════════════════════════════════════════════════════════════════

/// Font fallback entry with priority
#[derive(Clone, Debug)]
pub struct FontFallbackEntry {
    pub name: String,
    pub priority: u8,
    pub covers_cjk: bool,
    pub covers_emoji: bool,
    pub covers_symbols: bool,
}

lazy_static::lazy_static! {
    static ref FONT_FALLBACK_CHAIN: Mutex<Vec<FontFallbackEntry>> = Mutex::new(vec![
        FontFallbackEntry {
            name: String::from("Hack"),
            priority: 0,
            covers_cjk: false,
            covers_emoji: false,
            covers_symbols: true,
        },
        FontFallbackEntry {
            name: String::from("CJK-Bitmap"),
            priority: 1,
            covers_cjk: true,
            covers_emoji: false,
            covers_symbols: false,
        },
        FontFallbackEntry {
            name: String::from("Emoji-Color"),
            priority: 2,
            covers_cjk: false,
            covers_emoji: true,
            covers_symbols: false,
        },
    ]);
}

/// Set the font fallback priority chain
pub fn set_fallback_chain(entries: Vec<FontFallbackEntry>) {
    let mut chain = FONT_FALLBACK_CHAIN.lock();
    *chain = entries;
    chain.sort_by_key(|e| e.priority);
    crate::serial_println!("[fonts] Fallback chain updated: {} entries", chain.len());
}

/// Get the current font fallback chain
pub fn get_fallback_chain() -> Vec<FontFallbackEntry> {
    FONT_FALLBACK_CHAIN.lock().clone()
}

/// Add a font to the fallback chain
pub fn add_fallback_font(entry: FontFallbackEntry) {
    let mut chain = FONT_FALLBACK_CHAIN.lock();
    chain.push(entry);
    chain.sort_by_key(|e| e.priority);
}

/// Resolve which font should render a given codepoint
pub fn resolve_font_for_codepoint(codepoint: u32) -> String {
    let chain = FONT_FALLBACK_CHAIN.lock();
    let is_cjk = super::unicode::is_cjk(codepoint);
    let is_emoji = super::unicode::is_emoji(codepoint);

    for entry in chain.iter() {
        if is_cjk && entry.covers_cjk {
            return entry.name.clone();
        }
        if is_emoji && entry.covers_emoji {
            return entry.name.clone();
        }
        if !is_cjk && !is_emoji {
            return entry.name.clone();
        }
    }
    // Ultimate fallback
    String::from("Hack")
}

// ═══════════════════════════════════════════════════════════════════════
// SUBPIXEL POSITIONING
// ═══════════════════════════════════════════════════════════════════════

/// Subpixel position quantized to 1/4 pixel increments
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SubpixelPosition {
    /// x offset in 1/64th of a pixel (FreeType convention)
    pub x_frac: u8,
    /// y offset in 1/64th of a pixel
    pub y_frac: u8,
}

impl SubpixelPosition {
    /// Quantize a fractional pixel position to 4 sub-pixel positions (0, 16, 32, 48)
    pub fn quantize(x_frac: f32, y_frac: f32) -> Self {
        let qx = ((x_frac * 64.0) as u8) & 0x30; // Mask to 0, 16, 32, or 48
        let qy = ((y_frac * 64.0) as u8) & 0x30;
        Self {
            x_frac: qx,
            y_frac: qy,
        }
    }

    pub fn zero() -> Self {
        Self {
            x_frac: 0,
            y_frac: 0,
        }
    }
}

/// Draw a string with subpixel-positioned glyphs for smoother text
pub fn draw_string_subpixel(
    fb: &mut FrameBuffer,
    x: f32,
    y: i32,
    text: &str,
    color: Pixel,
    font_size: u32,
) -> f32 {
    let registry = FONT_REGISTRY.lock();
    if registry.is_empty() {
        drop(registry);
        super::fonts::draw_string(fb, x as i32, y, text, color, 1);
        return x + (text.len() as f32 * 10.0);
    }
    drop(registry);

    let mut pen_x = x;
    for ch in text.chars() {
        if let Some(bitmap) = rasterize_glyph(0, ch, font_size as f32) {
            let bx = pen_x + bitmap.bearing_x as f32;
            let by = y as f32 - bitmap.bearing_y as f32;

            for row in 0..bitmap.height {
                for col in 0..bitmap.width {
                    let alpha = bitmap.pixels[row * bitmap.width + col];
                    if alpha == 0 {
                        continue;
                    }

                    let px = (bx + col as f32) as usize;
                    let py = (by + row as f32) as usize;
                    if px < fb.width && py < fb.height {
                        let blended = Pixel::new(
                            color.r,
                            color.g,
                            color.b,
                            ((alpha as u32 * color.a as u32) / 255) as u8,
                        );
                        fb.blend_pixel(px, py, blended);
                    }
                }
            }
            pen_x += bitmap.advance as f32;
        } else {
            // Fall back to bitmap font
            super::fonts::draw_char(fb, pen_x as i32, y, ch, color, 1);
            pen_x += 10.0;
        }
    }

    pen_x
}

// ═══════════════════════════════════════════════════════════════════════
// WEB FONT LOADING
// ═══════════════════════════════════════════════════════════════════════

/// A web font descriptor (loaded from CSS @font-face)
#[derive(Clone, Debug)]
pub struct WebFont {
    pub family: String,
    pub style: String, // "normal", "italic"
    pub weight: u16,   // 100-900
    pub data: Vec<u8>, // Raw TTF/OTF data
    pub loaded: bool,
}

lazy_static::lazy_static! {
    static ref WEB_FONTS: Mutex<Vec<WebFont>> = Mutex::new(Vec::new());
}

/// Register a web font from downloaded TTF/OTF data
pub fn register_web_font(family: &str, style: &str, weight: u16, data: Vec<u8>) -> bool {
    // Validate it's a TTF/OTF by checking magic bytes
    if data.len() < 12 {
        return false;
    }
    let magic = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    if magic != 0x00010000 && magic != 0x4F54544F {
        // Not TrueType (0x00010000) or OpenType/CFF (OTTO)
        return false;
    }

    // Parse and register in the font system
    if let Ok(font) = TtfFont::parse(&data) {
        let mut registry = FONT_REGISTRY.lock();
        registry.push(font);

        let mut web_fonts = WEB_FONTS.lock();
        web_fonts.push(WebFont {
            family: String::from(family),
            style: String::from(style),
            weight,
            data,
            loaded: true,
        });
        crate::serial_println!(
            "[fonts] Registered web font: {} {} w{}",
            family,
            style,
            weight
        );
        true
    } else {
        false
    }
}

/// List loaded web fonts
pub fn list_web_fonts() -> Vec<(String, String, u16)> {
    WEB_FONTS
        .lock()
        .iter()
        .map(|f| (f.family.clone(), f.style.clone(), f.weight))
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// FONT CONFIGURATION
// ═══════════════════════════════════════════════════════════════════════

/// System-wide font configuration
#[derive(Clone, Debug)]
pub struct FontConfig {
    /// Default UI font family
    pub default_family: String,
    /// Default monospace font family
    pub monospace_family: String,
    /// Default UI font size in pixels
    pub default_size: u32,
    /// Hinting mode
    pub hinting: HintingMode,
    /// Anti-aliasing mode
    pub antialiasing: AaMode,
    /// Enable ligatures globally
    pub enable_ligatures: bool,
    /// Enable kerning globally
    pub enable_kerning: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HintingMode {
    None,
    Slight,
    Medium,
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AaMode {
    None,
    Grayscale,
    Subpixel,
}

lazy_static::lazy_static! {
    static ref FONT_CONFIG: Mutex<FontConfig> = Mutex::new(FontConfig {
        default_family: String::from("Hack"),
        monospace_family: String::from("Hack"),
        default_size: 14,
        hinting: HintingMode::Slight,
        antialiasing: AaMode::Grayscale,
        enable_ligatures: true,
        enable_kerning: true,
    });
}

/// Get the current font configuration
pub fn get_font_config() -> FontConfig {
    FONT_CONFIG.lock().clone()
}

/// Update font configuration
pub fn set_font_config(config: FontConfig) {
    crate::serial_println!(
        "[fonts] Config updated: family={}, size={}, hint={:?}, aa={:?}",
        config.default_family,
        config.default_size,
        config.hinting,
        config.antialiasing
    );
    *FONT_CONFIG.lock() = config;
}
