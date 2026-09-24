/// File explorer window content drawing
use alloc::vec::Vec;

use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

use super::types::{ExplorerAction, ExplorerSort, Window};

impl Window {
    pub(super) fn draw_file_explorer_content(&mut self, fb: &mut FrameBuffer, content: Rect) {
        use crate::gui::explorer;

        let current_path = explorer::current_path(&self.title);

        // === Read and sort directory entries using explorer module ===
        let mut entries = explorer::read_entries(&current_path, self.explorer_show_hidden);
        explorer::sort_entries(&mut entries, self.explorer_sort, self.explorer_sort_asc);

        // === Apply search filter if search mode is active ===
        if self.explorer_search_active && !self.explorer_search_query.is_empty() {
            entries = explorer::filter_entries(&entries, &self.explorer_search_query);
        }

        // === Navigation bar (35px) ===
        let nav_h: u32 = 35;
        fb.fill_rect(
            Rect::new(content.x, content.y, content.width, nav_h),
            Pixel::rgb(40, 40, 40),
        );
        fb.draw_hline(
            content.x,
            content.y + nav_h as i32,
            content.width,
            Pixel::rgb(60, 60, 60),
        );

        // Navigation buttons (back/forward/up) with active/dim states
        {
            let can_back = self.explorer_history_idx > 0;
            let can_forward = self.explorer_history_idx + 1 < self.explorer_history.len();
            let can_up = current_path != "/";

            let btn_active = Pixel::rgb(180, 185, 195);
            let btn_dim = Pixel::rgb(80, 85, 95);
            let btn_y = content.y + 8;

            // Back arrow (◄)
            let back_c = if can_back { btn_active } else { btn_dim };
            let bx = content.x + 10;
            fb.draw_line_aa(bx + 6, btn_y, bx, btn_y + 6, back_c);
            fb.draw_line_aa(bx, btn_y + 6, bx + 6, btn_y + 12, back_c);
            fb.draw_line_aa(bx + 7, btn_y, bx + 1, btn_y + 6, back_c);
            fb.draw_line_aa(bx + 1, btn_y + 6, bx + 7, btn_y + 12, back_c);

            // Forward arrow (►)
            let fwd_c = if can_forward { btn_active } else { btn_dim };
            let fx = content.x + 28;
            fb.draw_line_aa(fx, btn_y, fx + 6, btn_y + 6, fwd_c);
            fb.draw_line_aa(fx + 6, btn_y + 6, fx, btn_y + 12, fwd_c);
            fb.draw_line_aa(fx + 1, btn_y, fx + 7, btn_y + 6, fwd_c);
            fb.draw_line_aa(fx + 7, btn_y + 6, fx + 1, btn_y + 12, fwd_c);

            // Up arrow (▲)
            let up_c = if can_up { btn_active } else { btn_dim };
            let ux = content.x + 52;
            fb.draw_line_aa(ux, btn_y + 8, ux + 6, btn_y + 2, up_c);
            fb.draw_line_aa(ux + 6, btn_y + 2, ux + 12, btn_y + 8, up_c);
            fb.draw_line_aa(ux, btn_y + 9, ux + 6, btn_y + 3, up_c);
            fb.draw_line_aa(ux + 6, btn_y + 3, ux + 12, btn_y + 9, up_c);
        }

        // Address bar (editable)
        let path_bar_w = content.width.saturating_sub(90);
        let path_bar_rect = Rect::new(content.x + 80, content.y + 6, path_bar_w, 22);

        if self.explorer_editing_path {
            // Editing mode — cyan border, show buffer text with cursor
            fb.fill_rounded_rect_aa(path_bar_rect, Pixel::rgb(20, 20, 20), 4);
            fb.draw_rounded_rect(path_bar_rect, Pixel::rgb(0, 200, 220), 4, 1);
            let max_w = path_bar_w.saturating_sub(16);
            let display = fonts::truncate_with_ellipsis(&self.explorer_path_buf, max_w, 1);
            fonts::draw_string_compact(
                fb,
                content.x + 88,
                content.y + 11,
                &display,
                Pixel::rgb(230, 230, 230),
                1,
            );
            // Blinking cursor (approximate position)
            let cursor_x =
                content.x + 88 + (self.explorer_path_cursor as i32 * 6).min(max_w as i32 - 2);
            fb.fill_rect(
                Rect::new(cursor_x, content.y + 9, 1, 14),
                Pixel::rgb(0, 200, 220),
            );
        } else {
            // Normal mode — display current path
            fb.fill_rounded_rect_aa(path_bar_rect, Pixel::rgb(25, 25, 25), 4);
            fb.draw_rounded_rect(path_bar_rect, Pixel::rgb(60, 60, 60), 4, 1);
            let max_path_w = path_bar_w.saturating_sub(16);
            let display_path = fonts::truncate_with_ellipsis(&current_path, max_path_w, 1);
            fonts::draw_string_compact(
                fb,
                content.x + 88,
                content.y + 11,
                &display_path,
                Pixel::rgb(200, 200, 200),
                1,
            );
        }

        // === Search bar (Ctrl+F) — shown between nav bar and column headers ===
        let search_bar_h: u32 = if self.explorer_search_active { 28 } else { 0 };
        if self.explorer_search_active {
            let sb_y = content.y + nav_h as i32;
            fb.fill_rect(
                Rect::new(content.x, sb_y, content.width, search_bar_h),
                Pixel::rgb(30, 30, 35),
            );
            fb.draw_hline(
                content.x,
                sb_y + search_bar_h as i32 - 1,
                content.width,
                Pixel::rgb(55, 55, 60),
            );

            // Search icon (magnifying glass)
            let icon_x = content.x + 12;
            let icon_y = sb_y + 6;
            fb.draw_circle_aa(icon_x + 6, icon_y + 6, 5, Pixel::rgb(0, 200, 220));
            fb.draw_line_aa(
                icon_x + 10,
                icon_y + 10,
                icon_x + 14,
                icon_y + 14,
                Pixel::rgb(0, 200, 220),
            );

            // Search text field
            let search_field_rect = Rect::new(
                content.x + 30,
                sb_y + 4,
                content.width.saturating_sub(44),
                20,
            );
            fb.fill_rounded_rect_aa(search_field_rect, Pixel::rgb(20, 20, 24), 4);
            fb.draw_rounded_rect(search_field_rect, Pixel::rgb(0, 200, 220), 4, 1);

            if self.explorer_search_query.is_empty() {
                fonts::draw_string_compact(
                    fb,
                    content.x + 38,
                    sb_y + 9,
                    "Search files...",
                    Pixel::rgb(100, 100, 110),
                    1,
                );
            } else {
                let max_sw = search_field_rect.width.saturating_sub(16);
                let display = fonts::truncate_with_ellipsis(&self.explorer_search_query, max_sw, 1);
                fonts::draw_string_compact(
                    fb,
                    content.x + 38,
                    sb_y + 9,
                    &display,
                    Pixel::rgb(230, 230, 230),
                    1,
                );
            }
            // Cursor
            let cur_x = content.x
                + 38
                + (self.explorer_search_cursor as i32 * 6).min(search_field_rect.width as i32 - 16);
            fb.fill_rect(Rect::new(cur_x, sb_y + 7, 1, 14), Pixel::rgb(0, 200, 220));
        }

        // === Sidebar bookmarks panel (left side) ===
        let sidebar_w: u32 = if self.explorer_sidebar_visible {
            160
        } else {
            0
        };
        let sidebar_x = content.x;
        let sidebar_y = content.y + nav_h as i32 + search_bar_h as i32;
        let sidebar_h = content.height.saturating_sub(nav_h + search_bar_h);

        if self.explorer_sidebar_visible {
            // Sidebar background
            fb.fill_rect(
                Rect::new(sidebar_x, sidebar_y, sidebar_w, sidebar_h),
                Pixel::rgb(28, 28, 32),
            );
            // Right border
            fb.draw_vline(
                sidebar_x + sidebar_w as i32 - 1,
                sidebar_y,
                sidebar_h,
                Pixel::rgb(55, 55, 60),
            );

            let bookmark_items: [(&str, &str, Pixel); 7] = [
                ("Home", "/home/user", Pixel::rgb(0, 180, 255)),
                ("Desktop", "/home/user/Desktop", Pixel::rgb(180, 140, 255)),
                (
                    "Documents",
                    "/home/user/Documents",
                    Pixel::rgb(255, 180, 60),
                ),
                (
                    "Downloads",
                    "/home/user/Downloads",
                    Pixel::rgb(100, 220, 100),
                ),
                ("Pictures", "/home/user/Pictures", Pixel::rgb(255, 120, 180)),
                ("Music", "/home/user/Music", Pixel::rgb(255, 100, 100)),
                ("Root /", "/", Pixel::rgb(160, 160, 160)),
            ];

            // Section header
            fonts::draw_string_bold_compact(
                fb,
                sidebar_x + 10,
                sidebar_y + 6,
                "Bookmarks",
                Pixel::rgb(120, 120, 130),
                1,
            );

            for (i, &(label, path, icon_color)) in bookmark_items.iter().enumerate() {
                let by = sidebar_y + 24 + (i as i32 * 26);
                let item_rect = Rect::new(sidebar_x + 4, by, sidebar_w - 8, 24);

                // Highlight if current path matches
                let is_current = current_path == path;
                if is_current {
                    fb.fill_rounded_rect_aa(item_rect, Pixel::new(0, 180, 220, 25), 4);
                }

                // Folder icon dot
                fb.fill_circle_aa(sidebar_x + 14, by + 12, 4, icon_color);

                // Label
                let label_color = if is_current {
                    Pixel::rgb(0, 200, 220)
                } else {
                    Pixel::rgb(180, 180, 185)
                };
                fonts::draw_string_compact(fb, sidebar_x + 24, by + 6, label, label_color, 1);
            }

            // Separator
            let sep_y = sidebar_y + 24 + 7 * 26 + 4;
            fb.draw_hline(sidebar_x + 8, sep_y, sidebar_w - 16, Pixel::rgb(50, 50, 55));

            // Devices section header
            fonts::draw_string_bold_compact(
                fb,
                sidebar_x + 10,
                sep_y + 6,
                "Devices",
                Pixel::rgb(120, 120, 130),
                1,
            );

            // Show root filesystem as a device
            let dev_y = sep_y + 22;
            let dev_rect = Rect::new(sidebar_x + 4, dev_y, sidebar_w - 8, 24);
            let is_root_current = current_path == "/";
            if is_root_current {
                fb.fill_rounded_rect_aa(dev_rect, Pixel::new(0, 180, 220, 25), 4);
            }
            // Drive icon
            fb.fill_rounded_rect_aa(
                Rect::new(sidebar_x + 10, dev_y + 6, 10, 8),
                Pixel::rgb(140, 140, 150),
                2,
            );
            fonts::draw_string_compact(
                fb,
                sidebar_x + 24,
                dev_y + 6,
                "Filesystem",
                Pixel::rgb(160, 160, 165),
                1,
            );
        }

        // Adjust content area for sidebar offset
        let list_content_x = content.x + sidebar_w as i32;
        let list_content_w = content.width.saturating_sub(sidebar_w);

        // === Column headers with sort indicators ===
        let header_y = content.y + nav_h as i32 + search_bar_h as i32;
        fb.fill_rect(
            Rect::new(list_content_x, header_y, list_content_w, 20),
            Pixel::rgb(35, 35, 35),
        );
        fb.draw_hline(
            list_content_x,
            header_y + 19,
            list_content_w,
            Pixel::rgb(55, 55, 55),
        );

        let name_col = list_content_x + 12;
        let date_col = list_content_x + list_content_w as i32 - 275;
        let size_col = list_content_x + list_content_w as i32 - 180;
        let type_col = list_content_x + list_content_w as i32 - 100;
        let perms_col = list_content_x + list_content_w as i32 - 365;

        // Helper: draw sort arrow indicator after label
        let draw_sort_indicator = |fb: &mut FrameBuffer, x: i32, y: i32, ascending: bool| {
            let c = Pixel::rgb(0, 180, 220);
            if ascending {
                // ▲
                fb.draw_line_aa(x, y + 6, x + 3, y + 1, c);
                fb.draw_line_aa(x + 3, y + 1, x + 6, y + 6, c);
            } else {
                // ▼
                fb.draw_line_aa(x, y + 1, x + 3, y + 6, c);
                fb.draw_line_aa(x + 3, y + 6, x + 6, y + 1, c);
            }
        };

        // Name header
        let name_active = self.explorer_sort == ExplorerSort::Name;
        let name_c = if name_active {
            Pixel::rgb(0, 200, 220)
        } else {
            Pixel::rgb(160, 160, 160)
        };
        fonts::draw_string_bold_compact(fb, name_col, header_y + 4, "Name", name_c, 1);
        if name_active {
            draw_sort_indicator(fb, name_col + 32, header_y + 4, self.explorer_sort_asc);
        }

        // Perms header
        if list_content_w > 450 {
            fonts::draw_string_bold_compact(
                fb,
                perms_col,
                header_y + 4,
                "Perms",
                Pixel::rgb(160, 160, 160),
                1,
            );
        }

        // Date header
        if list_content_w > 350 {
            let date_active = self.explorer_sort == ExplorerSort::Date;
            let date_c = if date_active {
                Pixel::rgb(0, 200, 220)
            } else {
                Pixel::rgb(160, 160, 160)
            };
            fonts::draw_string_bold_compact(fb, date_col, header_y + 4, "Modified", date_c, 1);
            if date_active {
                draw_sort_indicator(fb, date_col + 52, header_y + 4, self.explorer_sort_asc);
            }
        }

        // Size header
        if list_content_w > 250 {
            let size_active = self.explorer_sort == ExplorerSort::Size;
            let size_c = if size_active {
                Pixel::rgb(0, 200, 220)
            } else {
                Pixel::rgb(160, 160, 160)
            };
            fonts::draw_string_bold_compact(fb, size_col, header_y + 4, "Size", size_c, 1);
            if size_active {
                draw_sort_indicator(fb, size_col + 28, header_y + 4, self.explorer_sort_asc);
            }
        }

        // Type header
        let type_active = self.explorer_sort == ExplorerSort::Type;
        let type_c = if type_active {
            Pixel::rgb(0, 200, 220)
        } else {
            Pixel::rgb(160, 160, 160)
        };
        fonts::draw_string_bold_compact(fb, type_col, header_y + 4, "Type", type_c, 1);
        if type_active {
            draw_sort_indicator(fb, type_col + 28, header_y + 4, self.explorer_sort_asc);
        }

        // === File/folder entries with scroll support ===
        let entries_y = content.y + nav_h as i32 + search_bar_h as i32 + 24;
        let status_h: u32 = 23;
        let list_h = content
            .height
            .saturating_sub(nav_h + 20 + status_h + search_bar_h);
        let item_h = 28i32;

        // Preview panel width (0 when hidden, ~40% when visible)
        let preview_w: u32 = if self.explorer_preview_visible {
            (list_content_w * 2 / 5)
                .max(200)
                .min(list_content_w.saturating_sub(300))
        } else {
            0
        };
        let list_w = list_content_w.saturating_sub(preview_w);

        let folder_color = Pixel::rgb(0, 131, 213);
        let file_color = Pixel::rgb(160, 180, 200);
        let symlink_color = Pixel::rgb(0, 200, 200);
        let device_color = Pixel::rgb(200, 200, 0);
        let exec_color = Pixel::rgb(88, 255, 0);
        let selected_bg = Pixel::new(0, 180, 220, 30);

        // Generate thumbnails for image files in grid view
        if self.explorer_grid_view {
            if self.thumbnail_cache_dir != current_path {
                self.thumbnail_cache.clear();
                self.thumbnail_cache_dir = current_path.clone();
            }
            // Lazily load thumbnails for visible entries (limit per frame to avoid stalls)
            let mut loads_this_frame = 0u32;
            let cols = (list_w.saturating_sub(20) / 90u32).max(1);
            let scroll_row_off = (self.scroll_y / 80i32).max(0) as usize;
            let visible_rows = (list_h / 80u32).max(1) as usize;
            let start_idx = scroll_row_off * cols as usize;
            let end_idx = ((scroll_row_off + visible_rows + 1) * cols as usize).min(entries.len());
            for entry in entries[start_idx..end_idx].iter() {
                if entry.is_dir || loads_this_frame >= 3 {
                    continue;
                }
                let lower = entry.name.to_ascii_lowercase();
                let is_img = lower.ends_with(".png")
                    || lower.ends_with(".jpg")
                    || lower.ends_with(".jpeg")
                    || lower.ends_with(".bmp");
                if !is_img {
                    continue;
                }
                let file_path = if current_path == "/" {
                    alloc::format!("/{}", entry.name)
                } else {
                    alloc::format!("{}/{}", current_path, entry.name)
                };
                if self.thumbnail_cache.contains_key(&file_path) {
                    continue;
                }
                // Load and decode
                if let Some(data) = crate::vfs::read_file_dispatch(&file_path) {
                    if let Some(img) = crate::gui::image::decode(&data) {
                        // Scale down to thumbnail: max 48x36
                        let thumb_max_w = 48u32;
                        let thumb_max_h = 36u32;
                        let scale = {
                            let sw = thumb_max_w as f32 / img.width.max(1) as f32;
                            let sh = thumb_max_h as f32 / img.height.max(1) as f32;
                            if sw < sh { sw } else { sh }
                        };
                        let tw = ((img.width as f32 * scale) as u32).max(1).min(thumb_max_w);
                        let th = ((img.height as f32 * scale) as u32).max(1).min(thumb_max_h);
                        // Convert pixels to BGRA byte buffer
                        let mut bgra = Vec::with_capacity((img.width * img.height * 4) as usize);
                        for p in &img.pixels {
                            bgra.push(p.b);
                            bgra.push(p.g);
                            bgra.push(p.r);
                            bgra.push(p.a);
                        }
                        self.thumbnail_cache
                            .insert(file_path, Some((bgra, img.width, img.height)));
                    } else {
                        self.thumbnail_cache.insert(file_path, None);
                    }
                } else {
                    self.thumbnail_cache.insert(file_path, None);
                }
                loads_this_frame += 1;
            }
        }

        let vfs = crate::vfs::VFS.lock();

        if self.explorer_grid_view {
            // ══════ GRID VIEW ══════
            let cell_w = 90u32;
            let cell_h = 80u32;
            let cols = (list_w.saturating_sub(20) / cell_w).max(1);
            let total_rows = (entries.len() as u32).div_ceil(cols);
            let visible_rows = (list_h / cell_h).max(1);
            let scroll_row_offset = (self.scroll_y / cell_h as i32).max(0) as u32;

            self.max_scroll_y = ((total_rows * cell_h) as i32 - list_h as i32).max(0);

            for (vi, entry) in entries.iter().enumerate() {
                let row = vi as u32 / cols;
                let col = vi as u32 % cols;
                if row < scroll_row_offset {
                    continue;
                }
                if row > scroll_row_offset + visible_rows {
                    break;
                }

                let cx = list_content_x + 8 + (col * cell_w) as i32;
                let cy = entries_y + ((row - scroll_row_offset) * cell_h) as i32;

                if cy + cell_h as i32 <= entries_y || cy >= entries_y + list_h as i32 {
                    continue;
                }

                let is_selected = self.explorer_selected == vi as i32;
                if is_selected {
                    fb.fill_rounded_rect_aa(
                        Rect::new(cx, cy, cell_w - 4, cell_h - 4),
                        selected_bg,
                        6,
                    );
                }

                // Icon (centered, larger) — or thumbnail for image files
                let icon_cx = cx + cell_w as i32 / 2 - 10;
                let icon_cy = cy + 8;
                if entry.is_dir {
                    fb.fill_rounded_rect_aa(Rect::new(icon_cx, icon_cy, 10, 5), folder_color, 1);
                    fb.fill_rounded_rect_aa(
                        Rect::new(icon_cx - 2, icon_cy + 4, 24, 18),
                        folder_color,
                        3,
                    );
                    fb.fill_rounded_rect_aa(
                        Rect::new(icon_cx - 2, icon_cy + 10, 24, 12),
                        colors::lighten(folder_color, 30),
                        3,
                    );
                } else {
                    // Check for cached thumbnail
                    let file_path = if current_path == "/" {
                        alloc::format!("/{}", entry.name)
                    } else {
                        alloc::format!("{}/{}", current_path, entry.name)
                    };
                    let mut drew_thumb = false;
                    if let Some(Some((bgra, src_w, src_h))) = self.thumbnail_cache.get(&file_path) {
                        // Scale to fit 48x36 area centered in the cell
                        let thumb_max_w = 48u32;
                        let thumb_max_h = 36u32;
                        let scale_w = thumb_max_w as f32 / (*src_w).max(1) as f32;
                        let scale_h = thumb_max_h as f32 / (*src_h).max(1) as f32;
                        let scale = if scale_w < scale_h { scale_w } else { scale_h };
                        let tw = ((*src_w as f32 * scale) as u32).max(1).min(thumb_max_w);
                        let th = ((*src_h as f32 * scale) as u32).max(1).min(thumb_max_h);
                        let tx = cx + (cell_w as i32 - tw as i32) / 2;
                        let ty = cy + 4;
                        // Draw a subtle border
                        fb.fill_rounded_rect_aa(
                            Rect::new(tx - 1, ty - 1, tw + 2, th + 2),
                            Pixel::rgb(60, 60, 64),
                            2,
                        );
                        fb.blit_bgra_scaled(tx, ty, tw, th, bgra, *src_w, *src_h);
                        drew_thumb = true;
                    }
                    if !drew_thumb {
                        fb.fill_rounded_rect_aa(
                            Rect::new(icon_cx, icon_cy, 20, 24),
                            Pixel::rgb(48, 52, 58),
                            3,
                        );
                        fb.fill_rect(
                            Rect::new(icon_cx + 3, icon_cy + 6, 14, 1),
                            Pixel::rgb(80, 90, 100),
                        );
                        fb.fill_rect(
                            Rect::new(icon_cx + 3, icon_cy + 10, 14, 1),
                            Pixel::rgb(70, 80, 90),
                        );
                        fb.fill_rect(
                            Rect::new(icon_cx + 3, icon_cy + 14, 10, 1),
                            Pixel::rgb(70, 80, 90),
                        );
                    }
                }

                // Name (centered, truncated)
                let max_name_chars = (cell_w / 6) as usize;
                let name_display = if entry.name.len() > max_name_chars {
                    alloc::format!("{}…", &entry.name[..max_name_chars.saturating_sub(1)])
                } else {
                    entry.name.clone()
                };
                let name_px = name_display.len() as i32 * 6;
                let name_x = cx + (cell_w as i32 - name_px) / 2;
                let name_color = if entry.is_dir {
                    Pixel::rgb(80, 180, 255)
                } else if entry.name.starts_with('.') {
                    Pixel::rgb(120, 120, 120)
                } else {
                    colors::WHITE
                };
                fonts::draw_string_compact(
                    fb,
                    name_x,
                    cy + cell_h as i32 - 22,
                    &name_display,
                    name_color,
                    1,
                );
            }
        } else {
            // ══════ LIST VIEW (original) ══════
            let visible_items = (list_h as i32 / item_h) as usize;
            let scroll_item_offset = (self.scroll_y / item_h.max(1)) as usize;

            for (vi, entry) in entries
                .iter()
                .enumerate()
                .skip(scroll_item_offset)
                .take(visible_items + 1)
            {
                let ey = entries_y + (vi as i32 - scroll_item_offset as i32) * item_h;
                if ey + item_h <= entries_y {
                    continue;
                }
                if ey >= entries_y + list_h as i32 {
                    break;
                }

                // Selected item highlight
                let is_selected = self.explorer_selected == vi as i32;
                if is_selected {
                    fb.fill_rounded_rect_aa(
                        Rect::new(content.x + 4, ey, list_w.saturating_sub(18), item_h as u32),
                        selected_bg,
                        4,
                    );
                } else if vi % 2 == 1 {
                    // Alternating row background
                    fb.fill_rounded_rect_aa(
                        Rect::new(content.x + 4, ey, list_w.saturating_sub(18), item_h as u32),
                        Pixel::new(255, 255, 255, 6),
                        4,
                    );
                }

                // Draw icons (same as before)
                if entry.is_dir {
                    let ix = content.x + 10;
                    let iy = ey + 4;
                    fb.fill_rounded_rect_aa(Rect::new(ix, iy, 7, 4), folder_color, 1);
                    fb.fill_rounded_rect_aa(Rect::new(ix, iy + 3, 18, 13), folder_color, 2);
                    fb.fill_rounded_rect_aa(
                        Rect::new(ix, iy + 7, 18, 9),
                        colors::lighten(folder_color, 30),
                        2,
                    );
                } else if entry.kind == "Symlink" {
                    let sym_c = symlink_color;
                    fb.fill_rounded_rect_aa(
                        Rect::new(content.x + 12, ey + 3, 14, 16),
                        Pixel::rgb(35, 38, 42),
                        2,
                    );
                    fb.draw_line_aa(content.x + 12, ey + 5, content.x + 12, ey + 18, sym_c);
                    fb.draw_line_aa(content.x + 12, ey + 18, content.x + 25, ey + 18, sym_c);
                    fb.draw_line_aa(content.x + 25, ey + 5, content.x + 25, ey + 18, sym_c);
                    fb.draw_line_aa(content.x + 12, ey + 5, content.x + 21, ey + 5, sym_c);
                    fb.draw_line_aa(content.x + 16, ey + 14, content.x + 22, ey + 10, sym_c);
                    fb.draw_line_aa(content.x + 22, ey + 10, content.x + 19, ey + 10, sym_c);
                    fb.draw_line_aa(content.x + 22, ey + 10, content.x + 22, ey + 13, sym_c);
                } else if entry.kind == "Char Device" || entry.kind == "Block Device" {
                    fb.fill_rounded_rect_aa(
                        Rect::new(content.x + 11, ey + 3, 16, 16),
                        Pixel::rgb(40, 40, 30),
                        3,
                    );
                    fb.fill_circle_aa(content.x + 19, ey + 11, 5, Pixel::rgb(50, 50, 35));
                    fb.fill_circle_aa(content.x + 19, ey + 11, 3, device_color);
                    fb.fill_circle_aa(content.x + 19, ey + 11, 1, Pixel::rgb(40, 40, 30));
                } else {
                    let ix = content.x + 12;
                    let iy = ey + 3;
                    fb.fill_rounded_rect_aa(Rect::new(ix, iy, 14, 17), Pixel::rgb(48, 52, 58), 2);
                    fb.fill_rect(Rect::new(ix + 2, iy + 4, 10, 11), Pixel::rgb(42, 46, 52));
                    let fold_x = ix + 10;
                    let fold_y = iy;
                    fb.fill_rect(Rect::new(fold_x, fold_y, 4, 4), Pixel::rgb(60, 64, 70));
                    fb.draw_line_aa(fold_x, fold_y, fold_x, fold_y + 4, file_color);
                    fb.draw_line_aa(fold_x, fold_y + 4, fold_x + 4, fold_y, file_color);
                    fb.fill_rect(Rect::new(ix + 3, iy + 6, 7, 1), Pixel::rgb(80, 90, 100));
                    fb.fill_rect(Rect::new(ix + 3, iy + 9, 8, 1), Pixel::rgb(70, 80, 90));
                    fb.fill_rect(Rect::new(ix + 3, iy + 12, 5, 1), Pixel::rgb(70, 80, 90));
                }

                // Name text — check if in rename mode
                let is_renaming = self.explorer_renaming == Some(vi);

                if is_renaming {
                    // Rename mode: draw editable text field
                    let rename_x = content.x + 36;
                    let rename_w = 200u32.min(content.width.saturating_sub(50));
                    fb.fill_rounded_rect_aa(
                        Rect::new(rename_x - 2, ey + 4, rename_w, 18),
                        Pixel::rgb(20, 20, 20),
                        3,
                    );
                    fb.draw_rounded_rect(
                        Rect::new(rename_x - 2, ey + 4, rename_w, 18),
                        Pixel::rgb(0, 200, 220),
                        3,
                        1,
                    );
                    fonts::draw_string_compact(
                        fb,
                        rename_x + 2,
                        ey + 8,
                        &self.explorer_rename_buf,
                        Pixel::rgb(240, 240, 240),
                        1,
                    );
                    // Cursor
                    let cur_x = rename_x + 2 + self.explorer_rename_buf.len() as i32 * 6;
                    fb.fill_rect(Rect::new(cur_x, ey + 6, 1, 14), Pixel::rgb(0, 200, 220));
                } else {
                    // Normal name rendering with color by type
                    let name_color = if entry.is_dir {
                        Pixel::rgb(80, 180, 255)
                    } else if entry.kind == "Symlink" {
                        Pixel::rgb(0, 220, 220)
                    } else if entry.kind == "Char Device" || entry.kind == "Block Device" {
                        Pixel::rgb(220, 220, 0)
                    } else if entry.kind == "FIFO" || entry.kind == "Socket" {
                        Pixel::rgb(220, 0, 220)
                    } else if entry.name.starts_with('.') {
                        Pixel::rgb(120, 120, 120)
                    } else {
                        // Check if executable
                        if entry.permissions & 0o111 != 0 {
                            exec_color
                        } else {
                            colors::WHITE
                        }
                    };

                    let max_name_w = if content.width > 450 {
                        (perms_col - name_col - 36) as u32
                    } else if content.width > 350 {
                        (date_col - name_col - 36) as u32
                    } else if content.width > 250 {
                        (size_col - name_col - 36) as u32
                    } else {
                        (type_col - name_col - 36) as u32
                    };
                    let display_name = fonts::truncate_with_ellipsis(&entry.name, max_name_w, 1);
                    fonts::draw_string_compact(
                        fb,
                        content.x + 36,
                        ey + 8,
                        &display_name,
                        name_color,
                        1,
                    );
                }

                // Permissions column
                if content.width > 450 {
                    let perms_str = Self::format_permissions(entry.permissions, entry.is_dir);
                    fonts::draw_string_compact(
                        fb,
                        perms_col,
                        ey + 8,
                        &perms_str,
                        Pixel::rgb(100, 100, 100),
                        1,
                    );
                }

                // Date modified column
                if content.width > 350 && !entry.date_display.is_empty() {
                    fonts::draw_string_compact(
                        fb,
                        date_col,
                        ey + 8,
                        &entry.date_display,
                        Pixel::rgb(110, 115, 120),
                        1,
                    );
                }

                // Size
                if content.width > 250 && !entry.size.is_empty() {
                    fonts::draw_string_compact(
                        fb,
                        size_col,
                        ey + 8,
                        &entry.size,
                        Pixel::rgb(120, 120, 120),
                        1,
                    );
                }

                // Type
                fonts::draw_string_compact(
                    fb,
                    type_col,
                    ey + 8,
                    &entry.kind,
                    Pixel::rgb(120, 120, 120),
                    1,
                );
            }
        } // end list/grid view branch

        drop(vfs);

        // === Scrollbar ===
        let scroll_track_x = content.x + list_w as i32 - 10;
        let scroll_track_y = entries_y;
        let scroll_track_h = list_h;
        fb.fill_rounded_rect_aa(
            Rect::new(scroll_track_x, scroll_track_y, 8, scroll_track_h),
            colors::SCROLLBAR_TRACK,
            4,
        );
        let total_items_h = entries.len() as u32 * item_h as u32;
        self.max_scroll_y = (total_items_h as i32 - scroll_track_h as i32).max(0);
        if total_items_h > scroll_track_h {
            let visible_ratio = scroll_track_h as f32 / total_items_h as f32;
            let thumb_h = ((visible_ratio * scroll_track_h as f32) as u32).max(20);
            let scroll_ratio = self.scroll_y as f32 / (total_items_h - scroll_track_h) as f32;
            let thumb_y =
                scroll_track_y + (scroll_ratio.min(1.0) * (scroll_track_h - thumb_h) as f32) as i32;
            fb.fill_rounded_rect_aa(
                Rect::new(scroll_track_x + 1, thumb_y, 6, thumb_h),
                colors::SCROLLBAR_THUMB,
                3,
            );
        } else {
            let thumb_h = scroll_track_h.clamp(10, 20);
            fb.fill_rounded_rect_aa(
                Rect::new(scroll_track_x + 1, scroll_track_y + 2, 6, thumb_h),
                colors::SCROLLBAR_THUMB,
                3,
            );
        }

        // === Preview panel (right side, when visible) ===
        if self.explorer_preview_visible && preview_w > 0 {
            let pv_x = content.x + list_w as i32;
            let pv_y = content.y + nav_h as i32 + search_bar_h as i32;
            let pv_h = content
                .height
                .saturating_sub(nav_h + status_h + search_bar_h);

            // Separator line
            fb.draw_vline(pv_x, pv_y, pv_h, Pixel::rgb(60, 60, 60));

            // Preview background
            fb.fill_rect(
                Rect::new(pv_x + 1, pv_y, preview_w.saturating_sub(1), pv_h),
                Pixel::rgb(22, 22, 26),
            );

            // Preview header bar (24px)
            let pv_header_h: u32 = 24;
            fb.fill_rect(
                Rect::new(pv_x + 1, pv_y, preview_w.saturating_sub(1), pv_header_h),
                Pixel::rgb(35, 35, 40),
            );
            fb.draw_hline(
                pv_x + 1,
                pv_y + pv_header_h as i32,
                preview_w.saturating_sub(1),
                Pixel::rgb(55, 55, 60),
            );

            // Title: filename or "No Preview"
            if !self.explorer_preview_path.is_empty() {
                // Extract filename from path
                let fname = self
                    .explorer_preview_path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&self.explorer_preview_path);
                let max_title_w = preview_w.saturating_sub(20);
                let title = fonts::truncate_with_ellipsis(fname, max_title_w, 1);
                fonts::draw_string_bold_compact(
                    fb,
                    pv_x + 8,
                    pv_y + 6,
                    &title,
                    Pixel::rgb(200, 210, 220),
                    1,
                );
            } else {
                fonts::draw_string_compact(
                    fb,
                    pv_x + 8,
                    pv_y + 6,
                    "No file selected",
                    Pixel::rgb(100, 100, 110),
                    1,
                );
            }

            // Preview content area
            let pv_content_y = pv_y + pv_header_h as i32 + 4;
            let pv_content_h = pv_h.saturating_sub(pv_header_h + 8);

            if self.explorer_preview_content.is_empty() {
                // Empty state
                if !self.explorer_preview_path.is_empty() {
                    fonts::draw_string_compact(
                        fb,
                        pv_x + 12,
                        pv_content_y + 8,
                        "(empty file)",
                        Pixel::rgb(80, 80, 90),
                        1,
                    );
                } else {
                    fonts::draw_string_compact(
                        fb,
                        pv_x + 12,
                        pv_content_y + 8,
                        "Press Ctrl+P to toggle",
                        Pixel::rgb(80, 80, 90),
                        1,
                    );
                    fonts::draw_string_compact(
                        fb,
                        pv_x + 12,
                        pv_content_y + 24,
                        "Select a file to preview",
                        Pixel::rgb(80, 80, 90),
                        1,
                    );
                }
            } else {
                // Render text content line by line
                let max_chars = ((preview_w.saturating_sub(24)) / 6) as usize; // 6px per char compact
                let line_h = 14i32; // compact line height
                let max_lines = (pv_content_h as i32 / line_h) as usize;
                let mut line_idx = 0usize;

                // Line number gutter width
                let gutter_w = 30i32;
                let text_x = pv_x + 8 + gutter_w;
                let gutter_x = pv_x + 4;

                for line in self.explorer_preview_content.lines() {
                    if line_idx >= max_lines {
                        break;
                    }
                    let ly = pv_content_y + (line_idx as i32) * line_h;

                    // Line number
                    let ln_str = alloc::format!("{:>3}", line_idx + 1);
                    fonts::draw_string_compact(
                        fb,
                        gutter_x,
                        ly,
                        &ln_str,
                        Pixel::rgb(70, 75, 85),
                        1,
                    );

                    // Truncate long lines
                    let max_text_chars = max_chars.saturating_sub(5);
                    let display_line = if line.len() > max_text_chars {
                        &line[..max_text_chars]
                    } else {
                        line
                    };
                    fonts::draw_string_compact(
                        fb,
                        text_x,
                        ly,
                        display_line,
                        Pixel::rgb(190, 195, 205),
                        1,
                    );

                    line_idx += 1;
                }

                // Show truncation indicator if file content was truncated
                if self.explorer_preview_content.len() >= 2040 {
                    let trunc_y =
                        pv_content_y + (line_idx.min(max_lines.saturating_sub(1)) as i32) * line_h;
                    if line_idx < max_lines {
                        fonts::draw_string_compact(
                            fb,
                            pv_x + 12,
                            trunc_y,
                            "--- truncated ---",
                            Pixel::rgb(100, 100, 60),
                            1,
                        );
                    }
                }
            }
        }

