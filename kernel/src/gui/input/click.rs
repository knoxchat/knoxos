/// Click, double-click, right-click, and middle-click paste handlers
use crate::gui::desktop;
use crate::gui::startmenu;
use crate::gui::taskbar;
use crate::gui::window;
use crate::gui::window::{ViewKind, WindowContentType};

/// Handle right-click (context menu)
pub(super) fn handle_right_click(x: i32, y: i32, screen_w: u32, screen_h: u32) {
    let taskbar_y = screen_h as i32 - crate::gui::scale::taskbar_height() as i32;

    // D9.6: Alt-Tab / overview consume the click (dismiss on right-click).
    if crate::gui::gpui_platform::switcher_is_live()
        && (crate::gui::expose::is_visible() || crate::gui::alt_tab::is_visible())
    {
        let _ = crate::gui::gpui_platform::pointer_down_button(x, y, knox_gpui::MouseButton::Right);
        crate::gui::request_redraw();
        return;
    }

    // D9.5: tray icons and their menus are GPUI panel children.
    if crate::gui::gpui_platform::panel_is_live()
        && crate::gui::gpui_platform::pointer_down_button(x, y, knox_gpui::MouseButton::Right)
    {
        crate::gui::gpui_platform::damage_proof();
        crate::gui::request_taskbar_redraw();
        return;
    }

    // On taskbar — show taskbar context menu (legacy dock only).
    if !crate::gui::gpui_platform::panel_is_live() && y >= taskbar_y {
        crate::gui::taskbar::show_context_menu(x, y);
        return;
    }

    // Check if right-clicking inside a file explorer window
    let wm = window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.window_at(x, y) {
        if let Some(win) = wm.windows.iter().rev().find(|w| w.id == wid) {
            if win.content_type == WindowContentType::Gpui(ViewKind::Files) {
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

    if crate::gui::gpui_platform::pointer_down_button(x, y, knox_gpui::MouseButton::Right) {
        crate::gui::gpui_platform::damage_proof();
        return;
    }

    // Show desktop context menu
    desktop::show_context_menu(x, y);
}

/// Close / maximize / minimize — must run before resize-edge grab.
/// Hit-tests chrome rects directly (not `window_at`) so a click on the X
/// still wins when the pointer is in the top-right resize grab.
fn handle_chrome_button_click(x: i32, y: i32, screen_w: u32, screen_h: u32) -> bool {
    let wm = window::WINDOW_MANAGER.lock();
    let mut hit: Option<(window::WindowId, u8)> = None;
    for win in wm.windows.iter().rev() {
        if !win.is_visible() {
            continue;
        }
        // D10.5: CSD buttons are GPUI hitboxes; keep WM resize edges.
        if win.uses_csd() {
            continue;
        }
        if win.closeable && win.close_button_rect().contains(x, y) {
            hit = Some((win.id, 0));
            break;
        }
        if win.maximizable && win.maximize_button_rect().contains(x, y) {
            hit = Some((win.id, 1));
            break;
        }
        if win.minimizable && win.minimize_button_rect().contains(x, y) {
            hit = Some((win.id, 2));
            break;
        }
    }
    let Some((wid, action)) = hit else {
        return false;
    };
    let Some(win) = wm.windows.iter().rev().find(|w| w.id == wid) else {
        return false;
    };
    let close = action == 0;
    let maximize = action == 1;

    let old_rect = win.rect;
    let id = win.id;
    let is_terminal = win.content_type == WindowContentType::TerminalSurface;
    let is_browser = win.content_type == WindowContentType::Gpui(ViewKind::Browser);
    let is_ai = win.content_type == WindowContentType::Gpui(ViewKind::Chat);
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
        crate::gui::request_redraw();
        return true;
    }

    if maximize {
        crate::gui::request_window_redraw(old_rect);
        window::WINDOW_MANAGER
            .lock()
            .toggle_maximize(id, screen_w, screen_h);
        crate::gui::damage_window(id);
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
    crate::gui::request_window_redraw(old_rect);
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
    // If login screen is active, route clicks there
    if !crate::gui::login::is_logged_in() {
        if crate::gui::gpui_platform::session_is_live() {
            let _ =
                crate::gui::gpui_platform::session_pointer_down(x, y, knox_gpui::MouseButton::Left);
        } else {
            crate::gui::login::handle_click(x, y, screen_w as i32, screen_h as i32);
        }
        crate::gui::request_redraw();
        return;
    }

    // If screen is locked, route clicks to lock screen
    if crate::gui::lock_screen::is_locked() {
        if crate::gui::gpui_platform::session_is_live() {
            let _ =
                crate::gui::gpui_platform::session_pointer_down(x, y, knox_gpui::MouseButton::Left);
        } else {
            crate::gui::lock_screen::handle_click(x, y, screen_w as i32, screen_h as i32);
        }
        crate::gui::request_redraw();
        return;
    }

    // Check context menu first (if visible, clicking an item)
    if crate::gui::gpui_platform::desktop_is_live() {
        if crate::gui::gpui_platform::desktop_menu_open() {
            if crate::gui::gpui_platform::pointer_down(x, y) {
                crate::gui::request_redraw();
                return;
            }
        }
    } else if desktop::handle_context_menu_click(x, y) {
        crate::gui::request_redraw();
        return;
    }

    // Alt-Tab / Mission Control — intercepts clicks when visible (D9.6 GPUI).
    if crate::gui::expose::is_visible() || crate::gui::alt_tab::is_visible() {
        if crate::gui::gpui_platform::switcher_is_live() {
            let _ = crate::gui::gpui_platform::pointer_down(x, y);
            crate::gui::request_redraw();
            return;
        }
        if crate::gui::expose::is_visible() {
            if let Some(wid) = crate::gui::expose::handle_click(x, y) {
                let mut wm = window::WINDOW_MANAGER.lock();
                wm.focus_window(wid);
                drop(wm);
                taskbar::set_active(wid);
            }
            crate::gui::request_redraw();
            return;
        }
    }

    // Check taskbar context menu
    if taskbar::handle_context_menu_click(x, y) {
        crate::gui::request_redraw();
        return;
    }

    // File picker / screenshot / installer Dialog overlays (D11.10).
    if crate::gui::gpui_platform::dialogs_is_live()
        && (crate::gui::file_picker::is_visible()
            || crate::gui::screenshot::is_dialog_open()
            || crate::gui::installer::is_active())
    {
        let _ = crate::gui::gpui_platform::dialogs_pointer_down(x, y, knox_gpui::MouseButton::Left);
        crate::gui::request_redraw();
        return;
    }
    if crate::gui::file_picker::handle_click(x, y) {
        crate::gui::request_redraw();
        return;
    }
    if crate::gui::installer::is_active() {
        crate::gui::installer::handle_click(
            x as usize,
            y as usize,
            screen_w as usize,
            screen_h as usize,
        );
        crate::gui::request_redraw();
        return;
    }

    // Keyboard shortcuts overlay — intercepts clicks when visible
    if crate::gui::shortcuts_overlay::handle_click(x, y) {
        crate::gui::request_redraw();
        return;
    }

    // D9.4 / D9.5: toast, notification-center, and tray-menu clicks are GPUI.

    // Panel / launcher / desktop-icon GPUI hits. Returning here used to skip
    // desktop icons because they were dispatched only at the bottom of this
    // function; they now live in `pointer_down_button`.
    let taskbar_y = screen_h as i32 - crate::gui::scale::taskbar_height() as i32;
    if crate::gui::gpui_platform::panel_is_live() || crate::gui::gpui_platform::launcher_is_live() {
        if crate::gui::gpui_platform::pointer_down(x, y) {
            crate::gui::gpui_platform::damage_proof();
            crate::gui::request_taskbar_redraw();
            return;
        }
    } else if y >= taskbar_y && taskbar::handle_click(x, y, screen_w, screen_h) {
        crate::gui::request_taskbar_redraw();
        return;
    }

    // Legacy start menu only. GPUI launcher hits are consumed in `pointer_down`
    // (including press-to-open); outside clicks close there too.
    if startmenu::is_visible() && !crate::gui::gpui_platform::launcher_is_live() {
        if let Some(item) = startmenu::handle_click(x, y) {
            crate::serial_println!("[KnoxOS] Start menu: Opening {}", item.name);
            desktop::open_application(&item.name, item.icon_type);
            return;
        } else {
            startmenu::close();
        }
    }

    // D9.3 / D9.4: OSD flyouts and the notification center are GPUI overlays
    // dispatched in `gpui_platform::pointer_down`. Outside clicks close there.

    // Title-bar buttons must win over the top-right resize grab zone.
    if handle_chrome_button_click(x, y, screen_w, screen_h) {
        return;
    }

    // Check for resize edge on any visible window (before checking interior clicks)
    {
        let wm = window::WINDOW_MANAGER.lock();
        let (resize_wid, edge) = wm.resize_edge_at(x, y);
        if let Some(wid) = resize_wid {
            if edge.is_resizing() {
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
    let wm = window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.window_at(x, y) {
        crate::gui::gpui_platform::release_key_focus();
        drop(wm);

        let mut wm = window::WINDOW_MANAGER.lock();
        wm.focus_window(wid);

        // Check title bar buttons (using scale-aware button rects)
        if let Some(win) = wm.windows.iter().find(|w| w.id == wid) {
            let close_rect = win.close_button_rect();
            if !win.uses_csd() && win.closeable && close_rect.contains(x, y) {
                let id = win.id;
                let old_rect = win.rect;
                let is_terminal = win.content_type == WindowContentType::TerminalSurface;
                let is_browser = win.content_type == WindowContentType::Gpui(ViewKind::Browser);
                let is_ai = win.content_type == WindowContentType::Gpui(ViewKind::Chat);
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
                crate::gui::request_window_redraw(old_rect);
                return;
            }
            if !win.uses_csd() && win.maximizable && win.maximize_button_rect().contains(x, y) {
                let id = win.id;
                let old_rect = win.rect;
                drop(wm);
                crate::gui::request_window_redraw(old_rect);
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
            if !win.uses_csd() && win.minimizable && win.minimize_button_rect().contains(x, y) {
                let id = win.id;
                let old_rect = win.rect;
                drop(wm);
                crate::gui::window_events::push_event(
                    id,
                    crate::gui::window_events::WindowEvent::Minimized,
                );
                let mut wm = window::WINDOW_MANAGER.lock();
                wm.minimize_window(id);
                crate::gui::request_window_redraw(old_rect);
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
            if win.content_type == WindowContentType::Gpui(ViewKind::Settings) {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    if crate::gui::gpui_platform::settings_is_live() {
                        crate::gui::gpui_platform::settings_pointer_down(
                            x,
                            y,
                            knox_gpui::MouseButton::Left,
                        );
                    } else {
                        handle_settings_content_click(x, y, wid);
                    }
                    taskbar::set_active(wid);
                    crate::gui::damage_window(wid);
                    return;
                }
            }

            // Check for clicks on AI assistant content
            if win.content_type == WindowContentType::Gpui(ViewKind::Chat) {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    if crate::gui::gpui_platform::chat_is_live() {
                        crate::gui::gpui_platform::chat_pointer_down(
                            x,
                            y,
                            knox_gpui::MouseButton::Left,
                        );
                    } else {
                        crate::gui::ai_assistant::handle_click(wid, x, y);
                    }
                    taskbar::set_active(wid);
                    crate::gui::damage_window(wid);
                    return;
                }
            }

            // Check for clicks on browser window content
            if win.content_type == WindowContentType::Gpui(ViewKind::Browser) {
                let content = win.content_rect();
                if content.contains(x, y) {
                    let scroll = win.scroll_y;
                    drop(wm);
                    if crate::gui::gpui_platform::browse_is_live() {
                        crate::gui::gpui_platform::browse_pointer_down(
                            x,
                            y,
                            knox_gpui::MouseButton::Left,
                        );
                    } else {
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
                    }
                    taskbar::set_active(wid);
                    crate::gui::damage_window(wid);
                    return;
                }
            }

            // Check for clicks on file explorer content
            if win.content_type == WindowContentType::Gpui(ViewKind::Files) {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    if crate::gui::gpui_platform::files_is_live() {
                        crate::gui::gpui_platform::files_pointer_down(
                            x,
                            y,
                            knox_gpui::MouseButton::Left,
                        );
                    } else {
                        crate::gui::explorer::handle_click(wid, x, y);
                    }
                    taskbar::set_active(wid);
                    crate::gui::damage_window(wid);
                    return;
                }
            }

            let utility_live = crate::gui::gpui_platform::apps_is_live()
                && matches!(
                    win.content_type,
                    WindowContentType::Gpui(ViewKind::Archive)
                        | WindowContentType::Gpui(ViewKind::Disks)
                        | WindowContentType::Gpui(ViewKind::Bluetooth)
                        | WindowContentType::Gpui(ViewKind::Calendar)
                        | WindowContentType::Gpui(ViewKind::Logs)
                        | WindowContentType::Gpui(ViewKind::Updater)
                        | WindowContentType::Gpui(ViewKind::Software)
                        | WindowContentType::Gpui(ViewKind::Image)
                );
            if utility_live {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::gpui_platform::apps_pointer_down(
                        x,
                        y,
                        knox_gpui::MouseButton::Left,
                    );
                    taskbar::set_active(wid);
                    crate::gui::damage_window(wid);
                    return;
                }
            }

            // Check for clicks on archive viewer content
            if win.content_type == WindowContentType::Gpui(ViewKind::Archive) {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::archive_manager::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    crate::gui::damage_window(wid);
                    return;
                }
            }

            // Check for clicks on disk utility content
            if win.content_type == WindowContentType::Gpui(ViewKind::Disks) {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::disk_utility::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    crate::gui::damage_window(wid);
                    return;
                }
            }

            // Check for clicks on bluetooth manager content
            if win.content_type == WindowContentType::Gpui(ViewKind::Bluetooth) {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::bt_manager::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    crate::gui::damage_window(wid);
                    return;
                }
            }

            // Check for clicks on calendar app content
            if win.content_type == WindowContentType::Gpui(ViewKind::Calendar) {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::calendar_app::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    crate::gui::damage_window(wid);
                    return;
                }
            }

            // Check for clicks on log viewer content
            if win.content_type == WindowContentType::Gpui(ViewKind::Logs) {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::log_viewer::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    crate::gui::damage_window(wid);
                    return;
                }
            }

            // Check for clicks on software updater content
            if win.content_type == WindowContentType::Gpui(ViewKind::Updater) {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::software_updater::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    crate::gui::damage_window(wid);
                    return;
                }
            }

            // Check for clicks on software center content
            if win.content_type == WindowContentType::Gpui(ViewKind::Software) {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::software_center::handle_click(wid, x, y);
                    taskbar::set_active(wid);
                    crate::gui::damage_window(wid);
                    return;
                }
            }

            // Check for clicks on setup wizard content
            if win.content_type == WindowContentType::Gpui(ViewKind::Wizard) {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    if crate::gui::gpui_platform::dialogs_is_live() {
                        crate::gui::gpui_platform::dialogs_pointer_down(
                            x,
                            y,
                            knox_gpui::MouseButton::Left,
                        );
                    } else {
                        crate::gui::setup_wizard::handle_click(wid, x, y);
                    }
                    taskbar::set_active(wid);
                    crate::gui::damage_window(wid);
                    return;
                }
            }

            // Check for clicks on task manager content
            if win.content_type == WindowContentType::Gpui(ViewKind::Tasks) {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    if crate::gui::gpui_platform::tasks_is_live() {
                        crate::gui::gpui_platform::tasks_pointer_down(
                            x,
                            y,
                            knox_gpui::MouseButton::Left,
                        );
                    } else {
                        crate::gui::task_manager::handle_click(wid, content, x, y);
                    }
                    taskbar::set_active(wid);
                    crate::gui::damage_window(wid);
                    return;
                }
            }

            if win.content_type == WindowContentType::Gpui(ViewKind::Editor) {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    if crate::gui::gpui_platform::editor_is_live() {
                        crate::gui::gpui_platform::editor_pointer_down(
                            x,
                            y,
                            knox_gpui::MouseButton::Left,
                        );
                    }
                    taskbar::set_active(wid);
                    crate::gui::damage_window(wid);
                    return;
                }
            }

            // Check for clicks on calculator content
            if win.content_type == WindowContentType::Gpui(ViewKind::Calculator) {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    if crate::gui::gpui_platform::calc_is_live() {
                        crate::gui::gpui_platform::calc_pointer_down(
                            x,
                            y,
                            knox_gpui::MouseButton::Left,
                        );
                    } else {
                        crate::gui::calculator::handle_click(wid, content, x, y);
                    }
                    taskbar::set_active(wid);
                    crate::gui::damage_window(wid);
                    return;
                }
            }

            // Check for clicks on image viewer content
            if win.content_type == WindowContentType::Gpui(ViewKind::Image) {
                let content = win.content_rect();
                if content.contains(x, y) {
                    drop(wm);
                    crate::gui::image_viewer::handle_click(wid, content, x, y);
                    taskbar::set_active(wid);
                    crate::gui::damage_window(wid);
                    return;
                }
            }

            // ── Terminal chrome (GPUI) / Ctrl+Click URL / fallback tab bar ──
            if win.content_type == WindowContentType::TerminalSurface {
                let content = win.content_rect();
                let gpui = crate::gui::gpui_platform::term_is_live();
                let tab_bar_h: i32 = if gpui {
                    crate::gui::gpui_platform::TERM_CHROME_H
                } else if win.terminal_tabs.len() > 1 {
                    26
                } else {
                    0
                };
                if gpui && content.contains(x, y) && y < content.y + tab_bar_h {
                    drop(wm);
                    crate::gui::gpui_platform::term_pointer_down(
                        x,
                        y,
                        knox_gpui::MouseButton::Left,
                    );
                    taskbar::set_active(wid);
                    crate::gui::damage_window(wid);
                    return;
                }
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
                    crate::gui::damage_window(wid);
                    return;
                }
            }

            // Check for clicks on terminal tab bar
            if win.content_type == WindowContentType::TerminalSurface
                && !crate::gui::gpui_platform::term_is_live()
                && win.terminal_tabs.len() > 1
            {
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
            if win.content_type != WindowContentType::TerminalSurface
                && win.content_type != WindowContentType::ShmEmpty
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

    // Knox GPUI proof window (not a WM client). Consume so desktop icons
    // and rubber-band do not steal the click. VirtIO-tablet absolute
    // coordinates arrive here via `set_absolute_mouse` (D6.1).
    if crate::gui::gpui_platform::pointer_down(x, y) {
        crate::gui::gpui_platform::damage_proof();
        return;
    }

    crate::gui::gpui_platform::release_key_focus();

    // D9.7: desktop icons / rubber-band start are a GPUI layer.
    if crate::gui::gpui_platform::desktop_is_live() {
        if crate::gui::gpui_platform::desktop_pointer_down(x, y, knox_gpui::MouseButton::Left) {
            crate::gui::request_redraw();
            return;
        }
    }

    // Click on desktop (no window found)
    desktop::handle_click(x, y);
}

/// Handle double click
pub(super) fn handle_double_click(x: i32, y: i32, screen_w: u32, screen_h: u32) {
    // File picker modal — intercept double clicks when visible
    if crate::gui::gpui_platform::dialogs_is_live() && crate::gui::file_picker::is_visible() {
        crate::gui::file_picker::confirm();
        crate::gui::request_redraw();
        return;
    }
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
                if win.content_type == WindowContentType::Gpui(ViewKind::Files) {
                    let content = win.content_rect();
                    if content.contains(x, y) {
                        drop(wm);
                        if crate::gui::gpui_platform::files_is_live() {
                            crate::gui::explorer::activate_selected(wid);
                        } else {
                            crate::gui::explorer::handle_double_click(wid, x, y);
                        }
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
            if win.content_type == WindowContentType::TerminalSurface {
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
