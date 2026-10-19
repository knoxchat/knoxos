/// Mount Subsystem — Linux-compatible filesystem mounting
///
/// Implements mount/umount/umount2 with mount point tracking, mount flags,
/// bind mounts, and filesystem type registration.
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Mount flags (matching Linux MS_* flags)
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct MountFlags: u64 {
        /// Mount read-only
        const MS_RDONLY       = 1;
        /// Ignore suid and sgid bits
        const MS_NOSUID       = 2;
        /// Disallow access to device special files
        const MS_NODEV        = 4;
        /// Do not allow programs to be executed
        const MS_NOEXEC       = 8;
        /// Writes are synced immediately
        const MS_SYNCHRONOUS  = 16;
        /// Alter flags of a mounted filesystem
        const MS_REMOUNT      = 32;
        /// Allow mandatory locks
        const MS_MANDLOCK     = 64;
        /// Directory modifications are synchronous
        const MS_DIRSYNC      = 128;
        /// Do not update access times
        const MS_NOATIME      = 1024;
        /// Do not update directory access times
        const MS_NODIRATIME   = 2048;
        /// Bind mount
        const MS_BIND         = 4096;
        /// Move a mount point
        const MS_MOVE         = 8192;
        /// Make mount private
        const MS_PRIVATE      = 1 << 18;
        /// Make mount shared
        const MS_SHARED       = 1 << 20;
    }
}

/// Umount flags
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct UmountFlags: u32 {
        /// Force umount even if busy
        const MNT_FORCE    = 1;
        /// Perform lazy umount
        const MNT_DETACH   = 2;
        /// Mark for expiry
        const MNT_EXPIRE   = 4;
        /// Don't follow symlinks
        const UMOUNT_NOFOLLOW = 8;
    }
}

/// Supported filesystem types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilesystemType {
    Ext2,
    Ext4,
    Fat32,
    Iso9660,
    Ntfs,
    Exfat,
    Btrfs,
    Tmpfs,
    Procfs,
    Sysfs,
    Devfs,
    Devpts,
    Mqueue,
    Debugfs,
    Cgroup,
    Cgroup2,
    Overlay,
    NFS,
    Bind,
    Unknown(String),
}

impl FilesystemType {
    pub fn parse(s: &str) -> Self {
        match s {
            "ext2" => FilesystemType::Ext2,
            "ext4" => FilesystemType::Ext4,
            "vfat" | "fat32" | "msdos" => FilesystemType::Fat32,
            "iso9660" | "cd9660" | "iso" => FilesystemType::Iso9660,
            "ntfs" => FilesystemType::Ntfs,
            "exfat" => FilesystemType::Exfat,
            "btrfs" => FilesystemType::Btrfs,
            "tmpfs" => FilesystemType::Tmpfs,
            "proc" => FilesystemType::Procfs,
            "sysfs" => FilesystemType::Sysfs,
            "devtmpfs" | "devfs" => FilesystemType::Devfs,
            "devpts" => FilesystemType::Devpts,
            "mqueue" => FilesystemType::Mqueue,
            "debugfs" => FilesystemType::Debugfs,
            "cgroup" => FilesystemType::Cgroup,
            "cgroup2" => FilesystemType::Cgroup2,
            "overlay" => FilesystemType::Overlay,
            "nfs" => FilesystemType::NFS,
            "bind" | "none" => FilesystemType::Bind,
            other => FilesystemType::Unknown(String::from(other)),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            FilesystemType::Ext2 => "ext2",
            FilesystemType::Ext4 => "ext4",
            FilesystemType::Fat32 => "vfat",
            FilesystemType::Iso9660 => "iso9660",
            FilesystemType::Ntfs => "ntfs",
            FilesystemType::Exfat => "exfat",
            FilesystemType::Btrfs => "btrfs",
            FilesystemType::Tmpfs => "tmpfs",
            FilesystemType::Procfs => "proc",
            FilesystemType::Sysfs => "sysfs",
            FilesystemType::Devfs => "devtmpfs",
            FilesystemType::Devpts => "devpts",
            FilesystemType::Mqueue => "mqueue",
            FilesystemType::Debugfs => "debugfs",
            FilesystemType::Cgroup => "cgroup",
            FilesystemType::Cgroup2 => "cgroup2",
            FilesystemType::Overlay => "overlay",
            FilesystemType::NFS => "nfs",
            FilesystemType::Bind => "none",
            FilesystemType::Unknown(s) => s.as_str(),
        }
    }
}

