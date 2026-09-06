/// Task Manager — Displays running processes, windows, memory, and CPU info
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

use super::colors;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::window::{self, WindowContentType, WindowId};

const ARCH_NAME: &str = if cfg!(target_arch = "x86_64") {
    "x86_64"
} else if cfg!(target_arch = "aarch64") {
    "aarch64"
} else {
    "riscv64"
};

// ─── Types ──────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub enum TaskManagerTab {
    Processes,
    Windows,
    Performance,
}

struct TaskManagerState {
    window_id: WindowId,
    tab: TaskManagerTab,
    selected: i32,
    scroll_y: i32,
}

lazy_static! {
    static ref STATES: Mutex<Vec<TaskManagerState>> = Mutex::new(Vec::new());
}

// ─── Constants ──────────────────────────────────────────────────────

const TAB_HEIGHT: i32 = 32;
const ROW_HEIGHT: i32 = 24;
const HEADER_ROW_HEIGHT: i32 = 24;
const SIDEBAR_NONE: i32 = 0;

const TAB_LABELS: &[(&str, TaskManagerTab)] = &[
    ("Processes", TaskManagerTab::Processes),
    ("Windows", TaskManagerTab::Windows),
    ("Performance", TaskManagerTab::Performance),
];

// ─── Public API ─────────────────────────────────────────────────────

pub fn open() {
    let mut win = window::Window::new("Task Manager", 250, 100, 700, 520);
    win.content_type = WindowContentType::TaskManager;
    let wid = win.id;

    let mut wm = window::WINDOW_MANAGER.lock();
    wm.add_window(win);
    drop(wm);

    STATES.lock().push(TaskManagerState {
        window_id: wid,
        tab: TaskManagerTab::Processes,
        selected: -1,
        scroll_y: 0,
    });

    super::taskbar::add_entry(wid, "Task Manager");
    super::taskbar::set_active(wid);
    super::sounds::window_open();
}

pub fn close(wid: WindowId) {
    STATES.lock().retain(|s| s.window_id != wid);
}

pub fn draw_content(fb: &mut FrameBuffer, wid: WindowId, area: Rect, _scroll_y: i32) {
    let states = STATES.lock();
    let state = match states.iter().find(|s| s.window_id == wid) {
        Some(s) => s,
        None => return,
    };
    let tab = state.tab;
    let selected = state.selected;
    let scroll = state.scroll_y;
    drop(states);

    // Background
    fb.fill_rect(area, Pixel::rgb(22, 22, 26));

    // ─── Tab bar ─────────────────────────────────────────────
    let tab_y = area.y;
    fb.fill_rect(
        Rect::new(area.x, tab_y, area.width, TAB_HEIGHT as u32),
        Pixel::rgb(30, 30, 36),
    );

    let tab_w = area.width as i32 / TAB_LABELS.len() as i32;
    for (i, (label, t)) in TAB_LABELS.iter().enumerate() {
        let tx = area.x + i as i32 * tab_w;
        let is_active = tab == *t;
        if is_active {
            fb.fill_rect(
                Rect::new(tx, tab_y, tab_w as u32, TAB_HEIGHT as u32),
                Pixel::rgb(44, 44, 52),
            );
            // Active indicator
            fb.fill_rect(
                Rect::new(tx, tab_y + TAB_HEIGHT - 2, tab_w as u32, 2),
                Pixel::rgb(82, 139, 255),
            );
        }
        let color = if is_active {
            colors::WHITE
        } else {
            Pixel::rgb(140, 140, 150)
        };
        let text_x = tx + (tab_w - label.len() as i32 * 7) / 2;
        fonts::draw_string_compact(fb, text_x, tab_y + 10, label, color, 1);
    }

    let content_y = tab_y + TAB_HEIGHT;
    let content_h = area.height as i32 - TAB_HEIGHT;

    match tab {
        TaskManagerTab::Processes => draw_processes_tab(
            fb,
            area.x,
            content_y,
            area.width as i32,
            content_h,
            selected,
            scroll,
        ),
        TaskManagerTab::Windows => draw_windows_tab(
            fb,
            area.x,
            content_y,
            area.width as i32,
            content_h,
            selected,
            scroll,
        ),
        TaskManagerTab::Performance => {
            draw_performance_tab(fb, area.x, content_y, area.width as i32, content_h)
        }
    }
}

