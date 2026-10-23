use alloc::vec::Vec;
use core::sync::atomic::Ordering;

use crate::gui::framebuffer::{FrameBuffer, Pixel};

use super::draw::{ACCENT_CYAN, TEXT_DIM, TEXT_WHITE, draw_text, format_u64};
use super::state::{INSTALL_COMPLETE, INSTALL_PROGRESS, INSTALLER_STATE};
use super::types::{BootloaderType, FsType, PartitionScheme, TIMEZONES};

// ─── Per-Step Renderers ──────────────────────────────────────────────

pub(super) fn render_welcome(fb: &mut FrameBuffer, x: usize, y: usize) {
    draw_text(fb, x, y, "Welcome to KnoxOS", TEXT_WHITE);
    draw_text(
        fb,
        x,
        y + 30,
        "This wizard will guide you through",
        TEXT_DIM,
    );
    draw_text(
        fb,
        x,
        y + 50,
        "installing KnoxOS on your computer.",
        TEXT_DIM,
    );

    draw_text(fb, x, y + 90, "Features:", ACCENT_CYAN);
    draw_text(
        fb,
        x + 16,
        y + 114,
        "- Modern desktop with glassmorphism UI",
        TEXT_DIM,
    );
    draw_text(
        fb,
        x + 16,
        y + 134,
        "- Linux binary compatibility",
        TEXT_DIM,
    );
    draw_text(
        fb,
        x + 16,
        y + 154,
        "- CFS fair scheduler & preemptive multitasking",
        TEXT_DIM,
    );
    draw_text(
        fb,
        x + 16,
        y + 174,
        "- Full networking stack with TLS/SSL",
        TEXT_DIM,
    );
    draw_text(
        fb,
        x + 16,
        y + 194,
        "- ALSA-compatible audio subsystem",
        TEXT_DIM,
    );

    draw_text(fb, x, y + 240, "Select your language:", TEXT_WHITE);

    let languages = [
        "English (US)",
        "English (UK)",
        "Deutsch",
        "Francais",
        "Espanol",
        "Portugues",
    ];
    let st = INSTALLER_STATE.lock();
    for (i, &lang) in languages.iter().enumerate() {
        let ly = y + 268 + i * 24;
        let col = if i == st.language_idx {
            ACCENT_CYAN
        } else {
            TEXT_DIM
        };
        let marker = if i == st.language_idx { "> " } else { "  " };
        draw_text(fb, x + 8, ly, marker, col);
        draw_text(fb, x + 28, ly, lang, col);
    }
}

pub(super) fn render_disk_setup(fb: &mut FrameBuffer, x: usize, y: usize) {
    draw_text(fb, x, y, "Disk Setup", TEXT_WHITE);
    draw_text(
        fb,
        x,
        y + 28,
        "Select the target disk for installation:",
        TEXT_DIM,
    );

    let st = INSTALLER_STATE.lock();
    for i in 0..st.disk_count {
        let dy = y + 60 + i * 48;
        let selected = i == st.selected_disk;
        let bg = if selected {
            Pixel::new(30, 60, 80, 200)
        } else {
            Pixel::new(28, 28, 44, 180)
        };
        for yy in dy..dy + 40 {
            for xx in x..x + 380 {
                fb.set_pixel(xx, yy, bg);
            }
        }
        if selected {
            for yy in dy..dy + 40 {
                fb.set_pixel(x, yy, ACCENT_CYAN);
                fb.set_pixel(x + 1, yy, ACCENT_CYAN);
            }
        }
        let name_str =
            core::str::from_utf8(&st.disks[i].name[..st.disks[i].name_len]).unwrap_or("???");
        let model_str =
            core::str::from_utf8(&st.disks[i].model[..st.disks[i].model_len]).unwrap_or("");
        draw_text(fb, x + 8, dy + 4, name_str, TEXT_WHITE);
        draw_text(fb, x + 8, dy + 22, model_str, TEXT_DIM);

        let size_gb = st.disks[i].size_mb / 1024;
        if size_gb > 0 {
            let mut buf = [0u8; 16];
            let s = format_u64(size_gb, &mut buf);
            draw_text(fb, x + 280, dy + 4, s, TEXT_DIM);
            draw_text(fb, x + 320, dy + 4, " GB", TEXT_DIM);
        }
    }

    let scheme_y = y + 60 + st.disk_count * 48 + 20;
    drop(st);
    draw_text(fb, x, scheme_y, "Partitioning:", TEXT_WHITE);

    let st2 = INSTALLER_STATE.lock();
    let schemes = [
        (PartitionScheme::EraseAll, "Erase disk (EFI+swap+root)"),
        (PartitionScheme::Manual, "Manual partitioning (advanced)"),
    ];
    for (i, (scheme, label)) in schemes.iter().enumerate() {
        let sy = scheme_y + 26 + i * 24;
        let sel = st2.partition_scheme == *scheme;
        let marker = if sel { "(*) " } else { "( ) " };
        let col = if sel { ACCENT_CYAN } else { TEXT_DIM };
        draw_text(fb, x + 8, sy, marker, col);
        draw_text(fb, x + 44, sy, label, col);
    }
}

