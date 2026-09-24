/// Window content dispatch — routes drawing to per-app renderers
use crate::gui::framebuffer::FrameBuffer;

use super::types::{Window, WindowContentType};

impl Window {
    // draw() and draw_window_icon() moved to wm_chrome.rs

    /// Draw window-specific content
    pub(crate) fn draw_content(&mut self, fb: &mut FrameBuffer) {
        let content = self.content_rect();

        match self.content_type {
            WindowContentType::Terminal => self.draw_terminal_content(fb, content),
            WindowContentType::FileExplorer => self.draw_file_explorer_content(fb, content),
            WindowContentType::Browser => self.draw_browser_content(fb, content),
            WindowContentType::AIAssistant => {
                crate::gui::ai_assistant::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::TextEditor => self.draw_editor_content(fb, content),
            WindowContentType::Settings => self.draw_settings_content(fb, content),
            WindowContentType::ArchiveViewer => {
                crate::gui::archive_manager::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::DiskUtility => {
                crate::gui::disk_utility::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::BluetoothManager => {
                crate::gui::bt_manager::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::CalendarApp => {
                crate::gui::calendar_app::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::LogViewer => {
                crate::gui::log_viewer::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::SoftwareUpdater => {
                crate::gui::software_updater::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::SoftwareCenter => {
                crate::gui::software_center::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::SetupWizard => {
                crate::gui::setup_wizard::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::TaskManager => {
                crate::gui::task_manager::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::Calculator => {
                crate::gui::calculator::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::ImageViewer => {
                crate::gui::image_viewer::draw_content(fb, self.id, content, self.scroll_y);
            }
            WindowContentType::Empty => {
                if let Some((pixels, w, h)) = crate::wayland::window_shm_pixels(self.id) {
                    fb.blit_bgra(content.x, content.y, w, h, &pixels);
                }
            }
        }
    }
}
