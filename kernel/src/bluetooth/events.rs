use crate::serial_println;

use super::adapter::{ConnectionState, HCI_ADAPTERS};

// ═══════════════════════════════════════════════════════════════════════
// HCI EVENT PROCESSING
// ═══════════════════════════════════════════════════════════════════════

/// Process an HCI event
pub fn process_event(adapter_id: u32, event_data: &[u8]) {
    if event_data.len() < 2 {
        return;
    }

    let event_code = event_data[0];
    let _param_len = event_data[1];

    match event_code {
        0x0E => {
            // Command Complete
            if event_data.len() >= 6 {
                let opcode = u16::from_le_bytes([event_data[3], event_data[4]]);
                let status = event_data[5];
                serial_println!(
                    "[BT] Command Complete: opcode=0x{:04X} status=0x{:02X}",
                    opcode,
                    status
                );
            }
        }
        0x0F => {
            // Command Status
            if event_data.len() >= 6 {
                let status = event_data[2];
                let opcode = u16::from_le_bytes([event_data[4], event_data[5]]);
                serial_println!(
                    "[BT] Command Status: opcode=0x{:04X} status=0x{:02X}",
                    opcode,
                    status
                );
            }
        }
        0x02 => {
            // Inquiry Result
            serial_println!("[BT] Inquiry Result received");
        }
        0x03 => {
            // Connection Complete
            if event_data.len() >= 13 {
                let status = event_data[2];
                let handle = u16::from_le_bytes([event_data[3], event_data[4]]);
                if status == 0 {
                    serial_println!("[BT] Connection established handle={}", handle);
                    let mut adapters = HCI_ADAPTERS.lock();
                    if let Some(adapter) = adapters.get_mut(&adapter_id) {
                        if let Some(conn) =
                            adapter.connections.iter_mut().find(|c| c.handle == handle)
                        {
                            conn.state = ConnectionState::Connected;
                        }
                    }
                }
            }
        }
        0x05 => {
            // Disconnection Complete
            if event_data.len() >= 6 {
                let handle = u16::from_le_bytes([event_data[3], event_data[4]]);
                let reason = event_data[5];
                serial_println!(
                    "[BT] Disconnected handle={} reason=0x{:02X}",
                    handle,
                    reason
                );
                let mut adapters = HCI_ADAPTERS.lock();
                if let Some(adapter) = adapters.get_mut(&adapter_id) {
                    adapter.connections.retain(|c| c.handle != handle);
                }
            }
        }
        _ => {
            serial_println!("[BT] Unknown event 0x{:02X}", event_code);
        }
    }
}
