//! Boot Splash Screen — Early graphical feedback during OS initialization
//!
//! Aurora background + Knox UI Splash card (Progress / Tag).
//! Fonts and the GPUI platform are brought up in `gui::init_framebuffer`
//! before the first `draw_splash`.

use crate::gui::font_engine::{self, FontId};
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::gui::gpui_platform;
use crate::serial_println;

fn draw_centered(
    fb: &mut FrameBuffer,
    y: i32,
    text: &str,
    font: FontId,
    size: u16,
    tracking: i32,
    color: Pixel,
) {
    font_engine::draw_text_centered(
        fb,
        fb.width as i32 / 2,
        y,
        text,
        font,
        size,
        color,
        tracking,
    );
}

fn fill_aurora(fb: &mut FrameBuffer) {
    fb.fill_rect(
        Rect {
            x: 0,
            y: 0,
            width: fb.width as u32,
            height: fb.height as u32,
        },
        Pixel::rgb(22, 20, 26),
    );
}

fn paint_card(fb: &mut FrameBuffer, percent: u32) {
    let percent = percent.min(100);
    if gpui_platform::is_ready() && font_engine::is_ready() {
        gpui_platform::blit_splash_card(fb, percent);
        return;
    }
    let dest = gpui_platform::splash_card_rect(fb.width as i32, fb.height as i32);
    fb.fill_rect(dest, Pixel::rgb(44, 40, 50));
    let cx = dest.x + dest.width as i32 / 2;
    let mut y = dest.y + 48;
    font_engine::draw_text_centered(
        fb,
        cx,
        y,
        "KnoxOS",
        FontId::UiBold,
        44,
        Pixel::rgb(240, 232, 224),
        0,
    );
    y += 56;
    draw_centered(
        fb,
        y,
        "SI-native Operating System",
        FontId::Ui,
        13,
        0,
        Pixel::rgb(140, 130, 120),
    );
    y += 28;
    draw_centered(
        fb,
        y,
        "v0.2.5",
        FontId::Ui,
        font_engine::CAPTION_SIZE,
        0,
        Pixel::rgb(140, 130, 120),
    );
    y += 36;
    let bar_w = 300i32;
    let bar_x = cx - bar_w / 2;
    fb.fill_rect(
        Rect {
            x: bar_x,
            y,
            width: bar_w as u32,
            height: 8,
        },
        Pixel::rgb(55, 50, 62),
    );
    let fill_w = (bar_w as u32 * percent / 100).max(if percent > 0 { 8 } else { 0 });
    if fill_w > 0 {
        fb.fill_rect(
            Rect {
                x: bar_x,
                y,
                width: fill_w,
                height: 8,
            },
            Pixel::rgb(232, 121, 100),
        );
    }
    y += 24;
    draw_centered(
        fb,
        y,
        knox_ui::splash_status(percent),
        FontId::Ui,
        13,
        0,
        Pixel::rgb(140, 130, 120),
    );
}

/// Draw the full boot splash (background + Knox UI card).
pub fn draw_splash(fb: &mut FrameBuffer) {
    fill_aurora(fb);
    paint_card(fb, 0);
    fb.present();
    serial_println!("[KnoxOS] Boot splash displayed");
}

/// Update the splash progress (0..100) by re-rasterizing the card.
pub fn update_progress(fb: &mut FrameBuffer, percent: u32) {
    paint_card(fb, percent);
    let dest = gpui_platform::splash_card_rect(fb.width as i32, fb.height as i32);
    fb.present_rect(dest.x, dest.y, dest.width, dest.height);
}

/// Clear the splash screen (draw black) before transitioning to desktop
pub fn clear_splash(fb: &mut FrameBuffer) {
    fb.fill_rect(
        Rect {
            x: 0,
            y: 0,
            width: fb.width as u32,
            height: fb.height as u32,
        },
        Pixel::rgb(0, 0, 0),
    );
    fb.present();
    serial_println!("[KnoxOS] Boot splash cleared");
}

// ═══════════════════════════════════════════════════════════════════════
// BOOT MENU WITH RECOVERY OPTIONS
// ═══════════════════════════════════════════════════════════════════════

/// Boot menu entry
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootMenuEntry {
    /// Normal boot (default)
    NormalBoot,
    /// Boot into recovery shell
    RecoveryShell,
    /// Boot with safe graphics (VGA text mode)
    SafeGraphics,
    /// Boot with verbose logging
    VerboseBoot,
    /// Memory test
    MemoryTest,
    /// Boot from previous kernel
    PreviousKernel,
    /// UEFI firmware settings
    FirmwareSettings,
}

impl BootMenuEntry {
    pub fn label(&self) -> &'static str {
        match self {
            Self::NormalBoot => "KnoxOS v0.2.5",
            Self::RecoveryShell => "KnoxOS Recovery Shell",
            Self::SafeGraphics => "KnoxOS (Safe Graphics)",
            Self::VerboseBoot => "KnoxOS (Verbose)",
            Self::MemoryTest => "Memory Test",
            Self::PreviousKernel => "KnoxOS (Previous Kernel)",
            Self::FirmwareSettings => "UEFI Firmware Settings",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::NormalBoot => "Boot normally with all drivers",
            Self::RecoveryShell => "Drop to root shell for system repair",
            Self::SafeGraphics => "Boot with basic VGA, no GPU acceleration",
            Self::VerboseBoot => "Boot with full kernel debug logging",
            Self::MemoryTest => "Run hardware memory diagnostics",
            Self::PreviousKernel => "Boot the previous working kernel image",
            Self::FirmwareSettings => "Reboot into UEFI/BIOS configuration",
        }
    }
}

