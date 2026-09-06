/// Namespaces - Linux namespace isolation for containers
/// Compatible with Linux namespace types (clone flags)
/// Provides resource isolation: PID, mount, network, user, UTS, IPC
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

/// Namespace types (matching Linux CLONE_NEW* flags)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum NamespaceType {
    /// Mount namespace (CLONE_NEWNS)
    Mount = 0x00020000,
    /// UTS namespace - hostname/domainname (CLONE_NEWUTS)
    Uts = 0x04000000,
    /// IPC namespace (CLONE_NEWIPC)
    Ipc = 0x08000000,
    /// PID namespace (CLONE_NEWPID)
    Pid = 0x20000000,
    /// Network namespace (CLONE_NEWNET)
    Net = 0x40000000,
    /// User namespace (CLONE_NEWUSER)
    User = 0x10000000,
    /// Cgroup namespace (CLONE_NEWCGROUP)
    Cgroup = 0x02000000,
}

/// A namespace instance
#[derive(Debug, Clone)]
pub struct Namespace {
    pub id: u64,
    pub ns_type: NamespaceType,
    pub owner_pid: u32,
    pub ref_count: u32,
}

/// UTS namespace data (hostname/domainname)
#[derive(Debug, Clone)]
pub struct UtsNamespace {
    pub hostname: String,
    pub domainname: String,
}

/// PID namespace data
#[derive(Debug, Clone)]
pub struct PidNamespace {
    pub id: u64,
    /// Real PID -> namespace PID mapping
    pub pid_map: BTreeMap<u32, u32>,
    pub next_pid: u32,
}

/// Mount namespace data
#[derive(Debug, Clone)]
pub struct MountNamespace {
    pub id: u64,
    pub mounts: Vec<MountEntry>,
}

/// A mount point entry
#[derive(Debug, Clone)]
pub struct MountEntry {
    pub source: String,
    pub target: String,
    pub fstype: String,
    pub flags: u32,
}

/// Network namespace data
#[derive(Debug, Clone)]
pub struct NetNamespace {
    pub id: u64,
    pub interfaces: Vec<String>,
    pub has_loopback: bool,
}

/// Per-process namespace set
#[derive(Debug, Clone)]
pub struct ProcessNamespaces {
    pub mount_ns: u64,
    pub uts_ns: u64,
    pub ipc_ns: u64,
    pub pid_ns: u64,
    pub net_ns: u64,
    pub user_ns: u64,
    pub cgroup_ns: u64,
}

impl Default for ProcessNamespaces {
    /// Default (init) namespace set
    fn default() -> Self {
        Self {
            mount_ns: 1,
            uts_ns: 1,
            ipc_ns: 1,
            pid_ns: 1,
            net_ns: 1,
            user_ns: 1,
            cgroup_ns: 1,
        }
    }
}

/// Global namespace registry
lazy_static::lazy_static! {
    static ref NAMESPACES: Mutex<BTreeMap<u64, Namespace>> = Mutex::new(BTreeMap::new());
    static ref UTS_NAMESPACES: Mutex<BTreeMap<u64, UtsNamespace>> = Mutex::new(BTreeMap::new());
    static ref PID_NAMESPACES: Mutex<BTreeMap<u64, PidNamespace>> = Mutex::new(BTreeMap::new());
    static ref MOUNT_NAMESPACES: Mutex<BTreeMap<u64, MountNamespace>> = Mutex::new(BTreeMap::new());
    static ref NET_NAMESPACES: Mutex<BTreeMap<u64, NetNamespace>> = Mutex::new(BTreeMap::new());
    static ref PROCESS_NS: Mutex<BTreeMap<u32, ProcessNamespaces>> = Mutex::new(BTreeMap::new());
}

static NEXT_NS_ID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(100);

/// Create a new namespace
pub fn create_namespace(ns_type: NamespaceType, owner_pid: u32) -> Result<u64, i32> {
    let id = NEXT_NS_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

    let ns = Namespace {
        id,
        ns_type,
        owner_pid,
        ref_count: 1,
    };

    NAMESPACES.lock().insert(id, ns);

    // Initialize type-specific data
    match ns_type {
        NamespaceType::Uts => {
            UTS_NAMESPACES.lock().insert(
                id,
                UtsNamespace {
                    hostname: String::from("knoxos"),
                    domainname: String::from("(none)"),
                },
            );
        }
        NamespaceType::Pid => {
            PID_NAMESPACES.lock().insert(
                id,
                PidNamespace {
                    id,
                    pid_map: BTreeMap::new(),
                    next_pid: 1,
                },
            );
        }
        NamespaceType::Mount => {
            MOUNT_NAMESPACES.lock().insert(
                id,
                MountNamespace {
                    id,
                    mounts: Vec::new(),
                },
            );
        }
        NamespaceType::Net => {
            let mut net_ns = NetNamespace {
                id,
                interfaces: Vec::new(),
                has_loopback: true,
            };
            net_ns.interfaces.push(String::from("lo"));
            NET_NAMESPACES.lock().insert(id, net_ns);
        }
        _ => {}
    }

    crate::serial_println!("[KnoxOS] Namespace created: type={:?} id={}", ns_type, id);
    Ok(id)
}

