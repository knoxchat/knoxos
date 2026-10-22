/// Wi-Fi 802.11 Driver Framework
/// Implements IEEE 802.11 wireless networking infrastructure
///
/// Features:
/// - 802.11 frame parsing (management, control, data)
/// - BSS/SSID scanning and selection
/// - Association/authentication state machine
/// - WPA2/WPA3 4-way handshake (key negotiation)
/// - Wireless extensions (iwconfig/iw compatible)
/// - nl80211/cfg80211 compatible abstractions
/// - Station mode (STA) and AP mode
/// - Rate control and power management
/// - Regulatory domain support
/// - Wireless network interface management
///
/// Module layout:
///   frames       — 802.11 frame types, IEs, beacon SSID parse
///   security     — cipher/AKM suites, PMF, SecurityConfig
///   bss          — scan-result BSS list
///   iface        — interface/PHY/rate/power-save types
///   regulatory   — country code and DFS rules
///   sta          — register/scan/connect/disconnect/iwconfig
///   wpa          — WPA2 PBKDF2/PRF and EAPOL-Key frames
///   ax211        — Intel AX211 Wi-Fi 6E stub
///   brcm         — Broadcom Wi-Fi/BT combo stub
///   rtl8125      — RTL8125 2.5G Ethernet stub
///   gates        — GATE_WIFI self-test
mod ax211;
mod brcm;
mod bss;
mod frames;
mod gates;
mod iface;
mod regulatory;
mod rtl8125;
mod security;
mod sta;
mod wpa;

pub use ax211::{IntelAx211, WiFi6eBand, ax211_load_firmware, ax211_probe};
pub use brcm::{BrcmChip, BrcmDevice, brcm_bt_enable, brcm_probe, brcm_wifi_enable};
pub use bss::{BssEntry, BssSecurity};
pub use frames::{
    FrameControl, FrameType, IeId, InformationElement, MgmtSubtype, parse_beacon_ssid,
};
pub use gates::GATE_WIFI_MARKER;
pub use iface::{
    FrequencyBand, InterfaceMode, PhyMode, PowerSaveMode, RateControlAlgorithm, RateInfo, WifiBand,
    WifiChannel, WifiState, WirelessInterface,
};
pub use regulatory::{DfsRegion, RegRule, RegulatoryDomain, set_regdomain};
pub use rtl8125::{Rtl8125, rtl8125_link_up, rtl8125_probe, rtl8125_send};
pub use security::{AkmSuite, CipherSuite, PmfMode, SecurityConfig};
pub use sta::{
    connect, disconnect, get_scan_results, get_signal_strength, get_status, iwconfig_info,
    process_beacon, register_interface, scan, scan_networks, set_mode, set_power_save,
    set_tx_power,
};
pub use wpa::build_eapol_key;

use crate::serial_println;

use gates::wifi_self_test;

/// Initialize Wi-Fi subsystem
pub fn init() {
    serial_println!("[WiFi] Initializing IEEE 802.11 wireless framework");

    // Set default regulatory domain
    set_regdomain("US");

    // Register a virtual Wi-Fi interface for testing
    let mac = [0x02, 0x00, 0x00, 0x42, 0x42, 0x42]; // Local admin MAC
    let _ = register_interface("wlan0", mac);

    serial_println!("[WiFi] 802.11 framework initialized");
    let _ = wifi_self_test();
}
