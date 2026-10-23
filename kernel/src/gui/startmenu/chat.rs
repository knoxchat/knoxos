//! AI-tab quick actions, chat bubbles, tool-call cards, and the send bar.

use alloc::string::String;

use crate::gui::colors;
use crate::gui::font_engine;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

use super::layout::{
    AI_BUBBLE_MAX_W_PCT, AI_INPUT_H, PAD_H, QUICK_ACTION_COLS, QUICK_ACTION_GAP, QUICK_ACTION_SIZE,
};
use super::state::StartMenu;
use super::types::{ChatMessage, ChatRole, ToolCallResult, ToolCallStatus};

// ═══════════════════════════════════════════════════════════════════════════
// AI CONTENT — Chat interface with tool calls, quick actions, voice
// ═══════════════════════════════════════════════════════════════════════════

pub(super) fn draw_ai_content(
    fb: &mut FrameBuffer,
    panel_x: i32,
    content_y: i32,
    panel_w: u32,
    content_h: u32,
    menu: &StartMenu,
) {
    let content_w = panel_w as i32 - PAD_H * 2;

    // ── Quick Actions Row ──
    let qa_y = content_y + 4;
    let qa_total_w =
        QUICK_ACTION_COLS * QUICK_ACTION_SIZE + (QUICK_ACTION_COLS - 1) * QUICK_ACTION_GAP;
    let qa_start_x = panel_x + (panel_w as i32 - qa_total_w as i32) / 2;

    for (i, action) in menu.quick_actions.iter().enumerate() {
        let col = i as u32;
        let ax = qa_start_x + (col * (QUICK_ACTION_SIZE + QUICK_ACTION_GAP)) as i32;
        let ay = qa_y;

        let bg_color = if action.enabled {
            Pixel::new(
                action.color.r / 4,
                action.color.g / 4,
                action.color.b / 4,
                120,
            )
        } else {
            Pixel::new(16, 20, 32, 100)
        };
        let border_color = if action.enabled {
            Pixel::new(action.color.r, action.color.g, action.color.b, 80)
        } else {
            Pixel::new(40, 60, 100, 40)
        };

        fb.fill_rounded_rect_aa(
            Rect::new(ax, ay, QUICK_ACTION_SIZE, QUICK_ACTION_SIZE),
            bg_color,
            12,
        );
        fb.draw_rounded_rect(
            Rect::new(ax, ay, QUICK_ACTION_SIZE, QUICK_ACTION_SIZE),
            border_color,
            12,
            1,
        );

        let glyph_color = if action.enabled {
            action.color
        } else {
            Pixel::new(80, 100, 140, 140)
        };
        let glyph_str: String = alloc::format!("{}", action.icon_glyph);
        {
            let _tw = font_engine::measure_ui_text(&glyph_str, 14) as i32;
            font_engine::draw_ui_bold(
                fb,
                ax + QUICK_ACTION_SIZE as i32 / 2 - _tw / 2,
                ay + 4,
                &glyph_str,
                14,
                glyph_color,
            );
        }

        let label_color = if action.enabled {
            Pixel::new(180, 200, 230, 200)
        } else {
            Pixel::new(70, 90, 130, 130)
        };
        {
            let _tw = font_engine::measure_ui_text(&action.name, 11) as i32;
            font_engine::draw_ui_text(
                fb,
                ax + QUICK_ACTION_SIZE as i32 / 2 - _tw / 2,
                ay + QUICK_ACTION_SIZE as i32 - 14,
                &action.name,
                11,
                label_color,
            );
        }
    }

    // Divider
    let div_y = qa_y + QUICK_ACTION_SIZE as i32 + 10;
    fb.draw_hline(
        panel_x + PAD_H,
        div_y,
        (panel_w as i32 - PAD_H * 2).max(0) as u32,
        Pixel::new(50, 80, 140, 25),
    );

    // ── Chat Messages Area ──
    let chat_y = div_y + 8;
    let chat_h = content_h as i32 - (chat_y - content_y) - AI_INPUT_H as i32 - 8;

    fb.push_clip(Rect::new(
        panel_x + PAD_H,
        chat_y,
        content_w as u32,
        chat_h.max(0) as u32,
    ));

    let mut msg_y = chat_y + 4;
    for msg in &menu.chat_messages {
        msg_y = draw_chat_message(
            fb,
            panel_x + PAD_H,
            msg_y,
            content_w,
            msg,
            chat_y,
            chat_y + chat_h,
        );
        msg_y += 8;
    }

    // Thinking indicator
    if menu.ai_is_thinking {
        let thinking_y = msg_y + 4;
        if thinking_y < chat_y + chat_h {
            fb.fill_circle_aa(
                panel_x + PAD_H + 8,
                thinking_y + 8,
                5,
                colors::AI_BALANCED_1,
            );
            for dot in 0..3 {
                let dx = panel_x + PAD_H + 22 + dot * 10;
                let alpha = 120u8 + ((dot as u8 * 40) % 135);
                fb.fill_circle_aa(
                    dx,
                    thinking_y + 8,
                    3,
                    colors::with_alpha(colors::ACCENT_BRIGHT, alpha),
                );
            }
        }
    }

    fb.pop_clip();

    // Suggested Prompts
    if menu.chat_messages.len() <= 2 && !menu.ai_is_thinking {
        let suggest_y = chat_y + chat_h - (menu.suggested_prompts.len() as i32 * 24 + 8);
        if suggest_y > chat_y {
            for (i, prompt) in menu.suggested_prompts.iter().enumerate() {
                let sy = suggest_y + i as i32 * 24;
                let prompt_w = font_engine::measure_ui_text(prompt, 12) + 16;
                let sx = panel_x + PAD_H + 4;

                fb.fill_rounded_rect_aa(
                    Rect::new(sx, sy, prompt_w, 20),
                    Pixel::new(36, 32, 40, 120),
                    8,
                );
                fb.draw_rounded_rect(
                    Rect::new(sx, sy, prompt_w, 20),
                    Pixel::new(140, 110, 100, 50),
                    8,
                    1,
                );
                font_engine::draw_ui_text(
                    fb,
                    sx + 8,
                    sy + 4,
                    prompt,
                    12,
                    Pixel::new(190, 160, 140, 180),
                );
            }
        }
    }

    // ── AI Chat Input Bar ──
    let input_y = chat_y + chat_h + 4;
    let input_x = panel_x + PAD_H;
    let input_w = content_w - 40;

    fb.fill_rounded_rect_aa(
        Rect::new(input_x, input_y, input_w as u32, AI_INPUT_H),
        Pixel::new(26, 24, 32, 200),
        10,
    );
    fb.draw_rounded_rect(
        Rect::new(input_x, input_y, input_w as u32, AI_INPUT_H),
        Pixel::new(130, 100, 90, 40),
        10,
        1,
    );

    let chat_text_x = input_x + 12;
    let chat_text_y = input_y + (AI_INPUT_H as i32 - 12) / 2;
    if !menu.chat_input.is_empty() {
        font_engine::draw_ui_text(
            fb,
            chat_text_x,
            chat_text_y,
            &menu.chat_input,
            12,
            Pixel::new(235, 225, 215, 240),
        );
        let cursor_x = chat_text_x + font_engine::measure_ui_text(&menu.chat_input, 12) as i32 + 1;
        fb.fill_rect(
            Rect::new(cursor_x, chat_text_y, 2, 12),
            Pixel::new(180, 140, 200, 200),
        );
    } else {
        font_engine::draw_ui_text(
            fb,
            chat_text_x,
            chat_text_y,
            "Type a message or command...",
            12,
            Pixel::new(100, 90, 80, 110),
        );
    }

    // Send button
    let send_x = input_x + input_w + 6;
    let send_size = AI_INPUT_H - 8;
    fb.fill_rounded_rect_aa(
        Rect::new(send_x, input_y + 4, send_size, send_size),
        colors::with_alpha(colors::ACCENT_PRIMARY, 180),
        8,
    );
    let arrow_cx = send_x + send_size as i32 / 2;
    let arrow_cy = input_y + AI_INPUT_H as i32 / 2;
    fb.draw_line_aa(
        arrow_cx - 4,
        arrow_cy,
        arrow_cx + 4,
        arrow_cy,
        colors::WHITE,
    );
    fb.draw_line_aa(
        arrow_cx + 1,
        arrow_cy - 4,
        arrow_cx + 4,
        arrow_cy,
        colors::WHITE,
    );
    fb.draw_line_aa(
        arrow_cx + 1,
        arrow_cy + 4,
        arrow_cx + 4,
        arrow_cy,
        colors::WHITE,
    );
}

