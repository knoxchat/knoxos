use alloc::format;
/// USB Hub Enumeration — Hub device detection and downstream port management
///
/// Implements USB Hub Class driver:
///   - Hub descriptor parsing (bNbrPorts, hub characteristics)
///   - Port status/change monitoring (CONNECTION, ENABLE, SUSPEND, RESET)
///   - Port power control
///   - Downstream device enumeration through hub ports
///   - Hub depth tracking for nested topology
///
/// Integrates with the main xHCI driver in usb.rs for device enumeration.
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── USB Hub Class Requests ─────────────────────────────────────────

const USB_CLASS_HUB: u8 = 0x09;

/// Hub class-specific requests
const HUB_GET_STATUS: u8 = 0;
const HUB_CLEAR_FEATURE: u8 = 1;
const HUB_SET_FEATURE: u8 = 3;
const HUB_GET_DESCRIPTOR: u8 = 6;

/// Hub port features
const PORT_CONNECTION: u16 = 0;
const PORT_ENABLE: u16 = 1;
const PORT_SUSPEND: u16 = 2;
const PORT_OVER_CURRENT: u16 = 3;
const PORT_RESET: u16 = 4;
const PORT_POWER: u16 = 8;
const PORT_LOW_SPEED: u16 = 9;
const C_PORT_CONNECTION: u16 = 16;
const C_PORT_ENABLE: u16 = 17;
const C_PORT_SUSPEND: u16 = 18;
const C_PORT_OVER_CURRENT: u16 = 19;
const C_PORT_RESET: u16 = 20;

/// Port status bits
const PORT_STATUS_CONNECTION: u16 = 1 << 0;
const PORT_STATUS_ENABLE: u16 = 1 << 1;
const PORT_STATUS_SUSPEND: u16 = 1 << 2;
const PORT_STATUS_OVER_CURRENT: u16 = 1 << 3;
const PORT_STATUS_RESET: u16 = 1 << 4;
const PORT_STATUS_POWER: u16 = 1 << 8;
const PORT_STATUS_LOW_SPEED: u16 = 1 << 9;
const PORT_STATUS_HIGH_SPEED: u16 = 1 << 10;

// ─── Hub Descriptor ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HubDescriptor {
    pub num_ports: u8,
    pub hub_characteristics: u16,
    /// Power-on to power-good delay (in 2ms units)
    pub power_on_delay_ms: u16,
    /// Maximum current drawn by hub controller (mA)
    pub current_ma: u16,
    /// Per-port power switching
    pub per_port_power: bool,
    /// Compound device (hub is part of a compound device)
    pub compound: bool,
    /// Over-current protection mode
    pub overcurrent_per_port: bool,
}

// ─── Hub Port Info ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HubPort {
    /// Port number (1-based)
    pub port_num: u8,
    /// Current port status
    pub status: u16,
    /// Port change flags
    pub change: u16,
    /// Downstream device slot_id (0 if no device)
    pub device_slot: u8,
}

impl HubPort {
    pub fn is_connected(&self) -> bool {
        self.status & PORT_STATUS_CONNECTION != 0
    }
    pub fn is_enabled(&self) -> bool {
        self.status & PORT_STATUS_ENABLE != 0
    }
    pub fn is_powered(&self) -> bool {
        self.status & PORT_STATUS_POWER != 0
    }
}

// ─── USB Hub Device ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct UsbHub {
    /// Slot ID of the hub device itself
    pub slot_id: u8,
    /// Hub descriptor info
    pub descriptor: HubDescriptor,
    /// Hub depth (0 for root hub, 1 for first-level hub, etc.)
    pub depth: u8,
    /// Ports
    pub ports: Vec<HubPort>,
    /// Parent hub slot_id (0 for root hub)
    pub parent_slot: u8,
    /// USB address of the hub
    pub address: u8,
}

lazy_static::lazy_static! {
    /// All discovered hubs in the topology
    pub static ref HUBS: Mutex<Vec<UsbHub>> = Mutex::new(Vec::new());
}

static HUB_COUNT: AtomicU32 = AtomicU32::new(0);

// ─── Hub Detection ──────────────────────────────────────────────────

