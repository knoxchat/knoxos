/// Color Theme — "Aurora" Sophisticated Design Language for KnoxOS
/// Warm slate palette with organic depth, soft translucency, and
/// rose gold / coral accent colors. Designed for a refined,
/// human-centered desktop experience with natural warmth.
use super::framebuffer::Pixel;

// ═══════════════════════════════════════════════════════════════════════
// SEMANTIC DESIGN TOKENS — KnoxUI Component Library
// Used across all KnoxUI widgets for consistent theming.
// ═══════════════════════════════════════════════════════════════════════

// Accent / Brand
pub const ACCENT_PRIMARY: Pixel = Pixel::new(232, 121, 100, 255); // Warm coral
pub const ACCENT_SECONDARY: Pixel = Pixel::new(180, 140, 200, 255); // Soft lavender

// Text hierarchy
pub const TEXT_PRIMARY: Pixel = Pixel::new(235, 230, 225, 240); // Warm white
pub const TEXT_SECONDARY: Pixel = Pixel::new(180, 172, 164, 210); // Warm gray
pub const TEXT_MUTED: Pixel = Pixel::new(130, 122, 114, 170); // Muted stone
pub const TEXT_DISABLED: Pixel = Pixel::new(85, 80, 75, 120); // Disabled warm gray

// Surface layers
pub const SURFACE_RAISED: Pixel = Pixel::new(42, 38, 36, 230); // Warm dark card
pub const SURFACE_SUNKEN: Pixel = Pixel::new(18, 16, 14, 240); // Deep warm black
pub const SURFACE_OVERLAY: Pixel = Pixel::new(36, 32, 30, 240); // Warm overlay
pub const SURFACE_BORDER: Pixel = Pixel::new(120, 100, 80, 35); // Subtle warm edge

// Semantic states
pub const ERROR: Pixel = Pixel::new(235, 87, 87, 255); // Soft red
pub const SUCCESS: Pixel = Pixel::new(100, 200, 130, 255); // Sage green
pub const WARNING: Pixel = Pixel::new(240, 185, 70, 255); // Warm amber
pub const INFO: Pixel = Pixel::new(120, 160, 220, 220); // Soft blue

// ═══════════════════════════════════════════════════════════════════════
// DESKTOP / WALLPAPER — Warm Aurora with Organic Shapes
// ═══════════════════════════════════════════════════════════════════════
pub const WALLPAPER_BASE: Pixel = Pixel::new(22, 20, 26, 255); // Deep warm charcoal
pub const WALLPAPER_DARK: Pixel = Pixel::new(12, 11, 14, 255); // Near-black warm
pub const WALLPAPER_MID: Pixel = Pixel::new(32, 28, 36, 255); // Warm slate
pub const WALLPAPER_LIGHT: Pixel = Pixel::new(44, 38, 48, 255); // Muted plum
pub const WALLPAPER_ACCENT: Pixel = Pixel::new(232, 121, 100, 255); // Coral

// Organic aurora accent colors
pub const ORB_CORAL: Pixel = Pixel::new(232, 121, 100, 255); // Warm coral
pub const ORB_LAVENDER: Pixel = Pixel::new(160, 130, 200, 255); // Soft lavender
pub const ORB_PEACH: Pixel = Pixel::new(240, 170, 130, 255); // Peach
pub const ORB_SAGE: Pixel = Pixel::new(130, 180, 150, 255); // Sage green

// ═══════════════════════════════════════════════════════════════════════
// FLOATING DOCK — Frosted warm glass (centered, pill-shaped)
// ═══════════════════════════════════════════════════════════════════════
pub const TASKBAR_BG: Pixel = Pixel::new(28, 24, 22, 175); // Warm frosted glass
pub const TASKBAR_HOVER: Pixel = Pixel::new(232, 121, 100, 50); // Coral hover
pub const TASKBAR_ACTIVE: Pixel = Pixel::new(232, 121, 100, 65); // Coral active
pub const TASKBAR_FOREGROUND: Pixel = Pixel::new(232, 121, 100, 35); // Active bg
pub const TASKBAR_FOREGROUND_HOVER: Pixel = Pixel::new(240, 150, 120, 55);
pub const TASKBAR_BORDER: Pixel = Pixel::new(160, 130, 110, 30); // Warm glass edge
pub const TASKBAR_TEXT: Pixel = Pixel::new(235, 228, 220, 240); // Warm white

