/// Desktop types, grid layout, and global desktop state
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::gui::framebuffer::Rect;

/// Desktop icon entry
#[derive(Clone)]
pub struct DesktopIcon {
    pub name: String,
    pub icon_type: IconType,
    pub x: i32,
    pub y: i32,
    pub selected: bool,
    pub is_shortcut: bool,
    /// Filesystem path this icon represents (empty for shortcuts)
    pub path: String,
}

#[derive(Clone, Copy, PartialEq)]
pub enum IconType {
    MyPC,
    Folder,
    Document,
    Globe,
    Terminal,
    MediaPlayer,
    Game,
    AIBrain,
    Settings,
    Trash,
    Image,
    Archive,
    Script,
}

/// Grid layout constants — scaled for 1920×1080
pub(crate) const ICON_GRID_X: i32 = 28; // Left margin for desktop area
pub(crate) const ICON_GRID_Y: i32 = 18; // Top margin
pub(crate) const ICON_GRID_SPACING_X: i32 = 110; // Horizontal spacing between columns
pub(crate) const ICON_GRID_SPACING_Y: i32 = 100; // Vertical spacing — compact for 1080p
pub(crate) const ICON_WIDTH: u32 = 80; // gridEntryWidth
pub(crate) const ICON_HEIGHT: u32 = 78; // gridEntryHeight
pub(crate) const ICON_SIZE_ACTUAL: i32 = 48; // Actual icon size (48x48 pixels)
pub(crate) const TEXT_LABEL_MARGIN_TOP: i32 = 6; // Space between icon and text
pub(crate) const TEXT_LABEL_PADDING_LEFT: i32 = 10; // Left padding for text

