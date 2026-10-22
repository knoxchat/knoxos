use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// BSS entry (scan result)
#[derive(Debug, Clone)]
pub struct BssEntry {
    pub bssid: [u8; 6],
    pub ssid: String,
    pub channel: u8,
    pub frequency: u32, // MHz
    pub rssi: i32,      // dBm
    pub security: BssSecurity,
    pub beacon_interval: u16,
    pub capability: u16,
    pub supported_rates: Vec<u8>,
    pub ht_capable: bool,
    pub vht_capable: bool,
    pub last_seen_tick: u64,
}

/// BSS security mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BssSecurity {
    Open,
    Wep,
    WpaPsk,
    Wpa2Psk,
    Wpa3Sae,
    Wpa2Enterprise,
}

impl core::fmt::Display for BssSecurity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BssSecurity::Open => write!(f, "Open"),
            BssSecurity::Wep => write!(f, "WEP"),
            BssSecurity::WpaPsk => write!(f, "WPA-PSK"),
            BssSecurity::Wpa2Psk => write!(f, "WPA2-PSK"),
            BssSecurity::Wpa3Sae => write!(f, "WPA3-SAE"),
            BssSecurity::Wpa2Enterprise => write!(f, "WPA2-EAP"),
        }
    }
}

/// Global BSS list (scan results)
pub(super) static BSS_LIST: Mutex<Vec<BssEntry>> = Mutex::new(Vec::new());
