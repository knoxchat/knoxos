use crate::serial_println;
/// Drag-and-Drop Files to Desktop
///
/// Desktop drop target, file copy/move/link operations,
/// visual drop indicator, undo support.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone, Copy)]
pub enum DropAction {
    Copy,
    Move,
    Link,
}

#[derive(Debug, Clone)]
pub struct DesktopItem {
    pub path: String,
    pub x: i32,
    pub y: i32,
    pub is_link: bool,
}

pub struct DesktopDragDrop {
    pub items: Vec<DesktopItem>,
    pub grid_size: u32,
    pub drop_highlight: bool,
}

lazy_static::lazy_static! {
    static ref STATE: Mutex<DesktopDragDrop> = Mutex::new(DesktopDragDrop {
        items: Vec::new(),
        grid_size: 64,
        drop_highlight: false,
    });
}

impl DesktopDragDrop {
    pub fn drop_files(&mut self, paths: &[&str], x: i32, y: i32, action: DropAction) {
        let grid = self.grid_size as i32;
        for (i, path) in paths.iter().enumerate() {
            let snap_x = ((x + i as i32 * grid) / grid) * grid;
            let snap_y = (y / grid) * grid;
            self.items.push(DesktopItem {
                path: String::from(*path),
                x: snap_x,
                y: snap_y,
                is_link: matches!(action, DropAction::Link),
            });
            serial_println!(
                "[DESKTOP_DND] Dropped '{}' at ({},{}) {:?}",
                path,
                snap_x,
                snap_y,
                action
            );
        }
        self.drop_highlight = false;
    }

    pub fn set_highlight(&mut self, active: bool) {
        self.drop_highlight = active;
    }
    pub fn remove_item(&mut self, path: &str) {
        self.items.retain(|i| i.path != path);
    }
}

pub fn init() {
    serial_println!("[DESKTOP_DND] Desktop drag-and-drop initialized");
}
