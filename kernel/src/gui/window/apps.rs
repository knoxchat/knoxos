/// AI assistant, text editor, and settings window content
use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

use super::types::Window;

const ARCH_NAME: &str = if cfg!(target_arch = "x86_64") {
    "x86_64"
} else if cfg!(target_arch = "aarch64") {
    "aarch64"
} else {
    "riscv64"
};

impl Window {
    pub(super) fn draw_ai_content(&mut self, fb: &mut FrameBuffer, content: Rect) {
        // Header bar
        let header_h: u32 = 48;
        fb.fill_rect(
            Rect::new(content.x, content.y, content.width, header_h),
            Pixel::rgb(25, 28, 40),
        );
        fonts::draw_string_bold(
            fb,
            content.x + 16,
            content.y + 14,
            "KnoxOS AI Assistant",
            Pixel::rgb(100, 200, 255),
            2,
        );
        fb.draw_hline(
            content.x,
            content.y + header_h as i32 - 1,
            content.width,
            Pixel::rgb(60, 60, 80),
        );

        // Chat area with scroll support
        let chat_y = content.y + header_h as i32 - self.scroll_y;
        let chat_clip_y = content.y + header_h as i32;
        let input_h: u32 = 50;
        let chat_clip_h = content.height.saturating_sub(header_h + input_h);

        // Chat background
        fb.fill_rect(
            Rect::new(content.x, chat_clip_y, content.width, chat_clip_h),
            colors::WINDOW_BG,
        );

        // AI welcome message bubble
        let msg_y = chat_y + 16;
        let bubble_w = (content.width - 48).min(400);
        if msg_y + 10 > chat_clip_y {
            fb.fill_rounded_rect_aa(
                Rect::new(content.x + 16, msg_y, bubble_w, 80),
                Pixel::rgb(30, 35, 50),
                8,
            );

            // AI avatar indicator
            fb.fill_circle_aa(content.x + 8, msg_y + 8, 4, colors::AI_BALANCED_1);

            let welcome_text = "Hello! I'm the KnoxOS AI Assistant. I can help \
you with system tasks, answer questions, and manage your \
files. What would you like to do today?";

            fonts::draw_text_wrapped(
                fb,
                content.x + 24,
                msg_y + 8,
                bubble_w - 16,
                welcome_text,
                Pixel::rgb(200, 210, 230),
                1,
            );
        }

        // Example user message
        let user_msg_y = msg_y + 100;
        if user_msg_y + 10 > chat_clip_y && user_msg_y < chat_clip_y + chat_clip_h as i32 {
            let user_bubble_w = (content.width - 80).min(300);
            let user_bubble_x = content.x + content.width as i32 - user_bubble_w as i32 - 16;
            fb.fill_rounded_rect_aa(
                Rect::new(user_bubble_x, user_msg_y, user_bubble_w, 36),
                Pixel::rgb(0, 106, 230),
                8,
            );
            fonts::draw_text_wrapped(
                fb,
                user_bubble_x + 12,
                user_msg_y + 8,
                user_bubble_w - 24,
                "Show system information",
                colors::WHITE,
                1,
            );
        }

        // AI response with system info
        let resp_y = user_msg_y + 52;
        if resp_y + 10 > chat_clip_y && resp_y < chat_clip_y + chat_clip_h as i32 {
            let resp_bubble_w = (content.width - 48).min(400);
            fb.fill_rounded_rect_aa(
                Rect::new(content.x + 16, resp_y, resp_bubble_w, 100),
                Pixel::rgb(30, 35, 50),
                8,
            );
            fb.fill_circle_aa(content.x + 8, resp_y + 8, 4, colors::AI_BALANCED_1);
            let info_text = &alloc::format!(
                "System: KnoxOS v0.1.0\n\
CPU: {} (QEMU/KVM)\n\
Memory: 128 MB allocated\n\
Uptime: Running since boot\n\
Kernel: Rust bare-metal",
                ARCH_NAME
            );
            fonts::draw_text_wrapped(
                fb,
                content.x + 24,
                resp_y + 8,
                resp_bubble_w - 16,
                info_text,
                Pixel::rgb(200, 210, 230),
                1,
            );
        }

        // Input bar (bottom)
        let input_y = content.y + content.height as i32 - input_h as i32;
        fb.fill_rect(
            Rect::new(content.x, input_y, content.width, input_h),
            Pixel::rgb(25, 28, 40),
        );
        fb.draw_hline(content.x, input_y, content.width, Pixel::rgb(60, 60, 80));

        fb.fill_rounded_rect_aa(
            Rect::new(content.x + 12, input_y + 10, content.width - 60, 30),
            Pixel::rgb(30, 30, 40),
            6,
        );
        fb.draw_rounded_rect(
            Rect::new(content.x + 12, input_y + 10, content.width - 60, 30),
            Pixel::rgb(60, 60, 80),
            6,
            1,
        );
        fonts::draw_string_compact(
            fb,
            content.x + 20,
            input_y + 19,
            "Ask me anything...",
            Pixel::rgb(100, 100, 120),
            1,
        );

        // Send button
        let send_x = content.x + content.width as i32 - 42;
        fb.fill_rounded_rect_aa(
            Rect::new(send_x, input_y + 10, 30, 30),
            colors::AI_BALANCED_2,
            6,
        );
        fonts::draw_string_centered_bold_compact(
            fb,
            send_x,
            input_y + 10,
            30,
            30,
            "->",
            colors::WHITE,
            1,
        );

        // ── Chat scrollbar ──
        let total_chat_h = resp_y + 100 - (content.y + header_h as i32) + self.scroll_y + 16;
        let visible_chat_h = chat_clip_h as i32;
        self.max_scroll_y = (total_chat_h - visible_chat_h).max(0);
        if total_chat_h > visible_chat_h {
            let sb_w = 6i32;
            let sb_x = content.x + content.width as i32 - sb_w - 2;
            let sb_top = chat_clip_y + 2;
            let sb_track_h = chat_clip_h.saturating_sub(4);
            fb.fill_rounded_rect_aa(
                Rect::new(sb_x, sb_top, sb_w as u32, sb_track_h),
                crate::gui::colors::SCROLLBAR_TRACK,
                (sb_w / 2) as u32,
            );
            let vis_ratio = visible_chat_h as f32 / total_chat_h as f32;
            let thumb_h = ((vis_ratio * sb_track_h as f32) as u32)
                .max(16)
                .min(sb_track_h);
            let max_sc = (total_chat_h - visible_chat_h).max(1);
            let sc_ratio = (self.scroll_y as f32 / max_sc as f32).clamp(0.0, 1.0);
            let track_space = sb_track_h.saturating_sub(thumb_h) as f32;
            let thumb_y = sb_top + (sc_ratio * track_space) as i32;
            fb.fill_rounded_rect_aa(
                Rect::new(sb_x, thumb_y, sb_w as u32, thumb_h),
                crate::gui::colors::SCROLLBAR_THUMB,
                (sb_w / 2) as u32,
            );
        }
    }

