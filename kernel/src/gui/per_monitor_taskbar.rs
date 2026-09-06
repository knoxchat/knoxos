use crate::serial_println;
/// Per-Monitor Taskbar
///
/// Independent taskbar instance per monitor in multi-monitor setups,
/// showing only windows on that monitor.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone)]
pub struct MonitorTaskbar {
    pub monitor_id: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub window_ids: Vec<u32>,
    pub show_all_windows: bool,
}

pub struct MultiMonitorTaskbar {
    pub taskbars: Vec<MonitorTaskbar>,
    pub primary_has_tray: bool,
}

lazy_static::lazy_static! {
    static ref TASKBARS: Mutex<MultiMonitorTaskbar> = Mutex::new(MultiMonitorTaskbar {
        taskbars: Vec::new(),
        primary_has_tray: true,
    });
}

impl MultiMonitorTaskbar {
    pub fn create_for_monitor(&mut self, monitor_id: u32, x: i32, y: i32, w: u32) {
        self.taskbars.push(MonitorTaskbar {
            monitor_id,
            x,
            y,
            width: w,
            height: 48,
            window_ids: Vec::new(),
            show_all_windows: false,
        });
        serial_println!("[TASKBAR_MM] Created taskbar for monitor {}", monitor_id);
    }

    pub fn assign_window(&mut self, monitor_id: u32, window_id: u32) {
        if let Some(tb) = self
            .taskbars
            .iter_mut()
            .find(|t| t.monitor_id == monitor_id)
        {
            if !tb.window_ids.contains(&window_id) {
                tb.window_ids.push(window_id);
            }
        }
    }

    pub fn remove_window(&mut self, window_id: u32) {
        for tb in &mut self.taskbars {
            tb.window_ids.retain(|&w| w != window_id);
        }
    }

    pub fn move_window(&mut self, window_id: u32, from: u32, to: u32) {
        if let Some(tb) = self.taskbars.iter_mut().find(|t| t.monitor_id == from) {
            tb.window_ids.retain(|&w| w != window_id);
        }
        if let Some(tb) = self.taskbars.iter_mut().find(|t| t.monitor_id == to) {
            tb.window_ids.push(window_id);
        }
    }
}

pub fn init() {
    serial_println!("[TASKBAR_MM] Per-monitor taskbar initialized");
}
