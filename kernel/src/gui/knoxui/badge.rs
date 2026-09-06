use super::text_helpers as fonts;
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::response::Response;
use crate::gui::ui::Ui;
/// Badge — Small label/tag component for status indicators, counts, etc.
///
/// ```ignore
/// Badge::new("3").color(colors::ACCENT_PRIMARY).show(ui);
/// Badge::status("Online", BadgeKind::Success).show(ui);
/// ```
use alloc::string::String;

#[derive(Clone, Copy)]
pub enum BadgeKind {
    Default,
    Success,
    Warning,
    Error,
    Info,
}

pub struct Badge {
    text: String,
    kind: BadgeKind,
    custom_color: Option<Pixel>,
    pill: bool,
}

impl Badge {
    pub fn new(text: &str) -> Self {
        Self {
            text: String::from(text),
            kind: BadgeKind::Default,
            custom_color: None,
            pill: true,
        }
    }

    pub fn status(text: &str, kind: BadgeKind) -> Self {
        Self {
            text: String::from(text),
            kind,
            custom_color: None,
            pill: true,
        }
    }

    pub fn color(mut self, c: Pixel) -> Self {
        self.custom_color = Some(c);
        self
    }
    pub fn pill(mut self, p: bool) -> Self {
        self.pill = p;
        self
    }
    pub fn kind(mut self, k: BadgeKind) -> Self {
        self.kind = k;
        self
    }

    fn bg_color(&self) -> Pixel {
        if let Some(c) = self.custom_color {
            return c;
        }
        match self.kind {
            BadgeKind::Default => colors::SURFACE_RAISED,
            BadgeKind::Success => Pixel::new(34, 197, 94, 180),
            BadgeKind::Warning => Pixel::new(234, 179, 8, 180),
            BadgeKind::Error => Pixel::new(239, 68, 68, 180),
            BadgeKind::Info => Pixel::new(59, 130, 246, 180),
        }
    }

    fn text_color(&self) -> Pixel {
        match self.kind {
            BadgeKind::Default => colors::TEXT_SECONDARY,
            _ => Pixel::new(255, 255, 255, 240),
        }
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let text_w = (self.text.len() as u32) * 8;
        let pad_h = 6u32;
        let pad_v = 3u32;
        let w = text_w + pad_h * 2;
        let h = 10 + pad_v * 2;

        let rect = ui.allocate_space(w, h);
        let id = ui.id.with("badge");

        let bg = self.bg_color();
        let corner = if self.pill { h / 2 } else { 4 };

        ui.fb.fill_rounded_rect_aa(rect, bg, corner);
        fonts::draw_string_centered_compact(
            ui.fb,
            &self.text,
            rect.x + w as i32 / 2,
            rect.y + pad_v as i32 + 1,
            self.text_color(),
        );

        ui.interact(rect, id, true, false)
    }
}
