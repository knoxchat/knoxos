use crate::gui::colors;
use crate::gui::framebuffer::Pixel;
use crate::gui::layout::Spacing;

// ═══════════════════════════════════════════════════════════════════════
// Style — visual appearance of widgets
// ═══════════════════════════════════════════════════════════════════════

/// Visual style for the immediate-mode UI.
#[derive(Clone, Debug)]
pub struct Style {
    pub spacing: Spacing,
    /// Background color for interactive widgets (buttons, etc.)
    pub widget_bg: Pixel,
    /// Hovered widget background.
    pub widget_bg_hovered: Pixel,
    /// Active/pressed widget background.
    pub widget_bg_active: Pixel,
    /// Primary accent color.
    pub accent: Pixel,
    /// Accent color, hovered.
    pub accent_hovered: Pixel,
    /// Text color.
    pub text_color: Pixel,
    /// Dimmed text.
    pub text_dimmed: Pixel,
    /// Disabled text.
    pub text_disabled: Pixel,
    /// Widget border color.
    pub border_color: Pixel,
    /// Focused widget border.
    pub border_focused: Pixel,
    /// Separator color.
    pub separator_color: Pixel,
    /// Panel / window background.
    pub panel_bg: Pixel,
    /// Selection / highlight color.
    pub selection_bg: Pixel,
    /// Corner radius for buttons and widgets.
    pub corner_radius: u32,
    /// Corner radius for windows/panels.
    pub window_corner_radius: u32,
    /// Font scale: 1 = compact (8×12), 2 = large
    pub font_scale: u32,
    /// Use bold text for headings.
    pub heading_bold: bool,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            spacing: Spacing::default(),
            widget_bg: Pixel::rgb(45, 48, 55),
            widget_bg_hovered: Pixel::rgb(55, 60, 70),
            widget_bg_active: Pixel::rgb(35, 38, 45),
            accent: Pixel::rgb(82, 139, 255),
            accent_hovered: Pixel::rgb(100, 155, 255),
            text_color: colors::WHITE,
            text_dimmed: Pixel::rgb(160, 165, 175),
            text_disabled: Pixel::rgb(90, 95, 105),
            border_color: Pixel::rgb(65, 70, 80),
            border_focused: Pixel::rgb(82, 139, 255),
            separator_color: Pixel::rgb(50, 55, 65),
            panel_bg: Pixel::rgb(24, 26, 32),
            selection_bg: Pixel::rgb(40, 100, 220),
            corner_radius: 6,
            window_corner_radius: 10,
            font_scale: 1,
            heading_bold: true,
        }
    }
}
