/// Embedded Font Data for KnoxOS
///
/// TTF font data compiled into the kernel binary via `include_bytes!()`.
///
///   - UI font: Inter (https://rsms.me/inter/) — SIL Open Font License
///   - Mono font: Hack (https://sourcefoundry.org/hack/) — MIT license

/// UI font data (proportional) — Inter Regular
pub static FONT_UI_DATA: &[u8] = include_bytes!("../../fonts/Inter-Regular.ttf");

/// UI bold font data — Inter Bold
pub static FONT_UI_BOLD_DATA: &[u8] = include_bytes!("../../fonts/Inter-Bold.ttf");

/// Monospace font data — Hack Regular
pub static FONT_MONO_DATA: &[u8] = include_bytes!("../../fonts/Hack-Regular.ttf");
