/// Bridge / TAP — Virtual network bridge and TAP devices for containers
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// MAC address
pub type MacAddr = [u8; 6];

/// A bridge port (member interface)
#[derive(Debug, Clone)]
pub struct BridgePort {
    pub iface: String,
    pub port_id: u16,
    pub state: PortState,
    pub hairpin: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortState {
    Disabled,
    Listening,
    Learning,
    Forwarding,
    Blocking,
}

/// MAC forwarding table entry
#[derive(Debug, Clone)]
pub struct FdbEntry {
    pub mac: MacAddr,
    pub port_id: u16,
    pub age_ticks: u32,
    pub is_static: bool,
}

/// A virtual bridge
#[derive(Debug, Clone)]
pub struct Bridge {
    pub name: String,
    pub mac: MacAddr,
    pub ports: Vec<BridgePort>,
    pub fdb: Vec<FdbEntry>,
    pub stp_enabled: bool,
    pub ageing_time_secs: u32,
    pub up: bool,
}

/// A TAP device (virtual ethernet endpoint for userspace)
#[derive(Debug, Clone)]
pub struct TapDevice {
    pub name: String,
    pub mac: MacAddr,
    pub attached_bridge: Option<String>,
    pub fd: i32,
    pub up: bool,
}

lazy_static::lazy_static! {
    static ref BRIDGES: Mutex<Vec<Bridge>> = Mutex::new(Vec::new());
    static ref TAPS: Mutex<Vec<TapDevice>> = Mutex::new(Vec::new());
}

/// Create a new bridge
pub fn bridge_create(name: &str) -> Result<(), &'static str> {
    let br = Bridge {
        name: String::from(name),
        mac: [0x02, 0x42, 0xAC, 0x11, 0x00, 0x01], // local admin MAC
        ports: Vec::new(),
        fdb: Vec::new(),
        stp_enabled: false,
        ageing_time_secs: 300,
        up: false,
    };
    serial_println!("[bridge] Created bridge {}", name);
    BRIDGES.lock().push(br);
    Ok(())
}

/// Add a port to a bridge
pub fn bridge_add_port(bridge: &str, iface: &str) -> Result<(), &'static str> {
    let mut bridges = BRIDGES.lock();
    let br = bridges
        .iter_mut()
        .find(|b| b.name == bridge)
        .ok_or("Bridge not found")?;
    let port_id = br.ports.len() as u16 + 1;
    br.ports.push(BridgePort {
        iface: String::from(iface),
        port_id,
        state: PortState::Forwarding,
        hairpin: false,
    });
    serial_println!("[bridge] Added {} to {} (port {})", iface, bridge, port_id);
    Ok(())
}

/// Remove a port from a bridge
pub fn bridge_del_port(bridge: &str, iface: &str) -> Result<(), &'static str> {
    let mut bridges = BRIDGES.lock();
    let br = bridges
        .iter_mut()
        .find(|b| b.name == bridge)
        .ok_or("Bridge not found")?;
    br.ports.retain(|p| p.iface != iface);
    Ok(())
}

/// Forward an Ethernet frame through the bridge
pub fn bridge_forward(bridge: &str, src_mac: MacAddr, dst_mac: MacAddr, in_port: u16) -> Vec<u16> {
    let mut bridges = BRIDGES.lock();
    let br = match bridges.iter_mut().find(|b| b.name == bridge) {
        Some(b) => b,
        None => return Vec::new(),
    };

    // Learn source MAC
    if !br
        .fdb
        .iter()
        .any(|e| e.mac == src_mac && e.port_id == in_port)
    {
        br.fdb.push(FdbEntry {
            mac: src_mac,
            port_id: in_port,
            age_ticks: 0,
            is_static: false,
        });
    }

    // Lookup destination
    if dst_mac == [0xFF; 6] {
        // Broadcast: flood to all ports except ingress
        return br
            .ports
            .iter()
            .filter(|p| p.port_id != in_port && p.state == PortState::Forwarding)
            .map(|p| p.port_id)
            .collect();
    }

    if let Some(entry) = br.fdb.iter().find(|e| e.mac == dst_mac) {
        if entry.port_id != in_port {
            return alloc::vec![entry.port_id];
        }
        return Vec::new(); // same port — do not forward
    }

    // Unknown unicast — flood
    br.ports
        .iter()
        .filter(|p| p.port_id != in_port && p.state == PortState::Forwarding)
        .map(|p| p.port_id)
        .collect()
}

/// Age out old FDB entries
pub fn bridge_age_fdb() {
    let mut bridges = BRIDGES.lock();
    for br in bridges.iter_mut() {
        for entry in br.fdb.iter_mut() {
            if !entry.is_static {
                entry.age_ticks += 1;
            }
        }
        let max_age = br.ageing_time_secs;
        br.fdb.retain(|e| e.is_static || e.age_ticks < max_age);
    }
}

/// Create a TAP device
pub fn tap_create(name: &str) -> Result<(), &'static str> {
    let tap = TapDevice {
        name: String::from(name),
        mac: [0x02, 0x42, 0xAC, 0x11, 0x00, 0x02],
        attached_bridge: None,
        fd: -1,
        up: false,
    };
    serial_println!("[bridge] Created TAP device {}", name);
    TAPS.lock().push(tap);
    Ok(())
}

/// Attach a TAP device to a bridge
pub fn tap_attach_bridge(tap_name: &str, bridge: &str) -> Result<(), &'static str> {
    let mut taps = TAPS.lock();
    let tap = taps
        .iter_mut()
        .find(|t| t.name == tap_name)
        .ok_or("TAP not found")?;
    tap.attached_bridge = Some(String::from(bridge));
    // Also add as bridge port
    drop(taps);
    bridge_add_port(bridge, tap_name)
}

/// Destroy a bridge
pub fn bridge_delete(name: &str) -> bool {
    let mut bridges = BRIDGES.lock();
    if let Some(pos) = bridges.iter().position(|b| b.name == name) {
        bridges.remove(pos);
        true
    } else {
        false
    }
}

pub fn init() {
    serial_println!("[bridge] Bridge/TAP networking initialized");
}
