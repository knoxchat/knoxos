/// Browser window content drawing
use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

use super::types::Window;

impl Window {
    pub(super) fn draw_browser_content(&mut self, fb: &mut FrameBuffer, content: Rect) {
        // Get browser state for this window
        let browsers = crate::gui::browser::BROWSERS.lock();
        let state = browsers.get(&self.id);

        // Detect if Vivaldi is installed/running
        let vivaldi_active = state.map(|s| s.vivaldi).unwrap_or_else(|| {
            let st = crate::vivaldi::status();
            st == crate::vivaldi::VivaldiState::Running
                || st == crate::vivaldi::VivaldiState::Installed
                || self.title.contains("Vivaldi")
        });

        // Colors based on Vivaldi mode
        let accent = if vivaldi_active {
            Pixel::rgb(239, 46, 67)
        } else {
            Pixel::rgb(50, 50, 50)
        };

        // ─── Tab bar (28px) ──────────────────────────────────────────
        let tab_h: u32 = 28;
        let tab_bar_bg = if vivaldi_active {
            Pixel::rgb(51, 17, 19)
        } else {
            Pixel::rgb(38, 38, 38)
        };
        fb.fill_rect(
            Rect::new(content.x, content.y, content.width, tab_h),
            tab_bar_bg,
        );
        let tab_bg = if vivaldi_active {
            Pixel::rgb(30, 30, 32)
        } else {
            Pixel::rgb(50, 50, 50)
        };
        fb.fill_rounded_rect_aa(
            Rect::new(content.x + 4, content.y + 4, 160, tab_h - 4),
            tab_bg,
            4,
        );
        if vivaldi_active {
            fb.fill_rounded_rect_aa(Rect::new(content.x + 4, content.y + 4, 160, 2), accent, 1);
        }
        // Tab title from browser state
        let tab_title = state
            .map(|s| s.page.title.as_str())
            .unwrap_or(if vivaldi_active {
                "Speed Dial - Vivaldi"
            } else {
                "KnoxOS - Home"
            });
        // Truncate tab title to fit
        let max_tab_chars = 18;
        let display_title: &str = if tab_title.len() > max_tab_chars {
            &tab_title[..max_tab_chars]
        } else {
            tab_title
        };
        fonts::draw_string_bold_compact(
            fb,
            content.x + 12,
            content.y + 8,
            display_title,
            colors::WHITE,
            1,
        );
        // Tab close X
        let tab_close_x = content.x + 150;
        let tab_close_y = content.y + 10;
        fonts::draw_string_bold_compact(
            fb,
            tab_close_x,
            tab_close_y,
            "x",
            Pixel::rgb(140, 140, 140),
            1,
        );
        // + new tab
        fonts::draw_string_bold_compact(
            fb,
            content.x + 172,
            content.y + 8,
            "+",
            Pixel::rgb(140, 140, 140),
            1,
        );

        // ─── URL bar (35px) ──────────────────────────────────────────
        let url_h: u32 = 35;
        let url_y = content.y + tab_h as i32;
        let url_bar_bg = if vivaldi_active {
            Pixel::rgb(30, 30, 32)
        } else {
            Pixel::rgb(50, 50, 50)
        };
        fb.fill_rect(
            Rect::new(content.x, url_y, content.width, url_h),
            url_bar_bg,
        );

        // Navigation buttons: < > O
        let can_back = state.map(|s| !s.back_stack.is_empty()).unwrap_or(false);
        let can_fwd = state.map(|s| !s.forward_stack.is_empty()).unwrap_or(false);
        let back_color = if can_back {
            Pixel::rgb(200, 200, 200)
        } else {
            Pixel::rgb(80, 80, 80)
        };
        let fwd_color = if can_fwd {
            Pixel::rgb(200, 200, 200)
        } else {
            Pixel::rgb(80, 80, 80)
        };
        fonts::draw_string_bold_compact(fb, content.x + 8, url_y + 11, "<", back_color, 1);
        fonts::draw_string_bold_compact(fb, content.x + 22, url_y + 11, ">", fwd_color, 1);
        fonts::draw_string_bold_compact(
            fb,
            content.x + 40,
            url_y + 11,
            "O",
            Pixel::rgb(160, 160, 160),
            1,
        );

        // URL input field
        let url_field_x = content.x + 60;
        let url_field_y = url_y + 6;
        let url_field_w = content.width.saturating_sub(70);
        let url_focused = state
            .map(|s| s.focus == crate::gui::browser::BrowserFocus::UrlBar)
            .unwrap_or(false);
        let url_field_bg = if url_focused {
            Pixel::rgb(45, 45, 50)
        } else {
            Pixel::rgb(35, 35, 35)
        };
        let url_border = if url_focused {
            accent
        } else {
            Pixel::rgb(70, 70, 70)
        };
        fb.fill_rounded_rect_aa(
            Rect::new(url_field_x, url_field_y, url_field_w, 22),
            url_field_bg,
            4,
        );
        fb.draw_rounded_rect(
            Rect::new(url_field_x, url_field_y, url_field_w, 22),
            url_border,
            4,
            1,
        );
        // Lock icon
        let is_https = state
            .map(|s| s.url_text.starts_with("https://"))
            .unwrap_or(false);
        let lock_color = if is_https {
            Pixel::rgb(80, 180, 80)
        } else {
            Pixel::rgb(100, 100, 100)
        };
        fb.fill_rounded_rect_aa(Rect::new(url_field_x + 5, url_y + 11, 6, 6), lock_color, 1);
        fb.draw_rounded_rect(
            Rect::new(url_field_x + 6, url_y + 8, 4, 4),
            lock_color,
            1,
            1,
        );

        // URL text
        let url_text = state
            .map(|s| s.url_text.as_str())
            .unwrap_or(if vivaldi_active {
                "vivaldi://newtab"
            } else {
                "https://knoxos.local"
            });
        let url_text_x = url_field_x + 16;
        // Calculate visible portion of URL text
        let max_url_chars = ((url_field_w as i32 - 24) / 7).max(1) as usize;
        let url_cursor = state.map_or(0, |s| s.url_cursor);
        // Scroll URL text so cursor is visible
        let url_scroll = url_cursor.saturating_sub(max_url_chars);
        let visible_url = if url_scroll < url_text.len() {
            let end = (url_scroll + max_url_chars).min(url_text.len());
            &url_text[url_scroll..end]
        } else {
            ""
        };
        let url_text_color = if url_focused {
            Pixel::rgb(220, 220, 225)
        } else {
            Pixel::rgb(180, 180, 180)
        };
        fonts::draw_string_compact(fb, url_text_x, url_y + 11, visible_url, url_text_color, 1);
        // Cursor in URL bar
        if url_focused {
            let cursor_x = url_text_x + ((url_cursor - url_scroll) as i32 * 8);
            // Blinking cursor (simple: always show when focused)
            fb.fill_rect(Rect::new(cursor_x, url_y + 9, 1, 14), colors::WHITE);
        }

        // ─── Page content area ───────────────────────────────────────
        let total_chrome = tab_h + url_h;
        let clip_y = content.y + total_chrome as i32;
        let clip_h = content.height.saturating_sub(total_chrome);
        let page_y_base = clip_y - self.scroll_y;

        // Page background
        let page_bg = if vivaldi_active {
            Pixel::rgb(24, 24, 26)
        } else {
            Pixel::rgb(255, 255, 255)
        };
        fb.fill_rect(Rect::new(content.x, clip_y, content.width, clip_h), page_bg);

        // Determine what to render
        let is_speed_dial = state
            .map(|s| s.page.url == "vivaldi://newtab" || s.page.url == "vivaldi://speeddial")
            .unwrap_or(vivaldi_active);

        let is_knoxos_home = state
            .map(|s| s.page.url == "knoxos://home")
            .unwrap_or(!vivaldi_active);

        if is_speed_dial {
            // ═══ Speed Dial rendering ═══
            let page_width = content.width.saturating_sub(40);
            let page_y = page_y_base;

            // Search bar
            let search_w = page_width.min(500);
            let search_x = content.x + (content.width as i32 - search_w as i32) / 2;
            let search_y = page_y + 60;
            let search_focused = state
                .map(|s| s.focus == crate::gui::browser::BrowserFocus::SearchBar)
                .unwrap_or(false);

            if search_y >= clip_y && search_y < clip_y + clip_h as i32 - 40 {
                let sb_bg = if search_focused {
                    Pixel::rgb(50, 50, 55)
                } else {
                    Pixel::rgb(44, 44, 48)
                };
                let sb_border = if search_focused {
                    accent
                } else {
                    Pixel::rgb(70, 70, 74)
                };
                fb.fill_rounded_rect_aa(Rect::new(search_x, search_y, search_w, 36), sb_bg, 8);
                fb.draw_rounded_rect(Rect::new(search_x, search_y, search_w, 36), sb_border, 8, 1);

                let search_text = state.map(|s| s.search_text.as_str()).unwrap_or("");
                if search_text.is_empty() && !search_focused {
                    fonts::draw_string_compact(
                        fb,
                        search_x + 16,
                        search_y + 12,
                        "Search with Google or enter address",
                        Pixel::rgb(120, 120, 125),
                        1,
                    );
                } else {
                    // Draw actual search text
                    let max_search_chars = ((search_w as i32 - 32) / 7).max(1) as usize;
                    let search_cursor = state.map(|s| s.search_cursor).unwrap_or(0);
                    let vis_end = max_search_chars.min(search_text.len());
                    let vis_text = &search_text[..vis_end];
                    fonts::draw_string_compact(
                        fb,
                        search_x + 16,
                        search_y + 12,
                        vis_text,
                        Pixel::rgb(220, 220, 225),
                        1,
                    );
                    // Cursor
                    if search_focused {
                        let cx = search_x + 16 + (search_cursor as i32 * 8);
                        fb.fill_rect(Rect::new(cx, search_y + 10, 1, 16), colors::WHITE);
                    }
                }
            }

            // Speed dial grid
            let tile_w: u32 = 140;
            let tile_h: u32 = 100;
            let tile_gap: i32 = 20;
            let grid_cols = 3i32;
            let grid_w = grid_cols * (tile_w as i32 + tile_gap) - tile_gap;
            let grid_x = content.x + (content.width as i32 - grid_w) / 2;
            let grid_y = page_y + 130;

            let speed_dial_colors = [
                Pixel::rgb(0, 160, 255), // KnoxOS
                Pixel::rgb(36, 41, 47),  // GitHub
                Pixel::rgb(239, 46, 67), // Vivaldi
                Pixel::rgb(60, 60, 60),  // Wikipedia
                Pixel::rgb(255, 69, 0),  // Reddit
                Pixel::rgb(255, 0, 0),   // YouTube
            ];

            // Check if mouse is hovering over a tile
            let hovered = state.map(|s| s.hovered_link).unwrap_or(None);

            for (i, ((name, _url), color)) in crate::gui::browser::SPEED_DIAL_SITES
                .iter()
                .zip(speed_dial_colors.iter())
                .enumerate()
            {
                let col = (i % grid_cols as usize) as i32;
                let row = (i / grid_cols as usize) as i32;
                let tx = grid_x + col * (tile_w as i32 + tile_gap);
                let ty = grid_y + row * (tile_h as i32 + tile_gap + 20);

                if ty >= clip_y && ty < clip_y + clip_h as i32 - tile_h as i32 {
                    let tile_bg = if Some(i) == hovered {
                        Pixel::rgb(55, 55, 60)
                    } else {
                        Pixel::rgb(40, 40, 44)
                    };
                    fb.fill_rounded_rect_aa(Rect::new(tx, ty, tile_w, tile_h), tile_bg, 8);
                    let cx = tx + tile_w as i32 / 2;
                    let cy = ty + 38;
                    fb.fill_circle_aa(cx, cy, 18, *color);
                    let first_char = &name[..1];
                    fonts::draw_string_bold_compact(
                        fb,
                        cx - 4,
                        cy - 5,
                        first_char,
                        colors::WHITE,
                        1,
                    );
                    let label_x = tx + (tile_w as i32 - name.len() as i32 * 8) / 2;
                    fonts::draw_string_compact(
                        fb,
                        label_x,
                        ty + tile_h as i32 + 6,
                        name,
                        Pixel::rgb(180, 180, 185),
                        1,
                    );
                }
            }

            // Branding
            let brand_y = grid_y + 2 * (tile_h as i32 + tile_gap + 20) + 40;
            if brand_y >= clip_y && brand_y < clip_y + clip_h as i32 {
                let brand_text = "Vivaldi 7.1.3570.39 on KnoxOS";
                let bx = content.x + (content.width as i32 - brand_text.len() as i32 * 8) / 2;
                fonts::draw_string_compact(fb, bx, brand_y, brand_text, Pixel::rgb(90, 90, 95), 1);
            }
        } else if let Some(browser_state) = state {
            // ═══ Loaded page rendering (from BrowserState lines) ═══
            let page_y = page_y_base;
            let page_width = content.width.saturating_sub(40);
            let margin_x = content.x + 20;
            let line_height: i32 = 20;
            let mut y_pos = page_y + 20;

            // Loading indicator
            if browser_state.page.loading {
                let loading_bar_w = content.width / 3;
                fb.fill_rounded_rect_aa(Rect::new(content.x, clip_y, loading_bar_w, 3), accent, 1);
            }

            for (i, line) in browser_state.page.lines.iter().enumerate() {
                if y_pos >= clip_y + clip_h as i32 {
                    break; // Below visible area
                }

                let extra_h = match line.style {
                    crate::gui::browser::LineStyle::Heading1 => 12,
                    crate::gui::browser::LineStyle::Heading2 => 6,
                    _ => 0,
                };

                if y_pos + line_height + extra_h >= clip_y {
                    // This line is visible
                    let text_color = if vivaldi_active {
                        match line.style {
                            crate::gui::browser::LineStyle::Heading1 => Pixel::rgb(240, 240, 245),
                            crate::gui::browser::LineStyle::Heading2 => Pixel::rgb(200, 200, 210),
                            crate::gui::browser::LineStyle::Heading3 => Pixel::rgb(180, 180, 190),
                            crate::gui::browser::LineStyle::Link => Pixel::rgb(100, 180, 255),
                            crate::gui::browser::LineStyle::Error => Pixel::rgb(255, 100, 100),
                            crate::gui::browser::LineStyle::Code => Pixel::rgb(160, 220, 160),
                            crate::gui::browser::LineStyle::ListItem => Pixel::rgb(180, 180, 185),
                            _ => Pixel::rgb(170, 170, 175),
                        }
                    } else {
                        match line.style {
                            crate::gui::browser::LineStyle::Heading1 => Pixel::rgb(0, 0, 0),
                            crate::gui::browser::LineStyle::Heading2 => Pixel::rgb(30, 30, 30),
                            crate::gui::browser::LineStyle::Heading3 => Pixel::rgb(50, 50, 50),
                            crate::gui::browser::LineStyle::Link => Pixel::rgb(26, 13, 171),
                            crate::gui::browser::LineStyle::Error => Pixel::rgb(200, 0, 0),
                            crate::gui::browser::LineStyle::Code => Pixel::rgb(60, 60, 60),
                            _ => Pixel::rgb(80, 80, 80),
                        }
                    };

                    let scale = match line.style {
                        crate::gui::browser::LineStyle::Heading1 => 2,
                        crate::gui::browser::LineStyle::Heading2 => 2,
                        _ => 1,
                    };

                    if line.style == crate::gui::browser::LineStyle::Blank {
                        // Blank line — just add spacing
                    } else if line.link_url.is_some() {
                        // Underline for links
                        fonts::draw_string(fb, margin_x, y_pos, &line.text, text_color, scale);
                        // Draw underline
                        let text_w = line.text.len() as i32 * 8 * scale as i32;
                        fb.fill_rect(
                            Rect::new(margin_x, y_pos + 14 * scale as i32, text_w as u32, 1),
                            text_color,
                        );
                    } else {
                        // Word wrap long lines
                        let max_chars = (page_width as i32 / (8 * scale as i32)).max(1) as usize;
                        let is_heading = matches!(
                            line.style,
                            crate::gui::browser::LineStyle::Heading1
                                | crate::gui::browser::LineStyle::Heading2
                                | crate::gui::browser::LineStyle::Heading3
                        );
                        if line.text.len() > max_chars {
                            // Multi-line rendering
                            let mut start = 0;
                            while start < line.text.len() {
                                let end = (start + max_chars).min(line.text.len());
                                // Try to break at a space
                                let break_at = if end < line.text.len() {
                                    line.text[start..end]
                                        .rfind(' ')
                                        .map(|p| start + p + 1)
                                        .unwrap_or(end)
                                } else {
                                    end
                                };
                                if y_pos >= clip_y && y_pos < clip_y + clip_h as i32 {
                                    if is_heading {
                                        fonts::draw_string_bold(
                                            fb,
                                            margin_x,
                                            y_pos,
                                            &line.text[start..break_at],
                                            text_color,
                                            scale,
                                        );
                                    } else {
                                        fonts::draw_string(
                                            fb,
                                            margin_x,
                                            y_pos,
                                            &line.text[start..break_at],
                                            text_color,
                                            scale,
                                        );
                                    }
                                }
                                y_pos += line_height;
                                start = break_at;
                            }
                            // Don't add extra spacing since we already advanced
                            y_pos += extra_h;
                            continue;
                        } else if is_heading {
                            fonts::draw_string_bold(
                                fb, margin_x, y_pos, &line.text, text_color, scale,
                            );
                        } else {
                            fonts::draw_string(fb, margin_x, y_pos, &line.text, text_color, scale);
                        }
                    }
                }

                y_pos += line_height + extra_h;
            }

            // Status bar at bottom (shows link URL on hover)
            if !browser_state.status_text.is_empty() {
                let status_y = clip_y + clip_h as i32 - 22;
                let status_text_w =
                    (browser_state.status_text.len() as u32 * 7 + 20).min(content.width);
                let status_bg = if vivaldi_active {
                    Pixel::rgb(30, 30, 34)
                } else {
                    Pixel::rgb(240, 240, 240)
                };
                fb.fill_rounded_rect_aa(
                    Rect::new(content.x, status_y, status_text_w, 22),
                    status_bg,
                    4,
                );
                fb.draw_rounded_rect(
                    Rect::new(content.x, status_y, status_text_w, 22),
                    if vivaldi_active {
                        Pixel::rgb(50, 50, 55)
                    } else {
                        Pixel::rgb(200, 200, 200)
                    },
                    4,
                    1,
                );
                let status_color = if vivaldi_active {
                    Pixel::rgb(140, 140, 145)
                } else {
                    Pixel::rgb(100, 100, 100)
                };
                fonts::draw_string_compact(
                    fb,
                    content.x + 8,
                    status_y + 5,
                    &browser_state.status_text,
                    status_color,
                    1,
                );
            }
        } else {
            // ═══ Fallback: no browser state (shouldn't happen) ═══
            let heading_y = page_y_base + 30;
            if heading_y >= clip_y && heading_y < clip_y + clip_h as i32 {
                fonts::draw_string_bold(
                    fb,
                    content.x + 20,
                    heading_y,
                    "Welcome to KnoxOS Browser",
                    Pixel::rgb(0, 0, 0),
                    2,
                );
            }
        }

        // Scrollbar
        let total_content_height = if is_speed_dial {
            500i32
        } else {
            state
                .map(|s| (s.page.lines.len() as i32 * 20).max(200) + 60)
                .unwrap_or(320)
        };
        if total_content_height > clip_h as i32 {
            let sb_x = content.x + content.width as i32 - 8;
            fb.fill_rounded_rect_aa(
                Rect::new(sb_x, clip_y, 6, clip_h),
                colors::SCROLLBAR_TRACK,
                3,
            );
            let visible_ratio = clip_h as f32 / total_content_height as f32;
            let thumb_h = ((visible_ratio * clip_h as f32) as u32).max(20).min(clip_h);
            let scroll_ratio = if total_content_height > clip_h as i32 {
                self.scroll_y as f32 / (total_content_height - clip_h as i32) as f32
            } else {
                0.0
            };
            let thumb_y = clip_y + (scroll_ratio * (clip_h - thumb_h) as f32) as i32;
            fb.fill_rounded_rect_aa(
                Rect::new(sb_x, thumb_y, 6, thumb_h),
                colors::SCROLLBAR_THUMB,
                3,
            );
        }
    }
}