// Active app indicator dots (warm coral dots below icons)
pub const DOCK_INDICATOR_ACTIVE: Pixel = Pixel::new(232, 121, 100, 255); // Coral
pub const DOCK_INDICATOR_GLOW: Pixel = Pixel::new(232, 121, 100, 50); // Glow halo

// ═══════════════════════════════════════════════════════════════════════
// WINDOW CHROME — Warm Translucent Material
// ═══════════════════════════════════════════════════════════════════════
pub const WINDOW_BG: Pixel = Pixel::new(30, 28, 26, 248); // Warm surface
pub const WINDOW_TITLE_BG: Pixel = Pixel::new(36, 32, 30, 235); // Warm frosted header
pub const WINDOW_TITLE_BG_INACTIVE: Pixel = Pixel::new(32, 30, 28, 200);
pub const WINDOW_TITLE_HOVER: Pixel = Pixel::new(60, 52, 48, 180);
pub const WINDOW_TITLE_TEXT: Pixel = Pixel::new(235, 228, 220, 255); // Warm white
pub const WINDOW_TITLE_TEXT_INACTIVE: Pixel = Pixel::new(130, 122, 114, 190);
pub const WINDOW_CLOSE_HOVER: Pixel = Pixel::new(235, 87, 87, 230); // Soft red
pub const WINDOW_CLOSE_ACTIVE: Pixel = Pixel::new(210, 60, 60, 255);
pub const WINDOW_BORDER: Pixel = Pixel::new(140, 120, 100, 50); // Warm subtle edge
pub const WINDOW_BORDER_INACTIVE: Pixel = Pixel::new(80, 72, 64, 35);
pub const WINDOW_BUTTON_INACTIVE: Pixel = Pixel::new(120, 112, 104, 170);

// Soft shadow colors for depth
pub const SHADOW_AMBIENT: Pixel = Pixel::new(0, 0, 0, 255); // For large soft shadows
pub const SHADOW_COLORED: Pixel = Pixel::new(80, 50, 30, 255); // Warm glow shadows
pub const SHADOW_CONTACT: Pixel = Pixel::new(0, 0, 0, 255); // Sharp contact shadows

// ═══════════════════════════════════════════════════════════════════════
// ICON COLORS — Warm Elevated Cards
// ═══════════════════════════════════════════════════════════════════════
pub const ICON_TEXT: Pixel = Pixel::new(235, 228, 220, 255); // Warm white
pub const ICON_TEXT_SHADOW: Pixel = Pixel::new(0, 0, 0, 140);
pub const ICON_BG_SELECTED: Pixel = Pixel::new(232, 121, 100, 25); // Coral selection
pub const ICON_BG_FOCUSED: Pixel = Pixel::new(232, 121, 100, 40); // Coral focus
pub const ICON_BORDER_SELECTED: Pixel = Pixel::new(232, 121, 100, 55); // Coral border
pub const ICON_BORDER_FOCUSED: Pixel = Pixel::new(232, 121, 100, 85); // Coral bright

// ═══════════════════════════════════════════════════════════════════════
// START MENU / COMMAND CENTER
// ═══════════════════════════════════════════════════════════════════════
pub const START_MENU_BG: Pixel = Pixel::new(26, 24, 22, 230); // Warm deep surface
pub const START_MENU_HOVER: Pixel = Pixel::new(232, 121, 100, 35); // Coral hover
pub const START_MENU_SIDEBAR_BG: Pixel = Pixel::new(20, 18, 16, 235);
pub const START_MENU_SHADOW: Pixel = Pixel::new(0, 0, 0, 100);
pub const START_MENU_ACTIVE_BORDER: Pixel = Pixel::new(232, 121, 100, 180); // Coral

// ═══════════════════════════════════════════════════════════════════════
// PANEL COLORS
// ═══════════════════════════════════════════════════════════════════════
pub const PANEL_BG: Pixel = Pixel::new(26, 24, 22, 230);
pub const PANEL_BORDER: Pixel = Pixel::new(120, 100, 80, 35);

