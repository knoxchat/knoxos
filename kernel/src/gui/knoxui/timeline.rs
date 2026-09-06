use super::text_helpers as fonts;
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::response::Response;
use crate::gui::ui::Ui;
/// Timeline — Vertical timeline widget for events/history display.
///
/// ```ignore
/// Timeline::new("history")
///     .event("v0.1", "Initial release", TimelineKind::Success)
///     .event("v0.2", "Added GUI", TimelineKind::Info)
///     .show(ui);
/// ```
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Copy)]
pub enum TimelineKind {
    Default,
    Success,
    Warning,
    Error,
    Info,
}

impl TimelineKind {
    fn color(self) -> Pixel {
        match self {
            TimelineKind::Default => colors::TEXT_MUTED,
            TimelineKind::Success => Pixel::new(34, 197, 94, 220),
            TimelineKind::Warning => Pixel::new(234, 179, 8, 220),
            TimelineKind::Error => Pixel::new(239, 68, 68, 220),
            TimelineKind::Info => Pixel::new(59, 130, 246, 220),
        }
    }
}

struct TimelineEvent {
    title: String,
    description: String,
    kind: TimelineKind,
}

pub struct Timeline {
    id: Id,
    events: Vec<TimelineEvent>,
    dot_radius: u32,
    line_x_offset: u32,
    event_height: u32,
}

impl Timeline {
    pub fn new(id_salt: &str) -> Self {
        Self {
            id: Id::from_str(id_salt),
            events: Vec::new(),
            dot_radius: 5,
            line_x_offset: 20,
            event_height: 48,
        }
    }

    pub fn event(mut self, title: &str, description: &str, kind: TimelineKind) -> Self {
        self.events.push(TimelineEvent {
            title: String::from(title),
            description: String::from(description),
            kind,
        });
        self
    }

    pub fn dot_radius(mut self, r: u32) -> Self {
        self.dot_radius = r;
        self
    }
    pub fn event_height(mut self, h: u32) -> Self {
        self.event_height = h;
        self
    }

    pub fn show(self, ui: &mut Ui) {
        if self.events.is_empty() {
            return;
        }

        let avail_w = ui.available_width().max(0) as u32;
        let total_h = self.events.len() as u32 * self.event_height;
        let outer = ui.allocate_space(avail_w, total_h);

        let line_x = outer.x + self.line_x_offset as i32;
        let text_x = line_x + self.dot_radius as i32 * 2 + 10;

        // Vertical line
        ui.fb.draw_line_aa(
            line_x,
            outer.y + self.dot_radius as i32,
            line_x,
            outer.y + total_h as i32 - self.dot_radius as i32,
            Pixel::new(255, 255, 255, 30),
        );

        for (i, event) in self.events.iter().enumerate() {
            let ey = outer.y + (i as u32 * self.event_height) as i32;
            let dot_cy = ey + self.event_height as i32 / 2;
            let color = event.kind.color();

            // Dot
            ui.fb.fill_circle_aa(line_x, dot_cy, self.dot_radius, color);

            // Glow ring
            let ring_color = Pixel::new(color.r, color.g, color.b, 60);
            let ring_r = self.dot_radius + 2;
            // Simple ring: draw outer circle, the inner will be covered
            ui.fb.fill_circle_aa(line_x, dot_cy, ring_r, ring_color);
            ui.fb.fill_circle_aa(line_x, dot_cy, self.dot_radius, color);

            // Title
            fonts::draw_string_bold_compact(
                ui.fb,
                &event.title,
                text_x,
                dot_cy - 10,
                colors::TEXT_PRIMARY,
            );

            // Description
            fonts::draw_string_compact(
                ui.fb,
                &event.description,
                text_x,
                dot_cy + 4,
                colors::TEXT_MUTED,
            );

            // Hover interaction
            let event_rect = Rect::new(outer.x, ey, avail_w, self.event_height);
            let event_id = self.id.with_index(i);
            let resp = ui.interact(event_rect, event_id, true, false);

            if resp.hovered {
                ui.fb.fill_rect(event_rect, Pixel::new(255, 255, 255, 5));
            }
        }
    }
}
