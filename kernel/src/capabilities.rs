/// Capabilities - Linux capability-based security
/// Compatible with Linux capabilities(7)
/// Fine-grained privilege control beyond simple root/non-root
use alloc::collections::BTreeMap;
use spin::Mutex;

/// Linux capabilities (subset of the most important ones)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u32)]
pub enum Capability {
    /// Bypass file read, write, execute permission checks
    CapDacOverride = 1,
    /// Bypass file read permission checks
    CapDacReadSearch = 2,
    /// Bypass permission checks on operations that require the filesystem UID
    CapFowner = 3,
    /// Don't clear setuid/setgid on modified files
    CapFsetid = 4,
    /// Bypass permission checks for sending signals
    CapKill = 5,
    /// Allow changing the group ID of a process
    CapSetgid = 6,
    /// Allow changing the user ID of a process
    CapSetuid = 7,
    /// Allow setting capabilities
    CapSetpcap = 8,
    /// Allow modifying kernel parameters via sysctl
    CapSysAdmin = 21,
    /// Allow use of reboot()
    CapSysBoot = 22,
    /// Allow setting the nice value
    CapSysNice = 23,
    /// Allow setting resource limits
    CapSysResource = 24,
    /// Allow system time manipulation
    CapSysTime = 25,
    /// Allow TTY configuration
    CapSysTtyConfig = 26,
    /// Allow loading/unloading kernel modules
    CapSysModule = 16,
    /// Allow raw I/O port access
    CapSysRawio = 17,
    /// Allow chroot
    CapSysChroot = 18,
    /// Allow ptrace
    CapSysPtrace = 19,
    /// Allow network administration
    CapNetAdmin = 12,
    /// Allow binding to privileged ports (< 1024)
    CapNetBindService = 10,
    /// Allow raw socket access
    CapNetRaw = 13,
    /// Allow IPC_SET and IPC_RMID operations
    CapIpcOwner = 15,
}

/// Capability set (bitmask)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilitySet(pub u64);

impl CapabilitySet {
    pub const EMPTY: CapabilitySet = CapabilitySet(0);
    pub const ALL: CapabilitySet = CapabilitySet(u64::MAX);

    /// Root capabilities (all capabilities)
    pub fn root() -> Self {
        Self::ALL
    }

    /// Default user capabilities
    pub fn default_user() -> Self {
        Self::EMPTY
    }

    pub fn has(&self, cap: Capability) -> bool {
        self.0 & (1u64 << (cap as u32)) != 0
    }

    pub fn add(&mut self, cap: Capability) {
        self.0 |= 1u64 << (cap as u32);
    }

    pub fn remove(&mut self, cap: Capability) {
        self.0 &= !(1u64 << (cap as u32));
    }

    pub fn clear(&mut self) {
        self.0 = 0;
    }
}

/// Per-process capability sets
#[derive(Debug, Clone)]
pub struct ProcessCapabilities {
    /// Effective capabilities (currently active)
    pub effective: CapabilitySet,
    /// Permitted capabilities (max set of caps process can have)
    pub permitted: CapabilitySet,
    /// Inheritable capabilities (passed to child processes)
    pub inheritable: CapabilitySet,
    /// Bounding set (upper limit on caps that can be gained)
    pub bounding: CapabilitySet,
    /// Ambient capabilities (automatically added to execve'd programs)
    pub ambient: CapabilitySet,
}

impl ProcessCapabilities {
    /// Create root capabilities (all caps)
    pub fn root() -> Self {
        Self {
            effective: CapabilitySet::ALL,
            permitted: CapabilitySet::ALL,
            inheritable: CapabilitySet::ALL,
            bounding: CapabilitySet::ALL,
            ambient: CapabilitySet::EMPTY,
        }
    }

    /// Create default user capabilities (minimal caps)
    pub fn user() -> Self {
        Self {
            effective: CapabilitySet::EMPTY,
            permitted: CapabilitySet::EMPTY,
            inheritable: CapabilitySet::EMPTY,
            bounding: CapabilitySet::ALL,
            ambient: CapabilitySet::EMPTY,
        }
    }
}

/// Global per-process capabilities
lazy_static::lazy_static! {
    static ref PROCESS_CAPS: Mutex<BTreeMap<u32, ProcessCapabilities>> = Mutex::new(BTreeMap::new());
}

/// Check if a process has a specific capability
pub fn has_capability(pid: u32, cap: Capability) -> bool {
    let caps = PROCESS_CAPS.lock();
    if let Some(pcaps) = caps.get(&pid) {
        pcaps.effective.has(cap)
    } else {
        // Check if process is root
        let table = crate::process::PROCESS_TABLE.lock();
        if let Some(proc) = table.get_process(pid) {
            proc.uid == 0 // Root has all capabilities
        } else {
            false
        }
    }
}

/// Set capabilities for a process
pub fn set_capabilities(pid: u32, caps: ProcessCapabilities) {
    PROCESS_CAPS.lock().insert(pid, caps);
}

/// Get capabilities for a process
pub fn get_capabilities(pid: u32) -> Option<ProcessCapabilities> {
    PROCESS_CAPS.lock().get(&pid).cloned()
}

/// Drop a capability from a process
pub fn drop_capability(pid: u32, cap: Capability) -> Result<(), i32> {
    let mut caps = PROCESS_CAPS.lock();
    if let Some(pcaps) = caps.get_mut(&pid) {
        pcaps.effective.remove(cap);
        pcaps.permitted.remove(cap); // Once dropped from permitted, can't regain
        Ok(())
    } else {
        Err(-3) // ESRCH
    }
}

/// Initialize capabilities for a new process
pub fn init_process_caps(pid: u32, parent_pid: u32) {
    let parent_caps = PROCESS_CAPS.lock().get(&parent_pid).cloned();

    let caps = if let Some(parent) = parent_caps {
        ProcessCapabilities {
            effective: CapabilitySet(parent.inheritable.0 & parent.bounding.0),
            permitted: CapabilitySet(parent.inheritable.0 & parent.bounding.0),
            inheritable: parent.inheritable,
            bounding: parent.bounding,
            ambient: parent.ambient,
        }
    } else {
        // Check if root
        let table = crate::process::PROCESS_TABLE.lock();
        if let Some(proc) = table.get_process(pid) {
            if proc.uid == 0 {
                ProcessCapabilities::root()
            } else {
                ProcessCapabilities::user()
            }
        } else {
            ProcessCapabilities::user()
        }
    };

    PROCESS_CAPS.lock().insert(pid, caps);
}

/// Initialize capabilities subsystem
pub fn init() {
    // Set up capabilities for initial processes
    PROCESS_CAPS.lock().insert(0, ProcessCapabilities::root()); // kernel
    PROCESS_CAPS.lock().insert(1, ProcessCapabilities::root()); // init
    PROCESS_CAPS.lock().insert(2, ProcessCapabilities::user()); // desktop

    crate::serial_println!("[KnoxOS] Linux capabilities initialized");
}