// ─── Processes Tab ──────────────────────────────────────────────────

fn draw_processes_tab(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    selected: i32,
    scroll: i32,
) {
    // Column headers
    let header_bg = Pixel::rgb(34, 34, 40);
    fb.fill_rect(
        Rect::new(x, y, w as u32, HEADER_ROW_HEIGHT as u32),
        header_bg,
    );
    let hdr = Pixel::rgb(160, 170, 190);
    fonts::draw_string_compact(fb, x + 12, y + 6, "PID", hdr, 1);
    fonts::draw_string_compact(fb, x + 60, y + 6, "Name", hdr, 1);
    fonts::draw_string_compact(fb, x + 320, y + 6, "State", hdr, 1);
    fonts::draw_string_compact(fb, x + 440, y + 6, "Priority", hdr, 1);
    fonts::draw_string_compact(fb, x + 560, y + 6, "UID", hdr, 1);

    fb.draw_hline(
        x,
        y + HEADER_ROW_HEIGHT - 1,
        w as u32,
        Pixel::rgb(50, 50, 58),
    );

    // Gather process data
    let pt = crate::process::PROCESS_TABLE.lock();
    let procs: Vec<(u32, String, String, i8, u32)> = pt
        .list_processes()
        .iter()
        .map(|p| {
            let state_str = match p.state {
                crate::process::ProcessState::Running => "Running",
                crate::process::ProcessState::Ready => "Ready",
                crate::process::ProcessState::Sleeping => "Sleeping",
                crate::process::ProcessState::Stopped => "Stopped",
                crate::process::ProcessState::Zombie => "Zombie",
            };
            (
                p.pid,
                p.name.clone(),
                String::from(state_str),
                p.priority,
                p.uid,
            )
        })
        .collect();
    drop(pt);

    let row_start = y + HEADER_ROW_HEIGHT;
    let max_rows = (h - HEADER_ROW_HEIGHT) / ROW_HEIGHT;

    for (i, (pid, name, state, prio, uid)) in procs.iter().enumerate() {
        let row_i = i as i32 - scroll;
        if row_i < 0 || row_i >= max_rows {
            continue;
        }
        let ry = row_start + row_i * ROW_HEIGHT;

        // Selection highlight
        if i as i32 == selected {
            fb.fill_rect(
                Rect::new(x, ry, w as u32, ROW_HEIGHT as u32),
                Pixel::rgb(45, 55, 80),
            );
        } else if row_i % 2 == 1 {
            fb.fill_rect(
                Rect::new(x, ry, w as u32, ROW_HEIGHT as u32),
                Pixel::rgb(26, 26, 30),
            );
        }

        let tc = if state == "Running" {
            Pixel::rgb(80, 230, 120)
        } else if state == "Zombie" {
            Pixel::rgb(255, 80, 80)
        } else {
            Pixel::rgb(200, 200, 210)
        };

        fonts::draw_string_compact(fb, x + 12, ry + 6, &format!("{}", pid), tc, 1);
        // Truncate name to fit
        let display_name = if name.len() > 36 { &name[..36] } else { name };
        fonts::draw_string_compact(
            fb,
            x + 60,
            ry + 6,
            display_name,
            Pixel::rgb(220, 220, 230),
            1,
        );
        fonts::draw_string_compact(fb, x + 320, ry + 6, state, tc, 1);
        fonts::draw_string_compact(
            fb,
            x + 440,
            ry + 6,
            &format!("{}", prio),
            Pixel::rgb(180, 180, 190),
            1,
        );
        fonts::draw_string_compact(
            fb,
            x + 560,
            ry + 6,
            &format!("{}", uid),
            Pixel::rgb(180, 180, 190),
            1,
        );
    }

    // Bottom info bar
    let bar_y = y + h - 28;
    fb.fill_rect(Rect::new(x, bar_y, w as u32, 28), Pixel::rgb(30, 30, 36));
    fb.draw_hline(x, bar_y, w as u32, Pixel::rgb(50, 50, 58));
    let count = procs.len();
    fonts::draw_string_compact(
        fb,
        x + 12,
        bar_y + 8,
        &format!("{} processes", count),
        Pixel::rgb(140, 140, 150),
        1,
    );

    // End Process button
    if selected >= 0 && (selected as usize) < count {
        let btn_w = 100i32;
        let btn_x = x + w - btn_w - 12;
        let btn_y = bar_y + 3;
        fb.fill_rounded_rect_aa(
            Rect::new(btn_x, btn_y, btn_w as u32, 22),
            Pixel::rgb(180, 50, 50),
            4,
        );
        fonts::draw_string_compact(fb, btn_x + 14, btn_y + 5, "End Process", colors::WHITE, 1);
    }
}

