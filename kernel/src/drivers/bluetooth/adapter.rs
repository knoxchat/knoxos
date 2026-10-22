use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

use super::addr::BdAddr;
use super::hci::{
    HCI_CREATE_CONNECTION, HCI_DISCONNECT, HCI_INQUIRY, HCI_INQUIRY_CANCEL, HCI_RESET,
    HCI_WRITE_LOCAL_NAME, HCI_WRITE_SCAN_ENABLE,
};

// ═══════════════════════════════════════════════════════════════════════
// HCI ADAPTER
// ═══════════════════════════════════════════════════════════════════════

/// Bluetooth adapter state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterState {
    Down,
    Init,
    Running,
    Suspended,
    Error,
}

/// Transport type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HciTransport {
    Usb,
    Uart,
    Sdio,
    Pci,
    Virtual,
}

/// Bluetooth adapter
#[derive(Debug, Clone)]
pub struct HciAdapter {
    pub id: u32,
    pub name: String,
    pub address: BdAddr,
    pub transport: HciTransport,
    pub state: AdapterState,
    pub hci_version: u8,
    pub hci_revision: u16,
    pub lmp_version: u8,
    pub lmp_subversion: u16,
    pub manufacturer: u16,
    pub le_supported: bool,
    pub discoverable: bool,
    pub connectable: bool,
    pub class_of_device: u32,
    pub local_name: String,
    pub acl_mtu: u16,
    pub acl_max_pkt: u16,
    pub sco_mtu: u16,
    pub sco_max_pkt: u16,
    pub le_acl_mtu: u16,
    pub le_acl_max_pkt: u16,
    pub connections: Vec<HciConnection>,
    pub pending_commands: Vec<Vec<u8>>,
}

/// HCI connection
#[derive(Debug, Clone)]
pub struct HciConnection {
    pub handle: u16,
    pub remote_addr: BdAddr,
    pub link_type: LinkType,
    pub state: ConnectionState,
    pub encryption: bool,
    pub role: ConnectionRole,
    pub rssi: i8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkType {
    Sco,
    Acl,
    Esco,
    Le,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
    ConfigPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionRole {
    Master,
    Slave,
}

/// Global adapter registry
pub(crate) static HCI_ADAPTERS: Mutex<BTreeMap<u32, HciAdapter>> = Mutex::new(BTreeMap::new());
static NEXT_ADAPTER_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_CONN_HANDLE: AtomicU16 = AtomicU16::new(1);

// ═══════════════════════════════════════════════════════════════════════
// ADAPTER MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════

/// Register a new Bluetooth adapter
pub fn register_adapter(name: &str, transport: HciTransport, address: BdAddr) -> u32 {
    let id = NEXT_ADAPTER_ID.fetch_add(1, Ordering::SeqCst);
    let mut adapters = HCI_ADAPTERS.lock();

    adapters.insert(
        id,
        HciAdapter {
            id,
            name: String::from(name),
            address,
            transport,
            state: AdapterState::Down,
            hci_version: 0x0B, // BT 5.2
            hci_revision: 0,
            lmp_version: 0x0B,
            lmp_subversion: 0,
            manufacturer: 0x000A, // Qualcomm (example)
            le_supported: true,
            discoverable: false,
            connectable: true,
            class_of_device: 0x00010C, // Computer / Laptop
            local_name: String::from("knoxos"),
            acl_mtu: 1021,
            acl_max_pkt: 8,
            sco_mtu: 64,
            sco_max_pkt: 1,
            le_acl_mtu: 251,
            le_acl_max_pkt: 8,
            connections: Vec::new(),
            pending_commands: Vec::new(),
        },
    );

    serial_println!(
        "[BT] Registered adapter hci{}: {} ({}) {}",
        id,
        name,
        address,
        match transport {
            HciTransport::Usb => "USB",
            HciTransport::Uart => "UART",
            HciTransport::Sdio => "SDIO",
            HciTransport::Pci => "PCI",
            HciTransport::Virtual => "Virtual",
        }
    );
    id
}

/// Bring adapter up
pub fn adapter_up(adapter_id: u32) -> Result<(), &'static str> {
    let mut adapters = HCI_ADAPTERS.lock();
    let adapter = adapters.get_mut(&adapter_id).ok_or("Adapter not found")?;

    // Send HCI Reset
    send_hci_command(adapter, HCI_RESET, &[]);

    adapter.state = AdapterState::Running;
    serial_println!("[BT] hci{} UP", adapter_id);
    Ok(())
}

/// Bring adapter down
pub fn adapter_down(adapter_id: u32) -> Result<(), &'static str> {
    let mut adapters = HCI_ADAPTERS.lock();
    let adapter = adapters.get_mut(&adapter_id).ok_or("Adapter not found")?;

    // Disconnect all connections
    adapter.connections.clear();
    adapter.state = AdapterState::Down;
    serial_println!("[BT] hci{} DOWN", adapter_id);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// HCI COMMAND CONSTRUCTION
// ═══════════════════════════════════════════════════════════════════════

/// Build and queue an HCI command
pub(crate) fn send_hci_command(adapter: &mut HciAdapter, opcode: u16, params: &[u8]) {
    let mut cmd = Vec::with_capacity(3 + params.len());
    cmd.push((opcode & 0xFF) as u8);
    cmd.push((opcode >> 8) as u8);
    cmd.push(params.len() as u8);
    cmd.extend_from_slice(params);
    adapter.pending_commands.push(cmd);
}

/// Start device discovery (inquiry)
pub fn start_inquiry(adapter_id: u32, duration_secs: u8) -> Result<(), &'static str> {
    let mut adapters = HCI_ADAPTERS.lock();
    let adapter = adapters.get_mut(&adapter_id).ok_or("Adapter not found")?;