pub(super) fn render_filesystem(fb: &mut FrameBuffer, x: usize, y: usize) {
    draw_text(fb, x, y, "Filesystem", TEXT_WHITE);
    draw_text(fb, x, y + 28, "Choose the root filesystem:", TEXT_DIM);

    let st = INSTALLER_STATE.lock();
    let options = [FsType::Ext4, FsType::Btrfs, FsType::Xfs];
    let desc_col = Pixel::new(100, 100, 130, 255);

    for (i, &fs) in options.iter().enumerate() {
        let fy = y + 64 + i * 40;
        let sel = st.root_fs == fs;
        let bg = if sel {
            Pixel::new(30, 60, 80, 200)
        } else {
            Pixel::new(28, 28, 44, 180)
        };
        for yy in fy..fy + 32 {
            for xx in x..x + 350 {
                fb.set_pixel(xx, yy, bg);
            }
        }
        let marker = if sel { "(*) " } else { "( ) " };
        let col = if sel { ACCENT_CYAN } else { TEXT_DIM };
        draw_text(fb, x + 8, fy + 6, marker, col);
        draw_text(fb, x + 44, fy + 6, fs.label(), col);

        let desc = match fs {
            FsType::Ext4 => "Stable, mature, excellent journaling",
            FsType::Btrfs => "Copy-on-write, snapshots, compression",
            FsType::Xfs => "High-performance, parallel I/O",
            FsType::Fat32 => "EFI system partition only",
        };
        draw_text(fb, x + 44, fy + 20, desc, desc_col);
    }

    draw_text(fb, x, y + 200, "A 512MB FAT32 EFI partition will", TEXT_DIM);
    draw_text(
        fb,
        x,
        y + 220,
        "be created automatically for UEFI.",
        TEXT_DIM,
    );
}

pub(super) fn render_bootloader(fb: &mut FrameBuffer, x: usize, y: usize) {
    draw_text(fb, x, y, "Bootloader", TEXT_WHITE);
    draw_text(fb, x, y + 28, "Select the bootloader to install:", TEXT_DIM);

    let st = INSTALLER_STATE.lock();
    let options = [BootloaderType::Grub, BootloaderType::SystemdBoot];
    let desc_col = Pixel::new(100, 100, 130, 255);

    for (i, &bl) in options.iter().enumerate() {
        let by = y + 64 + i * 50;
        let sel = st.bootloader == bl;
        let bg = if sel {
            Pixel::new(30, 60, 80, 200)
        } else {
            Pixel::new(28, 28, 44, 180)
        };
        for yy in by..by + 42 {
            for xx in x..x + 380 {
                fb.set_pixel(xx, yy, bg);
            }
        }
        let marker = if sel { "(*) " } else { "( ) " };
        let col = if sel { ACCENT_CYAN } else { TEXT_DIM };
        draw_text(fb, x + 8, by + 4, marker, col);
        draw_text(fb, x + 44, by + 4, bl.label(), col);

        let desc = match bl {
            BootloaderType::Grub => "BIOS + UEFI, chainloading, rescue",
            BootloaderType::SystemdBoot => "UEFI only, minimal and fast",
        };
        draw_text(fb, x + 44, by + 24, desc, desc_col);
    }
}

