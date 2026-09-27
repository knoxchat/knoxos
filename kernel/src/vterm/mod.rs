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

/// Initialize the VT100 emulator module
pub fn init() {
    crate::serial_println!("[KnoxOS] VT100/ANSI terminal emulator initialized");
}
