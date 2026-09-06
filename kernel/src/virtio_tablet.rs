/// VirtIO Tablet / USB Tablet Absolute Pointer Driver
///
/// Provides seamless mouse integration when running under QEMU with
/// `-usb -device usb-tablet`. QEMU's USB tablet device provides
/// absolute pointer coordinates, eliminating the dual-cursor problem
/// where both host and guest cursors are visible.
///
/// Architecture:
/// - QEMU's USB tablet translates absolute host cursor position
///   into PS/2 mouse deltas for the guest
/// - This module provides additional absolute coordinate tracking
///   via QEMU's fw_cfg or debug port for pixel-perfect positioning
/// - The GUI input handler prefers absolute coordinates when available,
///   falling back to PS/2 relative deltas
///
/// When `-usb -device usb-tablet` is used:
/// 1. QEMU hides the host cursor over the VM window (Cocoa/GTK display)
/// 2. The guest draws its own cursor at the correct position
/// 3. Mouse movement maps 1:1 to the QEMU window — no cursor grab needed
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── Absolute Mouse State ──────────────────────────────────────────

/// Whether USB tablet / absolute positioning mode is active
static TABLET_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Absolute cursor position (set by USB tablet events)
static ABS_X: AtomicI32 = AtomicI32::new(0);
static ABS_Y: AtomicI32 = AtomicI32::new(0);

/// Screen resolution for coordinate scaling
static SCREEN_WIDTH: AtomicU32 = AtomicU32::new(1024);
static SCREEN_HEIGHT: AtomicU32 = AtomicU32::new(768);

/// Button state from absolute input
static ABS_BUTTONS: AtomicU32 = AtomicU32::new(0);

// Button masks
pub const BTN_LEFT_MASK: u32 = 0x01;
pub const BTN_RIGHT_MASK: u32 = 0x02;
pub const BTN_MIDDLE_MASK: u32 = 0x04;

// ─── Public API ────────────────────────────────────────────────────

/// Check if tablet/absolute positioning mode is active
pub fn is_active() -> bool {
    TABLET_ACTIVE.load(Ordering::Relaxed)
}

/// Get absolute cursor position
pub fn get_position() -> (i32, i32) {
    (ABS_X.load(Ordering::Relaxed), ABS_Y.load(Ordering::Relaxed))
}

/// Set absolute cursor position (called from PS/2 handler when in tablet mode,
/// or from USB HID tablet report parsing)
pub fn set_position(x: i32, y: i32) {
    let sw = SCREEN_WIDTH.load(Ordering::Relaxed) as i32;
    let sh = SCREEN_HEIGHT.load(Ordering::Relaxed) as i32;
    ABS_X.store(x.clamp(0, sw - 1), Ordering::Relaxed);
    ABS_Y.store(y.clamp(0, sh - 1), Ordering::Relaxed);
}

/// Update screen resolution (called when framebuffer is initialized)
pub fn set_screen_size(w: u32, h: u32) {
    SCREEN_WIDTH.store(w, Ordering::Relaxed);
    SCREEN_HEIGHT.store(h, Ordering::Relaxed);
}

/// Get button state
pub fn get_buttons() -> (bool, bool, bool) {
    let btns = ABS_BUTTONS.load(Ordering::Relaxed);
    (
        btns & BTN_LEFT_MASK != 0,
        btns & BTN_RIGHT_MASK != 0,
        btns & BTN_MIDDLE_MASK != 0,
    )
}

/// Set button state
pub fn set_buttons(left: bool, right: bool, middle: bool) {
    let mut btns = 0u32;
    if left {
        btns |= BTN_LEFT_MASK;
    }
    if right {
        btns |= BTN_RIGHT_MASK;
    }
    if middle {
        btns |= BTN_MIDDLE_MASK;
    }
    ABS_BUTTONS.store(btns, Ordering::Relaxed);
}

/// Activate tablet mode - called when USB tablet device is detected
pub fn activate() {
    TABLET_ACTIVE.store(true, Ordering::Relaxed);
    serial_println!("[TABLET] Absolute pointer mode activated (seamless mouse)");
}

/// Deactivate tablet mode - fall back to relative PS/2 mouse
pub fn deactivate() {
    TABLET_ACTIVE.store(false, Ordering::Relaxed);
    serial_println!("[TABLET] Absolute pointer mode deactivated");
}

// ─── QEMU USB Tablet Detection ────────────────────────────────────

/// Detect QEMU USB tablet device via PCI/USB enumeration
/// The USB tablet appears as a USB HID device with usage Digitizer
/// In QEMU, it is at USB address 1 on the virtual UHCI/OHCI/EHCI controller
fn detect_usb_tablet() -> bool {
    // Check for QEMU USB tablet via PCI USB controller
    // QEMU's USB tablet is automatically enumerated when `-usb -device usb-tablet` is used
    // We detect it by checking if the USB controller has a tablet device

    // Method 1: Check QEMU fw_cfg for command line args
    // QEMU exposes its command line via fw_cfg, which we can check for "usb-tablet"
    if detect_via_fw_cfg() {
        return true;
    }

    // Method 2: Check QEMU's i8042 (PS/2 controller) auxiliary device ID
    // When USB tablet is active, QEMU still provides PS/2 compatibility
    // but the device reports differently
    if detect_via_ps2_id() {
        return true;
    }

    false
}

