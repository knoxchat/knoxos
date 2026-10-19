use spin::Mutex;

use crate::serial_println;

/// Broadcom combo chip type
#[derive(Debug, Clone, Copy)]
pub enum BrcmChip {
    Bcm4356,  // 802.11ac + BT 4.1
    Bcm4371,  // 802.11ac + BT 4.2
    Bcm43602, // 802.11ac + BT 4.1 (Apple)
    Bcm4378,  // Wi-Fi 6 + BT 5.0
}

/// Broadcom device state
pub struct BrcmDevice {
    pub chip: BrcmChip,
    pub wifi_active: bool,
    pub bt_active: bool,
    pub firmware_loaded: bool,
}

lazy_static::lazy_static! {
    static ref BRCM_DEVICE: Mutex<Option<BrcmDevice>> = Mutex::new(None);
}

/// Probe Broadcom WiFi/BT combo (vendor 0x14E4)
pub fn brcm_probe(chip: BrcmChip) -> bool {
    *BRCM_DEVICE.lock() = Some(BrcmDevice {
        chip,
        wifi_active: false,
        bt_active: false,
        firmware_loaded: false,
    });
    serial_println!("[WiFi] Broadcom {:?} combo chip probed", chip);
    true
}

/// Activate Broadcom WiFi
pub fn brcm_wifi_enable() -> bool {
    let mut dev = BRCM_DEVICE.lock();
    if let Some(ref mut d) = *dev {
        d.wifi_active = true;
        return true;
    }
    false
}

/// Activate Broadcom Bluetooth
pub fn brcm_bt_enable() -> bool {
    let mut dev = BRCM_DEVICE.lock();
    if let Some(ref mut d) = *dev {
        d.bt_active = true;
        return true;
    }
    false
}
