use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use alloc::string::String;
use alloc::vec::Vec;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// File Picker Dialog Widget
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// File picker mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilePickerMode {
    Open,
    Save,
    SelectDirectory,
}

/// File picker dialog
pub struct FilePickerDialog {
    pub rect: Rect,
    pub mode: FilePickerMode,
    pub current_path: String,
    pub selected_file: Option<String>,
    pub entries: Vec<FilePickerEntry>,
    pub filter: String,
    pub filename_input: String,
    pub scroll_offset: usize,
    pub visible: bool,
    pub confirmed: bool,
    pub cancelled: bool,
}

/// An entry in the file picker
#[derive(Debug, Clone)]
pub struct FilePickerEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub selected: bool,
}

impl FilePickerDialog {
    pub fn new(x: i32, y: i32, width: u32, height: u32, mode: FilePickerMode) -> Self {
        Self {
            rect: Rect::new(x, y, width, height),
            mode,
            current_path: String::from("/"),
            selected_file: None,
            entries: Vec::new(),
            filter: String::from("*"),
            filename_input: String::new(),
            scroll_offset: 0,
            visible: true,
            confirmed: false,
            cancelled: false,
        }
    }

    /// Refresh entries from VFS
    pub fn refresh(&mut self) {
        self.entries.clear();
        if let Ok(dir_entries) = crate::file_manager::list_dir(&self.current_path) {
            for entry in dir_entries {
                let is_dir = entry.file_type == crate::vfs::FileType::Directory;
                self.entries.push(FilePickerEntry {
                    name: entry.name.clone(),
                    is_dir,
                    size: entry.size,
                    selected: false,
                });
            }
        }
        // Sort: dirs first, then alphabetical
        self.entries
            .sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    }

    /// Navigate to directory
    pub fn navigate(&mut self, dir: &str) {
        if dir == ".." {
            if let Some(pos) = self.current_path.rfind('/') {
                if pos == 0 {
                    self.current_path = String::from("/");
                } else {
                    self.current_path.truncate(pos);
                }
            }
        } else {
            if self.current_path.ends_with('/') {
                self.current_path.push_str(dir);
            } else {
                self.current_path.push('/');
                self.current_path.push_str(dir);
            }
        }
        self.scroll_offset = 0;
        self.refresh();
    }

    /// Get the full path of the selected file
    pub fn selected_path(&self) -> Option<String> {
        self.selected_file.as_ref().map(|f| {
            if self.current_path.ends_with('/') {
                alloc::format!("{}{}", self.current_path, f)
            } else {
                alloc::format!("{}/{}", self.current_path, f)
            }
        })
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        if !self.visible {
            return;
        }
        let r = self.rect;
        // Background
        fb.fill_rounded_rect_aa(r, Pixel::new(30, 30, 45, 240), 12);
        fb.draw_rounded_rect(r, Pixel::rgb(80, 80, 120), 12, 1);

        // Title
        let title = match self.mode {
            FilePickerMode::Open => "Open File",
            FilePickerMode::Save => "Save File",
            FilePickerMode::SelectDirectory => "Select Directory",
        };
        fonts::draw_string_compact(fb, r.x + 12, r.y + 10, title, colors::WHITE, 1);

        // Path bar
        fonts::draw_string_compact(
            fb,
            r.x + 12,
            r.y + 30,
            &self.current_path,
            Pixel::rgb(150, 150, 200),
            1,
        );

        // File list
        let list_y = r.y + 50;
        let row_h = 20i32;
        let max_rows = ((r.height as i32 - 90) / row_h).max(1) as usize;
        for (i, entry) in self
            .entries
            .iter()
            .skip(self.scroll_offset)
            .take(max_rows)
            .enumerate()
        {
            let ey = list_y + i as i32 * row_h;
            let icon = if entry.is_dir { "[D] " } else { "    " };
            let label = alloc::format!("{}{}", icon, entry.name);
            let color = if entry.selected {
                Pixel::rgb(100, 200, 255)
            } else if entry.is_dir {
                Pixel::rgb(200, 200, 255)
            } else {
                Pixel::rgb(200, 200, 200)
            };
            fonts::draw_string_compact(fb, r.x + 12, ey, &label, color, 1);
        }

        // OK / Cancel buttons
        let btn_y = r.y + r.height as i32 - 32;
        fb.fill_rounded_rect_aa(
            Rect::new(r.x + r.width as i32 - 150, btn_y, 60, 24),
            Pixel::rgb(40, 120, 200),
            6,
        );
        fonts::draw_string_compact(
            fb,
            r.x + r.width as i32 - 142,
            btn_y + 6,
            "OK",
            colors::WHITE,
            1,
        );
        fb.fill_rounded_rect_aa(
            Rect::new(r.x + r.width as i32 - 80, btn_y, 68, 24),
            Pixel::rgb(80, 80, 80),
            6,
        );
        fonts::draw_string_compact(
            fb,
            r.x + r.width as i32 - 72,
            btn_y + 6,
            "Cancel",
            colors::WHITE,
            1,
        );
    }

    pub fn handle_click(&mut self, x: i32, y: i32) -> bool {
        if !self.rect.contains(x, y) {
            return false;
        }
        let list_y = self.rect.y + 50;
        let row_h = 20i32;
        let max_rows = ((self.rect.height as i32 - 90) / row_h).max(1) as usize;

        // Check file list clicks
        let idx = ((y - list_y) / row_h) as usize + self.scroll_offset;
        if idx < self.entries.len() && y >= list_y {
            let entry = &self.entries[idx];
            if entry.is_dir {
                let name = entry.name.clone();
                self.navigate(&name);
            } else {
                for e in &mut self.entries {
                    e.selected = false;
                }
                self.entries[idx].selected = true;
                self.selected_file = Some(self.entries[idx].name.clone());
            }
            return true;
        }

        // OK button
        let btn_y = self.rect.y + self.rect.height as i32 - 32;
        if x >= self.rect.x + self.rect.width as i32 - 150
            && x < self.rect.x + self.rect.width as i32 - 90
            && y >= btn_y
            && y < btn_y + 24
        {
            self.confirmed = true;
            self.visible = false;
            return true;
        }
        // Cancel
        if x >= self.rect.x + self.rect.width as i32 - 80 && y >= btn_y && y < btn_y + 24 {
            self.cancelled = true;
            self.visible = false;
            return true;
        }
        true
    }
}
