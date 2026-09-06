use alloc::format;
/// Desktop Applications Framework — Built-in application suite for KnoxOS desktop
/// Provides Calculator, System Monitor, Text Editor, File Manager, and Settings apps
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// APPLICATION FRAMEWORK
// ═══════════════════════════════════════════════════════════════════════

/// Application identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppId {
    Calculator,
    SystemMonitor,
    TextEditor,
    FileManager,
    Settings,
    ImageViewer,
    MusicPlayer,
    WebBrowser,
    Terminal,
    PackageManager,
    NetworkMonitor,
    About,
}

/// Application state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    Idle,
    Running,
    Minimized,
    Maximized,
}

/// Running application instance
pub struct AppInstance {
    pub id: AppId,
    pub state: AppState,
    pub title: String,
    pub data: AppData,
}

/// Per-application data
pub enum AppData {
    Calculator(CalculatorState),
    SystemMonitor(SystemMonitorState),
    TextEditor(TextEditorState),
    FileManager(FileManagerState),
    Settings(SettingsState),
    ImageViewer(ImageViewerState),
    None,
}

// ═══════════════════════════════════════════════════════════════════════
// IMAGE VIEWER APP
// ═══════════════════════════════════════════════════════════════════════

/// Image viewer state
pub struct ImageViewerState {
    /// File path of the currently loaded image
    pub file_path: String,
    /// Decoded pixel data (RGBA u8 quads packed as u32: 0xAARRGGBB)
    pub pixels: Vec<u32>,
    /// Original image width
    pub img_width: u32,
    /// Original image height
    pub img_height: u32,
    /// Zoom level (1.0 = 100%)
    pub zoom: f32,
    /// Pan offset X (pixels)
    pub pan_x: f32,
    /// Pan offset Y (pixels)
    pub pan_y: f32,
    /// Whether the image is currently being dragged
    pub dragging: bool,
    /// Status message for the UI
    pub status: String,
    /// Fit mode: true = fit-to-window, false = actual-size with pan/zoom
    pub fit_to_window: bool,
}

impl ImageViewerState {
    pub fn new() -> Self {
        ImageViewerState {
            file_path: String::new(),
            pixels: Vec::new(),
            img_width: 0,
            img_height: 0,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            dragging: false,
            status: String::from("No image loaded. Use File→Open to load an image."),
            fit_to_window: true,
        }
    }

    /// Load an image from a VFS path
    pub fn open_file(&mut self, path: &str) {
        self.file_path = String::from(path);
        self.status = format!("Loading {}...", path);

        // Read file from VFS
        let data = match crate::vfs::read_file_dispatch(path) {
            Some(d) => d,
            None => {
                self.status = format!("Error: could not read {}", path);
                return;
            }
        };

        // Decode image
        match crate::gui::image::decode(&data) {
            Some(img) => {
                self.img_width = img.width;
                self.img_height = img.height;
                // Convert Pixel vec to packed u32 for blitting
                self.pixels = img
                    .pixels
                    .iter()
                    .map(|p| {
                        ((p.a as u32) << 24)
                            | ((p.r as u32) << 16)
                            | ((p.g as u32) << 8)
                            | p.b as u32
                    })
                    .collect();
                self.zoom = 1.0;
                self.pan_x = 0.0;
                self.pan_y = 0.0;
                self.fit_to_window = true;
                self.status = format!(
                    "{}  ({}×{}, {} KB)",
                    path,
                    img.width,
                    img.height,
                    data.len() / 1024
                );
                serial_println!(
                    "[ImageViewer] Loaded {} ({}x{})",
                    path,
                    img.width,
                    img.height
                );
            }
            None => {
                self.status = format!("Error: unsupported or corrupt image: {}", path);
            }
        }
    }

    /// Zoom in
    pub fn zoom_in(&mut self) {
        self.zoom = (self.zoom * 1.25).min(16.0);
        self.fit_to_window = false;
    }

    /// Zoom out
    pub fn zoom_out(&mut self) {
        self.zoom = (self.zoom / 1.25).max(0.1);
        self.fit_to_window = false;
    }

    /// Reset zoom to 100%
    pub fn zoom_reset(&mut self) {
        self.zoom = 1.0;
        self.pan_x = 0.0;
        self.pan_y = 0.0;
        self.fit_to_window = false;
    }

    /// Fit image to a given viewport size
    pub fn fit_to(&mut self, viewport_w: u32, viewport_h: u32) {
        if self.img_width == 0 || self.img_height == 0 {
            return;
        }
        let scale_x = viewport_w as f32 / self.img_width as f32;
        let scale_y = viewport_h as f32 / self.img_height as f32;
        self.zoom = if scale_x < scale_y { scale_x } else { scale_y };
        self.pan_x = 0.0;
        self.pan_y = 0.0;
        self.fit_to_window = true;
    }
}

