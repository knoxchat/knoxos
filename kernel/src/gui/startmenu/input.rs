//! Hit testing plus click, hover, scroll, keyboard, and visibility control.

use alloc::string::String;

use crate::gui::font_engine;
use crate::gui::framebuffer::Rect;

use super::layout::{
    AI_INPUT_H, BOTTOM_BAR_H, DOCK_GAP, ITEM_INSET, MENU_ITEM_HEIGHT, PAD_H, PANEL_MAX_HEIGHT,
    PANEL_WIDTH, QUICK_ACTION_COLS, QUICK_ACTION_GAP, QUICK_ACTION_SIZE, SEARCH_BAR_H, TAB_BAR_H,
};
use super::state::START_MENU;
use super::types::{CommandCenterTab, MenuCategory, StartMenuItem, VoiceState};

// ═══════════════════════════════════════════════════════════════════════════
// GEOMETRY & HIT TESTING
// ═══════════════════════════════════════════════════════════════════════════

/// Compute panel geometry for hit testing.
fn panel_geometry(screen_w: i32, screen_h: i32) -> (i32, i32, u32, i32, i32, usize) {
    let dock_bottom_y = screen_h - crate::gui::scale::taskbar_height() as i32;
    let panel_h = PANEL_MAX_HEIGHT;
    let pw = PANEL_WIDTH as i32;
    let panel_x = (screen_w - pw) / 2;
    let panel_y = dock_bottom_y - panel_h as i32 - DOCK_GAP;

    let tab_y = panel_y + 14;
    let search_y = tab_y + TAB_BAR_H as i32 + 10;
    let content_y = search_y + SEARCH_BAR_H as i32 + 8;
    let list_start_y = content_y + 18;
    let list_h = panel_h as i32 - (list_start_y - panel_y) - BOTTOM_BAR_H as i32;
    let max_visible = (list_h / MENU_ITEM_HEIGHT as i32).max(0) as usize;

    (
        panel_x,
        panel_y,
        panel_h,
        content_y,
        list_start_y,
        max_visible,
    )
}

