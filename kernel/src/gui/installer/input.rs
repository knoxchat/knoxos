use core::sync::atomic::Ordering;

use super::engine::{begin_installation, do_reboot};
use super::nav::{go_back, go_next};
use super::state::{CURRENT_STEP, INSTALL_COMPLETE, INSTALL_PROGRESS, INSTALLER_STATE, is_active};
use super::types::{
    BUTTON_HEIGHT, BUTTON_WIDTH, BootloaderType, CARD_HEIGHT, CARD_WIDTH, FsType, PartitionScheme,
    SIDEBAR_WIDTH, STEP_COUNT, TIMEZONES,
};

// ─── Input Handling ──────────────────────────────────────────────────

/// Handle a keyboard character input
pub fn handle_char(c: char) {
    if !is_active() {
        return;
    }
    let step = CURRENT_STEP.load(Ordering::Relaxed);
    if step == 4 {
        let mut st = INSTALLER_STATE.lock();
        st.clear_error();
        match st.focused_field {
            0 => {
                let idx = st.full_name_len;
                if idx < 63 {
                    st.full_name[idx] = c as u8;
                    st.full_name_len = idx + 1;
                }
            }
            1 => {
                let idx = st.username_len;
                if idx < 31 && (c.is_ascii_alphanumeric() || c == '_' || c == '-') {
                    st.username[idx] = c as u8;
                    st.username_len = idx + 1;
                }
            }
            2 => {
                let idx = st.password_len;
                if idx < 63 {
                    st.password[idx] = c as u8;
                    st.password_len = idx + 1;
                }
            }
            3 => {
                let idx = st.password_confirm_len;
                if idx < 63 {
                    st.password_confirm[idx] = c as u8;
                    st.password_confirm_len = idx + 1;
                }
            }
            4 => {
                let idx = st.hostname_len;
                if idx < 63 && (c.is_ascii_alphanumeric() || c == '-') {
                    st.hostname[idx] = c as u8;
                    st.hostname_len = idx + 1;
                }
            }
            _ => {}
        }
    }
}

/// Handle backspace
pub fn handle_backspace() {
    if !is_active() {
        return;
    }
    let step = CURRENT_STEP.load(Ordering::Relaxed);
    if step == 4 {
        let mut st = INSTALLER_STATE.lock();
        match st.focused_field {
            0 if st.full_name_len > 0 => {
                st.full_name_len -= 1;
            }
            1 if st.username_len > 0 => {
                st.username_len -= 1;
            }
            2 if st.password_len > 0 => {
                st.password_len -= 1;
            }
            3 if st.password_confirm_len > 0 => {
                st.password_confirm_len -= 1;
            }
            4 if st.hostname_len > 0 => {
                st.hostname_len -= 1;
            }
            _ => {}
        }
    }
}

/// Handle Tab key — cycle focus in user account step
pub fn handle_tab() {
    if !is_active() {
        return;
    }
    if CURRENT_STEP.load(Ordering::Relaxed) == 4 {
        let mut st = INSTALLER_STATE.lock();
        st.focused_field = (st.focused_field + 1) % 5;
    }
}

/// Handle Up arrow
pub fn handle_arrow_up() {
    if !is_active() {
        return;
    }
    let step = CURRENT_STEP.load(Ordering::Relaxed);
    let mut st = INSTALLER_STATE.lock();
    match step {
        0 if st.language_idx > 0 => {
            st.language_idx -= 1;
        }
        1 if st.selected_disk > 0 => {
            st.selected_disk -= 1;
        }
        5 if st.timezone_idx > 0 => {
            st.timezone_idx -= 1;
        }
        _ => {}
    }
}

/// Handle Down arrow
pub fn handle_arrow_down() {
    if !is_active() {
        return;
    }
    let step = CURRENT_STEP.load(Ordering::Relaxed);
    let mut st = INSTALLER_STATE.lock();
    match step {
        0 if st.language_idx < 5 => {
            st.language_idx += 1;
        }
        1 if st.selected_disk + 1 < st.disk_count => {
            st.selected_disk += 1;
        }
        5 if st.timezone_idx + 1 < TIMEZONES.len() => {
            st.timezone_idx += 1;
        }
        _ => {}
    }
}

