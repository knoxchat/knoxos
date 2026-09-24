use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use alloc::string::String;
use alloc::vec::Vec;

/// Context menu (right-click menu)
pub struct ContextMenu {
    pub rect: Rect,
    pub items: Vec<MenuItem>,
    pub visible: bool,
    pub hovered_index: Option<usize>,
}

pub struct MenuItem {
    pub label: String,
    pub separator: bool,
    pub enabled: bool,
}

impl ContextMenu {
    pub fn new(x: i32, y: i32, items: Vec<MenuItem>) -> Self {
        let width = 200u32;
        let height = items.len() as u32 * 24 + 8;
        Self {
            rect: Rect::new(x, y, width, height),
            items,
            visible: false,
            hovered_index: None,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        if !self.visible {
            return;
        }

        // Background with soft AA shadow
        fb.fill_rounded_rect_aa(
            Rect::new(
                self.rect.x + 2,
                self.rect.y + 2,
                self.rect.width,
                self.rect.height,
            ),
            Pixel::new(0, 0, 0, 100),
            8,
        );
        fb.fill_rounded_rect_aa(self.rect, Pixel::rgb(40, 40, 40), 8);
        fb.draw_rounded_rect(self.rect, Pixel::rgb(70, 70, 70), 8, 1);

        // Items
        for (i, item) in self.items.iter().enumerate() {
            let item_y = self.rect.y + 4 + (i as i32 * 24);

            if item.separator {
                fb.draw_hline(
                    self.rect.x + 8,
                    item_y + 12,
                    self.rect.width - 16,
                    Pixel::rgb(70, 70, 70),
                );
                continue;
            }

            // Hover highlight (AA rounded)
            if self.hovered_index == Some(i) {
                fb.fill_rounded_rect_aa(
                    Rect::new(self.rect.x + 2, item_y, self.rect.width - 4, 24),
                    colors::SELECTION,
                    4,
                );
            }

            let text_color = if item.enabled {
                colors::WHITE
            } else {
                Pixel::rgb(100, 100, 100)
            };

            fonts::draw_string_compact(
                fb,
                self.rect.x + 12,
                item_y + 6,
                &item.label,
                text_color,
                1,
            );
        }
    }
}
