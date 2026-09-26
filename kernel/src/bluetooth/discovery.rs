use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use super::addr::BdAddr;

// ═══════════════════════════════════════════════════════════════════════
// DEVICE DISCOVERY RESULTS
// ═══════════════════════════════════════════════════════════════════════

/// Discovered device
#[derive(Debug, Clone)]
pub struct DiscoveredDevice {
    pub address: BdAddr,
    pub name: String,
    pub class_of_device: u32,
    pub rssi: i8,
    pub is_le: bool,
    pub adv_data: Vec<u8>,
    pub paired: bool,
    pub connected: bool,
}

/// Discovery results
static DISCOVERED_DEVICES: Mutex<Vec<DiscoveredDevice>> = Mutex::new(Vec::new());

/// Add a discovered device
pub fn add_discovered_device(addr: BdAddr, name: &str, class: u32, rssi: i8, is_le: bool) {
    let mut devices = DISCOVERED_DEVICES.lock();

    // Update existing or add new
    if let Some(dev) = devices.iter_mut().find(|d| d.address == addr) {
        if !name.is_empty() {
            dev.name = String::from(name);
        }
        dev.rssi = rssi;
    } else {
        devices.push(DiscoveredDevice {
            address: addr,
            name: String::from(name),
            class_of_device: class,
            rssi,
            is_le,
            adv_data: Vec::new(),
            paired: false,
            connected: false,
        });
    }
}

/// Get discovered devices
pub fn get_discovered_devices() -> Vec<DiscoveredDevice> {
    DISCOVERED_DEVICES.lock().clone()
}

/// Clear discovered devices
pub fn clear_discovered_devices() {
    DISCOVERED_DEVICES.lock().clear();
}