/// Enter a namespace (unshare)
pub fn unshare(pid: u32, flags: u32) -> Result<(), i32> {
    let mut proc_ns = PROCESS_NS.lock();
    let ns = proc_ns.entry(pid).or_default();

    // Create new namespaces for each flag
    if flags & NamespaceType::Mount as u32 != 0 {
        ns.mount_ns = create_namespace(NamespaceType::Mount, pid)?;
    }
    if flags & NamespaceType::Uts as u32 != 0 {
        ns.uts_ns = create_namespace(NamespaceType::Uts, pid)?;
    }
    if flags & NamespaceType::Ipc as u32 != 0 {
        ns.ipc_ns = create_namespace(NamespaceType::Ipc, pid)?;
    }
    if flags & NamespaceType::Pid as u32 != 0 {
        ns.pid_ns = create_namespace(NamespaceType::Pid, pid)?;
    }
    if flags & NamespaceType::Net as u32 != 0 {
        ns.net_ns = create_namespace(NamespaceType::Net, pid)?;
    }
    if flags & NamespaceType::User as u32 != 0 {
        ns.user_ns = create_namespace(NamespaceType::User, pid)?;
    }

    Ok(())
}

/// Set hostname in the UTS namespace of a process
pub fn sethostname(pid: u32, hostname: &str) -> Result<(), i32> {
    let proc_ns = PROCESS_NS.lock();
    let ns = proc_ns.get(&pid).ok_or(-3i32)?;
    let uts_id = ns.uts_ns;
    drop(proc_ns);

    let mut uts_ns = UTS_NAMESPACES.lock();
    let uts = uts_ns.get_mut(&uts_id).ok_or(-22i32)?;
    uts.hostname = String::from(hostname);
    Ok(())
}

/// Get hostname from the UTS namespace of a process
pub fn gethostname(pid: u32) -> String {
    let proc_ns = PROCESS_NS.lock();
    if let Some(ns) = proc_ns.get(&pid) {
        let uts_ns = UTS_NAMESPACES.lock();
        if let Some(uts) = uts_ns.get(&ns.uts_ns) {
            return uts.hostname.clone();
        }
    }
    String::from("knoxos")
}

/// Get namespaces for a process
pub fn get_process_namespaces(pid: u32) -> ProcessNamespaces {
    PROCESS_NS
        .lock()
        .get(&pid)
        .cloned()
        .unwrap_or_else(ProcessNamespaces::default)
}

/// Copy parent namespaces to child (on fork)
pub fn inherit_namespaces(child_pid: u32, parent_pid: u32) {
    let parent_ns = PROCESS_NS
        .lock()
        .get(&parent_pid)
        .cloned()
        .unwrap_or_else(ProcessNamespaces::default);
    PROCESS_NS.lock().insert(child_pid, parent_ns);
}

/// Initialize namespace subsystem
pub fn init() {
    // Create the initial (default) namespaces
    NAMESPACES.lock().insert(
        1,
        Namespace {
            id: 1,
            ns_type: NamespaceType::Uts,
            owner_pid: 1,
            ref_count: 1,
        },
    );

    UTS_NAMESPACES.lock().insert(
        1,
        UtsNamespace {
            hostname: String::from("knoxos"),
            domainname: String::from("(none)"),
        },
    );

    PID_NAMESPACES.lock().insert(
        1,
        PidNamespace {
            id: 1,
            pid_map: BTreeMap::new(),
            next_pid: 100,
        },
    );

    MOUNT_NAMESPACES.lock().insert(
        1,
        MountNamespace {
            id: 1,
            mounts: vec![
                MountEntry {
                    source: String::from("none"),
                    target: String::from("/"),
                    fstype: String::from("rootfs"),
                    flags: 0,
                },
                MountEntry {
                    source: String::from("tmpfs"),
                    target: String::from("/tmp"),
                    fstype: String::from("tmpfs"),
                    flags: 0,
                },
                MountEntry {
                    source: String::from("proc"),
                    target: String::from("/proc"),
                    fstype: String::from("proc"),
                    flags: 0,
                },
                MountEntry {
                    source: String::from("sysfs"),
                    target: String::from("/sys"),
                    fstype: String::from("sysfs"),
                    flags: 0,
                },
                MountEntry {
                    source: String::from("devtmpfs"),
                    target: String::from("/dev"),
                    fstype: String::from("devtmpfs"),
                    flags: 0,
                },
                MountEntry {
                    source: String::from("tmpfs"),
                    target: String::from("/dev/shm"),
                    fstype: String::from("tmpfs"),
                    flags: 0,
                },
                MountEntry {
                    source: String::from("tmpfs"),
                    target: String::from("/run"),
                    fstype: String::from("tmpfs"),
                    flags: 0,
                },
            ],
        },
    );

    NET_NAMESPACES.lock().insert(
        1,
        NetNamespace {
            id: 1,
            interfaces: vec![String::from("lo"), String::from("eth0")],
            has_loopback: true,
        },
    );

    // Set initial process namespaces
    let default_ns = ProcessNamespaces::default();
    PROCESS_NS.lock().insert(0, default_ns.clone());
    PROCESS_NS.lock().insert(1, default_ns.clone());
    PROCESS_NS.lock().insert(2, default_ns);

    crate::serial_println!(
        "[KnoxOS] Namespaces initialized (mount, uts, ipc, pid, net, user, cgroup)"
    );
}
