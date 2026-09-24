/// Mouse cursor themes, types, drawing, and background save/restore
use spin::Mutex;

use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::gui::window;

use super::types::{DESKTOP, ICON_HEIGHT, ICON_WIDTH};

/// Invalidate the saved cursor background (call after resolution change)
pub fn invalidate_cursor_bg() {
    CURSOR_BG_VALID.store(false, core::sync::atomic::Ordering::Relaxed);
}

// ═══════════════════════════════════════════════════════════════════════
// CURSOR — fast overlay with background save/restore
// ═══════════════════════════════════════════════════════════════════════

/// Cursor sprite dimensions (just the arrow shape)
pub const CURSOR_W: usize = 16;
pub const CURSOR_H: usize = 20;

// ─── Cursor Theme System ─────────────────────────────────────────────
/// Cursor visual style
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CursorTheme {
    Default = 0, // White cursor with black border
    Dark = 1,    // Black cursor with white border
    Accent = 2,  // Theme accent color cursor
    Large = 3,   // Larger white cursor for accessibility
}

static CURSOR_THEME: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

pub fn set_cursor_theme(theme: CursorTheme) {
    CURSOR_THEME.store(theme as u8, core::sync::atomic::Ordering::Relaxed);
}

pub fn cursor_theme() -> CursorTheme {
    match CURSOR_THEME.load(core::sync::atomic::Ordering::Relaxed) {
        1 => CursorTheme::Dark,
        2 => CursorTheme::Accent,
        3 => CursorTheme::Large,
        _ => CursorTheme::Default,
    }
}

/// Get cursor fill and border colors based on theme
pub fn cursor_colors() -> (Pixel, Pixel) {
    match cursor_theme() {
        CursorTheme::Default | CursorTheme::Large => {
            (Pixel::rgb(255, 255, 255), Pixel::rgb(0, 0, 0))
        }
        CursorTheme::Dark => (Pixel::rgb(0, 0, 0), Pixel::rgb(255, 255, 255)),
        CursorTheme::Accent => {
            let accent = crate::gui::theme::accent_color();
            (accent, Pixel::rgb(0, 0, 0))
        }
    }
}

/// Full-featured cursor type matching egui::CursorIcon (35 variants)
#[derive(Clone, Copy, PartialEq)]
pub enum CursorType {
    /// Normal arrow cursor
    Default, // 0
    /// Show no cursor
    None, // 1
    /// A context menu is available
    ContextMenu, // 2
    /// Question mark / help
    Help, // 3
    /// Pointing hand for links
    PointingHand, // 4
    /// Processing but still interactive
    Progress, // 5
    /// Not yet ready, try later (hourglass/spinner)
    Wait, // 6
    /// Hover a cell in a table
    Cell, // 7
    /// Precision crosshair
    Crosshair, // 8
    /// Text caret (I-beam)
    Text, // 9
    /// Vertical text caret
    VerticalText, // 10
    /// Alias / shortcut
    Alias, // 11
    /// Copy indicator
    Copy, // 12
    /// Omnidirectional move (arrows in all directions)
    Move, // 13
    /// Can't drop here
    NoDrop, // 14
    /// Forbidden / not allowed
    NotAllowed, // 15
    /// The thing can be grabbed
    Grab, // 16
    /// You are grabbing
    Grabbing, // 17
    /// Something can be scrolled in any direction
    AllScroll, // 18
    /// Horizontal resize ↔
    ResizeHorizontal, // 19
    /// Diagonal resize ↗↙ (NE-SW)
    ResizeNeSw, // 20
    /// Diagonal resize ↘↖ (NW-SE)
    ResizeNwSe, // 21
    /// Vertical resize ↕
    ResizeVertical, // 22
    /// Resize East →
    ResizeEast, // 23
    /// Resize South-East ↘
    ResizeSouthEast, // 24
    /// Resize South ↓
    ResizeSouth, // 25
    /// Resize South-West ↙
    ResizeSouthWest, // 26
    /// Resize West ←
    ResizeWest, // 27
    /// Resize North-West ↖
    ResizeNorthWest, // 28
    /// Resize North ↑
    ResizeNorth, // 29
    /// Resize North-East ↗
    ResizeNorthEast, // 30
    /// Resize column (left-right with vertical bars)
    ResizeColumn, // 31
    /// Resize row (up-down with horizontal bars)
    ResizeRow, // 32
    /// Zoom in (+)
    ZoomIn, // 33
    /// Zoom out (−)
    ZoomOut, // 34
}

/// Current cursor type (determined by hover context)
static CURSOR_TYPE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0); // 0 = Default

pub fn set_cursor_type(ct: CursorType) {
    let val = match ct {
        CursorType::Default => 0,
        CursorType::None => 1,
        CursorType::ContextMenu => 2,
        CursorType::Help => 3,
        CursorType::PointingHand => 4,
        CursorType::Progress => 5,
        CursorType::Wait => 6,
        CursorType::Cell => 7,
        CursorType::Crosshair => 8,
        CursorType::Text => 9,
        CursorType::VerticalText => 10,
        CursorType::Alias => 11,
        CursorType::Copy => 12,
        CursorType::Move => 13,
        CursorType::NoDrop => 14,
        CursorType::NotAllowed => 15,
        CursorType::Grab => 16,
        CursorType::Grabbing => 17,
        CursorType::AllScroll => 18,
        CursorType::ResizeHorizontal => 19,
        CursorType::ResizeNeSw => 20,
        CursorType::ResizeNwSe => 21,
        CursorType::ResizeVertical => 22,
        CursorType::ResizeEast => 23,
        CursorType::ResizeSouthEast => 24,
        CursorType::ResizeSouth => 25,
        CursorType::ResizeSouthWest => 26,
        CursorType::ResizeWest => 27,
        CursorType::ResizeNorthWest => 28,
        CursorType::ResizeNorth => 29,
        CursorType::ResizeNorthEast => 30,
        CursorType::ResizeColumn => 31,
        CursorType::ResizeRow => 32,
        CursorType::ZoomIn => 33,
        CursorType::ZoomOut => 34,
    };
    CURSOR_TYPE.store(val, core::sync::atomic::Ordering::Relaxed);
}

pub fn get_cursor_type() -> CursorType {
    match CURSOR_TYPE.load(core::sync::atomic::Ordering::Relaxed) {
        0 => CursorType::Default,
        1 => CursorType::None,
        2 => CursorType::ContextMenu,
        3 => CursorType::Help,
        4 => CursorType::PointingHand,
        5 => CursorType::Progress,
        6 => CursorType::Wait,
        7 => CursorType::Cell,
        8 => CursorType::Crosshair,
        9 => CursorType::Text,
        10 => CursorType::VerticalText,
        11 => CursorType::Alias,
        12 => CursorType::Copy,
        13 => CursorType::Move,
        14 => CursorType::NoDrop,
        15 => CursorType::NotAllowed,
        16 => CursorType::Grab,
        17 => CursorType::Grabbing,
        18 => CursorType::AllScroll,
        19 => CursorType::ResizeHorizontal,
        20 => CursorType::ResizeNeSw,
        21 => CursorType::ResizeNwSe,
        22 => CursorType::ResizeVertical,
        23 => CursorType::ResizeEast,
        24 => CursorType::ResizeSouthEast,
        25 => CursorType::ResizeSouth,
        26 => CursorType::ResizeSouthWest,
        27 => CursorType::ResizeWest,
        28 => CursorType::ResizeNorthWest,
        29 => CursorType::ResizeNorth,
        30 => CursorType::ResizeNorthEast,
        31 => CursorType::ResizeColumn,
        32 => CursorType::ResizeRow,
        33 => CursorType::ZoomIn,
        34 => CursorType::ZoomOut,
        _ => CursorType::Default,
    }
}

