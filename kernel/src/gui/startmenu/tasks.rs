//! Tasks-tab agent cards and progress paint.

use crate::gui::colors;
use crate::gui::font_engine;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

use super::layout::{ITEM_INSET, PAD_H, TASK_ITEM_H};
use super::state::StartMenu;
use super::types::{AgentTask, TaskStatus};

// ═══════════════════════════════════════════════════════════════════════════
// TASKS CONTENT — Background AI agent tasks & workflows
// ═══════════════════════════════════════════════════════════════════════════

pub(super) fn draw_tasks_content(
    fb: &mut FrameBuffer,
    panel_x: i32,
    content_y: i32,
    panel_w: u32,
    content_h: u32,
    menu: &StartMenu,
) {
    let header_y = content_y + 4;
    let running_count = menu
        .agent_tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Running)
        .count();
    let header_text = alloc::format!(
        "{} active task{}",
        running_count,
        if running_count == 1 { "" } else { "s" }
    );
    font_engine::draw_ui_text(
        fb,
        panel_x + PAD_H + 4,
        header_y,
        &header_text,
        12,
        Pixel::new(80, 140, 220, 150),
    );

    let list_y = header_y + 22;
    let list_h = content_h as i32 - (list_y - content_y);
    let max_visible = (list_h / TASK_ITEM_H as i32).max(0) as usize;

    for (i, task) in menu.agent_tasks.iter().enumerate().take(max_visible) {
        let ty = list_y + i as i32 * TASK_ITEM_H as i32;
        draw_agent_task(
            fb,
            panel_x + ITEM_INSET,
            ty,
            panel_w - ITEM_INSET as u32 * 2,
            task,
        );
    }

    if menu.agent_tasks.is_empty() {
        let empty_y = content_y + content_h as i32 / 2 - 30;
        let cx = panel_x + panel_w as i32 / 2;
        {
            let _tw = font_engine::measure_ui_text("No active tasks", 12) as i32;
            font_engine::draw_ui_text(
                fb,
                cx - _tw / 2,
                empty_y,
                "No active tasks",
                12,
                Pixel::new(100, 120, 160, 160),
            );
        }
        {
            let _tw = font_engine::measure_ui_text("AI agent tasks will appear here", 12) as i32;
            font_engine::draw_ui_text(
                fb,
                cx - _tw / 2,
                empty_y + 20,
                "AI agent tasks will appear here",
                12,
                Pixel::new(80, 100, 140, 120),
            );
        }
    }
}

/// Draw a single agent task card
fn draw_agent_task(fb: &mut FrameBuffer, x: i32, y: i32, w: u32, task: &AgentTask) {
    fb.fill_rounded_rect_aa(
        Rect::new(x, y, w, TASK_ITEM_H - 4),
        Pixel::new(16, 20, 34, 160),
        10,
    );
    fb.draw_rounded_rect(
        Rect::new(x, y, w, TASK_ITEM_H - 4),
        Pixel::new(60, 54, 50, 30),
        10,
        1,
    );

    let status_color = match task.status {
        TaskStatus::Queued => Pixel::new(140, 132, 126, 200),
        TaskStatus::Running => colors::with_alpha(colors::ACCENT_BRIGHT, 240),
        TaskStatus::Complete => Pixel::new(130, 190, 150, 230),
        TaskStatus::Failed => Pixel::new(220, 85, 85, 230),
        TaskStatus::Paused => Pixel::new(230, 180, 70, 200),
    };
    fb.fill_circle_aa(x + 16, y + 14, 5, status_color);

    if task.status == TaskStatus::Running {
        fb.draw_circle_aa(
            x + 16,
            y + 14,
            7,
            colors::with_alpha(colors::ACCENT_BRIGHT, 70),
        );
    }

    font_engine::draw_ui_text(
        fb,
        x + 30,
        y + 6,
        &task.title,
        12,
        Pixel::new(230, 220, 210, 230),
    );
    font_engine::draw_ui_text(
        fb,
        x + 30,
        y + 20,
        &task.description,
        12,
        Pixel::new(120, 110, 100, 140),
    );

    // Progress bar
    let bar_x = x + 30;
    let bar_y = y + TASK_ITEM_H as i32 - 14;
    let bar_w = w as i32 - 46;

    fb.fill_rounded_rect_aa(
        Rect::new(bar_x, bar_y, bar_w.max(0) as u32, 4),
        Pixel::new(36, 32, 40, 120),
        2,
    );

    let fill_w = ((bar_w as u64 * task.progress as u64) / 100) as u32;
    if fill_w > 0 {
        let fill_color = match task.status {
            TaskStatus::Running => colors::with_alpha(colors::ACCENT_BRIGHT, 200),
            TaskStatus::Complete => Pixel::new(130, 190, 150, 200),
            TaskStatus::Failed => Pixel::new(220, 85, 85, 200),
            _ => Pixel::new(140, 132, 126, 160),
        };
        fb.fill_rounded_rect_aa(Rect::new(bar_x, bar_y, fill_w, 4), fill_color, 2);
    }

    let steps_text = alloc::format!("{}/{}", task.steps_done, task.steps_total);
    let steps_w = font_engine::measure_ui_text(&steps_text, 12);
    font_engine::draw_ui_text(
        fb,
        x + w as i32 - steps_w as i32 - 12,
        y + 10,
        &steps_text,
        12,
        Pixel::new(130, 120, 110, 150),
    );
}
