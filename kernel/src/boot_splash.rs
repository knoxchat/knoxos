//! Boot Splash Screen — Early graphical feedback during OS initialization
//!
//! Displays a centered KnoxOS logo and progress bar on the framebuffer while
//! kernel subsystems are being initialized. Called from `kernel_main()` right
//! after framebuffer init, before any other subsystem needs the display.

use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::serial_println;
use alloc::format;

/// Draw the full boot splash (background + logo + tagline + progress bar)
pub fn draw_splash(fb: &mut FrameBuffer) {
    let w = fb.width as i32;
    let h = fb.height as i32;

    // ── Deep space gradient background ──────────────────────────────
    for y in 0..h {
        let t = y as f32 / h as f32;
        let r = (8.0 + t * 20.0) as u8;
        let g = (6.0 + t * 12.0) as u8;
        let b = (24.0 + t * 40.0) as u8;
        fb.fill_rect(
            Rect {
                x: 0,
                y,
                width: w as u32,
                height: 1,
            },
            Pixel::rgb(r, g, b),
        );
    }

    // ── Scatter some dim "star" dots ────────────────────────────────
    let star_seeds: [(i32, i32, u8); 30] = [
        (120, 80, 70),
        (340, 150, 50),
        (580, 60, 80),
        (800, 200, 40),
        (1000, 90, 65),
        (1200, 170, 55),
        (1400, 50, 75),
        (1600, 130, 45),
        (1780, 80, 60),
        (200, 400, 50),
        (500, 350, 70),
        (900, 450, 40),
        (1100, 380, 55),
        (1350, 500, 65),
        (1550, 420, 50),
        (1700, 550, 45),
        (100, 600, 60),
        (350, 700, 40),
        (600, 650, 55),
        (850, 750, 70),
        (1050, 680, 45),
        (1250, 800, 50),
        (1450, 720, 65),
        (1650, 850, 55),
        (250, 900, 40),
        (550, 950, 60),
        (750, 880, 50),
        (950, 1000, 45),
        (1150, 960, 55),
        (1500, 1020, 60),
    ];
    for &(sx, sy, brightness) in &star_seeds {
        if sx < w && sy < h {
            fb.set_pixel(
                sx as usize,
                sy as usize,
                Pixel::new(
                    brightness,
                    brightness,
                    (brightness as u16 + 40).min(255) as u8,
                    brightness,
                ),
            );
        }
    }

    // ── KnoxOS logo text (large, centered) ──────────────────────────
    let logo = "KnoxOS";
    let logo_scale = 4u32;
    let logo_w = logo.len() as i32 * fonts::FONT_WIDTH as i32 * logo_scale as i32;
    let logo_x = (w - logo_w) / 2;
    let logo_y = h / 2 - 80;

    // Shadow (offset scales with text size for proper depth)
    let shadow_off = (logo_scale as i32).max(2);
    fonts::draw_string_bold(
        fb,
        logo_x + shadow_off,
        logo_y + shadow_off,
        logo,
        Pixel::new(0, 0, 0, 140),
        logo_scale,
    );
    // Main text (bright white-blue)
    fonts::draw_string_bold(
        fb,
        logo_x,
        logo_y,
        logo,
        Pixel::new(200, 220, 255, 255),
        logo_scale,
    );

    // ── Tagline ─────────────────────────────────────────────────────
    let tagline = "AI-native Operating System";
    let tl_scale = 2u32;
    let tl_w = tagline.len() as i32 * fonts::FONT_WIDTH as i32 * tl_scale as i32;
    let tl_x = (w - tl_w) / 2;
    let tl_y = logo_y + (fonts::FONT_HEIGHT as i32 * logo_scale as i32) + 16;
    fonts::draw_string(
        fb,
        tl_x,
        tl_y,
        tagline,
        Pixel::new(140, 160, 200, 200),
        tl_scale,
    );

    // ── Version string ──────────────────────────────────────────────
    let version = "v0.2.1";
    let v_w = version.len() as i32 * fonts::FONT_WIDTH as i32;
    let v_x = (w - v_w) / 2;
    let v_y = tl_y + (fonts::FONT_HEIGHT as i32 * tl_scale as i32) + 8;
    fonts::draw_string(fb, v_x, v_y, version, Pixel::new(100, 120, 160, 160), 1);

    // ── Progress bar track (empty) ──────────────────────────────────
    let bar_w = 320u32;
    let bar_h = 6u32;
    let bar_x = (w as u32 - bar_w) / 2;
    let bar_y = (v_y + fonts::FONT_HEIGHT as i32 + 16) as u32;

    // Track background
    fb.fill_rect(
        Rect {
            x: bar_x as i32,
            y: bar_y as i32,
            width: bar_w,
            height: bar_h,
        },
        Pixel::new(40, 40, 60, 120),
    );

    // ── "Loading..." text ───────────────────────────────────────────
    let load_text = "Initializing...";
    let lt_w = load_text.len() as i32 * fonts::FONT_WIDTH as i32;
    let lt_x = (w - lt_w) / 2;
    let lt_y = bar_y as i32 + bar_h as i32 + 12;
    fonts::draw_string(fb, lt_x, lt_y, load_text, Pixel::new(120, 140, 180, 180), 1);

    // Present to hardware framebuffer
    fb.present();

    serial_println!("[KnoxOS] Boot splash displayed");
}

