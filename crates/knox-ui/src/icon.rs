// SPDX-License-Identifier: MIT
//! Icon (GPUI kit catalog, reimplemented).
//!
//! Knox-Studio paints Lucide SVGs through GPUI `svg()` (`currentColor`,
//! 24×24 viewBox, stroke-width 2, round cap/join). This crate is `no_std`, so
//! the same path data is CPU-stroked into an alpha tile and tinted as a
//! monochrome sprite — not geometric 1-bit stamps and not runtime resvg.

use alloc::vec;
use alloc::vec::Vec;

use knox_gpui::{
    Bounds, Element, Hsla, LayoutHints, MonochromeSprite, PaintEnv, Pixels, Point, Scene, Size,
    WindowInput,
};

use crate::size::{ControlSize, Sizable};
use crate::theme::Theme;

/// Lucide viewBox. Stroke width is 2 user units (round cap / round join).
const VIEW: f32 = 24.0;
const STROKE: f32 = 2.0;

/// Named KnoxOS icons. Variants match Knox-Studio / Lucide names where we
/// ship the same glyph (`Close` is Lucide `x`, `Home` is `house`, `Alert` is
/// `triangle-alert`, `Trash` is `trash`, `SquareTerminal` is `square-terminal`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum IconName {
    #[default]
    Check,
    Close,
    Plus,
    Minus,
    Search,
    User,
    Bell,
    Image,
    ChevronDown,
    ChevronRight,
    ChevronLeft,
    ChevronUp,
    Star,
    Info,
    Alert,
    Inbox,
    Folder,
    File,
    Settings,
    Home,
    Copy,
    Globe,
    SquareTerminal,
    Trash,
    Sparkles,
    FileText,
    HardDrive,
    Bot,
    LayoutGrid,
    Wifi,
    Volume2,
    VolumeX,
    Battery,
    BatteryCharging,
    Bluetooth,
    Calendar,
    Calculator,
    Gamepad,
    Archive,
    Clipboard,
    Scissors,
    Undo,
    Save,
    RefreshCw,
    ZoomIn,
    ZoomOut,
    Monitor,
    Sun,
    Play,
    Pause,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    Menu,
    AppWindow,
    Maximize2,
    Minimize2,
    Lock,
    RotateCw,
    Power,
}

/// Tinted Lucide glyph painted as a monochrome sprite.
pub struct Icon {
    theme: Theme,
    name: IconName,
    size: ControlSize,
    color: Option<Hsla>,
    box_px: Option<Pixels>,
    computed: Bounds,
}

