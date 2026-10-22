use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// L2CAP (Logical Link Control and Adaptation Protocol)
// ═══════════════════════════════════════════════════════════════════════

/// L2CAP Channel IDs
pub const L2CAP_CID_SIGNALING: u16 = 0x0001;
pub const L2CAP_CID_CONNECTIONLESS: u16 = 0x0002;
pub const L2CAP_CID_ATT: u16 = 0x0004; // BLE Attribute Protocol
pub const L2CAP_CID_LE_SIGNALING: u16 = 0x0005;
pub const L2CAP_CID_SMP: u16 = 0x0006; // BLE Security Manager

/// L2CAP header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct L2capHeader {
    pub length: u16,
    pub channel_id: u16,
}

/// L2CAP channel
#[derive(Debug, Clone)]
pub struct L2capChannel {
    pub local_cid: u16,
    pub remote_cid: u16,
    pub psm: u16, // Protocol/Service Multiplexer
    pub mtu: u16,
    pub state: L2capState,
    pub connection_handle: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum L2capState {
    Closed,
    WaitConnect,
    WaitConnectRsp,
    Config,
    Open,
    WaitDisconnect,
}

/// L2CAP PSM values
pub const L2CAP_PSM_SDP: u16 = 0x0001;
pub const L2CAP_PSM_RFCOMM: u16 = 0x0003;
pub const L2CAP_PSM_BNEP: u16 = 0x000F;
pub const L2CAP_PSM_AVCTP: u16 = 0x0017;
pub const L2CAP_PSM_AVDTP: u16 = 0x0019;

/// Global L2CAP channel registry
pub(crate) static L2CAP_CHANNELS: Mutex<Vec<L2capChannel>> = Mutex::new(Vec::new());
static NEXT_LOCAL_CID: AtomicU16 = AtomicU16::new(0x0040);

/// Open an L2CAP channel
pub fn l2cap_connect(connection_handle: u16, psm: u16) -> Result<u16, &'static str> {
    let local_cid = NEXT_LOCAL_CID.fetch_add(1, Ordering::SeqCst);
    let mut channels = L2CAP_CHANNELS.lock();

    channels.push(L2capChannel {
        local_cid,
        remote_cid: 0,
        psm,
        mtu: 672, // Default L2CAP MTU
        state: L2capState::WaitConnect,
        connection_handle,
    });

    serial_println!("[L2CAP] Open channel CID={} PSM=0x{:04X}", local_cid, psm);
    Ok(local_cid)
}
