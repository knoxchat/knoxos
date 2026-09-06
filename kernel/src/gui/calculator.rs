/// Calculator — Basic calculator with standard and scientific modes
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

use super::colors;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::window::{self, WindowContentType, WindowId};

// ─── State ──────────────────────────────────────────────────────────

struct CalcState {
    window_id: WindowId,
    /// Display string (current input or result)
    display: String,
    /// Stored value for binary operations
    stored: f64,
    /// Pending operation
    pending_op: Option<char>,
    /// Whether the next digit starts a new number
    new_number: bool,
    /// Last result for display
    history: String,
}

lazy_static! {
    static ref STATES: Mutex<Vec<CalcState>> = Mutex::new(Vec::new());
}

const BUTTON_ROWS: &[&[(&str, char)]] = &[
    &[("C", 'C'), ("±", 'N'), ("%", '%'), ("÷", '/')],
    &[("7", '7'), ("8", '8'), ("9", '9'), ("×", '*')],
    &[("4", '4'), ("5", '5'), ("6", '6'), ("−", '-')],
    &[("1", '1'), ("2", '2'), ("3", '3'), ("+", '+')],
    &[("0", '0'), (".", '.'), ("⌫", 'B'), ("=", '=')],
];

// ─── Public API ─────────────────────────────────────────────────────

pub fn open() {
    let mut win = window::Window::new("Calculator", 400, 140, 320, 440);
    win.content_type = WindowContentType::Calculator;
    win.resizable = false;
    let wid = win.id;

    let mut wm = window::WINDOW_MANAGER.lock();
    wm.add_window(win);
    drop(wm);

    STATES.lock().push(CalcState {
        window_id: wid,
        display: String::from("0"),
        stored: 0.0,
        pending_op: None,
        new_number: true,
        history: String::new(),
    });

    super::taskbar::add_entry(wid, "Calculator");
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

    // Background
    fb.fill_rect(area, Pixel::rgb(28, 28, 32));

    // ── Display area ──
    let display_h = 80;
    let display_rect = Rect::new(area.x, area.y, area.width, display_h as u32);
    fb.fill_rect(display_rect, Pixel::rgb(20, 20, 24));

    // History line (smaller, dimmer)
    if !state.history.is_empty() {
        let hist_display = if state.history.len() > 36 {
            &state.history[state.history.len() - 36..]
        } else {
            &state.history
        };
        fonts::draw_string_compact(
            fb,
            area.x + area.width as i32 - hist_display.len() as i32 * 7 - 12,
            area.y + 10,
            hist_display,
            Pixel::rgb(120, 120, 130),
            1,
        );
    }

    // Main display
    let display_text = if state.display.len() > 18 {
        &state.display[state.display.len() - 18..]
    } else {
        &state.display
    };
    // Draw at larger scale (2x)
    let text_w = display_text.len() as i32 * 14;
    fonts::draw_string_bold_compact(
        fb,
        area.x + area.width as i32 - text_w - 16,
        area.y + 38,
        display_text,
        colors::WHITE,
        2,
    );

    // ── Button grid ──
    let grid_y = area.y + display_h;
    let grid_h = area.height as i32 - display_h;
    let cols = 4;
    let rows = BUTTON_ROWS.len() as i32;
    let btn_w = area.width as i32 / cols;
    let btn_h = grid_h / rows;
    let gap = 2;

    for (ri, row) in BUTTON_ROWS.iter().enumerate() {
        for (ci, (label, _code)) in row.iter().enumerate() {
            let bx = area.x + ci as i32 * btn_w + gap;
            let by = grid_y + ri as i32 * btn_h + gap;
            let bw = (btn_w - gap * 2) as u32;
            let bh = (btn_h - gap * 2) as u32;

            // Button colors
            let (bg, fg) = match *label {
                "C" | "±" | "%" => (Pixel::rgb(60, 60, 68), colors::WHITE),
                "÷" | "×" | "−" | "+" | "=" => (Pixel::rgb(82, 139, 255), colors::WHITE),
                _ => (Pixel::rgb(48, 48, 55), Pixel::rgb(220, 220, 230)),
            };

            fb.fill_rounded_rect_aa(Rect::new(bx, by, bw, bh), bg, 6);

            // Center text
            let text_px_w = label.chars().count() as i32 * 7;
            let tx = bx + (bw as i32 - text_px_w) / 2;
            let ty = by + (bh as i32 - 12) / 2;
            fonts::draw_string_bold_compact(fb, tx, ty, label, fg, 1);
        }
    }

    drop(states);
}

// ─── Click Handling ─────────────────────────────────────────────────

