/// Lucide stroke icons for compositor-owned chrome (taskbar, SSD, drag preview).
///
/// GPUI surfaces paint the same glyphs via `knox_ui::Icon`. This module CPU-strokes
/// those path tiles and `blend_pixel`s them — not the old Adwaita/Papirus `./icons`
/// blob and not runtime resvg.
use knox_ui::IconName;

use super::desktop::{ContextAction, IconType};
use super::framebuffer::{FrameBuffer, Pixel};
use super::window::{ViewKind, WindowContentType};

pub const ICON_SIZE_16: u32 = 16;
pub const ICON_SIZE_24: u32 = 24;
pub const ICON_SIZE_32: u32 = 32;
pub const ICON_SIZE_48: u32 = 48;

const TINT: Pixel = Pixel::new(240, 232, 224, 255);

/// No registry to load; Lucide tiles are rasterized on demand.
pub fn init() {}

pub fn for_desktop(icon_type: IconType) -> IconName {
    match icon_type {
        IconType::MyPC => IconName::Monitor,
        IconType::Folder => IconName::Folder,
        IconType::Document => IconName::FileText,
        IconType::Globe => IconName::Globe,
        IconType::Terminal => IconName::SquareTerminal,
        IconType::MediaPlayer => IconName::Play,
        IconType::Game => IconName::Gamepad,
        IconType::AIBrain => IconName::Sparkles,
        IconType::Settings => IconName::Settings,
        IconType::Trash => IconName::Trash,
        IconType::Image => IconName::Image,
        IconType::Archive => IconName::Archive,
        IconType::Script => IconName::FileText,
    }
}

pub fn for_content(ct: WindowContentType) -> IconName {
    match ct {
        WindowContentType::TerminalSurface => IconName::SquareTerminal,
        WindowContentType::ShmEmpty => IconName::AppWindow,
        WindowContentType::Gpui(ViewKind::Files) => IconName::Folder,
        WindowContentType::Gpui(ViewKind::Browser) => IconName::Globe,
        WindowContentType::Gpui(ViewKind::Chat) => IconName::Sparkles,
        WindowContentType::Gpui(ViewKind::Editor) => IconName::FileText,
        WindowContentType::Gpui(ViewKind::Settings) => IconName::Settings,
        WindowContentType::Gpui(ViewKind::Archive) => IconName::Archive,
        WindowContentType::Gpui(ViewKind::Disks) => IconName::HardDrive,
        WindowContentType::Gpui(ViewKind::Bluetooth) => IconName::Bluetooth,
        WindowContentType::Gpui(ViewKind::Calendar) => IconName::Calendar,
        WindowContentType::Gpui(ViewKind::Logs) => IconName::FileText,
        WindowContentType::Gpui(ViewKind::Updater) => IconName::RefreshCw,
        WindowContentType::Gpui(ViewKind::Software) => IconName::LayoutGrid,
        WindowContentType::Gpui(ViewKind::Wizard) => IconName::Info,
        WindowContentType::Gpui(ViewKind::Tasks) => IconName::Monitor,
        WindowContentType::Gpui(ViewKind::Calculator) => IconName::Calculator,
        WindowContentType::Gpui(ViewKind::Image) => IconName::Image,
    }
}

pub fn for_action(action: ContextAction) -> Option<IconName> {
    match action {
        ContextAction::OpenTerminal => Some(IconName::SquareTerminal),
        ContextAction::OpenExplorer => Some(IconName::Folder),
        ContextAction::OpenBrowser => Some(IconName::Globe),
        ContextAction::Refresh => Some(IconName::RefreshCw),
        ContextAction::NewFile => Some(IconName::File),
        ContextAction::NewFolder => Some(IconName::Folder),
        ContextAction::TakeScreenshot | ContextAction::TakeScreenshotRegion => {
            Some(IconName::Image)
        }
        ContextAction::ChangeWallpaper => Some(IconName::Image),
        ContextAction::DisplaySettings => Some(IconName::Settings),
        ContextAction::None | ContextAction::Separator => None,
    }
}

pub fn blit(fb: &mut FrameBuffer, x: i32, y: i32, name: IconName, size: i32) {
    blit_tinted(fb, x, y, name, size, TINT);
}

pub fn blit_tinted(fb: &mut FrameBuffer, x: i32, y: i32, name: IconName, size: i32, tint: Pixel) {
    let size = size.max(8);
    let cover = name.raster(size);
    for row in 0..size {
        for col in 0..size {
            let a = cover[(row * size + col) as usize];
            if a == 0 {
                continue;
            }
            let px = x + col;
            let py = y + row;
            if px < 0 || py < 0 {
                continue;
            }
            fb.blend_pixel(
                px as usize,
                py as usize,
                Pixel::new(tint.r, tint.g, tint.b, a),
            );
        }
    }
}

pub fn draw_desktop_icon(fb: &mut FrameBuffer, x: i32, y: i32, name: IconName) {
    blit(fb, x, y, name, ICON_SIZE_48 as i32);
}

pub fn draw_small_icon(fb: &mut FrameBuffer, x: i32, y: i32, name: IconName) {
    blit(fb, x, y, name, ICON_SIZE_24 as i32);
}

pub fn draw_tiny_icon(fb: &mut FrameBuffer, x: i32, y: i32, name: IconName) {
    blit(fb, x, y, name, ICON_SIZE_16 as i32);
}
