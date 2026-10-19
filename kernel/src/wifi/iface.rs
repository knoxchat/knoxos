use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use super::security::SecurityConfig;

/// Wireless interface mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterfaceMode {
    Station,   // Client mode (STA)
    Ap,        // Access Point
    Monitor,   // Promiscuous monitoring
    Adhoc,     // IBSS
    P2pClient, // Wi-Fi Direct client
    P2pGo,     // Wi-Fi Direct Group Owner
    Mesh,      // Mesh Point
}

/// Wireless interface state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiState {
    Disconnected,
    Scanning,
    Authenticating,
    Associating,
    FourWayHandshake,
    GroupHandshake,
    Connected,
    Disconnecting,
}

/// PHY modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhyMode {
    Ieee80211a,  // 5 GHz OFDM
    Ieee80211b,  // 2.4 GHz DSSS/CCK
    Ieee80211g,  // 2.4 GHz OFDM
    Ieee80211n,  // HT (Wi-Fi 4)
    Ieee80211ac, // VHT (Wi-Fi 5)
    Ieee80211ax, // HE (Wi-Fi 6)
}

/// Band info
#[derive(Debug, Clone)]
pub struct WifiBand {
    pub band: FrequencyBand,
    pub channels: Vec<WifiChannel>,
    pub ht_capable: bool,
    pub vht_capable: bool,
}

/// Frequency band
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrequencyBand {
    Band2Ghz,
    Band5Ghz,
    Band6Ghz,
}

/// Channel info
#[derive(Debug, Clone, Copy)]
pub struct WifiChannel {
    pub number: u8,
    pub frequency: u32, // MHz
    pub max_power: i32, // dBm
    pub flags: u32,
    pub dfs: bool, // Dynamic Frequency Selection
}

/// Wireless interface
#[derive(Debug, Clone)]
pub struct WirelessInterface {
    pub name: String,
    pub index: u32,
    pub mac: [u8; 6],
    pub mode: InterfaceMode,
    pub state: WifiState,
    pub phy_index: u32,
    pub channel: Option<u8>,
    pub frequency: Option<u32>,
    pub bandwidth: u32, // MHz: 20, 40, 80, 160
    pub tx_power: i32,  // dBm
    pub signal: i32,    // dBm (when connected)
    pub ssid: Option<String>,
    pub bssid: Option<[u8; 6]>,
    pub security: Option<SecurityConfig>,
    pub bands: Vec<WifiBand>,
}

/// Global wireless interface registry
pub(super) static WIFI_INTERFACES: Mutex<BTreeMap<String, WirelessInterface>> =
    Mutex::new(BTreeMap::new());

/// Rate control algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateControlAlgorithm {
    Minstrel,   // Linux default
    MinstrelHt, // HT version
    Pid,        // PID controller
    Fixed,      // Fixed rate
}

/// Rate info
#[derive(Debug, Clone)]
pub struct RateInfo {
    pub algorithm: RateControlAlgorithm,
    pub current_rate_mbps: u32,
    pub max_rate_mbps: u32,
    pub mcs_index: Option<u8>,
    pub nss: u8,        // Number of spatial streams
    pub short_gi: bool, // Short guard interval
}

/// Wi-Fi power save mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSaveMode {
    Disabled,
    Legacy,  // 802.11 PS-Poll
    Uapsd,   // Unscheduled APSD
    Dynamic, // Dynamic PS
}