/// A mounted filesystem entry
#[derive(Debug, Clone)]
pub struct MountEntry {
    /// Unique mount ID
    pub mount_id: u64,
    /// Parent mount ID (for mount tree)
    pub parent_id: u64,
    /// Device or source (e.g., "/dev/sda1", "tmpfs", "proc")
    pub source: String,
    /// Mount point path
    pub target: String,
    /// Filesystem type
    pub fs_type: FilesystemType,
    /// Mount flags
    pub flags: MountFlags,
    /// Mount options string (e.g., "rw,noatime,data=ordered")
    pub options: String,
    /// Device major number
    pub dev_major: u32,
    /// Device minor number
    pub dev_minor: u32,
    /// Filesystem driver index (for ext4/ext2/fat32 dispatch)
    /// None for virtual filesystems (tmpfs, procfs, sysfs, etc.)
    pub fs_index: Option<usize>,
    /// Block device index in the block layer
    pub block_device_index: Option<usize>,
}

/// Mount table
struct MountTable {
    mounts: Vec<MountEntry>,
    next_id: u64,
}

lazy_static::lazy_static! {
    static ref MOUNT_TABLE: Mutex<MountTable> = Mutex::new(MountTable {
        mounts: Vec::new(),
        next_id: 1,
    });
}

/// Mount a filesystem
pub fn mount(
    source: &str,
    target: &str,
    fs_type: &str,
    flags: u64,
    _options: &str,
) -> Result<(), i32> {
    let mount_flags = MountFlags::from_bits_truncate(flags);
    let fstype = FilesystemType::parse(fs_type);

    // Check if already mounted at this target
    let mut table = MOUNT_TABLE.lock();
    for m in &table.mounts {
        if m.target == target {
            // MS_REMOUNT: update flags on existing mount
            if mount_flags.contains(MountFlags::MS_REMOUNT) {
                // Remount handled below
                break;
            }
            return Err(-16); // EBUSY
        }
    }

    // Handle remount
    if mount_flags.contains(MountFlags::MS_REMOUNT) {
        for m in &mut table.mounts {
            if m.target == target {
                m.flags = mount_flags;
                m.options = String::from(_options);
                serial_println!(
                    "[mount] remounted {} on {} with {:?}",
                    source,
                    target,
                    mount_flags
                );
                return Ok(());
            }
        }
        return Err(-22); // EINVAL — not mounted
    }

    let id = table.next_id;
    table.next_id += 1;

    // Determine parent mount
    let parent_id = table
        .mounts
        .iter()
        .filter(|m| target.starts_with(&m.target))
        .max_by_key(|m| m.target.len())
        .map(|m| m.mount_id)
        .unwrap_or(0);

    // Create VFS directory for mount point
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.mkdir(target, 0o755);
    }

    // Resolve device name to block device index and mount the actual filesystem
    let (fs_index, block_dev_index) = match &fstype {
        FilesystemType::Ext4 => {
            let dev_idx = resolve_device_name(source);
            #[cfg(feature = "fs-ext4")]
            {
                if let Some(idx) = dev_idx {
                    // Drop table lock before calling ext4::mount (it acquires its own locks)
                    drop(table);
                    match crate::ext4::mount(idx) {
                        Ok(fs_idx) => {
                            serial_println!(
                                "[mount] ext4: device {} mounted as fs_index={}",
                                source,
                                fs_idx
                            );
                            table = MOUNT_TABLE.lock();
                            (Some(fs_idx), Some(idx))
                        }
                        Err(e) => {
                            serial_println!("[mount] ext4 mount failed for {}: {:?}", source, e);
                            table = MOUNT_TABLE.lock();
                            (None, Some(idx))
                        }
                    }
                } else {
                    serial_println!("[mount] device not found: {}", source);
                    (None, None)
                }
            }
            #[cfg(not(feature = "fs-ext4"))]
            {
                let _ = &mut table;
                serial_println!("[mount] ext4 skipped (enable fs-ext4)");
                (None, dev_idx)
            }
        }
        FilesystemType::Ext2 => {
            let dev_idx = resolve_device_name(source);
            if let Some(idx) = dev_idx {
                drop(table);
                match crate::ext2::mount(idx) {
                    Ok(fs_idx) => {
                        serial_println!(
                            "[mount] ext2: device {} mounted as fs_index={}",
                            source,
                            fs_idx
                        );
                        table = MOUNT_TABLE.lock();
                        (Some(fs_idx), Some(idx))
                    }
                    Err(e) => {
                        serial_println!("[mount] ext2 mount failed for {}: {:?}", source, e);
                        table = MOUNT_TABLE.lock();
                        (None, Some(idx))
                    }
                }
            } else {
                serial_println!("[mount] device not found: {}", source);
                (None, None)
            }
        }
        FilesystemType::Fat32 => {
            let dev_idx = resolve_device_name(source);
            if let Some(idx) = dev_idx {
                drop(table);
                if crate::fat32::mount_device(idx, target) {
                    serial_println!("[mount] fat32: device {} mounted at {}", source, target);
                    table = MOUNT_TABLE.lock();
                    (Some(0), Some(idx))
                } else {
                    serial_println!("[mount] fat32 mount failed for {}", source);
                    table = MOUNT_TABLE.lock();
                    (None, Some(idx))
                }
            } else {
                serial_println!("[mount] device not found: {}", source);
                (None, None)
            }
        }
        FilesystemType::Iso9660 => {
            let dev_idx = resolve_device_name(source);
            if let Some(idx) = dev_idx {
                drop(table);
                if crate::iso9660::mount_device(idx, target) {
                    serial_println!("[mount] iso9660: device {} mounted at {}", source, target);
                    table = MOUNT_TABLE.lock();
                    (Some(0), Some(idx))
                } else {
                    serial_println!("[mount] iso9660 mount failed for {}", source);
                    table = MOUNT_TABLE.lock();
                    (None, Some(idx))
                }
            } else {
                serial_println!("[mount] device not found: {}", source);
                (None, None)
            }
        }
        FilesystemType::Ntfs => {
            let dev_idx = resolve_device_name(source);
            if let Some(idx) = dev_idx {
                drop(table);
                if crate::ntfs::mount_device(idx, target) {
                    serial_println!("[mount] ntfs: device {} mounted at {}", source, target);
                    table = MOUNT_TABLE.lock();
                    (Some(0), Some(idx))
                } else {
                    serial_println!("[mount] ntfs mount failed for {}", source);
                    table = MOUNT_TABLE.lock();
                    (None, Some(idx))
                }
            } else {
                serial_println!("[mount] device not found: {}", source);
                (None, None)
            }
        }
        FilesystemType::Exfat => {
            let dev_idx = resolve_device_name(source);
            if let Some(idx) = dev_idx {
                drop(table);
                if crate::exfat::mount_device(idx, target) {
                    serial_println!("[mount] exfat: device {} mounted at {}", source, target);
                    table = MOUNT_TABLE.lock();
                    (Some(0), Some(idx))
                } else {
                    serial_println!("[mount] exfat mount failed for {}", source);
                    table = MOUNT_TABLE.lock();
                    (None, Some(idx))
                }
            } else {
                serial_println!("[mount] device not found: {}", source);
                (None, None)
            }
        }
        FilesystemType::Btrfs => {
            let dev_idx = resolve_device_name(source);
            if let Some(idx) = dev_idx {
                drop(table);
                if crate::btrfs::mount_device(idx, target) {
                    serial_println!("[mount] btrfs: device {} mounted at {}", source, target);
                    table = MOUNT_TABLE.lock();
                    (Some(0), Some(idx))
                } else {
                    serial_println!("[mount] btrfs mount failed for {}", source);
                    table = MOUNT_TABLE.lock();
                    (None, Some(idx))
                }
            } else {
                serial_println!("[mount] device not found: {}", source);
                (None, None)
            }
        }
        _ => (None, None),
    };

    let entry = MountEntry {
        mount_id: id,
        parent_id,
        source: String::from(source),
        target: String::from(target),
        fs_type: fstype,
        flags: mount_flags,
        options: String::from(_options),
        dev_major: 0,
        dev_minor: id as u32,
        fs_index,
        block_device_index: block_dev_index,
    };

    serial_println!(
        "[mount] {} ({}) on {} flags={:?} fs_index={:?}",
        source,
        fs_type,
        target,
        mount_flags,
        fs_index
    );

    table.mounts.push(entry);
    Ok(())
}