// ═══════════════════════════════════════════════════════════════════════
// CALCULATOR APP
// ═══════════════════════════════════════════════════════════════════════

/// Calculator operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalcOp {
    None,
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Power,
}

/// Calculator state
pub struct CalculatorState {
    pub display: String,
    pub accumulator: f64,
    pub current: f64,
    pub operation: CalcOp,
    pub new_input: bool,
    pub error: bool,
    pub history: Vec<String>,
    pub memory: f64,
}

impl CalculatorState {
    pub fn new() -> Self {
        Self {
            display: String::from("0"),
            accumulator: 0.0,
            current: 0.0,
            operation: CalcOp::None,
            new_input: true,
            error: false,
            history: Vec::new(),
            memory: 0.0,
        }
    }

    /// Input a digit
    pub fn input_digit(&mut self, digit: char) {
        if self.error {
            self.clear();
        }
        if self.new_input {
            self.display = String::new();
            self.new_input = false;
        }
        if digit == '.' && self.display.contains('.') {
            return; // Only one decimal point
        }
        self.display.push(digit);
    }

    /// Set operation
    pub fn set_operation(&mut self, op: CalcOp) {
        self.current = self.parse_display();
        if self.operation != CalcOp::None && !self.new_input {
            self.calculate();
        } else {
            self.accumulator = self.current;
        }
        self.operation = op;
        self.new_input = true;
    }

    /// Calculate result
    pub fn calculate(&mut self) {
        if self.new_input && self.operation == CalcOp::None {
            return;
        }
        if !self.new_input {
            self.current = self.parse_display();
        }

        let result = match self.operation {
            CalcOp::None => self.current,
            CalcOp::Add => self.accumulator + self.current,
            CalcOp::Subtract => self.accumulator - self.current,
            CalcOp::Multiply => self.accumulator * self.current,
            CalcOp::Divide => {
                if self.current == 0.0 {
                    self.error = true;
                    self.display = String::from("Error: Div/0");
                    return;
                }
                self.accumulator / self.current
            }
            CalcOp::Modulo => {
                if self.current == 0.0 {
                    self.error = true;
                    self.display = String::from("Error: Div/0");
                    return;
                }
                self.accumulator % self.current
            }
            CalcOp::Power => {
                // Simple integer power for no_std
                let mut result = 1.0;
                let n = self.current as i32;
                for _ in 0..n.unsigned_abs() {
                    result *= self.accumulator;
                }
                if n < 0 { 1.0 / result } else { result }
            }
        };

        // Add to history
        let op_str = match self.operation {
            CalcOp::None => "",
            CalcOp::Add => "+",
            CalcOp::Subtract => "-",
            CalcOp::Multiply => "×",
            CalcOp::Divide => "÷",
            CalcOp::Modulo => "%",
            CalcOp::Power => "^",
        };
        let entry = format!(
            "{} {} {} = {}",
            self.accumulator, op_str, self.current, result
        );
        self.history.push(entry);
        if self.history.len() > 50 {
            self.history.remove(0);
        }

        self.accumulator = result;
        self.display = format_number(result);
        self.operation = CalcOp::None;
        self.new_input = true;
    }

    /// Clear
    pub fn clear(&mut self) {
        self.display = String::from("0");
        self.accumulator = 0.0;
        self.current = 0.0;
        self.operation = CalcOp::None;
        self.new_input = true;
        self.error = false;
    }

    /// Clear entry (CE)
    pub fn clear_entry(&mut self) {
        self.display = String::from("0");
        self.new_input = true;
        self.error = false;
    }

    /// Negate
    pub fn negate(&mut self) {
        if self.display != "0" {
            if self.display.starts_with('-') {
                self.display = self.display[1..].to_string();
            } else {
                self.display = format!("-{}", self.display);
            }
        }
    }

    /// Backspace
    pub fn backspace(&mut self) {
        if !self.new_input && self.display.len() > 1 {
            self.display.pop();
        } else {
            self.display = String::from("0");
            self.new_input = true;
        }
    }

    /// Memory store
    pub fn memory_store(&mut self) {
        self.memory = self.parse_display();
    }

    /// Memory recall
    pub fn memory_recall(&mut self) {
        self.display = format_number(self.memory);
        self.new_input = true;
    }

    /// Memory add
    pub fn memory_add(&mut self) {
        self.memory += self.parse_display();
    }

    /// Memory clear
    pub fn memory_clear(&mut self) {
        self.memory = 0.0;
    }

