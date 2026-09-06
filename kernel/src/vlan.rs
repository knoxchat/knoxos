/// VLAN — IEEE 802.1Q VLAN tagging for Ethernet frames
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// VLAN ID (1-4094)
pub type VlanId = u16;

/// 802.1Q tag header (4 bytes inserted after src MAC)
#[derive(Debug, Clone, Copy)]
pub struct VlanTag {
    /// Tag Protocol Identifier — always 0x8100
    pub tpid: u16,
    /// Priority Code Point (0-7)
    pub pcp: u8,
    /// Drop Eligible Indicator
    pub dei: bool,
    /// VLAN Identifier (0-4095, 0 = priority-tagged, 4095 = reserved)
    pub vid: VlanId,
}

impl VlanTag {
    pub fn new(vid: VlanId, pcp: u8) -> Self {
        Self {
            tpid: 0x8100,
            pcp: pcp & 0x07,
            dei: false,
            vid: vid & 0x0FFF,
        }
    }

    /// Encode to 4 bytes (big-endian)
    pub fn encode(&self) -> [u8; 4] {
        let tci = ((self.pcp as u16 & 0x07) << 13)
            | if self.dei { 1 << 12 } else { 0 }
            | (self.vid & 0x0FFF);
        let tpid = self.tpid.to_be_bytes();
        let tci_bytes = tci.to_be_bytes();
        [tpid[0], tpid[1], tci_bytes[0], tci_bytes[1]]
    }

    /// Decode from 4 bytes
    pub fn decode(data: &[u8; 4]) -> Self {
        let tpid = u16::from_be_bytes([data[0], data[1]]);
        let tci = u16::from_be_bytes([data[2], data[3]]);
        Self {
            tpid,
            pcp: ((tci >> 13) & 0x07) as u8,
            dei: (tci >> 12) & 1 != 0,
            vid: tci & 0x0FFF,
        }
    }
}

/// A VLAN interface on a physical NIC
#[derive(Debug, Clone)]
pub struct VlanInterface {
    pub parent_iface: String,
    pub vid: VlanId,
    pub name: String,
    pub mtu: u16,
    pub up: bool,
}

lazy_static::lazy_static! {
    static ref VLAN_INTERFACES: Mutex<BTreeMap<String, VlanInterface>> = Mutex::new(BTreeMap::new());
}

/// Create a VLAN sub-interface (e.g., eth0.100)
pub fn create(parent: &str, vid: VlanId) -> Result<String, &'static str> {
    if vid == 0 || vid > 4094 {
        return Err("Invalid VLAN ID (must be 1-4094)");
    }
    let name = alloc::format!("{}.{}", parent, vid);
    let iface = VlanInterface {
        parent_iface: String::from(parent),
        vid,
        name: name.clone(),
        mtu: 1500,
        up: false,
    };
    serial_println!("[vlan] Created {} (VID={}) on {}", name, vid, parent);
    VLAN_INTERFACES.lock().insert(name.clone(), iface);
    Ok(name)
}

/// Remove a VLAN interface
pub fn destroy(name: &str) -> bool {
    VLAN_INTERFACES.lock().remove(name).is_some()
}

/// Bring a VLAN interface up
pub fn up(name: &str) -> bool {
    if let Some(iface) = VLAN_INTERFACES.lock().get_mut(name) {
        iface.up = true;
        true
    } else {
        false
    }
}

/// Insert 802.1Q tag into an Ethernet frame
pub fn tag_frame(frame: &[u8], vid: VlanId, pcp: u8) -> Vec<u8> {
    if frame.len() < 14 {
        return frame.to_vec();
    }
    let tag = VlanTag::new(vid, pcp);
    let mut tagged = Vec::with_capacity(frame.len() + 4);
    tagged.extend_from_slice(&frame[..12]); // dst + src MAC
    tagged.extend_from_slice(&tag.encode());
    tagged.extend_from_slice(&frame[12..]); // ethertype + payload
    tagged
}

/// Strip 802.1Q tag from an Ethernet frame, returns (vid, untagged_frame)
pub fn untag_frame(frame: &[u8]) -> Option<(VlanId, Vec<u8>)> {
    if frame.len() < 18 {
        return None;
    }
    let tpid = u16::from_be_bytes([frame[12], frame[13]]);
    if tpid != 0x8100 {
        return None;
    }
    let tag = VlanTag::decode(&[frame[12], frame[13], frame[14], frame[15]]);
    let mut untagged = Vec::with_capacity(frame.len() - 4);
    untagged.extend_from_slice(&frame[..12]);
    untagged.extend_from_slice(&frame[16..]);
    Some((tag.vid, untagged))
}

pub fn list() -> Vec<(String, VlanId, bool)> {
    VLAN_INTERFACES
        .lock()
        .iter()
        .map(|(n, i)| (n.clone(), i.vid, i.up))
        .collect()
}

pub fn init() {
    serial_println!("[vlan] 802.1Q VLAN tagging initialized");
}
