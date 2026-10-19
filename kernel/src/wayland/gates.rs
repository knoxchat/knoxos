/// Gate F / Gate WL / Gate TERM markers, SHM magics, and kernel-only self-test.
use alloc::vec;
use alloc::vec::Vec;

use crate::serial_println;

use super::compositor::COMPOSITOR;

/// Gate F client buffer: 64×64 BGRA, magic first pixel `EE FF C0 FF`.
pub const GATE_F_WIDTH: u32 = 64;
pub const GATE_F_HEIGHT: u32 = 64;
pub const GATE_F1_MARKER: &str = "GATE_F1 client isolated";
pub const GATE_F2_MARKER: &str = "GATE_F2 shm commit";
pub const GATE_F3_MARKER: &str = "GATE_F3 terminal isolated";
pub const GATE_F4_MARKER: &str = "GATE_F4 launcher userspace";
pub const GATE_F5_MARKER: &str = "GATE_F5 wayland unix";
pub const GATE_WL_MARKER: &str = "GATE_WL display protocol";
pub const GATE_F6_MARKER: &str = "GATE_F6 calculator userspace";
pub const GATE_F7_MARKER: &str = "GATE_F7 files userspace";
pub const WAYLAND_SOCKET_PATH: &str = "/run/wayland-0";
const GATE_F_MAGIC: [u8; 4] = [0xEE, 0xFF, 0xC0, 0xFF];
pub(super) const GATE_F3_MAGIC: [u8; 4] = [0x1A, 0x1A, 0x1A, 0xFF];
pub(super) const GATE_F4_MAGIC: [u8; 4] = [0xCC, 0x66, 0x33, 0xFF];
pub(super) const GATE_F6_MAGIC: [u8; 4] = [0x97, 0xC9, 0x20, 0xFF];
pub(super) const GATE_F7_MAGIC: [u8; 4] = [0x26, 0x8A, 0x4E, 0xFF];
const GATE_F8_MAGIC: [u8; 4] = [0x40, 0x38, 0x38, 0xFF];
const GATE_F9_MAGIC: [u8; 4] = [0x48, 0x32, 0x48, 0xFF];
const GATE_F10_MAGIC: [u8; 4] = [0x1E, 0x1E, 0x1E, 0xFF];
const GATE_F11_MAGIC: [u8; 4] = [0x3A, 0x2D, 0x28, 0xFF];
const GATE_F12_MAGIC: [u8; 4] = [0x26, 0x16, 0x12, 0xFF];
pub(super) const GATE_TERM_MAGIC: [u8; 4] = [0x0D, 0x1A, 0x2B, 0xFF];
pub const GATE_TERM_MARKER: &str = "GATE_TERM pty userspace";
pub const GATE_TERM_IO_MARKER: &str = "GATE_TERM pty interactive";
pub const GATE_TERM_PAINT_MARKER: &str = "GATE_TERM shm paint";

pub(super) fn shm_title_for_magic(pixels: &[u8]) -> &'static str {
    let Some(head) = pixels.get(0..4) else {
        return "Client";
    };
    if head == GATE_F3_MAGIC {
        "Terminal"
    } else if head == GATE_F4_MAGIC {
        "Paint"
    } else if head == GATE_F6_MAGIC {
        "Calculator"
    } else if head == GATE_F7_MAGIC {
        "Files"
    } else if head == GATE_F8_MAGIC {
        "Settings"
    } else if head == GATE_F9_MAGIC {
        "Task Manager"
    } else if head == GATE_F10_MAGIC {
        "Editor"
    } else if head == GATE_F11_MAGIC {
        "Browser"
    } else if head == GATE_F12_MAGIC {
        "AI Assistant"
    } else if head == GATE_TERM_MAGIC {
        "Terminal"
    } else {
        "Client"
    }
}

pub(super) fn is_known_shm_magic(p: &[u8]) -> bool {
    p == GATE_F_MAGIC
        || p == GATE_F3_MAGIC
        || p == GATE_F4_MAGIC
        || p == GATE_F6_MAGIC
        || p == GATE_F7_MAGIC
        || p == GATE_F8_MAGIC
        || p == GATE_F9_MAGIC
        || p == GATE_F10_MAGIC
        || p == GATE_F11_MAGIC
        || p == GATE_F12_MAGIC
        || p == GATE_TERM_MAGIC
}

/// 5×7 glyphs for the first PTY frame so the Terminal client area is not a
/// 64×64 brown fill (`GATE_TERM shm paint`). Pixel 0 stays the magic.
pub(super) fn paint_term_first_frame(pixels: &mut [u8]) -> bool {
    if pixels.get(0..4) != Some(&GATE_TERM_MAGIC[..]) {
        return false;
    }
    const GLYPH_O: [u8; 7] = [
        0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
    ];
    const GLYPH_K: [u8; 7] = [
        0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
    ];
    let stamp = |pixels: &mut [u8], origin_x: usize, glyph: &[u8; 7]| {
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5 {
                if bits & (0x10 >> col) == 0 {
                    continue;
                }
                for dy in 0..2 {
                    for dx in 0..2 {
                        let x = origin_x + col * 2 + dx;
                        let y = 10 + row * 2 + dy;
                        if x == 0 && y == 0 {
                            continue;
                        }
                        if x >= GATE_F_WIDTH as usize || y >= GATE_F_HEIGHT as usize {
                            continue;
                        }
                        let off = (y * GATE_F_WIDTH as usize + x) * 4;
                        if off + 3 < pixels.len() {
                            pixels[off] = 0xEE;
                            pixels[off + 1] = 0xEE;
                            pixels[off + 2] = 0xF2;
                            pixels[off + 3] = 0xFF;
                        }
                    }
                }
            }
        }
    };
    stamp(pixels, 8, &GLYPH_O);
    stamp(pixels, 22, &GLYPH_K);
    let painted = pixels.chunks(4).skip(1).any(|p| p != GATE_TERM_MAGIC);
    if painted {
        serial_println!(
            "[WL] {} (first-frame glyphs on 64x64 SHM)",
            GATE_TERM_PAINT_MARKER
        );
    }
    painted
}

/// Kernel-only SHM pool write/read round-trip (no Ring 3).
pub fn shm_pool_self_test() -> bool {
    let len = 64;
    let mut payload = vec![0u8; len];
    payload[0..4].copy_from_slice(&GATE_F_MAGIC);
    let mut comp = COMPOSITOR.lock();
    let pool = comp.create_shm_pool(-1, len, 0);
    if !comp.write_to_pool(pool, 0, &payload) {
        return false;
    }
    let back = comp.read_from_pool(pool, 0, 4);
    comp.destroy_shm_pool(pool);
    back.as_deref() == Some(&GATE_F_MAGIC[..])
}
