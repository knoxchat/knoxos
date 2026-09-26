/// Namespaces - Linux namespace isolation for containers
/// Compatible with Linux namespace types (clone flags)
/// Provides resource isolation: PID, mount, network, user, UTS, IPC, cgroup, time
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
    /// Time namespace (CLONE_NEWTIME) — CLOCK_MONOTONIC / CLOCK_BOOTTIME offsets
    Time = 0x00000080,
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
    pub time_ns: u64,
    /// Host (kuid) user id; `getuid` maps this through `user_ns`.
    pub host_uid: u32,
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
            time_ns: 1,
            host_uid: 0,
        }
    }
}

/// A single uid_map row: `inside`..`inside+count` ↔ `outside`..`outside+count`.
#[derive(Debug, Clone, Copy)]
pub struct UidMap {
    pub inside: u32,
    pub outside: u32,
    pub count: u32,
}

/// User namespace data (uid/gid mappings)
#[derive(Debug, Clone)]
pub struct UserNamespace {
    pub id: u64,
    pub uid_map: Vec<UidMap>,
}

/// Cgroup namespace data — virtualizes the cgroup path view.
#[derive(Debug, Clone)]
pub struct CgroupNamespace {
    pub id: u64,
    pub cgroups: Vec<String>,
}

/// Time namespace data — offsets applied to monotonic / boottime clocks.
#[derive(Debug, Clone)]
pub struct TimeNamespace {
    pub id: u64,
    pub monotonic_offset_ns: i64,
    pub boottime_offset_ns: i64,
}

/// Global namespace registry
lazy_static::lazy_static! {
    static ref NAMESPACES: Mutex<BTreeMap<u64, Namespace>> = Mutex::new(BTreeMap::new());
    static ref UTS_NAMESPACES: Mutex<BTreeMap<u64, UtsNamespace>> = Mutex::new(BTreeMap::new());
    static ref PID_NAMESPACES: Mutex<BTreeMap<u64, PidNamespace>> = Mutex::new(BTreeMap::new());
    static ref MOUNT_NAMESPACES: Mutex<BTreeMap<u64, MountNamespace>> = Mutex::new(BTreeMap::new());
    static ref NET_NAMESPACES: Mutex<BTreeMap<u64, NetNamespace>> = Mutex::new(BTreeMap::new());
    static ref USER_NAMESPACES: Mutex<BTreeMap<u64, UserNamespace>> = Mutex::new(BTreeMap::new());
    static ref CGROUP_NAMESPACES: Mutex<BTreeMap<u64, CgroupNamespace>> = Mutex::new(BTreeMap::new());
    static ref TIME_NAMESPACES: Mutex<BTreeMap<u64, TimeNamespace>> = Mutex::new(BTreeMap::new());
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
        NamespaceType::User => {
            USER_NAMESPACES.lock().insert(
                id,
                UserNamespace {
                    id,
                    uid_map: Vec::new(),
                },
            );
        }
        NamespaceType::Cgroup => {
            CGROUP_NAMESPACES.lock().insert(
                id,
                CgroupNamespace {
                    id,
                    cgroups: vec![String::from("/")],
                },
            );
        }
        NamespaceType::Time => {
            TIME_NAMESPACES.lock().insert(
                id,
                TimeNamespace {
                    id,
                    monotonic_offset_ns: 0,
                    boottime_offset_ns: 0,
                },
            );
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
        let parent_id = ns.mount_ns;
        let copied = MOUNT_NAMESPACES
            .lock()
            .get(&parent_id)
            .map(|m| m.mounts.clone())
            .unwrap_or_default();
        let new_id = create_namespace(NamespaceType::Mount, pid)?;
        if let Some(m) = MOUNT_NAMESPACES.lock().get_mut(&new_id) {
            m.mounts = copied;
        }
        ns.mount_ns = new_id;
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
        let kuid = ns.host_uid;
        let new_id = create_namespace(NamespaceType::User, pid)?;
        if let Some(u) = USER_NAMESPACES.lock().get_mut(&new_id) {
            u.uid_map.push(UidMap {
                inside: 0,
                outside: kuid,
                count: 1,
            });
        }
        ns.user_ns = new_id;
    }
    if flags & NamespaceType::Cgroup as u32 != 0 {
        ns.cgroup_ns = create_namespace(NamespaceType::Cgroup, pid)?;
    }
    if flags & NamespaceType::Time as u32 != 0 {
        let parent_id = ns.time_ns;
        let copied = TIME_NAMESPACES.lock().get(&parent_id).cloned();
        let new_id = create_namespace(NamespaceType::Time, pid)?;
        if let Some(src) = copied {
            if let Some(dst) = TIME_NAMESPACES.lock().get_mut(&new_id) {
                dst.monotonic_offset_ns = src.monotonic_offset_ns;
                dst.boottime_offset_ns = src.boottime_offset_ns;
            }
        }
        ns.time_ns = new_id;
    }
    drop(proc_ns);

    if flags & NamespaceType::Pid as u32 != 0 {
        crate::pidns::unshare_newpid(pid)?;
    }

    Ok(())
}