        // === Status bar (23px) ===
        let status_h_i = status_h as i32;
        let status_y = content.y + content.height as i32 - status_h_i;
        fb.fill_rect(
            Rect::new(content.x, status_y, content.width, status_h),
            Pixel::rgb(40, 40, 40),
        );
        fb.draw_hline(content.x, status_y, content.width, Pixel::rgb(60, 60, 60));

        // Item count + selection info
        let count_text = if self.explorer_search_active && !self.explorer_search_query.is_empty() {
            if self.explorer_selected >= 0 {
                alloc::format!(
                    "{} matches • selected: {}",
                    entries.len(),
                    self.explorer_selected + 1
                )
            } else {
                alloc::format!(
                    "{} matches for \"{}\"",
                    entries.len(),
                    self.explorer_search_query
                )
            }
        } else if self.explorer_selected >= 0 {
            alloc::format!(
                "{} items • selected: {}",
                entries.len(),
                self.explorer_selected + 1
            )
        } else {
            alloc::format!("{} items", entries.len())
        };
        fonts::draw_string_compact(
            fb,
            content.x + 8,
            status_y + 5,
            &count_text,
            Pixel::rgb(150, 150, 150),
            1,
        );

        // Hidden files indicator
        if self.explorer_show_hidden {
            fonts::draw_string_compact(
                fb,
                content.x + 8 + count_text.len() as i32 * 6 + 12,
                status_y + 5,
                "[hidden]",
                Pixel::rgb(100, 180, 200),
                1,
            );
        }

