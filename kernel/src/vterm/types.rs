//! Screen cells, colors, graphics image records, and parser states.
use alloc::vec::Vec;

/// Sixel graphics image (P8.2)
#[derive(Debug, Clone)]
pub struct SixelImage {
    /// Row where the image starts
    pub row: usize,
    /// Column where the image starts
    pub col: usize,
    /// Image width in pixels
    pub width: usize,
    /// Image height in pixels
    pub height: usize,
    /// RGBA pixel data (width * height * 4)
    pub pixels: Vec<u8>,
}

/// Kitty graphics protocol image (P8.3)
#[derive(Debug, Clone)]
pub struct KittyImage {
    /// Image ID
    pub id: u32,
    /// Placement row
    pub row: usize,
    /// Placement col
    pub col: usize,
    /// Image width
    pub width: usize,
    /// Image height
    pub height: usize,
    /// RGBA pixel data
    pub pixels: Vec<u8>,
    /// Z-index for layering
    pub z_index: i32,
}

/// A single cell on screen
#[derive(Debug, Clone, Copy)]
pub struct Cell {
    pub ch: char,
    pub attr: CellAttr,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            attr: CellAttr::default(),
        }
    }
}

/// Cell text attributes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellAttr {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub blink: bool,
    pub inverse: bool,
    pub hidden: bool,
    pub strikethrough: bool,
}

impl Default for CellAttr {
    fn default() -> Self {
        Self {
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            blink: false,
            inverse: false,
            hidden: false,
            strikethrough: false,
        }
    }
}

/// Terminal colors (16 standard + 256 extended + 24-bit)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Default,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl Color {
    /// Convert to RGBA for rendering
    pub fn to_rgba(&self, is_bold: bool) -> (u8, u8, u8, u8) {
        match self {
            Color::Default => {
                if is_bold {
                    (255, 255, 255, 255)
                } else {
                    (204, 204, 204, 255)
                }
            }
            Color::Black => {
                if is_bold {
                    (85, 85, 85, 255)
                } else {
                    (0, 0, 0, 255)
                }
            }
            Color::Red => {
                if is_bold {
                    (255, 85, 85, 255)
                } else {
                    (170, 0, 0, 255)
                }
            }
            Color::Green => {
                if is_bold {
                    (85, 255, 85, 255)
                } else {
                    (0, 170, 0, 255)
                }
            }
            Color::Yellow => {
                if is_bold {
                    (255, 255, 85, 255)
                } else {
                    (170, 170, 0, 255)
                }
            }
            Color::Blue => {
                if is_bold {
                    (85, 85, 255, 255)
                } else {
                    (0, 0, 170, 255)
                }
            }
            Color::Magenta => {
                if is_bold {
                    (255, 85, 255, 255)
                } else {
                    (170, 0, 170, 255)
                }
            }
            Color::Cyan => {
                if is_bold {
                    (85, 255, 255, 255)
                } else {
                    (0, 170, 170, 255)
                }
            }
            Color::White => {
                if is_bold {
                    (255, 255, 255, 255)
                } else {
                    (170, 170, 170, 255)
                }
            }
            Color::BrightBlack => (85, 85, 85, 255),
            Color::BrightRed => (255, 85, 85, 255),
            Color::BrightGreen => (85, 255, 85, 255),
            Color::BrightYellow => (255, 255, 85, 255),
            Color::BrightBlue => (85, 85, 255, 255),
            Color::BrightMagenta => (255, 85, 255, 255),
            Color::BrightCyan => (85, 255, 255, 255),
            Color::BrightWhite => (255, 255, 255, 255),
            Color::Indexed(idx) => index_to_rgb(*idx),
            Color::Rgb(r, g, b) => (*r, *g, *b, 255),
        }
    }

    pub fn to_bg_rgba(&self) -> (u8, u8, u8, u8) {
        match self {
            Color::Default => (0, 0, 0, 255),
            _ => self.to_rgba(false),
        }
    }
}

/// Convert 256-color index to RGB
pub fn index_to_rgb(idx: u8) -> (u8, u8, u8, u8) {
    match idx {
        0 => (0, 0, 0, 255),
        1 => (170, 0, 0, 255),
        2 => (0, 170, 0, 255),
        3 => (170, 170, 0, 255),
        4 => (0, 0, 170, 255),
        5 => (170, 0, 170, 255),
        6 => (0, 170, 170, 255),
        7 => (170, 170, 170, 255),
        8 => (85, 85, 85, 255),
        9 => (255, 85, 85, 255),
        10 => (85, 255, 85, 255),
        11 => (255, 255, 85, 255),
        12 => (85, 85, 255, 255),
        13 => (255, 85, 255, 255),
        14 => (85, 255, 255, 255),
        15 => (255, 255, 255, 255),
        16..=231 => {
            let idx = idx - 16;
            let r = (idx / 36) * 51;
            let g = ((idx / 6) % 6) * 51;
            let b = (idx % 6) * 51;
            (r, g, b, 255)
        }
        232..=255 => {
            let gray = 8 + (idx - 232) * 10;
            (gray, gray, gray, 255)
        }
    }
}

/// Parser state for escape sequences
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserState {
    Normal,
    Escape,    // Got ESC
    Csi,       // Got ESC [
    Osc,       // Got ESC ]
    OscString, // Inside OSC string
    SixelData, // Sixel graphics data
    Dcs,       // Device Control String
}