// ─── Windows Tab ────────────────────────────────────────────────────

fn draw_windows_tab(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    selected: i32,
    scroll: i32,
) {
    // Header
    let header_bg = Pixel::rgb(34, 34, 40);
    fb.fill_rect(
        Rect::new(x, y, w as u32, HEADER_ROW_HEIGHT as u32),
        header_bg,
    );
    let hdr = Pixel::rgb(160, 170, 190);
    fonts::draw_string_compact(fb, x + 12, y + 6, "ID", hdr, 1);
    fonts::draw_string_compact(fb, x + 60, y + 6, "Title", hdr, 1);
    fonts::draw_string_compact(fb, x + 360, y + 6, "Type", hdr, 1);
    fonts::draw_string_compact(fb, x + 500, y + 6, "State", hdr, 1);
    fb.draw_hline(
        x,
        y + HEADER_ROW_HEIGHT - 1,
        w as u32,
        Pixel::rgb(50, 50, 58),
    );

    // Collect window info
    let wm = window::WINDOW_MANAGER.lock();
    let windows: Vec<(WindowId, String, String, String)> = wm
        .windows
        .iter()
        .map(|win| {
            let type_str = match win.content_type {
                WindowContentType::Terminal => "Terminal",
                WindowContentType::FileExplorer => "File Explorer",
                WindowContentType::Browser => "Browser",
                WindowContentType::AIAssistant => "AI Assistant",
                WindowContentType::TextEditor => "Text Editor",
                WindowContentType::Settings => "Settings",
                WindowContentType::TaskManager => "Task Manager",
                _ => "Application",
            };
            let state_str = if !win.visible {
                "Hidden"
            } else if win.state == window::WindowState::Minimized {
                "Minimized"
            } else if win.state == window::WindowState::Maximized {
                "Maximized"
            } else {
                "Normal"
            };
            (
                win.id,
                win.title.clone(),
                String::from(type_str),
                String::from(state_str),
            )
        })
        .collect();
    drop(wm);

    let row_start = y + HEADER_ROW_HEIGHT;
    let max_rows = (h - HEADER_ROW_HEIGHT - 28) / ROW_HEIGHT;

    for (i, (id, title, wtype, wstate)) in windows.iter().enumerate() {
        let row_i = i as i32 - scroll;
        if row_i < 0 || row_i >= max_rows {
            continue;
        }
        let ry = row_start + row_i * ROW_HEIGHT;

        if i as i32 == selected {
            fb.fill_rect(
                Rect::new(x, ry, w as u32, ROW_HEIGHT as u32),
                Pixel::rgb(45, 55, 80),
            );
        } else if row_i % 2 == 1 {
            fb.fill_rect(
                Rect::new(x, ry, w as u32, ROW_HEIGHT as u32),
                Pixel::rgb(26, 26, 30),
            );
        }

        let tc = Pixel::rgb(200, 200, 210);
        fonts::draw_string_compact(fb, x + 12, ry + 6, &format!("{}", id), tc, 1);
        let display_title = if title.len() > 38 {
            &title[..38]
        } else {
            title
        };
        fonts::draw_string_compact(
            fb,
            x + 60,
            ry + 6,
            display_title,
            Pixel::rgb(220, 220, 230),
            1,
        );
        fonts::draw_string_compact(fb, x + 360, ry + 6, wtype, Pixel::rgb(130, 180, 255), 1);
        fonts::draw_string_compact(fb, x + 500, ry + 6, wstate, Pixel::rgb(180, 180, 190), 1);
    }

    // Bottom bar
    let bar_y = y + h - 28;
    fb.fill_rect(Rect::new(x, bar_y, w as u32, 28), Pixel::rgb(30, 30, 36));
    fb.draw_hline(x, bar_y, w as u32, Pixel::rgb(50, 50, 58));
    fonts::draw_string_compact(
        fb,
        x + 12,
        bar_y + 8,
        &format!("{} windows", windows.len()),
        Pixel::rgb(140, 140, 150),
        1,
    );
}

