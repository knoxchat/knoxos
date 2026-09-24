//! USB Power Delivery profile negotiation placeholders.
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// USB Power Delivery Negotiation
// ═══════════════════════════════════════════════════════════════════════

/// USB PD power profile
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdProfile {
    Usb2_5W, // 5V @ 0.5A
    Usb7_5W, // 5V @ 1.5A
    Usb15W,  // 5V @ 3A
    Pd27W,   // 9V @ 3A
    Pd45W,   // 15V @ 3A
    Pd60W,   // 20V @ 3A
    Pd100W,  // 20V @ 5A
    Pd240W,  // 48V @ 5A (EPR)
}

/// USB PD port state
#[derive(Debug, Clone)]
pub struct PdPort {
    pub port_id: u8,
    pub negotiated_profile: PdProfile,
    pub voltage_mv: u32,
    pub current_ma: u32,
    pub source: bool, // true = source, false = sink
}

lazy_static::lazy_static! {
    static ref PD_PORTS: Mutex<Vec<PdPort>> = Mutex::new(Vec::new());
}

/// Negotiate USB PD profile on a port
pub fn pd_negotiate(port_id: u8, profile: PdProfile) -> bool {
    let mut ports = PD_PORTS.lock();
    let (voltage_mv, current_ma) = match profile {
        PdProfile::Usb2_5W => (5000, 500),
        PdProfile::Usb7_5W => (5000, 1500),
        PdProfile::Usb15W => (5000, 3000),
        PdProfile::Pd27W => (9000, 3000),
        PdProfile::Pd45W => (15000, 3000),
        PdProfile::Pd60W => (20000, 3000),
        PdProfile::Pd100W => (20000, 5000),
        PdProfile::Pd240W => (48000, 5000),
    };
    if let Some(port) = ports.iter_mut().find(|p| p.port_id == port_id) {
        port.negotiated_profile = profile;
        port.voltage_mv = voltage_mv;
        port.current_ma = current_ma;
        serial_println!(
            "[USB-PD] Port {} negotiated {}mV / {}mA",
            port_id,
            voltage_mv,
            current_ma
        );
        true
    } else {
        ports.push(PdPort {
            port_id,
            negotiated_profile: profile,
            voltage_mv,
            current_ma,
            source: false,
        });
        true
    }
}

/// Get USB PD port status
pub fn pd_get_port(port_id: u8) -> Option<PdPort> {
    PD_PORTS
        .lock()
        .iter()
        .find(|p| p.port_id == port_id)
        .cloned()
}
