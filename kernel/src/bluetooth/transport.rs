use alloc::vec::Vec;

use crate::serial_println;

use super::adapter::{HCI_ADAPTERS, HciTransport, register_adapter};
use super::addr::BdAddr;

// ═══════════════════════════════════════════════════════════════════════
// HCI TRANSPORT LAYER
// ═══════════════════════════════════════════════════════════════════════

/// UART HCI transport (H4 protocol)
pub struct UartHciTransport {
    pub port: u16, // UART I/O port
    pub baudrate: u32,
}

impl UartHciTransport {
    pub fn send(&self, _packet_type: u8, _data: &[u8]) -> Result<(), &'static str> {
        // Write packet to UART with leading packet type byte
        // packet_type: 0x01 = Command, 0x02 = ACL Data, 0x03 = SCO Data
        // Format: [packet_type | data...]
        serial_println!(
            "[BT-UART] Sending HCI packet via UART port {:#x}",
            self.port
        );
        Ok(())
    }

    pub fn recv(&self) -> Result<Option<Vec<u8>>, &'static str> {
        // Read packet from UART, parse packet type and return payload
        Ok(None) // No data available
    }

    pub fn is_connected(&self) -> bool {
        true // UART is persistent
    }

    pub fn configure(&self, param: &str, value: &[u8]) -> Result<(), &'static str> {
        match param {
            "baudrate" => {
                if value.len() >= 4 {
                    let baud = u32::from_le_bytes([value[0], value[1], value[2], value[3]]);
                    serial_println!("[BT-UART] Configured baudrate to {} bps", baud);
                    Ok(())
                } else {
                    Err("Invalid baudrate value")
                }
            }
            _ => Err("Unknown parameter"),
        }
    }
}

/// USB HCI transport (bulk endpoints)
pub struct UsbHciTransport {
    pub device_id: u32,
    pub ep_cmd_out: u8,  // Command OUT endpoint
    pub ep_event_in: u8, // Event IN endpoint
    pub ep_acl_out: u8,  // ACL OUT endpoint
    pub ep_acl_in: u8,   // ACL IN endpoint
}

impl UsbHciTransport {
    pub fn send(&self, packet_type: u8, data: &[u8]) -> Result<(), &'static str> {
        match packet_type {
            0x01 => {
                // HCI Command → send via bt_hci_usb control transfer
                if data.len() >= 3 {
                    let opcode = u16::from_le_bytes([data[0], data[1]]);
                    let params = if data.len() > 3 { &data[3..] } else { &[] };
                    crate::bt_hci_usb::send_hci_command(opcode, params)?;
                }
            }
            0x02 => {
                // ACL Data → send via bt_hci_usb bulk OUT
                if data.len() >= 4 {
                    let handle = u16::from_le_bytes([data[0], data[1]]) & 0x0FFF;
                    let pb_flag = (data[0] >> 4) & 0x03;
                    let bc_flag = (data[1] >> 6) & 0x03;
                    let payload = &data[4..];
                    crate::bt_hci_usb::send_acl_data(handle, pb_flag, bc_flag, payload)?;
                }
            }
            0x03 => {
                // SCO Data
                serial_println!(
                    "[BT-USB] Sending SCO data to device {} (EP {})",
                    self.device_id,
                    self.ep_acl_out
                );
            }
            _ => return Err("Invalid packet type"),
        }