// ─── Performance Tab ────────────────────────────────────────────────

fn draw_performance_tab(fb: &mut FrameBuffer, x: i32, y: i32, w: i32, _h: i32) {
    let info = crate::sysinfo::get_sysinfo();

    let mut cy = y + 16;

    // ── Memory Section ──
    fonts::draw_string_bold_compact(fb, x + 16, cy, "Memory", Pixel::rgb(82, 139, 255), 1);
    cy += 22;

    let total_mb = info.totalram / (1024 * 1024);
    let free_mb = info.freeram / (1024 * 1024);
    let used_mb = total_mb.saturating_sub(free_mb);
    let usage_pct = (used_mb * 100).checked_div(total_mb).unwrap_or(0) as i32;

    // Memory bar
    let bar_x = x + 16;
    let bar_w = w - 32;
    let bar_h = 18;
    fb.fill_rounded_rect_aa(
        Rect::new(bar_x, cy, bar_w as u32, bar_h as u32),
        Pixel::rgb(40, 40, 48),
        4,
    );
    let filled_w = ((bar_w as i64 * usage_pct as i64) / 100).max(1) as u32;
    let bar_color = if usage_pct > 85 {
        Pixel::rgb(230, 60, 60)
    } else if usage_pct > 60 {
        Pixel::rgb(230, 180, 40)
    } else {
        Pixel::rgb(60, 180, 100)
    };
    if filled_w > 0 {
        fb.fill_rounded_rect_aa(Rect::new(bar_x, cy, filled_w, bar_h as u32), bar_color, 4);
    }
    // Percentage text on bar
    let pct_text = format!("{}%", usage_pct);
    fonts::draw_string_compact(
        fb,
        bar_x + bar_w / 2 - 10,
        cy + 3,
        &pct_text,
        colors::WHITE,
        1,
    );
    cy += bar_h + 8;

    // Memory details
    let detail_color = Pixel::rgb(180, 180, 195);
    fonts::draw_string_compact(
        fb,
        x + 16,
        cy,
        &format!("Total: {} MiB", total_mb),
        detail_color,
        1,
    );
    fonts::draw_string_compact(
        fb,
        x + 200,
        cy,
        &format!("Used: {} MiB", used_mb),
        detail_color,
        1,
    );
    fonts::draw_string_compact(
        fb,
        x + 380,
        cy,
        &format!("Free: {} MiB", free_mb),
        detail_color,
        1,
    );
    cy += 30;

    // ── CPU / Load ──
    fb.draw_hline(x + 16, cy, (w - 32) as u32, Pixel::rgb(50, 50, 58));
    cy += 12;
    fonts::draw_string_bold_compact(fb, x + 16, cy, "CPU / Load", Pixel::rgb(82, 139, 255), 1);
    cy += 22;

    // Load averages (scaled by 65536)
    let load1 = info.loads[0] as f64 / 65536.0;
    let load5 = info.loads[1] as f64 / 65536.0;
    let load15 = info.loads[2] as f64 / 65536.0;

    // Since we don't have floats easily, show as fixed-point
    let l1_int = (info.loads[0] * 100 / 65536) as u32;
    let l5_int = (info.loads[1] * 100 / 65536) as u32;
    let l15_int = (info.loads[2] * 100 / 65536) as u32;

    fonts::draw_string_compact(
        fb,
        x + 16,
        cy,
        &format!(
            "Load avg: {}.{:02}  {}.{:02}  {}.{:02}",
            l1_int / 100,
            l1_int % 100,
            l5_int / 100,
            l5_int % 100,
            l15_int / 100,
            l15_int % 100
        ),
        detail_color,
        1,
    );
    cy += 20;

    // Process count
    fonts::draw_string_compact(
        fb,
        x + 16,
        cy,
        &format!("Processes: {}", info.procs),
        detail_color,
        1,
    );
    cy += 30;

    // ── Uptime ──
    fb.draw_hline(x + 16, cy, (w - 32) as u32, Pixel::rgb(50, 50, 58));
    cy += 12;
    fonts::draw_string_bold_compact(fb, x + 16, cy, "System Uptime", Pixel::rgb(82, 139, 255), 1);
    cy += 22;

    let uptime_secs = info.uptime as u64;
    let days = uptime_secs / 86400;
    let hours = (uptime_secs % 86400) / 3600;
    let mins = (uptime_secs % 3600) / 60;
    let secs = uptime_secs % 60;

    fonts::draw_string_compact(
        fb,
        x + 16,
        cy,
        &format!("{}d {}h {}m {}s", days, hours, mins, secs),
        Pixel::rgb(220, 220, 230),
        1,
    );
    cy += 30;

    // ── Kernel Info ──
    fb.draw_hline(x + 16, cy, (w - 32) as u32, Pixel::rgb(50, 50, 58));
    cy += 12;
    fonts::draw_string_bold_compact(fb, x + 16, cy, "Kernel", Pixel::rgb(82, 139, 255), 1);
    cy += 22;

    fonts::draw_string_compact(
        fb,
        x + 16,
        cy,
        &format!("KnoxOS v0.2.1  ({})", ARCH_NAME),
        detail_color,
        1,
    );
    cy += 18;

    let ticks = crate::interrupts::get_ticks();
    fonts::draw_string_compact(
        fb,
        x + 16,
        cy,
        &format!("Tick count: {}", ticks),
        detail_color,
        1,
    );
}