impl Icon {
    pub fn new(name: IconName) -> Self {
        Self {
            theme: Theme::default(),
            name,
            size: ControlSize::Medium,
            color: None,
            box_px: None,
            computed: Bounds::ZERO,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    /// Override the density box (desktop icons use 48px).
    pub fn pixel_size(mut self, size: i32) -> Self {
        self.box_px = Some(knox_gpui::px(size.max(8)));
        self
    }

    pub fn name(&self) -> IconName {
        self.name
    }

    fn side(&self) -> i32 {
        self.box_px.map(|p| p.0).unwrap_or(self.size.icon()).max(8)
    }
}

impl From<IconName> for Icon {
    fn from(name: IconName) -> Self {
        Self::new(name)
    }
}

impl IconName {
    pub const ALL: &'static [Self] = &[
        Self::Check,
        Self::Close,
        Self::Plus,
        Self::Minus,
        Self::Search,
        Self::User,
        Self::Bell,
        Self::Image,
        Self::ChevronDown,
        Self::ChevronRight,
        Self::ChevronLeft,
        Self::ChevronUp,
        Self::Star,
        Self::Info,
        Self::Alert,
        Self::Inbox,
        Self::Folder,
        Self::File,
        Self::Settings,
        Self::Home,
        Self::Copy,
        Self::Globe,
        Self::SquareTerminal,
        Self::Trash,
        Self::Sparkles,
        Self::FileText,
        Self::HardDrive,
        Self::Bot,
        Self::LayoutGrid,
        Self::Wifi,
        Self::Volume2,
        Self::VolumeX,
        Self::Battery,
        Self::BatteryCharging,
        Self::Bluetooth,
        Self::Calendar,
        Self::Calculator,
        Self::Gamepad,
        Self::Archive,
        Self::Clipboard,
        Self::Scissors,
        Self::Undo,
        Self::Save,
        Self::RefreshCw,
        Self::ZoomIn,
        Self::ZoomOut,
        Self::Monitor,
        Self::Sun,
        Self::Play,
        Self::Pause,
        Self::ArrowLeft,
        Self::ArrowRight,
        Self::ArrowUp,
        Self::Menu,
        Self::AppWindow,
        Self::Maximize2,
        Self::Minimize2,
        Self::Lock,
        Self::RotateCw,
        Self::Power,
    ];

    /// Coverage bitmap for compositor blits (taskbar, drag preview, chrome).
    pub fn raster(self, size: i32) -> Vec<u8> {
        raster(self, size)
    }

    /// Pick a Lucide glyph from an app / window title (Knox-Studio sidebar style).
    pub fn for_label(label: &str) -> Self {
        if has_ci(label, "term") {
            Self::SquareTerminal
        } else if has_ci(label, "browser") || has_ci(label, "web") {
            Self::Globe
        } else if has_ci(label, "file")
            || has_ci(label, "folder")
            || has_ci(label, "document")
            || has_ci(label, "explorer")
        {
            Self::Folder
        } else if has_ci(label, "setting") {
            Self::Settings
        } else if has_ci(label, "trash") {
            Self::Trash
        } else if has_ci(label, "ai") || has_ci(label, "assistant") || has_ci(label, "chat") {
            Self::Sparkles
        } else if has_ci(label, "calc") {
            Self::Calculator
        } else if has_ci(label, "edit") || has_ci(label, "log") {
            Self::FileText
        } else if has_ci(label, "image") || has_ci(label, "photo") {
            Self::Image
        } else if has_ci(label, "game") {
            Self::Gamepad
        } else if has_ci(label, "media") || has_ci(label, "music") || has_ci(label, "video") {
            Self::Play
        } else if has_ci(label, "disk") || has_ci(label, "drive") {
            Self::HardDrive
        } else if has_ci(label, "blue") {
            Self::Bluetooth
        } else if has_ci(label, "calendar") {
            Self::Calendar
        } else if has_ci(label, "archive") {
            Self::Archive
        } else if has_ci(label, "task")
            || has_ci(label, "monitor")
            || has_ci(label, "home")
            || has_ci(label, "computer")
        {
            Self::Monitor
        } else if has_ci(label, "lock") {
            Self::Lock
        } else if has_ci(label, "wizard") || has_ci(label, "gallery") {
            Self::LayoutGrid
        } else {
            Self::AppWindow
        }
    }
}

fn has_ci(hay: &str, needle: &str) -> bool {
    let h = hay.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || h.len() < n.len() {
        return false;
    }
    h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
}

impl Sizable for Icon {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Element for Icon {
    fn request_layout(&mut self, origin: Point, _available: Size) -> Bounds {
        let s = self.side();
        self.computed = Bounds::from_xywh(origin.x.0, origin.y.0, s, s);
        self.computed
    }

    fn layout_hints(&self) -> LayoutHints {
        LayoutHints::default()
    }

