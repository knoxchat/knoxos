use crate::serial_println;
/// Keyboard Layout Switching
///
/// Multiple keyboard layout support with runtime switching,
/// per-window layout memory, and indicator display.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone)]
pub struct KeyboardLayout {
    pub name: String, // "us", "de", "fr", "jp"
    pub display_name: String,
    pub variant: String,
    pub scancode_map: [u8; 128],
}

pub struct LayoutManager {
    pub layouts: Vec<KeyboardLayout>,
    pub active_index: usize,
    pub per_window: Vec<(u32, usize)>, // (window_id, layout_index)
}

lazy_static::lazy_static! {
    static ref LAYOUTS: Mutex<LayoutManager> = Mutex::new(LayoutManager {
        layouts: Vec::new(),
        active_index: 0,
        per_window: Vec::new(),
    });
}

impl LayoutManager {
    pub fn add_layout(&mut self, layout: KeyboardLayout) {
        serial_println!(
            "[KBD_LAYOUT] Added: {} ({})",
            layout.display_name,
            layout.name
        );
        self.layouts.push(layout);
    }

    pub fn switch_next(&mut self) {
        if self.layouts.is_empty() {
            return;
        }
        self.active_index = (self.active_index + 1) % self.layouts.len();
        serial_println!(
            "[KBD_LAYOUT] Switched to: {}",
            self.layouts[self.active_index].name
        );
    }

    pub fn set_for_window(&mut self, window_id: u32, idx: usize) {
        if let Some(entry) = self.per_window.iter_mut().find(|(w, _)| *w == window_id) {
            entry.1 = idx;
        } else {
            self.per_window.push((window_id, idx));
        }
    }

    pub fn get_for_window(&self, window_id: u32) -> usize {
        self.per_window
            .iter()
            .find(|(w, _)| *w == window_id)
            .map(|(_, i)| *i)
            .unwrap_or(self.active_index)
    }

    pub fn active_name(&self) -> &str {
        self.layouts
            .get(self.active_index)
            .map(|l| l.name.as_str())
            .unwrap_or("us")
    }

    pub fn translate_scancode(&self, scancode: u8) -> u8 {
        if let Some(layout) = self.layouts.get(self.active_index) {
            layout
                .scancode_map
                .get(scancode as usize)
                .copied()
                .unwrap_or(scancode)
        } else {
            scancode
        }
    }
}

pub fn init() {
    let mut mgr = LAYOUTS.lock();
    let mut us = KeyboardLayout {
        name: String::from("us"),
        display_name: String::from("English (US)"),
        variant: String::new(),
        scancode_map: [0; 128],
    };
    for i in 0..128u8 {
        us.scancode_map[i as usize] = i;
    }
    mgr.add_layout(us);
    serial_println!("[KBD_LAYOUT] Keyboard layout manager initialized");
}
