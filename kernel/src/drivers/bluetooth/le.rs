use alloc::vec;
use alloc::vec::Vec;

use crate::serial_println;

use super::adapter::{HCI_ADAPTERS, send_hci_command};
use super::addr::BdAddr;
use super::hci::{
    HCI_LE_SET_ADV_DATA, HCI_LE_SET_ADV_ENABLE, HCI_LE_SET_ADV_PARAMS, HCI_LE_SET_SCAN_ENABLE,
    HCI_LE_SET_SCAN_PARAMS,
};

// ═══════════════════════════════════════════════════════════════════════
// BLE (Bluetooth Low Energy)
// ═══════════════════════════════════════════════════════════════════════

/// BLE advertising type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AdvType {
    AdvInd = 0x00,           // Connectable undirected
    AdvDirectIndHigh = 0x01, // Connectable directed (high duty)
    AdvScanInd = 0x02,       // Scannable undirected
    AdvNonconnInd = 0x03,    // Non-connectable undirected
    AdvDirectIndLow = 0x04,  // Connectable directed (low duty)
}

/// BLE advertising data types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AdvDataType {
    Flags = 0x01,
    IncompleteListUuid16 = 0x02,
    CompleteListUuid16 = 0x03,
    IncompleteListUuid128 = 0x06,
    CompleteListUuid128 = 0x07,
    ShortenedLocalName = 0x08,
    CompleteLocalName = 0x09,
    TxPowerLevel = 0x0A,
    ManufacturerSpecific = 0xFF,
}

/// BLE advertising parameters
#[derive(Debug, Clone)]
pub struct AdvParams {
    pub adv_interval_min: u16, // In 0.625ms units
    pub adv_interval_max: u16,
    pub adv_type: AdvType,
    pub own_addr_type: u8,
    pub peer_addr_type: u8,
    pub peer_addr: BdAddr,
    pub channel_map: u8, // Bit mask: ch37, ch38, ch39
    pub filter_policy: u8,
}

impl AdvParams {
    pub fn default() -> Self {
        Self {
            adv_interval_min: 0x0800, // 1.28s
            adv_interval_max: 0x0800,
            adv_type: AdvType::AdvInd,
            own_addr_type: 0,
            peer_addr_type: 0,
            peer_addr: BdAddr::ZERO,
            channel_map: 0x07, // All 3 advertising channels
            filter_policy: 0,
        }
    }
}

/// Set BLE advertising parameters
pub fn le_set_adv_params(adapter_id: u32, params: &AdvParams) -> Result<(), &'static str> {
    let mut adapters = HCI_ADAPTERS.lock();
    let adapter = adapters.get_mut(&adapter_id).ok_or("Adapter not found")?;

    let mut cmd_params = Vec::with_capacity(15);
    cmd_params.extend_from_slice(&params.adv_interval_min.to_le_bytes());
    cmd_params.extend_from_slice(&params.adv_interval_max.to_le_bytes());
    cmd_params.push(params.adv_type as u8);
    cmd_params.push(params.own_addr_type);
    cmd_params.push(params.peer_addr_type);
    cmd_params.extend_from_slice(&params.peer_addr.0);
    cmd_params.push(params.channel_map);
    cmd_params.push(params.filter_policy);

    send_hci_command(adapter, HCI_LE_SET_ADV_PARAMS, &cmd_params);
    serial_println!("[BLE] Set advertising parameters on hci{}", adapter_id);
    Ok(())
}

/// Set BLE advertising data
pub fn le_set_adv_data(adapter_id: u32, data: &[u8]) -> Result<(), &'static str> {
    let mut adapters = HCI_ADAPTERS.lock();
    let adapter = adapters.get_mut(&adapter_id).ok_or("Adapter not found")?;

    let len = data.len().min(31);
    let mut params = vec![0u8; 32];
    params[0] = len as u8;
    params[1..1 + len].copy_from_slice(&data[..len]);

    send_hci_command(adapter, HCI_LE_SET_ADV_DATA, &params);
    serial_println!(
        "[BLE] Set advertising data ({} bytes) on hci{}",
        len,
        adapter_id
    );
    Ok(())
}

/// Enable/disable BLE advertising
pub fn le_set_adv_enable(adapter_id: u32, enable: bool) -> Result<(), &'static str> {
    let mut adapters = HCI_ADAPTERS.lock();
    let adapter = adapters.get_mut(&adapter_id).ok_or("Adapter not found")?;
    send_hci_command(adapter, HCI_LE_SET_ADV_ENABLE, &[enable as u8]);
    serial_println!(
        "[BLE] Advertising {} on hci{}",
        if enable { "enabled" } else { "disabled" },
        adapter_id
    );
    Ok(())
}

/// BLE scan parameters
#[derive(Debug, Clone)]
pub struct LeScanParams {
    pub scan_type: u8,      // 0=passive, 1=active
    pub scan_interval: u16, // 0.625ms units
    pub scan_window: u16,   // 0.625ms units
    pub own_addr_type: u8,
    pub filter_policy: u8,
}

/// Set BLE scan parameters
pub fn le_set_scan_params(adapter_id: u32, params: &LeScanParams) -> Result<(), &'static str> {
    let mut adapters = HCI_ADAPTERS.lock();
    let adapter = adapters.get_mut(&adapter_id).ok_or("Adapter not found")?;

    let mut cmd_params = Vec::with_capacity(7);
    cmd_params.push(params.scan_type);
    cmd_params.extend_from_slice(&params.scan_interval.to_le_bytes());
    cmd_params.extend_from_slice(&params.scan_window.to_le_bytes());
    cmd_params.push(params.own_addr_type);
    cmd_params.push(params.filter_policy);

    send_hci_command(adapter, HCI_LE_SET_SCAN_PARAMS, &cmd_params);
    Ok(())
}

/// Enable/disable BLE scanning
pub fn le_set_scan_enable(
    adapter_id: u32,
    enable: bool,
    filter_dups: bool,
) -> Result<(), &'static str> {
    let mut adapters = HCI_ADAPTERS.lock();
    let adapter = adapters.get_mut(&adapter_id).ok_or("Adapter not found")?;
    send_hci_command(
        adapter,
        HCI_LE_SET_SCAN_ENABLE,
        &[enable as u8, filter_dups as u8],
    );
    serial_println!(
        "[BLE] Scanning {} on hci{}",
        if enable { "enabled" } else { "disabled" },
        adapter_id
    );
    Ok(())
}