/// Compute tab bar hit regions.
fn tab_hit_test(
    panel_x: i32,
    panel_y: i32,
    panel_w: u32,
    x: i32,
    y: i32,
) -> Option<CommandCenterTab> {
    let tab_y = panel_y + 14;
    let tab_total_w = (panel_w as i32 - PAD_H * 2) as u32;
    let tab_w = tab_total_w / 3;

    if y < tab_y || y >= tab_y + TAB_BAR_H as i32 {
        return None;
    }
    let rel_x = x - (panel_x + PAD_H);
    if rel_x < 0 || rel_x >= tab_total_w as i32 {
        return None;
    }
    match rel_x / tab_w as i32 {
        0 => Some(CommandCenterTab::Apps),
        1 => Some(CommandCenterTab::AI),
        2 => Some(CommandCenterTab::Tasks),
        _ => None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PUBLIC API — Click, Hover, Scroll, Keyboard, Toggle
// ═══════════════════════════════════════════════════════════════════════════

/// Handle a click on the start menu, returns true if consumed
pub fn handle_click(x: i32, y: i32) -> Option<StartMenuItem> {
    if crate::gui::gpui_platform::launcher_is_live() {
        return None;
    }
    let mut menu = START_MENU.lock();
    if !menu.visible {
        return None;
    }

    let (screen_w, screen_h) = crate::gui::cached_screen_size();
    let (panel_x, panel_y, panel_h, content_y, list_start_y, max_visible) =
        panel_geometry(screen_w, screen_h);

    let panel_rect = Rect::new(panel_x, panel_y, PANEL_WIDTH, panel_h);

    if !panel_rect.contains(x, y) {
        menu.visible = false;
        return None;
    }

    // Tab bar clicks
    if let Some(tab) = tab_hit_test(panel_x, panel_y, PANEL_WIDTH, x, y) {
        menu.set_tab(tab);
        return None;
    }

    // Voice button click
    let search_y = panel_y + 14 + TAB_BAR_H as i32 + 10;
    let mic_cx = panel_x + PANEL_WIDTH as i32 - PAD_H - 28;
    let mic_cy = search_y + SEARCH_BAR_H as i32 / 2;
    if (x - mic_cx).abs() < 14 && (y - mic_cy).abs() < 14 {
        menu.voice_state = match menu.voice_state {
            VoiceState::Idle => VoiceState::Listening,
            VoiceState::Listening => VoiceState::Idle,
            VoiceState::Processing => VoiceState::Idle,
        };
        return None;
    }

    // Apps tab: item clicks
    if menu.active_tab == CommandCenterTab::Apps
        && x > panel_x + ITEM_INSET
        && x < panel_x + PANEL_WIDTH as i32 - ITEM_INSET
    {
        for (vi, &real_idx) in menu
            .filtered_indices
            .iter()
            .enumerate()
            .skip(menu.scroll_offset)
            .take(max_visible)
        {
            let item_y =
                list_start_y + ((vi - menu.scroll_offset) as i32 * MENU_ITEM_HEIGHT as i32);
            if y >= item_y && y < item_y + MENU_ITEM_HEIGHT as i32 {
                let result = menu.items[real_idx].clone();
                menu.items[real_idx].launch_count += 1;
                menu.visible = false;
                return Some(result);
            }
        }
    }

    // AI tab: quick action clicks
    if menu.active_tab == CommandCenterTab::AI {
        let qa_y = content_y + 4;
        let qa_total_w =
            QUICK_ACTION_COLS * QUICK_ACTION_SIZE + (QUICK_ACTION_COLS - 1) * QUICK_ACTION_GAP;
        let qa_start_x = panel_x + (PANEL_WIDTH as i32 - qa_total_w as i32) / 2;

        for (i, action) in menu.quick_actions.iter_mut().enumerate() {
            let col = i as u32;
            let ax = qa_start_x + (col * (QUICK_ACTION_SIZE + QUICK_ACTION_GAP)) as i32;
            let ay = qa_y;
            let action_rect = Rect::new(ax, ay, QUICK_ACTION_SIZE, QUICK_ACTION_SIZE);
            if action_rect.contains(x, y) {
                action.enabled = !action.enabled;
                return None;
            }
        }

        // Send button click
        let div_y = qa_y + QUICK_ACTION_SIZE as i32 + 10;
        let chat_y = div_y + 8;
        let ai_content_h = panel_h as i32 - (content_y - panel_y) - BOTTOM_BAR_H as i32;
        let chat_h = ai_content_h - (chat_y - content_y) - AI_INPUT_H as i32 - 8;
        let input_y = chat_y + chat_h + 4;
        let input_x = panel_x + PAD_H;
        let content_w = PANEL_WIDTH as i32 - PAD_H * 2;
        let input_w = content_w - 40;
        let send_x = input_x + input_w + 6;
        let send_size = AI_INPUT_H - 8;

        let send_rect = Rect::new(send_x, input_y + 4, send_size, send_size);
        if send_rect.contains(x, y) {
            menu.submit_chat();
            return None;
        }

        // Suggested prompt clicks
        if menu.chat_messages.len() <= 2 && !menu.ai_is_thinking {
            let suggest_y = chat_y + chat_h - (menu.suggested_prompts.len() as i32 * 24 + 8);
            for (i, prompt) in menu.suggested_prompts.iter().enumerate() {
                let sy = suggest_y + i as i32 * 24;
                let prompt_w = font_engine::measure_ui_text(prompt, 12) + 16;
                let sx = panel_x + PAD_H + 4;
                let prompt_rect = Rect::new(sx, sy, prompt_w, 20);
                if prompt_rect.contains(x, y) {
                    menu.chat_input = prompt.clone();
                    menu.submit_chat();
                    return None;
                }
            }
        }
    }

    // Bottom bar: power button — shows power menu (shutdown/restart/logout)
    let bottom_y = panel_y + panel_h as i32 - BOTTOM_BAR_H as i32;
    let bar_cy = bottom_y + BOTTOM_BAR_H as i32 / 2;
    let power_cx = panel_x + PANEL_WIDTH as i32 - PAD_H - 18;
    if (x - power_cx).abs() < 12 && (y - bar_cy).abs() < 12 {
        // Toggle the power menu popup
        menu.power_menu_visible = !menu.power_menu_visible;
        return None;
    }

    // Power menu option clicks (when power menu popup is visible)
    if menu.power_menu_visible {
        let pm_x = panel_x + PANEL_WIDTH as i32 - PAD_H - 130;
        let pm_y = bottom_y - 110;
        let pm_w = 120;
        let pm_item_h = 30;
        // Shutdown
        if x >= pm_x && x < pm_x + pm_w && y >= pm_y && y < pm_y + pm_item_h {
            menu.visible = false;
            menu.power_menu_visible = false;
            crate::serial_println!("[KnoxOS] Power: Shutdown requested via GUI");
            crate::acpi::shutdown();
        }
        // Restart
        if x >= pm_x && x < pm_x + pm_w && y >= pm_y + pm_item_h && y < pm_y + 2 * pm_item_h {
            menu.visible = false;
            menu.power_menu_visible = false;
            crate::serial_println!("[KnoxOS] Power: Reboot requested via GUI");
            crate::acpi::reboot();
        }
        // Log Out (lock screen)
        if x >= pm_x && x < pm_x + pm_w && y >= pm_y + 2 * pm_item_h && y < pm_y + 3 * pm_item_h {
            menu.visible = false;
            menu.power_menu_visible = false;
            crate::serial_println!("[KnoxOS] Power: Lock screen requested via GUI");
            crate::gui::lock_screen::lock();
            return None;
        }
        // Click anywhere else dismisses the power menu
        menu.power_menu_visible = false;
        return None;
    }

    // Bottom bar: settings shortcut
    let settings_cx = panel_x + PANEL_WIDTH as i32 - PAD_H - 50;
    if (x - settings_cx).abs() < 12 && (y - bar_cy).abs() < 12 {
        menu.visible = false;
        return Some(StartMenuItem {
            name: String::from("Settings"),
            description: String::from("System configuration"),
            category: MenuCategory::System,
            icon_type: crate::gui::desktop::IconType::Settings,
            keywords: String::new(),
            pinned: true,
            launch_count: 0,
            last_launched: 0,
        });
    }

    None
}

/// Toggle the start menu visibility
pub fn toggle() {
    START_MENU.lock().toggle();
    if is_visible() {
        crate::gui::gpui_platform::launcher_reset();
        crate::gui::gpui_platform::claim_key_focus();
    } else {
        crate::gui::gpui_platform::release_key_focus();
    }
    crate::gui::request_redraw();
}

/// Check if start menu is currently visible
pub fn is_visible() -> bool {
    START_MENU.lock().visible
}

/// Close the start menu
pub fn close() {
    let mut menu = START_MENU.lock();
    if menu.visible {
        menu.visible = false;
        drop(menu);
        crate::gui::gpui_platform::release_key_focus();
        crate::gui::request_redraw();
    }
}

/// Handle scroll wheel in start menu
pub fn handle_scroll(scroll_z: i8) {
    let mut menu = START_MENU.lock();
    if !menu.visible {
        return;
    }

    match menu.active_tab {
        CommandCenterTab::Apps => {
            if scroll_z > 0 {
                menu.scroll_offset = menu.scroll_offset.saturating_sub(2);
            } else {
                let max = menu.filtered_indices.len().saturating_sub(6);
                menu.scroll_offset = (menu.scroll_offset + 2).min(max);
            }
        }
        CommandCenterTab::AI => {
            if scroll_z > 0 {
                menu.chat_scroll_y = menu.chat_scroll_y.saturating_sub(40);
            } else {
                menu.chat_scroll_y += 40;
            }
        }
        CommandCenterTab::Tasks => {
            if scroll_z > 0 {
                menu.tasks_scroll_y = menu.tasks_scroll_y.saturating_sub(40);
            } else {
                menu.tasks_scroll_y += 40;
            }
        }
    }
}

/// Update hover state based on mouse position (called during redraw)
pub fn update_hover(mouse_x: i32, mouse_y: i32) {
    let mut menu = START_MENU.lock();
    if !menu.visible {
        return;
    }

    if menu.active_tab != CommandCenterTab::Apps {
        menu.hovered_index = None;
        return;
    }

    let (screen_w, screen_h) = crate::gui::cached_screen_size();
    let (panel_x, _panel_y, _panel_h, _content_y, list_start_y, max_visible) =
        panel_geometry(screen_w, screen_h);

    let mut hovered = None;
    if mouse_x > panel_x + ITEM_INSET && mouse_x < panel_x + PANEL_WIDTH as i32 - ITEM_INSET {
        for (vi, _) in menu
            .filtered_indices
            .iter()
            .enumerate()
            .skip(menu.scroll_offset)
            .take(max_visible)
        {
            let item_y =
                list_start_y + ((vi - menu.scroll_offset) as i32 * MENU_ITEM_HEIGHT as i32);
            if mouse_y >= item_y && mouse_y < item_y + MENU_ITEM_HEIGHT as i32 {
                hovered = Some(vi);
                break;
            }
        }
    }
    menu.hovered_index = hovered;
}

/// Handle keyboard input when command center is visible.
/// Returns true if the key was consumed.
pub fn handle_key(scancode: u8, ch: Option<char>) -> bool {
    if crate::gui::gpui_platform::launcher_is_live() {
        return false;
    }
    let mut menu = START_MENU.lock();
    if !menu.visible {
        return false;
    }

    // Escape — close
    if scancode == 0x01 {
        menu.visible = false;
        drop(menu);
        crate::gui::request_redraw();
        return true;
    }

    // Tab key — cycle tabs
    if scancode == 0x0F {
        let next = match menu.active_tab {
            CommandCenterTab::Apps => CommandCenterTab::AI,
            CommandCenterTab::AI => CommandCenterTab::Tasks,
            CommandCenterTab::Tasks => CommandCenterTab::Apps,
        };
        menu.set_tab(next);
        return true;
    }

    // Enter — submit
    if scancode == 0x1C {
        match menu.active_tab {
            CommandCenterTab::Apps => {
                let target_idx = if let Some(vi) = menu.hovered_index {
                    menu.filtered_indices.get(vi).copied()
                } else if !menu.filtered_indices.is_empty() {
                    Some(menu.filtered_indices[0])
                } else {
                    None
                };

                if let Some(real_idx) = target_idx {
                    let result = menu.items[real_idx].clone();
                    menu.items[real_idx].launch_count += 1;
                    menu.visible = false;
                    drop(menu);
                    crate::gui::desktop::open_application(&result.name, result.icon_type);
                    crate::gui::request_redraw();
                    return true;
                }
            }
            CommandCenterTab::AI => {
                menu.submit_chat();
            }
            CommandCenterTab::Tasks => {}
        }
        return true;
    }

    // Backspace
    if scancode == 0x0E {
        menu.backspace();
        return true;
    }

    // Arrow Up
    if scancode == 0x48 {
        if menu.active_tab == CommandCenterTab::Apps && !menu.filtered_indices.is_empty() {
            let current = menu.hovered_index.unwrap_or(0);
            if current > 0 {
                menu.hovered_index = Some(current - 1);
                if current - 1 < menu.scroll_offset {
                    menu.scroll_offset = current - 1;
                }
            }
        }
        return true;
    }

    // Arrow Down
    if scancode == 0x50 {
        if menu.active_tab == CommandCenterTab::Apps && !menu.filtered_indices.is_empty() {
            let current = menu.hovered_index.unwrap_or(0);
            let max_idx = menu.filtered_indices.len().saturating_sub(1);
            if current < max_idx {
                menu.hovered_index = Some(current + 1);
            }
        }
        return true;
    }

    // Printable character — type into search/chat
    if let Some(ch) = ch {
        if ch.is_ascii_graphic() || ch == ' ' {
            menu.type_char(ch);
            return true;
        }
    }

    false
}