    fn paint(
        &mut self,
        bounds: Bounds,
        scene: &mut Scene,
        _input: &WindowInput,
        env: &mut PaintEnv,
    ) {
        let bounds = if bounds.is_empty() {
            self.computed
        } else {
            bounds
        };
        // Intern at the requested box, not the stretched paint bounds, so a
        // gallery layout pass cannot flood the atlas with unique sizes and
        // evict glyphs already in the scene.
        let side = self.side();
        let alpha = raster(self.name, side);
        let key = 0xD802_1000u64 | ((self.name as u64) << 8) | (side as u64);
        let tile = env.atlas.intern_alpha(key, side as u32, side as u32, alpha);
        let color = self.color.unwrap_or(self.theme.colors.foreground);
        scene.push_mono_sprite(MonochromeSprite::new(bounds, tile, color));
    }
}

#[derive(Clone, Copy)]
enum Prim {
    Path(&'static str),
    Circle(f32, f32, f32),
    Rect(f32, f32, f32, f32, f32),
}

fn primitives(name: IconName) -> &'static [Prim] {
    // Path data copied from knox-studio `gpui_kit/assets/icons/*.svg` (Lucide).
    match name {
        IconName::Check => &[Prim::Path("M20 6 9 17l-5-5")],
        IconName::Close => &[Prim::Path("M18 6 6 18"), Prim::Path("m6 6 12 12")],
        IconName::Plus => &[Prim::Path("M5 12h14"), Prim::Path("M12 5v14")],
        IconName::Minus => &[Prim::Path("M5 12h14")],
        IconName::Search => &[
            Prim::Path("m21 21-4.34-4.34"),
            Prim::Circle(11.0, 11.0, 8.0),
        ],
        IconName::User => &[
            Prim::Path("M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"),
            Prim::Circle(12.0, 7.0, 4.0),
        ],
        IconName::Bell => &[
            Prim::Path("M10.268 21a2 2 0 0 0 3.464 0"),
            Prim::Path(
                "M3.262 15.326A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673C19.41 13.956 18 12.499 18 8A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326",
            ),
        ],
        IconName::Image => &[
            Prim::Rect(3.0, 3.0, 18.0, 18.0, 2.0),
            Prim::Circle(9.0, 9.0, 2.0),
            Prim::Path("m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"),
        ],
        IconName::ChevronDown => &[Prim::Path("m6 9 6 6 6-6")],
        IconName::ChevronRight => &[Prim::Path("m9 18 6-6-6-6")],
        IconName::ChevronLeft => &[Prim::Path("m15 18-6-6 6-6")],
        IconName::ChevronUp => &[Prim::Path("m18 15-6-6-6 6")],
        IconName::Star => &[Prim::Path(
            "M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z",
        )],
        IconName::Info => &[
            Prim::Circle(12.0, 12.0, 10.0),
            Prim::Path("M12 16v-4"),
            Prim::Path("M12 8h.01"),
        ],
        IconName::Alert => &[
            Prim::Path("m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"),
            Prim::Path("M12 9v4"),
            Prim::Path("M12 17h.01"),
        ],
        IconName::Inbox => &[
            Prim::Path("M22 12 16 12 14 15 10 15 8 12 2 12"),
            Prim::Path(
                "M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z",
            ),
        ],
        IconName::Folder => &[Prim::Path(
            "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
        )],
        IconName::File => &[
            Prim::Path(
                "M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z",
            ),
            Prim::Path("M14 2v5a1 1 0 0 0 1 1h5"),
        ],
        IconName::Settings => &[
            Prim::Path(
                "M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915",
            ),
            Prim::Circle(12.0, 12.0, 3.0),
        ],
        IconName::Home => &[
            Prim::Path("M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8"),
            Prim::Path(
                "M3 10a2 2 0 0 1 .709-1.528l7-6a2 2 0 0 1 2.582 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
            ),
        ],
        IconName::Copy => &[
            Prim::Rect(8.0, 8.0, 14.0, 14.0, 2.0),
            Prim::Path("M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"),
        ],
        IconName::Globe => &[
            Prim::Circle(12.0, 12.0, 10.0),
            Prim::Path("M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"),
            Prim::Path("M2 12h20"),
        ],
        IconName::SquareTerminal => &[
            Prim::Path("m7 11 2-2-2-2"),
            Prim::Path("M11 13h4"),
            Prim::Rect(3.0, 3.0, 18.0, 18.0, 2.0),
        ],
        IconName::Trash => &[
            Prim::Path("M10 11v6"),
            Prim::Path("M14 11v6"),
            Prim::Path("M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"),
            Prim::Path("M3 6h18"),
            Prim::Path("M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"),
        ],
        IconName::Sparkles => &[
            Prim::Path(
                "M11.017 2.814a1 1 0 0 1 1.966 0l1.051 5.558a2 2 0 0 0 1.594 1.594l5.558 1.051a1 1 0 0 1 0 1.966l-5.558 1.051a2 2 0 0 0-1.594 1.594l-1.051 5.558a1 1 0 0 1-1.966 0l-1.051-5.558a2 2 0 0 0-1.594-1.594l-5.558-1.051a1 1 0 0 1 0-1.966l5.558-1.051a2 2 0 0 0 1.594-1.594z",
            ),
            Prim::Path("M20 2v4"),
            Prim::Path("M22 4h-4"),
            Prim::Circle(4.0, 20.0, 2.0),
        ],
        IconName::FileText => &[
            Prim::Path(
                "M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z",
            ),
            Prim::Path("M14 2v5a1 1 0 0 0 1 1h5"),
            Prim::Path("M10 9H8"),
            Prim::Path("M16 13H8"),
            Prim::Path("M16 17H8"),
        ],
        IconName::HardDrive => &[
            Prim::Path("M10 16h.01"),
            Prim::Path(
                "M2.212 11.577a2 2 0 0 0-.212.896V18a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-5.527a2 2 0 0 0-.212-.896L18.55 5.11A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z",
            ),
            Prim::Path("M21.946 12.013H2.054"),
            Prim::Path("M6 16h.01"),
        ],
        IconName::Bot => &[
            Prim::Path("M12 8V4H8"),
            Prim::Rect(4.0, 8.0, 16.0, 12.0, 2.0),
            Prim::Path("M2 14h2"),
            Prim::Path("M20 14h2"),
            Prim::Path("M15 13v2"),
            Prim::Path("M9 13v2"),
        ],
        IconName::LayoutGrid => &[
            Prim::Rect(3.0, 3.0, 7.0, 7.0, 1.0),
            Prim::Rect(14.0, 3.0, 7.0, 7.0, 1.0),
            Prim::Rect(14.0, 14.0, 7.0, 7.0, 1.0),
            Prim::Rect(3.0, 14.0, 7.0, 7.0, 1.0),
        ],
        IconName::Wifi => &[
            Prim::Path("M12 20h.01"),
            Prim::Path("M2 8.82a15 15 0 0 1 20 0"),
            Prim::Path("M5 12.859a10 10 0 0 1 14 0"),
            Prim::Path("M8.5 16.429a5 5 0 0 1 7 0"),
        ],
        IconName::Volume2 => &[
            Prim::Path(
                "M11 4.702a.705.705 0 0 0-1.203-.498L6.413 7.587A1.4 1.4 0 0 1 5.416 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.416a1.4 1.4 0 0 1 .997.413l3.383 3.384A.705.705 0 0 0 11 19.298z",
            ),
            Prim::Path("M16 9a5 5 0 0 1 0 6"),
            Prim::Path("M19.364 18.364a9 9 0 0 0 0-12.728"),
        ],
        IconName::VolumeX => &[
            Prim::Path(
                "M11 4.702a.7.7 0 0 0-1.203-.498L6.413 7.587A1.4 1.4 0 0 1 5.416 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.416a1.4 1.4 0 0 1 .997.413l3.383 3.384A.7.7 0 0 0 11 19.298z",
            ),
            Prim::Path("m16.5 14.5 5-5"),
            Prim::Path("m16.5 9.5 5 5"),
        ],
        IconName::Battery => &[
            Prim::Path("M22 14 22 10"),
            Prim::Rect(2.0, 6.0, 16.0, 12.0, 2.0),
        ],
        IconName::BatteryCharging => &[
            Prim::Path("m11 7-3 5h4l-3 5"),
            Prim::Path("M14.856 6H16a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-2.935"),
            Prim::Path("M22 14v-4"),
            Prim::Path("M5.14 18H4a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h2.936"),
        ],
        IconName::Bluetooth => &[Prim::Path("m7 7 10 10-5 5V2l5 5L7 17")],
        IconName::Calendar => &[
            Prim::Path("M8 2v3"),
            Prim::Path("M16 2v3"),
            Prim::Rect(3.0, 3.0, 18.0, 18.0, 2.0),
            Prim::Path("M3 9h18"),
        ],
        IconName::Calculator => &[
            Prim::Rect(4.0, 2.0, 16.0, 20.0, 2.0),
            Prim::Path("M16 10h.01"),
            Prim::Path("M12 10h.01"),
            Prim::Path("M8 10h.01"),
            Prim::Path("M12 14h.01"),
            Prim::Path("M8 14h.01"),
            Prim::Path("M12 18h.01"),
            Prim::Path("M8 18h.01"),
        ],
        IconName::Gamepad => &[Prim::Path(
            "M17.32 5H6.68a4 4 0 0 0-3.978 3.59c-.006.052-.01.101-.017.152C2.604 9.416 2 14.456 2 16a3 3 0 0 0 3 3c1 0 1.5-.5 2-1l1.414-1.414A2 2 0 0 1 9.828 16h4.344a2 2 0 0 1 1.414.586L17 18c.5.5 1 1 2 1a3 3 0 0 0 3-3c0-1.545-.604-6.584-.685-7.258-.007-.05-.011-.1-.017-.151A4 4 0 0 0 17.32 5z",
        )],
        IconName::Archive => &[
            Prim::Rect(2.0, 3.0, 20.0, 5.0, 1.0),
            Prim::Path("M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8"),
            Prim::Path("M10 12h4"),
        ],
        IconName::Clipboard => &[
            Prim::Rect(8.0, 2.0, 8.0, 4.0, 1.0),
            Prim::Path("M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"),
        ],
        IconName::Scissors => &[
            Prim::Circle(6.0, 6.0, 3.0),
            Prim::Path("M8.12 8.12 12 12"),
            Prim::Path("M20 4 8.12 15.88"),
            Prim::Circle(6.0, 18.0, 3.0),
            Prim::Path("M14.8 14.8 20 20"),
        ],
        IconName::Undo => &[
            Prim::Path("M3 7v6h6"),
            Prim::Path("M21 17a9 9 0 0 0-9-9 9 9 0 0 0-6 2.3L3 13"),
        ],
        IconName::Save => &[
            Prim::Path(
                "M15.2 3a2 2 0 0 1 1.4.6l3.8 3.8a2 2 0 0 1 .6 1.4V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z",
            ),
            Prim::Path("M17 21v-7a1 1 0 0 0-1-1H8a1 1 0 0 0-1 1v7"),
            Prim::Path("M7 3v4a1 1 0 0 0 1 1h7"),
        ],
        IconName::RefreshCw => &[
            Prim::Path("M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"),
            Prim::Path("M21 3v5h-5"),
            Prim::Path("M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"),
            Prim::Path("M8 16H3v5"),
        ],
        IconName::ZoomIn => &[
            Prim::Circle(11.0, 11.0, 8.0),
            Prim::Path("M21 21 16.65 16.65"),
            Prim::Path("M11 8v6"),
            Prim::Path("M8 11h6"),
        ],
        IconName::ZoomOut => &[
            Prim::Circle(11.0, 11.0, 8.0),
            Prim::Path("M21 21 16.65 16.65"),
            Prim::Path("M8 11h6"),
        ],
        IconName::Monitor => &[
            Prim::Rect(2.0, 3.0, 20.0, 14.0, 2.0),
            Prim::Path("M8 21h8"),
            Prim::Path("M12 17v4"),
        ],
        IconName::Sun => &[
            Prim::Circle(12.0, 12.0, 4.0),
            Prim::Path("M12 2v2"),
            Prim::Path("M12 20v2"),
            Prim::Path("m4.93 4.93 1.41 1.41"),
            Prim::Path("m17.66 17.66 1.41 1.41"),
            Prim::Path("M2 12h2"),
            Prim::Path("M20 12h2"),
            Prim::Path("m6.34 17.66-1.41 1.41"),
            Prim::Path("m19.07 4.93-1.41 1.41"),
        ],
        IconName::Play => &[Prim::Path(
            "M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z",
        )],
        IconName::Pause => &[
            Prim::Rect(14.0, 3.0, 5.0, 18.0, 1.0),
            Prim::Rect(5.0, 3.0, 5.0, 18.0, 1.0),
        ],
        IconName::ArrowLeft => &[Prim::Path("m12 19-7-7 7-7"), Prim::Path("M19 12H5")],
        IconName::ArrowRight => &[Prim::Path("M5 12h14"), Prim::Path("m12 5 7 7-7 7")],
        IconName::ArrowUp => &[Prim::Path("m5 12 7-7 7 7"), Prim::Path("M12 19V5")],
        IconName::Menu => &[
            Prim::Path("M4 5h16"),
            Prim::Path("M4 12h16"),
            Prim::Path("M4 19h16"),
        ],
        IconName::AppWindow => &[
            Prim::Rect(2.0, 4.0, 20.0, 16.0, 2.0),
            Prim::Path("M10 4v4"),
            Prim::Path("M2 8h20"),
            Prim::Path("M6 4v4"),
        ],
        IconName::Maximize2 => &[
            Prim::Path("M15 3h6v6"),
            Prim::Path("M9 21H3v-6"),
            Prim::Path("M21 3l-7 7"),
            Prim::Path("M3 21l7-7"),
        ],
        IconName::Minimize2 => &[
            Prim::Path("M4 14h6v6"),
            Prim::Path("M20 10h-6V4"),
            Prim::Path("M14 10l7-7"),
            Prim::Path("M3 21l7-7"),
        ],
        IconName::Lock => &[
            Prim::Rect(3.0, 11.0, 18.0, 11.0, 2.0),
            Prim::Path("M7 11V7a5 5 0 0 1 10 0v4"),
        ],
        IconName::RotateCw => &[
            Prim::Path("M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8"),
            Prim::Path("M21 3v5h-5"),
        ],
        IconName::Power => &[
            Prim::Path("M12 2v10"),
            Prim::Path("M18.4 6.6a9 9 0 1 1-12.77.04"),
        ],
    }
}

fn hypot(x: f32, y: f32) -> f32 {
    libm::sqrtf(x * x + y * y)
}

fn dist_seg(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
    let vx = x1 - x0;
    let vy = y1 - y0;
    let wx = px - x0;
    let wy = py - y0;
    let c1 = vx * wx + vy * wy;
    if c1 <= 0.0 {
        return hypot(wx, wy);
    }
    let c2 = vx * vx + vy * vy;
    if c2 <= c1 {
        return hypot(px - x1, py - y1);
    }
    let t = c1 / c2;
    hypot(px - (x0 + t * vx), py - (y0 + t * vy))
}

fn coverage(dist: f32, half: f32) -> u8 {
    let a = half + 0.5 - dist;
    if a <= 0.0 {
        0
    } else if a >= 1.0 {
        255
    } else {
        (a * 255.0 + 0.5) as u8
    }
}

fn stamp_seg(buf: &mut [u8], size: i32, x0: f32, y0: f32, x1: f32, y1: f32, half: f32) {
    let pad = half + 1.5;
    let min_x = libm::floorf(x0.min(x1) - pad).max(0.0) as i32;
    let max_x = libm::ceilf(x0.max(x1) + pad).min(size as f32) as i32;
    let min_y = libm::floorf(y0.min(y1) - pad).max(0.0) as i32;
    let max_y = libm::ceilf(y0.max(y1) + pad).min(size as f32) as i32;
    for y in min_y..max_y {
        for x in min_x..max_x {
            let d = dist_seg(x as f32 + 0.5, y as f32 + 0.5, x0, y0, x1, y1);
            let c = coverage(d, half);
            if c > 0 {
                let i = (y * size + x) as usize;
                if i < buf.len() {
                    buf[i] = buf[i].max(c);
                }
            }
        }
    }
}

fn add_line(segs: &mut Vec<(f32, f32, f32, f32)>, x0: f32, y0: f32, x1: f32, y1: f32) {
    if (x0 - x1).abs() < 1e-6 && (y0 - y1).abs() < 1e-6 {
        // Degenerate (Lucide `h.01` dots): still emit a hairline so round caps stamp.
        segs.push((x0, y0, x0 + 0.01, y0));
        return;
    }
    segs.push((x0, y0, x1, y1));
}

fn cubic(
    segs: &mut Vec<(f32, f32, f32, f32)>,
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
) {
    let steps = 10;
    let mut prev = p0;
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let u = 1.0 - t;
        let x =
            u * u * u * p0.0 + 3.0 * u * u * t * p1.0 + 3.0 * u * t * t * p2.0 + t * t * t * p3.0;
        let y =
            u * u * u * p0.1 + 3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t * p3.1;
        add_line(segs, prev.0, prev.1, x, y);
        prev = (x, y);
    }
}

fn quad(segs: &mut Vec<(f32, f32, f32, f32)>, p0: (f32, f32), p1: (f32, f32), p2: (f32, f32)) {
    let steps = 8;
    let mut prev = p0;
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let u = 1.0 - t;
        let x = u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0;
        let y = u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1;
        add_line(segs, prev.0, prev.1, x, y);
        prev = (x, y);
    }
}