    if adapter.state != AdapterState::Running {
        return Err("Adapter not running");
    }

    // LAP for General Inquiry: 0x338B9E
    let params: [u8; 5] = [0x33, 0x8B, 0x9E, duration_secs, 0]; // LAP + length + num_responses
    send_hci_command(adapter, HCI_INQUIRY, &params);

    serial_println!(
        "[BT] Started inquiry for {}s on hci{}",
        duration_secs,
        adapter_id
    );
    Ok(())
}

/// Cancel ongoing inquiry
pub fn cancel_inquiry(adapter_id: u32) -> Result<(), &'static str> {
    let mut adapters = HCI_ADAPTERS.lock();
    let adapter = adapters.get_mut(&adapter_id).ok_or("Adapter not found")?;
    send_hci_command(adapter, HCI_INQUIRY_CANCEL, &[]);
    serial_println!("[BT] Cancelled inquiry on hci{}", adapter_id);
    Ok(())
}

/// Set adapter discoverable
pub fn set_discoverable(adapter_id: u32, discoverable: bool) -> Result<(), &'static str> {
    let mut adapters = HCI_ADAPTERS.lock();
    let adapter = adapters.get_mut(&adapter_id).ok_or("Adapter not found")?;

    let scan_enable: u8 = match (discoverable, adapter.connectable) {
        (true, true) => 0x03,   // Inquiry + Page scan
        (true, false) => 0x01,  // Inquiry scan only
        (false, true) => 0x02,  // Page scan only
        (false, false) => 0x00, // No scanning
    };

    send_hci_command(adapter, HCI_WRITE_SCAN_ENABLE, &[scan_enable]);
    adapter.discoverable = discoverable;
    serial_println!("[BT] hci{} discoverable={}", adapter_id, discoverable);
    Ok(())
}

/// Set local name
pub fn set_local_name(adapter_id: u32, name: &str) -> Result<(), &'static str> {
    let mut adapters = HCI_ADAPTERS.lock();
    let adapter = adapters.get_mut(&adapter_id).ok_or("Adapter not found")?;

    let mut params = [0u8; 248]; // Name is null-terminated, max 248 bytes
    let name_bytes = name.as_bytes();
    let len = name_bytes.len().min(247);
    params[..len].copy_from_slice(&name_bytes[..len]);

    send_hci_command(adapter, HCI_WRITE_LOCAL_NAME, &params);
    adapter.local_name = String::from(name);
    serial_println!("[BT] hci{} name='{}'", adapter_id, name);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// CONNECTION MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════

/// Create ACL connection
pub fn create_connection(adapter_id: u32, remote: BdAddr) -> Result<u16, &'static str> {
    let mut adapters = HCI_ADAPTERS.lock();
    let adapter = adapters.get_mut(&adapter_id).ok_or("Adapter not found")?;

    if adapter.state != AdapterState::Running {
        return Err("Adapter not running");
    }

    let handle = NEXT_CONN_HANDLE.fetch_add(1, Ordering::SeqCst);

    adapter.connections.push(HciConnection {
        handle,
        remote_addr: remote,
        link_type: LinkType::Acl,
        state: ConnectionState::Connecting,
        encryption: false,
        role: ConnectionRole::Master,
        rssi: -60,
    });

    // Send HCI Create Connection command
    let mut params = Vec::with_capacity(13);
    params.extend_from_slice(&remote.0); // BD_ADDR
    params.extend_from_slice(&[0x18, 0xCC]); // Packet type (DM1, DH1, DM3, DH3, DM5, DH5)
    params.push(0x02); // Page scan repetition mode R2
    params.push(0x00); // Reserved
    params.extend_from_slice(&[0x00, 0x00]); // Clock offset
    params.push(0x01); // Allow role switch

    send_hci_command(adapter, HCI_CREATE_CONNECTION, &params);
    serial_println!("[BT] Creating connection to {} (handle={})", remote, handle);
    Ok(handle)
}

/// Disconnect a connection
pub fn disconnect(adapter_id: u32, handle: u16, reason: u8) -> Result<(), &'static str> {
    let mut adapters = HCI_ADAPTERS.lock();
    let adapter = adapters.get_mut(&adapter_id).ok_or("Adapter not found")?;

    let mut params = [0u8; 3];
    params[0] = (handle & 0xFF) as u8;
    params[1] = ((handle >> 8) & 0x0F) as u8;
    params[2] = reason;

    send_hci_command(adapter, HCI_DISCONNECT, &params);

    // Mark connection as disconnecting
    if let Some(conn) = adapter.connections.iter_mut().find(|c| c.handle == handle) {
        conn.state = ConnectionState::Disconnecting;
    }

    serial_println!(
        "[BT] Disconnecting handle={} reason=0x{:02X}",
        handle,
        reason
    );
    Ok(())
}
