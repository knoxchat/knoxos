//! OpenType advanced features — kern table and GSUB ligatures.
use alloc::vec;
use alloc::vec::Vec;

use super::font::TtfFont;

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