/// Update the progress bar on the splash screen (0..100)
pub fn update_progress(fb: &mut FrameBuffer, percent: u32) {
    let w = fb.width as i32;
    let h = fb.height as i32;

    // Must match the bar position from draw_splash()
    let logo_scale = 4u32;
    let tl_scale = 2u32;
    let logo_y = h / 2 - 80;
    let tl_y = logo_y + (fonts::FONT_HEIGHT as i32 * logo_scale as i32) + 16;
    let v_y = tl_y + (fonts::FONT_HEIGHT as i32 * tl_scale as i32) + 8;

    let bar_w = 320u32;
    let bar_h = 6u32;
    let bar_x = (w as u32 - bar_w) / 2;
    let bar_y = (v_y + fonts::FONT_HEIGHT as i32 + 16) as u32;

    let fill_w = (bar_w * percent.min(100)) / 100;
    if fill_w > 0 {
        // Gradient fill: blue → cyan
        for dx in 0..fill_w {
            let t = dx as f32 / bar_w as f32;
            let r = (60.0 + t * 40.0) as u8;
            let g = (130.0 + t * 80.0) as u8;
            let b = (220.0 + t * 35.0) as u8;
            for dy in 0..bar_h {
                fb.set_pixel(
                    (bar_x + dx) as usize,
                    (bar_y + dy) as usize,
                    Pixel::rgb(r, g, b),
                );
            }
        }
        // Only present the progress bar region for speed
        fb.present_rect(bar_x as i32, bar_y as i32, bar_w, bar_h);
    }
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
            Self::NormalBoot => "KnoxOS v0.2.1",
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
    let title = "KnoxOS Boot Menu";
    let title_scale = 3u32;
    let title_w = title.len() as i32 * fonts::FONT_WIDTH as i32 * title_scale as i32;
    let title_x = (w - title_w) / 2;
    fonts::draw_string(
        fb,
        title_x,
        60,
        title,
        Pixel::new(200, 220, 255, 255),
        title_scale,
    );

    // Separator line
    let sep_y = 60 + fonts::FONT_HEIGHT as i32 * title_scale as i32 + 16;
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
        fonts::draw_string(fb, entry_x, y + 4, &combined, label_color, 2);

        // Description text for selected item
        if is_selected {
            let desc_y = start_y + BOOT_MENU_ENTRIES.len() as i32 * entry_height + 24;
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

    // Footer help
    let help = "Use Arrow Keys to select, Enter to boot, Esc for normal boot";
    let help_w = help.len() as i32 * fonts::FONT_WIDTH as i32;
    let help_x = (w - help_w) / 2;
    fonts::draw_string(fb, help_x, h - 40, help, Pixel::new(80, 90, 120, 180), 1);

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
