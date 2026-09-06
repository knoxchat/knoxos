/// tmpfs - In-Memory Temporary Filesystem
/// Compatible with Linux tmpfs mount
/// Provides a size-limited RAM-backed filesystem
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Default tmpfs size limit (16 MiB)
pub const DEFAULT_SIZE_LIMIT: usize = 16 * 1024 * 1024;

/// tmpfs inode types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TmpfsNodeType {
    File,
    Directory,
    Symlink,
    Device,
}

/// A tmpfs inode
#[derive(Debug, Clone)]
pub struct TmpfsNode {
    pub ino: u64,
    pub node_type: TmpfsNodeType,
    pub name: String,
    pub data: Vec<u8>,
    pub children: Vec<u64>,
    pub permissions: u16,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    pub symlink_target: Option<String>,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
}

/// A tmpfs filesystem instance
pub struct TmpfsInstance {
    pub name: String,
    pub mount_point: String,
    pub nodes: BTreeMap<u64, TmpfsNode>,
    pub next_ino: u64,
    pub size_limit: usize,
    pub used_bytes: usize,
}

impl TmpfsInstance {
    pub fn new(name: &str, mount_point: &str, size_limit: usize) -> Self {
        let mut fs = Self {
            name: String::from(name),
            mount_point: String::from(mount_point),
            nodes: BTreeMap::new(),
            next_ino: 2, // 1 is reserved for root
            size_limit,
            used_bytes: 0,
        };

        // Create root directory
        let root = TmpfsNode {
            ino: 1,
            node_type: TmpfsNodeType::Directory,
            name: String::from(""),
            data: Vec::new(),
            children: Vec::new(),
            permissions: 0o1777,
            uid: 0,
            gid: 0,
            size: 0,
            symlink_target: None,
            atime: 0,
            mtime: 0,
            ctime: 0,
        };
        fs.nodes.insert(1, root);
        fs
    }

    fn alloc_ino(&mut self) -> u64 {
        let ino = self.next_ino;
        self.next_ino += 1;
        ino
    }

    /// Create a file in the tmpfs
    pub fn create_file(
        &mut self,
        parent_ino: u64,
        name: &str,
        permissions: u16,
    ) -> Result<u64, i32> {
        let ino = self.alloc_ino();
        let node = TmpfsNode {
            ino,
            node_type: TmpfsNodeType::File,
            name: String::from(name),
            data: Vec::new(),
            children: Vec::new(),
            permissions,
            uid: 1000,
            gid: 1000,
            size: 0,
            symlink_target: None,
            atime: crate::rtc::uptime_seconds(),
            mtime: crate::rtc::uptime_seconds(),
            ctime: crate::rtc::uptime_seconds(),
        };
        self.nodes.insert(ino, node);

        // Add to parent
        if let Some(parent) = self.nodes.get_mut(&parent_ino) {
            parent.children.push(ino);
        }

        Ok(ino)
    }

    /// Create a directory
    pub fn create_dir(
        &mut self,
        parent_ino: u64,
        name: &str,
        permissions: u16,
    ) -> Result<u64, i32> {
        let ino = self.alloc_ino();
        let node = TmpfsNode {
            ino,
            node_type: TmpfsNodeType::Directory,
            name: String::from(name),
            data: Vec::new(),
            children: Vec::new(),
            permissions,
            uid: 1000,
            gid: 1000,
            size: 0,
            symlink_target: None,
            atime: crate::rtc::uptime_seconds(),
            mtime: crate::rtc::uptime_seconds(),
            ctime: crate::rtc::uptime_seconds(),
        };
        self.nodes.insert(ino, node);

        if let Some(parent) = self.nodes.get_mut(&parent_ino) {
            parent.children.push(ino);
        }

        Ok(ino)
    }

    /// Create a symlink
    pub fn create_symlink(
        &mut self,
        parent_ino: u64,
        name: &str,
        target: &str,
    ) -> Result<u64, i32> {
        let ino = self.alloc_ino();
        let node = TmpfsNode {
            ino,
            node_type: TmpfsNodeType::Symlink,
            name: String::from(name),
            data: Vec::new(),
            children: Vec::new(),
            permissions: 0o777,
            uid: 1000,
            gid: 1000,
            size: target.len() as u64,
            symlink_target: Some(String::from(target)),
            atime: crate::rtc::uptime_seconds(),
            mtime: crate::rtc::uptime_seconds(),
            ctime: crate::rtc::uptime_seconds(),
        };
        self.nodes.insert(ino, node);

        if let Some(parent) = self.nodes.get_mut(&parent_ino) {
            parent.children.push(ino);
        }

        Ok(ino)
    }

    /// Write data to a file
    pub fn write_file(&mut self, ino: u64, data: &[u8]) -> Result<usize, i32> {
        let old_size = self.nodes.get(&ino).map(|n| n.data.len()).unwrap_or(0);
        let new_bytes = if data.len() > old_size {
            data.len() - old_size
        } else {
            0
        };

        if self.used_bytes + new_bytes > self.size_limit {
            return Err(-28); // ENOSPC
        }

        if let Some(node) = self.nodes.get_mut(&ino) {
            if node.node_type != TmpfsNodeType::File {
                return Err(-21); // EISDIR
            }
            self.used_bytes -= node.data.len();
            node.data = data.to_vec();
            node.size = data.len() as u64;
            node.mtime = crate::rtc::uptime_seconds();
            self.used_bytes += data.len();
            Ok(data.len())
        } else {
            Err(-2) // ENOENT
        }
    }

    /// Read data from a file
    pub fn read_file(&self, ino: u64) -> Result<&[u8], i32> {
        let node = self.nodes.get(&ino).ok_or(-2i32)?;
        if node.node_type != TmpfsNodeType::File {
            return Err(-21); // EISDIR
        }
        Ok(&node.data)
    }

