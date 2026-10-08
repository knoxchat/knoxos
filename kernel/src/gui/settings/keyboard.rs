/// Keyboard navigation for the Settings panel.
use super::{FocusRegion, SETTINGS_STATE, SettingsTab, TABS};

/// Handle a keyboard event in the Settings window.
/// Returns `true` if the key was consumed.
///
/// Keys handled:
///   Tab / Shift+Tab — cycle focus forward / backward
///   Enter / Space — activate focused element
///   ArrowUp / ArrowDown — move within current region
///   ArrowRight — move from sidebar into content
///   ArrowLeft — move from content back to sidebar
pub fn handle_settings_key(scancode: u8, character: Option<char>, shift: bool) -> bool {
    let mut state = SETTINGS_STATE.lock();
    state.keyboard_nav = true;

    let sidebar_n = state.sidebar_count();
    let content_n = state.content_item_count();

    match character {
        // ── Tab / Shift+Tab: cycle through ALL focusable items ──
        Some('\t') => {
            if shift {
                // Backward
                if state.focus_index < 0 {
                    // In sidebar
                    let tab_idx = -(state.focus_index + 1);
                    if tab_idx > 0 {
                        state.focus_index = -(tab_idx - 1 + 1);
                    } else {
                        // Wrap to last content item (or last sidebar if no content)
                        if content_n > 0 {
                            state.focus_index = content_n - 1;
                        } else {
                            state.focus_index = -(sidebar_n - 1 + 1);
                        }
                    }
                } else {
                    // In content
                    if state.focus_index > 0 {
                        state.focus_index -= 1;
                    } else {
                        // Wrap to last sidebar tab
                        state.focus_index = -(sidebar_n - 1 + 1);
                    }
                }
            } else {
                // Forward
                if state.focus_index < 0 {
                    let tab_idx = -(state.focus_index + 1);
                    if tab_idx < sidebar_n - 1 {
                        state.focus_index = -(tab_idx + 1 + 1);
                    } else {
                        // Move to first content item
                        if content_n > 0 {
                            state.focus_index = 0;
                        } else {
                            state.focus_index = -1; // Wrap to first sidebar
                        }
                    }
                } else {
                    if state.focus_index < content_n - 1 {
                        state.focus_index += 1;
                    } else {
                        // Wrap to first sidebar tab
                        state.focus_index = -1;
                    }
                }
            }
            true
        }

        // ── Enter / Space: activate focused element ──
        Some('\n') | Some('\r') | Some(' ') => {
            let region = state.focus_region();
            match region {
                FocusRegion::Sidebar(tab_idx) => {
                    // Switch to the focused tab
                    if let Some((tab, _, _)) = TABS.get(tab_idx as usize) {
                        state.active_tab = *tab;
                        // Reset content focus when switching tabs
                        state.focus_index = -(tab_idx + 1);
                    }
                }
                FocusRegion::Content(idx) => {
                    activate_content_item(&state.active_tab, idx);
                }
            }
            true
        }

        _ => {
            // Check scancode for arrow keys
            match scancode {
                // Up arrow (0x48 via scancode set 1, or we check via the raw code)
                0x48 => {
                    if state.focus_index < 0 {
                        let tab_idx = -(state.focus_index + 1);
                        if tab_idx > 0 {
                            state.focus_index = -(tab_idx - 1 + 1);
                        }
                    } else if state.focus_index > 0 {
                        state.focus_index -= 1;
                    }
                    true
                }
                // Down arrow
                0x50 => {
                    if state.focus_index < 0 {
                        let tab_idx = -(state.focus_index + 1);
                        if tab_idx < sidebar_n - 1 {
                            state.focus_index = -(tab_idx + 1 + 1);
                        }
                    } else if state.focus_index < content_n - 1 {
                        state.focus_index += 1;
                    }
                    true
                }
                // Right arrow — move from sidebar to content
                0x4D => {
                    if state.focus_index < 0 && content_n > 0 {
                        state.focus_index = 0;
                    }
                    true
                }
                // Left arrow — move from content to sidebar
                0x4B => {
                    if state.focus_index >= 0 {
                        // Go to the sidebar tab matching current active_tab
                        let tab_idx = TABS
                            .iter()
                            .position(|(t, _, _)| *t == state.active_tab)
                            .unwrap_or(0) as i32;
                        state.focus_index = -(tab_idx + 1);
                    }
                    true
                }
                _ => false,
            }
        }
    }
}

