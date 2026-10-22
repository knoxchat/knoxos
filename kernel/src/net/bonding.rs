/// Bonding / Link Aggregation — IEEE 802.3ad LACP
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BondMode {
    RoundRobin,   // mode 0
    ActiveBackup, // mode 1
    XorHash,      // mode 2
    Broadcast,    // mode 3
    Lacp,         // mode 4 — 802.3ad
    BalanceTlb,   // mode 5
    BalanceAlb,   // mode 6
}

#[derive(Debug, Clone)]
pub struct BondInterface {
    pub name: String,
    pub mode: BondMode,
    pub members: Vec<BondMember>,
    pub mtu: u16,
    pub up: bool,
    pub mii_interval_ms: u32,
    pub xmit_hash: XmitHash,
}

#[derive(Debug, Clone)]
pub struct BondMember {
    pub iface: String,
    pub active: bool,
    pub link_up: bool,
    pub speed_mbps: u32,
    pub lacp_partner_mac: Option<[u8; 6]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmitHash {
    Layer2,  // src+dst MAC
    Layer34, // src+dst IP + port
    Layer23, // src+dst MAC + IP
}

lazy_static::lazy_static! {
    static ref BONDS: Mutex<Vec<BondInterface>> = Mutex::new(Vec::new());
}

/// Create a bonding interface
pub fn create(name: &str, mode: BondMode) -> Result<(), &'static str> {
    let bond = BondInterface {
        name: String::from(name),
        mode,
        members: Vec::new(),
        mtu: 1500,
        up: false,
        mii_interval_ms: 100,
        xmit_hash: XmitHash::Layer34,
    };
    serial_println!("[bonding] Created {} mode {:?}", name, mode);
    BONDS.lock().push(bond);
    Ok(())
}

/// Add a member interface to a bond
pub fn add_member(bond_name: &str, iface: &str) -> Result<(), &'static str> {
    let mut bonds = BONDS.lock();
    let bond = bonds
        .iter_mut()
        .find(|b| b.name == bond_name)
        .ok_or("Bond not found")?;
    bond.members.push(BondMember {
        iface: String::from(iface),
        active: true,
        link_up: true,
        speed_mbps: 1000,
        lacp_partner_mac: None,
    });
    serial_println!("[bonding] Added {} to {}", iface, bond_name);
    Ok(())
}

/// Remove a member from a bond
pub fn remove_member(bond_name: &str, iface: &str) -> Result<(), &'static str> {
    let mut bonds = BONDS.lock();
    let bond = bonds
        .iter_mut()
        .find(|b| b.name == bond_name)
        .ok_or("Bond not found")?;
    bond.members.retain(|m| m.iface != iface);
    Ok(())
}

/// Select the transmit interface for a packet using the hash policy
pub fn select_tx_member(
    bond: &BondInterface,
    src_ip: u32,
    dst_ip: u32,
    src_port: u16,
    dst_port: u16,
) -> Option<usize> {
    let active: Vec<usize> = bond
        .members
        .iter()
        .enumerate()
        .filter(|(_, m)| m.active && m.link_up)
        .map(|(i, _)| i)
        .collect();
    if active.is_empty() {
        return None;
    }

    let hash = match bond.xmit_hash {
        XmitHash::Layer2 => 0u32, // would hash MACs
        XmitHash::Layer34 => src_ip
            .wrapping_add(dst_ip)
            .wrapping_add(src_port as u32)
            .wrapping_add(dst_port as u32),
        XmitHash::Layer23 => src_ip.wrapping_add(dst_ip),
    };

    Some(active[hash as usize % active.len()])
}

/// MII link monitoring tick — detect member link failures
pub fn mii_monitor() {
    let mut bonds = BONDS.lock();
    for bond in bonds.iter_mut() {
        for member in bond.members.iter_mut() {
            // Would check PHY link status here
            // If link lost → mark inactive, failover to backup
        }
    }
}

/// Destroy a bond interface
pub fn destroy(name: &str) -> bool {
    let mut bonds = BONDS.lock();
    if let Some(pos) = bonds.iter().position(|b| b.name == name) {
        bonds.remove(pos);
        true
    } else {
        false
    }
}

pub fn init() {
    serial_println!("[bonding] Link aggregation (802.3ad LACP) initialized");
}