    /// Square root (approximate via Newton's method)
    pub fn sqrt(&mut self) {
        let val = self.parse_display();
        if val < 0.0 {
            self.error = true;
            self.display = String::from("Error: sqrt(neg)");
            return;
        }
        if val == 0.0 {
            self.display = String::from("0");
            return;
        }
        // Newton's method for sqrt
        let mut guess = val / 2.0;
        for _ in 0..50 {
            guess = (guess + val / guess) / 2.0;
        }
        self.display = format_number(guess);
        self.new_input = true;
    }

    /// Percentage
    pub fn percent(&mut self) {
        let val = self.parse_display();
        let result = if self.operation != CalcOp::None {
            self.accumulator * val / 100.0
        } else {
            val / 100.0
        };
        self.display = format_number(result);
        self.new_input = true;
    }

    fn parse_display(&self) -> f64 {
        // Simple f64 parse for no_std
        parse_f64(&self.display)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SYSTEM MONITOR APP
// ═══════════════════════════════════════════════════════════════════════

/// System monitor state
pub struct SystemMonitorState {
    pub tab: MonitorTab,
    pub cpu_history: Vec<u8>,     // CPU usage % history (last 60 samples)
    pub mem_history: Vec<u8>,     // Memory usage % history
    pub net_rx_history: Vec<u64>, // Network RX bytes history
    pub net_tx_history: Vec<u64>, // Network TX bytes history
    pub update_counter: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorTab {
    Overview,
    Processes,
    Cpu,
    Memory,
    Network,
    Disks,
}

impl SystemMonitorState {
    pub fn new() -> Self {
        Self {
            tab: MonitorTab::Overview,
            cpu_history: Vec::new(),
            mem_history: Vec::new(),
            net_rx_history: Vec::new(),
            net_tx_history: Vec::new(),
            update_counter: 0,
        }
    }

    /// Update system metrics
    pub fn update(&mut self) {
        self.update_counter += 1;

        // Sample real CPU usage from scheduler stats
        let sched_stats = crate::scheduler::SCHEDULER.lock().stats();
        // Approximate CPU usage: ratio of running processes to total
        let total_procs = sched_stats.run_queue_len + sched_stats.wait_queue_len + 1;
        let busy = sched_stats.run_queue_len.max(1);
        let cpu_usage = ((busy * 100) / total_procs.max(1)).min(100) as u8;
        self.cpu_history.push(cpu_usage);
        if self.cpu_history.len() > 60 {
            self.cpu_history.remove(0);
        }

        // Sample real memory usage from heap allocator
        let heap_total = crate::allocator::HEAP_SIZE;
        // Approximate used memory from process count and slab stats
        let process_count = crate::process::PROCESS_TABLE.lock().processes.len();
        let est_used = (process_count * 2 * 1024 * 1024).min(heap_total); // ~2MB per process estimate
        let mem_pct = ((est_used * 100) / heap_total.max(1)).min(100) as u8;
        self.mem_history.push(mem_pct);
        if self.mem_history.len() > 60 {
            self.mem_history.remove(0);
        }

        // Sample network stats from real NIC byte counters
        let net_stats = crate::net::get_net_stats();
        let rx = net_stats.rx_bytes;
        let tx = net_stats.tx_bytes;
        self.net_rx_history.push(rx);
        if self.net_rx_history.len() > 60 {
            self.net_rx_history.remove(0);
        }
        self.net_tx_history.push(tx);
        if self.net_tx_history.len() > 60 {
            self.net_tx_history.remove(0);
        }
    }

    /// Get overview text
    pub fn overview(&self) -> String {
        let process_count = crate::process::PROCESS_TABLE.lock().processes.len();
        let uptime = crate::interrupts::get_ticks() / 18; // Approximate seconds
        let sched_stats = crate::scheduler::SCHEDULER.lock().stats();
        let smp_cpus = crate::smp::online_cpus();

        let mut s = String::new();
        s.push_str("═══ KnoxOS System Monitor ═══\n\n");
        s.push_str(&format!(
            "Uptime: {}h {}m {}s\n",
            uptime / 3600,
            (uptime % 3600) / 60,
            uptime % 60
        ));
        s.push_str(&format!("CPUs Online: {}\n", smp_cpus));
        s.push_str(&format!(
            "Processes: {} (run={}, wait={})\n",
            process_count, sched_stats.run_queue_len, sched_stats.wait_queue_len
        ));
        s.push_str(&format!(
            "Context Switches: {}\n",
            sched_stats.context_switches
        ));
        s.push_str(&format!(
            "Heap: {} MiB total\n",
            crate::allocator::HEAP_SIZE / 1024 / 1024
        ));
        s.push_str(&format!(
            "CPU Usage: {}%\n",
            self.cpu_history.last().unwrap_or(&0)
        ));
        s.push_str(&format!(
            "Memory Usage: {}%\n",
            self.mem_history.last().unwrap_or(&0)
        ));

        // Network stats
        let rx = self.net_rx_history.last().unwrap_or(&0);
        let tx = self.net_tx_history.last().unwrap_or(&0);
        s.push_str(&format!("Network: RX {} bytes, TX {} bytes\n", rx, tx));

        // Stack guard stats
        let (stacks, guard_faults) = crate::stack_guard::stats();
        s.push_str(&format!(
            "Guarded Stacks: {}, Guard Faults Caught: {}\n",
            stacks, guard_faults
        ));

        // APIC timer info
        if crate::apic_timer::is_initialized() {
            s.push_str(&format!(
                "APIC Timer Ticks: {}\n",
                crate::apic_timer::total_ticks()
            ));
        }

        s
    }

    /// Get process list
    pub fn process_list(&self) -> String {
        let table = crate::process::PROCESS_TABLE.lock();
        let mut s = String::new();
        s.push_str("PID  PPID  STATE      NAME\n");
        s.push_str("───  ────  ─────      ────\n");
        for proc in &table.processes {
            let state = match proc.state {
                crate::process::ProcessState::Running => "Running  ",
                crate::process::ProcessState::Ready => "Ready    ",
                crate::process::ProcessState::Sleeping => "Sleeping ",
                crate::process::ProcessState::Stopped => "Stopped  ",
                crate::process::ProcessState::Zombie => "Zombie   ",
            };
            s.push_str(&format!(
                "{:<5}{:<6}{}{}\n",
                proc.pid, proc.ppid, state, proc.name
            ));
        }
        s
    }
}

// ═══════════════════════════════════════════════════════════════════════
// TEXT EDITOR APP
// ═══════════════════════════════════════════════════════════════════════

/// Text editor state
pub struct TextEditorState {
    pub filename: Option<String>,
    pub lines: Vec<String>,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub scroll_offset: usize,
    pub modified: bool,
    pub mode: EditorMode,
    pub status_msg: String,
    pub selection_start: Option<(usize, usize)>,
    pub clipboard: String,
    pub undo_stack: Vec<EditorAction>,
    pub redo_stack: Vec<EditorAction>,
    pub tab_size: usize,
    pub show_line_numbers: bool,
    pub word_wrap: bool,
    pub syntax_highlighting: bool,

    // ── Find / Replace (9.94) ───────────────────────────────────────
    /// Search query for Ctrl+F / Ctrl+H find & replace
    pub search_query: String,
    /// Replacement text for Ctrl+H replace mode
    pub replace_text: String,
    /// All match positions: (line, col)
    pub search_matches: Vec<(usize, usize)>,
    /// Index of the currently highlighted match
    pub search_match_idx: usize,
    /// true = editing replace field, false = editing search field
    pub replace_field_focused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    Normal,
    Insert,
    Command,
    Search,
    Replace,
}

#[derive(Debug, Clone)]
pub enum EditorAction {
    Insert {
        line: usize,
        col: usize,
        text: String,
    },
    Delete {
        line: usize,
        col: usize,
        text: String,
    },
    JoinLine {
        line: usize,
    },
    SplitLine {
        line: usize,
        col: usize,
    },
}

impl TextEditorState {
    pub fn new() -> Self {
        Self {
            filename: None,
            lines: vec![String::new()],
            cursor_line: 0,
            cursor_col: 0,
            scroll_offset: 0,
            modified: false,
            mode: EditorMode::Insert,
            status_msg: String::from("New file"),
            selection_start: None,
            clipboard: String::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            tab_size: 4,
            show_line_numbers: true,
            word_wrap: false,
            syntax_highlighting: true,
            search_query: String::new(),
            replace_text: String::new(),
            search_matches: Vec::new(),
            search_match_idx: 0,
            replace_field_focused: false,
        }
    }

    /// Open a file
    pub fn open(&mut self, filename: &str) {
        let vfs = crate::vfs::VFS.lock();
        if let Some(data) = vfs.read_file(filename) {
            let content = core::str::from_utf8(data).unwrap_or("");
            self.lines = content.lines().map(String::from).collect();
            if self.lines.is_empty() {
                self.lines.push(String::new());
            }
            self.filename = Some(String::from(filename));
            self.cursor_line = 0;
            self.cursor_col = 0;
            self.scroll_offset = 0;
            self.modified = false;
            self.status_msg = format!("Opened: {}", filename);
        } else {
            self.status_msg = format!("Error: Cannot open {}", filename);
        }
    }

    /// Save current file
    pub fn save(&mut self) -> bool {
        if let Some(ref filename) = self.filename {
            let content: String = self.lines.join("\n");
            let mut vfs = crate::vfs::VFS.lock();
            if vfs.write_file(filename, content.as_bytes()) {
                self.modified = false;
                self.status_msg = format!("Saved: {} ({} bytes)", filename, content.len());
                return true;
            }
        }
        self.status_msg = String::from("Error: Cannot save file");
        false
    }

    /// Insert character at cursor
    pub fn insert_char(&mut self, ch: char) {
        if self.cursor_line >= self.lines.len() {
            return;
        }
        let line = &mut self.lines[self.cursor_line];
        if ch == '\t' {
            for _ in 0..self.tab_size {
                line.insert(self.cursor_col.min(line.len()), ' ');
                self.cursor_col += 1;
            }
        } else {
            line.insert(self.cursor_col.min(line.len()), ch);
            self.cursor_col += 1;
        }
        self.modified = true;
        self.undo_stack.push(EditorAction::Insert {
            line: self.cursor_line,
            col: self.cursor_col - 1,
            text: ch.to_string(),
        });
        self.redo_stack.clear();
    }

    /// Insert newline at cursor
    pub fn insert_newline(&mut self) {
        if self.cursor_line >= self.lines.len() {
            self.lines.push(String::new());
            self.cursor_line = self.lines.len() - 1;
            self.cursor_col = 0;
            return;
        }
        let line = self.lines[self.cursor_line].clone();
        let (left, right) = line.split_at(self.cursor_col.min(line.len()));
        self.lines[self.cursor_line] = String::from(left);
        self.lines.insert(self.cursor_line + 1, String::from(right));
        self.cursor_line += 1;
        self.cursor_col = 0;
        self.modified = true;
        self.undo_stack.push(EditorAction::SplitLine {
            line: self.cursor_line - 1,
            col: left.len(),
        });
    }

    /// Delete character before cursor (backspace)
    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            let ch = self.lines[self.cursor_line].remove(self.cursor_col - 1);
            self.cursor_col -= 1;
            self.modified = true;
            self.undo_stack.push(EditorAction::Delete {
                line: self.cursor_line,
                col: self.cursor_col,
                text: ch.to_string(),
            });
        } else if self.cursor_line > 0 {
            // Join with previous line
            let current_line = self.lines.remove(self.cursor_line);
            self.cursor_line -= 1;
            self.cursor_col = self.lines[self.cursor_line].len();
            self.lines[self.cursor_line].push_str(&current_line);
            self.modified = true;
            self.undo_stack.push(EditorAction::JoinLine {
                line: self.cursor_line,
            });
        }
    }

    /// Move cursor
    pub fn move_cursor(&mut self, dx: i32, dy: i32) {
        if dy < 0 && self.cursor_line > 0 {
            self.cursor_line -= 1;
        } else if dy > 0 && self.cursor_line < self.lines.len() - 1 {
            self.cursor_line += 1;
        }
        if dx < 0 && self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if dx > 0 {
            self.cursor_col += 1;
        }
        // Clamp cursor_col to line length
        let line_len = self.lines.get(self.cursor_line).map_or(0, |l| l.len());
        self.cursor_col = self.cursor_col.min(line_len);
    }

    /// Get current line count
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Get character count
    pub fn char_count(&self) -> usize {
        self.lines.iter().map(|l| l.len()).sum::<usize>() + self.lines.len().saturating_sub(1)
    }

    /// Status bar text
    pub fn status_bar(&self) -> String {
        let mode = match self.mode {
            EditorMode::Normal => "NORMAL",
            EditorMode::Insert => "INSERT",
            EditorMode::Command => "COMMAND",
            EditorMode::Search => "SEARCH",
            EditorMode::Replace => "REPLACE",
        };
        let modified = if self.modified { "[+]" } else { "" };
        let filename = self.filename.as_deref().unwrap_or("[No Name]");
        format!(
            "{} {} {} | Ln {}, Col {} | {} lines | {}",
            mode,
            filename,
            modified,
            self.cursor_line + 1,
            self.cursor_col + 1,
            self.lines.len(),
            self.status_msg
        )
    }

    // ── Find & Replace (9.94) ─────────────────────────────────────────

    /// Enter Search mode (Ctrl+F)
    pub fn start_search(&mut self) {
        self.mode = EditorMode::Search;
        self.search_query.clear();
        self.search_matches.clear();
        self.search_match_idx = 0;
        self.replace_field_focused = false;
        self.status_msg = String::from("Find: type query, Enter=next, Esc=cancel");
    }

    /// Enter Replace mode (Ctrl+H)
    pub fn start_replace(&mut self) {
        self.mode = EditorMode::Replace;
        self.search_query.clear();
        self.replace_text.clear();
        self.search_matches.clear();
        self.search_match_idx = 0;
        self.replace_field_focused = false;
        self.status_msg =
            String::from("Find & Replace: Tab=switch fields, Enter=replace, Ctrl+A=replace all");
    }

    /// Rebuild search matches from current query
    pub fn update_search_matches(&mut self) {
        self.search_matches.clear();
        if self.search_query.is_empty() {
            return;
        }
        let query_lower = self.search_query.to_ascii_lowercase();
        for (li, line) in self.lines.iter().enumerate() {
            let line_lower = line.to_ascii_lowercase();
            let mut start = 0;
            while let Some(pos) = line_lower[start..].find(&query_lower) {
                self.search_matches.push((li, start + pos));
                start += pos + 1;
            }
        }
        if !self.search_matches.is_empty() {
            self.search_match_idx = self.search_match_idx.min(self.search_matches.len() - 1);
            self.status_msg = alloc::format!("{} matches found", self.search_matches.len());
        } else {
            self.status_msg = String::from("No matches");
        }
    }

    /// Jump to the next match
    pub fn find_next(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.search_match_idx = (self.search_match_idx + 1) % self.search_matches.len();
        let (line, col) = self.search_matches[self.search_match_idx];
        self.cursor_line = line;
        self.cursor_col = col;
        self.status_msg = alloc::format!(
            "Match {}/{}",
            self.search_match_idx + 1,
            self.search_matches.len()
        );
    }

    /// Jump to the previous match
    pub fn find_prev(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        if self.search_match_idx == 0 {
            self.search_match_idx = self.search_matches.len() - 1;
        } else {
            self.search_match_idx -= 1;
        }
        let (line, col) = self.search_matches[self.search_match_idx];
        self.cursor_line = line;
        self.cursor_col = col;
        self.status_msg = alloc::format!(
            "Match {}/{}",
            self.search_match_idx + 1,
            self.search_matches.len()
        );
    }

    /// Replace the current match with the replacement text
    pub fn replace_current(&mut self) {
        if self.search_matches.is_empty() || self.replace_text.is_empty() {
            return;
        }
        let (line, col) = self.search_matches[self.search_match_idx];
        let qlen = self.search_query.len();
        if line < self.lines.len() && col + qlen <= self.lines[line].len() {
            self.lines[line].replace_range(col..col + qlen, &self.replace_text);
            self.modified = true;
            self.status_msg = String::from("Replaced 1 occurrence");
        }
        self.update_search_matches();
        // Keep match index in range
        if !self.search_matches.is_empty() {
            self.search_match_idx = self.search_match_idx.min(self.search_matches.len() - 1);
        }
    }

    /// Replace all occurrences
    pub fn replace_all(&mut self) {
        if self.search_query.is_empty() || self.replace_text.is_empty() {
            return;
        }
        let mut count = 0usize;
        let query_lower = self.search_query.to_ascii_lowercase();
        for line in self.lines.iter_mut() {
            let line_lower = line.to_ascii_lowercase();
            if line_lower.contains(&query_lower) {
                // Case-insensitive replace
                let mut result = String::new();
                let mut remaining = line.as_str();
                let mut rem_lower = line_lower.as_str();
                while let Some(pos) = rem_lower.find(&query_lower) {
                    result.push_str(&remaining[..pos]);
                    result.push_str(&self.replace_text);
                    remaining = &remaining[pos + self.search_query.len()..];
                    rem_lower = &rem_lower[pos + query_lower.len()..];
                    count += 1;
                }
                result.push_str(remaining);
                *line = result;
            }
        }
        if count > 0 {
            self.modified = true;
        }
        self.status_msg = alloc::format!("Replaced {} occurrences", count);
        self.search_matches.clear();
        self.search_match_idx = 0;
    }

    /// Cancel search/replace mode
    pub fn cancel_search(&mut self) {
        self.mode = EditorMode::Insert;
        self.search_matches.clear();
        self.status_msg = String::new();
    }
}

// ═══════════════════════════════════════════════════════════════════════
// FILE MANAGER APP
// ═══════════════════════════════════════════════════════════════════════

/// File manager state
pub struct FileManagerState {
    pub current_path: String,
    pub entries: Vec<FileEntry>,
    pub selected: usize,
    pub view_mode: ViewMode,
    pub show_hidden: bool,
    pub sort_by: SortBy,
    pub sort_asc: bool,
    pub clipboard_path: Option<String>,
    pub clipboard_op: ClipboardOp,
    pub history: Vec<String>,
    pub history_idx: usize,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub permissions: u16,
    pub owner: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    List,
    Grid,
    Details,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Name,
    Size,
    Type,
    Date,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardOp {
    None,
    Copy,
    Cut,
}

impl FileManagerState {
    pub fn new() -> Self {
        let mut fm = Self {
            current_path: String::from("/"),
            entries: Vec::new(),
            selected: 0,
            view_mode: ViewMode::Details,
            show_hidden: false,
            sort_by: SortBy::Name,
            sort_asc: true,
            clipboard_path: None,
            clipboard_op: ClipboardOp::None,
            history: vec![String::from("/")],
            history_idx: 0,
        };
        fm.refresh();
        fm
    }

