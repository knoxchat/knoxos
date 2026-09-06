/// Network Manager — connection profile management and auto-configuration
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

static NEXT_CONN_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionType {
    Ethernet,
    Wifi,
    Vpn,
    Bridge,
    Bond,
    Vlan,
    Loopback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ipv4Method {
    Auto, // DHCP
    Manual,
    Disabled,
}

#[derive(Debug, Clone)]
pub struct IpConfig {
    pub method: Ipv4Method,
    pub address: Option<u32>,
    pub netmask: Option<u32>,
    pub gateway: Option<u32>,
    pub dns: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct WifiConfig {
    pub ssid: String,
    pub security: String, // "wpa2", "wpa3", "open"
    pub hidden: bool,
}

/// A connection profile
#[derive(Debug, Clone)]
pub struct ConnectionProfile {
    pub id: u64,
    pub name: String,
    pub conn_type: ConnectionType,
    pub interface: String,
    pub auto_connect: bool,
    pub priority: i32,
    pub ipv4: IpConfig,
    pub wifi: Option<WifiConfig>,
    pub state: ConnectionState,
}

impl ConnectionProfile {
    pub fn new_ethernet(name: &str, iface: &str) -> Self {
        Self {
            id: NEXT_CONN_ID.fetch_add(1, Ordering::Relaxed),
            name: String::from(name),
            conn_type: ConnectionType::Ethernet,
            interface: String::from(iface),
            auto_connect: true,
            priority: 0,
            ipv4: IpConfig {
                method: Ipv4Method::Auto,
                address: None,
                netmask: None,
                gateway: None,
                dns: Vec::new(),
            },
            wifi: None,
            state: ConnectionState::Disconnected,
        }
    }

    pub fn new_wifi(name: &str, ssid: &str) -> Self {
        Self {
            id: NEXT_CONN_ID.fetch_add(1, Ordering::Relaxed),
            name: String::from(name),
            conn_type: ConnectionType::Wifi,
            interface: String::from("wlan0"),
            auto_connect: true,
            priority: 0,
            ipv4: IpConfig {
                method: Ipv4Method::Auto,
                address: None,
                netmask: None,
                gateway: None,
                dns: Vec::new(),
            },
            wifi: Some(WifiConfig {
                ssid: String::from(ssid),
                security: String::from("wpa2"),
                hidden: false,
            }),
            state: ConnectionState::Disconnected,
        }
    }
}

lazy_static::lazy_static! {
    static ref PROFILES: Mutex<Vec<ConnectionProfile>> = Mutex::new(Vec::new());
}

static NM_RUNNING: AtomicBool = AtomicBool::new(false);

/// Add a connection profile
pub fn add_profile(profile: ConnectionProfile) -> u64 {
    let id = profile.id;
    serial_println!(
        "[nm] Added profile: {} ({})",
        profile.name,
        profile.interface
    );
    PROFILES.lock().push(profile);
    id
}

/// Remove a connection profile
pub fn remove_profile(id: u64) -> bool {
    let mut profiles = PROFILES.lock();
    if let Some(pos) = profiles.iter().position(|p| p.id == id) {
        profiles.remove(pos);
        true
    } else {
        false
    }
}

/// Activate a connection (bring up the interface)
pub fn activate(id: u64) -> Result<(), &'static str> {
    let mut profiles = PROFILES.lock();
    let profile = profiles
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or("Profile not found")?;
    serial_println!("[nm] Activating {} on {}", profile.name, profile.interface);
    profile.state = ConnectionState::Connecting;

    match profile.ipv4.method {
        Ipv4Method::Auto => {
            serial_println!("[nm] Starting DHCP on {}", profile.interface);
        }
        Ipv4Method::Manual => {
            if let Some(addr) = profile.ipv4.address {
                serial_println!(
                    "[nm] Setting static IP {:08X} on {}",
                    addr,
                    profile.interface
                );
            }
        }
        Ipv4Method::Disabled => {}
    }

    profile.state = ConnectionState::Connected;
    Ok(())
}

/// Deactivate a connection
pub fn deactivate(id: u64) -> Result<(), &'static str> {
    let mut profiles = PROFILES.lock();
    let profile = profiles
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or("Profile not found")?;
    profile.state = ConnectionState::Disconnecting;
    serial_println!(
        "[nm] Deactivating {} on {}",
        profile.name,
        profile.interface
    );
    profile.state = ConnectionState::Disconnected;
    Ok(())
}

/// List all connection profiles
pub fn list_profiles() -> Vec<(u64, String, ConnectionType, ConnectionState)> {
    PROFILES
        .lock()
        .iter()
        .map(|p| (p.id, p.name.clone(), p.conn_type, p.state))
        .collect()
}

/// Auto-connect: activate all auto-connect profiles in priority order
pub fn auto_connect() {
    let mut profiles = PROFILES.lock();
    profiles.sort_by_key(|b| core::cmp::Reverse(b.priority));
    for profile in profiles.iter_mut() {
        if profile.auto_connect && profile.state == ConnectionState::Disconnected {
            serial_println!("[nm] Auto-connecting: {}", profile.name);
            profile.state = ConnectionState::Connected;
        }
    }
}

/// Start the Network Manager daemon
pub fn start() {
    NM_RUNNING.store(true, Ordering::SeqCst);
    serial_println!("[nm] Network Manager daemon started");
    auto_connect();
}

/// Stop the daemon
pub fn stop() {
    NM_RUNNING.store(false, Ordering::SeqCst);
}

pub fn init() {
    serial_println!("[nm] Network Manager initialized");
}