pub fn handle_click(wid: WindowId, area: Rect, click_x: i32, click_y: i32) -> bool {
    let display_h = 80;
    let grid_y = area.y + display_h;

    if click_y < grid_y {
        return false; // Click on display, ignore
    }

    let grid_h = area.height as i32 - display_h;
    let cols = 4i32;
    let rows = BUTTON_ROWS.len() as i32;
    let btn_w = area.width as i32 / cols;
    let btn_h = grid_h / rows;

    let col = (click_x - area.x) / btn_w;
    let row = (click_y - grid_y) / btn_h;

    if row < 0 || row >= rows || col < 0 || col >= cols {
        return false;
    }

    let (_label, code) = BUTTON_ROWS[row as usize][col as usize];
    process_input(wid, code);
    super::sounds::click();
    true
}

fn process_input(wid: WindowId, code: char) {
    let mut states = STATES.lock();
    let state = match states.iter_mut().find(|s| s.window_id == wid) {
        Some(s) => s,
        None => return,
    };

    match code {
        '0'..='9' => {
            if state.new_number {
                state.display = String::from(if code == '0' { "0" } else { "" });
                if code != '0' {
                    state.display.push(code);
                }
                state.new_number = false;
            } else {
                if state.display == "0" && code != '0' {
                    state.display.clear();
                }
                if state.display.len() < 15 {
                    state.display.push(code);
                }
            }
        }
        '.' => {
            if state.new_number {
                state.display = String::from("0.");
                state.new_number = false;
            } else if !state.display.contains('.') {
                state.display.push('.');
            }
        }
        'C' => {
            state.display = String::from("0");
            state.stored = 0.0;
            state.pending_op = None;
            state.new_number = true;
            state.history.clear();
        }
        'N' => {
            // Negate
            if state.display.starts_with('-') {
                state.display = String::from(&state.display[1..]);
            } else if state.display != "0" {
                state.display = format!("-{}", state.display);
            }
        }
        '%' => {
            if let Ok(val) = parse_display(&state.display) {
                let result = val / 100.0;
                state.display = format_result(result);
                state.new_number = true;
            }
        }
        'B' => {
            // Backspace
            if !state.new_number && state.display.len() > 1 {
                state.display.pop();
            } else {
                state.display = String::from("0");
                state.new_number = true;
            }
        }
        '+' | '-' | '*' | '/' => {
            if let Some(op) = state.pending_op {
                // Chain operations
                if let Ok(val) = parse_display(&state.display) {
                    let result = calculate(state.stored, val, op);
                    state.history = format!(
                        "{} {} {}",
                        format_result(state.stored),
                        op_symbol(op),
                        format_result(val)
                    );
                    state.stored = result;
                    state.display = format_result(result);
                }
            } else if let Ok(val) = parse_display(&state.display) {
                state.stored = val;
            }
            state.pending_op = Some(code);
            state.new_number = true;
        }
        '=' => {
            if let Some(op) = state.pending_op {
                if let Ok(val) = parse_display(&state.display) {
                    let result = calculate(state.stored, val, op);
                    state.history = format!(
                        "{} {} {} =",
                        format_result(state.stored),
                        op_symbol(op),
                        format_result(val)
                    );
                    state.display = format_result(result);
                    state.stored = result;
                    state.pending_op = None;
                    state.new_number = true;
                }
            }
        }
        _ => {}
    }
}

fn calculate(a: f64, b: f64, op: char) -> f64 {
    match op {
        '+' => a + b,
        '-' => a - b,
        '*' => a * b,
        '/' => {
            if b == 0.0 {
                f64::NAN
            } else {
                a / b
            }
        }
        _ => b,
    }
}

fn op_symbol(op: char) -> &'static str {
    match op {
        '+' => "+",
        '-' => "−",
        '*' => "×",
        '/' => "÷",
        _ => "?",
    }
}

fn format_result(val: f64) -> String {
    if val.is_nan() {
        return String::from("Error");
    }
    if val.is_infinite() {
        return String::from("Infinity");
    }
    // If it's a whole number, display without decimals
    if val == (val as i64) as f64 && val.abs() < 1e15 {
        format!("{}", val as i64)
    } else {
        // Show up to 10 decimal places, trimming trailing zeros
        let s = format!("{:.10}", val);
        let s = s.trim_end_matches('0');
        let s = s.trim_end_matches('.');
        String::from(s)
    }
}

fn parse_display(s: &str) -> Result<f64, ()> {
    // Simple integer/float parser for no_std
    // Handle negative
    let (neg, s) = if let Some(rest) = s.strip_prefix('-') {
        (true, rest)
    } else {
        (false, s)
    };

    if s == "Error" || s == "Infinity" {
        return Err(());
    }

    let mut int_part: f64 = 0.0;
    let mut frac_part: f64 = 0.0;
    let mut frac_div: f64 = 1.0;
    let mut in_frac = false;

    for c in s.chars() {
        if c == '.' {
            in_frac = true;
            continue;
        }
        if let Some(d) = c.to_digit(10) {
            if in_frac {
                frac_div *= 10.0;
                frac_part += d as f64 / frac_div;
            } else {
                int_part = int_part * 10.0 + d as f64;
            }
        } else {
            return Err(());
        }
    }

    let val = int_part + frac_part;
    Ok(if neg { -val } else { val })
}
