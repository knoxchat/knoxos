//! TrueType/OpenType sfnt parser: table directory, cmap, loca/glyf, hmtx.
use alloc::string::String;
use alloc::vec::Vec;

use super::types::{Contour, GlyphOutline, GlyphPoint};

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

    pub(super) fn find_table(&self, tag: u32) -> Option<&[u8]> {
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
        self.load_glyph_depth(glyph_id, 0)
    }

    fn load_glyph_depth(&self, glyph_id: u16, depth: u8) -> Option<GlyphOutline> {
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
            self.parse_composite_glyph(glyf, go, depth)
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
                let remaining = num_points.saturating_sub(flags.len());
                for _ in 0..repeat.min(remaining) {
                    flags.push(flag);
                }
            }
        }
        flags.truncate(num_points);

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
                    x: x_coords[j] as i32,
                    y: y_coords[j] as i32,
                    on_curve: flags[j] & 0x01 != 0,
                });
            }
            contours.push(Contour { points });
            start = end + 1;
        }

        contours
    }

    /// Decode a TrueType composite (`numberOfContours < 0`): components,
    /// optional 2.14 scales, and XY offsets. Nested composites are allowed.
    fn parse_composite_glyph(&self, glyf: &[u8], offset: usize, depth: u8) -> Vec<Contour> {
        if depth >= 8 {
            return Vec::new();
        }
        const ARG_WORDS: u16 = 0x0001;
        const ARGS_XY: u16 = 0x0002;
        const HAVE_SCALE: u16 = 0x0008;
        const MORE: u16 = 0x0020;
        const HAVE_XY_SCALE: u16 = 0x0040;
        const HAVE_2X2: u16 = 0x0080;

        let mut pos = offset + 10;
        let mut contours = Vec::new();
        loop {
            if pos + 4 > glyf.len() {
                break;
            }
            let flags = u16::from_be_bytes([glyf[pos], glyf[pos + 1]]);
            let gid = u16::from_be_bytes([glyf[pos + 2], glyf[pos + 3]]);
            pos += 4;

            let mut dx = 0.0f32;
            let mut dy = 0.0f32;
            if flags & ARG_WORDS != 0 {
                if pos + 4 > glyf.len() {
                    break;
                }
                let a1 = i16::from_be_bytes([glyf[pos], glyf[pos + 1]]) as f32;
                let a2 = i16::from_be_bytes([glyf[pos + 2], glyf[pos + 3]]) as f32;
                pos += 4;
                if flags & ARGS_XY != 0 {
                    dx = a1;
                    dy = a2;
                }
            } else {
                if pos + 2 > glyf.len() {
                    break;
                }
                let a1 = glyf[pos] as i8 as f32;
                let a2 = glyf[pos + 1] as i8 as f32;
                pos += 2;
                if flags & ARGS_XY != 0 {
                    dx = a1;
                    dy = a2;
                }
            }

            let mut xx = 1.0f32;
            let mut xy = 0.0f32;
            let mut yx = 0.0f32;
            let mut yy = 1.0f32;
            if flags & HAVE_SCALE != 0 {
                if pos + 2 > glyf.len() {
                    break;
                }
                let s = i16::from_be_bytes([glyf[pos], glyf[pos + 1]]) as f32 / 16384.0;
                pos += 2;
                xx = s;
                yy = s;
            } else if flags & HAVE_XY_SCALE != 0 {
                if pos + 4 > glyf.len() {
                    break;
                }
                xx = i16::from_be_bytes([glyf[pos], glyf[pos + 1]]) as f32 / 16384.0;
                yy = i16::from_be_bytes([glyf[pos + 2], glyf[pos + 3]]) as f32 / 16384.0;
                pos += 4;
            } else if flags & HAVE_2X2 != 0 {
                if pos + 8 > glyf.len() {
                    break;
                }
                xx = i16::from_be_bytes([glyf[pos], glyf[pos + 1]]) as f32 / 16384.0;
                xy = i16::from_be_bytes([glyf[pos + 2], glyf[pos + 3]]) as f32 / 16384.0;
                yx = i16::from_be_bytes([glyf[pos + 4], glyf[pos + 5]]) as f32 / 16384.0;
                yy = i16::from_be_bytes([glyf[pos + 6], glyf[pos + 7]]) as f32 / 16384.0;
                pos += 8;
            }

            if let Some(child) = self.load_glyph_depth(gid, depth + 1) {
                for contour in &child.contours {
                    contours.push(transform_contour(contour, xx, xy, yx, yy, dx, dy));
                }
            }

            if flags & MORE == 0 {
                break;
            }
        }
        contours
    }
}

fn transform_contour(
    contour: &Contour,
    xx: f32,
    xy: f32,
    yx: f32,
    yy: f32,
    dx: f32,
    dy: f32,
) -> Contour {
    Contour {
        points: contour
            .points
            .iter()
            .map(|p| {
                let x = p.x as f32;
                let y = p.y as f32;
                GlyphPoint {
                    x: (xx * x + xy * y + dx) as i32,
                    y: (yx * x + yy * y + dy) as i32,
                    on_curve: p.on_curve,
                }
            })
            .collect(),
    }
}
