//! XHCI root-hub port status, reset, and enumeration.
use alloc::vec::Vec;

use crate::serial_println;

use super::types::UsbSpeed;
use super::xhci::{XHCI, xhci_read32, xhci_write32};

// ═══════════════════════════════════════════════════════════════════════
// PORT MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════

/// USB port status
#[derive(Debug, Clone)]
pub struct UsbPort {
    pub port_num: u8,
    pub connected: bool,
    pub enabled: bool,
    pub speed: UsbSpeed,
    pub power: bool,
    pub reset: bool,
    pub slot_id: Option<u8>,
}

/// Port status register offsets (relative to port register set base)
const PORTSC_OFFSET: u32 = 0x00;
const PORTPMSC_OFFSET: u32 = 0x04;
const PORTLI_OFFSET: u32 = 0x08;

/// PORTSC bits
const PORTSC_CCS: u32 = 1 << 0; // Current Connect Status
const PORTSC_PED: u32 = 1 << 1; // Port Enabled/Disabled
const PORTSC_PR: u32 = 1 << 4; // Port Reset
const PORTSC_PLS_MASK: u32 = 0xF << 5; // Port Link State
const PORTSC_PP: u32 = 1 << 9; // Port Power
const PORTSC_SPEED_MASK: u32 = 0xF << 10; // Port Speed
const PORTSC_CSC: u32 = 1 << 17; // Connect Status Change
const PORTSC_PEC: u32 = 1 << 18; // Port Enabled/Disabled Change
const PORTSC_PRC: u32 = 1 << 21; // Port Reset Change

/// Decode port speed from PORTSC register
fn decode_port_speed(portsc: u32) -> UsbSpeed {
    match (portsc & PORTSC_SPEED_MASK) >> 10 {
        1 => UsbSpeed::Full,
        2 => UsbSpeed::Low,
        3 => UsbSpeed::High,
        4 => UsbSpeed::Super,
        5 => UsbSpeed::SuperPlus,
        _ => UsbSpeed::Full,
    }
}

/// Enumerate all ports and detect connected devices via MMIO PORTSC
pub fn enumerate_ports() -> Vec<UsbPort> {
    let xhci = XHCI.lock();
    if !xhci.initialized || xhci.mmio_base == 0 {
        return Vec::new();
    }

    let max_ports = xhci.max_ports;
    let op_base = xhci.op_base;
    drop(xhci);

    let mut ports = Vec::new();

    for port_idx in 0..max_ports {
        // Port register set starts at operational base + 0x400 + (port * 0x10)
        let port_base = op_base + 0x400 + (port_idx as u64 * 0x10);

        let portsc = unsafe { xhci_read32(port_base, PORTSC_OFFSET) };

        let connected = portsc & PORTSC_CCS != 0;
        let enabled = portsc & PORTSC_PED != 0;
        let power = portsc & PORTSC_PP != 0;
        let resetting = portsc & PORTSC_PR != 0;
        let speed = decode_port_speed(portsc);

        ports.push(UsbPort {
            port_num: port_idx + 1,
            connected,
            enabled,
            speed,
            power,
            reset: resetting,
            slot_id: None,
        });

        if connected {
            serial_println!(
                "[XHCI] Port {}: Connected, {:?} speed, enabled={}, power={}",
                port_idx + 1,
                speed,
                enabled,
                power
            );

            // Clear any pending change bits (write-1-to-clear)
            let clear_bits = portsc & (PORTSC_CSC | PORTSC_PEC | PORTSC_PRC);
            if clear_bits != 0 {
                unsafe {
                    // Preserve RO/RW bits, write 1 to clear change bits
                    // Must NOT write 1 to PED (bit 1) as that disables the port
                    let preserve = portsc & !(PORTSC_PED);
                    xhci_write32(port_base, PORTSC_OFFSET, preserve | clear_bits);
                }
            }
        }
    }

    ports
}

/// Reset a USB port to enable a connected device
pub fn reset_port(port_num: u8) -> bool {
    let xhci = XHCI.lock();
    if !xhci.initialized {
        return false;
    }
    let op_base = xhci.op_base;
    drop(xhci);

    let port_base = op_base + 0x400 + ((port_num as u64 - 1) * 0x10);

    serial_println!("[XHCI] Resetting port {}...", port_num);

    unsafe {
        let portsc = xhci_read32(port_base, PORTSC_OFFSET);
        if portsc & PORTSC_CCS == 0 {
            serial_println!("[XHCI] Port {}: No device connected", port_num);
            return false;
        }

        // Issue port reset: set PR bit, preserve PP, don't accidentally clear PED
        let new_portsc =
            (portsc & !(PORTSC_PED | PORTSC_CSC | PORTSC_PEC | PORTSC_PRC)) | PORTSC_PR;
        xhci_write32(port_base, PORTSC_OFFSET, new_portsc);

        // Wait for Port Reset Change (PRC) bit — indicates reset complete
        for _ in 0..10000 {
            let sts = xhci_read32(port_base, PORTSC_OFFSET);
            if sts & PORTSC_PRC != 0 {
                // Clear PRC (write-1-to-clear), preserve other bits
                let clear = (sts & !(PORTSC_PED)) | PORTSC_PRC;
                xhci_write32(port_base, PORTSC_OFFSET, clear);

                let final_sts = xhci_read32(port_base, PORTSC_OFFSET);
                let enabled = final_sts & PORTSC_PED != 0;
                let speed = decode_port_speed(final_sts);
                serial_println!(
                    "[XHCI] Port {} reset complete: enabled={}, speed={:?}",
                    port_num,
                    enabled,
                    speed
                );
                return enabled;
            }
            for _ in 0..1000 {
                core::hint::spin_loop();
            }
        }
    }

    serial_println!("[XHCI] Port {} reset timed out", port_num);
    false
}
