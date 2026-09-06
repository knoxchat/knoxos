/// cred — Process credentials management
/// Linux-compatible credential tracking (real/effective/saved UIDs/GIDs,
/// supplementary groups, capabilities per-process)
///
/// Used by the kernel for permission checks on every syscall.
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Maximum supplementary groups per process
const NGROUPS_MAX: usize = 65536;

/// Process credentials
#[derive(Debug, Clone)]
pub struct Credentials {
    /// Real user ID
    pub uid: u32,
    /// Real group ID
    pub gid: u32,
    /// Effective user ID
    pub euid: u32,
    /// Effective group ID
    pub egid: u32,
    /// Saved set-user-ID
    pub suid: u32,
    /// Saved set-group-ID
    pub sgid: u32,
    /// Filesystem user ID (for VFS checks)
    pub fsuid: u32,
    /// Filesystem group ID
    pub fsgid: u32,
    /// Supplementary group IDs
    pub groups: Vec<u32>,
    /// Capability sets
    pub cap_inheritable: u64,
    pub cap_permitted: u64,
    pub cap_effective: u64,
    pub cap_bset: u64,    // Bounding set
    pub cap_ambient: u64, // Ambient capabilities
    /// User namespace ID
    pub user_ns: u32,
    /// Securebits flags
    pub securebits: u32,
}

impl Default for Credentials {
    fn default() -> Self {
        Self::root()
    }
}

impl Credentials {
    /// Create root credentials (all capabilities)
    pub fn root() -> Self {
        Credentials {
            uid: 0,
            gid: 0,
            euid: 0,
            egid: 0,
            suid: 0,
            sgid: 0,
            fsuid: 0,
            fsgid: 0,
            groups: Vec::new(),
            cap_inheritable: 0,
            cap_permitted: !0u64, // All capabilities
            cap_effective: !0u64,
            cap_bset: !0u64,
            cap_ambient: 0,
            user_ns: 0,
            securebits: 0,
        }
    }

    /// Create credentials for a regular user
    pub fn user(uid: u32, gid: u32) -> Self {
        Credentials {
            uid,
            gid,
            euid: uid,
            egid: gid,
            suid: uid,
            sgid: gid,
            fsuid: uid,
            fsgid: gid,
            groups: Vec::new(),
            cap_inheritable: 0,
            cap_permitted: 0,
            cap_effective: 0,
            cap_bset: !0u64,
            cap_ambient: 0,
            user_ns: 0,
            securebits: 0,
        }
    }

    /// Check if process has a specific capability
    pub fn has_capability(&self, cap: u32) -> bool {
        if cap > 63 {
            return false;
        }
        (self.cap_effective & (1u64 << cap)) != 0
    }

    /// Check if process is root (effective uid 0)
    pub fn is_root(&self) -> bool {
        self.euid == 0
    }

    /// Check if supplementary group is in the list
    pub fn in_group(&self, gid: u32) -> bool {
        self.gid == gid || self.egid == gid || self.groups.contains(&gid)
    }

    /// setuid syscall implementation
    pub fn set_uid(&mut self, uid: u32) -> Result<(), i32> {
        if self.euid == 0 {
            // Root can set all UIDs
            self.uid = uid;
            self.euid = uid;
            self.suid = uid;
            self.fsuid = uid;
        } else if uid == self.uid || uid == self.suid {
            self.euid = uid;
            self.fsuid = uid;
        } else {
            return Err(-1); // EPERM
        }
        Ok(())
    }

    /// setgid syscall implementation
    pub fn set_gid(&mut self, gid: u32) -> Result<(), i32> {
        if self.euid == 0 {
            self.gid = gid;
            self.egid = gid;
            self.sgid = gid;
            self.fsgid = gid;
        } else if gid == self.gid || gid == self.sgid {
            self.egid = gid;
            self.fsgid = gid;
        } else {
            return Err(-1); // EPERM
        }
        Ok(())
    }

    /// setreuid syscall
    pub fn set_reuid(&mut self, ruid: u32, euid: u32) -> Result<(), i32> {
        let ruid = if ruid == u32::MAX { self.uid } else { ruid };
        let euid = if euid == u32::MAX { self.euid } else { euid };

        if self.euid != 0 {
            if ruid != self.uid && ruid != self.euid {
                return Err(-1);
            }
            if euid != self.uid && euid != self.euid && euid != self.suid {
                return Err(-1);
            }
        }

        self.uid = ruid;
        self.euid = euid;
        self.fsuid = euid;
        if ruid != u32::MAX {
            self.suid = euid;
        }
        Ok(())
    }

