/// Virtual Filesystem - Linux-compatible VFS layer
/// Provides a basic in-memory filesystem with Linux-style paths
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// File types matching Linux
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Regular,
    Directory,
    SymLink,
    CharDevice,
    BlockDevice,
    Pipe,
    Socket,
}

/// Inode - file metadata
#[derive(Debug, Clone)]
pub struct Inode {
    pub ino: u64,
    pub file_type: FileType,
    pub name: String,
    pub size: u64,
    pub permissions: u16, // Unix permissions (e.g. 0o755)
    pub uid: u32,
    pub gid: u32,
    pub children: Vec<u64>, // Child inode numbers (for directories)
    pub data: Vec<u8>,      // File content (for regular files)
    /// Last access time (Unix timestamp, seconds since epoch)
    pub atime: i64,
    /// Last modification time (Unix timestamp, seconds since epoch)
    pub mtime: i64,
    /// Last status change time (Unix timestamp, seconds since epoch)
    pub ctime: i64,
}

/// Get the current Unix timestamp from RTC (or 0 if unavailable)
pub fn now_timestamp() -> i64 {
    crate::rtc::unix_time()
}

/// The virtual filesystem
pub struct VirtualFS {
    pub inodes: Vec<Inode>,
    next_ino: u64,
}

lazy_static::lazy_static! {
    pub static ref VFS: Mutex<VirtualFS> = Mutex::new(VirtualFS::new());
}

impl Default for VirtualFS {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtualFS {
    pub fn new() -> Self {
        let mut vfs = Self {
            inodes: Vec::new(),
            next_ino: 1,
        };

        // Create root directory tree matching Linux FHS
        let root = vfs.create_dir("/", 0o755);

        // Standard Linux directories
        let _bin = vfs.create_dir_under(root, "bin", 0o755);
        let _boot = vfs.create_dir_under(root, "boot", 0o755);
        let dev = vfs.create_dir_under(root, "dev", 0o755);
        let etc = vfs.create_dir_under(root, "etc", 0o755);
        let home = vfs.create_dir_under(root, "home", 0o755);
        let _lib = vfs.create_dir_under(root, "lib", 0o755);
        let _mnt = vfs.create_dir_under(root, "mnt", 0o755);
        let _proc = vfs.create_dir_under(root, "proc", 0o555);
        let _sbin = vfs.create_dir_under(root, "sbin", 0o755);
        let _sys = vfs.create_dir_under(root, "sys", 0o555);
        let tmp = vfs.create_dir_under(root, "tmp", 0o1777);
        let _usr = vfs.create_dir_under(root, "usr", 0o755);
        let _var = vfs.create_dir_under(root, "var", 0o755);

        // User home directory
        let user_home = vfs.create_dir_under(home, "user", 0o755);
        let _desktop = vfs.create_dir_under(user_home, "Desktop", 0o755);
        let _documents = vfs.create_dir_under(user_home, "Documents", 0o755);
        let _downloads = vfs.create_dir_under(user_home, "Downloads", 0o755);
        let _music = vfs.create_dir_under(user_home, "Music", 0o755);
        let _pictures = vfs.create_dir_under(user_home, "Pictures", 0o755);
        let _videos = vfs.create_dir_under(user_home, "Videos", 0o755);

        // Device files
        vfs.create_file_under(dev, "null", FileType::CharDevice, &[], 0o666);
        vfs.create_file_under(dev, "zero", FileType::CharDevice, &[], 0o666);
        vfs.create_file_under(dev, "random", FileType::CharDevice, &[], 0o666);
        vfs.create_file_under(dev, "urandom", FileType::CharDevice, &[], 0o666);
        vfs.create_file_under(dev, "tty", FileType::CharDevice, &[], 0o666);
        vfs.create_file_under(dev, "console", FileType::CharDevice, &[], 0o600);
        vfs.create_file_under(dev, "fb0", FileType::CharDevice, &[], 0o660);

        // /etc files
        vfs.create_file_under(etc, "hostname", FileType::Regular, b"knoxos\n", 0o644);
        vfs.create_file_under(etc, "os-release", FileType::Regular,
            b"NAME=\"KnoxOS\"\nVERSION=\"0.1.0\"\nID=knoxos\nPRETTY_NAME=\"KnoxOS - AI Operating System\"\n",
            0o644);
        vfs.create_file_under(
            etc,
            "passwd",
            FileType::Regular,
            b"root:x:0:0:root:/root:/bin/sh\nuser:x:1000:1000:KnoxOS User:/home/user:/bin/sh\n",
            0o644,
        );

        // Temp
        let _ = tmp;

        vfs
    }