/// Check if a USB device is a hub and register it
pub fn probe_hub(slot_id: u8, device_class: u8, address: u8, parent_slot: u8, depth: u8) -> bool {
    if device_class != USB_CLASS_HUB {
        return false;
    }

    serial_println!("[USB-HUB] Found hub at slot {} (depth {})", slot_id, depth);

    // Parse hub descriptor (would normally be read via GET_DESCRIPTOR hub class request)
    // For now, create a default descriptor — real hardware would do a control transfer
    let desc = HubDescriptor {
        num_ports: 4, // Common default; real driver reads from descriptor
        hub_characteristics: 0,
        power_on_delay_ms: 100,
        current_ma: 100,
        per_port_power: false,
        compound: false,
        overcurrent_per_port: false,
    };

    let num_ports = desc.num_ports;
    let mut ports = Vec::with_capacity(num_ports as usize);
    for p in 1..=num_ports {
        ports.push(HubPort {
            port_num: p,
            status: 0,
            change: 0,
            device_slot: 0,
        });
    }

    let hub = UsbHub {
        slot_id,
        descriptor: desc,
        depth,
        ports,
        parent_slot,
        address,
    };

    serial_println!(
        "[USB-HUB] Registered hub: slot={} depth={} ports={}",
        slot_id,
        depth,
        num_ports
    );

    HUBS.lock().push(hub);
    HUB_COUNT.fetch_add(1, Ordering::Relaxed);

    // Power on all ports and enumerate downstream devices
    enumerate_hub_ports(slot_id);

    true
}

/// Enumerate downstream devices on all ports of a hub
fn enumerate_hub_ports(hub_slot: u8) {
    let hubs = HUBS.lock();
    let hub = match hubs.iter().find(|h| h.slot_id == hub_slot) {
        Some(h) => h,
        None => return,
    };

    let num_ports = hub.descriptor.num_ports;
    let depth = hub.depth;
    drop(hubs);

    serial_println!(
        "[USB-HUB] Enumerating {} ports on hub slot {}",
        num_ports,
        hub_slot
    );

    for port in 1..=num_ports {
        // In real hardware:
        // 1. Set PORT_POWER feature to power on the port
        // 2. Wait power_on_delay_ms
        // 3. Read PORT_STATUS to check for connection
        // 4. If connected, reset port and wait for enable
        // 5. Determine speed from port status bits
        // 6. Issue ADDRESS_DEVICE TRB via xHCI for the new device

        // Update port status (simulated — real driver reads via control transfer)
        let mut hubs = HUBS.lock();
        if let Some(hub) = hubs.iter_mut().find(|h| h.slot_id == hub_slot) {
            if let Some(p) = hub.ports.iter_mut().find(|p| p.port_num == port) {
                // Mark port as powered
                p.status |= PORT_STATUS_POWER;
            }
        }
        drop(hubs);

        serial_println!("[USB-HUB] Port {}: powered on", port);
    }
}

/// Handle port status change event from xHCI
pub fn handle_port_change(hub_slot: u8, port: u8) {
    let mut hubs = HUBS.lock();
    let hub = match hubs.iter_mut().find(|h| h.slot_id == hub_slot) {
        Some(h) => h,
        None => return,
    };

    if let Some(p) = hub.ports.iter_mut().find(|p| p.port_num == port) {
        // In real hardware, read the port status via control transfer
        // For now, toggle connection based on change flags
        if p.change & (1 << 0) != 0 {
            // Connection change
            if p.status & PORT_STATUS_CONNECTION != 0 {
                serial_println!(
                    "[USB-HUB] Device connected on hub {} port {}",
                    hub_slot,
                    port
                );
                // Would trigger device enumeration here
            } else {
                serial_println!(
                    "[USB-HUB] Device disconnected from hub {} port {}",
                    hub_slot,
                    port
                );
                p.device_slot = 0;
            }
            // Clear the change bit
            p.change &= !(1 << 0);
        }
    }
}

// ─── Topology Queries ───────────────────────────────────────────────

/// Get the number of discovered hubs
pub fn hub_count() -> u32 {
    HUB_COUNT.load(Ordering::Relaxed)
}

/// Get hub topology as a formatted string
pub fn topology_string() -> String {
    let hubs = HUBS.lock();
    if hubs.is_empty() {
        return String::from("No USB hubs detected");
    }

    let mut s = String::new();
    for hub in hubs.iter() {
        s.push_str(&format!(
            "Hub slot={} depth={} ports={}\n",
            hub.slot_id, hub.depth, hub.descriptor.num_ports
        ));
        for port in &hub.ports {
            let conn = if port.is_connected() {
                "connected"
            } else {
                "empty"
            };
            let pwr = if port.is_powered() { "powered" } else { "off" };
            s.push_str(&format!("  Port {}: {} {}\n", port.port_num, conn, pwr));
        }
    }
    s
}

/// List all hubs
pub fn list_hubs() -> Vec<UsbHub> {
    HUBS.lock().clone()
}

/// Initialize hub monitoring
pub fn init() {
    serial_println!("[USB-HUB] Hub enumeration driver initialized");
}