/// Endpoint-parameterized SVG arc → polylines (W3C implementation notes).
fn arc_to(
    segs: &mut Vec<(f32, f32, f32, f32)>,
    x0: f32,
    y0: f32,
    mut rx: f32,
    mut ry: f32,
    phi_deg: f32,
    large: bool,
    sweep: bool,
    x1: f32,
    y1: f32,
) {
    rx = libm::fabsf(rx);
    ry = libm::fabsf(ry);
    if rx < 1e-6 || ry < 1e-6 {
        add_line(segs, x0, y0, x1, y1);
        return;
    }
    let phi = phi_deg * core::f32::consts::PI / 180.0;
    let cos_p = libm::cosf(phi);
    let sin_p = libm::sinf(phi);
    let dx = (x0 - x1) / 2.0;
    let dy = (y0 - y1) / 2.0;
    let x1p = cos_p * dx + sin_p * dy;
    let y1p = -sin_p * dx + cos_p * dy;
    let mut lam = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lam > 1.0 {
        lam = libm::sqrtf(lam);
        rx *= lam;
        ry *= lam;
    }
    let rx2 = rx * rx;
    let ry2 = ry * ry;
    let x1p2 = x1p * x1p;
    let y1p2 = y1p * y1p;
    let num = (rx2 * ry2 - rx2 * y1p2 - ry2 * x1p2).max(0.0);
    let den = rx2 * y1p2 + ry2 * x1p2;
    let mut coef = if den < 1e-12 {
        0.0
    } else {
        libm::sqrtf(num / den)
    };
    if large == sweep {
        coef = -coef;
    }
    let cxp = coef * rx * y1p / ry;
    let cyp = coef * -ry * x1p / rx;
    let cx = cos_p * cxp - sin_p * cyp + (x0 + x1) / 2.0;
    let cy = sin_p * cxp + cos_p * cyp + (y0 + y1) / 2.0;

    let ux = (x1p - cxp) / rx;
    let uy = (y1p - cyp) / ry;
    let vx = (-x1p - cxp) / rx;
    let vy = (-y1p - cyp) / ry;
    let n1 = hypot(ux, uy).max(1e-12);
    let mut theta1 = libm::acosf((ux / n1).clamp(-1.0, 1.0));
    if uy < 0.0 {
        theta1 = -theta1;
    }
    let n2 = (n1 * hypot(vx, vy)).max(1e-12);
    let mut dtheta = libm::acosf(((ux * vx + uy * vy) / n2).clamp(-1.0, 1.0));
    if ux * vy - uy * vx < 0.0 {
        dtheta = -dtheta;
    }
    if !sweep && dtheta > 0.0 {
        dtheta -= 2.0 * core::f32::consts::PI;
    } else if sweep && dtheta < 0.0 {
        dtheta += 2.0 * core::f32::consts::PI;
    }
    let steps =
        (libm::ceilf(libm::fabsf(dtheta) / (core::f32::consts::PI / 8.0)) as i32).clamp(4, 24);
    let mut prev = (x0, y0);
    for i in 1..=steps {
        let t = theta1 + dtheta * (i as f32 / steps as f32);
        let x = cos_p * rx * libm::cosf(t) - sin_p * ry * libm::sinf(t) + cx;
        let y = sin_p * rx * libm::cosf(t) + cos_p * ry * libm::sinf(t) + cy;
        add_line(segs, prev.0, prev.1, x, y);
        prev = (x, y);
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn new(s: &'a str) -> Self {
        Self {
            s: s.as_bytes(),
            i: 0,
        }
    }

    fn skip(&mut self) {
        while self.i < self.s.len() {
            match self.s[self.i] {
                b' ' | b',' | b'\t' | b'\n' | b'\r' => self.i += 1,
                _ => break,
            }
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip();
        self.s.get(self.i).copied()
    }

    fn is_num_start(c: u8) -> bool {
        c == b'+' || c == b'-' || c == b'.' || c.is_ascii_digit()
    }

    fn number(&mut self) -> Option<f32> {
        self.skip();
        if self.i >= self.s.len() || !Self::is_num_start(self.s[self.i]) {
            return None;
        }
        let start = self.i;
        if self.s[self.i] == b'+' || self.s[self.i] == b'-' {
            self.i += 1;
        }
        while self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
            self.i += 1;
        }
        if self.i < self.s.len() && self.s[self.i] == b'.' {
            self.i += 1;
            while self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
                self.i += 1;
            }
        }
        if self.i < self.s.len() && (self.s[self.i] == b'e' || self.s[self.i] == b'E') {
            self.i += 1;
            if self.i < self.s.len() && (self.s[self.i] == b'+' || self.s[self.i] == b'-') {
                self.i += 1;
            }
            while self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
                self.i += 1;
            }
        }
        core::str::from_utf8(&self.s[start..self.i])
            .ok()
            .and_then(|t| t.parse().ok())
    }

    fn flag(&mut self) -> Option<bool> {
        self.skip();
        let c = *self.s.get(self.i)?;
        if c == b'0' || c == b'1' {
            self.i += 1;
            return Some(c == b'1');
        }
        None
    }
}