/// Set hostname in the UTS namespace of a process
pub fn sethostname(pid: u32, hostname: &str) -> Result<(), i32> {
    let proc_ns = PROCESS_NS.lock();
    let uts_id = proc_ns.get(&pid).map(|n| n.uts_ns).unwrap_or(1);
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

/// Record a mount in `pid`'s mount namespace (bind or otherwise).
pub fn add_mount(
    pid: u32,
    source: &str,
    target: &str,
    fstype: &str,
    flags: u32,
) -> Result<(), i32> {
    let ns_id = PROCESS_NS.lock().get(&pid).map(|n| n.mount_ns).unwrap_or(1);
    let mut mounts = MOUNT_NAMESPACES.lock();
    let ns = mounts.entry(ns_id).or_insert_with(|| MountNamespace {
        id: ns_id,
        mounts: Vec::new(),
    });
    if ns.mounts.iter().any(|m| m.target == target) {
        return Err(-16); // EBUSY
    }
    ns.mounts.push(MountEntry {
        source: String::from(source),
        target: String::from(target),
        fstype: String::from(fstype),
        flags,
    });
    Ok(())
}

/// Bind-mount `source` at `target` in `pid`'s mount namespace.
pub fn bind_mount(pid: u32, source: &str, target: &str) -> Result<(), i32> {
    add_mount(pid, source, target, "bind", 0x1000)
}

/// Whether `pid`'s mount namespace lists `target`.
pub fn has_mount(pid: u32, target: &str) -> bool {
    let ns_id = PROCESS_NS.lock().get(&pid).map(|n| n.mount_ns).unwrap_or(1);
    MOUNT_NAMESPACES
        .lock()
        .get(&ns_id)
        .map(|ns| ns.mounts.iter().any(|m| m.target == target))
        .unwrap_or(false)
}

/// Add a virtual interface name to `pid`'s network namespace.
pub fn add_net_interface(pid: u32, name: &str) -> Result<(), i32> {
    let ns_id = PROCESS_NS.lock().get(&pid).map(|n| n.net_ns).unwrap_or(1);
    let mut nets = NET_NAMESPACES.lock();
    let ns = nets.entry(ns_id).or_insert_with(|| NetNamespace {
        id: ns_id,
        interfaces: Vec::new(),
        has_loopback: false,
    });
    if ns.interfaces.iter().any(|i| i == name) {
        return Err(-17); // EEXIST
    }
    ns.interfaces.push(String::from(name));
    if name == "lo" {
        ns.has_loopback = true;
    }
    Ok(())
}

/// Add a cgroup path to `pid`'s cgroup namespace.
pub fn add_cgroup(pid: u32, path: &str) -> Result<(), i32> {
    let ns_id = PROCESS_NS
        .lock()
        .get(&pid)
        .map(|n| n.cgroup_ns)
        .unwrap_or(1);
    let mut cgroups = CGROUP_NAMESPACES.lock();
    let ns = cgroups.entry(ns_id).or_insert_with(|| CgroupNamespace {
        id: ns_id,
        cgroups: Vec::new(),
    });
    if ns.cgroups.iter().any(|c| c == path) {
        return Err(-17); // EEXIST
    }
    ns.cgroups.push(String::from(path));
    Ok(())
}

/// Whether `pid`'s cgroup namespace lists `path`.
pub fn has_cgroup(pid: u32, path: &str) -> bool {
    let ns_id = PROCESS_NS
        .lock()
        .get(&pid)
        .map(|n| n.cgroup_ns)
        .unwrap_or(1);
    CGROUP_NAMESPACES
        .lock()
        .get(&ns_id)
        .map(|ns| ns.cgroups.iter().any(|c| c == path))
        .unwrap_or(false)
}

/// Whether `pid`'s network namespace lists `name`.
pub fn has_net_interface(pid: u32, name: &str) -> bool {
    let ns_id = PROCESS_NS.lock().get(&pid).map(|n| n.net_ns).unwrap_or(1);
    NET_NAMESPACES
        .lock()
        .get(&ns_id)
        .map(|ns| ns.interfaces.iter().any(|i| i == name))
        .unwrap_or(false)
}

fn map_kuid(ns_id: u64, kuid: u32) -> u32 {
    if ns_id == 1 {
        return kuid;
    }
    let nss = USER_NAMESPACES.lock();
    let Some(ns) = nss.get(&ns_id) else {
        return kuid;
    };
    for m in &ns.uid_map {
        if kuid >= m.outside && kuid < m.outside.saturating_add(m.count) {
            return m.inside + (kuid - m.outside);
        }
    }
    65534 // overflowuid
}

/// Host (kuid) for `pid`, then mapped through its user namespace.
pub fn ns_uid(pid: u32) -> u32 {
    let (user_ns, host_uid) = {
        let proc_ns = PROCESS_NS.lock();
        match proc_ns.get(&pid) {
            Some(ns) => (ns.user_ns, ns.host_uid),
            None => {
                drop(proc_ns);
                let kuid = crate::process::PROCESS_TABLE
                    .lock()
                    .get_process(pid)
                    .map(|p| p.uid)
                    .unwrap_or(0);
                return kuid;
            }
        }
    };
    map_kuid(user_ns, host_uid)
}

/// Set the host uid recorded for `pid` (used by tests and `setuid`).
pub fn set_host_uid(pid: u32, uid: u32) {
    PROCESS_NS.lock().entry(pid).or_default().host_uid = uid;
}

fn time_ns_id(pid: u32) -> u64 {
    PROCESS_NS.lock().get(&pid).map(|n| n.time_ns).unwrap_or(1)
}

/// Offset applied to `clock_id` in `pid`'s time namespace (0 if not namespaced).
pub fn time_offset_ns(pid: u32, clock_id: u32) -> i64 {
    let ns_id = time_ns_id(pid);
    let nss = TIME_NAMESPACES.lock();
    let Some(ns) = nss.get(&ns_id) else {
        return 0;
    };
    match clock_id {
        crate::rtc::CLOCK_MONOTONIC | crate::rtc::CLOCK_MONOTONIC_COARSE => ns.monotonic_offset_ns,
        crate::rtc::CLOCK_BOOTTIME => ns.boottime_offset_ns,
        _ => 0,
    }
}

/// Replace the monotonic / boottime offsets in `pid`'s time namespace.
pub fn set_time_offsets(pid: u32, monotonic_ns: i64, boottime_ns: i64) -> Result<(), i32> {
    let ns_id = time_ns_id(pid);
    let mut nss = TIME_NAMESPACES.lock();
    let ns = nss.get_mut(&ns_id).ok_or(-22i32)?;
    ns.monotonic_offset_ns = monotonic_ns;
    ns.boottime_offset_ns = boottime_ns;
    Ok(())
}

fn add_ns_offset(ts: crate::rtc::Timespec, offset_ns: i64) -> crate::rtc::Timespec {
    let total = ts
        .tv_sec
        .saturating_mul(1_000_000_000)
        .saturating_add(ts.tv_nsec)
        .saturating_add(offset_ns);
    if total < 0 {
        crate::rtc::Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        }
    } else {
        crate::rtc::Timespec {
            tv_sec: total / 1_000_000_000,
            tv_nsec: total % 1_000_000_000,
        }
    }
}