// ═══════════════════════════════════════════════════════════════════════
// COMMON COLORS & INTERACTIVE ELEMENTS
// ═══════════════════════════════════════════════════════════════════════
pub const BLACK: Pixel = Pixel::new(0, 0, 0, 255);
pub const WHITE: Pixel = Pixel::new(255, 255, 255, 255);
pub const TRANSPARENT: Pixel = Pixel::new(0, 0, 0, 0);
pub const HIGHLIGHT: Pixel = Pixel::new(232, 121, 100, 240); // Coral highlight
pub const SELECTION: Pixel = Pixel::new(232, 140, 120, 200); // Coral selection
pub const TEXT_SELECTION_BG: Pixel = Pixel::new(232, 121, 100, 160);

// Button colors (warm palette)
pub const BUTTON_PRIMARY: Pixel = Pixel::new(232, 121, 100, 255); // Coral
pub const BUTTON_PRIMARY_HOVER: Pixel = Pixel::new(240, 140, 120, 255);
pub const BUTTON_PRIMARY_ACTIVE: Pixel = Pixel::new(210, 105, 85, 255);
pub const BUTTON_SUCCESS: Pixel = Pixel::new(100, 200, 130, 255); // Sage green
pub const BUTTON_SUCCESS_HOVER: Pixel = Pixel::new(120, 215, 148, 255);
pub const BUTTON_DANGER: Pixel = Pixel::new(235, 87, 87, 255); // Soft red
pub const BUTTON_DANGER_HOVER: Pixel = Pixel::new(245, 110, 110, 255);
pub const BUTTON_WARNING: Pixel = Pixel::new(240, 185, 70, 255); // Warm amber
pub const BUTTON_WARNING_HOVER: Pixel = Pixel::new(248, 200, 90, 255);

// ═══════════════════════════════════════════════════════════════════════
// AI ASSISTANT — Warm Gradient Palette
// ═══════════════════════════════════════════════════════════════════════
pub const AI_BALANCED_1: Pixel = Pixel::new(232, 155, 130, 255); // Light coral
pub const AI_BALANCED_2: Pixel = Pixel::new(180, 140, 200, 255); // Lavender
pub const AI_BALANCED_3: Pixel = Pixel::new(200, 120, 160, 255); // Rose

// ═══════════════════════════════════════════════════════════════════════
// PROGRESS BAR
// ═══════════════════════════════════════════════════════════════════════
pub const PROGRESS_GREEN: Pixel = Pixel::new(100, 200, 130, 230); // Sage green
pub const PROGRESS_BG: Pixel = Pixel::new(32, 28, 26, 180);

// ═══════════════════════════════════════════════════════════════════════
// SCROLLBAR
// ═══════════════════════════════════════════════════════════════════════
pub const SCROLLBAR_TRACK: Pixel = Pixel::new(26, 24, 22, 200);
pub const SCROLLBAR_THUMB: Pixel = Pixel::new(85, 75, 68, 140);
pub const SCROLLBAR_THUMB_HOVER: Pixel = Pixel::new(120, 105, 92, 170);
pub const SCROLLBAR_THUMB_ACTIVE: Pixel = Pixel::new(232, 121, 100, 180);

// ═══════════════════════════════════════════════════════════════════════
// TERMINAL — Warm Dusk Theme
// ═══════════════════════════════════════════════════════════════════════
pub const TERMINAL_BG: Pixel = Pixel::new(18, 16, 14, 255); // Deep warm black
pub const TERMINAL_FG: Pixel = Pixel::new(210, 200, 190, 255); // Warm cream
pub const TERMINAL_CURSOR: Pixel = Pixel::new(232, 121, 100, 255); // Coral cursor
pub const TERMINAL_SELECTION: Pixel = Pixel::new(232, 121, 100, 60);
pub const TERMINAL_PROMPT_USER: Pixel = Pixel::new(232, 155, 130, 255); // Light coral
pub const TERMINAL_PROMPT_HOST: Pixel = Pixel::new(200, 190, 180, 255);
pub const TERMINAL_PROMPT_PATH: Pixel = Pixel::new(160, 140, 200, 255); // Lavender
pub const TERMINAL_PROMPT_SIGIL: Pixel = Pixel::new(130, 190, 150, 255); // Sage
pub const TERMINAL_SUGGESTION: Pixel = Pixel::new(70, 64, 58, 255);
pub const TERMINAL_ERROR: Pixel = Pixel::new(235, 87, 87, 255); // Soft red
pub const TERMINAL_COMMAND_VALID: Pixel = Pixel::new(130, 190, 150, 255);
pub const TERMINAL_STRING: Pixel = Pixel::new(200, 170, 110, 255); // Warm gold
pub const TERMINAL_VARIABLE: Pixel = Pixel::new(240, 185, 100, 255); // Amber
pub const TERMINAL_OPERATOR: Pixel = Pixel::new(180, 140, 200, 255); // Lavender