    /// Refresh directory listing
    pub fn refresh(&mut self) {
        self.entries.clear();
        let vfs = crate::vfs::VFS.lock();

        if let Some(names) = vfs.list_dir(&self.current_path) {
            for name in names {
                if !self.show_hidden && name.starts_with('.') {
                    continue;
                }
                // Resolve child path to check if directory
                let child_path = if self.current_path == "/" {
                    format!("/{}", name)
                } else {
                    format!("{}/{}", self.current_path, name)
                };
                let is_dir = vfs
                    .resolve_path(&child_path)
                    .and_then(|ino| vfs.get_inode(ino))
                    .map(|i| i.file_type == crate::vfs::FileType::Directory)
                    .unwrap_or(false);
                let size = vfs
                    .resolve_path(&child_path)
                    .and_then(|ino| vfs.get_inode(ino))
                    .map(|i| i.size)
                    .unwrap_or(0);
                self.entries.push(FileEntry {
                    name,
                    is_dir,
                    size,
                    permissions: 0o755,
                    owner: String::from("root"),
                });
            }
        }

        // Sort
        match self.sort_by {
            SortBy::Name => {
                self.entries.sort_by(|a, b| {
                    // Directories first
                    match (a.is_dir, b.is_dir) {
                        (true, false) => core::cmp::Ordering::Less,
                        (false, true) => core::cmp::Ordering::Greater,
                        _ => a.name.cmp(&b.name),
                    }
                });
            }
            SortBy::Size => {
                self.entries.sort_by_key(|a| a.size);
            }
            _ => {}
        }
        if !self.sort_asc {
            self.entries.reverse();
        }
    }