fn parse_path(d: &str, segs: &mut Vec<(f32, f32, f32, f32)>) {
    let mut p = Parser::new(d);
    let mut x = 0.0f32;
    let mut y = 0.0f32;
    let mut sx = 0.0f32;
    let mut sy = 0.0f32;
    let mut last_c = (x, y);
    let mut last_was_c = false;
    let mut cmd = b'M';
    while let Some(c) = p.peek() {
        if c.is_ascii_alphabetic() {
            cmd = c;
            p.i += 1;
        } else if cmd == 0 {
            break;
        }
        let rel = cmd.is_ascii_lowercase();
        let op = cmd.to_ascii_uppercase();
        match op {
            b'M' => {
                let Some(nx) = p.number() else { break };
                let Some(ny) = p.number() else { break };
                if rel {
                    x += nx;
                    y += ny;
                } else {
                    x = nx;
                    y = ny;
                }
                sx = x;
                sy = y;
                last_was_c = false;
                cmd = if rel { b'l' } else { b'L' };
            }
            b'L' => {
                let Some(nx) = p.number() else { break };
                let Some(ny) = p.number() else { break };
                let (x1, y1) = if rel { (x + nx, y + ny) } else { (nx, ny) };
                add_line(segs, x, y, x1, y1);
                x = x1;
                y = y1;
                last_was_c = false;
            }
            b'H' => {
                let Some(nx) = p.number() else { break };
                let x1 = if rel { x + nx } else { nx };
                add_line(segs, x, y, x1, y);
                x = x1;
                last_was_c = false;
            }
            b'V' => {
                let Some(ny) = p.number() else { break };
                let y1 = if rel { y + ny } else { ny };
                add_line(segs, x, y, x, y1);
                y = y1;
                last_was_c = false;
            }
            b'C' => {
                let Some(x1) = p.number() else { break };
                let Some(y1) = p.number() else { break };
                let Some(x2) = p.number() else { break };
                let Some(y2) = p.number() else { break };
                let Some(x3) = p.number() else { break };
                let Some(y3) = p.number() else { break };
                let c1 = if rel { (x + x1, y + y1) } else { (x1, y1) };
                let c2 = if rel { (x + x2, y + y2) } else { (x2, y2) };
                let p3 = if rel { (x + x3, y + y3) } else { (x3, y3) };
                cubic(segs, (x, y), c1, c2, p3);
                last_c = c2;
                last_was_c = true;
                x = p3.0;
                y = p3.1;
            }
            b'S' => {
                let Some(x2) = p.number() else { break };
                let Some(y2) = p.number() else { break };
                let Some(x3) = p.number() else { break };
                let Some(y3) = p.number() else { break };
                let c1 = if last_was_c {
                    (2.0 * x - last_c.0, 2.0 * y - last_c.1)
                } else {
                    (x, y)
                };
                let c2 = if rel { (x + x2, y + y2) } else { (x2, y2) };
                let p3 = if rel { (x + x3, y + y3) } else { (x3, y3) };
                cubic(segs, (x, y), c1, c2, p3);
                last_c = c2;
                last_was_c = true;
                x = p3.0;
                y = p3.1;
            }
            b'Q' => {
                let Some(x1) = p.number() else { break };
                let Some(y1) = p.number() else { break };
                let Some(x2) = p.number() else { break };
                let Some(y2) = p.number() else { break };
                let c1 = if rel { (x + x1, y + y1) } else { (x1, y1) };
                let p2 = if rel { (x + x2, y + y2) } else { (x2, y2) };
                quad(segs, (x, y), c1, p2);
                last_c = c1;
                last_was_c = true;
                x = p2.0;
                y = p2.1;
            }
            b'T' => {
                let Some(x2) = p.number() else { break };
                let Some(y2) = p.number() else { break };
                let c1 = if last_was_c {
                    (2.0 * x - last_c.0, 2.0 * y - last_c.1)
                } else {
                    (x, y)
                };
                let p2 = if rel { (x + x2, y + y2) } else { (x2, y2) };
                quad(segs, (x, y), c1, p2);
                last_c = c1;
                last_was_c = true;
                x = p2.0;
                y = p2.1;
            }
            b'A' => {
                let Some(rx) = p.number() else { break };
                let Some(ry) = p.number() else { break };
                let Some(rot) = p.number() else { break };
                let Some(large) = p.flag() else { break };
                let Some(sweep) = p.flag() else { break };
                let Some(nx) = p.number() else { break };
                let Some(ny) = p.number() else { break };
                let (x1, y1) = if rel { (x + nx, y + ny) } else { (nx, ny) };
                arc_to(segs, x, y, rx, ry, rot, large, sweep, x1, y1);
                x = x1;
                y = y1;
                last_was_c = false;
            }
            b'Z' => {
                add_line(segs, x, y, sx, sy);
                x = sx;
                y = sy;
                last_was_c = false;
            }
            _ => break,
        }
    }
}