/// Desktop right-click context menu item
#[derive(Clone)]
pub struct ContextMenuItem {
    pub label: String,
    pub separator: bool,
    pub action: ContextAction,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ContextAction {
    None,
    OpenTerminal,
    OpenBrowser,
    OpenExplorer,
    NewFile,
    NewFolder,
    Refresh,
    TakeScreenshot,
    TakeScreenshotRegion,
    ChangeWallpaper,
    DisplaySettings,
    Separator,
}

/// Desktop context menu state
pub struct DesktopContextMenu {
    pub visible: bool,
    pub x: i32,
    pub y: i32,
    pub items: Vec<ContextMenuItem>,
}

lazy_static::lazy_static! {
    pub static ref CONTEXT_MENU: Mutex<DesktopContextMenu> = Mutex::new(DesktopContextMenu {
        visible: false,
        x: 0,
        y: 0,
        items: vec![
            ContextMenuItem { label: String::from("Open Terminal"), separator: false, action: ContextAction::OpenTerminal },
            ContextMenuItem { label: String::from("Open Files"), separator: false, action: ContextAction::OpenExplorer },
            ContextMenuItem { label: String::from("Open Browser"), separator: false, action: ContextAction::OpenBrowser },
            ContextMenuItem { label: String::from(""), separator: true, action: ContextAction::Separator },
            ContextMenuItem { label: String::from("New File"), separator: false, action: ContextAction::NewFile },
            ContextMenuItem { label: String::from("New Folder"), separator: false, action: ContextAction::NewFolder },
            ContextMenuItem { label: String::from(""), separator: true, action: ContextAction::Separator },
            ContextMenuItem { label: String::from("Screenshot"), separator: false, action: ContextAction::TakeScreenshot },
            ContextMenuItem { label: String::from("Screenshot Region"), separator: false, action: ContextAction::TakeScreenshotRegion },
            ContextMenuItem { label: String::from("Change Wallpaper"), separator: false, action: ContextAction::ChangeWallpaper },
            ContextMenuItem { label: String::from("Display Settings"), separator: false, action: ContextAction::DisplaySettings },
            ContextMenuItem { label: String::from(""), separator: true, action: ContextAction::Separator },
            ContextMenuItem { label: String::from("Refresh Desktop"), separator: false, action: ContextAction::Refresh },
        ],
    });
}

/// The desktop state
pub struct Desktop {
    pub icons: Vec<DesktopIcon>,
    pub selected_icon: Option<usize>,
    /// Icon being dragged — index into `icons`
    pub dragging_icon: Option<usize>,
    /// Drag offset from icon origin to mouse position
    pub drag_offset_x: i32,
    pub drag_offset_y: i32,
    /// Current drag position (icon origin while dragging)
    pub drag_x: i32,
    pub drag_y: i32,
    /// Rubber band selection state
    pub rubber_band: Option<RubberBand>,
}

/// Rubber band selection rectangle state
#[derive(Clone, Copy)]
pub struct RubberBand {
    /// Start point (where mouse was pressed)
    pub start_x: i32,
    pub start_y: i32,
    /// Current point (where mouse is now)
    pub end_x: i32,
    pub end_y: i32,
}

lazy_static::lazy_static! {
    pub static ref DESKTOP: Mutex<Desktop> = Mutex::new(Desktop::new());
}

impl Default for Desktop {
    fn default() -> Self {
        Self::new()
    }
}

impl Desktop {
    pub fn new() -> Self {
        // Create desktop icons — a comprehensive set
        let icons = vec![
            DesktopIcon {
                name: String::from("Files"),
                icon_type: IconType::MyPC,
                x: ICON_GRID_X,
                y: ICON_GRID_Y,
                selected: false,
                is_shortcut: false,
                path: String::from("/home"),
            },
            DesktopIcon {
                name: String::from("Terminal"),
                icon_type: IconType::Terminal,
                x: ICON_GRID_X,
                y: ICON_GRID_Y + ICON_GRID_SPACING_Y,
                selected: false,
                is_shortcut: true,
                path: String::new(),
            },
            DesktopIcon {
                name: String::from("Browser"),
                icon_type: IconType::Globe,
                x: ICON_GRID_X,
                y: ICON_GRID_Y + ICON_GRID_SPACING_Y * 2,
                selected: false,
                is_shortcut: true,
                path: String::new(),
            },
            DesktopIcon {
                name: String::from("Documents"),
                icon_type: IconType::Folder,
                x: ICON_GRID_X,
                y: ICON_GRID_Y + ICON_GRID_SPACING_Y * 3,
                selected: false,
                is_shortcut: false,
                path: String::from("/home/user/Documents"),
            },
            DesktopIcon {
                name: String::from("AI Assistant"),
                icon_type: IconType::AIBrain,
                x: ICON_GRID_X,
                y: ICON_GRID_Y + ICON_GRID_SPACING_Y * 4,
                selected: false,
                is_shortcut: true,
                path: String::new(),
            },
            DesktopIcon {
                name: String::from("Settings"),
                icon_type: IconType::Settings,
                x: ICON_GRID_X,
                y: ICON_GRID_Y + ICON_GRID_SPACING_Y * 5,
                selected: false,
                is_shortcut: true,
                path: String::new(),
            },
            DesktopIcon {
                name: String::from("Trash"),
                icon_type: IconType::Trash,
                x: ICON_GRID_X,
                y: ICON_GRID_Y + ICON_GRID_SPACING_Y * 6,
                selected: false,
                is_shortcut: false,
                path: String::from("/home/user/.trash"),
            },
        ];

        Self {
            icons,
            selected_icon: None,
            dragging_icon: None,
            drag_offset_x: 0,
            drag_offset_y: 0,
            drag_x: 0,
            drag_y: 0,
            rubber_band: None,
        }
    }
}

impl RubberBand {
    /// Get the normalized (top-left origin) rectangle
    pub fn to_rect(&self) -> Rect {
        let x0 = self.start_x.min(self.end_x);
        let y0 = self.start_y.min(self.end_y);
        let x1 = self.start_x.max(self.end_x);
        let y1 = self.start_y.max(self.end_y);
        Rect::new(x0, y0, (x1 - x0).max(1) as u32, (y1 - y0).max(1) as u32)
    }
}
