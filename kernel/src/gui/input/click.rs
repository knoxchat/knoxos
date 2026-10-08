/// Click, double-click, right-click, and middle-click paste handlers
use crate::gui::desktop;
use crate::gui::startmenu;
use crate::gui::taskbar;
use crate::gui::window;
use crate::gui::window::WindowContentType;

/// Handle right-click (context menu)
pub(super) fn handle_right_click(x: i32, y: i32, screen_w: u32, screen_h: u32) {
    let taskbar_y = screen_h as i32 - crate::gui::scale::taskbar_height() as i32;

    // Close any open tray context menu first
    if crate::gui::system_tray::is_context_menu_open()
        && crate::gui::system_tray::handle_context_menu_click(x, y)
    {
        return;
    }

    // On taskbar — check if right-clicking on a tray icon
    if y >= taskbar_y {
        if let Some((tray_x, dock_cy)) = crate::gui::taskbar::tray_layout(screen_w, screen_h) {
            if crate::gui::system_tray::handle_right_click(x, y, tray_x, dock_cy) {
                return;
            }
        }
        // Right-click elsewhere on taskbar — show taskbar context menu
        crate::gui::taskbar::show_context_menu(x, y);
        return;
    }

    // Check if right-clicking inside a file explorer window
    let wm = window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.window_at(x, y) {
        if let Some(win) = wm.windows.iter().rev().find(|w| w.id == wid) {
            if win.content_type == WindowContentType::FileExplorer {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::explorer::handle_right_click(wid, x, y);
                    crate::gui::request_redraw();
                    return;
                }
            }
        }
        drop(wm);
        return; // Don't show desktop context menu on other windows
    }
    drop(wm);

    // Show desktop context menu
    desktop::show_context_menu(x, y);
}

/// Close / maximize / minimize — must run before resize-edge grab.
fn handle_chrome_button_click(x: i32, y: i32, screen_w: u32, screen_h: u32) -> bool {
    let wm = window::WINDOW_MANAGER.lock();
    let Some(wid) = wm.window_at(x, y) else {
        return false;
    };
    let Some(win) = wm.windows.iter().rev().find(|w| w.id == wid) else {
        return false;
    };

    let close = win.closeable && win.close_button_rect().contains(x, y);
    let maximize = win.maximizable && win.maximize_button_rect().contains(x, y);
    let minimize = win.minimizable && win.minimize_button_rect().contains(x, y);
    if !close && !maximize && !minimize {
        return false;
    }

    let id = win.id;
    let is_terminal = win.content_type == WindowContentType::Terminal;
    let is_browser = win.content_type == WindowContentType::Browser;
    let is_ai = win.content_type == WindowContentType::AIAssistant;
    let tab_ids: alloc::vec::Vec<u32> = win.terminal_tabs.clone();
    drop(wm);

    if close {
        crate::gui::window_events::push_event(
            id,
            crate::gui::window_events::WindowEvent::CloseRequested,
        );
        window::WINDOW_MANAGER.lock().close_window(id);
        crate::gui::window_events::unregister_window(id);
        taskbar::remove_entry(id);
        if is_terminal {
            crate::terminal::destroy_for_window(id);
            for tab_id in &tab_ids {
                if *tab_id != id {
                    crate::terminal::destroy_tab(*tab_id);
                }
            }
        }
        if is_browser {
            crate::gui::browser::destroy_for_window(id);
        }
        if is_ai {
            crate::gui::ai_assistant::destroy_for_window(id);
        }
        return true;
    }

    if maximize {
        window::WINDOW_MANAGER
            .lock()
            .toggle_maximize(id, screen_w, screen_h);
        let wm2 = window::WINDOW_MANAGER.lock();
        if let Some(w) = wm2.windows.iter().find(|w| w.id == id) {
            let evt = match w.state {
                window::WindowState::Maximized => crate::gui::window_events::WindowEvent::Maximized,
                _ => crate::gui::window_events::WindowEvent::Restored,
            };
            crate::gui::window_events::push_event(id, evt);
        }
        return true;
    }

    // minimize
    crate::gui::window_events::push_event(id, crate::gui::window_events::WindowEvent::Minimized);
    let mut wm = window::WINDOW_MANAGER.lock();
    wm.minimize_window(id);
    let new_focus = wm.focused_window;
    drop(wm);
    if let Some(fid) = new_focus {
        taskbar::set_active(fid);
    } else {
        let mut tb = taskbar::TASKBAR.lock();
        for e in tb.entries.iter_mut() {
            e.active = false;
        }
    }
    true
}