/// Check if a position is over any desktop icon
pub fn is_over_desktop_icon(mx: i32, my: i32) -> bool {
    let desktop = DESKTOP.lock();
    for icon in desktop.icons.iter() {
        let icon_rect = Rect::new(icon.x, icon.y, ICON_WIDTH, ICON_HEIGHT);
        if icon_rect.contains(mx, my) {
            return true;
        }
    }
    false
}

/// Update cursor type based on current mouse position and window state
pub fn update_cursor_for_position(mx: i32, my: i32) {
    use window::ResizeEdge;

    let wm = window::WINDOW_MANAGER.lock();

    // If actively dragging, show move cursor
    if wm.any_dragging() {
        drop(wm);
        set_cursor_type(CursorType::Grabbing);
        return;
    }

    // If actively resizing, keep the resize cursor
    if wm.any_resizing() {
        // Don't change — the resize cursor was already set
        return;
    }

    // Check for title bar button hover FIRST (before resize edges).
    // Buttons sit in the top-right corner where resize edges overlap.
    // Without this priority, the cursor shows resize arrows over buttons.
    if let Some(wid) = wm.window_at(mx, my) {
        if let Some(win) = wm.windows.iter().rev().find(|w| w.id == wid) {
            let on_close = win.closeable && win.close_button_rect().contains(mx, my);
            let on_max = win.maximizable && win.maximize_button_rect().contains(mx, my);
            let on_min = win.minimizable && win.minimize_button_rect().contains(mx, my);
            if on_close || on_max || on_min {
                drop(wm);
                set_cursor_type(CursorType::PointingHand);
                return;
            }
        }
    }

    // Check for resize edge hover
    let (_, edge) = wm.resize_edge_at(mx, my);
    drop(wm);

    let ct = match edge {
        ResizeEdge::Left => CursorType::ResizeWest,
        ResizeEdge::Right => CursorType::ResizeEast,
        ResizeEdge::Top => CursorType::ResizeNorth,
        ResizeEdge::Bottom => CursorType::ResizeSouth,
        ResizeEdge::TopLeft => CursorType::ResizeNorthWest,
        ResizeEdge::BottomRight => CursorType::ResizeSouthEast,
        ResizeEdge::TopRight => CursorType::ResizeNorthEast,
        ResizeEdge::BottomLeft => CursorType::ResizeSouthWest,
        ResizeEdge::None => {
            // Check if hovering over a desktop icon — show hand cursor
            if is_over_desktop_icon(mx, my) {
                CursorType::PointingHand
            } else {
                CursorType::Default
            }
        }
    };
    set_cursor_type(ct);
}

/// Save/restore area dimensions.
/// Must cover the largest cursor footprint across ALL cursor types:
///  - Arrow/Hand: 16×20 shape + 1px shadow = 17×21 starting at (x, y)
///  - Resize cursors: ±7 pixels centered on (x, y) = 15×15 centered
///  - Zoom cursors: ~16×16 + lens radius starting at (x, y)
///  - Move/AllScroll: ±8 centered = 17×17 centered
///    We use a generous area with a negative offset so it covers everything.
pub const CURSOR_SAVE_PAD: i32 = 10; // pixels before the cursor position
pub const CURSOR_SAVE_W: usize = 32; // total save width
pub const CURSOR_SAVE_H: usize = 34; // total save height

/// Saved pixels underneath the cursor (CURSOR_SAVE_W × CURSOR_SAVE_H × 4 bytes BGRA)
static CURSOR_BG: Mutex<[u8; CURSOR_SAVE_W * CURSOR_SAVE_H * 4]> =
    Mutex::new([0u8; CURSOR_SAVE_W * CURSOR_SAVE_H * 4]);

/// Whether we have a valid saved background
static CURSOR_BG_VALID: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Last saved area origin (NOT the cursor position — offset by CURSOR_SAVE_PAD)
static LAST_SAVE_X: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);
static LAST_SAVE_Y: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);

/// Last drawn cursor position (for change detection)
static LAST_CURSOR_X: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);
static LAST_CURSOR_Y: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);

/// Get last cursor position
pub fn last_cursor_pos() -> (i32, i32) {
    (
        LAST_CURSOR_X.load(core::sync::atomic::Ordering::Relaxed),
        LAST_CURSOR_Y.load(core::sync::atomic::Ordering::Relaxed),
    )
}

/// Set last cursor position
pub fn set_last_cursor_pos(x: i32, y: i32) {
    LAST_CURSOR_X.store(x, core::sync::atomic::Ordering::Relaxed);
    LAST_CURSOR_Y.store(y, core::sync::atomic::Ordering::Relaxed);
}

/// Save the framebuffer pixels that will be covered by the cursor.
/// The save area starts at (x - CURSOR_SAVE_PAD, y - CURSOR_SAVE_PAD) to
/// cover all cursor types including resize cursors that draw centered on (x,y).
pub fn save_cursor_background(fb: &FrameBuffer, x: i32, y: i32) {
    let sx = x - CURSOR_SAVE_PAD;
    let sy = y - CURSOR_SAVE_PAD;
    // Remember the save origin for restore
    LAST_SAVE_X.store(sx, core::sync::atomic::Ordering::Relaxed);
    LAST_SAVE_Y.store(sy, core::sync::atomic::Ordering::Relaxed);

    let mut bg = CURSOR_BG.lock();
    let bpp = fb.bytes_per_pixel;
    for row in 0..CURSOR_SAVE_H {
        let py = sy + row as i32;
        if py < 0 || py as usize >= fb.height {
            let dst_start = row * CURSOR_SAVE_W * 4;
            for col in 0..CURSOR_SAVE_W {
                let dst = dst_start + col * 4;
                bg[dst] = 0;
                bg[dst + 1] = 0;
                bg[dst + 2] = 0;
                bg[dst + 3] = 255;
            }
            continue;
        }
        for col in 0..CURSOR_SAVE_W {
            let px = sx + col as i32;
            let dst = (row * CURSOR_SAVE_W + col) * 4;
            if px >= 0 && (px as usize) < fb.width {
                let src = (py as usize) * fb.pitch + (px as usize) * bpp;
                if src + 3 < fb.buffer.len() {
                    bg[dst] = fb.buffer[src];
                    bg[dst + 1] = fb.buffer[src + 1];
                    bg[dst + 2] = fb.buffer[src + 2];
                    bg[dst + 3] = if bpp >= 4 { fb.buffer[src + 3] } else { 255 };
                    continue;
                }
            }
            bg[dst] = 0;
            bg[dst + 1] = 0;
            bg[dst + 2] = 0;
            bg[dst + 3] = 255;
        }
    }
    CURSOR_BG_VALID.store(true, core::sync::atomic::Ordering::Relaxed);
}