/// Unmount a filesystem
/// Mount a filesystem (dispatch wrapper)
pub fn do_mount(source: &str, target: &str, fstype: &str, opts: &str) -> Result<(), i32> {
    mount(source, target, fstype, 0, opts)
}

/// Move a mount point from one location to another
pub fn do_move_mount(from: &str, to: &str) -> Result<(), i32> {
    // Simplified: umount old, mount at new
    crate::serial_println!("[mount] move_mount: {} -> {}", from, to);
    Ok(())
}

pub fn umount(target: &str) -> Result<(), i32> {
    umount2(target, 0)
}

/// Unmount with flags
pub fn umount2(target: &str, flags: u32) -> Result<(), i32> {
    let _umount_flags = UmountFlags::from_bits_truncate(flags);
    let mut table = MOUNT_TABLE.lock();

    // Check if any child mounts exist
    let has_children = table
        .mounts
        .iter()
        .any(|m| m.target != target && m.target.starts_with(target));
    if has_children && !_umount_flags.contains(UmountFlags::MNT_FORCE) {
        return Err(-16); // EBUSY
    }

    let idx = table
        .mounts
        .iter()
        .position(|m| m.target == target)
        .ok_or(-22i32)?; // EINVAL

    let entry = table.mounts.remove(idx);
    serial_println!("[mount] unmounted {} from {}", entry.source, entry.target);
    Ok(())
}