    pub(super) fn draw_editor_content(&mut self, fb: &mut FrameBuffer, content: Rect) {
        let total_content_h = crate::gui::editor::draw(fb, content, self.scroll_y, self.id);
        let line_h = 14i32;
        let status_h = 22i32;
        let visible_h = content.height as i32 - status_h;
        self.max_scroll_y = (total_content_h - visible_h).max(0);

        // ── Editor scrollbar ──
        if total_content_h > visible_h {
            let sb_w = 6i32;
            let minimap_offset = if content.width > 400 { 50i32 } else { 0 };
            let sb_x = content.x + content.width as i32 - minimap_offset - sb_w - 2;
            let sb_top = content.y + 2;
            let sb_track_h = (visible_h - 4).max(1) as u32;
            fb.fill_rounded_rect_aa(
                Rect::new(sb_x, sb_top, sb_w as u32, sb_track_h),
                crate::gui::colors::SCROLLBAR_TRACK,
                (sb_w / 2) as u32,
            );
            let vis_ratio = visible_h as f32 / total_content_h as f32;
            let thumb_h = ((vis_ratio * sb_track_h as f32) as u32)
                .max(16)
                .min(sb_track_h);
            let max_sc = (total_content_h - visible_h).max(1);
            let sc_ratio = (self.scroll_y as f32 / max_sc as f32).clamp(0.0, 1.0);
            let track_space = sb_track_h.saturating_sub(thumb_h) as f32;
            let thumb_y = sb_top + (sc_ratio * track_space) as i32;
            fb.fill_rounded_rect_aa(
                Rect::new(sb_x, thumb_y, sb_w as u32, thumb_h),
                crate::gui::colors::SCROLLBAR_THUMB,
                (sb_w / 2) as u32,
            );
        }
    }

    pub(super) fn draw_settings_content(&mut self, fb: &mut FrameBuffer, content: Rect) {
        // Use the global shared settings state
        let state = crate::gui::settings::SETTINGS_STATE.lock();
        let total_h = crate::gui::settings::draw_settings(fb, content, &state, self.scroll_y);
        // Let the window know the virtual content height for scroll clamping
        let visible_h = content.height as i32;
        self.max_scroll_y = (total_h - visible_h).max(0);
    }
}
