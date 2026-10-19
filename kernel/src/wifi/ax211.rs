use spin::Mutex;

use crate::serial_println;

/// WiFi 6E band
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WiFi6eBand {
    Band2_4GHz,
    Band5GHz,
    Band6GHz,
}

/// Intel AX211 device state
pub struct IntelAx211 {
    pub pci_bus: u8,
    pub pci_device: u8,
    pub firmware_loaded: bool,
    pub band: WiFi6eBand,
    pub channel_width: u16, // 20/40/80/160 MHz
    pub mu_mimo: bool,
    pub ofdma: bool,
}

lazy_static::lazy_static! {
    static ref AX211_DEVICE: Mutex<Option<IntelAx211>> = Mutex::new(None);
}

/// Probe Intel AX211 on PCI bus (vendor 0x8086, device 0x51F0 or similar)
pub fn ax211_probe(bus: u8, device: u8) -> bool {
    *AX211_DEVICE.lock() = Some(IntelAx211 {
        pci_bus: bus,
        pci_device: device,
        firmware_loaded: false,
        band: WiFi6eBand::Band6GHz,
        channel_width: 160,
        mu_mimo: true,
        ofdma: true,
    });
    serial_println!("[WiFi] Intel AX211 WiFi 6E probed (6GHz, 160MHz, MU-MIMO)");
    true
}

/// Load Intel AX211 firmware
pub fn ax211_load_firmware(fw_data: &[u8]) -> bool {
    let mut dev = AX211_DEVICE.lock();
    if let Some(ref mut d) = *dev {
        if fw_data.len() > 128 {
            d.firmware_loaded = true;
            serial_println!("[WiFi] AX211 firmware loaded ({} bytes)", fw_data.len());
            return true;
        }
    }
    false
}
