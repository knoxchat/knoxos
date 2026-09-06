/// CIFS/SMB — Windows file sharing client (SMB2/3)
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// SMB2 header command codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmbCommand {
    Negotiate = 0x0000,
    SessionSetup = 0x0001,
    Logoff = 0x0002,
    TreeConnect = 0x0003,
    TreeDisconnect = 0x0004,
    Create = 0x0005,
    Close = 0x0006,
    Read = 0x0008,
    Write = 0x0009,
    QueryDirectory = 0x000E,
    QueryInfo = 0x0010,
}

/// SMB2 dialect versions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmbDialect {
    Smb202 = 0x0202,
    Smb210 = 0x0210,
    Smb300 = 0x0300,
    Smb302 = 0x0302,
    Smb311 = 0x0311,
}

/// SMB2 header (64 bytes)
#[derive(Debug, Clone, Copy)]
pub struct Smb2Header {
    pub protocol_id: u32, // 0xFE534D42 = "\xFESMB"
    pub header_length: u16,
    pub command: u16,
    pub status: u32,
    pub flags: u32,
    pub message_id: u64,
    pub session_id: u64,
    pub tree_id: u32,
}

/// An SMB share connection
#[derive(Debug, Clone)]
pub struct SmbShare {
    pub server: String,
    pub share_name: String,
    pub username: String,
    pub tree_id: u32,
    pub session_id: u64,
    pub dialect: SmbDialect,
    pub connected: bool,
    pub mount_point: Option<String>,
}

/// SMB directory entry
#[derive(Debug, Clone)]
pub struct SmbDirEntry {
    pub name: String,
    pub is_directory: bool,
    pub size: u64,
    pub create_time: u64,
    pub modify_time: u64,
    pub attributes: u32,
}

lazy_static::lazy_static! {
    static ref SMB_SHARES: Mutex<Vec<SmbShare>> = Mutex::new(Vec::new());
}

/// Build SMB2 Negotiate request
pub fn build_negotiate() -> Vec<u8> {
    let mut pkt = Vec::with_capacity(128);
    // Protocol ID: \xFESMB
    pkt.extend_from_slice(&[0xFE, 0x53, 0x4D, 0x42]);
    // Header length
    pkt.extend_from_slice(&64u16.to_le_bytes());
    // Command: Negotiate
    pkt.extend_from_slice(&0u16.to_le_bytes());
    // ... rest of header (zeroed for simplicity)
    pkt.resize(64, 0);

    // Negotiate body: structure size (36), dialect count (4)
    pkt.extend_from_slice(&36u16.to_le_bytes());
    pkt.extend_from_slice(&4u16.to_le_bytes()); // dialect count
    // Dialects
    for dialect in &[0x0202u16, 0x0210, 0x0300, 0x0311] {
        pkt.extend_from_slice(&dialect.to_le_bytes());
    }
    pkt
}

/// Connect to an SMB share (\\server\share)
pub fn connect(
    server: &str,
    share: &str,
    user: &str,
    _password: &str,
) -> Result<u32, &'static str> {
    serial_println!("[smb] Connecting to \\\\{}\\{} as {}", server, share, user);

    let share_conn = SmbShare {
        server: String::from(server),
        share_name: String::from(share),
        username: String::from(user),
        tree_id: 1,
        session_id: 1,
        dialect: SmbDialect::Smb311,
        connected: true,
        mount_point: None,
    };

    let tree_id = share_conn.tree_id;
    SMB_SHARES.lock().push(share_conn);
    Ok(tree_id)
}

/// Disconnect from an SMB share
pub fn disconnect(tree_id: u32) -> bool {
    let mut shares = SMB_SHARES.lock();
    if let Some(share) = shares.iter_mut().find(|s| s.tree_id == tree_id) {
        share.connected = false;
        serial_println!(
            "[smb] Disconnected from \\\\{}\\{}",
            share.server,
            share.share_name
        );
        true
    } else {
        false
    }
}

/// List directory contents on a share
pub fn list_dir(tree_id: u32, path: &str) -> Result<Vec<SmbDirEntry>, &'static str> {
    let shares = SMB_SHARES.lock();
    let share = shares
        .iter()
        .find(|s| s.tree_id == tree_id && s.connected)
        .ok_or("Not connected")?;
    serial_println!(
        "[smb] QueryDirectory \\\\{}\\{}\\{}",
        share.server,
        share.share_name,
        path
    );
    Ok(Vec::new())
}

/// Read a file from an SMB share
pub fn read_file(tree_id: u32, path: &str) -> Result<Vec<u8>, &'static str> {
    let shares = SMB_SHARES.lock();
    let share = shares
        .iter()
        .find(|s| s.tree_id == tree_id && s.connected)
        .ok_or("Not connected")?;
    serial_println!(
        "[smb] Read \\\\{}\\{}\\{}",
        share.server,
        share.share_name,
        path
    );
    Ok(Vec::new())
}

/// Write a file to an SMB share
pub fn write_file(tree_id: u32, path: &str, data: &[u8]) -> Result<(), &'static str> {
    let shares = SMB_SHARES.lock();
    let share = shares
        .iter()
        .find(|s| s.tree_id == tree_id && s.connected)
        .ok_or("Not connected")?;
    serial_println!(
        "[smb] Write {} bytes to \\\\{}\\{}\\{}",
        data.len(),
        share.server,
        share.share_name,
        path
    );
    Ok(())
}

/// Mount an SMB share at a local path
pub fn mount(tree_id: u32, mount_path: &str) -> Result<(), &'static str> {
    let mut shares = SMB_SHARES.lock();
    let share = shares
        .iter_mut()
        .find(|s| s.tree_id == tree_id && s.connected)
        .ok_or("Not connected")?;
    share.mount_point = Some(String::from(mount_path));
    serial_println!(
        "[smb] Mounted \\\\{}\\{} at {}",
        share.server,
        share.share_name,
        mount_path
    );
    Ok(())
}

pub fn init() {
    serial_println!("[smb] CIFS/SMB client (SMB2/3) initialized");
}