/// Draw a single chat message bubble, returns the Y coordinate after this message
fn draw_chat_message(
    fb: &mut FrameBuffer,
    area_x: i32,
    msg_y: i32,
    area_w: i32,
    msg: &ChatMessage,
    clip_top: i32,
    clip_bottom: i32,
) -> i32 {
    let max_bubble_w = ((area_w as u32 * AI_BUBBLE_MAX_W_PCT) / 100) as i32;
    let is_user = msg.role == ChatRole::User;

    let chars_per_line = (max_bubble_w - 24) / 8;
    let text_lines = if chars_per_line > 0 {
        ((msg.content.len() as i32 + chars_per_line - 1) / chars_per_line).max(1)
    } else {
        1
    };
    let text_h = text_lines * 14 + 4;

    let tool_h: i32 = msg
        .tool_calls
        .iter()
        .map(|tc| {
            let output_lines = tc.output.lines().count() as i32;
            36 + (output_lines.max(1) * 14)
        })
        .sum();

    let bubble_h = (text_h + tool_h + 16).max(28) as u32;
    let bubble_w = max_bubble_w.min(area_w - 16) as u32;

    if msg_y + bubble_h as i32 + 4 < clip_top || msg_y > clip_bottom {
        return msg_y + bubble_h as i32;
    }

    let bubble_x = if is_user {
        area_x + area_w - bubble_w as i32 - 4
    } else {
        area_x + 4
    };

    // Avatar dot
    if !is_user && msg_y + 8 >= clip_top {
        let avatar_color = match msg.role {
            ChatRole::Assistant => colors::AI_BALANCED_1,
            ChatRole::System => Pixel::new(255, 200, 0, 200),
            ChatRole::ToolResult => Pixel::new(0, 200, 140, 200),
            _ => Pixel::new(100, 100, 100, 200),
        };
        fb.fill_circle_aa(area_x, msg_y + 10, 4, avatar_color);
    }

    // Bubble background
    let bubble_bg = if is_user {
        Pixel::new(0, 100, 220, 160)
    } else {
        Pixel::new(24, 30, 48, 200)
    };
    fb.fill_rounded_rect_aa(
        Rect::new(bubble_x, msg_y, bubble_w, bubble_h),
        bubble_bg,
        10,
    );

    if !is_user {
        fb.draw_rounded_rect(
            Rect::new(bubble_x, msg_y, bubble_w, bubble_h),
            Pixel::new(50, 80, 140, 35),
            10,
            1,
        );
    }

    // Message text
    let text_color = if is_user {
        colors::WHITE
    } else {
        Pixel::new(200, 215, 240, 240)
    };
    fonts::draw_text_wrapped_compact(
        fb,
        bubble_x + 12,
        msg_y + 8,
        bubble_w - 24,
        &msg.content,
        text_color,
        1,
    );

    // Tool call results
    let mut tc_y = msg_y + text_h + 12;
    for tc in &msg.tool_calls {
        if tc_y > clip_bottom {
            break;
        }
        tc_y = draw_tool_call(fb, bubble_x + 8, tc_y, bubble_w - 16, tc);
        tc_y += 4;
    }

    msg_y + bubble_h as i32
}