pub(super) fn render_user_account(fb: &mut FrameBuffer, x: usize, y: usize) {
    draw_text(fb, x, y, "User Account", TEXT_WHITE);
    draw_text(fb, x, y + 28, "Create your user account:", TEXT_DIM);

    let st = INSTALLER_STATE.lock();
    let fields: [(&str, &[u8], usize, bool); 5] = [
        ("Full Name:", &st.full_name, st.full_name_len, false),
        ("Username:", &st.username, st.username_len, false),
        ("Password:", &st.password, st.password_len, true),
        (
            "Confirm:",
            &st.password_confirm,
            st.password_confirm_len,
            true,
        ),
        ("Hostname:", &st.hostname, st.hostname_len, false),
    ];

    for (i, (label, buf, len, is_pw)) in fields.iter().enumerate() {
        let fy = y + 64 + i * 48;
        let focused = st.focused_field == i as u8;

        draw_text(fb, x, fy, label, TEXT_DIM);

        // Input box
        let box_x = x + 100;
        let box_w = 300usize;
        let box_h = 28usize;
        let border = if focused {
            ACCENT_CYAN
        } else {
            Pixel::new(60, 60, 80, 255)
        };
        let bg = Pixel::new(18, 18, 30, 255);
        for yy in fy..fy + box_h {
            for xx in box_x..box_x + box_w {
                if yy == fy || yy == fy + box_h - 1 || xx == box_x || xx == box_x + box_w - 1 {
                    fb.set_pixel(xx, yy, border);
                } else {
                    fb.set_pixel(xx, yy, bg);
                }
            }
        }

        if *is_pw {
            for d in 0..(*len).min(30) {
                draw_text(fb, box_x + 6 + d * 8, fy + 6, "*", TEXT_WHITE);
            }
        } else {
            let text = core::str::from_utf8(&buf[..*len]).unwrap_or("");
            draw_text(fb, box_x + 6, fy + 6, text, TEXT_WHITE);
        }

        if focused {
            let cursor_x = box_x + 6 + *len * 8;
            for yy in fy + 4..fy + box_h - 4 {
                fb.set_pixel(cursor_x, yy, ACCENT_CYAN);
            }
        }
    }
}

pub(super) fn render_timezone(fb: &mut FrameBuffer, x: usize, y: usize) {
    draw_text(fb, x, y, "Timezone", TEXT_WHITE);
    draw_text(fb, x, y + 28, "Select your timezone:", TEXT_DIM);

    let st = INSTALLER_STATE.lock();
    for (i, &tz) in TIMEZONES.iter().enumerate() {
        let ty = y + 60 + i * 24;
        let sel = i == st.timezone_idx;
        let col = if sel { ACCENT_CYAN } else { TEXT_DIM };
        let marker = if sel { "> " } else { "  " };
        draw_text(fb, x + 8, ty, marker, col);
        draw_text(fb, x + 28, ty, tz, col);
    }
}