/// Detect USB tablet via QEMU fw_cfg interface
fn detect_via_fw_cfg() -> bool {
    #[cfg(target_arch = "x86_64")]
    use crate::arch_compat::instructions::port::Port;
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::instructions::port::Port;

    // QEMU fw_cfg I/O ports
    const FW_CFG_PORT_SEL: u16 = 0x510;
    const FW_CFG_PORT_DATA: u16 = 0x511;

    // fw_cfg key for command line
    const FW_CFG_CMDLINE_SIZE: u16 = 0x14;
    const FW_CFG_CMDLINE_DATA: u16 = 0x15;

    unsafe {
        let mut sel_port = Port::<u16>::new(FW_CFG_PORT_SEL);
        let mut data_port = Port::<u8>::new(FW_CFG_PORT_DATA);

        // Read command line size
        sel_port.write(FW_CFG_CMDLINE_SIZE);
        let mut size_bytes = [0u8; 4];
        for byte in &mut size_bytes {
            *byte = data_port.read();
        }
        let size = u32::from_le_bytes(size_bytes) as usize;

        if size == 0 || size > 4096 {
            return false;
        }

        // Read command line data and search for "usb-tablet"
        sel_port.write(FW_CFG_CMDLINE_DATA);
        let needle = b"usb-tablet";
        let mut match_idx = 0usize;

        for _ in 0..size {
            let ch = data_port.read();
            if ch == needle[match_idx] {
                match_idx += 1;
                if match_idx == needle.len() {
                    return true;
                }
            } else if ch == needle[0] {
                match_idx = 1;
            } else {
                match_idx = 0;
            }
        }
    }

    false
}

/// Try to detect tablet by querying PS/2 mouse device ID
/// When QEMU has USB tablet, the PS/2 mouse may report as ImPS/2 (ID=3) or ImExPS/2 (ID=4)
fn detect_via_ps2_id() -> bool {
    #[cfg(target_arch = "x86_64")]
    use crate::arch_compat::instructions::port::Port;
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::instructions::port::Port;

    unsafe {
        let mut cmd_port = Port::<u8>::new(0x64);
        let mut data_port = Port::<u8>::new(0x60);

        // Send "Get Device ID" command to mouse (0xF2)
        // First, signal that next byte goes to auxiliary device
        wait_ps2_write();
        cmd_port.write(0xD4);
        wait_ps2_write();
        data_port.write(0xF2);

        // Read ACK
        if wait_ps2_read_timeout() {
            let ack = data_port.read();
            if ack != 0xFA {
                return false;
            }
        } else {
            return false;
        }

        // Read device ID
        if wait_ps2_read_timeout() {
            let id = data_port.read();
            // Standard PS/2 mouse = 0x00
            // Intellimouse = 0x03
            // Intellimouse Explorer = 0x04
            // QEMU USB tablet typically makes PS/2 report as ID 0x00
            // but the presence of a USB tablet is handled at QEMU level
            serial_println!("[TABLET] PS/2 mouse device ID: {:#x}", id);
        }
    }

    false
}

unsafe fn wait_ps2_write() {
    let mut port = crate::arch_compat::instructions::port::Port::<u8>::new(0x64);
    for _ in 0..10000 {
        if port.read() & 0x02 == 0 {
            return;
        }
    }
}

unsafe fn wait_ps2_read_timeout() -> bool {
    let mut port = crate::arch_compat::instructions::port::Port::<u8>::new(0x64);
    for _ in 0..10000 {
        if port.read() & 0x01 != 0 {
            return true;
        }
    }
    false
}

// ─── Initialize ────────────────────────────────────────────────────

/// Initialize the tablet input subsystem
/// Detects whether QEMU USB tablet is present and activates absolute mode
pub fn init() {
    serial_println!("[TABLET] Initializing USB tablet / absolute pointer support...");

    // Set initial screen dimensions from GUI module
    let (w, h) = crate::gui::cached_screen_size();
    set_screen_size(w as u32, h as u32);

    // Set initial position to center of screen
    ABS_X.store(w / 2, Ordering::Relaxed);
    ABS_Y.store(h / 2, Ordering::Relaxed);

    // Try to detect USB tablet
    if detect_usb_tablet() {
        activate();
        serial_println!("[TABLET] QEMU USB tablet detected - seamless mouse enabled");
    } else {
        // No USB tablet device found — stay in PS/2 relative mouse mode.
        // Do NOT activate tablet mode without a real device, or the cursor
        // sync assumptions will be wrong.
        serial_println!("[TABLET] No USB tablet detected — using PS/2 relative mouse");
    }

    serial_println!(
        "[TABLET] USB tablet driver initialized (screen {}x{})",
        w,
        h
    );
}