const BOOT_MENU_ENTRIES: &[BootMenuEntry] = &[
    BootMenuEntry::NormalBoot,
    BootMenuEntry::RecoveryShell,
    BootMenuEntry::SafeGraphics,
    BootMenuEntry::VerboseBoot,
    BootMenuEntry::MemoryTest,
    BootMenuEntry::PreviousKernel,
    BootMenuEntry::FirmwareSettings,
];

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

static BOOT_MENU_SELECTION: AtomicU8 = AtomicU8::new(0);
static BOOT_MENU_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Draw the boot menu (shown when user presses a key during splash)
pub fn draw_boot_menu(fb: &mut FrameBuffer, selected: usize) {
    let w = fb.width as i32;
    let h = fb.height as i32;

    // Dark background
    fb.fill_rect(
        Rect {
            x: 0,
            y: 0,
            width: w as u32,
            height: h as u32,
        },
        Pixel::rgb(12, 10, 28),
    );

    // Title
    let title_y = 56i32;
    draw_centered(
        fb,
        title_y,
        "KnoxOS Boot Menu",
        FontId::UiBold,
        font_engine::TITLE_SIZE,
        0,
        Pixel::rgb(240, 232, 224),
    );

    // Separator line
    let sep_y = title_y + font_engine::TITLE_SIZE as i32 + 20;
    fb.fill_rect(
        Rect {
            x: w / 4,
            y: sep_y,
            width: w as u32 / 2,
            height: 1,
        },
        Pixel::new(80, 80, 120, 180),
    );

    // Menu entries
    let entry_height = 40i32;
    let start_y = sep_y + 24;
    let entry_x = w / 4;

    for (i, entry) in BOOT_MENU_ENTRIES.iter().enumerate() {
        let y = start_y + i as i32 * entry_height;
        let is_selected = i == selected;

        if is_selected {
            // Highlight background
            fb.fill_rect(
                Rect {
                    x: entry_x - 8,
                    y: y - 4,
                    width: w as u32 / 2 + 16,
                    height: entry_height as u32,
                },
                Pixel::new(40, 60, 100, 200),
            );
        }

        let label_color = if is_selected {
            Pixel::new(100, 200, 255, 255)
        } else {
            Pixel::new(160, 170, 190, 255)
        };

        let indicator = if is_selected { "> " } else { "  " };
        let label = entry.label();
        let combined = alloc::format!("{}{}", indicator, label);
        if font_engine::is_ready() {
            font_engine::draw_text(
                fb,
                entry_x,
                y + 6,
                &combined,
                FontId::Ui,
                font_engine::BODY_SIZE,
                label_color,
            );
        } else {
            fonts::draw_string(fb, entry_x, y + 4, &combined, label_color, 2);
        }

        // Description text for selected item
        if is_selected {
            let desc_y = start_y + BOOT_MENU_ENTRIES.len() as i32 * entry_height + 24;
            if font_engine::is_ready() {
                font_engine::draw_text(
                    fb,
                    entry_x,
                    desc_y,
                    entry.description(),
                    FontId::Ui,
                    font_engine::CAPTION_SIZE,
                    Pixel::new(120, 130, 160, 200),
                );
            } else {
                fonts::draw_string(
                    fb,
                    entry_x,
                    desc_y,
                    entry.description(),
                    Pixel::new(120, 130, 160, 200),
                    1,
                );
            }
        }
    }

    // Footer help
    let help = "Use Arrow Keys to select, Enter to boot, Esc for normal boot";
    if font_engine::is_ready() {
        draw_centered(
            fb,
            h - 44,
            help,
            FontId::Ui,
            font_engine::CAPTION_SIZE,
            0,
            Pixel::new(96, 108, 140, 200),
        );
    } else {
        let help_w = help.len() as i32 * fonts::FONT_WIDTH as i32;
        fonts::draw_string(
            fb,
            (w - help_w) / 2,
            h - 40,
            help,
            Pixel::new(80, 90, 120, 180),
            1,
        );
    }

    fb.present();
}

/// Handle boot menu keyboard input. Returns selected entry when Enter pressed.
pub fn boot_menu_key(scancode: u8) -> Option<BootMenuEntry> {
    let current = BOOT_MENU_SELECTION.load(Ordering::Relaxed) as usize;
    let count = BOOT_MENU_ENTRIES.len();

    match scancode {
        0x48 => {
            // Up arrow
            let new = if current == 0 { count - 1 } else { current - 1 };
            BOOT_MENU_SELECTION.store(new as u8, Ordering::Relaxed);
            None
        }
        0x50 => {
            // Down arrow
            let new = if current + 1 >= count { 0 } else { current + 1 };
            BOOT_MENU_SELECTION.store(new as u8, Ordering::Relaxed);
            None
        }
        0x1C => {
            // Enter
            Some(BOOT_MENU_ENTRIES[current])
        }
        0x01 => {
            // Escape — normal boot
            Some(BootMenuEntry::NormalBoot)
        }
        _ => None,
    }
}

/// Check if boot menu should be shown (key pressed during splash)
pub fn is_boot_menu_active() -> bool {
    BOOT_MENU_ACTIVE.load(Ordering::Relaxed)
}

/// Activate the boot menu
pub fn activate_boot_menu() {
    BOOT_MENU_ACTIVE.store(true, Ordering::Relaxed);
    serial_println!("[KnoxOS] Boot menu activated");
}

/// Get the currently selected menu index
pub fn current_selection() -> usize {
    BOOT_MENU_SELECTION.load(Ordering::Relaxed) as usize
}