pub(super) fn render_install(fb: &mut FrameBuffer, x: usize, y: usize, _w: usize) {
    let progress = INSTALL_PROGRESS.load(Ordering::Relaxed);
    let complete = INSTALL_COMPLETE.load(Ordering::Relaxed);

    if complete {
        draw_text(
            fb,
            x,
            y,
            "Installation Complete!",
            Pixel::new(100, 220, 140, 255),
        );
        draw_text(
            fb,
            x,
            y + 30,
            "KnoxOS has been installed successfully.",
            TEXT_DIM,
        );
        draw_text(fb, x, y + 54, "Click 'Reboot' to restart.", TEXT_DIM);
        return;
    }

    if progress == 0 {
        draw_text(fb, x, y, "Ready to Install", TEXT_WHITE);
        draw_text(fb, x, y + 30, "Review your selections:", TEXT_DIM);

        let st = INSTALLER_STATE.lock();
        let disk_name = core::str::from_utf8(
            &st.disks[st.selected_disk].name[..st.disks[st.selected_disk].name_len],
        )
        .unwrap_or("?");
        draw_text(fb, x + 16, y + 60, "Disk:", TEXT_DIM);
        draw_text(fb, x + 100, y + 60, disk_name, TEXT_WHITE);

        draw_text(fb, x + 16, y + 82, "Filesystem:", TEXT_DIM);
        draw_text(fb, x + 100, y + 82, st.root_fs.label(), TEXT_WHITE);

        draw_text(fb, x + 16, y + 104, "Bootloader:", TEXT_DIM);
        draw_text(fb, x + 100, y + 104, st.bootloader.label(), TEXT_WHITE);

        let user = core::str::from_utf8(&st.username[..st.username_len]).unwrap_or("?");
        draw_text(fb, x + 16, y + 126, "User:", TEXT_DIM);
        draw_text(fb, x + 100, y + 126, user, TEXT_WHITE);

        let host = core::str::from_utf8(&st.hostname[..st.hostname_len]).unwrap_or("?");
        draw_text(fb, x + 16, y + 148, "Hostname:", TEXT_DIM);
        draw_text(fb, x + 100, y + 148, host, TEXT_WHITE);

        draw_text(fb, x + 16, y + 170, "Timezone:", TEXT_DIM);
        draw_text(fb, x + 100, y + 170, TIMEZONES[st.timezone_idx], TEXT_WHITE);

        draw_text(fb, x, y + 210, "Click 'Install' to begin.", ACCENT_CYAN);
    } else {
        draw_text(fb, x, y, "Installing KnoxOS...", TEXT_WHITE);

        // Progress bar
        let bar_y = y + 40;
        let bar_w = 380usize;
        let bar_h = 24usize;
        let border_col = Pixel::new(60, 60, 80, 255);
        let fill_col = ACCENT_CYAN;
        let bg_col = Pixel::new(18, 18, 30, 255);

        for yy in bar_y..bar_y + bar_h {
            for xx in x..x + bar_w {
                if yy == bar_y || yy == bar_y + bar_h - 1 || xx == x || xx == x + bar_w - 1 {
                    fb.set_pixel(xx, yy, border_col);
                } else {
                    let fill_w = (bar_w - 2) * progress as usize / 100;
                    if xx - x - 1 < fill_w {
                        fb.set_pixel(xx, yy, fill_col);
                    } else {
                        fb.set_pixel(xx, yy, bg_col);
                    }
                }
            }
        }

        let mut pct_buf = [0u8; 16];
        let pct_str = format_u64(progress as u64, &mut pct_buf);
        draw_text(fb, x + bar_w / 2 - 12, bar_y + 4, pct_str, TEXT_WHITE);
        draw_text(fb, x + bar_w / 2 + 8, bar_y + 4, "%", TEXT_WHITE);

        // Log area
        let st = INSTALLER_STATE.lock();
        let log_text = core::str::from_utf8(&st.install_log[..st.install_log_len]).unwrap_or("");
        let lines: Vec<&str> = log_text.lines().collect();
        let start = if lines.len() > 12 {
            lines.len() - 12
        } else {
            0
        };
        for (i, line) in lines[start..].iter().enumerate() {
            draw_text(fb, x + 4, bar_y + 40 + i * 18, line, TEXT_DIM);
        }
    }
}
