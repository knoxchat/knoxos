use super::framebuffer::Rect;
use super::window_events::Theme;
/// Window Attributes & Builder — winit-inspired window creation API
///
/// Provides a `WindowAttributes` struct with a builder pattern for creating windows
/// with rich configuration: size constraints, z-ordering (WindowLevel), theme,
/// button visibility (WindowButtons), transparency, and more.
///
/// Adapted from `winit/winit-core/src/window.rs`: `WindowAttributes`, `WindowLevel`,
/// `WindowButtons`, and `CursorGrabMode`.
use alloc::string::String;

// Re-export Theme from window_events for convenience
pub use super::window_events::Theme as WindowTheme;

// ═══════════════════════════════════════════════════════════════════════
// WindowLevel — z-ordering groups
// ═══════════════════════════════════════════════════════════════════════

/// A window level groups windows with respect to their z-position.
///
/// The relative ordering between windows in different window levels is fixed.
/// The z-order of a window within the same window level may change dynamically
/// on user interaction.
///
/// Adapted from `winit::window::WindowLevel`.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub enum WindowLevel {
    /// The window will always be below normal windows.
    /// Useful for widget-based / dashboard windows.
    AlwaysOnBottom,

    /// The default z-level.
    #[default]
    Normal,

    /// The window will always be on top of normal windows.
    /// Useful for floating toolbars, notifications, always-on-top utilities.
    AlwaysOnTop,
}

// ═══════════════════════════════════════════════════════════════════════
// WindowButtons — which title bar buttons are shown
// ═══════════════════════════════════════════════════════════════════════

/// Bitflags for which window buttons are enabled.
///
/// Adapted from `winit::window::WindowButtons`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct WindowButtons(u32);

impl WindowButtons {
    pub const CLOSE: Self = Self(1 << 0);
    pub const MINIMIZE: Self = Self(1 << 1);
    pub const MAXIMIZE: Self = Self(1 << 2);
    pub const ALL: Self = Self(0b111);
    pub const NONE: Self = Self(0);

    /// Check if a specific button flag is set.
    #[inline]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Combine two button sets (union).
    #[inline]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Remove button flags.
    #[inline]
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// Returns true if no buttons are enabled.
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl Default for WindowButtons {
    fn default() -> Self {
        Self::ALL
    }
}

// ═══════════════════════════════════════════════════════════════════════
// UserAttentionType — urgency level
// ═══════════════════════════════════════════════════════════════════════

/// Adapted from `winit::window::UserAttentionType`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
pub enum UserAttentionType {
    /// Strong urgency — bounces/flashes until focused.
    Critical,
    /// Mild urgency — single bounce/flash.
    #[default]
    Informational,
}

// ═══════════════════════════════════════════════════════════════════════
// WindowAttributes — builder for window configuration
// ═══════════════════════════════════════════════════════════════════════

/// Configuration for a new window, providing a builder-pattern API.
///
/// Adapted from `winit::window::WindowAttributes`.
///
/// # Usage
/// ```
/// let attrs = WindowAttributes::new()
///     .with_title("My Window")
///     .with_surface_size(800, 600)
///     .with_min_surface_size(320, 240)
///     .with_resizable(true)
///     .with_level(WindowLevel::Normal);
/// let window = Window::from_attributes(attrs);
/// ```
pub struct WindowAttributes {
    /// Window title (shown in title bar and taskbar).
    pub title: String,
    /// Initial width and height (content area, not including chrome).
    pub surface_width: u32,
    pub surface_height: u32,
    /// Initial position on screen. `None` = system decides (cascade).
    pub position: Option<(i32, i32)>,
    /// Minimum window size (content area).
    pub min_surface_size: Option<(u32, u32)>,
    /// Maximum window size (content area).
    pub max_surface_size: Option<(u32, u32)>,
    /// Whether the window can be resized by the user.
    pub resizable: bool,
    /// Which title bar buttons are visible/enabled.
    pub enabled_buttons: WindowButtons,
    /// Whether the window starts maximized.
    pub maximized: bool,
    /// Whether the window is initially visible.
    pub visible: bool,
    /// Whether the window has transparent content support.
    pub transparent: bool,
    /// Whether the window should have a blurred background.
    pub blur: bool,
    /// Whether standard window decorations (title bar, border) are drawn.
    pub decorations: bool,
    /// The z-order level for the window.
    pub window_level: WindowLevel,
    /// The preferred theme for this window.
    pub preferred_theme: Option<Theme>,
    /// Whether the window should start as the active (focused) window.
    pub active: bool,
    /// The content type (Terminal, Browser, etc.)
    pub content_type: super::window::WindowContentType,
}

impl Default for WindowAttributes {
    fn default() -> Self {
        Self {
            title: String::from("Untitled"),
            surface_width: 800,
            surface_height: 600,
            position: None,
            min_surface_size: None,
            max_surface_size: None,
            resizable: true,
            enabled_buttons: WindowButtons::ALL,
            maximized: false,
            visible: true,
            transparent: false,
            blur: false,
            decorations: true,
            window_level: WindowLevel::Normal,
            preferred_theme: None,
            active: true,
            content_type: super::window::WindowContentType::Empty,
        }
    }
}

impl WindowAttributes {
    /// Create a new default `WindowAttributes`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the window title.
    pub fn with_title(mut self, title: &str) -> Self {
        self.title = String::from(title);
        self
    }

    /// Set the initial content size.
    pub fn with_surface_size(mut self, width: u32, height: u32) -> Self {
        self.surface_width = width;
        self.surface_height = height;
        self
    }

    /// Set the initial position.
    pub fn with_position(mut self, x: i32, y: i32) -> Self {
        self.position = Some((x, y));
        self
    }

    /// Set the minimum surface size.
    pub fn with_min_surface_size(mut self, width: u32, height: u32) -> Self {
        self.min_surface_size = Some((width, height));
        self
    }

    /// Set the maximum surface size.
    pub fn with_max_surface_size(mut self, width: u32, height: u32) -> Self {
        self.max_surface_size = Some((width, height));
        self
    }

    /// Set whether the window is resizable.
    pub fn with_resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Set which title bar buttons are enabled.
    pub fn with_enabled_buttons(mut self, buttons: WindowButtons) -> Self {
        self.enabled_buttons = buttons;
        self
    }

    /// Set whether the window starts maximized.
    pub fn with_maximized(mut self, maximized: bool) -> Self {
        self.maximized = maximized;
        self
    }

    /// Set whether the window is initially visible.
    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Set whether the window has transparent content support.
    pub fn with_transparent(mut self, transparent: bool) -> Self {
        self.transparent = transparent;
        self
    }

    /// Set whether the window should have a blurred background.
    pub fn with_blur(mut self, blur: bool) -> Self {
        self.blur = blur;
        self
    }

    /// Set whether decorations (title bar, border) are drawn.
    pub fn with_decorations(mut self, decorations: bool) -> Self {
        self.decorations = decorations;
        self
    }

    /// Set the z-order level.
    pub fn with_window_level(mut self, level: WindowLevel) -> Self {
        self.window_level = level;
        self
    }

    /// Set the preferred theme.
    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.preferred_theme = Some(theme);
        self
    }

    /// Set whether the window starts active (focused).
    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Set the content type.
    pub fn with_content_type(mut self, ct: super::window::WindowContentType) -> Self {
        self.content_type = ct;
        self
    }
}