        // Preview indicator
        if self.explorer_preview_visible {
            let preview_x = content.x
                + 8
                + count_text.len() as i32 * 6
                + 12
                + if self.explorer_show_hidden {
                    8 * 6 + 8
                } else {
                    0
                };
            fonts::draw_string_compact(
                fb,
                preview_x,
                status_y + 5,
                "[preview]",
                Pixel::rgb(180, 140, 255),
                1,
            );
        }

        // Grid/list view indicator
        if self.explorer_grid_view {
            let grid_x = content.x
                + 8
                + count_text.len() as i32 * 6
                + 12
                + if self.explorer_show_hidden {
                    8 * 6 + 8
                } else {
                    0
                }
                + if self.explorer_preview_visible {
                    9 * 6 + 8
                } else {
                    0
                };
            fonts::draw_string_compact(
                fb,
                grid_x,
                status_y + 5,
                "[grid]",
                Pixel::rgb(140, 200, 140),
                1,
            );
        }

        // Path in status bar right side
        let path_short = if current_path.len() > 40 {
            alloc::format!("...{}", &current_path[current_path.len() - 37..])
        } else {
            current_path.clone()
        };
        let path_px_w = path_short.len() as u32 * 8;
        if content.width > path_px_w + 100 {
            fonts::draw_string_compact(
                fb,
                content.x + content.width as i32 - path_px_w as i32 - 16,
                status_y + 5,
                &path_short,
                Pixel::rgb(100, 100, 100),
                1,
            );
        }

