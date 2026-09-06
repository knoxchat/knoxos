/// Network Namespace Wiring
/// Integrates Linux network namespaces (CLONE_NEWNET) with the TCP/IP stack
///
/// Features:
/// - Per-namespace network interface list
/// - Per-namespace routing table
/// - Per-namespace socket isolation
/// - Veth (virtual Ethernet) pair creation for cross-namespace communication
/// - Namespace-aware socket/bind/connect
/// - Default namespace with real NIC interfaces
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── Network Namespace ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NetNamespace {
    pub id: u32,
    pub name: String,
    pub interfaces: Vec<NetInterface>,
    pub routes: Vec<Route>,
    pub arp_cache: BTreeMap<u32, [u8; 6]>, // IP -> MAC
    pub socket_count: u32,
}

#[derive(Debug, Clone)]
pub struct NetInterface {
    pub name: String,
    pub index: u32,
    pub mac: [u8; 6],
    pub ipv4_addr: u32,
    pub ipv4_mask: u32,
    pub ipv6_addr: [u8; 16],
    pub mtu: u32,
    pub flags: u32, // IFF_UP, IFF_RUNNING, etc.
    pub tx_packets: u64,
    pub rx_packets: u64,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
    pub peer_ns: Option<u32>,      // For veth: peer namespace ID
    pub peer_name: Option<String>, // For veth: peer interface name
}

#[derive(Debug, Clone)]
pub struct Route {
    pub destination: u32,  // Network address
    pub gateway: u32,      // Gateway IP (0 = directly connected)
    pub mask: u32,         // Network mask
    pub interface: String, // Interface name
    pub metric: u32,
    pub flags: u32,
}

// Interface flags
pub const IFF_UP: u32 = 1 << 0;
pub const IFF_BROADCAST: u32 = 1 << 1;
pub const IFF_LOOPBACK: u32 = 1 << 3;
pub const IFF_POINTOPOINT: u32 = 1 << 4;
pub const IFF_RUNNING: u32 = 1 << 6;
pub const IFF_MULTICAST: u32 = 1 << 12;

// Route flags
pub const RTF_UP: u32 = 0x0001;
pub const RTF_GATEWAY: u32 = 0x0002;
pub const RTF_HOST: u32 = 0x0004;

impl NetNamespace {
    pub fn new(id: u32, name: &str) -> Self {
        Self {
            id,
            name: String::from(name),
            interfaces: Vec::new(),
            routes: Vec::new(),
            arp_cache: BTreeMap::new(),
            socket_count: 0,
        }
    }

    /// Create default namespace with loopback and real interfaces
    pub fn new_default() -> Self {
        let mut ns = Self::new(0, "default");

        // Loopback interface
        ns.interfaces.push(NetInterface {
            name: String::from("lo"),
            index: 1,
            mac: [0; 6],
            ipv4_addr: u32::from_be_bytes([127, 0, 0, 1]),
            ipv4_mask: u32::from_be_bytes([255, 0, 0, 0]),
            ipv6_addr: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
            mtu: 65536,
            flags: IFF_UP | IFF_LOOPBACK | IFF_RUNNING,
            tx_packets: 0,
            rx_packets: 0,
            tx_bytes: 0,
            rx_bytes: 0,
            peer_ns: None,
            peer_name: None,
        });

        // eth0 - default NIC
        ns.interfaces.push(NetInterface {
            name: String::from("eth0"),
            index: 2,
            mac: [0x52, 0x54, 0x00, 0x12, 0x34, 0x56],
            ipv4_addr: u32::from_be_bytes([10, 0, 2, 15]),
            ipv4_mask: u32::from_be_bytes([255, 255, 255, 0]),
            ipv6_addr: [0; 16],
            mtu: 1500,
            flags: IFF_UP | IFF_BROADCAST | IFF_RUNNING | IFF_MULTICAST,
            tx_packets: 0,
            rx_packets: 0,
            tx_bytes: 0,
            rx_bytes: 0,
            peer_ns: None,
            peer_name: None,
        });

        // Default routes
        ns.routes.push(Route {
            destination: u32::from_be_bytes([10, 0, 2, 0]),
            gateway: 0,
            mask: u32::from_be_bytes([255, 255, 255, 0]),
            interface: String::from("eth0"),
            metric: 100,
            flags: RTF_UP,
        });
        ns.routes.push(Route {
            destination: 0, // default route
            gateway: u32::from_be_bytes([10, 0, 2, 2]),
            mask: 0,
            interface: String::from("eth0"),
            metric: 100,
            flags: RTF_UP | RTF_GATEWAY,
        });

        ns
    }