    /// setregid syscall
    pub fn set_regid(&mut self, rgid: u32, egid: u32) -> Result<(), i32> {
        let rgid = if rgid == u32::MAX { self.gid } else { rgid };
        let egid = if egid == u32::MAX { self.egid } else { egid };

        if self.euid != 0 {
            if rgid != self.gid && rgid != self.egid {
                return Err(-1);
            }
            if egid != self.gid && egid != self.egid && egid != self.sgid {
                return Err(-1);
            }
        }

        self.gid = rgid;
        self.egid = egid;
        self.fsgid = egid;
        Ok(())
    }

    /// Set supplementary groups
    pub fn set_groups(&mut self, groups: &[u32]) -> Result<(), i32> {
        if self.euid != 0 {
            return Err(-1); // EPERM
        }
        if groups.len() > NGROUPS_MAX {
            return Err(-22); // EINVAL
        }
        self.groups = groups.to_vec();
        Ok(())
    }

    /// Apply exec transformations (setuid/setgid binaries, capabilities)
    pub fn exec_transform(&mut self, file_uid: u32, file_gid: u32, setuid: bool, setgid: bool) {
        if setuid {
            self.euid = file_uid;
            self.suid = file_uid;
            self.fsuid = file_uid;
        }
        if setgid {
            self.egid = file_gid;
            self.sgid = file_gid;
            self.fsgid = file_gid;
        }

        // Capability transformations on exec
        if self.euid == 0 && self.uid != 0 {
            // Root exec from non-root: gain all caps
            self.cap_permitted = self.cap_bset;
            self.cap_effective = self.cap_permitted;
        } else if self.uid != 0 {
            // Non-root exec: only keep ambient caps
            let new_permitted = (self.cap_inheritable & self.cap_bset) | self.cap_ambient;
            self.cap_permitted = new_permitted;
            self.cap_effective = new_permitted & self.cap_ambient;
        }
    }

    /// Fork credentials (copy on fork)
    pub fn fork(&self) -> Self {
        self.clone()
    }
}

/// Per-process credential table
lazy_static::lazy_static! {
    static ref PROCESS_CREDS: Mutex<BTreeMap<u32, Credentials>> = Mutex::new(BTreeMap::new());
}

/// Set credentials for a process
pub fn set_process_creds(pid: u32, creds: Credentials) {
    PROCESS_CREDS.lock().insert(pid, creds);
}

/// Get credentials for a process
pub fn get_process_creds(pid: u32) -> Option<Credentials> {
    PROCESS_CREDS.lock().get(&pid).cloned()
}

/// Get current process credentials
pub fn current_creds() -> Credentials {
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    get_process_creds(pid).unwrap_or_else(Credentials::root)
}

/// Remove credentials for a process (on exit)
pub fn remove_process_creds(pid: u32) {
    PROCESS_CREDS.lock().remove(&pid);
}

/// Fork credentials from parent to child
pub fn fork_creds(parent_pid: u32, child_pid: u32) {
    let creds = get_process_creds(parent_pid).unwrap_or_else(Credentials::root);
    set_process_creds(child_pid, creds.fork());
}

/// Permission check: can process access file with given uid/gid/mode?
pub fn check_file_permission(
    creds: &Credentials,
    file_uid: u32,
    file_gid: u32,
    file_mode: u32,
    access: u32,
) -> bool {
    // Root can do everything (with DAC_OVERRIDE capability)
    if creds.has_capability(1) {
        // CAP_DAC_OVERRIDE
        return true;
    }

    let mode_bits = if creds.fsuid == file_uid {
        (file_mode >> 6) & 7
    } else if creds.in_group(file_gid) {
        (file_mode >> 3) & 7
    } else {
        file_mode & 7
    };

    (mode_bits & access) == access
}

pub fn init() {
    // Set up root credentials for PID 0 and PID 1
    set_process_creds(0, Credentials::root());
    set_process_creds(1, Credentials::root());

    serial_println!("[KnoxOS] Process credentials subsystem initialized");
}