fn circle_path(segs: &mut Vec<(f32, f32, f32, f32)>, cx: f32, cy: f32, r: f32) {
    arc_to(segs, cx + r, cy, r, r, 0.0, false, true, cx - r, cy);
    arc_to(segs, cx - r, cy, r, r, 0.0, false, true, cx + r, cy);
}

fn rounded_rect(segs: &mut Vec<(f32, f32, f32, f32)>, x: f32, y: f32, w: f32, h: f32, mut rx: f32) {
    rx = rx.min(w / 2.0).min(h / 2.0).max(0.0);
    if rx < 0.01 {
        add_line(segs, x, y, x + w, y);
        add_line(segs, x + w, y, x + w, y + h);
        add_line(segs, x + w, y + h, x, y + h);
        add_line(segs, x, y + h, x, y);
        return;
    }
    add_line(segs, x + rx, y, x + w - rx, y);
    arc_to(segs, x + w - rx, y, rx, rx, 0.0, false, true, x + w, y + rx);
    add_line(segs, x + w, y + rx, x + w, y + h - rx);
    arc_to(
        segs,
        x + w,
        y + h - rx,
        rx,
        rx,
        0.0,
        false,
        true,
        x + w - rx,
        y + h,
    );
    add_line(segs, x + w - rx, y + h, x + rx, y + h);
    arc_to(segs, x + rx, y + h, rx, rx, 0.0, false, true, x, y + h - rx);
    add_line(segs, x, y + h - rx, x, y + rx);
    arc_to(segs, x, y + rx, rx, rx, 0.0, false, true, x + rx, y);
}

