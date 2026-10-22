/// Window content dispatch — routes drawing to per-app renderers
use crate::gui::framebuffer::FrameBuffer;

use super::types::{ViewKind, Window, WindowContentType};

impl Window {
    // draw() and draw_window_icon() moved to wm_chrome.rs

    /// Draw window-specific content
    pub(crate) fn draw_content(&mut self, fb: &mut FrameBuffer) {
        let content = self.content_rect();

        match self.content_type {
            WindowContentType::TerminalSurface => self.draw_terminal_content(fb, content),
            WindowContentType::ShmEmpty => {
                if let Some((pixels, w, h)) = crate::wayland::window_shm_pixels(self.id) {
                    fb.blit_bgra_scaled(
                        content.x,
                        content.y,
                        content.width,
                        content.height,
                        &pixels,
                        w,
                        h,
                    );
                }
            }
            WindowContentType::Gpui(kind) => present_gpui_view(self, fb, content, kind),
        }
    }
}

fn present_gpui_view(
    win: &mut Window,
    fb: &mut FrameBuffer,
    content: crate::gui::framebuffer::Rect,
    kind: ViewKind,
) {
    match kind {
        ViewKind::Files => win.draw_file_explorer_content(fb, content),
        ViewKind::Browser => win.draw_browser_content(fb, content),
        ViewKind::Chat => {
            crate::gui::ai_assistant::draw_content(fb, win.id, content, win.scroll_y);
        }
        ViewKind::Editor => win.draw_editor_content(fb, content),
        ViewKind::Settings => win.draw_settings_content(fb, content),
        ViewKind::Archive => {
            crate::gui::archive_manager::draw_content(fb, win.id, content, win.scroll_y);
        }
        ViewKind::Disks => {
            crate::gui::disk_utility::draw_content(fb, win.id, content, win.scroll_y);
        }
        ViewKind::Bluetooth => {
            crate::gui::bt_manager::draw_content(fb, win.id, content, win.scroll_y);
        }
        ViewKind::Calendar => {
            crate::gui::calendar_app::draw_content(fb, win.id, content, win.scroll_y);
        }
        ViewKind::Logs => {
            crate::gui::log_viewer::draw_content(fb, win.id, content, win.scroll_y);
        }
        ViewKind::Updater => {
            crate::gui::software_updater::draw_content(fb, win.id, content, win.scroll_y);
        }
        ViewKind::Software => {
            crate::gui::software_center::draw_content(fb, win.id, content, win.scroll_y);
        }
        ViewKind::Wizard => {
            crate::gui::setup_wizard::draw_content(fb, win.id, content, win.scroll_y);
        }
        ViewKind::Tasks => {
            crate::gui::task_manager::draw_content(fb, win.id, content, win.scroll_y);
        }
        ViewKind::Calculator => {
            crate::gui::calculator::draw_content(fb, win.id, content, win.scroll_y);
        }
        ViewKind::Image => {
            crate::gui::image_viewer::draw_content(fb, win.id, content, win.scroll_y);
        }
    }
}