/// Handle a single click
pub(super) fn handle_click(x: i32, y: i32, screen_w: u32, screen_h: u32) {
    crate::serial_println!(
        "[CLICK-DBG] handle_click({},{}) screen={}x{}",
        x,
        y,
        screen_w,
        screen_h
    );
    // If login screen is active, route clicks there
    if !crate::gui::login::is_logged_in() {
        crate::serial_println!("[CLICK-DBG] consumed by: login");
        crate::gui::login::handle_click(x, y, screen_w as i32, screen_h as i32);
        return;
    }

    // If screen is locked, route clicks to lock screen
    if crate::gui::lock_screen::is_locked() {
        crate::serial_println!("[CLICK-DBG] consumed by: lock_screen");
        crate::gui::lock_screen::handle_click(x, y, screen_w as i32, screen_h as i32);
        return;
    }

    // Check context menu first (if visible, clicking an item)
    if desktop::handle_context_menu_click(x, y) {
        crate::serial_println!("[CLICK-DBG] consumed by: context_menu");
        return;
    }

    // Exposé / Mission Control — intercepts clicks when visible
    if crate::gui::expose::is_visible() {
        crate::serial_println!("[CLICK-DBG] consumed by: expose");
        if let Some(wid) = crate::gui::expose::handle_click(x, y) {
            let mut wm = window::WINDOW_MANAGER.lock();
            wm.focus_window(wid);
            drop(wm);
            taskbar::set_active(wid);
        }
        crate::gui::request_redraw();
        return;
    }

    // Check taskbar context menu
    if taskbar::handle_context_menu_click(x, y) {
        crate::serial_println!("[CLICK-DBG] consumed by: taskbar_context_menu");
        return;
    }

    // File picker modal — intercepts all clicks when visible
    if crate::gui::file_picker::handle_click(x, y) {
        crate::serial_println!("[CLICK-DBG] consumed by: file_picker");
        return;
    }

    // Keyboard shortcuts overlay — intercepts clicks when visible
    if crate::gui::shortcuts_overlay::handle_click(x, y) {
        crate::serial_println!("[CLICK-DBG] consumed by: shortcuts_overlay");
        return;
    }

    // Check tray context menu
    if crate::gui::system_tray::is_context_menu_open()
        && crate::gui::system_tray::handle_context_menu_click(x, y)
    {
        crate::serial_println!("[CLICK-DBG] consumed by: tray_context_menu");
        return;
    }

    // Check toast notification clicks (top-right toasts)
    if crate::gui::notifications::handle_toast_click(x, y, screen_w as i32) {
        crate::serial_println!("[CLICK-DBG] consumed by: toast");
        return;
    }

    // Check notification panel clicks
    if crate::gui::notifications::is_panel_open()
        && crate::gui::notifications::handle_panel_click(x, y, screen_w as i32, screen_h as i32)
    {
        crate::serial_println!("[CLICK-DBG] consumed by: notification_panel");
        return;
    }

    // Check taskbar clicks BEFORE start menu, so clicking the start button
    // properly toggles the menu (instead of close→reopen race)
    let taskbar_y = screen_h as i32 - crate::gui::scale::taskbar_height() as i32;
    if y >= taskbar_y && taskbar::handle_click(x, y, screen_w, screen_h) {
        crate::serial_println!("[CLICK-DBG] consumed by: taskbar");
        return;
    }

    // Check start menu (if visible, clicking a menu item or outside to close)
    if startmenu::is_visible() {
        if let Some(item) = startmenu::handle_click(x, y) {
            crate::serial_println!("[KnoxOS] Start menu: Opening {}", item.name);
            desktop::open_application(&item.name, item.icon_type);
            return;
        }
        // If clicking outside the menu (and not on taskbar which was handled above), close it
        startmenu::close();
    }

    // Check system popups (calendar, volume, quick settings)
    if crate::gui::popups::any_popup_open() {
        let sw = screen_w as i32;
        let sh = screen_h as i32;
        if crate::gui::popups::handle_calendar_click(x, y, sw, sh) {
            crate::serial_println!("[CLICK-DBG] consumed by: calendar_popup");
            return;
        }
        if crate::gui::popups::handle_volume_click(x, y, sw, sh) {
            crate::serial_println!("[CLICK-DBG] consumed by: volume_popup");
            return;
        }
        if crate::gui::popups::handle_quick_settings_click(x, y, sw, sh) {
            crate::serial_println!("[CLICK-DBG] consumed by: quick_settings_popup");
            return;
        }
        // Click outside all popups — close them
        crate::gui::popups::close_all_popups();
    }

    // Check notification panel — clicking INSIDE the panel is handled by the
    // earlier handle_panel_click call.  Clicking OUTSIDE closes the panel but
    // lets the click fall through to the underlying element (consistent with
    // start menu / popup behavior).
    {
        let mut nc = crate::gui::notifications::NOTIFICATIONS.lock();
        if nc.panel_open {
            let panel_w: u32 = 340;
            let panel_x = screen_w as i32 - panel_w as i32 - 8;
            let panel_rect = crate::gui::framebuffer::Rect::new(panel_x, 36, panel_w, 400);
            if panel_rect.contains(x, y) {
                // Click inside panel — already handled above; consume
                crate::serial_println!("[CLICK-DBG] consumed by: notification_panel_inside");
                drop(nc);
                return;
            }
            // Click outside — close panel and fall through
            nc.panel_open = false;
        }
    }

    // Title-bar buttons must win over the top-right resize grab zone.
    if handle_chrome_button_click(x, y, screen_w, screen_h) {
        crate::serial_println!("[CLICK-DBG] consumed by: chrome_button");
        return;
    }

    // Check for resize edge on any visible window (before checking interior clicks)
    {
        let wm = window::WINDOW_MANAGER.lock();
        let (resize_wid, edge) = wm.resize_edge_at(x, y);
        if let Some(wid) = resize_wid {
            if edge.is_resizing() {
                crate::serial_println!(
                    "[CLICK-DBG] consumed by: resize_edge wid={} edge={:?}",
                    wid,
                    edge
                );
                drop(wm);
                let mut wm = window::WINDOW_MANAGER.lock();
                wm.focus_window(wid);
                if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                    win.resizing = edge;
                }
                drop(wm);
                taskbar::set_active(wid);
                return;
            }
        }
    }

    // Check windows (top to bottom z-order)
    crate::serial_println!("[CLICK-DBG] reached window check at ({},{})", x, y);
    let wm = window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.window_at(x, y) {
        drop(wm);

        let mut wm = window::WINDOW_MANAGER.lock();
        wm.focus_window(wid);

        // Check title bar buttons (using scale-aware button rects)
        if let Some(win) = wm.windows.iter().find(|w| w.id == wid) {
            let close_rect = win.close_button_rect();
            crate::serial_println!(
                "[HIT] wid={} click=({},{}) close_rect=({},{},{}x{}) closeable={}",
                wid,
                x,
                y,
                close_rect.x,
                close_rect.y,
                close_rect.width,
                close_rect.height,
                win.closeable
            );
            if win.closeable && close_rect.contains(x, y) {
                let id = win.id;
                let is_terminal = win.content_type == WindowContentType::Terminal;
                let is_browser = win.content_type == WindowContentType::Browser;
                let is_ai = win.content_type == WindowContentType::AIAssistant;
                // Collect terminal tab IDs before closing
                let tab_ids: alloc::vec::Vec<u32> = win.terminal_tabs.clone();
                drop(wm);
                // Emit CloseRequested event
                crate::gui::window_events::push_event(
                    id,
                    crate::gui::window_events::WindowEvent::CloseRequested,
                );
                window::WINDOW_MANAGER.lock().close_window(id);
                // Clean up event queue
                crate::gui::window_events::unregister_window(id);
                taskbar::remove_entry(id);
                // Clean up terminal instance if this was a terminal window
                if is_terminal {
                    crate::terminal::destroy_for_window(id);
                    // Also destroy any additional tab terminals
                    for tab_id in &tab_ids {
                        if *tab_id != id {
                            crate::terminal::destroy_tab(*tab_id);
                        }
                    }
                }
                // Clean up browser instance if this was a browser window
                if is_browser {
                    crate::gui::browser::destroy_for_window(id);
                }
                // Clean up AI assistant state if this was an AI window
                if is_ai {
                    crate::gui::ai_assistant::destroy_for_window(id);
                }
                return;
            }
            if win.maximizable && win.maximize_button_rect().contains(x, y) {
                let id = win.id;
                drop(wm);
                window::WINDOW_MANAGER
                    .lock()
                    .toggle_maximize(id, screen_w, screen_h);
                // Emit Maximized/Restored event
                let wm2 = window::WINDOW_MANAGER.lock();
                if let Some(w) = wm2.windows.iter().find(|w| w.id == id) {
                    let evt = match w.state {
                        window::WindowState::Maximized => {
                            crate::gui::window_events::WindowEvent::Maximized
                        }
                        _ => crate::gui::window_events::WindowEvent::Restored,
                    };
                    crate::gui::window_events::push_event(id, evt);
                }
                return;
            }
            if win.minimizable && win.minimize_button_rect().contains(x, y) {
                let id = win.id;
                drop(wm);
                crate::gui::window_events::push_event(
                    id,
                    crate::gui::window_events::WindowEvent::Minimized,
                );
                let mut wm = window::WINDOW_MANAGER.lock();
                wm.minimize_window(id);
                // Sync taskbar active state with newly focused window
                let new_focus = wm.focused_window;
                drop(wm);
                if let Some(fid) = new_focus {
                    taskbar::set_active(fid);
                } else {
                    // No windows left visible — deactivate all entries
                    let mut tb = taskbar::TASKBAR.lock();
                    for e in tb.entries.iter_mut() {
                        e.active = false;
                    }
                }
                return;
            }

            // Check for clicks on window content (settings tabs, etc.)
            if win.content_type == WindowContentType::Settings {
                let content = win.content_rect();
                crate::serial_println!(
                    "[CLICK-DBG] Settings content check: mouse=({},{}) rect=({},{},{}x{})",
                    x,
                    y,
                    content.x,
                    content.y,
                    content.width,
                    content.height
                );
                if content.contains(x, y) {
                    drop(wm);
                    handle_settings_content_click(x, y, wid);
                    taskbar::set_active(wid);
                    return;
                }
            }

            // Check for clicks on AI assistant content
            if win.content_type == WindowContentType::AIAssistant {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::ai_assistant::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    crate::gui::request_redraw();
                    return;
                }
            }

            // Check for clicks on browser window content
            if win.content_type == WindowContentType::Browser {
                let content = win.content_rect();
                if content.contains(x, y) {
                    let scroll = win.scroll_y;
                    drop(wm);
                    crate::gui::browser::handle_browser_click(
                        wid,
                        x,
                        y,
                        content.x,
                        content.y,
                        content.width,
                        content.height,
                        scroll,
                    );
                    taskbar::set_active(wid);
                    crate::gui::request_redraw();
                    return;
                }
            }

            // Check for clicks on file explorer content
            if win.content_type == WindowContentType::FileExplorer {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::explorer::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    crate::gui::request_redraw();
                    return;
                }
            }

            // Check for clicks on archive viewer content
            if win.content_type == WindowContentType::ArchiveViewer {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::archive_manager::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    crate::gui::request_redraw();
                    return;
                }
            }

            // Check for clicks on disk utility content
            if win.content_type == WindowContentType::DiskUtility {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::disk_utility::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    crate::gui::request_redraw();
                    return;
                }
            }

            // Check for clicks on bluetooth manager content
            if win.content_type == WindowContentType::BluetoothManager {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::bt_manager::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    crate::gui::request_redraw();
                    return;
                }
            }

            // Check for clicks on calendar app content
            if win.content_type == WindowContentType::CalendarApp {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::calendar_app::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    crate::gui::request_redraw();
                    return;
                }
            }

            // Check for clicks on log viewer content
            if win.content_type == WindowContentType::LogViewer {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::log_viewer::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    crate::gui::request_redraw();
                    return;
                }
            }

            // Check for clicks on software updater content
            if win.content_type == WindowContentType::SoftwareUpdater {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::software_updater::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    crate::gui::request_redraw();
                    return;
                }
            }

            // Check for clicks on software center content
            if win.content_type == WindowContentType::SoftwareCenter {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::software_center::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    crate::gui::request_redraw();
                    return;
                }
            }

            // Check for clicks on setup wizard content
            if win.content_type == WindowContentType::SetupWizard {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::setup_wizard::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    crate::gui::request_redraw();
                    return;
                }
            }

            // Check for clicks on task manager content
            if win.content_type == WindowContentType::TaskManager {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::task_manager::handle_click(wid, content, x, y);
                    taskbar::set_active(wid);
                    crate::gui::request_redraw();
                    return;
                }
            }

            // Check for clicks on calculator content
            if win.content_type == WindowContentType::Calculator {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::calculator::handle_click(wid, content, x, y);
                    taskbar::set_active(wid);
                    crate::gui::request_redraw();
                    return;
                }
            }

            // Check for clicks on image viewer content
            if win.content_type == WindowContentType::ImageViewer {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::image_viewer::handle_click(wid, content, x, y);
                    taskbar::set_active(wid);
                    crate::gui::request_redraw();
                    return;
                }
            }

            // ── Ctrl+Click on terminal content → open URL ──
            if win.content_type == WindowContentType::Terminal {
                let content = win.content_rect();
                let tab_bar_h: i32 = if win.terminal_tabs.len() > 1 { 26 } else { 0 };
                let term_area = crate::gui::framebuffer::Rect::new(
                    content.x,
                    content.y + tab_bar_h,
                    content.width,
                    content.height.saturating_sub(tab_bar_h as u32),
                );
                if term_area.contains(x, y) && crate::task::keyboard::is_ctrl_held() {
                    let term_id = if !win.terminal_tabs.is_empty() {
                        win.terminal_tabs
                            .get(win.terminal_active_tab)
                            .copied()
                            .unwrap_or(wid)
                    } else {
                        wid
                    };
                    drop(wm);
                    crate::terminal::handle_ctrl_click(term_id, x, y, term_area);
                    crate::gui::request_redraw();
                    return;
                }
            }

            // Check for clicks on terminal tab bar
            if win.content_type == WindowContentType::Terminal && win.terminal_tabs.len() > 1 {
                let content = win.content_rect();
                let tab_bar_h = 26i32;
                if y >= content.y && y < content.y + tab_bar_h {
                    let tab_w =
                        140i32.min(content.width as i32 / win.terminal_tabs.len().max(1) as i32);
                    let rel_x = x - content.x;

                    // Check "+" button
                    let plus_x = (win.terminal_tabs.len() as i32) * tab_w;
                    if rel_x >= plus_x && rel_x < plus_x + 24 {
                        // New tab
                        let wid_copy = wid;
                        let tab_idx = win.terminal_tabs.len();
                        drop(wm);
                        let tab_id = crate::terminal::create_tab(wid_copy, tab_idx);
                        let mut wm2 = window::WINDOW_MANAGER.lock();
                        if let Some(w) = wm2.windows.iter_mut().find(|w| w.id == wid_copy) {
                            w.terminal_tabs.push(tab_id);
                            w.terminal_active_tab = tab_idx;
                        }
                        drop(wm2);
                        crate::gui::request_redraw();
                        return;
                    }

                    // Check tab clicks
                    let tab_idx = (rel_x / tab_w) as usize;
                    if tab_idx < win.terminal_tabs.len() {
                        // Check close button area (last 20px of tab)
                        let tab_local_x = rel_x - (tab_idx as i32 * tab_w);
                        if tab_local_x >= tab_w - 20 && win.terminal_tabs.len() > 1 {
                            // Close this tab
                            let tab_id = win.terminal_tabs[tab_idx];
                            let wid_copy = wid;
                            drop(wm);
                            crate::terminal::destroy_tab(tab_id);
                            let mut wm2 = window::WINDOW_MANAGER.lock();
                            if let Some(w) = wm2.windows.iter_mut().find(|w| w.id == wid_copy) {
                                w.terminal_tabs.retain(|t| *t != tab_id);
                                if w.terminal_active_tab >= w.terminal_tabs.len() {
                                    w.terminal_active_tab = w.terminal_tabs.len().saturating_sub(1);
                                }
                            }
                            drop(wm2);
                        } else {
                            // Switch to this tab
                            let wid_mut = wid;
                            drop(wm);
                            let mut wm2 = window::WINDOW_MANAGER.lock();
                            if let Some(w) = wm2.windows.iter_mut().find(|w| w.id == wid_mut) {
                                w.terminal_active_tab = tab_idx;
                            }
                            drop(wm2);
                        }
                        crate::gui::request_redraw();
                        return;
                    }
                }
            }

            // Check for scrollbar thumb click — start scrollbar drag
            if win.content_type != WindowContentType::Terminal
                && win.content_type != WindowContentType::Empty
            {
                let content = win.content_rect();
                let sb_w = 12i32; // hit target wider than visual 6-8px
                let sb_x = content.x + content.width as i32 - sb_w;
                let sb_top = content.y;
                let sb_bot = content.y + content.height as i32;
                if x >= sb_x && x <= sb_x + sb_w && y >= sb_top && y <= sb_bot {
                    let max_sc = win.max_scroll_y;
                    let id = win.id;
                    let track_h = content.height as i32;
                    drop(wm);
                    if max_sc > 0 && track_h > 0 {
                        let mut wm2 = window::WINDOW_MANAGER.lock();
                        if let Some(w) = wm2.windows.iter_mut().find(|w| w.id == id) {
                            // Jump scroll to the click position on the track
                            let click_ratio = (y - sb_top) as f32 / track_h as f32;
                            let new_scroll = (click_ratio * max_sc as f32) as i32;
                            w.scroll_y = new_scroll.clamp(0, max_sc);
                            // Start drag from this new position
                            w.scrollbar_dragging = true;
                            w.scrollbar_drag_start_y = y;
                            w.scrollbar_drag_start_scroll = w.scroll_y;
                        }
                        drop(wm2);
                        crate::gui::request_redraw();
                    }
                    taskbar::set_active(wid);
                    return;
                }
            }

            // Start dragging if on title bar but NOT on any button
            let on_button = (win.closeable && win.close_button_rect().contains(x, y))
                || (win.maximizable && win.maximize_button_rect().contains(x, y))
                || (win.minimizable && win.minimize_button_rect().contains(x, y));
            if win.hit_test_titlebar(x, y) && !on_button {
                let win_state = win.state;
                drop(wm);
                let mut wm = window::WINDOW_MANAGER.lock();
                if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                    // If maximized or snapped, un-maximize/unsnap on drag start
                    // and reposition so the title bar follows the cursor
                    if win_state == window::WindowState::Maximized {
                        win.state = window::WindowState::Normal;
                        win.rect = win.saved_rect;
                        // Center the title bar under the cursor
                        win.rect.x = x - win.rect.width as i32 / 2;
                        win.rect.y = y.max(0);
                    } else if win_state == window::WindowState::SnappedLeft
                        || win_state == window::WindowState::SnappedRight
                        || win_state == window::WindowState::SnappedTopLeft
                        || win_state == window::WindowState::SnappedTopRight
                        || win_state == window::WindowState::SnappedBottomLeft
                        || win_state == window::WindowState::SnappedBottomRight
                    {
                        win.state = window::WindowState::Normal;
                        win.rect = win.pre_snap_rect;
                        win.rect.x = x - win.rect.width as i32 / 2;
                        win.rect.y = y.max(0);
                    }
                    win.dragging = true;
                    win.drag_offset_x = x - win.rect.x;
                    win.drag_offset_y = y - win.rect.y;
                }
            }
        }

        taskbar::set_active(wid);
        return;
    }

    drop(wm);

    // Click on desktop (no window found)
    crate::serial_println!("[CLICK] no window at ({},{}), desktop click", x, y);
    desktop::handle_click(x, y);
}