        Ok(())
    }

    pub fn recv(&self) -> Result<Option<Vec<u8>>, &'static str> {
        // Poll HCI events from bt_hci_usb
        let events = crate::bt_hci_usb::recv_hci_events();
        if let Some(first) = events.into_iter().next() {
            Ok(Some(first))
        } else {
            // Try ACL data
            let acl = crate::bt_hci_usb::recv_acl_data();
            if let Some(first) = acl.into_iter().next() {
                Ok(Some(first))
            } else {
                Ok(None)
            }
        }
    }

    pub fn is_connected(&self) -> bool {
        crate::bt_hci_usb::is_initialized()
    }

    pub fn configure(&self, param: &str, _value: &[u8]) -> Result<(), &'static str> {
        match param {
            "reset" => {
                serial_println!(
                    "[BT-USB] Resetting Bluetooth controller on device {}",
                    self.device_id
                );
                // Send HCI_Reset command (OGF=0x03, OCF=0x0003 → opcode 0x0C03)
                crate::bt_hci_usb::send_hci_command(0x0C03, &[])?;
                Ok(())
            }
            _ => Err("Unknown parameter"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// TRANSPORT MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════

/// Register a UART-based Bluetooth adapter
pub fn register_uart_adapter(name: &str, port: u16, baudrate: u32, addr: BdAddr) -> u32 {
    serial_println!(
        "[BT] Registering UART Bluetooth adapter: {} on port {:#x} @ {} bps",
        name,
        port,
        baudrate
    );
    let id = register_adapter(name, HciTransport::Uart, addr);
    serial_println!("[BT] Adapter registered as hci{}", id);
    id
}

/// Register a USB-based Bluetooth adapter
pub fn register_usb_adapter(
    name: &str,
    device_id: u32,
    ep_cmd_out: u8,
    ep_event_in: u8,
    ep_acl_out: u8,
    ep_acl_in: u8,
    addr: BdAddr,
) -> u32 {
    serial_println!(
        "[BT] Registering USB Bluetooth adapter: {} (device {})",
        name,
        device_id
    );
    let id = register_adapter(name, HciTransport::Usb, addr);
    serial_println!(
        "[BT] Adapter registered as hci{} with USB endpoints: cmd={}, event={}, acl_out={}, acl_in={}",
        id,
        ep_cmd_out,
        ep_event_in,
        ep_acl_out,
        ep_acl_in
    );
    id
}

/// Send HCI command via transport
pub fn send_hci_packet(adapter_id: u32, packet_type: u8, data: &[u8]) -> Result<(), &'static str> {
    let adapters = HCI_ADAPTERS.lock();
    let adapter = adapters.get(&adapter_id).ok_or("Adapter not found")?;

    match adapter.transport {
        HciTransport::Uart => {
            serial_println!(
                "[BT-UART] Sending HCI packet type 0x{:02X} ({} bytes)",
                packet_type,
                data.len()
            );
        }
        HciTransport::Usb => {
            serial_println!(
                "[BT-USB] Sending HCI packet type 0x{:02X} ({} bytes)",
                packet_type,
                data.len()
            );
        }
        _ => {}
    }

    Ok(())
}

/// Poll for HCI events from transport
pub fn poll_hci_events(adapter_id: u32) -> Result<(), &'static str> {
    let adapters = HCI_ADAPTERS.lock();
    let adapter = adapters.get(&adapter_id).ok_or("Adapter not found")?;

    // In real implementation: read from transport queue
    match adapter.transport {
        HciTransport::Uart => {
            // Poll UART for incoming data
        }
        HciTransport::Usb => {
            // Poll USB bulk endpoint for events
        }
        _ => {}
    }

    Ok(())
}

/// Configure transport parameters
pub fn configure_transport(adapter_id: u32, param: &str, value: &[u8]) -> Result<(), &'static str> {
    let adapters = HCI_ADAPTERS.lock();
    let adapter = adapters.get(&adapter_id).ok_or("Adapter not found")?;

    match adapter.transport {
        HciTransport::Uart => {
            serial_println!(
                "[BT] UART adapter {}: configure {} = {:?}",
                adapter_id,
                param,
                value
            );
        }
        HciTransport::Usb => {
            serial_println!(
                "[BT] USB adapter {}: configure {} = {:?}",
                adapter_id,
                param,
                value
            );
        }
        _ => {}
    }

    Ok(())
}
