/// Bluetooth HCI (Host Controller Interface) Driver
/// Implements the Bluetooth Host Controller Interface for wireless connectivity
///
/// Features:
/// - HCI command/event transport layer
/// - ACL/SCO data packet handling
/// - Bluetooth device discovery (inquiry)
/// - Connection management (ACL, SCO, LE)
/// - L2CAP basic support
/// - Bluetooth Low Energy (BLE) advertising and scanning
/// - Device pairing state machine (SSP, Legacy)
/// - Bluetooth address management
/// - HCI adapter registration
/// - /sys/class/bluetooth integration
///
/// Split into submodules for maintainability:
///   addr       — BD_ADDR type
///   hci        — Packet types, opcodes, event codes, headers
///   transport  — UART/USB HCI transports and registration
///   adapter    — Adapter registry, inquiry, connections
///   le         — BLE advertising and scanning
///   l2cap      — L2CAP channels and PSMs
///   discovery  — Discovered-device cache
///   events     — HCI event processing
///   a2dp       — A2DP profile and HDA audio routing
use alloc::string::String;

use crate::serial_println;

mod a2dp;
mod adapter;
mod addr;
mod discovery;
mod events;
mod hci;
mod l2cap;
mod le;
mod transport;

pub use a2dp::*;
pub use adapter::*;
pub use addr::*;
pub use discovery::*;
pub use events::*;
pub use hci::*;
pub use l2cap::*;
pub use le::*;
pub use transport::*;

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Initialize Bluetooth subsystem
pub fn init() {
    serial_println!("[BT] Initializing Bluetooth HCI subsystem");

    // Register a virtual Bluetooth adapter for testing
    let addr = BdAddr::new([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    let id = register_adapter("Virtual BT 5.2", HciTransport::Virtual, addr);

    // Bring it up
    let _ = adapter_up(id);
    let _ = set_local_name(id, "knoxos");

    // Initialize A2DP audio profile
    a2dp_init();

    serial_println!("[BT] Bluetooth subsystem initialized (HCI + L2CAP + BLE + A2DP)");
    let _ = hci_self_test();
}

pub const GATE_BT_MARKER: &str = "GATE_BT hci";

/// Virtual adapter queues HCI Reset; a Command Complete event is applied.
pub fn hci_self_test() -> bool {
    use core::sync::atomic::Ordering;

    let mut adapters = HCI_ADAPTERS.lock();
    let Some((id, adapter)) = adapters.iter_mut().next() else {
        serial_println!("[BT] GATE_BT FAILED: no adapter");
        return false;
    };
    let adapter_id = *id;
    let queued_reset = adapter
        .pending_commands
        .iter()
        .any(|cmd| cmd.len() >= 2 && u16::from_le_bytes([cmd[0], cmd[1]]) == HCI_RESET);
    if !queued_reset && events::LAST_COMPLETE_OPCODE.load(Ordering::Relaxed) != HCI_RESET {
        send_hci_command(adapter, HCI_RESET, &[]);
    }
    drop(adapters);

    process_event(adapter_id, &[0x0E, 0x04, 1, 0x03, 0x0C, 0x00]);
    let opcode = events::LAST_COMPLETE_OPCODE.load(Ordering::Relaxed);
    let status = events::LAST_COMPLETE_STATUS.load(Ordering::Relaxed);
    if opcode != HCI_RESET || status != 0 {
        serial_println!(
            "[BT] GATE_BT FAILED: opcode={:#06x} status={:#04x}",
            opcode,
            status
        );
        return false;
    }
    serial_println!("[BT] {} (HCI Reset Command Complete)", GATE_BT_MARKER);
    true
}

/// Get adapter info for /sys/class/bluetooth
pub fn sys_bluetooth_info() -> String {
    let adapters = HCI_ADAPTERS.lock();
    let mut out = String::new();
    for (_, adapter) in adapters.iter() {
        out.push_str(&alloc::format!("hci{}:\n", adapter.id));
        out.push_str(&alloc::format!("  Type: Primary\n"));
        out.push_str(&alloc::format!("  Address: {}\n", adapter.address));
        out.push_str(&alloc::format!("  Name: {}\n", adapter.local_name));
        out.push_str(&alloc::format!("  State: {:?}\n", adapter.state));
        out.push_str(&alloc::format!("  LE: {}\n", adapter.le_supported));
        out.push_str(&alloc::format!(
            "  Discoverable: {}\n",
            adapter.discoverable
        ));
        out.push_str(&alloc::format!(
            "  Connections: {}\n",
            adapter.connections.len()
        ));
    }
    out
}