    fn alloc_ino(&mut self) -> u64 {
        let ino = self.next_ino;
        self.next_ino += 1;
        ino
    }

    fn create_dir(&mut self, name: &str, permissions: u16) -> u64 {
        let ino = self.alloc_ino();
        let ts = now_timestamp();
        self.inodes.push(Inode {
            ino,
            file_type: FileType::Directory,
            name: String::from(name),
            size: 0,
            permissions,
            uid: 0,
            gid: 0,
            children: Vec::new(),
            data: Vec::new(),
            atime: ts,
            mtime: ts,
            ctime: ts,
        });
        ino
    }

    fn create_dir_under(&mut self, parent_ino: u64, name: &str, permissions: u16) -> u64 {
        let ino = self.create_dir(name, permissions);
        if let Some(parent) = self.inodes.iter_mut().find(|i| i.ino == parent_ino) {
            parent.children.push(ino);
        }
        ino
    }

    pub fn create_file_under(
        &mut self,
        parent_ino: u64,
        name: &str,
        file_type: FileType,
        data: &[u8],
        permissions: u16,
    ) -> u64 {
        let ino = self.alloc_ino();
        let ts = now_timestamp();
        self.inodes.push(Inode {
            ino,
            file_type,
            name: String::from(name),
            size: data.len() as u64,
            permissions,
            uid: 0,
            gid: 0,
            children: Vec::new(),
            data: Vec::from(data),
            atime: ts,
            mtime: ts,
            ctime: ts,
        });
        if let Some(parent) = self.inodes.iter_mut().find(|i| i.ino == parent_ino) {
            parent.children.push(ino);
            parent.mtime = ts;
        }
        ino
    }

