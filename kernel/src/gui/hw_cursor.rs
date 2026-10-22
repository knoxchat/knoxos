//! Hardware Cursor — GPU overlay cursor rendering
//!
//! Provides hardware cursor support via VGA/VBE cursor registers,
//! eliminating the need for software cursor save/restore and
//! reducing cursor latency.
//! Covers status.md item 7.38 (Hardware cursor).

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

/// Hardware cursor state
#[derive(Debug, Clone, Copy)]
pub struct HwCursorState {
    pub x: i32,
    pub y: i32,
    pub visible: bool,
    pub enabled: bool,
    pub width: u32,
    pub height: u32,
}

/// Cursor plane (GPU overlay)
#[derive(Debug)]
struct CursorPlane {
    /// MMIO base address for cursor registers (if available)
    mmio_base: Option<u64>,
    /// Cursor image buffer physical address
    image_phys_addr: u64,
    /// Current cursor data (ARGB, 64×64 max)
    image_data: [u32; 64 * 64],
    state: HwCursorState,
}

lazy_static::lazy_static! {
    static ref CURSOR: Mutex<CursorPlane> = Mutex::new(CursorPlane {
        mmio_base: None,
        image_phys_addr: 0,
        image_data: [0; 64 * 64],
        state: HwCursorState {
            x: 0,
            y: 0,
            visible: true,
            enabled: false,
            width: 32,
            height: 32,
        },
    });
}

static HW_CURSOR_AVAILABLE: AtomicBool = AtomicBool::new(false);
static MOVE_COUNT: AtomicU64 = AtomicU64::new(0);

// BGA (Bochs VGA) cursor register offsets
const VBE_DISPI_INDEX_CURSOR_X: u16 = 0x0E;
const VBE_DISPI_INDEX_CURSOR_Y: u16 = 0x0F;
const VBE_DISPI_IOPORT_INDEX: u16 = 0x01CE;
const VBE_DISPI_IOPORT_DATA: u16 = 0x01CF;

