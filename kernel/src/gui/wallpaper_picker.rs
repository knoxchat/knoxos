use crate::serial_println;
/// Custom Wallpaper from File Picker
///
/// File picker integration for wallpaper selection, fill/fit/stretch/center
/// modes, slideshow, per-monitor wallpaper.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone, Copy)]
pub enum WallpaperMode {
    Fill,
    Fit,
    Stretch,
    Center,
    Tile,
}

#[derive(Debug, Clone)]
pub struct WallpaperConfig {
    pub path: String,
    pub mode: WallpaperMode,
    pub monitor_id: u32,
}

pub struct WallpaperManager {
    pub configs: Vec<WallpaperConfig>,
    pub slideshow_enabled: bool,
    pub slideshow_interval_secs: u32,
    pub slideshow_paths: Vec<String>,
    pub slideshow_index: usize,
}

lazy_static::lazy_static! {
    static ref WALLPAPER: Mutex<WallpaperManager> = Mutex::new(WallpaperManager {
        configs: Vec::new(),
        slideshow_enabled: false,
        slideshow_interval_secs: 600,
        slideshow_paths: Vec::new(),
        slideshow_index: 0,
    });
}

impl WallpaperManager {
    pub fn set_wallpaper(&mut self, monitor: u32, path: &str, mode: WallpaperMode) {
        if let Some(cfg) = self.configs.iter_mut().find(|c| c.monitor_id == monitor) {
            cfg.path = String::from(path);
            cfg.mode = mode;
        } else {
            self.configs.push(WallpaperConfig {
                path: String::from(path),
                mode,
                monitor_id: monitor,
            });
        }
        serial_println!("[WALLPAPER] Monitor {}: {} ({:?})", monitor, path, mode);
    }

    pub fn setup_slideshow(&mut self, paths: Vec<String>, interval: u32) {
        self.slideshow_paths = paths;
        self.slideshow_interval_secs = interval;
        self.slideshow_enabled = true;
        self.slideshow_index = 0;
        serial_println!(
            "[WALLPAPER] Slideshow: {} images, {}s interval",
            self.slideshow_paths.len(),
            interval
        );
    }

    pub fn advance_slideshow(&mut self) {
        if !self.slideshow_enabled || self.slideshow_paths.is_empty() {
            return;
        }
        self.slideshow_index = (self.slideshow_index + 1) % self.slideshow_paths.len();
        let path = self.slideshow_paths[self.slideshow_index].clone();
        for cfg in &mut self.configs {
            cfg.path = path.clone();
        }
    }

    pub fn get_wallpaper(&self, monitor: u32) -> Option<&WallpaperConfig> {
        self.configs.iter().find(|c| c.monitor_id == monitor)
    }
}

pub fn init() {
    serial_println!("[WALLPAPER] Wallpaper picker initialized");
}