    /// Resolve a path to an inode number
    pub fn resolve_path(&self, path: &str) -> Option<u64> {
        if path == "/" {
            return self.inodes.first().map(|i| i.ino);
        }

        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        let mut current_ino = self.inodes.first()?.ino;

        for part in parts {
            if part.is_empty() {
                continue;
            }
            let current = self.inodes.iter().find(|i| i.ino == current_ino)?;
            let mut found = false;
            for &child_ino in &current.children {
                if let Some(child) = self.inodes.iter().find(|i| i.ino == child_ino) {
                    if child.name == part {
                        current_ino = child_ino;
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                return None;
            }
        }

        Some(current_ino)
    }

    /// Get an inode by number
    pub fn get_inode(&self, ino: u64) -> Option<&Inode> {
        self.inodes.iter().find(|i| i.ino == ino)
    }

    /// Get a mutable inode by number
    pub fn get_inode_mut(&mut self, ino: u64) -> Option<&mut Inode> {
        self.inodes.iter_mut().find(|i| i.ino == ino)
    }

    /// List children of a directory
    pub fn list_dir(&self, path: &str) -> Option<Vec<String>> {
        let ino = self.resolve_path(path)?;
        let inode = self.get_inode(ino)?;
        if inode.file_type != FileType::Directory {
            return None;
        }

        let mut names = Vec::new();
        for &child_ino in &inode.children {
            if let Some(child) = self.get_inode(child_ino) {
                names.push(child.name.clone());
            }
        }
        Some(names)
    }

    /// Read file content
    pub fn read_file(&self, path: &str) -> Option<&[u8]> {
        let ino = self.resolve_path(path)?;
        let inode = self.get_inode(ino)?;
        if inode.file_type != FileType::Regular && inode.file_type != FileType::CharDevice {
            return None;
        }
        Some(&inode.data)
    }

    /// Write data to a file (creates if needed)
    pub fn write_file(&mut self, path: &str, data: &[u8]) -> bool {
        if let Some(ino) = self.resolve_path(path) {
            // File exists - update content
            if let Some(inode) = self.inodes.iter_mut().find(|i| i.ino == ino) {
                inode.data = Vec::from(data);
                inode.size = data.len() as u64;
                let ts = now_timestamp();
                inode.mtime = ts;
                inode.ctime = ts;
                return true;
            }
        }
        // File doesn't exist - create it
        self.create_file_at_path(path, FileType::Regular, data, 0o644)
    }

    /// Write a file but only store a small stub in memory.
    /// Reports `virtual_size` for stat/ls but only keeps the first
    /// `keep_bytes` of actual data (saves heap for huge binaries).
    pub fn write_file_sparse(
        &mut self,
        path: &str,
        data: &[u8],
        virtual_size: u64,
        keep_bytes: usize,
    ) -> bool {
        let stub: Vec<u8> = if data.len() <= keep_bytes {
            Vec::from(data)
        } else {
            Vec::from(&data[..keep_bytes])
        };
        if let Some(ino) = self.resolve_path(path) {
            if let Some(inode) = self.inodes.iter_mut().find(|i| i.ino == ino) {
                inode.data = stub;
                inode.size = virtual_size;
                return true;
            }
        }
        // Create new
        if self.create_file_at_path(path, FileType::Regular, &stub, 0o755) {
            // Fix up the size to virtual_size
            if let Some(ino) = self.resolve_path(path) {
                if let Some(inode) = self.inodes.iter_mut().find(|i| i.ino == ino) {
                    inode.size = virtual_size;
                }
            }
            true
        } else {
            false
        }
    }

    /// Create a file at the given path, creating parent directories as needed
    pub fn create_file_at_path(
        &mut self,
        path: &str,
        file_type: FileType,
        data: &[u8],
        permissions: u16,
    ) -> bool {
        let path = path.trim_start_matches('/');
        let parts: Vec<&str> = path.split('/').collect();
        if parts.is_empty() {
            return false;
        }

        let file_name = parts.last().unwrap();
        let dir_parts = &parts[..parts.len() - 1];

        // Navigate/create parent directories
        let mut current_ino = match self.inodes.first() {
            Some(i) => i.ino,
            None => return false,
        };

        for &part in dir_parts {
            if part.is_empty() {
                continue;
            }
            let found = {
                let current = self.inodes.iter().find(|i| i.ino == current_ino);
                if let Some(current) = current {
                    let mut found_ino = None;
                    for &child_ino in &current.children {
                        if let Some(child) = self.inodes.iter().find(|i| i.ino == child_ino) {
                            if child.name == part {
                                found_ino = Some(child_ino);
                                break;
                            }
                        }
                    }
                    found_ino
                } else {
                    None
                }
            };

            if let Some(ino) = found {
                current_ino = ino;
            } else {
                // Create the missing directory
                let new_ino = self.create_dir_under(current_ino, part, 0o755);
                current_ino = new_ino;
            }
        }

        // Create the file
        self.create_file_under(current_ino, file_name, file_type, data, permissions);
        true
    }

    /// Create a directory at the given path
    pub fn mkdir(&mut self, path: &str, permissions: u16) -> Result<u64, i32> {
        // Check if it already exists
        if self.resolve_path(path).is_some() {
            return Err(-17); // EEXIST
        }

        let path_trimmed = path.trim_start_matches('/');
        let parts: Vec<&str> = path_trimmed.split('/').collect();
        if parts.is_empty() {
            return Err(-22); // EINVAL
        }

        let dir_name = parts.last().unwrap();
        let parent_path = if parts.len() > 1 {
            let parent_parts = &parts[..parts.len() - 1];
            alloc::format!("/{}", parent_parts.join("/"))
        } else {
            String::from("/")
        };

        let parent_ino = self.resolve_path(&parent_path).ok_or(-2i32)?; // ENOENT
        let ino = self.create_dir_under(parent_ino, dir_name, permissions);
        Ok(ino)
    }

    /// Remove a file
    pub fn unlink(&mut self, path: &str) -> Result<(), i32> {
        let ino = self.resolve_path(path).ok_or(-2i32)?; // ENOENT

        // Check it's not a directory
        if let Some(inode) = self.get_inode(ino) {
            if inode.file_type == FileType::Directory {
                return Err(-21); // EISDIR
            }
        }

        // Remove from parent's children
        let path_trimmed = path.trim_start_matches('/');
        let parts: Vec<&str> = path_trimmed.split('/').collect();
        let parent_path = if parts.len() > 1 {
            let parent_parts = &parts[..parts.len() - 1];
            alloc::format!("/{}", parent_parts.join("/"))
        } else {
            String::from("/")
        };

        if let Some(parent_ino) = self.resolve_path(&parent_path) {
            if let Some(parent) = self.inodes.iter_mut().find(|i| i.ino == parent_ino) {
                parent.children.retain(|&c| c != ino);
            }
        }

        // Remove the inode
        self.inodes.retain(|i| i.ino != ino);
        Ok(())
    }

    /// Remove an empty directory
    pub fn rmdir(&mut self, path: &str) -> Result<(), i32> {
        let ino = self.resolve_path(path).ok_or(-2i32)?; // ENOENT

        // Check it's a directory and empty
        if let Some(inode) = self.get_inode(ino) {
            if inode.file_type != FileType::Directory {
                return Err(-20); // ENOTDIR
            }
            if !inode.children.is_empty() {
                return Err(-39); // ENOTEMPTY
            }
        }

        // Remove from parent
        let path_trimmed = path.trim_start_matches('/');
        let parts: Vec<&str> = path_trimmed.split('/').collect();
        let parent_path = if parts.len() > 1 {
            let parent_parts = &parts[..parts.len() - 1];
            alloc::format!("/{}", parent_parts.join("/"))
        } else {
            String::from("/")
        };

        if let Some(parent_ino) = self.resolve_path(&parent_path) {
            if let Some(parent) = self.inodes.iter_mut().find(|i| i.ino == parent_ino) {
                parent.children.retain(|&c| c != ino);
            }
        }

        self.inodes.retain(|i| i.ino != ino);
        Ok(())
    }

    /// Rename/move a file or directory
    pub fn rename(&mut self, old_path: &str, new_path: &str) -> Result<(), i32> {
        let ino = self.resolve_path(old_path).ok_or(-2i32)?; // ENOENT

        // Remove from old parent
        let old_trimmed = old_path.trim_start_matches('/');
        let old_parts: Vec<&str> = old_trimmed.split('/').collect();
        let old_parent_path = if old_parts.len() > 1 {
            alloc::format!("/{}", old_parts[..old_parts.len() - 1].join("/"))
        } else {
            String::from("/")
        };

        if let Some(parent_ino) = self.resolve_path(&old_parent_path) {
            if let Some(parent) = self.inodes.iter_mut().find(|i| i.ino == parent_ino) {
                parent.children.retain(|&c| c != ino);
            }
        }

        // Update name
        let new_trimmed = new_path.trim_start_matches('/');
        let new_parts: Vec<&str> = new_trimmed.split('/').collect();
        let new_name = new_parts.last().ok_or(-22i32)?;
        let new_parent_path = if new_parts.len() > 1 {
            alloc::format!("/{}", new_parts[..new_parts.len() - 1].join("/"))
        } else {
            String::from("/")
        };

        if let Some(inode) = self.inodes.iter_mut().find(|i| i.ino == ino) {
            inode.name = String::from(*new_name);
        }

        // Add to new parent
        let new_parent_ino = self.resolve_path(&new_parent_path).ok_or(-2i32)?;
        if let Some(parent) = self.inodes.iter_mut().find(|i| i.ino == new_parent_ino) {
            parent.children.push(ino);
        }

        Ok(())
    }

    /// Check file permissions for a specific user
    /// Returns true if access is allowed.
    ///
    /// perm_bits: bitmask of requested access
    ///   1 = execute, 2 = write, 4 = read (matching Linux R_OK/W_OK/X_OK)
    pub fn check_permission(&self, ino: u64, uid: u32, gid: u32, perm_bits: u32) -> bool {
        // Root can do anything
        if uid == 0 {
            return true;
        }

        let inode = match self.get_inode(ino) {
            Some(i) => i,
            None => return false,
        };

        let mode = inode.permissions as u32;

        // Determine which permission triplet to check (owner / group / other)
        let shift = if uid == inode.uid {
            6 // owner bits: rwx at bits 8-6
        } else if gid == inode.gid {
            3 // group bits: rwx at bits 5-3
        } else {
            0 // other bits: rwx at bits 2-0
        };

        let allowed = (mode >> shift) & 0o7;

        // Check each requested permission
        if perm_bits & 4 != 0 && allowed & 4 == 0 {
            return false;
        } // read
        if perm_bits & 2 != 0 && allowed & 2 == 0 {
            return false;
        } // write
        if perm_bits & 1 != 0 && allowed & 1 == 0 {
            return false;
        } // execute

        true
    }

    /// Check if a path exists and matches access mode
    /// mode: 0 = F_OK (existence), 1 = X_OK, 2 = W_OK, 4 = R_OK
    pub fn access(&self, path: &str, mode: u32) -> Result<(), i32> {
        let ino = self.resolve_path(path).ok_or(-2i32)?; // ENOENT
        if mode == 0 {
            return Ok(()); // F_OK: just check existence
        }
        // Get current process uid/gid (default to root for now)
        let (uid, gid) = Self::current_credentials();
        if self.check_permission(ino, uid, gid, mode) {
            Ok(())
        } else {
            Err(-13) // EACCES
        }
    }

    /// Get current process credentials (uid, gid)
    fn current_credentials() -> (u32, u32) {
        // Try to get from current process; default to root
        (0, 0)
    }

    /// Read file with permission check
    pub fn read_file_checked(&self, path: &str, uid: u32, gid: u32) -> Result<&[u8], i32> {
        let ino = self.resolve_path(path).ok_or(-2i32)?;
        let inode = self.get_inode(ino).ok_or(-2i32)?;
        if !self.check_permission(ino, uid, gid, 4) {
            // R_OK
            return Err(-13); // EACCES
        }
        if inode.file_type != FileType::Regular && inode.file_type != FileType::CharDevice {
            return Err(-21); // EISDIR
        }
        Ok(&inode.data)
    }

    /// Write file with permission check
    pub fn write_file_checked(
        &mut self,
        path: &str,
        data: &[u8],
        uid: u32,
        gid: u32,
    ) -> Result<(), i32> {
        if let Some(ino) = self.resolve_path(path) {
            if !self.check_permission(ino, uid, gid, 2) {
                // W_OK
                return Err(-13); // EACCES
            }
            if let Some(inode) = self.inodes.iter_mut().find(|i| i.ino == ino) {
                inode.data = Vec::from(data);
                inode.size = data.len() as u64;
                return Ok(());
            }
        }
        // File doesn't exist — check write on parent directory
        let path_trimmed = path.trim_start_matches('/');
        let parts: Vec<&str> = path_trimmed.split('/').collect();
        let parent_path = if parts.len() > 1 {
            alloc::format!("/{}", parts[..parts.len() - 1].join("/"))
        } else {
            String::from("/")
        };
        if let Some(parent_ino) = self.resolve_path(&parent_path) {
            if !self.check_permission(parent_ino, uid, gid, 2) {
                return Err(-13); // EACCES
            }
        }
        if self.create_file_at_path(path, FileType::Regular, data, 0o644) {
            // Set the new file's owner
            if let Some(ino) = self.resolve_path(path) {
                if let Some(inode) = self.inodes.iter_mut().find(|i| i.ino == ino) {
                    inode.uid = uid;
                    inode.gid = gid;
                }
            }
            Ok(())
        } else {
            Err(-5) // EIO
        }
    }

    /// chmod — change file permissions
    pub fn chmod(&mut self, path: &str, mode: u16, uid: u32) -> Result<(), i32> {
        let ino = self.resolve_path(path).ok_or(-2i32)?;
        let inode = self.inodes.iter_mut().find(|i| i.ino == ino).ok_or(-2i32)?;
        // Only owner or root can chmod
        if uid != 0 && uid != inode.uid {
            return Err(-1); // EPERM
        }
        inode.permissions = mode;
        Ok(())
    }

    /// chown — change file ownership
    pub fn chown(
        &mut self,
        path: &str,
        new_uid: u32,
        new_gid: u32,
        caller_uid: u32,
    ) -> Result<(), i32> {
        let ino = self.resolve_path(path).ok_or(-2i32)?;
        let inode = self.inodes.iter_mut().find(|i| i.ino == ino).ok_or(-2i32)?;
        // Only root can chown
        if caller_uid != 0 {
            return Err(-1); // EPERM
        }
        if new_uid != u32::MAX {
            inode.uid = new_uid;
        }
        if new_gid != u32::MAX {
            inode.gid = new_gid;
        }
        Ok(())
    }

    /// Get file stat information
    pub fn stat(&self, path: &str) -> Result<VfsStat, i32> {
        let ino = self.resolve_path(path).ok_or(-2i32)?; // ENOENT
        let inode = self.get_inode(ino).ok_or(-2i32)?;

        Ok(VfsStat {
            ino,
            file_type: inode.file_type,
            size: inode.size,
            permissions: inode.permissions,
            uid: inode.uid,
            gid: inode.gid,
            nlink: if inode.file_type == FileType::Directory {
                2 + inode.children.len() as u64
            } else {
                1
            },
        })
    }

    /// Get total number of inodes
    pub fn inode_count(&self) -> usize {
        self.inodes.len()
    }
}

/// File stat result
#[derive(Debug, Clone)]
pub struct VfsStat {
    pub ino: u64,
    pub file_type: FileType,
    pub size: u64,
    pub permissions: u16,
    pub uid: u32,
    pub gid: u32,
    pub nlink: u64,
}

/// Initialize the VFS
pub fn init() {
    // Force lazy initialization
    let vfs = VFS.lock();
    crate::serial_println!(
        "[KnoxOS] VFS initialized: {} inodes, Linux FHS layout",
        vfs.inode_count()
    );
    drop(vfs);

    // Install built-in binaries (/bin/sh, /bin/busybox)
    install_builtin_binaries();
}

// ─── Ext4 Dispatch Layer ────────────────────────────────────────────────
// When a path falls under an ext4 mount point, we dispatch to the ext4
// driver instead of the in-memory VFS.

/// Read a file, dispatching to ext4 if the path is under an ext4 mount
pub fn read_file_dispatch(path: &str) -> Option<Vec<u8>> {
    // Check if this path is under an ext4 mount point
    if let Some((fs_idx, rel_path)) = crate::mount::find_ext4_fs_for_path(path) {
        match crate::ext4::read_file(fs_idx, &rel_path) {
            Ok(data) => return Some(data),
            Err(_) => {
                // Fall through to in-memory VFS
            }
        }
    }
    // Fall back to in-memory VFS
    let vfs = VFS.lock();
    vfs.read_file(path).map(|d| d.to_vec())
}

/// List directory contents, dispatching to ext4 if under ext4 mount
pub fn list_dir_dispatch(path: &str) -> Option<Vec<String>> {
    if let Some((fs_idx, rel_path)) = crate::mount::find_ext4_fs_for_path(path) {
        if let Ok(entries) = crate::ext4::list_dir(fs_idx, &rel_path) {
            return Some(entries.into_iter().map(|e| e.name).collect());
        }
    }
    let vfs = VFS.lock();
    vfs.list_dir(path)
}

/// Stat a file, dispatching to ext4 if under ext4 mount
pub fn stat_dispatch(path: &str) -> Result<VfsStat, i32> {
    if let Some((fs_idx, rel_path)) = crate::mount::find_ext4_fs_for_path(path) {
        if let Ok(info) = crate::ext4::stat_file(fs_idx, &rel_path) {
            let file_type = if info.mode & 0xF000 == 0x4000 {
                FileType::Directory
            } else if info.mode & 0xF000 == 0xA000 {
                FileType::SymLink
            } else {
                FileType::Regular
            };
            return Ok(VfsStat {
                ino: info.inode as u64,
                file_type,
                size: info.size,
                permissions: info.mode & 0o7777,
                uid: info.uid,
                gid: info.gid,
                nlink: info.links as u64,
            });
        }
    }
    let vfs = VFS.lock();
    vfs.stat(path)
}

/// Write a file, dispatching to ext4 if under ext4 mount
pub fn write_file_dispatch(path: &str, data: &[u8]) -> bool {
    if let Some((fs_idx, rel_path)) = crate::mount::find_ext4_fs_for_path(path) {
        match crate::ext4::write_file(fs_idx, &rel_path, data) {
            Ok(()) => return true,
            Err(_) => {
                // Fall through to in-memory VFS
            }
        }
    }
    let mut vfs = VFS.lock();
    let ok = vfs.write_file(path, data);
    let perms = vfs
        .resolve_path(path)
        .and_then(|ino| vfs.get_inode(ino))
        .map(|i| i.permissions)
        .unwrap_or(0o644);
    drop(vfs);
    // Persist to disk for durability across reboots
    if ok {
        crate::persist::persist_file(path, data, perms);
    }
    ok
}

/// Create a file via dispatch (ext4 or in-memory VFS)
pub fn create_file_dispatch(path: &str, data: &[u8]) {
    write_file_dispatch(path, data);
}

/// List directory contents (alias for list_dir_dispatch)
pub fn list_directory(path: &str) -> Option<Vec<String>> {
    list_dir_dispatch(path)
}

/// Ensure a directory exists, creating it if needed (like mkdir -p)
pub fn ensure_directory(path: &str) {
    let mut vfs = VFS.lock();
    // Walk the path and create each component
    let parts: Vec<&str> = path
        .trim_start_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    let mut current = String::from("");
    for part in parts {
        current.push('/');
        current.push_str(part);
        if vfs.resolve_path(&current).is_none() {
            let _ = vfs.mkdir(&current, 0o755);
        }
    }
}

/// Remove a file or directory via dispatch
pub fn remove_dispatch(path: &str) -> Result<(), i32> {
    let mut vfs = VFS.lock();
    vfs.unlink(path)
}

/// Create a symbolic link
pub fn create_symlink_dispatch(target: &str, linkpath: &str) {
    let mut vfs = VFS.lock();
    // Create a symlink inode with the target as content
    let _ = vfs.create_file_at_path(linkpath, FileType::SymLink, target.as_bytes(), 0o777);
}

/// Copy a file from `src_path` to `dst_path` via VFS dispatch.
/// If `dst_path` is a directory, the file is copied into it with the same name.
pub fn copy_file_dispatch(src_path: &str, dst_path: &str) -> Result<(), i32> {
    let data = read_file_dispatch(src_path).ok_or(-2i32)?; // ENOENT
    // If dst_path is a directory, append the source filename
    let final_dst = {
        let vfs = VFS.lock();
        let is_dir = vfs
            .resolve_path(dst_path)
            .and_then(|ino| vfs.get_inode(ino))
            .map(|i| i.file_type == FileType::Directory)
            .unwrap_or(false);
        if is_dir {
            let name = src_path.rsplit('/').next().unwrap_or("copied_file");
            alloc::format!("{}/{}", dst_path.trim_end_matches('/'), name)
        } else {
            String::from(dst_path)
        }
    };
    if write_file_dispatch(&final_dst, &data) {
        Ok(())
    } else {
        Err(-5) // EIO
    }
}

/// Move (rename) a file from `src_path` to `dst_path` via VFS dispatch.
/// If `dst_path` is a directory, the file is moved into it with the same name.
pub fn move_file_dispatch(src_path: &str, dst_path: &str) -> Result<(), i32> {
    let final_dst = {
        let vfs = VFS.lock();
        let is_dir = vfs
            .resolve_path(dst_path)
            .and_then(|ino| vfs.get_inode(ino))
            .map(|i| i.file_type == FileType::Directory)
            .unwrap_or(false);
        if is_dir {
            let name = src_path.rsplit('/').next().unwrap_or("moved_file");
            alloc::format!("{}/{}", dst_path.trim_end_matches('/'), name)
        } else {
            String::from(dst_path)
        }
    };
    let mut vfs = VFS.lock();
    vfs.rename(src_path, &final_dst)
}

/// Set root filesystem path
pub fn set_root(_new_root: &str) {
    // Stub: in a full implementation this would pivot_root
    crate::serial_println!("[VFS] set_root: {}", _new_root);
}

/// Install built-in ELF binaries into the VFS
/// These are minimal static ELF64 programs that use Linux syscalls
/// to provide basic shell/utility functionality
fn install_builtin_binaries() {
    // Generate a minimal /bin/sh ELF that:
    //   1. Writes "$ " prompt to stdout
    //   2. Reads a line from stdin
    //   3. If "exit", calls exit(0)
    //   4. Otherwise, writes the line back to stdout (echo)
    //   5. Loops
    let sh_elf = generate_sh_elf();
    let mut vfs = VFS.lock();
    vfs.write_file("/bin/sh", &sh_elf);
    vfs.write_file("/bin/bash", &sh_elf);
    vfs.write_file("/bin/ash", &sh_elf);
    drop(vfs);

    // Also install into initramfs-searched paths
    let mut vfs = VFS.lock();
    vfs.write_file("/sbin/init", &generate_init_elf());
    vfs.write_file("/bin/hello", &crate::init::hello_userspace_elf_data());
    drop(vfs);

    crate::serial_println!(
        "[VFS] Installed built-in binaries: /bin/sh, /bin/bash, /bin/ash, /sbin/init, /bin/hello"
    );
}

/// Generate a minimal /bin/sh ELF64 executable
/// This is a statically-linked shell that uses Linux x86_64 syscalls:
///   - write(1, "$ ", 2) — print prompt
///   - read(0, buf, 256) — read a line from stdin
///   - write(1, buf, n)  — echo the line
///   - Loop forever (PID 1 init does wait4)
fn generate_sh_elf() -> Vec<u8> {
    let entry_point: u64 = 0x0040_1000;
    let data_vaddr: u64 = 0x0040_2000;
    let program_header_offset: u64 = 0x40;

    let mut elf = Vec::new();

    // ─── ELF Header (64 bytes) ──────────────────────────────────────
    elf.extend_from_slice(&[0x7f, b'E', b'L', b'F']); // magic
    elf.push(2); // ELFCLASS64
    elf.push(1); // ELFDATA2LSB
    elf.push(1); // EV_CURRENT
    elf.push(0); // ELFOSABI_NONE
    elf.extend_from_slice(&[0; 8]); // padding
    elf.extend_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    elf.extend_from_slice(&62u16.to_le_bytes()); // EM_X86_64
    elf.extend_from_slice(&1u32.to_le_bytes()); // e_version
    elf.extend_from_slice(&entry_point.to_le_bytes());
    elf.extend_from_slice(&program_header_offset.to_le_bytes());
    elf.extend_from_slice(&0u64.to_le_bytes()); // e_shoff
    elf.extend_from_slice(&0u32.to_le_bytes()); // e_flags
    elf.extend_from_slice(&64u16.to_le_bytes()); // e_ehsize
    elf.extend_from_slice(&56u16.to_le_bytes()); // e_phentsize
    elf.extend_from_slice(&2u16.to_le_bytes()); // e_phnum (2 segments: code + data)
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shentsize
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shnum
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shstrndx

    // ─── Program Header 1: CODE (.text) at 0x401000 ────────────────
    let code_file_offset: u64 = 0x1000;
    let code_vaddr: u64 = 0x0040_1000;
    let code_size: u64 = 0x1000;
    elf.extend_from_slice(&1u32.to_le_bytes()); // PT_LOAD
    elf.extend_from_slice(&5u32.to_le_bytes()); // PF_R | PF_X
    elf.extend_from_slice(&code_file_offset.to_le_bytes());
    elf.extend_from_slice(&code_vaddr.to_le_bytes());
    elf.extend_from_slice(&code_vaddr.to_le_bytes()); // p_paddr
    elf.extend_from_slice(&code_size.to_le_bytes());
    elf.extend_from_slice(&code_size.to_le_bytes()); // p_memsz
    elf.extend_from_slice(&0x1000u64.to_le_bytes()); // p_align

    // ─── Program Header 2: DATA (.data/.bss) at 0x402000 ───────────
    let data_file_offset: u64 = 0x2000;
    let data_size: u64 = 0x1000;
    elf.extend_from_slice(&1u32.to_le_bytes()); // PT_LOAD
    elf.extend_from_slice(&6u32.to_le_bytes()); // PF_R | PF_W
    elf.extend_from_slice(&data_file_offset.to_le_bytes());
    elf.extend_from_slice(&data_vaddr.to_le_bytes());
    elf.extend_from_slice(&data_vaddr.to_le_bytes());
    elf.extend_from_slice(&data_size.to_le_bytes());
    elf.extend_from_slice(&data_size.to_le_bytes());
    elf.extend_from_slice(&0x1000u64.to_le_bytes());

    // ─── Pad to code_file_offset ────────────────────────────────────
    elf.resize(code_file_offset as usize, 0);

    // ─── Code Section: Minimal interactive shell ────────────────────
    // This shell:
    //   prompt: write(1, prompt_str, 2)       ; "$ "
    //   read(0, buffer, 255)                  ; read line from stdin
    //   if read returned 0 or negative, exit
    //   write(1, buffer, n)                   ; echo input
    //   jmp prompt
    //
    // Addresses (data segment): prompt_str = 0x402000, buffer = 0x402010
    #[rustfmt::skip]
    let code: &[u8] = &[
        // _start:
        // ── Print prompt "$ " ──
        // write(1, 0x402000, 2)
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1 (SYS_write)
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // mov rdi, 1 (stdout)
        0x48, 0xC7, 0xC6, 0x00, 0x20, 0x40, 0x00, // mov rsi, 0x402000 (prompt)
        0x48, 0xC7, 0xC2, 0x02, 0x00, 0x00, 0x00, // mov rdx, 2 (length)
        0x0F, 0x05,                                 // syscall
        // ── Read input from stdin ──
        // read(0, 0x402010, 255)
        0x48, 0xC7, 0xC0, 0x00, 0x00, 0x00, 0x00, // mov rax, 0 (SYS_read)
        0x48, 0x31, 0xFF,                           // xor rdi, rdi (stdin=0)
        0x48, 0xC7, 0xC6, 0x10, 0x20, 0x40, 0x00, // mov rsi, 0x402010 (buffer)
        0x48, 0xC7, 0xC2, 0xFF, 0x00, 0x00, 0x00, // mov rdx, 255
        0x0F, 0x05,                                 // syscall
        // ── Check if read returned <= 0 (EOF) ──
        0x48, 0x85, 0xC0,                           // test rax, rax
        0x7E, 0x1D,                                 // jle exit (offset to exit)
        // ── Save count in r8 ──
        0x49, 0x89, 0xC0,                           // mov r8, rax
        // ── Echo: write(1, buffer, n) ──
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1 (SYS_write)
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // mov rdi, 1 (stdout)
        0x48, 0xC7, 0xC6, 0x10, 0x20, 0x40, 0x00, // mov rsi, 0x402010 (buffer)
        0x4C, 0x89, 0xC2,                           // mov rdx, r8 (count)
        0x0F, 0x05,                                 // syscall
        // ── Loop back to prompt ──
        0xEB, 0xA3,                                 // jmp _start (back to top)
        // exit:
        // exit(0)
        0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, // mov rax, 60 (SYS_exit)
        0x48, 0x31, 0xFF,                           // xor rdi, rdi (status=0)
        0x0F, 0x05,                                 // syscall
        // unreachable
        0xF4,                                       // hlt
    ];

    elf.extend_from_slice(code);
    // Pad to full code page
    elf.resize((code_file_offset + code_size) as usize, 0);

    // ─── Data Section at 0x2000 ─────────────────────────────────────
    elf.resize(data_file_offset as usize, 0);
    // prompt_str at offset 0 in data page: "$ "
    elf.extend_from_slice(b"$ ");
    // Pad to 0x10 where buffer starts (will be zero / .bss)
    elf.resize((data_file_offset + data_size) as usize, 0);

    elf
}

/// Generate a minimal /sbin/init ELF64
fn generate_init_elf() -> Vec<u8> {
    // Reuse the same builtin init from the init module
    crate::init::builtin_init_elf_data()
}
