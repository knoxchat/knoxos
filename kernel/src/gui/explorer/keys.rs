/// Keyboard shortcuts for the explorer
use alloc::format;

use crate::gui::event_types::KeyCode;
use crate::gui::window::{self, ExplorerAction, WindowContentType, WindowId};

use super::actions::{
    cancel_rename, confirm_rename, execute_action, open_file_in_editor, rename_backspace,
};
use super::address::{
    address_bar_backspace, cancel_address_edit, confirm_address_edit, start_address_edit,
};
use super::entries::{read_entries, sort_entries};
use super::nav::{navigate_to, navigate_up};
use super::path::current_path;
use super::preview::{is_previewable, load_preview, toggle_grid_view, toggle_preview};
use super::search::{cancel_search, search_backspace, start_search};

/// Handle keyboard shortcuts for the explorer
pub fn handle_key(wid: WindowId, key: KeyCode, ctrl: bool, shift: bool) -> bool {
    let wm = window::WINDOW_MANAGER.lock();
    let win = match wm.windows.iter().find(|w| w.id == wid) {
        Some(w) => w,
        None => return false,
    };

    if win.content_type != WindowContentType::FileExplorer {
        return false;
    }

    // If editing address bar, route keys there
    if win.explorer_editing_path {
        drop(wm);
        match key {
            KeyCode::Enter => confirm_address_edit(wid),
            KeyCode::Escape => cancel_address_edit(wid),
            KeyCode::Backspace => address_bar_backspace(wid),
            _ => {}
        }
        return true;
    }

    // If search mode is active, route keys to search bar
    if win.explorer_search_active {
        drop(wm);
        match key {
            KeyCode::Escape => cancel_search(wid),
            KeyCode::Backspace => search_backspace(wid),
            KeyCode::Enter => {
                // Enter while searching: close search and open selected entry
                cancel_search(wid);
            }
            _ => {}
        }
        return true;
    }

    // If renaming, route keys there
    if win.explorer_renaming.is_some() {
        drop(wm);
        match key {
            KeyCode::Enter => confirm_rename(wid),
            KeyCode::Escape => cancel_rename(wid),
            KeyCode::Backspace => rename_backspace(wid),
            _ => {}
        }
        return true;
    }

    let selected = win.explorer_selected;
    let cur_path = current_path(&win.title);
    let show_hidden = win.explorer_show_hidden;
    let sort = win.explorer_sort;
    let sort_asc = win.explorer_sort_asc;
    drop(wm);

    match key {
        KeyCode::ArrowUp => {
            let mut wm = window::WINDOW_MANAGER.lock();
            if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                if win.explorer_selected > 0 {
                    win.explorer_selected -= 1;
                }
                // Auto-load preview if visible
                if win.explorer_preview_visible {
                    let new_sel = win.explorer_selected;
                    drop(wm);
                    let mut entries = read_entries(&cur_path, show_hidden);
                    sort_entries(&mut entries, sort, sort_asc);
                    if new_sel >= 0 {
                        if let Some(entry) = entries.get(new_sel as usize) {
                            if !entry.is_dir && is_previewable(&entry.name) {
                                let full = if cur_path == "/" {
                                    format!("/{}", entry.name)
                                } else {
                                    format!("{}/{}", cur_path, entry.name)
                                };
                                load_preview(wid, &full);
                            } else {
                                let mut wm2 = window::WINDOW_MANAGER.lock();
                                if let Some(w) = wm2.windows.iter_mut().find(|w| w.id == wid) {
                                    w.explorer_preview_content.clear();
                                    w.explorer_preview_path.clear();
                                }
                            }
                        }
                    }
                }
            }
            true
        }
        KeyCode::ArrowDown => {
            let entries = read_entries(&cur_path, show_hidden);
            let mut wm = window::WINDOW_MANAGER.lock();
            if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                if (win.explorer_selected + 1) < entries.len() as i32 {
                    win.explorer_selected += 1;
                }
                // Auto-load preview if visible
                if win.explorer_preview_visible {
                    let new_sel = win.explorer_selected;
                    drop(wm);
                    let mut sorted = read_entries(&cur_path, show_hidden);
                    sort_entries(&mut sorted, sort, sort_asc);
                    if new_sel >= 0 {
                        if let Some(entry) = sorted.get(new_sel as usize) {
                            if !entry.is_dir && is_previewable(&entry.name) {
                                let full = if cur_path == "/" {
                                    format!("/{}", entry.name)
                                } else {
                                    format!("{}/{}", cur_path, entry.name)
                                };
                                load_preview(wid, &full);
                            } else {
                                let mut wm2 = window::WINDOW_MANAGER.lock();
                                if let Some(w) = wm2.windows.iter_mut().find(|w| w.id == wid) {
                                    w.explorer_preview_content.clear();
                                    w.explorer_preview_path.clear();
                                }
                            }
                        }
                    }
                }
            }
            true
        }
        KeyCode::Enter => {
            if selected >= 0 {
                let mut entries = read_entries(&cur_path, show_hidden);
                sort_entries(&mut entries, sort, sort_asc);
                if let Some(entry) = entries.get(selected as usize) {
                    let full_path = if cur_path == "/" {
                        format!("/{}", entry.name)
                    } else {
                        format!("{}/{}", cur_path, entry.name)
                    };
                    if entry.is_dir {
                        navigate_to(wid, &full_path);
                    } else {
                        open_file_in_editor(&full_path);
                    }
                }
            }
            true
        }
        KeyCode::Backspace => {
            navigate_up(wid);
            true
        }
        KeyCode::Delete => {
            if selected >= 0 {
                let mut entries = read_entries(&cur_path, show_hidden);
                sort_entries(&mut entries, sort, sort_asc);
                if let Some(entry) = entries.get(selected as usize) {
                    let full_path = if cur_path == "/" {
                        format!("/{}", entry.name)
                    } else {
                        format!("{}/{}", cur_path, entry.name)
                    };
                    let mut vfs = crate::vfs::VFS.lock();
                    if entry.is_dir {
                        let _ = vfs.rmdir(&full_path);
                    } else {
                        let _ = vfs.unlink(&full_path);
                    }
                }
            }
            true
        }
        KeyCode::F2 => {
            // Rename
            if selected >= 0 {
                let mut entries = read_entries(&cur_path, show_hidden);
                sort_entries(&mut entries, sort, sort_asc);
                if let Some(entry) = entries.get(selected as usize) {
                    let mut wm = window::WINDOW_MANAGER.lock();
                    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                        win.explorer_renaming = Some(selected as usize);
                        win.explorer_rename_buf = entry.name.clone();
                    }
                }
            }
            true
        }
        _ if ctrl => {
            match key {
                KeyCode::C => {
                    // Copy
                    if selected >= 0 {
                        let mut entries = read_entries(&cur_path, show_hidden);
                        sort_entries(&mut entries, sort, sort_asc);
                        if let Some(entry) = entries.get(selected as usize) {
                            let full_path = if cur_path == "/" {
                                format!("/{}", entry.name)
                            } else {
                                format!("{}/{}", cur_path, entry.name)
                            };
                            crate::clipboard::copy_files(&[full_path.as_str()]);
                        }
                    }
                    true
                }
                KeyCode::X => {
                    // Cut
                    if selected >= 0 {
                        let mut entries = read_entries(&cur_path, show_hidden);
                        sort_entries(&mut entries, sort, sort_asc);
                        if let Some(entry) = entries.get(selected as usize) {
                            let full_path = if cur_path == "/" {
                                format!("/{}", entry.name)
                            } else {
                                format!("{}/{}", cur_path, entry.name)
                            };
                            crate::clipboard::cut_files(&[full_path.as_str()]);
                        }
                    }
                    true
                }
                KeyCode::V => {
                    // Paste
                    execute_action(wid, ExplorerAction::Paste);
                    true
                }
                KeyCode::H => {
                    // Toggle hidden files
                    let mut wm = window::WINDOW_MANAGER.lock();
                    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                        win.explorer_show_hidden = !win.explorer_show_hidden;
                    }
                    true
                }
                KeyCode::L => {
                    // Focus address bar
                    start_address_edit(wid);
                    true
                }
                KeyCode::F => {
                    // Open search bar (Ctrl+F)
                    start_search(wid);
                    true
                }
                KeyCode::P => {
                    // Toggle preview panel (Ctrl+P)
                    toggle_preview(wid);
                    true
                }
                KeyCode::G => {
                    // Toggle grid/list view (Ctrl+G)
                    toggle_grid_view(wid);
                    true
                }
                KeyCode::B => {
                    // Toggle sidebar (Ctrl+B)
                    let mut wm = window::WINDOW_MANAGER.lock();
                    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                        win.explorer_sidebar_visible = !win.explorer_sidebar_visible;
                    }
                    true
                }
                _ => false,
            }
        }
        _ => false,
    }
}
