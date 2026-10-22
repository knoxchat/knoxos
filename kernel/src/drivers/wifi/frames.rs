use alloc::string::String;
use alloc::vec::Vec;

/// IEEE 802.11 frame types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    Management,
    Control,
    Data,
    Extension,
}

/// IEEE 802.11 management frame subtypes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MgmtSubtype {
    AssociationRequest = 0,
    AssociationResponse = 1,
    ReassociationRequest = 2,
    ReassociationResponse = 3,
    ProbeRequest = 4,
    ProbeResponse = 5,
    Beacon = 8,
    Atim = 9,
    Disassociation = 10,
    Authentication = 11,
    Deauthentication = 12,
    Action = 13,
}

/// IEEE 802.11 frame control
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct FrameControl {
    pub protocol_version: u8,
    pub frame_type: u8,
    pub subtype: u8,
    pub to_ds: bool,
    pub from_ds: bool,
    pub more_fragments: bool,
    pub retry: bool,
    pub power_management: bool,
    pub more_data: bool,
    pub protected_frame: bool,
    pub order: bool,
}

impl FrameControl {
    pub fn from_u16(val: u16) -> Self {
        Self {
            protocol_version: (val & 0x03) as u8,
            frame_type: ((val >> 2) & 0x03) as u8,
            subtype: ((val >> 4) & 0x0F) as u8,
            to_ds: (val >> 8) & 1 != 0,
            from_ds: (val >> 9) & 1 != 0,
            more_fragments: (val >> 10) & 1 != 0,
            retry: (val >> 11) & 1 != 0,
            power_management: (val >> 12) & 1 != 0,
            more_data: (val >> 13) & 1 != 0,
            protected_frame: (val >> 14) & 1 != 0,
            order: (val >> 15) & 1 != 0,
        }
    }
}

/// 802.11 Information Element IDs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IeId {
    Ssid = 0,
    SupportedRates = 1,
    DsParamSet = 3, // Channel
    Tim = 5,        // Traffic Indication Map
    Country = 7,
    BssLoad = 11,
    PowerConstraint = 32,
    Ht = 45,  // HT Capabilities
    Rsn = 48, // RSN (WPA2) information
    ExtSupportedRates = 50,
    HtOperation = 61,
    Vht = 191, // VHT Capabilities
    VhtOperation = 192,
    VendorSpecific = 221,
}

/// Information Element
#[derive(Debug, Clone)]
pub struct InformationElement {
    pub id: u8,
    pub data: Vec<u8>,
}

/// Parse SSID from an 802.11 Beacon (mgmt subtype 8). IEs start at offset 36.
pub fn parse_beacon_ssid(frame: &[u8]) -> Option<String> {
    if frame.len() < 38 {
        return None;
    }
    let fc = u16::from_le_bytes([frame[0], frame[1]]);
    if (fc >> 2) & 0x03 != 0 || (fc >> 4) & 0x0F != 8 {
        return None;
    }
    let mut off = 36usize;
    while off + 2 <= frame.len() {
        let id = frame[off];
        let len = frame[off + 1] as usize;
        if off + 2 + len > frame.len() {
            return None;
        }
        if id == 0 {
            return Some(String::from_utf8_lossy(&frame[off + 2..off + 2 + len]).into_owned());
        }
        off += 2 + len;
    }
    None
}
