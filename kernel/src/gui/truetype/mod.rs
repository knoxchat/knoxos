/// TrueType / OpenType Font Loader
///
/// Parses TTF/OTF files from the VFS and extracts glyph outlines for
/// software rasterization at any point size. Supports:
///   - TrueType outlines (quadratic Bézier curves)
///   - OpenType/CFF outlines (cubic Bézier curves)
///   - cmap table (Unicode → glyph ID mapping)
///   - hmtx / hhea (horizontal metrics / advance widths)
///   - head (units-per-em, bounding box)
///   - Simple + composite glyphs (components, scales, XY offsets)
///
/// The rasterizer uses a non-zero winding scanline fill with 8×8
/// super-sampling at UI sizes. UI chrome is rasterized by `fontdue`
/// (Inter); this module remains the OpenType parser (GSUB/GPOS) and
/// a fallback outline rasterizer.
mod config;
mod fallback;
mod features;
mod font;
mod hint;
mod raster;
mod registry;
mod subpixel;
mod types;
mod web;

pub use config::{AaMode, FontConfig, HintingMode, get_font_config, set_font_config};
pub use fallback::{
    FontFallbackEntry, add_fallback_font, get_fallback_chain, resolve_font_for_codepoint,
    set_fallback_chain,
};
pub use features::{KernPair, Ligature, OtFeatures};
pub use font::TtfFont;
pub use hint::{GATE_TTF_HINT_MARKER, hint_glyph, hint_self_test, round_26_6};
pub use registry::{draw_ttf_string, font_count, load_font, load_font_from_path, rasterize_glyph};
pub use subpixel::{SubpixelPosition, draw_string_subpixel};
pub use types::{Contour, GlyphBitmap, GlyphOutline, GlyphPoint};
pub use web::{WebFont, list_web_fonts, register_web_font};
