// ═══════════════════════════════════════════════════════════════════════
// THEME TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::gui::theme;

#[test_case]
fn test_theme_switch() {
    theme::set_theme(theme::ThemeId::ArcticLight);
    assert_eq!(theme::active_theme(), theme::ThemeId::ArcticLight);

    theme::set_theme(theme::ThemeId::NebulaDark);
    assert_eq!(theme::active_theme(), theme::ThemeId::NebulaDark);
}

#[test_case]
fn test_theme_colors_non_zero() {
    let colors = theme::colors();
    assert!(colors.bg_primary.a > 0);
}