// ─── Click Handling ─────────────────────────────────────────────────

pub fn handle_click(wid: WindowId, area: Rect, click_x: i32, click_y: i32) -> bool {
    let mut states = STATES.lock();
    let state = match states.iter_mut().find(|s| s.window_id == wid) {
        Some(s) => s,
        None => return false,
    };

    // Tab bar click
    let tab_y = area.y;
    if click_y >= tab_y && click_y < tab_y + TAB_HEIGHT {
        let tab_w = area.width as i32 / TAB_LABELS.len() as i32;
        let idx = (click_x - area.x) / tab_w;
        if idx >= 0 && (idx as usize) < TAB_LABELS.len() {
            state.tab = TAB_LABELS[idx as usize].1;
            state.selected = -1;
            state.scroll_y = 0;
            return true;
        }
    }

    let content_y = tab_y + TAB_HEIGHT;
    let content_h = area.height as i32 - TAB_HEIGHT;

    // Row click
    if click_y >= content_y + HEADER_ROW_HEIGHT && click_y < content_y + content_h - 28 {
        let row = (click_y - content_y - HEADER_ROW_HEIGHT) / ROW_HEIGHT + state.scroll_y;
        state.selected = row;

        // Check "End Process" button click in processes tab
        if state.tab == TaskManagerTab::Processes {
            let bar_y = content_y + content_h - 28;
            // (button checked separately below)
        }
        return true;
    }

    // Bottom bar - End Process button
    let bar_y = content_y + content_h - 28;
    if state.tab == TaskManagerTab::Processes && click_y >= bar_y && click_y < bar_y + 28 {
        let btn_w = 100;
        let btn_x = area.x + area.width as i32 - btn_w - 12;
        if click_x >= btn_x && click_x < btn_x + btn_w && state.selected >= 0 {
            let sel = state.selected as usize;
            drop(states);
            // Kill selected process
            let pt = crate::process::PROCESS_TABLE.lock();
            let pids: Vec<u32> = pt.list_processes().iter().map(|p| p.pid).collect();
            drop(pt);
            if sel < pids.len() {
                crate::process::kill(pids[sel]);
            }
            return true;
        }
    }

    false
}

pub fn handle_scroll(wid: WindowId, delta: i32) -> bool {
    let mut states = STATES.lock();
    if let Some(state) = states.iter_mut().find(|s| s.window_id == wid) {
        state.scroll_y = (state.scroll_y - delta).max(0);
        true
    } else {
        false
    }
}