        // === Context menu overlay ===
        if let Some(ref ctx) = self.explorer_ctx_menu {
            let menu_w = 180u32;
            let item_h_menu = 28i32;
            let menu_h = ctx.items.len() as u32 * item_h_menu as u32 + 8;

            // Menu background with shadow
            fb.fill_rounded_rect_aa(
                Rect::new(ctx.x + 2, ctx.y + 2, menu_w, menu_h),
                Pixel::new(0, 0, 0, 60),
                8,
            );
            fb.fill_rounded_rect_aa(
                Rect::new(ctx.x, ctx.y, menu_w, menu_h),
                Pixel::rgb(35, 38, 42),
                8,
            );
            fb.draw_rounded_rect(
                Rect::new(ctx.x, ctx.y, menu_w, menu_h),
                Pixel::rgb(60, 65, 75),
                8,
                1,
            );

            for (i, item) in ctx.items.iter().enumerate() {
                let iy = ctx.y + 4 + i as i32 * item_h_menu;
                let text_color = if item.enabled {
                    Pixel::rgb(220, 220, 220)
                } else {
                    Pixel::rgb(80, 80, 80)
                };
                fonts::draw_string_compact(fb, ctx.x + 12, iy + 7, &item.label, text_color, 1);

                // Show keyboard shortcut hints
                let shortcut = match item.action {
                    ExplorerAction::Copy => Some("Ctrl+C"),
                    ExplorerAction::Cut => Some("Ctrl+X"),
                    ExplorerAction::Paste => Some("Ctrl+V"),
                    ExplorerAction::Delete => Some("Del"),
                    ExplorerAction::Rename => Some("F2"),
                    ExplorerAction::ToggleHidden => Some("Ctrl+H"),
                    _ => None,
                };
                if let Some(sc) = shortcut {
                    let sc_w = sc.len() as i32 * 6;
                    fonts::draw_string_compact(
                        fb,
                        ctx.x + menu_w as i32 - sc_w - 12,
                        iy + 7,
                        sc,
                        Pixel::rgb(90, 95, 100),
                        1,
                    );
                }
            }
        }
    }

    /// Format a file size in human-readable form (B, KB, MB, GB)
    fn format_size(bytes: u64) -> alloc::string::String {
        if bytes == 0 {
            alloc::string::String::from("0 B")
        } else if bytes < 1024 {
            alloc::format!("{} B", bytes)
        } else if bytes < 1024 * 1024 {
            let kb = bytes as f64 / 1024.0;
            if kb < 10.0 {
                alloc::format!("{:.1} KB", kb)
            } else {
                alloc::format!("{} KB", bytes / 1024)
            }
        } else if bytes < 1024 * 1024 * 1024 {
            let mb = bytes as f64 / (1024.0 * 1024.0);
            if mb < 10.0 {
                alloc::format!("{:.1} MB", mb)
            } else {
                alloc::format!("{} MB", bytes / (1024 * 1024))
            }
        } else {
            let gb = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
            alloc::format!("{:.1} GB", gb)
        }
    }

    /// Format Unix permissions as rwxrwxrwx string
    fn format_permissions(mode: u16, is_dir: bool) -> alloc::string::String {
        let mut s = alloc::string::String::with_capacity(10);
        s.push(if is_dir { 'd' } else { '-' });
        s.push(if mode & 0o400 != 0 { 'r' } else { '-' });
        s.push(if mode & 0o200 != 0 { 'w' } else { '-' });
        s.push(if mode & 0o100 != 0 { 'x' } else { '-' });
        s.push(if mode & 0o040 != 0 { 'r' } else { '-' });
        s.push(if mode & 0o020 != 0 { 'w' } else { '-' });
        s.push(if mode & 0o010 != 0 { 'x' } else { '-' });
        s.push(if mode & 0o004 != 0 { 'r' } else { '-' });
        s.push(if mode & 0o002 != 0 { 'w' } else { '-' });
        s.push(if mode & 0o001 != 0 { 'x' } else { '-' });
        s
    }

    /// Detect file type from filename extension
    fn file_type_from_name(name: &str) -> alloc::string::String {
        if let Some(dot_pos) = name.rfind('.') {
            let ext = &name[dot_pos + 1..];
            match ext {
                "txt" | "text" | "log" => alloc::string::String::from("Text"),
                "md" | "markdown" => alloc::string::String::from("Markdown"),
                "rs" => alloc::string::String::from("Rust"),
                "c" | "h" => alloc::string::String::from("C Source"),
                "cpp" | "cc" | "cxx" | "hpp" => alloc::string::String::from("C++"),
                "py" => alloc::string::String::from("Python"),
                "js" => alloc::string::String::from("JavaScript"),
                "ts" => alloc::string::String::from("TypeScript"),
                "html" | "htm" => alloc::string::String::from("HTML"),
                "css" => alloc::string::String::from("CSS"),
                "json" => alloc::string::String::from("JSON"),
                "xml" => alloc::string::String::from("XML"),
                "yaml" | "yml" => alloc::string::String::from("YAML"),
                "toml" => alloc::string::String::from("TOML"),
                "ini" | "cfg" | "conf" => alloc::string::String::from("Config"),
                "sh" | "bash" | "zsh" => alloc::string::String::from("Script"),
                "png" => alloc::string::String::from("PNG Image"),
                "jpg" | "jpeg" => alloc::string::String::from("JPEG Image"),
                "gif" => alloc::string::String::from("GIF Image"),
                "svg" => alloc::string::String::from("SVG Image"),
                "bmp" => alloc::string::String::from("Bitmap"),
                "pdf" => alloc::string::String::from("PDF"),
                "zip" | "gz" | "bz2" | "xz" | "tar" | "tgz" => {
                    alloc::string::String::from("Archive")
                }
                "o" | "a" | "so" | "dylib" => alloc::string::String::from("Binary"),
                "elf" | "bin" => alloc::string::String::from("Executable"),
                "deb" | "rpm" => alloc::string::String::from("Package"),
                _ => alloc::format!(".{} File", ext),
            }
        } else {
            alloc::string::String::from("File")
        }
    }
}