/// Activate a content item by index in the given tab
fn activate_content_item(tab: &SettingsTab, idx: i32) {
    match tab {
        SettingsTab::Display => {
            // 0..N-1 = resolution options, N = brightness (no-op for now)
            let res_count = crate::gui::RESOLUTIONS.len() as i32;
            if idx < res_count {
                if let Some(&(rw, rh, _)) = crate::gui::RESOLUTIONS.get(idx as usize) {
                    let (cur_w, cur_h) = crate::gui::screen_size();
                    if rw != cur_w || rh != cur_h {
                        crate::serial_println!(
                            "[Settings] Focus-activate resolution {}x{}",
                            rw,
                            rh
                        );
                        crate::gui::change_resolution(rw, rh);
                    }
                }
            }
        }
        SettingsTab::System => {
            // 0 = power saving toggle, 1..6 = keyboard layouts
            if (1..=6).contains(&idx) {
                let layouts = [
                    crate::task::keyboard::KeyboardLayout::Us,
                    crate::task::keyboard::KeyboardLayout::Uk,
                    crate::task::keyboard::KeyboardLayout::De,
                    crate::task::keyboard::KeyboardLayout::Fr,
                    crate::task::keyboard::KeyboardLayout::Es,
                    crate::task::keyboard::KeyboardLayout::Dvorak,
                ];
                if let Some(layout) = layouts.get((idx - 1) as usize) {
                    crate::task::keyboard::set_layout(*layout);
                    crate::serial_println!(
                        "[Settings] Focus-activate keyboard layout: {}",
                        layout.name()
                    );
                }
            }
        }
        SettingsTab::Users => {
            match idx {
                0 => {
                    // Lock Screen
                    crate::gui::lock_screen::lock();
                }
                1 => {
                    // Log Out
                    crate::gui::lock_screen::lock();
                    crate::gui::login::reset();
                }
                _ => {}
            }
        }
        SettingsTab::DateTime => {
            match idx {
                0 => {
                    // Toggle NTP
                    let mut dt = crate::gui::settings_ext::get_datetime();
                    // Toggle NTP by accessing the settings ext module
                    crate::serial_println!("[Settings] Toggle NTP");
                }
                1 => {
                    // Toggle 24h format
                    crate::serial_println!("[Settings] Toggle 24h format");
                }
                2 => {
                    // Toggle show seconds
                    crate::serial_println!("[Settings] Toggle show seconds");
                }
                i if i >= 3 => {
                    // Select timezone
                    let tz_idx = (i - 3) as usize;
                    let timezones = crate::gui::settings_ext::common_timezones();
                    if let Some(tz) = timezones.get(tz_idx) {
                        crate::gui::settings_ext::set_timezone(&tz.name);
                        crate::serial_println!("[Settings] Timezone: {}", tz.name);
                    }
                }
                _ => {}
            }
        }
        SettingsTab::Privacy => {
            let settings = [
                "location",
                "analytics",
                "camera",
                "microphone",
                "firewall",
                "autoupdate",
            ];
            if let Some(key) = settings.get(idx as usize) {
                let new_val = crate::gui::settings_ext::toggle_privacy(key);
                crate::serial_println!("[Settings] Privacy '{}' = {}", key, new_val);
            }
        }
        SettingsTab::Startup => {
            // Toggle startup app by index
            let startup = crate::gui::settings_ext::STARTUP.lock();
            if let Some(app) = startup.apps.get(idx as usize) {
                let name = app.name.clone();
                drop(startup);
                let new_val = crate::gui::settings_ext::toggle_startup_app(&name);
                crate::serial_println!("[Settings] Startup '{}' enabled = {}", name, new_val);
            }
        }
        _ => {
            // Other tabs: toggle/slider items — placeholder (would toggle state)
            crate::serial_println!(
                "[Settings] Focus-activate content item {} in {:?}",
                idx,
                tab
            );
        }
    }
}
