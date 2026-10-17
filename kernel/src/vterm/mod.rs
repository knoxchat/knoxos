//! VT100/ANSI Terminal Emulator - Full escape sequence processing
//! Implements VT100, VT220, and xterm-compatible escape sequences

mod base64;
mod emulator;
mod graphics;
mod host;
mod osc;
mod parser;
mod screen;
mod selection;
mod sgr;
mod types;

pub use emulator::VtEmulator;
pub use types::{Cell, CellAttr, Color, KittyImage, ParserState, SixelImage, index_to_rgb};

pub const GATE_VT100_MARKER: &str = "GATE_VT100 xterm";

fn vt100_self_test() -> bool {
    let mut vt = VtEmulator::new(80, 24);
    vt.process_bytes(b"\x1b[2;3H\x1b[1m\x1b[38;5;196mA");
    let cell = vt.cells.get(1).and_then(|r| r.get(2));
    let pos_ok = vt.cursor_row == 1 && vt.cursor_col == 3;
    let color_ok = matches!(
        cell.map(|c| c.attr.fg),
        Some(crate::vterm::Color::Indexed(196))
    );
    let ch_ok = cell.map(|c| c.ch) == Some('A');
    let bold_ok = cell.map(|c| c.attr.bold) == Some(true);
    vt.process_bytes(b"\x1b[2J");
    let erased = vt
        .cells
        .get(1)
        .and_then(|r| r.get(2))
        .map(|c| c.ch == ' ')
        .unwrap_or(false);
    if pos_ok && color_ok && ch_ok && bold_ok && erased {
        crate::serial_println!("[vterm] {} (CUP+SGR256+ED)", GATE_VT100_MARKER);
        true
    } else {
        crate::serial_println!(
            "[vterm] GATE_VT100 FAILED pos={} color={} ch={} bold={} ed={}",
            pos_ok,
            color_ok,
            ch_ok,
            bold_ok,
            erased
        );
        false
    }
}

/// Initialize the VT100 emulator module
pub fn init() {
    crate::serial_println!("[KnoxOS] VT100/ANSI terminal emulator initialized");
    let _ = vt100_self_test();
}