/// Handle double click
pub(super) fn handle_double_click(x: i32, y: i32, screen_w: u32, screen_h: u32) {
    // File picker modal — intercept double clicks when visible
    if crate::gui::file_picker::handle_double_click(x, y) {
        return;
    }

    let taskbar_y = screen_h as i32 - crate::gui::scale::taskbar_height() as i32;
    if y < taskbar_y {
        // Check for double-click on a window title bar → toggle maximize
        let wm = window::WINDOW_MANAGER.lock();
        if let Some(wid) = wm.window_at(x, y) {
            if let Some(win) = wm.windows.iter().rev().find(|w| w.id == wid) {
                // If double-clicking on a button, treat as single click (fire button action)
                let on_button = (win.closeable && win.close_button_rect().contains(x, y))
                    || (win.maximizable && win.maximize_button_rect().contains(x, y))
                    || (win.minimizable && win.minimize_button_rect().contains(x, y));
                if on_button {
                    drop(wm);
                    handle_click(x, y, screen_w, screen_h);
                    return;
                }
                // Only toggle maximize on title bar double-click (not on buttons)
                if win.maximizable && win.hit_test_titlebar(x, y) {
                    drop(wm);
                    window::WINDOW_MANAGER
                        .lock()
                        .toggle_maximize(wid, screen_w, screen_h);
                    return;
                }
                // Double-click in file explorer content → navigate/open
                if win.content_type == WindowContentType::FileExplorer {
                    let content = win.content_rect();
                    if content.contains(x, y) {
                        drop(wm);
                        crate::gui::explorer::handle_double_click(wid, x, y);
                        crate::gui::request_redraw();
                        return;
                    }
                }
                // For other content types (Settings, Browser, etc.),
                // treat double-click as a single click so the content
                // handler receives it.
                drop(wm);
                handle_click(x, y, screen_w, screen_h);
                return;
            }
        }
        drop(wm);
        desktop::handle_double_click(x, y);
    }
}