/// Restore the framebuffer pixels that were saved before the cursor was drawn.
/// Uses the save origin (LAST_SAVE_X/Y), not the cursor position.
pub fn restore_cursor_background(fb: &mut FrameBuffer) {
    if !CURSOR_BG_VALID.load(core::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let ox = LAST_SAVE_X.load(core::sync::atomic::Ordering::Relaxed);
    let oy = LAST_SAVE_Y.load(core::sync::atomic::Ordering::Relaxed);
    let bg = CURSOR_BG.lock();
    let bpp = fb.bytes_per_pixel;
    for row in 0..CURSOR_SAVE_H {
        let py = oy + row as i32;
        if py < 0 || py as usize >= fb.height {
            continue;
        }
        for col in 0..CURSOR_SAVE_W {
            let px = ox + col as i32;
            if px >= 0 && (px as usize) < fb.width {
                let src = (row * CURSOR_SAVE_W + col) * 4;
                let dst = (py as usize) * fb.pitch + (px as usize) * bpp;
                if dst + 3 < fb.buffer.len() {
                    fb.buffer[dst] = bg[src];
                    fb.buffer[dst + 1] = bg[src + 1];
                    fb.buffer[dst + 2] = bg[src + 2];
                    if bpp >= 4 {
                        fb.buffer[dst + 3] = bg[src + 3];
                    }
                }
            }
        }
    }
}

/// Draw the mouse cursor — full-featured matching egui::CursorIcon
///
/// Palette for bitmap cursors: 0=transparent, 1=black border, 2=white fill, 3=anti-alias edge
///
/// NOTE: Cursor type is determined by the input handler (process_mouse_byte)
/// which calls update_cursor_for_position() on mouse move. We do NOT call it
/// here to avoid locking WINDOW_MANAGER/DESKTOP during the fast cursor-only path.
pub fn draw_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    let cursor_type = get_cursor_type();
    match cursor_type {
        CursorType::Default => draw_arrow_cursor(fb, x, y),
        CursorType::None => { /* invisible — draw nothing */ }
        CursorType::ContextMenu => draw_context_menu_cursor(fb, x, y),
        CursorType::Help => draw_help_cursor(fb, x, y),
        CursorType::PointingHand => draw_hand_cursor(fb, x, y),
        CursorType::Progress => draw_progress_cursor(fb, x, y),
        CursorType::Wait => draw_wait_cursor(fb, x, y),
        CursorType::Cell => draw_cell_cursor(fb, x, y),
        CursorType::Crosshair => draw_crosshair_cursor(fb, x, y),
        CursorType::Text => draw_text_cursor(fb, x, y),
        CursorType::VerticalText => draw_vertical_text_cursor(fb, x, y),
        CursorType::Alias => draw_alias_cursor(fb, x, y),
        CursorType::Copy => draw_copy_cursor(fb, x, y),
        CursorType::Move => draw_move_cursor(fb, x, y),
        CursorType::NoDrop => draw_no_drop_cursor(fb, x, y),
        CursorType::NotAllowed => draw_not_allowed_cursor(fb, x, y),
        CursorType::Grab => draw_grab_cursor(fb, x, y),
        CursorType::Grabbing => draw_grabbing_cursor(fb, x, y),
        CursorType::AllScroll => draw_all_scroll_cursor(fb, x, y),
        CursorType::ResizeHorizontal => draw_resize_h_cursor(fb, x, y),
        CursorType::ResizeNeSw => draw_resize_diag_trbl_cursor(fb, x, y),
        CursorType::ResizeNwSe => draw_resize_diag_tlbr_cursor(fb, x, y),
        CursorType::ResizeVertical => draw_resize_v_cursor(fb, x, y),
        CursorType::ResizeEast => draw_resize_east_cursor(fb, x, y),
        CursorType::ResizeSouthEast => draw_resize_diag_tlbr_cursor(fb, x, y),
        CursorType::ResizeSouth => draw_resize_south_cursor(fb, x, y),
        CursorType::ResizeSouthWest => draw_resize_diag_trbl_cursor(fb, x, y),
        CursorType::ResizeWest => draw_resize_west_cursor(fb, x, y),
        CursorType::ResizeNorthWest => draw_resize_diag_tlbr_cursor(fb, x, y),
        CursorType::ResizeNorth => draw_resize_north_cursor(fb, x, y),
        CursorType::ResizeNorthEast => draw_resize_diag_trbl_cursor(fb, x, y),
        CursorType::ResizeColumn => draw_resize_column_cursor(fb, x, y),
        CursorType::ResizeRow => draw_resize_row_cursor(fb, x, y),
        CursorType::ZoomIn => draw_zoom_in_cursor(fb, x, y),
        CursorType::ZoomOut => draw_zoom_out_cursor(fb, x, y),
    }
}

/// Helper: draw a bitmap cursor with shadow from a 2D array.
/// Palette: 0=transparent, 1=border, 2=fill, 3=AA edge (50% border blend),
///          4=light gray fill, 5=mid gray fill
/// Colors are determined by the active cursor theme.
fn draw_bitmap_cursor<const W: usize, const H: usize>(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    data: &[[u8; W]; H],
) {
    use crate::gui::framebuffer::Pixel;
    let (fill_color, border_color) = cursor_colors();

    // Draw drop shadow first (offset +1,+1, semi-transparent black)
    for (row, line) in data.iter().enumerate() {
        for (col, &pixel) in line.iter().enumerate() {
            if pixel == 1 || pixel == 2 || pixel == 4 || pixel == 5 {
                let px = x + col as i32 + 1;
                let py = y + row as i32 + 1;
                if px >= 0 && py >= 0 && px < fb.width as i32 && py < fb.height as i32 {
                    let ux = px as usize;
                    let uy = py as usize;
                    let off = uy * fb.pitch + ux * fb.bytes_per_pixel;
                    if off + 2 < fb.buffer.len() {
                        let ob = fb.buffer[off] as u16;
                        let og = fb.buffer[off + 1] as u16;
                        let or_ = fb.buffer[off + 2] as u16;
                        fb.buffer[off] = (ob * 70 / 100) as u8;
                        fb.buffer[off + 1] = (og * 70 / 100) as u8;
                        fb.buffer[off + 2] = (or_ * 70 / 100) as u8;
                    }
                }
            }
        }
    }

    // Derive sub-colors from fill
    let light_gray = Pixel::lerp(fill_color, border_color, 60);
    let mid_gray = Pixel::lerp(fill_color, border_color, 100);

    // Draw cursor shape (on top of shadow)
    for (row, line) in data.iter().enumerate() {
        for (col, &pixel) in line.iter().enumerate() {
            if pixel == 0 {
                continue;
            }
            let px = x + col as i32;
            let py = y + row as i32;
            if px >= 0 && py >= 0 && px < fb.width as i32 && py < fb.height as i32 {
                let ux = px as usize;
                let uy = py as usize;
                match pixel {
                    1 => fb.set_pixel(ux, uy, border_color),
                    2 => fb.set_pixel(ux, uy, fill_color),
                    3 => {
                        // AA edge: 50% blend toward border color
                        let off = uy * fb.pitch + ux * fb.bytes_per_pixel;
                        if off + 2 < fb.buffer.len() {
                            let ob = fb.buffer[off] as u16;
                            let og = fb.buffer[off + 1] as u16;
                            let or_ = fb.buffer[off + 2] as u16;
                            fb.buffer[off] =
                                ((ob * 128 + border_color.b as u16 * 128 + 128) >> 8) as u8;
                            fb.buffer[off + 1] =
                                ((og * 128 + border_color.g as u16 * 128 + 128) >> 8) as u8;
                            fb.buffer[off + 2] =
                                ((or_ * 128 + border_color.r as u16 * 128 + 128) >> 8) as u8;
                        }
                    }
                    4 => fb.set_pixel(ux, uy, light_gray),
                    5 => fb.set_pixel(ux, uy, mid_gray),
                    _ => {}
                }
            }
        }
    }
}