/// Write a VBE register (x86_64 only — BGA I/O ports)
#[cfg(target_arch = "x86_64")]
fn vbe_write(index: u16, value: u16) {
    // SAFETY: I/O port access is exclusive to this driver path; the port number is a fixed architecture or BAR-derived address.
    unsafe {
        #[cfg(target_arch = "x86_64")]
        use crate::arch_compat::instructions::port::Port;
        #[cfg(not(target_arch = "x86_64"))]
        use crate::arch_compat::instructions::port::Port;
        let mut idx_port: Port<u16> = Port::new(VBE_DISPI_IOPORT_INDEX);
        let mut data_port: Port<u16> = Port::new(VBE_DISPI_IOPORT_DATA);
        idx_port.write(index);
        data_port.write(value);
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn vbe_write(_index: u16, _value: u16) {
    // BGA I/O ports not available on aarch64/riscv64
}

/// Detect hardware cursor support by probing VGA/BGA registers and VirtIO GPU
pub fn detect() -> bool {
    let mut available = false;

    // Method 1: Check for BGA (Bochs Graphics Adapter) via I/O ports (x86_64 only)
    #[cfg(target_arch = "x86_64")]
    {
        let bga_id: u16;
        // SAFETY: I/O port access is exclusive to this driver path; the port number is a fixed architecture or BAR-derived address.
        unsafe {
            #[cfg(target_arch = "x86_64")]
            use crate::arch_compat::instructions::port::Port;
            #[cfg(not(target_arch = "x86_64"))]
            use crate::arch_compat::instructions::port::Port;
            let mut idx_port: Port<u16> = Port::new(VBE_DISPI_IOPORT_INDEX);
            let mut data_port: Port<u16> = Port::new(VBE_DISPI_IOPORT_DATA);
            idx_port.write(0x00); // VBE_DISPI_INDEX_ID
            bga_id = data_port.read();
        }

        // BGA ID should be 0xB0C0..0xB0C5 for Bochs VGA. Those ports are
        // not a cursor overlay — keep using the software cursor unless
        // VirtIO-GPU cursorq is live.
        if (0xB0C0..=0xB0CF).contains(&bga_id) {
            crate::serial_println!(
                "[hw_cursor] BGA detected: ID=0x{:04X} (no cursor plane)",
                bga_id
            );
        }
    }

    // Method 2: live VirtIO-GPU cursorq (Gate F8). BGA I/O ports are not a
    // cursor plane — only enable hardware cursor when the host consumed
    // VIRTIO_GPU_CMD_UPDATE_CURSOR.
    if crate::virtio_gpu::cursor_live() {
        available = true;
        crate::serial_println!("[hw_cursor] VirtIO GPU cursor plane live");
    }

    HW_CURSOR_AVAILABLE.store(available, Ordering::Relaxed);
    available
}

/// Enable hardware cursor overlay
pub fn enable() {
    if !HW_CURSOR_AVAILABLE.load(Ordering::Relaxed) {
        return;
    }
    let mut cursor = CURSOR.lock();
    cursor.state.enabled = true;
    crate::serial_println!("[hw_cursor] Hardware cursor enabled");
}

/// Disable hardware cursor (fall back to software cursor)
pub fn disable() {
    let mut cursor = CURSOR.lock();
    cursor.state.enabled = false;
}

/// Update cursor position (very fast — single register write)
pub fn set_position(x: i32, y: i32) {
    MOVE_COUNT.fetch_add(1, Ordering::Relaxed);
    let mut cursor = CURSOR.lock();
    cursor.state.x = x;
    cursor.state.y = y;

    if cursor.state.enabled {
        if crate::virtio_gpu::cursor_live() {
            crate::virtio_gpu::hw_move_cursor(x, y);
        } else if x >= 0 && y >= 0 {
            vbe_write(VBE_DISPI_INDEX_CURSOR_X, x as u16);
            vbe_write(VBE_DISPI_INDEX_CURSOR_Y, y as u16);
        }
    }
}

/// Upload cursor image (ARGB, max 64×64) and sync to GPU hardware
pub fn set_image(data: &[u32], width: u32, height: u32) {
    let w = width.min(64) as usize;
    let h = height.min(64) as usize;
    let mut cursor = CURSOR.lock();
    cursor.state.width = w as u32;
    cursor.state.height = h as u32;

    // Copy image data
    for y in 0..h {
        for x in 0..w {
            let si = y * (width as usize) + x;
            let di = y * 64 + x;
            if si < data.len() {
                cursor.image_data[di] = data[si];
            }
        }
    }

    // Upload to VirtIO GPU cursor plane if available
    if cursor.mmio_base.is_some() {
        // Create a cursor resource and upload the image data
        // VirtIO GPU cursor images must be exactly 64×64 BGRA
        let mut cursor_bytes = alloc::vec![0u8; 64 * 64 * 4];
        for y in 0..64usize {
            for x in 0..64usize {
                let pixel = cursor.image_data[y * 64 + x];
                let off = (y * 64 + x) * 4;
                cursor_bytes[off] = (pixel & 0xFF) as u8; // B
                cursor_bytes[off + 1] = ((pixel >> 8) & 0xFF) as u8; // G
                cursor_bytes[off + 2] = ((pixel >> 16) & 0xFF) as u8; // R
                cursor_bytes[off + 3] = ((pixel >> 24) & 0xFF) as u8; // A
            }
        }
        // Upload via VirtIO GPU update_cursor command
        let _ = crate::virtio_gpu::update_cursor(
            cursor.state.x as u32,
            cursor.state.y as u32,
            0, // scanout_id
            0, // resource_id (cursor resource)
            0, // hot_x
            0, // hot_y
        );
        crate::serial_println!(
            "[hw_cursor] Cursor image uploaded to VirtIO GPU ({}x{})",
            w,
            h
        );
    }
}

/// Show/hide cursor
pub fn set_visible(visible: bool) {
    CURSOR.lock().state.visible = visible;
}

/// Check if hardware cursor is available and enabled
pub fn is_enabled() -> bool {
    CURSOR.lock().state.enabled
}

/// Check if hardware cursor is available
pub fn is_available() -> bool {
    HW_CURSOR_AVAILABLE.load(Ordering::Relaxed)
}

/// Get cursor move count
pub fn move_count() -> u64 {
    MOVE_COUNT.load(Ordering::Relaxed)
}

/// Initialize hardware cursor subsystem
pub fn init() {
    let available = detect();
    if available {
        // QEMU cocoa/GTK does not composite virtio-gpu cursorq onto the
        // window. The visible pointer is the software arrow in the GPU
        // scanout (host cursor is also on in ./run.sh for window testing).
        crate::serial_println!(
            "[hw_cursor] VirtIO-GPU cursorq live — software arrow on GPU scanout"
        );
    } else {
        crate::serial_println!("[hw_cursor] No hardware cursor — using software cursor");
    }
}