    /// Remove a file
    pub fn unlink(&mut self, parent_ino: u64, name: &str) -> Result<(), i32> {
        let child_ino = self.find_child(parent_ino, name).ok_or(-2i32)?;
        let node = self.nodes.get(&child_ino).ok_or(-2i32)?;

        if node.node_type == TmpfsNodeType::Directory {
            return Err(-21); // EISDIR
        }

        let data_len = node.data.len();
        self.nodes.remove(&child_ino);
        self.used_bytes -= data_len;

        if let Some(parent) = self.nodes.get_mut(&parent_ino) {
            parent.children.retain(|&c| c != child_ino);
        }

        Ok(())
    }

    /// Remove an empty directory
    pub fn rmdir(&mut self, parent_ino: u64, name: &str) -> Result<(), i32> {
        let child_ino = self.find_child(parent_ino, name).ok_or(-2i32)?;
        let node = self.nodes.get(&child_ino).ok_or(-2i32)?;

        if node.node_type != TmpfsNodeType::Directory {
            return Err(-20); // ENOTDIR
        }
        if !node.children.is_empty() {
            return Err(-39); // ENOTEMPTY
        }

        self.nodes.remove(&child_ino);

        if let Some(parent) = self.nodes.get_mut(&parent_ino) {
            parent.children.retain(|&c| c != child_ino);
        }

        Ok(())
    }

    /// Find a child node by name
    pub fn find_child(&self, parent_ino: u64, name: &str) -> Option<u64> {
        let parent = self.nodes.get(&parent_ino)?;
        for &child_ino in &parent.children {
            if let Some(child) = self.nodes.get(&child_ino) {
                if child.name == name {
                    return Some(child_ino);
                }
            }
        }
        None
    }

    /// List directory entries
    pub fn list_dir(&self, ino: u64) -> Result<Vec<(String, u64, TmpfsNodeType)>, i32> {
        let node = self.nodes.get(&ino).ok_or(-2i32)?;
        if node.node_type != TmpfsNodeType::Directory {
            return Err(-20); // ENOTDIR
        }

        let mut entries = Vec::new();
        for &child_ino in &node.children {
            if let Some(child) = self.nodes.get(&child_ino) {
                entries.push((child.name.clone(), child_ino, child.node_type));
            }
        }
        Ok(entries)
    }

    /// Resolve a path relative to mount point
    pub fn resolve_path(&self, path: &str) -> Option<u64> {
        let relative = path.strip_prefix(&self.mount_point).unwrap_or(path);
        let parts: Vec<&str> = relative
            .trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        let mut current_ino = 1u64; // root
        for part in parts {
            current_ino = self.find_child(current_ino, part)?;
        }
        Some(current_ino)
    }

    /// Get filesystem statistics
    pub fn statfs(&self) -> TmpfsStats {
        TmpfsStats {
            total_bytes: self.size_limit,
            used_bytes: self.used_bytes,
            free_bytes: self.size_limit.saturating_sub(self.used_bytes),
            total_inodes: u64::MAX,
            used_inodes: self.nodes.len() as u64,
            block_size: 4096,
        }
    }
}

/// tmpfs filesystem statistics
#[derive(Debug, Clone)]
pub struct TmpfsStats {
    pub total_bytes: usize,
    pub used_bytes: usize,
    pub free_bytes: usize,
    pub total_inodes: u64,
    pub used_inodes: u64,
    pub block_size: usize,
}

/// Global tmpfs instances (mount_point -> instance)
lazy_static::lazy_static! {
    static ref TMPFS_MOUNTS: Mutex<BTreeMap<String, TmpfsInstance>> = Mutex::new(BTreeMap::new());
}

/// Mount a new tmpfs at the given mount point
pub fn mount(mount_point: &str, size_limit: usize) -> Result<(), i32> {
    let mut mounts = TMPFS_MOUNTS.lock();
    if mounts.contains_key(mount_point) {
        return Err(-16); // EBUSY
    }

    let name = mount_point.rsplit('/').next().unwrap_or("tmpfs");
    mounts.insert(
        String::from(mount_point),
        TmpfsInstance::new(name, mount_point, size_limit),
    );

    crate::serial_println!(
        "[KnoxOS] tmpfs mounted at {} ({}KB limit)",
        mount_point,
        size_limit / 1024
    );
    Ok(())
}

/// Unmount a tmpfs
pub fn umount(mount_point: &str) -> Result<(), i32> {
    let mut mounts = TMPFS_MOUNTS.lock();
    mounts.remove(mount_point).ok_or(-22i32)?; // EINVAL
    Ok(())
}

/// Check if a path is on a tmpfs mount
pub fn is_tmpfs_path(path: &str) -> bool {
    TMPFS_MOUNTS
        .lock()
        .keys()
        .any(|mp| path.starts_with(mp.as_str()))
}

/// Get the tmpfs instance for a path
pub fn get_instance_for_path(path: &str) -> Option<String> {
    TMPFS_MOUNTS
        .lock()
        .keys()
        .filter(|mp| path.starts_with(mp.as_str()))
        .max_by_key(|mp| mp.len())
        .cloned()
}

/// Initialize tmpfs with default mounts
pub fn init() {
    // Mount tmpfs at /tmp (standard location)
    let _ = mount("/tmp", DEFAULT_SIZE_LIMIT);
    // Mount tmpfs at /dev/shm (shared memory)
    let _ = mount("/dev/shm", 32 * 1024 * 1024);
    // Mount tmpfs at /run
    let _ = mount("/run", 8 * 1024 * 1024);

    crate::serial_println!("[KnoxOS] tmpfs initialized (3 mount points)");
}