/// Handle mouse click
pub fn handle_click(mx: usize, my: usize, screen_w: usize, screen_h: usize) {
    if !is_active() {
        return;
    }

    let cx = screen_w.saturating_sub(CARD_WIDTH) / 2;
    let cy = screen_h.saturating_sub(CARD_HEIGHT) / 2;
    let cur_step = CURRENT_STEP.load(Ordering::Relaxed) as usize;
    let btn_y = cy + CARD_HEIGHT - 52;

    // Back button
    if cur_step > 0 && !INSTALL_COMPLETE.load(Ordering::Relaxed) {
        let back_x = cx + SIDEBAR_WIDTH + 24;
        if mx >= back_x && mx < back_x + BUTTON_WIDTH && my >= btn_y && my < btn_y + BUTTON_HEIGHT {
            go_back();
            return;
        }
    }

    // Next / Install / Reboot
    let next_x = cx + CARD_WIDTH - BUTTON_WIDTH - 24;
    if mx >= next_x && mx < next_x + BUTTON_WIDTH && my >= btn_y && my < btn_y + BUTTON_HEIGHT {
        if INSTALL_COMPLETE.load(Ordering::Relaxed) {
            do_reboot();
            return;
        }
        if cur_step == STEP_COUNT - 1 {
            if INSTALL_PROGRESS.load(Ordering::Relaxed) == 0 {
                begin_installation();
            }
        } else {
            go_next();
        }
        return;
    }

    // Step-specific clicks
    let content_x = cx + SIDEBAR_WIDTH + 24;
    let content_y = cy + 24;

    match cur_step {
        1 => {
            let count = { INSTALLER_STATE.lock().disk_count };
            for i in 0..count {
                let dy = content_y + 60 + i * 48;
                if my >= dy && my < dy + 40 && mx >= content_x && mx < content_x + 380 {
                    INSTALLER_STATE.lock().selected_disk = i;
                    return;
                }
            }
            let scheme_y = content_y + 60 + count * 48 + 20;
            for i in 0..2 {
                let sy = scheme_y + 26 + i * 24;
                if my >= sy && my < sy + 20 && mx >= content_x {
                    INSTALLER_STATE.lock().partition_scheme = if i == 0 {
                        PartitionScheme::EraseAll
                    } else {
                        PartitionScheme::Manual
                    };
                    return;
                }
            }
        }
        2 => {
            let options = [FsType::Ext4, FsType::Btrfs, FsType::Xfs];
            for (i, &fs) in options.iter().enumerate() {
                let fy = content_y + 64 + i * 40;
                if my >= fy && my < fy + 32 && mx >= content_x && mx < content_x + 350 {
                    INSTALLER_STATE.lock().root_fs = fs;
                    return;
                }
            }
        }
        3 => {
            let options = [BootloaderType::Grub, BootloaderType::SystemdBoot];
            for (i, &bl) in options.iter().enumerate() {
                let by = content_y + 64 + i * 50;
                if my >= by && my < by + 42 && mx >= content_x && mx < content_x + 380 {
                    INSTALLER_STATE.lock().bootloader = bl;
                    return;
                }
            }
        }
        4 => {
            for i in 0..5u8 {
                let fy = content_y + 64 + i as usize * 48;
                let box_x = content_x + 100;
                if my >= fy && my < fy + 28 && mx >= box_x && mx < box_x + 300 {
                    INSTALLER_STATE.lock().focused_field = i;
                    return;
                }
            }
        }
        5 => {
            for i in 0..TIMEZONES.len() {
                let ty = content_y + 60 + i * 24;
                if my >= ty && my < ty + 20 && mx >= content_x {
                    INSTALLER_STATE.lock().timezone_idx = i;
                    return;
                }
            }
        }
        _ => {}
    }
}
