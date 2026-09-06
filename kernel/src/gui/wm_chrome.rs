/// Window chrome rendering — title bars, buttons, decorations, shadows
/// Uses font_engine for title text and icon_theme for window icons.
/// Extracted from window.rs for modularity.
use super::colors;
use super::font_engine;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::icon_theme::{self, IconCategory};
use super::scale;
use super::window::*;

/// Map WindowContentType to an icon theme (category, name) pair for the title bar icon.
fn content_type_icon(ct: WindowContentType) -> Option<(IconCategory, &'static str)> {
    match ct {
        WindowContentType::Terminal => Some((IconCategory::Apps, "terminal-1")),
        WindowContentType::FileExplorer => Some((IconCategory::Places, "folder")),
        WindowContentType::Browser => Some((IconCategory::Apps, "internet-web-browser")),
        WindowContentType::AIAssistant => Some((IconCategory::Apps, "ai-assistant")),
        WindowContentType::TextEditor => Some((IconCategory::Apps, "accessories-text-editor")),
        WindowContentType::Settings => Some((IconCategory::Apps, "org.gnome.Settings")),
        WindowContentType::ArchiveViewer => Some((IconCategory::Apps, "archive-manager")),
        WindowContentType::DiskUtility => Some((IconCategory::Apps, "disk-utility")),
        WindowContentType::BluetoothManager => Some((IconCategory::Apps, "bluetooth")),
        WindowContentType::CalendarApp => Some((IconCategory::Apps, "office-calendar")),
        WindowContentType::LogViewer => Some((IconCategory::Apps, "accessories-text-editor")),
        WindowContentType::SoftwareUpdater => Some((IconCategory::Apps, "software-center")),
        WindowContentType::SoftwareCenter => Some((IconCategory::Apps, "software-center")),
        WindowContentType::SetupWizard => Some((IconCategory::Apps, "org.gnome.Settings")),
        WindowContentType::TaskManager => Some((IconCategory::Apps, "org.gnome.SystemMonitor")),
        WindowContentType::Calculator => Some((IconCategory::Apps, "accessories-calculator")),
        WindowContentType::ImageViewer => Some((IconCategory::Apps, "accessories-image-viewer")),
        WindowContentType::Empty => None,
    }
}

impl Window {
    /// Draw a small window icon (16×16) using the icon theme system.
    /// Falls back to hardcoded procedural icons if no theme icon is available.
    fn draw_window_icon(&self, fb: &mut FrameBuffer, x: i32, y: i32) {
        // Try icon theme first
        if let Some((category, name)) = content_type_icon(self.content_type) {
            icon_theme::draw_tiny_icon(fb, x, y, category, name);
            return;
        }

        // Fallback: generic window icon
        let icon_color = if self.focused {
            colors::WINDOW_TITLE_TEXT
        } else {
            colors::WINDOW_BUTTON_INACTIVE
        };
        fb.draw_rounded_rect(Rect::new(x + 1, y + 1, 14, 14), icon_color, 3, 1);
    }

