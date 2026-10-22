// ═══════════════════════════════════════════════════════════════════════
// HCI PACKET TYPES
// ═══════════════════════════════════════════════════════════════════════

/// HCI packet type indicators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HciPacketType {
    Command = 0x01,
    AclData = 0x02,
    ScoData = 0x03,
    Event = 0x04,
    IsoData = 0x05,
}

/// HCI command OpCode Group Field
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum OgfGroup {
    LinkControl = 0x01,
    LinkPolicy = 0x02,
    HostController = 0x03,
    Informational = 0x04,
    StatusParams = 0x05,
    Testing = 0x06,
    LeController = 0x08,
    VendorSpecific = 0x3F,
}

/// Common HCI commands (OGF << 10 | OCF)
pub const HCI_INQUIRY: u16 = 0x0401;
pub const HCI_INQUIRY_CANCEL: u16 = 0x0402;
pub const HCI_CREATE_CONNECTION: u16 = 0x0405;
pub const HCI_DISCONNECT: u16 = 0x0406;
pub const HCI_ACCEPT_CONNECTION: u16 = 0x0409;
pub const HCI_REJECT_CONNECTION: u16 = 0x040A;
pub const HCI_LINK_KEY_REPLY: u16 = 0x040B;
pub const HCI_PIN_CODE_REPLY: u16 = 0x040D;
pub const HCI_REMOTE_NAME_REQUEST: u16 = 0x0419;
pub const HCI_READ_LOCAL_NAME: u16 = 0x0C14;
pub const HCI_WRITE_LOCAL_NAME: u16 = 0x0C13;
pub const HCI_READ_SCAN_ENABLE: u16 = 0x0C19;
pub const HCI_WRITE_SCAN_ENABLE: u16 = 0x0C1A;
pub const HCI_READ_CLASS_OF_DEVICE: u16 = 0x0C23;
pub const HCI_WRITE_CLASS_OF_DEVICE: u16 = 0x0C24;
pub const HCI_RESET: u16 = 0x0C03;
pub const HCI_SET_EVENT_MASK: u16 = 0x0C01;
pub const HCI_READ_BD_ADDR: u16 = 0x1009;
pub const HCI_READ_LOCAL_VERSION: u16 = 0x1001;
pub const HCI_READ_LOCAL_FEATURES: u16 = 0x1003;
pub const HCI_READ_BUFFER_SIZE: u16 = 0x1005;

// LE commands
pub const HCI_LE_SET_EVENT_MASK: u16 = 0x2001;
pub const HCI_LE_READ_BUFFER_SIZE: u16 = 0x2002;
pub const HCI_LE_SET_ADV_PARAMS: u16 = 0x2006;
pub const HCI_LE_SET_ADV_DATA: u16 = 0x2008;
pub const HCI_LE_SET_ADV_ENABLE: u16 = 0x200A;
pub const HCI_LE_SET_SCAN_PARAMS: u16 = 0x200B;
pub const HCI_LE_SET_SCAN_ENABLE: u16 = 0x200C;
pub const HCI_LE_CREATE_CONNECTION: u16 = 0x200D;
pub const HCI_LE_CREATE_CONN_CANCEL: u16 = 0x200E;

/// HCI event codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HciEventCode {
    InquiryComplete = 0x01,
    InquiryResult = 0x02,
    ConnectionComplete = 0x03,
    ConnectionRequest = 0x04,
    DisconnectionComplete = 0x05,
    AuthenticationComplete = 0x06,
    RemoteNameRequestComplete = 0x07,
    EncryptionChange = 0x08,
    CommandComplete = 0x0E,
    CommandStatus = 0x0F,
    NumberOfCompletedPackets = 0x13,
    PinCodeRequest = 0x16,
    LinkKeyRequest = 0x17,
    LinkKeyNotification = 0x18,
    InquiryResultWithRssi = 0x22,
    ExtendedInquiryResult = 0x2F,
    LeMeta = 0x3E,
}

// ═══════════════════════════════════════════════════════════════════════
// HCI COMMAND/EVENT STRUCTURES
// ═══════════════════════════════════════════════════════════════════════

/// HCI command header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct HciCommandHeader {
    pub opcode: u16,
    pub param_len: u8,
}

/// HCI event header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct HciEventHeader {
    pub event_code: u8,
    pub param_len: u8,
}

/// HCI ACL data header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct HciAclHeader {
    pub handle_flags: u16, // Handle (12 bits) + PB flag (2 bits) + BC flag (2 bits)
    pub data_len: u16,
}

impl HciAclHeader {
    pub fn handle(&self) -> u16 {
        u16::from_le(self.handle_flags) & 0x0FFF
    }

    pub fn pb_flag(&self) -> u8 {
        ((u16::from_le(self.handle_flags) >> 12) & 0x03) as u8
    }

    pub fn bc_flag(&self) -> u8 {
        ((u16::from_le(self.handle_flags) >> 14) & 0x03) as u8
    }
}
