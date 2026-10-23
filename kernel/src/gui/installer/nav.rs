use core::sync::atomic::Ordering;

use super::state::{CURRENT_STEP, INSTALLER_STATE};
use super::types::STEP_COUNT;

// ─── Navigation ──────────────────────────────────────────────────────

pub(super) fn go_next() {
    let cur = CURRENT_STEP.load(Ordering::Relaxed);
    if validate_step(cur) && (cur as usize) < STEP_COUNT - 1 {
        CURRENT_STEP.store(cur + 1, Ordering::Relaxed);
    }
}

pub(super) fn go_back() {
    let cur = CURRENT_STEP.load(Ordering::Relaxed);
    if cur > 0 {
        CURRENT_STEP.store(cur - 1, Ordering::Relaxed);
    }
}

fn validate_step(step: u8) -> bool {
    let mut st = INSTALLER_STATE.lock();
    st.clear_error();
    match step {
        1 if st.disk_count == 0 => {
            st.set_error("No disk available for installation");
            return false;
        }
        4 => {
            if st.username_len == 0 {
                st.set_error("Username is required");
                return false;
            }
            if st.password_len == 0 {
                st.set_error("Password is required");
                return false;
            }
            if st.password_len != st.password_confirm_len
                || st.password[..st.password_len] != st.password_confirm[..st.password_confirm_len]
            {
                st.set_error("Passwords do not match");
                return false;
            }
            if st.hostname_len == 0 {
                st.set_error("Hostname is required");
                return false;
            }
        }
        _ => {}
    }
    true
}