/// `clock_gettime` as seen from `pid`'s time namespace.
pub fn namespaced_clock_gettime(pid: u32, clock_id: u32) -> crate::rtc::Timespec {
    add_ns_offset(
        crate::rtc::clock_gettime(clock_id),
        time_offset_ns(pid, clock_id),
    )
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

    USER_NAMESPACES.lock().insert(
        1,
        UserNamespace {
            id: 1,
            uid_map: Vec::new(),
        },
    );

    CGROUP_NAMESPACES.lock().insert(
        1,
        CgroupNamespace {
            id: 1,
            cgroups: vec![
                String::from("/"),
                String::from("/system.slice"),
                String::from("/user.slice"),
            ],
        },
    );

    TIME_NAMESPACES.lock().insert(
        1,
        TimeNamespace {
            id: 1,
            monotonic_offset_ns: 0,
            boottime_offset_ns: 0,
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
    let _ = uts_isolation_self_test();
    let _ = mount_isolation_self_test();
    let _ = net_isolation_self_test();
    let _ = user_isolation_self_test();
    let _ = ipc_isolation_self_test();
    let _ = cgroup_isolation_self_test();
    let _ = time_isolation_self_test();
}

pub const GATE_L2_MARKER: &str = "GATE_L2 uts ns";
const GATE_L2_PID: u32 = 0x0000_4C02;

/// Child `unshare(CLONE_NEWUTS)` + `sethostname` must not change the parent's hostname.
pub fn uts_isolation_self_test() -> bool {
    inherit_namespaces(GATE_L2_PID, 1);
    let parent_before = gethostname(1);
    if unshare(GATE_L2_PID, NamespaceType::Uts as u32).is_err() {
        crate::serial_println!("[ns] Gate L2 FAILED: unshare");
        return false;
    }
    if sethostname(GATE_L2_PID, "gate-l2").is_err() {
        crate::serial_println!("[ns] Gate L2 FAILED: sethostname");
        return false;
    }
    let child = gethostname(GATE_L2_PID);
    let parent_after = gethostname(1);
    if child != "gate-l2" || parent_after != parent_before {
        crate::serial_println!(
            "[ns] Gate L2 FAILED: child={} parent={} was={}",
            child,
            parent_after,
            parent_before
        );
        return false;
    }
    crate::serial_println!("[ns] {}", GATE_L2_MARKER);
    true
}

pub const GATE_N1_MARKER: &str = "GATE_N1 mount ns";
const GATE_N1_PID: u32 = 0x0000_4E01;
const GATE_N1_TARGET: &str = "/tmp/gate_n1";

/// Child `unshare(CLONE_NEWNS)` + bind mount must not appear in the parent.
pub fn mount_isolation_self_test() -> bool {
    inherit_namespaces(GATE_N1_PID, 1);
    if unshare(GATE_N1_PID, NamespaceType::Mount as u32).is_err() {
        crate::serial_println!("[ns] Gate N1 FAILED: unshare");
        return false;
    }
    if bind_mount(GATE_N1_PID, "/tmp", GATE_N1_TARGET).is_err() {
        crate::serial_println!("[ns] Gate N1 FAILED: bind_mount");
        return false;
    }
    let child = has_mount(GATE_N1_PID, GATE_N1_TARGET);
    let parent = has_mount(1, GATE_N1_TARGET);
    if !child || parent {
        crate::serial_println!("[ns] Gate N1 FAILED: child={} parent={}", child, parent);
        return false;
    }
    crate::serial_println!("[ns] {}", GATE_N1_MARKER);
    true
}

pub const GATE_O1_MARKER: &str = "GATE_O1 net ns";
const GATE_O1_PID: u32 = 0x0000_4F01;

/// Child `unshare(CLONE_NEWNET)` must not see the parent's `eth0`, and a
/// child-only interface must not appear in the parent.
pub fn net_isolation_self_test() -> bool {
    inherit_namespaces(GATE_O1_PID, 1);
    if !has_net_interface(1, "eth0") {
        crate::serial_println!("[ns] Gate O1 FAILED: parent missing eth0");
        return false;
    }
    if unshare(GATE_O1_PID, NamespaceType::Net as u32).is_err() {
        crate::serial_println!("[ns] Gate O1 FAILED: unshare");
        return false;
    }
    if has_net_interface(GATE_O1_PID, "eth0") {
        crate::serial_println!("[ns] Gate O1 FAILED: child still has eth0");
        return false;
    }
    if add_net_interface(GATE_O1_PID, "veth0").is_err() {
        crate::serial_println!("[ns] Gate O1 FAILED: add veth0");
        return false;
    }
    let child = has_net_interface(GATE_O1_PID, "veth0");
    let parent = has_net_interface(1, "veth0");
    if !child || parent {
        crate::serial_println!("[ns] Gate O1 FAILED: child={} parent={}", child, parent);
        return false;
    }
    crate::serial_println!("[ns] {}", GATE_O1_MARKER);
    true
}

pub const GATE_P1_MARKER: &str = "GATE_P1 user ns";
const GATE_P1_PARENT: u32 = 0x0000_5001;
const GATE_P1_CHILD: u32 = 0x0000_5002;

/// Child `unshare(CLONE_NEWUSER)` sees uid 0; the parent uid is unchanged.
pub fn user_isolation_self_test() -> bool {
    inherit_namespaces(GATE_P1_PARENT, 1);
    set_host_uid(GATE_P1_PARENT, 1000);
    inherit_namespaces(GATE_P1_CHILD, GATE_P1_PARENT);
    if ns_uid(GATE_P1_PARENT) != 1000 || ns_uid(GATE_P1_CHILD) != 1000 {
        crate::serial_println!(
            "[ns] Gate P1 FAILED: before unshare parent={} child={}",
            ns_uid(GATE_P1_PARENT),
            ns_uid(GATE_P1_CHILD)
        );
        return false;
    }
    if unshare(GATE_P1_CHILD, NamespaceType::User as u32).is_err() {
        crate::serial_println!("[ns] Gate P1 FAILED: unshare");
        return false;
    }
    let child = ns_uid(GATE_P1_CHILD);
    let parent = ns_uid(GATE_P1_PARENT);
    if child != 0 || parent != 1000 {
        crate::serial_println!(
            "[ns] Gate P1 FAILED: child={} parent={} (want 0 / 1000)",
            child,
            parent
        );
        return false;
    }
    crate::serial_println!("[ns] {}", GATE_P1_MARKER);
    true
}

pub const GATE_Q1_MARKER: &str = "GATE_Q1 ipc ns";
const GATE_Q1_PID: u32 = 0x0000_5101;
const GATE_Q1_KEY: u32 = 0x0000_51C1;
const GATE_Q1_PRIV: u32 = 0x0000_51C2;

/// Child `unshare(CLONE_NEWIPC)` must not see the parent's SysV shm key,
/// and a child-only key must not appear in the parent.
pub fn ipc_isolation_self_test() -> bool {
    inherit_namespaces(GATE_Q1_PID, 1);
    let parent_ns = get_process_namespaces(1).ipc_ns;
    let Ok(parent_id) =
        crate::shm::shmget_in_ns(parent_ns, 1, GATE_Q1_KEY, 4096, crate::shm::IPC_CREAT)
    else {
        crate::serial_println!("[ns] Gate Q1 FAILED: parent shmget");
        return false;
    };
    if unshare(GATE_Q1_PID, NamespaceType::Ipc as u32).is_err() {
        crate::serial_println!("[ns] Gate Q1 FAILED: unshare");
        return false;
    }
    let child_ns = get_process_namespaces(GATE_Q1_PID).ipc_ns;
    if child_ns == parent_ns {
        crate::serial_println!("[ns] Gate Q1 FAILED: child still in parent ipc ns");
        return false;
    }
    if crate::shm::has_shm_key(child_ns, GATE_Q1_KEY) {
        crate::serial_println!("[ns] Gate Q1 FAILED: child still sees parent key");
        return false;
    }
    let Ok(child_id) = crate::shm::shmget_in_ns(
        child_ns,
        GATE_Q1_PID,
        GATE_Q1_KEY,
        4096,
        crate::shm::IPC_CREAT,
    ) else {
        crate::serial_println!("[ns] Gate Q1 FAILED: child shmget");
        return false;
    };
    if child_id == parent_id {
        crate::serial_println!("[ns] Gate Q1 FAILED: child reused parent shmid");
        return false;
    }
    if crate::shm::shmget_in_ns(
        child_ns,
        GATE_Q1_PID,
        GATE_Q1_PRIV,
        4096,
        crate::shm::IPC_CREAT,
    )
    .is_err()
    {
        crate::serial_println!("[ns] Gate Q1 FAILED: child private shmget");
        return false;
    }
    if crate::shm::has_shm_key(parent_ns, GATE_Q1_PRIV) {
        crate::serial_println!("[ns] Gate Q1 FAILED: parent sees child key");
        return false;
    }
    if !crate::shm::has_shm_key(parent_ns, GATE_Q1_KEY) {
        crate::serial_println!("[ns] Gate Q1 FAILED: parent lost its key");
        return false;
    }
    crate::serial_println!("[ns] {}", GATE_Q1_MARKER);
    true
}

pub const GATE_R1_MARKER: &str = "GATE_R1 cgroup ns";
const GATE_R1_PID: u32 = 0x0000_5201;
const GATE_R1_PATH: &str = "/gate_r1";

/// Child `unshare(CLONE_NEWCGROUP)` must not see the parent's extra cgroups,
/// and a child-only cgroup must not appear in the parent.
pub fn cgroup_isolation_self_test() -> bool {
    inherit_namespaces(GATE_R1_PID, 1);
    if !has_cgroup(1, "/system.slice") {
        crate::serial_println!("[ns] Gate R1 FAILED: parent missing /system.slice");
        return false;
    }
    if unshare(GATE_R1_PID, NamespaceType::Cgroup as u32).is_err() {
        crate::serial_println!("[ns] Gate R1 FAILED: unshare");
        return false;
    }
    if has_cgroup(GATE_R1_PID, "/system.slice") {
        crate::serial_println!("[ns] Gate R1 FAILED: child still has /system.slice");
        return false;
    }
    if add_cgroup(GATE_R1_PID, GATE_R1_PATH).is_err() {
        crate::serial_println!("[ns] Gate R1 FAILED: add child cgroup");
        return false;
    }
    let child = has_cgroup(GATE_R1_PID, GATE_R1_PATH);
    let parent = has_cgroup(1, GATE_R1_PATH);
    if !child || parent {
        crate::serial_println!("[ns] Gate R1 FAILED: child={} parent={}", child, parent);
        return false;
    }
    if !has_cgroup(1, "/system.slice") {
        crate::serial_println!("[ns] Gate R1 FAILED: parent lost /system.slice");
        return false;
    }
    crate::serial_println!("[ns] {}", GATE_R1_MARKER);
    true
}

pub const GATE_S1_MARKER: &str = "GATE_S1 time ns";
const GATE_S1_PID: u32 = 0x0000_5301;
const GATE_S1_OFFSET_NS: i64 = 3_600_000_000_000; // +1 hour

/// Child `unshare(CLONE_NEWTIME)` + monotonic offset must not change the
/// parent's CLOCK_MONOTONIC, and CLOCK_MONOTONIC_RAW stays unoffset.
pub fn time_isolation_self_test() -> bool {
    inherit_namespaces(GATE_S1_PID, 1);
    let parent_before = namespaced_clock_gettime(1, crate::rtc::CLOCK_MONOTONIC);
    if unshare(GATE_S1_PID, NamespaceType::Time as u32).is_err() {
        crate::serial_println!("[ns] Gate S1 FAILED: unshare");
        return false;
    }
    if time_ns_id(GATE_S1_PID) == time_ns_id(1) {
        crate::serial_println!("[ns] Gate S1 FAILED: child still in parent time ns");
        return false;
    }
    if set_time_offsets(GATE_S1_PID, GATE_S1_OFFSET_NS, GATE_S1_OFFSET_NS).is_err() {
        crate::serial_println!("[ns] Gate S1 FAILED: set offsets");
        return false;
    }
    let child = namespaced_clock_gettime(GATE_S1_PID, crate::rtc::CLOCK_MONOTONIC);
    let parent_after = namespaced_clock_gettime(1, crate::rtc::CLOCK_MONOTONIC);
    let child_raw = namespaced_clock_gettime(GATE_S1_PID, crate::rtc::CLOCK_MONOTONIC_RAW);
    if child.tv_sec < parent_before.tv_sec + 3600 {
        crate::serial_println!(
            "[ns] Gate S1 FAILED: child monotonic {} (want >= {})",
            child.tv_sec,
            parent_before.tv_sec + 3600
        );
        return false;
    }
    if parent_after.tv_sec >= parent_before.tv_sec + 3600 {
        crate::serial_println!(
            "[ns] Gate S1 FAILED: parent monotonic leaked offset {}",
            parent_after.tv_sec
        );
        return false;
    }
    if child_raw.tv_sec >= parent_before.tv_sec + 3600 {
        crate::serial_println!(
            "[ns] Gate S1 FAILED: CLOCK_MONOTONIC_RAW was namespaced {}",
            child_raw.tv_sec
        );
        return false;
    }
    crate::serial_println!("[ns] {}", GATE_S1_MARKER);
    true
}