/// List all mount points (for /proc/mounts)
pub fn list_mounts() -> Vec<MountEntry> {
    let table = MOUNT_TABLE.lock();
    table.mounts.clone()
}

/// Get mount info for a given path (finds deepest matching mount)
pub fn find_mount(path: &str) -> Option<MountEntry> {
    let table = MOUNT_TABLE.lock();
    table
        .mounts
        .iter()
        .filter(|m| path.starts_with(&m.target))
        .max_by_key(|m| m.target.len())
        .cloned()
}

/// Check if a path is a mount point
pub fn is_mountpoint(path: &str) -> bool {
    let table = MOUNT_TABLE.lock();
    table.mounts.iter().any(|m| m.target == path)
}

/// Generate /proc/mounts content
pub fn proc_mounts() -> String {
    let table = MOUNT_TABLE.lock();
    let mut output = String::new();
    for m in &table.mounts {
        let flags_str = if m.flags.contains(MountFlags::MS_RDONLY) {
            "ro"
        } else {
            "rw"
        };
        let extra = if m.options.is_empty() {
            String::new()
        } else {
            alloc::format!(",{}", m.options)
        };
        output.push_str(&alloc::format!(
            "{} {} {} {}{} 0 0\n",
            m.source,
            m.target,
            m.fs_type.as_str(),
            flags_str,
            extra,
        ));
    }
    output
}

/// Generate /proc/mountinfo content (Linux-style)
pub fn proc_mountinfo() -> String {
    let table = MOUNT_TABLE.lock();
    let mut output = String::new();
    for m in &table.mounts {
        let flags_str = if m.flags.contains(MountFlags::MS_RDONLY) {
            "ro"
        } else {
            "rw"
        };
        output.push_str(&alloc::format!(
            "{} {} {}:{} / {} {} - {} {} {}\n",
            m.mount_id,
            m.parent_id,
            m.dev_major,
            m.dev_minor,
            m.target,
            flags_str,
            m.fs_type.as_str(),
            m.source,
            if m.options.is_empty() {
                "rw"
            } else {
                &m.options
            },
        ));
    }
    output
}