fn flatten(name: IconName) -> Vec<(f32, f32, f32, f32)> {
    let mut segs = Vec::new();
    for prim in primitives(name) {
        match *prim {
            Prim::Path(d) => parse_path(d, &mut segs),
            Prim::Circle(cx, cy, r) => circle_path(&mut segs, cx, cy, r),
            Prim::Rect(x, y, w, h, rx) => rounded_rect(&mut segs, x, y, w, h, rx),
        }
    }
    segs
}

fn raster(name: IconName, size: i32) -> Vec<u8> {
    let size = size.max(8);
    let scale = size as f32 / VIEW;
    let half = (STROKE * scale) * 0.5;
    let mut buf = vec![0u8; (size * size) as usize];
    for (x0, y0, x1, y1) in flatten(name) {
        stamp_seg(
            &mut buf,
            size,
            x0 * scale,
            y0 * scale,
            x1 * scale,
            y1 * scale,
            half,
        );
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::{IconName, raster};

    #[test]
    fn every_icon_has_ink() {
        for &name in IconName::ALL {
            let ink = raster(name, 24).iter().filter(|p| **p > 0).count();
            assert!(ink > 16, "{name:?} should paint ink, got {ink}");
        }
    }

    #[test]
    fn lucide_stroke_is_coverage_aa() {
        let buf = raster(IconName::Check, 24);
        assert!(
            buf.iter().any(|&p| (1..255).contains(&p)),
            "Lucide strokes must anti-alias, not 1-bit stamps"
        );
    }

    #[test]
    fn for_label_maps_desktop_apps() {
        assert_eq!(IconName::for_label("Terminal"), IconName::SquareTerminal);
        assert_eq!(IconName::for_label("Files"), IconName::Folder);
        assert_eq!(IconName::for_label("Calculator"), IconName::Calculator);
        assert_eq!(IconName::for_label("Settings"), IconName::Settings);
    }
}