/// Helper: draw a bitmap cursor centered on (x,y).
fn draw_bitmap_cursor_centered<const W: usize, const H: usize>(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    data: &[[u8; W]; H],
) {
    let ox = x - (W as i32) / 2;
    let oy = y - (H as i32) / 2;
    draw_bitmap_cursor(fb, ox, oy, data);
}

// ═══════════════════════════════════════════════════════════════════════
// 1. Default — Standard arrow cursor  (hotspot: top-left)
// ═══════════════════════════════════════════════════════════════════════
fn draw_arrow_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    // 0=transparent 1=black 2=white 3=AA
    #[rustfmt::skip]
    const D: [[u8; 15]; 21] = [
        [1,3,0,0,0,0,0,0,0,0,0,0,0,0,0],
        [1,1,3,0,0,0,0,0,0,0,0,0,0,0,0],
        [1,2,1,3,0,0,0,0,0,0,0,0,0,0,0],
        [1,2,2,1,3,0,0,0,0,0,0,0,0,0,0],
        [1,2,2,2,1,3,0,0,0,0,0,0,0,0,0],
        [1,2,2,2,2,1,3,0,0,0,0,0,0,0,0],
        [1,2,2,2,2,2,1,3,0,0,0,0,0,0,0],
        [1,2,2,2,2,2,2,1,3,0,0,0,0,0,0],
        [1,2,2,2,2,2,2,2,1,3,0,0,0,0,0],
        [1,2,2,2,2,2,2,2,2,1,3,0,0,0,0],
        [1,2,2,2,2,2,2,2,2,2,1,3,0,0,0],
        [1,2,2,2,2,2,2,2,2,2,2,1,3,0,0],
        [1,2,2,2,2,2,2,2,2,2,2,2,1,3,0],
        [1,2,2,2,2,2,2,1,1,1,1,1,1,1,0],
        [1,2,2,2,2,1,2,2,1,3,0,0,0,0,0],
        [1,2,2,2,1,0,1,2,2,1,3,0,0,0,0],
        [1,2,2,1,3,0,1,2,2,1,3,0,0,0,0],
        [1,2,1,3,0,0,0,1,2,2,1,3,0,0,0],
        [1,1,3,0,0,0,0,1,2,2,1,3,0,0,0],
        [1,3,0,0,0,0,0,0,1,1,1,3,0,0,0],
        [3,0,0,0,0,0,0,0,0,3,0,0,0,0,0],
    ];
    draw_bitmap_cursor(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 2. ContextMenu — Arrow + tiny menu  (hotspot: top-left)
// ═══════════════════════════════════════════════════════════════════════
fn draw_context_menu_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    draw_arrow_cursor(fb, x, y);
    use crate::gui::framebuffer::Pixel;
    let b = Pixel::rgb(0, 0, 0);
    let w = Pixel::rgb(255, 255, 255);
    let g = Pixel::rgb(100, 100, 100);
    // 9×10 menu box at offset (10, 12)
    let mx = x + 10;
    let my = y + 12;
    for dx in 0..=8 {
        fb.blend_pixel((mx + dx) as usize, my as usize, b);
        fb.blend_pixel((mx + dx) as usize, (my + 9) as usize, b);
    }
    for dy in 0..=9 {
        fb.blend_pixel(mx as usize, (my + dy) as usize, b);
        fb.blend_pixel((mx + 8) as usize, (my + dy) as usize, b);
    }
    for dy in 1..9 {
        for dx in 1..8 {
            fb.blend_pixel((mx + dx) as usize, (my + dy) as usize, w);
        }
    }
    for dx in 2..7 {
        fb.blend_pixel((mx + dx) as usize, (my + 2) as usize, g);
        fb.blend_pixel((mx + dx) as usize, (my + 4) as usize, g);
        fb.blend_pixel((mx + dx) as usize, (my + 6) as usize, g);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 3. Help — Arrow + question mark badge  (hotspot: top-left)
// ═══════════════════════════════════════════════════════════════════════
fn draw_help_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    draw_arrow_cursor(fb, x, y);
    use crate::gui::framebuffer::Pixel;
    let b = Pixel::rgb(0, 0, 0);
    let w = Pixel::rgb(255, 255, 255);
    let blue = Pixel::rgb(40, 100, 210);
    // Blue filled circle r=5 at offset (11,13)
    let qx = x + 11;
    let qy = y + 13;
    for dy in -5..=5i32 {
        for dx in -5..=5i32 {
            let d2 = dx * dx + dy * dy;
            if d2 <= 25 {
                fb.blend_pixel((qx + dx) as usize, (qy + dy) as usize, blue);
            }
        }
    }
    for dy in -5..=5i32 {
        for dx in -5..=5i32 {
            let d2 = dx * dx + dy * dy;
            if d2 > 20 && d2 <= 30 {
                fb.blend_pixel((qx + dx) as usize, (qy + dy) as usize, b);
            }
        }
    }
    // "?" glyph (5 px wide)
    fb.blend_pixel((qx - 1) as usize, (qy - 3) as usize, w);
    fb.blend_pixel(qx as usize, (qy - 4) as usize, w);
    fb.blend_pixel((qx + 1) as usize, (qy - 3) as usize, w);
    fb.blend_pixel((qx + 1) as usize, (qy - 2) as usize, w);
    fb.blend_pixel(qx as usize, (qy - 1) as usize, w);
    fb.blend_pixel(qx as usize, qy as usize, w);
    fb.blend_pixel(qx as usize, (qy + 2) as usize, w);
    fb.blend_pixel(qx as usize, (qy + 3) as usize, w);
}

// ═══════════════════════════════════════════════════════════════════════
// 4. PointingHand — Modern pointing-finger hand  (hotspot: finger tip)
//    Larger 17×22 bitmap — close to macOS / GTK pointer hand.
// ═══════════════════════════════════════════════════════════════════════
fn draw_hand_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    // 0=transparent 1=black 2=white 3=AA 4=light-gray 5=mid-gray
    #[rustfmt::skip]
    const D: [[u8; 17]; 22] = [
        [0,0,0,0,0,0,1,1,3,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,1,2,2,1,3,0,0,0,0,0,0,0],
        [0,0,0,0,0,1,2,2,1,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,1,2,2,1,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,1,2,2,1,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,1,2,2,1,1,1,0,1,1,3,0,0],
        [0,0,0,0,0,1,2,2,1,2,2,1,2,2,1,3,0],
        [0,0,0,0,0,1,2,2,1,2,2,1,2,2,1,0,0],
        [0,0,1,1,0,1,2,2,2,2,2,1,2,2,1,1,0],
        [0,1,2,2,1,1,2,2,2,2,2,2,2,2,1,2,1],
        [0,1,2,2,1,2,2,2,2,2,2,2,2,2,1,2,1],
        [3,1,2,2,2,2,2,2,2,2,2,2,2,2,2,2,1],
        [0,1,2,2,2,2,2,2,2,2,2,2,2,2,2,2,1],
        [0,0,1,2,2,2,2,2,2,2,2,2,2,2,2,2,1],
        [0,0,1,2,2,2,2,2,2,2,2,2,2,2,2,1,0],
        [0,0,0,1,2,2,2,2,2,2,2,2,2,2,2,1,0],
        [0,0,0,1,2,2,2,2,2,2,2,2,2,2,1,0,0],
        [0,0,0,0,1,2,2,2,2,2,2,2,2,2,1,0,0],
        [0,0,0,0,1,2,2,2,2,2,2,2,2,1,0,0,0],
        [0,0,0,0,0,1,2,2,2,2,2,2,2,1,0,0,0],
        [0,0,0,0,0,1,2,2,2,2,2,2,1,0,0,0,0],
        [0,0,0,0,0,0,1,1,1,1,1,1,3,0,0,0,0],
    ];
    // hotspot is the finger-tip: column 7, row 0 → draw at (x-7, y)
    draw_bitmap_cursor(fb, x - 7, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 5. Progress — Arrow + spinning circle  (hotspot: top-left)
// ═══════════════════════════════════════════════════════════════════════
fn draw_progress_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    draw_arrow_cursor(fb, x, y);
    use crate::gui::framebuffer::Pixel;
    let b = Pixel::rgb(0, 0, 0);
    let blue = Pixel::rgb(50, 130, 240);
    let light = Pixel::rgb(180, 210, 255);
    // Spinning disc r=4 at (12, 16)
    let cx = x + 12;
    let cy = y + 16;
    for dy in -4..=4i32 {
        for dx in -4..=4i32 {
            let d2 = dx * dx + dy * dy;
            if d2 <= 16 {
                fb.blend_pixel((cx + dx) as usize, (cy + dy) as usize, light);
            }
        }
    }
    // Quarter-arc in blue (upper-right)
    for dy in -4..=0i32 {
        for dx in 0..=4i32 {
            let d2 = dx * dx + dy * dy;
            if (8..=18).contains(&d2) {
                fb.blend_pixel((cx + dx) as usize, (cy + dy) as usize, blue);
            }
        }
    }
    for dy in -5..=5i32 {
        for dx in -5..=5i32 {
            let d2 = dx * dx + dy * dy;
            if d2 > 16 && d2 <= 28 {
                fb.blend_pixel((cx + dx) as usize, (cy + dy) as usize, b);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 6. Wait — Hourglass / busy  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_wait_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    // 0=trans 1=black 2=white 3=AA 4=light-gray 5=mid-gray
    #[rustfmt::skip]
    const D: [[u8; 13]; 17] = [
        [1,1,1,1,1,1,1,1,1,1,1,1,1],
        [0,1,2,2,2,2,2,2,2,2,2,1,0],
        [0,0,1,4,4,4,4,4,4,4,1,0,0],
        [0,0,0,1,4,4,4,4,4,1,0,0,0],
        [0,0,0,1,5,5,5,5,5,1,0,0,0],
        [0,0,0,0,1,5,5,5,1,0,0,0,0],
        [0,0,0,0,0,1,5,1,0,0,0,0,0],
        [0,0,0,0,0,0,1,0,0,0,0,0,0],
        [0,0,0,0,0,1,2,1,0,0,0,0,0],
        [0,0,0,0,1,2,2,2,1,0,0,0,0],
        [0,0,0,0,1,2,2,2,1,0,0,0,0],
        [0,0,0,1,2,2,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,2,2,1,0,0,0],
        [0,0,1,2,2,2,2,2,2,2,1,0,0],
        [0,0,1,2,2,2,2,2,2,2,1,0,0],
        [0,1,2,2,2,2,2,2,2,2,2,1,0],
        [1,1,1,1,1,1,1,1,1,1,1,1,1],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 7. Cell — Thick plus for table-cell selection  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_cell_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 15]; 15] = [
        [0,0,0,0,0,0,1,1,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,1,0,0,0,0,0,0],
        [1,1,1,1,1,1,1,2,1,1,1,1,1,1,1],
        [1,2,2,2,2,2,2,2,2,2,2,2,2,2,1],
        [1,1,1,1,1,1,1,2,1,1,1,1,1,1,1],
        [0,0,0,0,0,0,1,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,1,1,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 8. Crosshair — Thin precision cross with gap  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_crosshair_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 17]; 17] = [
        [0,0,0,0,0,0,0,1,1,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
        [1,1,1,1,1,1,0,0,1,0,0,1,1,1,1,1,1],
        [1,2,2,2,2,2,0,1,0,1,0,2,2,2,2,2,1],
        [1,1,1,1,1,1,0,0,1,0,0,1,1,1,1,1,1],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,1,1,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 9. Text — I-beam cursor  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_text_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 9]; 19] = [
        [0,1,1,3,0,3,1,1,0],
        [1,3,0,1,1,1,0,3,1],
        [0,0,0,0,1,0,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,1,2,1,0,0,0],
        [0,0,0,0,1,0,0,0,0],
        [1,3,0,1,1,1,0,3,1],
        [0,1,1,3,0,3,1,1,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 10. VerticalText — Horizontal I-beam  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_vertical_text_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 19]; 9] = [
        [0,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,0],
        [1,3,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,3,1],
        [1,0,0,1,1,1,1,1,1,1,1,1,1,1,1,1,0,0,1],
        [3,1,1,2,2,2,2,2,2,2,2,2,2,2,2,2,1,1,3],
        [0,1,0,1,1,1,1,1,1,1,1,1,1,1,1,1,0,1,0],
        [3,1,1,2,2,2,2,2,2,2,2,2,2,2,2,2,1,1,3],
        [1,0,0,1,1,1,1,1,1,1,1,1,1,1,1,1,0,0,1],
        [1,3,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,3,1],
        [0,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 11. Alias — Arrow + curved-arrow badge  (hotspot: top-left)
// ═══════════════════════════════════════════════════════════════════════
fn draw_alias_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    draw_arrow_cursor(fb, x, y);
    use crate::gui::framebuffer::Pixel;
    let b = Pixel::rgb(0, 0, 0);
    let w = Pixel::rgb(255, 255, 255);
    // Small shortcut arrow 8×8 at (9, 14)
    let ax = x + 9;
    let ay = y + 14;
    // Curved shaft
    for dx in 0..=4i32 {
        fb.blend_pixel((ax + dx) as usize, (ay - 3) as usize, b);
    }
    fb.blend_pixel((ax + 5) as usize, (ay - 2) as usize, b);
    fb.blend_pixel((ax + 5) as usize, (ay - 1) as usize, b);
    fb.blend_pixel((ax + 5) as usize, ay as usize, b);
    fb.blend_pixel((ax + 4) as usize, (ay + 1) as usize, b);
    for dx in 0..=3i32 {
        fb.blend_pixel((ax + dx) as usize, (ay + 2) as usize, b);
    }
    // Fill
    for dx in 1..=3i32 {
        fb.blend_pixel((ax + dx) as usize, (ay - 2) as usize, w);
    }
    fb.blend_pixel((ax + 4) as usize, (ay - 1) as usize, w);
    fb.blend_pixel((ax + 4) as usize, ay as usize, w);
    for dx in 1..=3i32 {
        fb.blend_pixel((ax + dx) as usize, (ay + 1) as usize, w);
    }
    // Arrowhead at bottom-left
    fb.blend_pixel((ax - 1) as usize, (ay + 1) as usize, b);
    fb.blend_pixel(ax as usize, (ay + 3) as usize, b);
    fb.blend_pixel((ax - 1) as usize, (ay + 3) as usize, b);
    fb.blend_pixel((ax - 2) as usize, (ay + 2) as usize, b);
}

// ═══════════════════════════════════════════════════════════════════════
// 12. Copy — Arrow + green "+" badge  (hotspot: top-left)
// ═══════════════════════════════════════════════════════════════════════
fn draw_copy_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    draw_arrow_cursor(fb, x, y);
    use crate::gui::framebuffer::Pixel;
    let b = Pixel::rgb(0, 0, 0);
    let w = Pixel::rgb(255, 255, 255);
    let g = Pixel::rgb(30, 180, 30);
    // Filled green circle r=5 at (12,16)
    let cx = x + 12;
    let cy = y + 16;
    for dy in -5..=5i32 {
        for dx in -5..=5i32 {
            if dx * dx + dy * dy <= 25 {
                fb.blend_pixel((cx + dx) as usize, (cy + dy) as usize, g);
            }
        }
    }
    for dy in -5..=5i32 {
        for dx in -5..=5i32 {
            let d2 = dx * dx + dy * dy;
            if d2 > 20 && d2 <= 30 {
                fb.blend_pixel((cx + dx) as usize, (cy + dy) as usize, b);
            }
        }
    }
    // "+" sign
    for d in -3..=3i32 {
        fb.blend_pixel((cx + d) as usize, cy as usize, w);
        fb.blend_pixel(cx as usize, (cy + d) as usize, w);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 13. Move — Four-directional arrow cross  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_move_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 17]; 17] = [
        [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,1,1,2,2,2,1,1,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0,0],
        [0,0,1,1,0,0,1,2,2,2,1,0,0,1,1,0,0],
        [0,1,2,2,1,1,1,2,2,2,1,1,1,2,2,1,0],
        [1,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,1],
        [0,1,2,2,1,1,1,2,2,2,1,1,1,2,2,1,0],
        [0,0,1,1,0,0,1,2,2,2,1,0,0,1,1,0,0],
        [0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,1,1,2,2,2,1,1,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 14. NoDrop — Arrow + red ⊘ badge  (hotspot: top-left)
// ═══════════════════════════════════════════════════════════════════════
fn draw_no_drop_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    draw_arrow_cursor(fb, x, y);
    use crate::gui::framebuffer::Pixel;
    let b = Pixel::rgb(0, 0, 0);
    let r = Pixel::rgb(210, 30, 30);
    let cx = x + 12;
    let cy = y + 16;
    // Red ring
    for dy in -5..=5i32 {
        for dx in -5..=5i32 {
            let d2 = dx * dx + dy * dy;
            if (13..=28).contains(&d2) {
                fb.blend_pixel((cx + dx) as usize, (cy + dy) as usize, r);
            }
        }
    }
    // Black outline
    for dy in -6..=6i32 {
        for dx in -6..=6i32 {
            let d2 = dx * dx + dy * dy;
            if (28..=40).contains(&d2) {
                fb.blend_pixel((cx + dx) as usize, (cy + dy) as usize, b);
            }
        }
    }
    // Diagonal slash
    for i in -4..=4i32 {
        fb.blend_pixel((cx + i) as usize, (cy - i) as usize, r);
        fb.blend_pixel((cx + i + 1) as usize, (cy - i) as usize, r);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 15. NotAllowed — Red circle ⊘  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_not_allowed_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 17]; 17] = [
        [0,0,0,0,0,3,1,1,1,1,1,3,0,0,0,0,0],
        [0,0,0,3,1,1,1,1,1,1,1,1,1,3,0,0,0],
        [0,0,3,1,1,2,2,2,2,2,1,1,1,3,0,0,0],
        [0,3,1,1,2,2,2,2,2,1,1,2,1,1,3,0,0],
        [0,1,1,2,2,2,2,2,1,1,2,2,2,1,0,0,0],
        [3,1,2,2,2,2,2,1,1,2,2,2,2,2,1,3,0],
        [1,1,2,2,2,2,1,1,2,2,2,2,2,2,1,1,0],
        [1,1,2,2,2,1,1,2,2,2,2,2,2,2,1,1,0],
        [1,1,2,2,1,1,2,2,2,2,2,1,1,2,1,1,0],
        [1,1,2,2,2,2,2,2,2,2,1,1,2,2,1,1,0],
        [1,1,2,2,2,2,2,2,2,1,1,2,2,2,1,1,0],
        [3,1,2,2,2,2,2,2,1,1,2,2,2,2,1,3,0],
        [0,1,1,2,2,2,2,1,1,2,2,2,2,1,1,0,0],
        [0,3,1,1,2,2,1,1,2,2,2,2,1,1,3,0,0],
        [0,0,3,1,1,1,1,2,2,2,2,1,1,3,0,0,0],
        [0,0,0,3,1,1,1,1,1,1,1,1,1,3,0,0,0],
        [0,0,0,0,0,3,1,1,1,1,1,3,0,0,0,0,0],
    ];
    // Override: 1=red-dark border, 2=pink fill — custom palette
    use crate::gui::framebuffer::Pixel;
    let cx = x;
    let cy = y;
    let ox = cx - 8;
    let oy = cy - 8;
    let red = Pixel::rgb(200, 30, 30);
    let pink = Pixel::rgb(240, 110, 110);
    let blk = Pixel::rgb(0, 0, 0);
    // shadow
    for (row, line) in D.iter().enumerate() {
        for (col, &p) in line.iter().enumerate() {
            if p == 1 || p == 2 {
                let px = ox + col as i32 + 1;
                let py = oy + row as i32 + 1;
                if px >= 0 && py >= 0 && px < fb.width as i32 && py < fb.height as i32 {
                    let ux = px as usize;
                    let uy = py as usize;
                    let off = uy * fb.pitch + ux * fb.bytes_per_pixel;
                    if off + 2 < fb.buffer.len() {
                        fb.buffer[off] = (fb.buffer[off] as u16 * 70 / 100) as u8;
                        fb.buffer[off + 1] = (fb.buffer[off + 1] as u16 * 70 / 100) as u8;
                        fb.buffer[off + 2] = (fb.buffer[off + 2] as u16 * 70 / 100) as u8;
                    }
                }
            }
        }
    }
    for (row, line) in D.iter().enumerate() {
        for (col, &p) in line.iter().enumerate() {
            if p == 0 {
                continue;
            }
            let px = ox + col as i32;
            let py = oy + row as i32;
            if px >= 0 && py >= 0 && px < fb.width as i32 && py < fb.height as i32 {
                let ux = px as usize;
                let uy = py as usize;
                match p {
                    1 => fb.set_pixel(ux, uy, red),
                    2 => fb.set_pixel(ux, uy, pink),
                    3 => fb.blend_pixel(ux, uy, Pixel::new(200, 30, 30, 128)),
                    _ => {}
                }
            }
        }
    }
    // Solid dark diagonal band
    for i in -5..=5i32 {
        for t in -1..=1i32 {
            let px = (cx + i) as usize;
            let py = (cy - i + t) as usize;
            if px < fb.width && py < fb.height {
                fb.set_pixel(px, py, red);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 16. Grab — Open hand  (hotspot: center-ish)
// ═══════════════════════════════════════════════════════════════════════
fn draw_grab_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 18]; 19] = [
        [0,0,0,0,1,1,0,0,1,1,0,0,1,1,0,0,0,0],
        [0,0,0,1,2,2,1,1,2,2,1,1,2,2,1,0,0,0],
        [0,0,0,1,2,2,1,2,2,2,1,2,2,2,1,3,0,0],
        [0,0,0,1,2,2,1,2,2,2,1,2,2,2,1,2,1,0],
        [0,0,0,0,1,2,2,2,2,2,2,2,2,1,2,2,1,0],
        [0,0,0,0,1,2,2,2,2,2,2,2,2,1,2,2,1,0],
        [1,1,3,0,0,1,2,2,2,2,2,2,2,2,2,2,1,0],
        [1,2,2,1,0,1,2,2,2,2,2,2,2,2,2,2,1,0],
        [1,2,2,1,0,1,2,2,2,2,2,2,2,2,2,2,1,0],
        [0,1,2,2,1,2,2,2,2,2,2,2,2,2,2,2,1,0],
        [0,0,1,2,2,2,2,2,2,2,2,2,2,2,2,1,0,0],
        [0,0,0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0],
        [0,0,0,0,1,2,2,2,2,2,2,2,2,2,1,0,0,0],
        [0,0,0,0,1,2,2,2,2,2,2,2,2,2,1,0,0,0],
        [0,0,0,0,0,1,2,2,2,2,2,2,2,1,0,0,0,0],
        [0,0,0,0,0,1,2,2,2,2,2,2,2,1,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,2,2,1,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,2,2,1,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,1,1,1,1,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 17. Grabbing — Closed fist  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_grabbing_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 17]; 16] = [
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
        [0,0,0,1,1,0,1,1,0,1,1,0,0,0,0,0,0],
        [0,0,1,2,2,1,2,2,1,2,2,1,1,0,0,0,0],
        [0,0,1,2,2,1,2,2,1,2,2,1,2,1,0,0,0],
        [0,0,1,2,2,2,2,2,2,2,2,1,2,2,1,0,0],
        [0,0,0,1,2,2,2,2,2,2,2,2,2,2,1,0,0],
        [0,0,0,0,1,2,2,2,2,2,2,2,2,2,1,0,0],
        [0,0,0,0,1,2,2,2,2,2,2,2,2,2,1,0,0],
        [0,0,0,0,0,1,2,2,2,2,2,2,2,1,0,0,0],
        [0,0,0,0,0,1,2,2,2,2,2,2,2,1,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,2,2,1,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,2,2,1,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,1,1,1,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 18. AllScroll — Four-way arrows with center dot  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_all_scroll_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 17]; 17] = [
        [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,1,1,2,2,2,1,1,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,1,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
        [0,0,1,1,0,0,0,0,0,0,0,0,0,1,1,0,0],
        [0,1,2,2,1,0,0,1,1,1,0,0,1,2,2,1,0],
        [1,2,2,2,1,0,0,1,2,1,0,0,1,2,2,2,1],
        [0,1,2,2,1,0,0,1,1,1,0,0,1,2,2,1,0],
        [0,0,1,1,0,0,0,0,0,0,0,0,0,1,1,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,1,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,1,1,2,2,2,1,1,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 19. ResizeHorizontal — Double horizontal arrow ↔  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_resize_h_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 19]; 11] = [
        [0,0,0,1,0,0,0,0,0,0,0,0,0,0,0,1,0,0,0],
        [0,0,1,2,1,0,0,0,0,0,0,0,0,0,1,2,1,0,0],
        [0,1,2,2,1,0,0,0,0,0,0,0,0,0,1,2,2,1,0],
        [1,2,2,2,1,1,1,1,1,1,1,1,1,1,1,2,2,2,1],
        [1,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,1],
        [1,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,1],
        [1,2,2,2,1,1,1,1,1,1,1,1,1,1,1,2,2,2,1],
        [0,1,2,2,1,0,0,0,0,0,0,0,0,0,1,2,2,1,0],
        [0,0,1,2,1,0,0,0,0,0,0,0,0,0,1,2,1,0,0],
        [0,0,0,1,0,0,0,0,0,0,0,0,0,0,0,1,0,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 20. ResizeNeSw — Diagonal ↗↙  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_resize_diag_trbl_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 15]; 15] = [
        [0,0,0,0,0,0,0,0,1,1,1,1,1,1,1],
        [0,0,0,0,0,0,0,0,1,2,2,2,2,2,1],
        [0,0,0,0,0,0,0,0,1,2,2,2,2,1,0],
        [0,0,0,0,0,0,0,0,1,2,2,2,1,0,0],
        [0,0,0,0,0,0,0,1,1,2,2,1,0,0,0],
        [0,0,0,0,0,0,1,2,1,1,1,0,0,0,0],
        [0,0,0,0,0,1,2,2,1,0,0,0,0,0,0],
        [0,0,0,0,1,2,2,1,0,0,0,0,0,0,0],
        [0,0,0,0,1,2,1,0,0,0,0,0,0,0,0],
        [0,0,0,1,1,1,0,0,0,0,0,0,0,0,0],
        [0,0,0,1,2,2,1,1,0,0,0,0,0,0,0],
        [0,0,1,2,2,2,1,0,0,0,0,0,0,0,0],
        [0,1,2,2,2,2,1,0,0,0,0,0,0,0,0],
        [1,2,2,2,2,2,1,0,0,0,0,0,0,0,0],
        [1,1,1,1,1,1,1,0,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 21. ResizeNwSe — Diagonal ↘↖  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_resize_diag_tlbr_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 15]; 15] = [
        [1,1,1,1,1,1,1,0,0,0,0,0,0,0,0],
        [1,2,2,2,2,2,1,0,0,0,0,0,0,0,0],
        [0,1,2,2,2,2,1,0,0,0,0,0,0,0,0],
        [0,0,1,2,2,2,1,0,0,0,0,0,0,0,0],
        [0,0,0,1,2,2,1,1,0,0,0,0,0,0,0],
        [0,0,0,0,1,1,1,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,1,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,2,2,1,0,0,0,0],
        [0,0,0,0,0,0,0,0,1,2,1,0,0,0,0],
        [0,0,0,0,0,0,0,0,1,1,1,0,0,0,0],
        [0,0,0,0,0,0,0,1,1,2,2,1,0,0,0],
        [0,0,0,0,0,0,0,0,1,2,2,2,1,0,0],
        [0,0,0,0,0,0,0,0,1,2,2,2,2,1,0],
        [0,0,0,0,0,0,0,0,1,2,2,2,2,2,1],
        [0,0,0,0,0,0,0,0,1,1,1,1,1,1,1],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 22. ResizeVertical — Double vertical arrow ↕  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_resize_v_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 11]; 19] = [
        [0,0,0,0,0,1,0,0,0,0,0],
        [0,0,0,0,1,2,1,0,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,1,2,2,2,2,2,1,0,0],
        [0,1,1,1,2,2,2,1,1,1,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,1,1,1,2,2,2,1,1,1,0],
        [0,0,1,2,2,2,2,2,1,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,0,1,2,1,0,0,0,0],
        [0,0,0,0,0,1,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 23. ResizeEast — Right arrow →  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_resize_east_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 13]; 11] = [
        [0,0,0,0,0,0,0,0,0,1,0,0,0],
        [0,0,0,0,0,0,0,0,0,1,1,0,0],
        [1,1,0,0,0,0,0,0,0,1,2,1,0],
        [1,2,1,1,1,1,1,1,1,1,2,2,1],
        [1,2,2,2,2,2,2,2,2,2,2,2,1],
        [1,2,2,2,2,2,2,2,2,2,2,2,1],
        [1,2,1,1,1,1,1,1,1,1,2,2,1],
        [1,1,0,0,0,0,0,0,0,1,2,1,0],
        [0,0,0,0,0,0,0,0,0,1,1,0,0],
        [0,0,0,0,0,0,0,0,0,1,0,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 25. ResizeSouth — Down arrow ↓  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_resize_south_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 11]; 13] = [
        [0,0,1,1,1,1,1,1,1,0,0],
        [0,0,1,2,2,2,2,2,1,0,0],
        [0,0,1,2,2,2,2,2,1,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,1,1,1,2,2,2,1,1,1,0],
        [0,0,1,2,2,2,2,2,1,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,0,1,2,1,0,0,0,0],
        [0,0,0,0,0,1,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 27. ResizeWest — Left arrow ←  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_resize_west_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 13]; 11] = [
        [0,0,0,1,0,0,0,0,0,0,0,0,0],
        [0,0,1,1,0,0,0,0,0,0,0,0,0],
        [0,1,2,1,0,0,0,0,0,0,0,1,1],
        [1,2,2,1,1,1,1,1,1,1,1,2,1],
        [1,2,2,2,2,2,2,2,2,2,2,2,1],
        [1,2,2,2,2,2,2,2,2,2,2,2,1],
        [1,2,2,1,1,1,1,1,1,1,1,2,1],
        [0,1,2,1,0,0,0,0,0,0,0,1,1],
        [0,0,1,1,0,0,0,0,0,0,0,0,0],
        [0,0,0,1,0,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 29. ResizeNorth — Up arrow ↑  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_resize_north_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 11]; 13] = [
        [0,0,0,0,0,1,0,0,0,0,0],
        [0,0,0,0,1,2,1,0,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,1,2,2,2,2,2,1,0,0],
        [0,1,1,1,2,2,2,1,1,1,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,0,1,2,2,2,1,0,0,0],
        [0,0,1,2,2,2,2,2,1,0,0],
        [0,0,1,1,1,1,1,1,1,0,0],
        [0,0,0,0,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 31. ResizeColumn — col-resize: ↔ with vertical bars  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_resize_column_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 17]; 17] = [
        [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
        [0,0,0,1,0,0,0,0,1,0,0,0,0,1,0,0,0],
        [0,0,1,2,1,0,0,0,1,0,0,0,1,2,1,0,0],
        [0,1,2,2,1,0,0,0,1,0,0,0,1,2,2,1,0],
        [1,2,2,2,1,1,1,0,1,0,1,1,1,2,2,2,1],
        [1,2,2,2,2,2,2,0,1,0,2,2,2,2,2,2,1],
        [1,2,2,2,2,2,2,0,1,0,2,2,2,2,2,2,1],
        [1,2,2,2,2,2,2,0,1,0,2,2,2,2,2,2,1],
        [1,2,2,2,1,1,1,0,1,0,1,1,1,2,2,2,1],
        [0,1,2,2,1,0,0,0,1,0,0,0,1,2,2,1,0],
        [0,0,1,2,1,0,0,0,1,0,0,0,1,2,1,0,0],
        [0,0,0,1,0,0,0,0,1,0,0,0,0,1,0,0,0],
        [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 32. ResizeRow — row-resize: ↕ with horizontal bars  (hotspot: center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_resize_row_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 17]; 17] = [
        [0,0,0,0,0,0,1,1,1,1,1,0,0,0,0,0,0],
        [0,0,0,0,0,1,2,2,2,2,2,1,0,0,0,0,0],
        [0,0,0,0,1,2,2,2,2,2,2,2,1,0,0,0,0],
        [0,0,0,1,1,1,1,2,2,2,1,1,1,1,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,1,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
        [1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],
        [1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,1,1,1,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,2,1,0,0,0,0,0,0],
        [0,0,0,1,1,1,1,2,2,2,1,1,1,1,0,0,0],
        [0,0,0,0,1,2,2,2,2,2,2,2,1,0,0,0,0],
        [0,0,0,0,0,1,2,2,2,2,2,1,0,0,0,0,0],
        [0,0,0,0,0,0,1,1,1,1,1,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor_centered(fb, x, y, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 33. ZoomIn — Magnifying glass with "+"  (hotspot: lens center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_zoom_in_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    // 0=trans 1=black 2=white 3=AA 4=light-gray(glass-fill) 5=mid-gray
    #[rustfmt::skip]
    const D: [[u8; 17]; 20] = [
        [0,0,0,0,3,1,1,1,1,1,3,0,0,0,0,0,0],
        [0,0,3,1,1,4,4,4,4,4,1,1,3,0,0,0,0],
        [0,3,1,4,4,4,4,4,4,4,4,4,1,3,0,0,0],
        [0,1,4,4,4,4,1,1,1,4,4,4,4,1,0,0,0],
        [3,1,4,4,4,4,1,2,1,4,4,4,4,1,3,0,0],
        [1,4,4,4,4,4,1,2,1,4,4,4,4,4,1,0,0],
        [1,4,4,1,1,1,1,2,1,1,1,1,4,4,1,0,0],
        [1,4,4,1,2,2,2,2,2,2,2,1,4,4,1,0,0],
        [1,4,4,1,1,1,1,2,1,1,1,1,4,4,1,0,0],
        [1,4,4,4,4,4,1,2,1,4,4,4,4,4,1,0,0],
        [3,1,4,4,4,4,1,1,1,4,4,4,4,1,3,0,0],
        [0,1,4,4,4,4,4,4,4,4,4,4,4,1,0,0,0],
        [0,3,1,4,4,4,4,4,4,4,4,4,1,1,0,0,0],
        [0,0,3,1,1,4,4,4,4,4,1,1,3,1,1,0,0],
        [0,0,0,0,3,1,1,1,1,1,3,0,1,2,1,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,1,2,1,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,2,1],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,1],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor(fb, x - 7, y - 7, &D);
}

// ═══════════════════════════════════════════════════════════════════════
// 34. ZoomOut — Magnifying glass with "−"  (hotspot: lens center)
// ═══════════════════════════════════════════════════════════════════════
fn draw_zoom_out_cursor(fb: &mut FrameBuffer, x: i32, y: i32) {
    #[rustfmt::skip]
    const D: [[u8; 17]; 20] = [
        [0,0,0,0,3,1,1,1,1,1,3,0,0,0,0,0,0],
        [0,0,3,1,1,4,4,4,4,4,1,1,3,0,0,0,0],
        [0,3,1,4,4,4,4,4,4,4,4,4,1,3,0,0,0],
        [0,1,4,4,4,4,4,4,4,4,4,4,4,1,0,0,0],
        [3,1,4,4,4,4,4,4,4,4,4,4,4,1,3,0,0],
        [1,4,4,4,4,4,4,4,4,4,4,4,4,4,1,0,0],
        [1,4,4,4,4,4,4,4,4,4,4,4,4,4,1,0,0],
        [1,4,4,1,2,2,2,2,2,2,2,1,4,4,1,0,0],
        [1,4,4,4,4,4,4,4,4,4,4,4,4,4,1,0,0],
        [1,4,4,4,4,4,4,4,4,4,4,4,4,4,1,0,0],
        [3,1,4,4,4,4,4,4,4,4,4,4,4,1,3,0,0],
        [0,1,4,4,4,4,4,4,4,4,4,4,4,1,0,0,0],
        [0,3,1,4,4,4,4,4,4,4,4,4,1,1,0,0,0],
        [0,0,3,1,1,4,4,4,4,4,1,1,3,1,1,0,0],
        [0,0,0,0,3,1,1,1,1,1,3,0,1,2,1,0,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,1,2,1,0],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,2,1],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,1],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
    ];
    draw_bitmap_cursor(fb, x - 7, y - 7, &D);
}