// ═══════════════════════════════════════════════════════════════════════
// PILL CONTROLS — Colored dot buttons (left-aligned in title bar)
// ═══════════════════════════════════════════════════════════════════════
pub const PILL_CLOSE: Pixel = Pixel::new(228, 92, 82, 255); // Terracotta red
pub const PILL_MAXIMIZE: Pixel = Pixel::new(100, 190, 130, 255); // Sage green
pub const PILL_MINIMIZE: Pixel = Pixel::new(230, 186, 80, 255); // Warm amber
pub const PILL_CLOSE_GLOW: Pixel = Pixel::new(228, 92, 82, 40);
pub const PILL_MAXIMIZE_GLOW: Pixel = Pixel::new(100, 190, 130, 40);
pub const PILL_MINIMIZE_GLOW: Pixel = Pixel::new(230, 186, 80, 40);

// ═══════════════════════════════════════════════════════════════════════
// UTILITY FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════

/// Create a color with modified alpha
pub const fn with_alpha(color: Pixel, alpha: u8) -> Pixel {
    Pixel::new(color.r, color.g, color.b, alpha)
}

/// Lighten a color by a factor (0-255 scale, where 128 = 50%)
pub fn lighten(color: Pixel, factor_pct: u32) -> Pixel {
    Pixel {
        r: (color.r as u32 + ((255 - color.r as u32) * factor_pct) / 255).min(255) as u8,
        g: (color.g as u32 + ((255 - color.g as u32) * factor_pct) / 255).min(255) as u8,
        b: (color.b as u32 + ((255 - color.b as u32) * factor_pct) / 255).min(255) as u8,
        a: color.a,
    }
}

/// Darken a color by a factor (0-255 scale, where 128 = 50%)
pub fn darken(color: Pixel, factor_pct: u32) -> Pixel {
    Pixel {
        r: (color.r as u32 * (255 - factor_pct) / 255) as u8,
        g: (color.g as u32 * (255 - factor_pct) / 255) as u8,
        b: (color.b as u32 * (255 - factor_pct) / 255) as u8,
        a: color.a,
    }
}

/// Interpolate between two colors by t (0-255)
pub fn mix(a: Pixel, b: Pixel, t: u8) -> Pixel {
    let t16 = t as u16;
    let inv = 255 - t16;
    Pixel {
        r: ((a.r as u16 * inv + b.r as u16 * t16) / 255) as u8,
        g: ((a.g as u16 * inv + b.g as u16 * t16) / 255) as u8,
        b: ((a.b as u16 * inv + b.b as u16 * t16) / 255) as u8,
        a: 255,
    }
}

/// Apply high-contrast transformation if enabled (17.7)
/// Boosts contrast: brightens light colors, darkens dark ones
#[inline]
pub fn hc(color: Pixel) -> Pixel {
    super::accessibility::high_contrast_color(color)
}

/// Get the appropriate window title bar background, respecting theme and high contrast
pub fn title_bg(active: bool) -> Pixel {
    if super::accessibility::is_high_contrast() {
        if active {
            Pixel::rgb(0, 0, 0)
        } else {
            Pixel::rgb(40, 40, 40)
        }
    } else {
        let tc = super::theme::colors();
        if active {
            tc.titlebar_bg
        } else {
            darken(tc.titlebar_bg, 30)
        }
    }
}

/// Get the appropriate window body background, respecting theme and high contrast
pub fn window_bg() -> Pixel {
    if super::accessibility::is_high_contrast() {
        Pixel::rgb(0, 0, 0)
    } else {
        super::theme::colors().bg_surface
    }
}

/// Get text color, respecting theme and high contrast
pub fn text_primary() -> Pixel {
    if super::accessibility::is_high_contrast() {
        Pixel::rgb(255, 255, 255)
    } else {
        super::theme::colors().text_primary
    }
}

/// Get border color, respecting theme and high contrast
pub fn border_active() -> Pixel {
    if super::accessibility::is_high_contrast() {
        Pixel::rgb(255, 255, 0)
    } else {
        super::theme::colors().border_focus
    }
}

/// Get the current accent color (theme-aware)
pub fn accent() -> Pixel {
    super::theme::accent_color()
}