    /// Draw the window on the framebuffer — Aurora design
    /// Warm frosted glass with soft edges, ambient glow, and pill controls
    pub fn draw(&mut self, fb: &mut FrameBuffer) {
        if !self.is_visible() {
            return;
        }

        // ── Animation: temporarily override rect for animated positioning ──
        let (anim_rect, anim_opacity) = self.animated_rect_opacity();
        let original_rect = self.rect;
        let is_anim = self.anim.is_some();
        if is_anim {
            self.rect = anim_rect;
        }

        // Skip drawing if fully transparent (animation faded out)
        if anim_opacity <= 0.01 {
            self.rect = original_rect;
            return;
        }

        let title_bg = if self.focused {
            if is_anim && anim_opacity < 1.0 {
                let mut c = colors::title_bg(true);
                c.a = (c.a as f32 * anim_opacity) as u8;
                c
            } else {
                colors::title_bg(true)
            }
        } else {
            if is_anim && anim_opacity < 1.0 {
                let mut c = colors::title_bg(false);
                c.a = (c.a as f32 * anim_opacity) as u8;
                c
            } else {
                colors::title_bg(false)
            }
        };

        let title_text_color = if self.focused {
            colors::text_primary()
        } else {
            if crate::gui::accessibility::is_high_contrast() {
                Pixel::rgb(200, 200, 200)
            } else {
                colors::WINDOW_TITLE_TEXT_INACTIVE
            }
        };

        let border_color = if self.focused {
            colors::border_active()
        } else {
            if crate::gui::accessibility::is_high_contrast() {
                Pixel::rgb(180, 180, 0)
            } else {
                colors::WINDOW_BORDER_INACTIVE
            }
        };

        // ══════════════════════════════════════════════════════════
        // LIGHTWEIGHT SHADOW SYSTEM
        // ══════════════════════════════════════════════════════════
        if self.state == WindowState::Normal {
            if self.focused {
                for offset in (0i32..16).step_by(4) {
                    let alpha = (16 - offset).min(14) as u8;
                    let shadow_rect = Rect::new(
                        self.rect.x - offset / 2,
                        self.rect.y + 6 + offset / 2,
                        self.rect.width + offset as u32,
                        self.rect.height + offset as u32 / 2,
                    );
                    fb.draw_rounded_rect(
                        shadow_rect,
                        Pixel::new(0, 0, 0, alpha),
                        scaled_portal_radius() + offset as u32 / 3,
                        1,
                    );
                }
                for offset in 0i32..3 {
                    let alpha = ((3 - offset) * 10).min(30) as u8;
                    let shadow_rect = Rect::new(
                        self.rect.x - offset,
                        self.rect.y + 2 + offset,
                        self.rect.width + offset as u32 * 2,
                        self.rect.height + offset as u32,
                    );
                    fb.draw_rounded_rect(
                        shadow_rect,
                        Pixel::new(40, 20, 15, alpha),
                        scaled_portal_radius() + 1,
                        1,
                    );
                }
            } else {
                for offset in (0i32..10).step_by(4) {
                    let alpha = (8 - offset).clamp(0, 8) as u8;
                    let shadow_rect = Rect::new(
                        self.rect.x - offset / 2,
                        self.rect.y + 4 + offset / 2,
                        self.rect.width + offset as u32,
                        self.rect.height + offset as u32 / 2,
                    );
                    fb.draw_rounded_rect(
                        shadow_rect,
                        Pixel::new(0, 0, 0, alpha),
                        scaled_portal_radius() + offset as u32 / 3,
                        1,
                    );
                }
            }
        }

        // ══════════════════════════════════════════════════════════
        // WINDOW BODY — Glassmorphism with depth gradient
        // ══════════════════════════════════════════════════════════
        let r = if self.state == WindowState::Normal {
            scaled_portal_radius()
        } else {
            0
        };

        let bg_top = if self.focused {
            Pixel::new(32, 30, 38, 250)
        } else {
            Pixel::new(26, 24, 30, 235)
        };
        let bg_bottom = if self.focused {
            Pixel::new(24, 22, 28, 248)
        } else {
            Pixel::new(22, 20, 26, 230)
        };
        fb.fill_rounded_rect_gradient_aa(self.rect, bg_top, bg_bottom, r);

        if self.focused {
            let inner_glow_rect = Rect::new(
                self.rect.x + 1,
                self.rect.y + 1,
                self.rect.width - 2,
                self.rect.height - 2,
            );
            fb.draw_hline(
                self.rect.x + 2,
                self.rect.y + 1,
                self.rect.width.saturating_sub(4),
                Pixel::new(200, 150, 130, 22),
            );
            fb.draw_rounded_rect(
                inner_glow_rect,
                Pixel::new(200, 140, 120, 12),
                r.saturating_sub(1),
                1,
            );
        }

        // ── Border ──
        if self.focused {
            fb.draw_hline(
                self.rect.x + r as i32,
                self.rect.y,
                self.rect.width - r * 2,
                Pixel::new(232, 121, 100, 70),
            );
            fb.draw_rounded_rect(self.rect, Pixel::new(180, 120, 100, 40), r, 1);
        } else {
            fb.draw_rounded_rect(self.rect, border_color, r, 1);
        }

        // ══════════════════════════════════════════════════════════
        // TITLE BAR — Frosted glass with depth gradient
        // ══════════════════════════════════════════════════════════
        let title_top = if self.focused {
            Pixel::new(28, 26, 34, 240)
        } else {
            Pixel::new(24, 22, 28, 210)
        };
        let title_bottom = if self.focused {
            Pixel::new(22, 20, 28, 230)
        } else {
            Pixel::new(20, 18, 24, 200)
        };
        let tb_h = scaled_title_bar_height();
        fb.fill_rounded_rect_gradient_aa(
            Rect::new(
                self.rect.x + 1,
                self.rect.y + 1,
                self.rect.width - 2,
                tb_h - 1,
            ),
            title_top,
            title_bottom,
            r.saturating_sub(1),
        );

        if self.focused {
            fb.draw_hline(
                self.rect.x + 3,
                self.rect.y + 2,
                self.rect.width.saturating_sub(6),
                Pixel::new(200, 150, 130, 20),
            );
        }

        let sep_color = if self.focused {
            Pixel::new(232, 140, 120, 22)
        } else {
            Pixel::new(255, 255, 255, 8)
        };
        fb.draw_hline(
            self.rect.x + 1,
            self.rect.y + tb_h as i32 - 1,
            self.rect.width - 2,
            sep_color,
        );

        // ══════════════════════════════════════════════════════════
        // WINDOW CONTROLS — Right-aligned, minimal, elegant
        // ══════════════════════════════════════════════════════════
        let (mouse_x, mouse_y) = {
            let m = super::input::MOUSE.lock();
            (m.x, m.y)
        };

        let ctrl_cy = self.rect.y + tb_h as i32 / 2;
        let btn_w = scaled_btn_width();
        let btn_h = scaled_btn_height();
        let btn_gap = scaled_btn_gap();
        let btn_margin = scaled_btn_margin_right();
        let close_x = self.rect.x + self.rect.width as i32 - btn_w - btn_margin;
        let max_x = close_x - btn_w - btn_gap;
        let min_x = max_x - btn_w - btn_gap;
        let btn_top_pad = (tb_h as i32 - btn_h) / 2;

        // ── MINIMIZE ──
        if self.minimizable {
            let bx = min_x;
            let mr = Rect::new(bx, self.rect.y + btn_top_pad, btn_w as u32, btn_h as u32);
            let hovered = mr.contains(mouse_x, mouse_y) && self.focused;

            if hovered {
                fb.fill_rounded_rect_aa(mr, Pixel::new(255, 255, 255, 18), 6);
            }

            let glyph_color = if hovered {
                Pixel::new(240, 232, 224, 255)
            } else if self.focused {
                Pixel::new(180, 170, 160, 180)
            } else {
                Pixel::new(90, 82, 76, 100)
            };
            let gcx = bx + btn_w / 2;
            let dh = scaled_dash_half();
            fb.draw_hline(gcx - dh, ctrl_cy, (dh * 2) as u32, glyph_color);
            fb.draw_hline(gcx - dh, ctrl_cy + 1, (dh * 2) as u32, glyph_color);
        }

        // ── MAXIMIZE ──
        if self.maximizable {
            let bx = max_x;
            let mr = Rect::new(bx, self.rect.y + btn_top_pad, btn_w as u32, btn_h as u32);
            let hovered = mr.contains(mouse_x, mouse_y) && self.focused;

            if hovered {
                fb.fill_rounded_rect_aa(mr, Pixel::new(255, 255, 255, 18), 6);
            }

            let glyph_color = if hovered {
                Pixel::new(240, 232, 224, 255)
            } else if self.focused {
                Pixel::new(180, 170, 160, 180)
            } else {
                Pixel::new(90, 82, 76, 100)
            };
            let gcx = bx + btn_w / 2;
            let mbh = scaled_max_box_half();

            let box_size = (mbh * 2 + 1) as u32;

            if self.state == WindowState::Maximized
                || self.state == WindowState::SnappedLeft
                || self.state == WindowState::SnappedRight
                || self.state == WindowState::SnappedTopLeft
                || self.state == WindowState::SnappedTopRight
                || self.state == WindowState::SnappedBottomLeft
                || self.state == WindowState::SnappedBottomRight
            {
                fb.draw_rounded_rect(
                    Rect::new(gcx - mbh, ctrl_cy - mbh, box_size - 1, box_size - 1),
                    glyph_color,
                    1,
                    1,
                );
                fb.draw_rounded_rect(
                    Rect::new(gcx - mbh + 2, ctrl_cy - mbh - 2, box_size - 1, box_size - 1),
                    glyph_color,
                    1,
                    1,
                );
                let fill_size = (box_size as i32 - 3).max(1) as u32;
                fb.fill_rect(
                    Rect::new(gcx - mbh + 1, ctrl_cy - mbh + 1, fill_size, fill_size),
                    if self.focused {
                        colors::WINDOW_TITLE_BG
                    } else {
                        colors::WINDOW_TITLE_BG_INACTIVE
                    },
                );
                fb.draw_rounded_rect(
                    Rect::new(gcx - mbh, ctrl_cy - mbh, box_size - 1, box_size - 1),
                    glyph_color,
                    1,
                    1,
                );
            } else {
                fb.draw_rounded_rect(
                    Rect::new(gcx - mbh, ctrl_cy - mbh, box_size, box_size),
                    glyph_color,
                    1,
                    1,
                );
            }
        }

        // ── CLOSE ──
        if self.closeable {
            let bx = close_x;
            let cr = Rect::new(bx, self.rect.y + btn_top_pad, btn_w as u32, btn_h as u32);
            let hovered = cr.contains(mouse_x, mouse_y) && self.focused;

            if hovered {
                fb.fill_rounded_rect_aa(cr, Pixel::new(220, 40, 50, 70), 6);
            }

            let glyph_color = if hovered {
                Pixel::new(255, 160, 150, 255)
            } else if self.focused {
                Pixel::new(180, 170, 160, 180)
            } else {
                Pixel::new(90, 82, 76, 100)
            };
            let gcx = bx + btn_w / 2;
            let s = scaled_glyph_half();
            fb.draw_line_aa(gcx - s, ctrl_cy - s, gcx + s, ctrl_cy + s, glyph_color);
            fb.draw_line_aa(
                gcx - s + 1,
                ctrl_cy - s,
                gcx + s + 1,
                ctrl_cy + s,
                glyph_color,
            );
            fb.draw_line_aa(gcx + s, ctrl_cy - s, gcx - s, ctrl_cy + s, glyph_color);
            fb.draw_line_aa(
                gcx + s - 1,
                ctrl_cy - s,
                gcx - s - 1,
                ctrl_cy + s,
                glyph_color,
            );
        }

        // ══════════════════════════════════════════════════════════
        // TITLE TEXT — Using font_engine for scalable rendering
        // ══════════════════════════════════════════════════════════
        let title_x = self.rect.x + scaled_title_pad_left();
        let title_max_w = self.rect.width as i32
            - scaled_title_pad_left()
            - (btn_w * 3 + btn_gap * 2 + scaled_title_pad_left());
        let display_title: alloc::string::String = {
            let full_w = font_engine::measure_ui_text(&self.title, 13) as i32;
            if full_w <= title_max_w {
                self.title.clone()
            } else {
                // Truncate with ellipsis
                let max_chars = (title_max_w / 8).max(3) as usize;
                let mut t = alloc::string::String::from(
                    &self.title[..max_chars.min(self.title.len()).saturating_sub(3)],
                );
                t.push_str("...");
                t
            }
        };
        // Use font_engine for high-quality scalable title text
        font_engine::draw_ui_bold(
            fb,
            title_x,
            self.rect.y + (tb_h as i32 - 13) / 2,
            &display_title,
            13,
            title_text_color,
        );

        // ── Content area ──
        let content = self.content_rect();
        fb.fill_rect(content, self.content_color);

        fb.push_clip(content);
        self.draw_content(fb);
        fb.pop_clip();

        // ── Resize grip indicator ──
        if self.resizable && self.state == WindowState::Normal {
            let grip_color = if self.focused {
                Pixel::new(200, 150, 130, 50)
            } else {
                Pixel::new(140, 130, 120, 30)
            };
            let bx = self.rect.x + self.rect.width as i32 - 14;
            let by = self.rect.y + self.rect.height as i32 - 14;
            fb.fill_circle_aa(bx + 8, by + 8, 1, grip_color);
            fb.fill_circle_aa(bx + 4, by + 8, 1, grip_color);
            fb.fill_circle_aa(bx + 8, by + 4, 1, grip_color);
            fb.fill_circle_aa(bx, by + 8, 1, grip_color);
            fb.fill_circle_aa(bx + 4, by + 4, 1, grip_color);
            fb.fill_circle_aa(bx + 8, by, 1, grip_color);
        }

        // ── Restore original rect after animation override ──
        if is_anim {
            self.rect = original_rect;
        }
    }
}