/// Handle middle-click paste — inserts clipboard contents at the focused widget
pub(super) fn handle_middle_click_paste(x: i32, y: i32) {
    // Check if a terminal window is focused — paste into it
    let wm = window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.window_at(x, y) {
        if let Some(win) = wm.windows.iter().find(|w| w.id == wid) {
            if win.content_type == WindowContentType::Terminal {
                let term_id = if !win.terminal_tabs.is_empty() {
                    win.terminal_tabs
                        .get(win.terminal_active_tab)
                        .copied()
                        .unwrap_or(wid)
                } else {
                    wid
                };
                drop(wm);
                // Read clipboard and paste into terminal as key events
                if let Some(clip) = crate::clipboard::paste_text() {
                    for ch in clip.chars() {
                        crate::terminal::handle_key_for_window(
                            term_id,
                            crate::terminal::TerminalKey::Char(ch),
                        );
                    }
                }
                return;
            }
        }
    }
    drop(wm);
    // For other contexts, log the paste attempt
    if crate::clipboard::has_text() {
        crate::serial_println!("[INPUT] Middle-click paste (non-terminal context)");
    }
}

/// Handle click on settings window content (tab switching)
fn handle_settings_content_click(x: i32, y: i32, wid: window::WindowId) {
    let wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter().find(|w| w.id == wid) {
        let content = win.content_rect();
        let scroll_y = win.scroll_y;
        drop(wm);
        let mut state = crate::gui::settings::SETTINGS_STATE.lock();
        crate::gui::settings::handle_settings_click(x, y, content, &mut state, scroll_y);
    }
}
