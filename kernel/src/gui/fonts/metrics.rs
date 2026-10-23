use crate::gui::hack_font;
use crate::gui::hack_font_hd;

/// Font cell dimensions — standard 10x20 (high-quality AA monospace)
pub const FONT_WIDTH: usize = hack_font::GLYPH_WIDTH;
pub const FONT_HEIGHT: usize = hack_font::GLYPH_HEIGHT;
/// Compact font dimensions — 8x14 (for small UI text)
pub const FONT_WIDTH_COMPACT: usize = hack_font::GLYPH_WIDTH_COMPACT;
pub const FONT_HEIGHT_COMPACT: usize = hack_font::GLYPH_HEIGHT_COMPACT;
/// HD font dimensions — 20x40 (for crisp rendering at scale ≥ 2)
pub const FONT_WIDTH_HD: usize = hack_font_hd::GLYPH_WIDTH_HD;
pub const FONT_HEIGHT_HD: usize = hack_font_hd::GLYPH_HEIGHT_HD;

/// Line spacing: 2px extra for comfortable reading
pub(super) const LINE_SPACING_EXTRA: i32 = 2;
/// Compact line spacing: 1px extra
pub(super) const LINE_SPACING_EXTRA_COMPACT: i32 = 1;
