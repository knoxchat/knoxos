/// Font Rendering — ClearType-quality subpixel text for KnoxOS
///
/// Three rendering modes:
///   - **Subpixel (ClearType)**: 3× horizontal resolution by addressing LCD
///     subpixels individually. Each glyph column is sampled at 3× and the
///     coverage is distributed across R/G/B channels, then filtered with a
///     3-tap low-pass to suppress color fringing. This is what makes text
///     on macOS, Windows ClearType, and FreeType look razor-sharp.
///   - **Grayscale AA**: Pre-baked 8-bit alpha coverage with gamma-correct
///     blending. Good quality, no color fringing risk.
///   - **Legacy 1-bit**: Fast fallback, no antialiasing.
///
/// Two font sizes:
///   - Standard: 10x20 (UI text, window titles, menus, terminal)
///   - Compact:  8x14  (status bars, labels, small text)
///
/// Gamma-correct blending: all alpha compositing operates in linear light space
/// for correct color mixing without dark fringing halos.
mod dpi;
mod engine;
mod gamma;
mod glyph;
mod layout;
mod metrics;
mod quality;
mod string;
mod subpixel;

pub use dpi::{
    draw_string_bold_compact_dpi, draw_string_bold_dpi, draw_string_compact_dpi, draw_string_dpi,
    measure_string_height_compact_dpi, measure_string_height_dpi, measure_string_width_compact_dpi,
    measure_string_width_dpi,
};
pub use engine::{draw_mono, draw_ui, measure_mono, measure_ui};
pub use gamma::gamma_blend_public;
pub use glyph::{draw_char, draw_char_bold, draw_char_bold_compact, draw_char_compact};
pub use layout::{
    draw_text_wrapped, draw_text_wrapped_compact, measure_string_height,
    measure_string_height_compact, measure_string_width, measure_string_width_compact,
    truncate_with_ellipsis, truncate_with_ellipsis_compact, word_wrap,
};
pub use metrics::{
    FONT_HEIGHT, FONT_HEIGHT_COMPACT, FONT_HEIGHT_HD, FONT_WIDTH, FONT_WIDTH_COMPACT, FONT_WIDTH_HD,
};
pub use quality::{
    SubpixelOrder, TextQuality, get_subpixel_order, get_text_quality, set_stem_darkening,
    set_subpixel_order, set_text_quality,
};
pub use string::{
    draw_icon_label, draw_string, draw_string_bold, draw_string_bold_compact, draw_string_centered,
    draw_string_centered_bold, draw_string_centered_bold_compact, draw_string_centered_compact,
    draw_string_compact, draw_string_shadow, draw_string_shadow_compact,
};