    /// Navigate to a directory
    pub fn navigate(&mut self, path: &str) {
        if path == ".." {
            // Go up
            if let Some(pos) = self.current_path.rfind('/') {
                if pos == 0 {
                    self.current_path = String::from("/");
                } else {
                    self.current_path = self.current_path[..pos].to_string();
                }
            }
        } else {
            if self.current_path == "/" {
                self.current_path = format!("/{}", path);
            } else {
                self.current_path = format!("{}/{}", self.current_path, path);
            }
        }
        self.selected = 0;
        self.refresh();

        // Add to history
        self.history_idx += 1;
        self.history.truncate(self.history_idx);
        self.history.push(self.current_path.clone());
    }

    /// Enter selected entry
    pub fn enter_selected(&mut self) {
        if let Some(entry) = self.entries.get(self.selected) {
            if entry.is_dir {
                let name = entry.name.clone();
                self.navigate(&name);
            }
        }
    }

    /// Move selection
    pub fn move_selection(&mut self, delta: i32) {
        let new = self.selected as i32 + delta;
        if new >= 0 && (new as usize) < self.entries.len() {
            self.selected = new as usize;
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SETTINGS APP
// ═══════════════════════════════════════════════════════════════════════

/// Settings state
pub struct SettingsState {
    pub category: SettingsCategory,
    pub hostname: String,
    pub timezone: String,
    pub locale: String,
    pub theme: String,
    pub font_size: u8,
    pub wallpaper: String,
    pub cursor_blink: bool,
    pub sound_volume: u8,
    pub network_dhcp: bool,
    pub ssh_enabled: bool,
    pub firewall_enabled: bool,
    pub auto_update: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsCategory {
    System,
    Display,
    Network,
    Sound,
    Security,
    About,
}

impl SettingsState {
    pub fn new() -> Self {
        Self {
            category: SettingsCategory::System,
            hostname: String::from("knoxos"),
            timezone: String::from("UTC"),
            locale: String::from("en_US.UTF-8"),
            theme: String::from("Dark"),
            font_size: 14,
            wallpaper: String::from("vanta-waves"),
            cursor_blink: true,
            sound_volume: 75,
            network_dhcp: true,
            ssh_enabled: false,
            firewall_enabled: true,
            auto_update: true,
        }
    }

    /// Get about information
    pub fn about_info(&self) -> String {
        let mut s = String::new();
        s.push_str("╔═══════════════════════════════════╗\n");
        s.push_str("║        KnoxOS v0.15.0             ║\n");
        s.push_str("║   AI-Native Operating System      ║\n");
        s.push_str("╚═══════════════════════════════════╝\n\n");
        s.push_str("Built with Rust • Linux Compatible\n\n");
        s.push_str(&format!("Hostname: {}\n", self.hostname));
        s.push_str(&format!("Kernel: KnoxOS 6.1.0-knoxos\n"));
        s.push_str(&format!("Architecture: x86_64\n"));
        s.push_str(&format!(
            "Heap: {} MiB\n",
            crate::allocator::HEAP_SIZE / 1024 / 1024
        ));

        let process_count = crate::process::PROCESS_TABLE.lock().processes.len();
        s.push_str(&format!("Processes: {}\n", process_count));
        s.push_str(&format!("Timezone: {}\n", self.timezone));
        s.push_str(&format!("Locale: {}\n", self.locale));
        s
    }
}

// ═══════════════════════════════════════════════════════════════════════
// HELPER FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════

/// Format a floating-point number for display
fn format_number(n: f64) -> String {
    if n == 0.0 {
        return String::from("0");
    }

    let negative = n < 0.0;
    let abs_n = if negative { -n } else { n };

    let int_part = abs_n as u64;
    let frac_part = ((abs_n - int_part as f64) * 1_000_000.0) as u64;

    let mut s = String::new();
    if negative {
        s.push('-');
    }

    // Integer part
    s.push_str(&format!("{}", int_part));

    // Fractional part (trim trailing zeros)
    if frac_part > 0 {
        let frac_str = format!("{:06}", frac_part);
        let trimmed = frac_str.trim_end_matches('0');
        if !trimmed.is_empty() {
            s.push('.');
            s.push_str(trimmed);
        }
    }

    s
}

/// Parse a string as f64 (basic no_std implementation)
fn parse_f64(s: &str) -> f64 {
    let s = s.trim();
    if s.is_empty() {
        return 0.0;
    }

    let negative = s.starts_with('-');
    let s = if negative || s.starts_with('+') {
        &s[1..]
    } else {
        s
    };

    let mut result: f64 = 0.0;
    let mut decimal = false;
    let mut decimal_place = 1.0;

    for ch in s.chars() {
        if ch == '.' {
            decimal = true;
            continue;
        }
        if let Some(d) = ch.to_digit(10) {
            if decimal {
                decimal_place *= 10.0;
                result += d as f64 / decimal_place;
            } else {
                result = result * 10.0 + d as f64;
            }
        }
    }

    if negative { -result } else { result }
}

// ═══════════════════════════════════════════════════════════════════════
// GLOBAL STATE
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    static ref APPS: Mutex<Vec<AppInstance>> = Mutex::new(Vec::new());
}

/// Launch an application
pub fn launch(app_id: AppId) -> usize {
    let mut apps = APPS.lock();
    let instance = match app_id {
        AppId::Calculator => AppInstance {
            id: app_id,
            state: AppState::Running,
            title: String::from("Calculator"),
            data: AppData::Calculator(CalculatorState::new()),
        },
        AppId::SystemMonitor => AppInstance {
            id: app_id,
            state: AppState::Running,
            title: String::from("System Monitor"),
            data: AppData::SystemMonitor(SystemMonitorState::new()),
        },
        AppId::TextEditor => AppInstance {
            id: app_id,
            state: AppState::Running,
            title: String::from("Text Editor"),
            data: AppData::TextEditor(TextEditorState::new()),
        },
        AppId::FileManager => AppInstance {
            id: app_id,
            state: AppState::Running,
            title: String::from("File Manager"),
            data: AppData::FileManager(FileManagerState::new()),
        },
        AppId::Settings => AppInstance {
            id: app_id,
            state: AppState::Running,
            title: String::from("Settings"),
            data: AppData::Settings(SettingsState::new()),
        },
        AppId::ImageViewer => AppInstance {
            id: app_id,
            state: AppState::Running,
            title: String::from("Image Viewer"),
            data: AppData::ImageViewer(ImageViewerState::new()),
        },
        _ => AppInstance {
            id: app_id,
            state: AppState::Running,
            title: format!("{:?}", app_id),
            data: AppData::None,
        },
    };
    let idx = apps.len();
    apps.push(instance);
    serial_println!("[KnoxOS] Launched {:?} (instance {})", app_id, idx);
    idx
}

/// Close an application instance
pub fn close(instance_idx: usize) {
    let mut apps = APPS.lock();
    if instance_idx < apps.len() {
        apps[instance_idx].state = AppState::Idle;
        serial_println!("[KnoxOS] Closed app instance {}", instance_idx);
    }
}

/// Get running application count
pub fn running_count() -> usize {
    APPS.lock()
        .iter()
        .filter(|a| a.state == AppState::Running)
        .count()
}

/// Initialize desktop applications framework
pub fn init() {
    serial_println!("[KnoxOS] Desktop applications framework initialized (12 apps available)");
}
