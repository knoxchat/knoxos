/// Quick actions / jump lists shown on right-click of a dock entry.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::gui::font_engine;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::gui::window;

use super::state::TASKBAR;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Quick Actions / Jump Lists   (8.15)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// A single quick-action entry in a jump list
#[derive(Clone)]
pub struct QuickAction {
    pub label: String,
    pub action_id: u32,
}

/// Jump list state — shown on right-click of a dock entry
struct JumpListPopup {
    visible: bool,
    window_id: window::WindowId,
    x: i32,
    y: i32,
    actions: Vec<QuickAction>,
    hovered: Option<usize>,
}

static JUMP_LIST: Mutex<JumpListPopup> = Mutex::new(JumpListPopup {
    visible: false,
    window_id: 0,
    x: 0,
    y: 0,
    actions: Vec::new(),
    hovered: None,
});

/// Register quick actions (jump list) for an application
pub fn register_quick_actions(window_id: window::WindowId, actions: Vec<QuickAction>) {
    // Store in a side table keyed by content type
    let tb = TASKBAR.lock();
    if let Some(entry) = tb.entries.iter().find(|e| e.window_id == window_id) {
        let ct = entry.content_type;
        drop(tb);
        let mut qa = APP_QUICK_ACTIONS.lock();
        // Store by content type so all windows of same app share jump list
        if let Some(existing) = qa.iter_mut().find(|a| a.0 == ct) {
            existing.1 = actions;
        } else {
            qa.push((ct, actions));
        }
    }
}

/// Quick actions storage keyed by content type
static APP_QUICK_ACTIONS: Mutex<Vec<(window::WindowContentType, Vec<QuickAction>)>> =
    Mutex::new(Vec::new());

/// Show jump list popup for a dock entry (on right-click)
pub fn show_jump_list(entry_index: usize, x: i32, y: i32) {
    let tb = TASKBAR.lock();
    if let Some(entry) = tb.entries.get(entry_index) {
        let ct = entry.content_type;
        let wid = entry.window_id;
        drop(tb);

        let qa = APP_QUICK_ACTIONS.lock();
        let actions = qa
            .iter()
            .find(|a| a.0 == ct)
            .map(|a| a.1.clone())
            .unwrap_or_default();
        drop(qa);

        // Default actions if none registered
        let actions = if actions.is_empty() {
            alloc::vec![
                QuickAction {
                    label: String::from("New Window"),
                    action_id: 1
                },
                QuickAction {
                    label: String::from("Close"),
                    action_id: 2
                },
            ]
        } else {
            actions
        };

        let mut jl = JUMP_LIST.lock();
        jl.visible = true;
        jl.window_id = wid;
        jl.x = x;
        jl.y = y - (actions.len() as i32 * 28 + 8);
        jl.actions = actions;
        jl.hovered = None;
    }
    crate::gui::request_redraw();
}

/// Close jump list
pub fn close_jump_list() {
    JUMP_LIST.lock().visible = false;
    crate::gui::request_redraw();
}

/// Is jump list popup visible
pub fn is_jump_list_visible() -> bool {
    JUMP_LIST.lock().visible
}

/// Draw the jump list popup
pub fn draw_jump_list(fb: &mut FrameBuffer) {
    let jl = JUMP_LIST.lock();
    if !jl.visible {
        return;
    }
    let count = jl.actions.len();
    let item_h = 28i32;
    let popup_w = 180u32;
    let popup_h = count as u32 * item_h as u32 + 8;

    // Background
    fb.fill_rounded_rect_aa(
        Rect::new(jl.x, jl.y, popup_w, popup_h),
        Pixel::new(25, 28, 40, 230),
        8,
    );
    fb.draw_rounded_rect(
        Rect::new(jl.x, jl.y, popup_w, popup_h),
        Pixel::new(60, 70, 90, 180),
        8,
        1,
    );

    let mut iy = jl.y + 4;
    for (i, action) in jl.actions.iter().enumerate() {
        let hovered = jl.hovered == Some(i);
        if hovered {
            fb.fill_rounded_rect_aa(
                Rect::new(jl.x + 4, iy, popup_w - 8, item_h as u32),
                Pixel::new(0, 120, 200, 60),
                4,
            );
        }
        let tc = if hovered {
            Pixel::new(0, 200, 255, 255)
        } else {
            Pixel::new(190, 200, 220, 230)
        };
        font_engine::draw_ui_text(fb, jl.x + 12, iy + 7, &action.label, 12, tc);
        iy += item_h;
    }
}

/// Update hover state for jump list
pub fn update_jump_list_hover(mx: i32, my: i32) {
    let mut jl = JUMP_LIST.lock();
    if !jl.visible {
        return;
    }
    let item_h = 28;
    let rel_y = my - jl.y - 4;
    if rel_y >= 0 && mx >= jl.x && mx < jl.x + 180 {
        let idx = rel_y / item_h;
        if (idx as usize) < jl.actions.len() {
            jl.hovered = Some(idx as usize);
            return;
        }
    }
    jl.hovered = None;
}

/// Handle click on jump list; returns selected action_id or None
pub fn handle_jump_list_click(mx: i32, my: i32) -> Option<u32> {
    let jl = JUMP_LIST.lock();
    if !jl.visible {
        return None;
    }
    let item_h = 28;
    let rel_y = my - jl.y - 4;
    if rel_y >= 0 && mx >= jl.x && mx < jl.x + 180 {
        let idx = rel_y / item_h;
        if (idx as usize) < jl.actions.len() {
            let action_id = jl.actions[idx as usize].action_id;
            drop(jl);
            close_jump_list();
            return Some(action_id);
        }
    }
    drop(jl);
    close_jump_list();
    None
}