/// Draw a tool call result card inside a chat bubble
fn draw_tool_call(fb: &mut FrameBuffer, x: i32, y: i32, w: u32, tc: &ToolCallResult) -> i32 {
    let output_lines = tc.output.lines().count() as i32;
    let card_h = (28 + output_lines.max(1) * 14) as u32;

    fb.fill_rounded_rect_aa(Rect::new(x, y, w, card_h), Pixel::new(10, 14, 26, 180), 6);

    let status_color = match tc.status {
        ToolCallStatus::Running => Pixel::new(255, 200, 0, 220),
        ToolCallStatus::Success => Pixel::new(0, 220, 140, 220),
        ToolCallStatus::Error => Pixel::new(255, 80, 80, 220),
    };
    fb.fill_circle_aa(x + 10, y + 10, 3, status_color);

    let name_str = alloc::format!("{}: {}", tc.tool_name, tc.description);
    font_engine::draw_ui_text(
        fb,
        x + 20,
        y + 4,
        &name_str,
        12,
        Pixel::new(100, 160, 230, 200),
    );

    let mut out_y = y + 22;
    for line in tc.output.lines() {
        font_engine::draw_ui_text(fb, x + 12, out_y, line, 12, Pixel::new(160, 180, 210, 190));
        out_y += 14;
    }

    y + card_h as i32
}
