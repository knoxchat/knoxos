use super::text_helpers as fonts;
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::response::Response;
use crate::gui::ui::Ui;
/// Toast — Non-blocking notification popups that auto-dismiss.
///
/// ```ignore
/// // In setup:
/// ToastManager::push(Toast::info("File saved successfully"));
///
/// // In render loop:
/// ToastManager::show_all(ui);
/// ```
use alloc::string::String;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

#[derive(Clone, Copy, PartialEq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastKind {
    fn accent_color(self) -> Pixel {
        match self {
            ToastKind::Info => Pixel::new(59, 130, 246, 220),
            ToastKind::Success => Pixel::new(34, 197, 94, 220),
            ToastKind::Warning => Pixel::new(234, 179, 8, 220),
            ToastKind::Error => Pixel::new(239, 68, 68, 220),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            ToastKind::Info => "i",
            ToastKind::Success => "+",
            ToastKind::Warning => "!",
            ToastKind::Error => "x",
        }
    }
}

#[derive(Clone)]
pub struct Toast {
    pub kind: ToastKind,
    pub message: String,
    pub duration_frames: u32,
    pub remaining: u32,
}

impl Toast {
    pub fn new(kind: ToastKind, msg: &str) -> Self {
        let dur = 180; // ~3 seconds at 60fps
        Self {
            kind,
            message: String::from(msg),
            duration_frames: dur,
            remaining: dur,
        }
    }

    pub fn info(msg: &str) -> Self {
        Self::new(ToastKind::Info, msg)
    }
    pub fn success(msg: &str) -> Self {
        Self::new(ToastKind::Success, msg)
    }
    pub fn warning(msg: &str) -> Self {
        Self::new(ToastKind::Warning, msg)
    }
    pub fn error(msg: &str) -> Self {
        Self::new(ToastKind::Error, msg)
    }

    pub fn duration(mut self, frames: u32) -> Self {
        self.duration_frames = frames;
        self.remaining = frames;
        self
    }
}

lazy_static! {
    static ref TOASTS: Mutex<Vec<Toast>> = Mutex::new(Vec::new());
}

pub struct ToastManager;

impl ToastManager {
    /// Push a new toast notification.
    pub fn push(toast: Toast) {
        let mut toasts = TOASTS.lock();
        toasts.push(toast);
    }

    /// Render all active toasts in the top-right corner.
    pub fn show_all(ui: &mut Ui) {
        let mut toasts = TOASTS.lock();

        // Remove expired
        toasts.retain(|t| t.remaining > 0);

        if toasts.is_empty() {
            return;
        }

        let toast_w = 280u32;
        let toast_h = 40u32;
        let spacing = 6u32;
        let margin = 16i32;

        // Position from top-right
        let screen_w = ui.region.max_rect.x + ui.region.max_rect.width as i32;
        let base_x = screen_w - toast_w as i32 - margin;
        let base_y = ui.region.max_rect.y + margin;

        for (i, toast) in toasts.iter_mut().enumerate() {
            let ty = base_y + (i as u32 * (toast_h + spacing)) as i32;
            let rect = Rect::new(base_x, ty, toast_w, toast_h);

            // Calculate fade
            let alpha = if toast.remaining < 30 {
                toast.remaining as u8 * 8
            } else {
                240u8
            };

            // Background with glassmorphism
            let bg = Pixel::new(
                colors::SURFACE_RAISED.r,
                colors::SURFACE_RAISED.g,
                colors::SURFACE_RAISED.b,
                alpha,
            );
            ui.fb.fill_rounded_rect_aa(rect, bg, 8);

            // Left accent stripe
            let accent = toast.kind.accent_color();
            let accent_with_alpha = Pixel::new(accent.r, accent.g, accent.b, alpha);
            let stripe = Rect::new(base_x, ty, 4, toast_h);
            ui.fb.fill_rounded_rect_aa(stripe, accent_with_alpha, 2);

            // Icon
            let icon_text = toast.kind.icon();
            fonts::draw_string_bold_compact(
                ui.fb,
                icon_text,
                base_x + 12,
                ty + 15,
                accent_with_alpha,
            );

            // Message
            let msg_color = Pixel::new(
                colors::TEXT_PRIMARY.r,
                colors::TEXT_PRIMARY.g,
                colors::TEXT_PRIMARY.b,
                alpha,
            );
            // Truncate message to fit
            let max_chars = ((toast_w - 50) / 8) as usize;
            let display_msg = if toast.message.len() > max_chars {
                &toast.message[..max_chars]
            } else {
                &toast.message
            };
            fonts::draw_string_compact(ui.fb, display_msg, base_x + 28, ty + 15, msg_color);

            // Progress bar at bottom
            let progress_frac = toast.remaining as f32 / toast.duration_frames as f32;
            let progress_w = (toast_w as f32 * progress_frac) as u32;
            let progress_rect = Rect::new(base_x, ty + toast_h as i32 - 2, progress_w, 2);
            ui.fb.fill_rect(progress_rect, accent_with_alpha);

            // Close button
            let close_rect = Rect::new(base_x + toast_w as i32 - 20, ty + 4, 16, 16);
            let close_id = Id::from_str("toast").with_index(i).with("close");
            let close_resp = ui.interact(close_rect, close_id, true, false);
            if close_resp.clicked {
                toast.remaining = 0;
            }
            let xc = if close_resp.hovered {
                colors::TEXT_PRIMARY
            } else {
                Pixel::new(
                    colors::TEXT_MUTED.r,
                    colors::TEXT_MUTED.g,
                    colors::TEXT_MUTED.b,
                    alpha,
                )
            };
            fonts::draw_string_compact(ui.fb, "x", close_rect.x + 4, close_rect.y + 3, xc);

            toast.remaining = toast.remaining.saturating_sub(1);
        }
    }
}