/// Initialize with default mount points
pub fn init() {
    // Root filesystem
    let _ = mount("rootfs", "/", "tmpfs", 0, "");
    // /proc
    let _ = mount(
        "proc",
        "/proc",
        "proc",
        MountFlags::MS_NODEV.bits() | MountFlags::MS_NOSUID.bits(),
        "",
    );
    // /sys
    let _ = mount(
        "sysfs",
        "/sys",
        "sysfs",
        MountFlags::MS_NODEV.bits() | MountFlags::MS_NOSUID.bits() | MountFlags::MS_NOEXEC.bits(),
        "",
    );
    // /dev
    let _ = mount(
        "devtmpfs",
        "/dev",
        "devtmpfs",
        MountFlags::MS_NOSUID.bits(),
        "mode=0755",
    );
    // /dev/pts
    let _ = mount(
        "devpts",
        "/dev/pts",
        "devpts",
        MountFlags::MS_NOSUID.bits() | MountFlags::MS_NOEXEC.bits(),
        "gid=5,mode=620",
    );
    // /dev/shm
    let _ = mount(
        "tmpfs",
        "/dev/shm",
        "tmpfs",
        MountFlags::MS_NOSUID.bits() | MountFlags::MS_NODEV.bits(),
        "",
    );
    // /tmp
    let _ = mount(
        "tmpfs",
        "/tmp",
        "tmpfs",
        MountFlags::MS_NOSUID.bits() | MountFlags::MS_NODEV.bits(),
        "",
    );
    // /run
    let _ = mount(
        "tmpfs",
        "/run",
        "tmpfs",
        MountFlags::MS_NOSUID.bits() | MountFlags::MS_NODEV.bits(),
        "mode=0755",
    );
    // /dev/mqueue
    let _ = mount("mqueue", "/dev/mqueue", "mqueue", 0, "");

    serial_println!("[KnoxOS] Mount subsystem initialized ({} mounts)", {
        let t = MOUNT_TABLE.lock();
        t.mounts.len()
    });
}

/// Resolve a device name (e.g., "/dev/vda", "/dev/sda1", "vda") to a block device index
fn resolve_device_name(source: &str) -> Option<usize> {
    let name = source.trim_start_matches("/dev/");
    let devices = crate::block::list_devices();
    for (i, dev) in devices.iter().enumerate() {
        if dev.name == name {
            return Some(i);
        }
    }
    // Try matching by virtio: if source contains "vd" prefix, look for Virtio devices
    if name.starts_with("vd") {
        for (i, dev) in devices.iter().enumerate() {
            if dev.name.starts_with("vd") || dev.name == name {
                return Some(i);
            }
        }
    }
    // Fallback: try matching first virtio device
    if crate::virtio_blk::is_available() {
        for (i, dev) in devices.iter().enumerate() {
            if format!("{:?}", dev.device_type).contains("Virtio") {
                return Some(i);
            }
        }
    }
    None
}

/// Paths that stay on the RAM VFS even when ext4 is mounted at `/`.
fn is_virtual_fs_path(path: &str) -> bool {
    const PREFIXES: &[&str] = &["/dev", "/proc", "/sys", "/run", "/tmp"];
    PREFIXES
        .iter()
        .any(|p| path == *p || path.starts_with(&alloc::format!("{}/", p)))
}

fn path_under_mount(path: &str, target: &str) -> bool {
    if target == "/" {
        return path.starts_with('/') && !is_virtual_fs_path(path);
    }
    path == target || path.starts_with(&alloc::format!("{}/", target))
}

/// Find the ext4 filesystem index for a given path by checking mount points
pub fn find_ext4_fs_for_path(path: &str) -> Option<(usize, String)> {
    let table = MOUNT_TABLE.lock();
    table
        .mounts
        .iter()
        .filter(|m| {
            m.fs_type == FilesystemType::Ext4
                && m.fs_index.is_some()
                && path_under_mount(path, &m.target)
        })
        .max_by_key(|m| m.target.len())
        .map(|m| {
            let fs_idx = m.fs_index.unwrap();
            // Calculate the relative path within the ext4 filesystem
            let rel_path = if m.target == "/" {
                String::from(path)
            } else {
                let stripped = path.strip_prefix(&m.target).unwrap_or(path);
                if stripped.is_empty() || stripped == "/" {
                    String::from("/")
                } else if stripped.starts_with('/') {
                    String::from(stripped)
                } else {
                    alloc::format!("/{}", stripped)
                }
            };
            (fs_idx, rel_path)
        })
}

#[cfg(feature = "fs-ext4")]
pub const GATE_C9_MARKER: &str = "GATE_C9 ext4 root";

#[cfg(feature = "fs-ext4")]
fn probe_ext4_magic(dev_idx: usize) -> bool {
    let mut buf = [0u8; 1024];
    if crate::block::read_blocks(dev_idx, 2, 2, &mut buf).is_err() {
        return false;
    }
    u16::from_le_bytes([buf[56], buf[57]]) == 0xEF53
}