    /// Add an interface to this namespace
    pub fn add_interface(&mut self, iface: NetInterface) {
        self.interfaces.push(iface);
    }

    /// Remove an interface by name
    pub fn remove_interface(&mut self, name: &str) -> Option<NetInterface> {
        if let Some(pos) = self.interfaces.iter().position(|i| i.name == name) {
            Some(self.interfaces.remove(pos))
        } else {
            None
        }
    }

    /// Find interface by name
    pub fn find_interface(&self, name: &str) -> Option<&NetInterface> {
        self.interfaces.iter().find(|i| i.name == name)
    }

    /// Add a route
    pub fn add_route(&mut self, route: Route) {
        self.routes.push(route);
    }

    /// Lookup route for destination IP
    pub fn lookup_route(&self, dest_ip: u32) -> Option<&Route> {
        let mut best: Option<&Route> = None;
        let mut best_mask_len = 0u32;

        for route in &self.routes {
            if (dest_ip & route.mask) == (route.destination & route.mask) {
                let mask_len = route.mask.count_ones();
                if best.is_none() || mask_len > best_mask_len {
                    best = Some(route);
                    best_mask_len = mask_len;
                }
            }
        }
        best
    }
}

// ─── Virtual Ethernet (veth) Pair ───────────────────────────────────

/// Create a veth pair connecting two network namespaces
pub fn create_veth_pair(
    ns1_id: u32,
    name1: &str,
    ns2_id: u32,
    name2: &str,
) -> Result<(), &'static str> {
    let mut namespaces = NET_NAMESPACES.lock();

    let next_idx = NEXT_IF_INDEX.fetch_add(2, Ordering::Relaxed);

    // Create interface for ns1
    let iface1 = NetInterface {
        name: String::from(name1),
        index: next_idx,
        mac: generate_mac(next_idx),
        ipv4_addr: 0,
        ipv4_mask: 0,
        ipv6_addr: [0; 16],
        mtu: 1500,
        flags: IFF_UP | IFF_BROADCAST | IFF_RUNNING | IFF_MULTICAST,
        tx_packets: 0,
        rx_packets: 0,
        tx_bytes: 0,
        rx_bytes: 0,
        peer_ns: Some(ns2_id),
        peer_name: Some(String::from(name2)),
    };

    // Create interface for ns2
    let iface2 = NetInterface {
        name: String::from(name2),
        index: next_idx + 1,
        mac: generate_mac(next_idx + 1),
        ipv4_addr: 0,
        ipv4_mask: 0,
        ipv6_addr: [0; 16],
        mtu: 1500,
        flags: IFF_UP | IFF_BROADCAST | IFF_RUNNING | IFF_MULTICAST,
        tx_packets: 0,
        rx_packets: 0,
        tx_bytes: 0,
        rx_bytes: 0,
        peer_ns: Some(ns1_id),
        peer_name: Some(String::from(name1)),
    };

    if let Some(ns1) = namespaces.get_mut(&ns1_id) {
        ns1.add_interface(iface1);
    } else {
        return Err("Namespace 1 not found");
    }

    if let Some(ns2) = namespaces.get_mut(&ns2_id) {
        ns2.add_interface(iface2);
    } else {
        return Err("Namespace 2 not found");
    }

    serial_println!(
        "[NETNS] Created veth pair: {} (ns{}) <-> {} (ns{})",
        name1,
        ns1_id,
        name2,
        ns2_id
    );
    Ok(())
}

/// Generate a deterministic MAC address from index
fn generate_mac(index: u32) -> [u8; 6] {
    [
        0x02,
        0x42, // locally administered
        ((index >> 24) & 0xFF) as u8,
        ((index >> 16) & 0xFF) as u8,
        ((index >> 8) & 0xFF) as u8,
        (index & 0xFF) as u8,
    ]
}

// ─── Namespace Management ───────────────────────────────────────────

static NET_NAMESPACES: Mutex<BTreeMap<u32, NetNamespace>> = Mutex::new(BTreeMap::new());
static NEXT_NS_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_IF_INDEX: AtomicU32 = AtomicU32::new(10);

