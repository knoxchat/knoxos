use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

use super::client::{NfsClient, NfsMountOptions};
use super::types::NfsStat;

// ═══════════════════════════════════════════════════════════════════════
// NFS MOUNT TABLE
// ═══════════════════════════════════════════════════════════════════════

/// NFS mount entry
pub struct NfsMount {
    pub mount_point: String,
    pub server: String,
    pub export_path: String,
    pub client: NfsClient,
}

lazy_static::lazy_static! {
    static ref NFS_MOUNTS: Mutex<BTreeMap<String, NfsMount>> = Mutex::new(BTreeMap::new());
}

/// Mount an NFS export
pub fn nfs_mount(server: [u8; 4], export_path: &str, mount_point: &str) -> Result<(), NfsStat> {
    let options = NfsMountOptions::default(server, export_path, mount_point);
    let mut client = NfsClient::new(options);
    client.connect()?;

    let mount = NfsMount {
        mount_point: String::from(mount_point),
        server: alloc::format!("{}.{}.{}.{}", server[0], server[1], server[2], server[3]),
        export_path: String::from(export_path),
        client,
    };

    NFS_MOUNTS.lock().insert(String::from(mount_point), mount);
    serial_println!(
        "[NFS] Mounted {}:{} on {}",
        alloc::format!("{}.{}.{}.{}", server[0], server[1], server[2], server[3]),
        export_path,
        mount_point
    );
    Ok(())
}

/// Unmount an NFS filesystem
pub fn nfs_umount(mount_point: &str) -> Result<(), NfsStat> {
    let mut mounts = NFS_MOUNTS.lock();
    if let Some(mut mount) = mounts.remove(mount_point) {
        mount.client.disconnect();
        serial_println!("[NFS] Unmounted {}", mount_point);
        Ok(())
    } else {
        Err(NfsStat::Noent)
    }
}

/// List NFS mounts
pub fn list_nfs_mounts() -> Vec<(String, String, String)> {
    let mounts = NFS_MOUNTS.lock();
    mounts
        .values()
        .map(|m| {
            (
                m.mount_point.clone(),
                m.server.clone(),
                m.export_path.clone(),
            )
        })
        .collect()
}

/// Proc info
pub fn proc_nfs_info() -> String {
    let mounts = NFS_MOUNTS.lock();
    let mut info = String::from("NFS Mounts:\n");
    for (mp, mount) in mounts.iter() {
        info.push_str(&alloc::format!(
            "  {} -> {}:{}\n",
            mp,
            mount.server,
            mount.export_path
        ));
    }
    if mounts.is_empty() {
        info.push_str("  (none)\n");
    }
    info
}