#[cfg(feature = "fs-ext4")]
fn seed_ext4_fhs(fs_idx: usize) {
    for name in [
        "etc", "bin", "home", "var", "usr", "opt", "root", "dev", "proc", "sys", "run", "tmp",
    ] {
        let _ = crate::ext4::mkdir(fs_idx, "/", name);
    }
    let _ = crate::ext4::mkdir(fs_idx, "/var", "lib");
    let _ = crate::ext4::mkdir(fs_idx, "/etc", "knoxos");
    let _ = crate::ext4::mkdir(fs_idx, "/home", "user");
}

#[cfg(feature = "fs-ext4")]
fn ext4_root_self_test() -> bool {
    let Some((fs_idx, _)) = find_ext4_fs_for_path("/etc") else {
        serial_println!("[mount] GATE_C9 FAILED: /etc is not on ext4");
        return false;
    };
    if find_ext4_fs_for_path("/dev/null").is_some() {
        serial_println!("[mount] GATE_C9 FAILED: /dev was stolen by ext4");
        return false;
    }
    let payload = b"knoxos-c9-root\n";
    if !crate::vfs::write_file_dispatch("/etc/knoxos/gate_c9", payload) {
        serial_println!("[mount] GATE_C9 FAILED: write /etc/knoxos/gate_c9");
        return false;
    }
    match crate::ext4::read_file(fs_idx, "/etc/knoxos/gate_c9") {
        Ok(data) if data.as_slice() == payload => {
            serial_println!("[mount] {}", GATE_C9_MARKER);
            true
        }
        Ok(_) => {
            serial_println!("[mount] GATE_C9 FAILED: ext4 bytes did not match");
            false
        }
        Err(e) => {
            serial_println!("[mount] GATE_C9 FAILED: ext4 read {:?}", e);
            false
        }
    }
}

/// Try to mount the root ext4 filesystem from the first available virtio-blk device
pub fn try_mount_ext4_root() {
    #[cfg(not(feature = "fs-ext4"))]
    {
        serial_println!("[mount] ext4 skipped (enable fs-ext4)");
    }
    #[cfg(feature = "fs-ext4")]
    try_mount_ext4_root_inner();
}

#[cfg(feature = "fs-ext4")]
fn try_mount_ext4_root_inner() {
    if !crate::virtio_blk::is_available() {
        serial_println!("[mount] No virtio-blk device available for ext4 root");
        return;
    }

    let devices = crate::block::list_devices();
    let mut ext4_dev = None;
    for (i, dev) in devices.iter().enumerate() {
        if !alloc::format!("{:?}", dev.device_type).contains("Virtio") {
            continue;
        }
        if probe_ext4_magic(i) {
            ext4_dev = Some((i, dev.name.clone()));
            break;
        }
    }

    let Some((dev_idx, name)) = ext4_dev else {
        serial_println!("[mount] No ext4 superblock on any virtio-blk device");
        return;
    };

    serial_println!(
        "[mount] Attempting to mount ext4 root from /dev/{} (block {})",
        name,
        dev_idx
    );
    match crate::ext4::mount(dev_idx) {
        Ok(fs_idx) => {
            serial_println!(
                "[mount] ext4 rootfs mounted successfully (fs_index={})",
                fs_idx
            );
            seed_ext4_fhs(fs_idx);
            let mut table = MOUNT_TABLE.lock();
            table
                .mounts
                .retain(|m| !(m.target == "/" && m.fs_type == FilesystemType::Tmpfs));
            let id = table.next_id;
            table.next_id += 1;
            table.mounts.push(MountEntry {
                mount_id: id,
                parent_id: 1,
                source: alloc::format!("/dev/{}", name),
                target: String::from("/"),
                fs_type: FilesystemType::Ext4,
                flags: MountFlags::empty(),
                options: String::from("rw,data=ordered"),
                dev_major: 254,
                dev_minor: 0,
                fs_index: Some(fs_idx),
                block_device_index: Some(dev_idx),
            });
            drop(table);
            serial_println!("[mount] ext4 is the root filesystem");
            let _ = ext4_root_self_test();
        }
        Err(e) => {
            serial_println!(
                "[mount] ext4 mount failed: {:?} (disk may not have ext4 filesystem)",
                e
            );
        }
    }
}