/// Per-process network namespace mapping
static PROCESS_NETNS: Mutex<BTreeMap<u32, u32>> = Mutex::new(BTreeMap::new());

/// Create a new network namespace
pub fn create_netns(name: &str) -> u32 {
    let id = NEXT_NS_ID.fetch_add(1, Ordering::Relaxed);
    let mut ns = NetNamespace::new(id, name);

    // Every namespace gets a loopback
    ns.interfaces.push(NetInterface {
        name: String::from("lo"),
        index: NEXT_IF_INDEX.fetch_add(1, Ordering::Relaxed),
        mac: [0; 6],
        ipv4_addr: u32::from_be_bytes([127, 0, 0, 1]),
        ipv4_mask: u32::from_be_bytes([255, 0, 0, 0]),
        ipv6_addr: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
        mtu: 65536,
        flags: IFF_UP | IFF_LOOPBACK | IFF_RUNNING,
        tx_packets: 0,
        rx_packets: 0,
        tx_bytes: 0,
        rx_bytes: 0,
        peer_ns: None,
        peer_name: None,
    });

    NET_NAMESPACES.lock().insert(id, ns);
    serial_println!("[NETNS] Created network namespace {} '{}'", id, name);
    id
}

/// Delete a network namespace
pub fn delete_netns(id: u32) -> Result<(), &'static str> {
    if id == 0 {
        return Err("Cannot delete default namespace");
    }
    if NET_NAMESPACES.lock().remove(&id).is_some() {
        serial_println!("[NETNS] Deleted network namespace {}", id);
        Ok(())
    } else {
        Err("Namespace not found")
    }
}

/// Assign a process to a network namespace
pub fn set_process_netns(pid: u32, ns_id: u32) {
    PROCESS_NETNS.lock().insert(pid, ns_id);
}

/// Get the network namespace for a process
pub fn get_process_netns(pid: u32) -> u32 {
    PROCESS_NETNS.lock().get(&pid).copied().unwrap_or(0)
}

/// Move an interface between namespaces
pub fn move_interface(from_ns: u32, to_ns: u32, if_name: &str) -> Result<(), &'static str> {
    let mut namespaces = NET_NAMESPACES.lock();

    let iface = if let Some(ns) = namespaces.get_mut(&from_ns) {
        ns.remove_interface(if_name)
            .ok_or("Interface not found in source namespace")?
    } else {
        return Err("Source namespace not found");
    };

    if let Some(ns) = namespaces.get_mut(&to_ns) {
        ns.add_interface(iface);
        serial_println!(
            "[NETNS] Moved interface {} from ns{} to ns{}",
            if_name,
            from_ns,
            to_ns
        );
        Ok(())
    } else {
        Err("Destination namespace not found")
    }
}

/// Configure IP address on an interface within a namespace
pub fn set_interface_addr(
    ns_id: u32,
    if_name: &str,
    ipv4: u32,
    mask: u32,
) -> Result<(), &'static str> {
    let mut namespaces = NET_NAMESPACES.lock();
    if let Some(ns) = namespaces.get_mut(&ns_id) {
        if let Some(iface) = ns.interfaces.iter_mut().find(|i| i.name == if_name) {
            iface.ipv4_addr = ipv4;
            iface.ipv4_mask = mask;
            Ok(())
        } else {
            Err("Interface not found")
        }
    } else {
        Err("Namespace not found")
    }
}

/// List all network namespaces
pub fn list_namespaces() -> Vec<(u32, String, usize)> {
    let ns = NET_NAMESPACES.lock();
    ns.values()
        .map(|n| (n.id, n.name.clone(), n.interfaces.len()))
        .collect()
}

/// Get interfaces in a namespace
pub fn get_interfaces(ns_id: u32) -> Vec<NetInterface> {
    let ns = NET_NAMESPACES.lock();
    if let Some(n) = ns.get(&ns_id) {
        n.interfaces.clone()
    } else {
        Vec::new()
    }
}

pub fn init() {
    // Create default namespace with real interfaces
    let default_ns = NetNamespace::new_default();
    NET_NAMESPACES.lock().insert(0, default_ns);

    serial_println!("[NETNS] Network namespace subsystem initialized");
    serial_println!("[NETNS]   Default namespace: lo, eth0 (10.0.2.15/24)");
}
