/// Modal — Overlay dialog with backdrop dimming
///
/// ```ignore
/// if show_modal {
///     Modal::new("confirm_delete")
///         .title("Delete File?")
///         .show(ui, |ui| {
///             ui.label("This action cannot be undone.");
///             ui.horizontal(|ui| {
///                 if ui.button("Cancel").clicked() { show_modal = false; }
///                 if ui.primary_button("Delete").clicked() { do_delete(); show_modal = false; }
///             });
///         });
/// }
/// ```
use alloc::string::String;

use super::text_helpers as fonts;
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::layout::{Align, Layout, Region};
use crate::gui::response::{InnerResponse, Response};
use crate::gui::ui::Ui;

pub struct Modal {
    id: Id,
    title: Option<String>,
    width: u32,
    closable: bool,
    backdrop_color: Pixel,
    bg_color: Pixel,
    corner_radius: u32,
}

impl Modal {
    pub fn new(id_salt: &str) -> Self {
        Self {
            id: Id::from_str(id_salt),
            title: None,
            width: 400,
            closable: true,
            backdrop_color: Pixel::new(0, 0, 0, 150),
            bg_color: Pixel::rgb(24, 26, 34),
            corner_radius: 12,
        }
    }

    pub fn title(mut self, t: &str) -> Self {
        self.title = Some(String::from(t));
        self
    }
    pub fn width(mut self, w: u32) -> Self {
        self.width = w;
        self
    }
    pub fn closable(mut self, v: bool) -> Self {
        self.closable = v;
        self
    }
    pub fn backdrop_color(mut self, c: Pixel) -> Self {
        self.backdrop_color = c;
        self
    }

    pub fn show<'a, R>(
        self,
        ui: &mut Ui<'a>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> ModalResponse<R> {
        let screen = ui.clip_rect;
        let mut dismiss = false;

        // Backdrop
        ui.fb.fill_rect(screen, self.backdrop_color);

        // Click on backdrop to dismiss
        if self.closable && ui.input.pointer_primary_pressed {
            let modal_x = screen.x + (screen.width as i32 - self.width as i32) / 2;
            let modal_y = screen.y + screen.height as i32 / 4;
            // We'll compute the actual rect below, but for backdrop check:
            let rough_rect = Rect::new(modal_x, modal_y, self.width, screen.height / 2);
            if !rough_rect.contains(ui.input.pointer_x, ui.input.pointer_y) {
                dismiss = true;
            }
        }

        // Modal container (centered)
        let modal_x = screen.x + (screen.width as i32 - self.width as i32) / 2;
        let modal_y = screen.y + screen.height as i32 / 4;

        // Shadow
        ui.fb.fill_rounded_rect_aa(
            Rect::new(modal_x + 6, modal_y + 6, self.width, 200),
            Pixel::new(0, 0, 0, 100),
            self.corner_radius + 4,
        );

        // We run content first to determine height, then draw bg behind it.
        // For simplicity, use a fixed initial height and adjust.
        let content_start_y = modal_y + if self.title.is_some() { 40 } else { 16 };
        let content_rect = Rect::new(
            modal_x + 16,
            content_start_y,
            self.width - 32,
            400, // max content height
        );

        let saved = (ui.region, ui.layout, ui.clip_rect, ui.id);

        ui.region = Region::from_max_rect(&Layout::top_down(Align::LEFT), content_rect);
        ui.layout = Layout::top_down(Align::LEFT);
        ui.id = self.id.with("content");

        let inner = add_contents(ui);

        let content_h = (ui.region.cursor_y - content_start_y).max(20) as u32;

        ui.region = saved.0;
        ui.layout = saved.1;
        ui.clip_rect = saved.2;
        ui.id = saved.3;

        let modal_h = content_h + if self.title.is_some() { 40 + 16 } else { 32 };
        let modal_rect = Rect::new(modal_x, modal_y, self.width, modal_h);

        // Draw background (behind content — in real rendering we'd need layering)
        ui.fb
            .fill_rounded_rect_aa(modal_rect, self.bg_color, self.corner_radius);
        ui.fb.draw_rounded_rect(
            modal_rect,
            Pixel::new(80, 160, 255, 60),
            self.corner_radius,
            1,
        );

        // Title
        if let Some(ref title) = self.title {
            fonts::draw_string_bold_compact(
                ui.fb,
                title,
                modal_x + 16,
                modal_y + 12,
                colors::WHITE,
            );
            // Separator
            ui.fb.draw_hline(
                modal_x + 8,
                modal_y + 36,
                self.width - 16,
                Pixel::rgb(50, 55, 65),
            );
        }

        // Close button (X) in top-right
        if self.closable {
            let close_x = modal_x + self.width as i32 - 24;
            let close_y = modal_y + 8;
            let close_rect = Rect::new(close_x, close_y, 16, 16);
            let close_id = self.id.with("close");
            let close_resp = ui.interact(close_rect, close_id, true, false);

            if close_resp.hovered {
                ui.fb
                    .fill_circle_aa(close_x + 8, close_y + 8, 8, Pixel::rgb(200, 60, 70));
            }
            ui.fb.draw_line_aa(
                close_x + 3,
                close_y + 3,
                close_x + 13,
                close_y + 13,
                colors::WHITE,
            );
            ui.fb.draw_line_aa(
                close_x + 3,
                close_y + 13,
                close_x + 13,
                close_y + 3,
                colors::WHITE,
            );

            if close_resp.clicked() {
                dismiss = true;
            }
        }

        ModalResponse {
            inner,
            dismiss,
            modal_rect,
        }
    }
}

pub struct ModalResponse<R> {
    pub inner: R,
    pub dismiss: bool,
    pub modal_rect: Rect,
}
