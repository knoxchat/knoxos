/// DragHandle — A draggable grip indicator for reorderable lists, panels, etc.
///
/// ```ignore
/// let resp = DragHandle::new().show(ui);
/// if resp.dragged { /* reorder logic */ }
/// ```
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::response::Response;
use crate::gui::ui::Ui;

pub struct DragHandle {
    width: u32,
    height: u32,
    color: Pixel,
    dots: bool,
}

impl DragHandle {
    pub fn new() -> Self {
        Self {
            width: 14,
            height: 20,
            color: colors::TEXT_MUTED,
            dots: true,
        }
    }

    pub fn size(mut self, w: u32, h: u32) -> Self {
        self.width = w;
        self.height = h;
        self
    }

    pub fn color(mut self, c: Pixel) -> Self {
        self.color = c;
        self
    }

    /// Use horizontal lines instead of dots
    pub fn lines(mut self) -> Self {
        self.dots = false;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let rect = ui.allocate_space(self.width, self.height);
        let id = ui.id.with("drag_handle");

        let resp = ui.interact(rect, id, true, true);
        let color = if resp.hovered || resp.dragged {
            colors::TEXT_SECONDARY
        } else {
            self.color
        };

        if self.dots {
            // Draw 6 dots in 2 columns × 3 rows
            let col_spacing = 5i32;
            let row_spacing = 5i32;
            let cx = rect.x + self.width as i32 / 2;
            let cy = rect.y + self.height as i32 / 2;

            for row in -1i32..=1 {
                for col in -1i32..=0 {
                    let dx = cx + col * col_spacing + col_spacing / 2;
                    let dy = cy + row * row_spacing;
                    ui.fb.fill_circle_aa(dx, dy, 2, color);
                }
            }
        } else {
            // Draw 3 horizontal lines
            let cx = rect.x + 2;
            let line_w = self.width as i32 - 4;
            let cy = rect.y + self.height as i32 / 2;

            for i in -1i32..=1 {
                let y = cy + i * 4;
                ui.fb.draw_line_aa(cx, y, cx + line_w, y, color);
            }
        }

        resp
    }
}
