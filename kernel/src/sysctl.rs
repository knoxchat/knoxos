/// System Information — /proc/sys and sysctl interface
///
/// Implements Linux-compatible sysctl interface:
///   - Hierarchical key-value configuration
///   - /proc/sys filesystem mapping
///   - kernel.*, vm.*, net.*, fs.* namespaces
///   - Runtime kernel parameter tuning
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Sysctl value types
#[derive(Debug, Clone)]
pub enum SysctlValue {
    Int(i64),
    UInt(u64),
    String(String),
    Bool(bool),
}

impl SysctlValue {
    pub fn as_string(&self) -> String {
        match self {
            SysctlValue::Int(v) => alloc::format!("{}", v),
            SysctlValue::UInt(v) => alloc::format!("{}", v),
            SysctlValue::String(v) => v.clone(),
            SysctlValue::Bool(v) => alloc::format!("{}", if *v { 1 } else { 0 }),
        }
    }

    pub fn from_str(s: &str, template: &SysctlValue) -> Option<SysctlValue> {
        match template {
            SysctlValue::Int(_) => s.trim().parse::<i64>().ok().map(SysctlValue::Int),
            SysctlValue::UInt(_) => s.trim().parse::<u64>().ok().map(SysctlValue::UInt),
            SysctlValue::String(_) => Some(SysctlValue::String(String::from(s.trim()))),
            SysctlValue::Bool(_) => match s.trim() {
                "0" | "false" | "no" => Some(SysctlValue::Bool(false)),
                "1" | "true" | "yes" => Some(SysctlValue::Bool(true)),
                _ => None,
            },
        }
    }
}

/// A sysctl entry
#[derive(Debug, Clone)]
pub struct SysctlEntry {
    pub key: String,
    pub value: SysctlValue,
    pub description: String,
    pub writable: bool,
}

/// Sysctl table
struct SysctlTable {
    entries: BTreeMap<String, SysctlEntry>,
}

lazy_static::lazy_static! {
    static ref SYSCTL: Mutex<SysctlTable> = Mutex::new(SysctlTable {
        entries: BTreeMap::new(),
    });
}

/// Register a sysctl parameter
pub fn register(key: &str, value: SysctlValue, description: &str, writable: bool) {
    let mut table = SYSCTL.lock();
    table.entries.insert(
        String::from(key),
        SysctlEntry {
            key: String::from(key),
            value,
            description: String::from(description),
            writable,
        },
    );
}

/// Read a sysctl value
pub fn get(key: &str) -> Option<SysctlValue> {
    let table = SYSCTL.lock();
    table.entries.get(key).map(|e| e.value.clone())
}

/// Write a sysctl value
pub fn set(key: &str, value: &str) -> Result<(), i32> {
    let mut table = SYSCTL.lock();
    let entry = table.entries.get_mut(key).ok_or(-2i32)?; // ENOENT

    if !entry.writable {
        return Err(-1); // EPERM
    }

    let new_value = SysctlValue::from_str(value, &entry.value).ok_or(-22i32)?; // EINVAL
    entry.value = new_value;
    Ok(())
}

/// List all sysctl keys matching a prefix
pub fn list(prefix: &str) -> Vec<(String, String)> {
    let table = SYSCTL.lock();
    table
        .entries
        .iter()
        .filter(|(k, _)| k.starts_with(prefix))
        .map(|(k, v)| (k.clone(), v.value.as_string()))
        .collect()
}

/// Get all entries (for /proc/sys dumping)
pub fn list_all() -> Vec<(String, String, String)> {
    let table = SYSCTL.lock();
    table
        .entries
        .iter()
        .map(|(k, v)| (k.clone(), v.value.as_string(), v.description.clone()))
        .collect()
}

/// Get a sysctl value as i64
pub fn get_int(key: &str) -> Option<i64> {
    match get(key)? {
        SysctlValue::Int(v) => Some(v),
        SysctlValue::UInt(v) => Some(v as i64),
        SysctlValue::Bool(v) => Some(if v { 1 } else { 0 }),
        _ => None,
    }
}

/// Get a sysctl value as bool
pub fn get_bool(key: &str) -> Option<bool> {
    match get(key)? {
        SysctlValue::Bool(v) => Some(v),
        SysctlValue::Int(v) => Some(v != 0),
        SysctlValue::UInt(v) => Some(v != 0),
        _ => None,
    }
}

/// Initialize with default Linux-compatible sysctl parameters
pub fn init() {
    // kernel.*
    register(
        "kernel.hostname",
        SysctlValue::String(String::from("knoxos")),
        "System hostname",
        true,
    );
    register(
        "kernel.domainname",
        SysctlValue::String(String::from("(none)")),
        "NIS domain name",
        true,
    );
    register(
        "kernel.ostype",
        SysctlValue::String(String::from("KnoxOS")),
        "Operating system type",
        false,
    );
    register(
        "kernel.osrelease",
        SysctlValue::String(String::from("0.7.0")),
        "Kernel release version",
        false,
    );
    register(
        "kernel.version",
        SysctlValue::String(String::from("#1 SMP")),
        "Kernel version string",
        false,
    );
    register(
        "kernel.panic",
        SysctlValue::Int(0),
        "Seconds to wait before reboot on panic (0=halt)",
        true,
    );
    register(
        "kernel.panic_on_oops",
        SysctlValue::Bool(false),
        "Panic on kernel oops",
        true,
    );
    register(
        "kernel.pid_max",
        SysctlValue::Int(32768),
        "Maximum PID value",
        true,
    );
    register(
        "kernel.threads-max",
        SysctlValue::Int(4096),
        "Maximum number of threads",
        true,
    );
    register(
        "kernel.shmmax",
        SysctlValue::UInt(33554432),
        "Max shared memory segment size",
        true,
    );
    register(
        "kernel.shmall",
        SysctlValue::UInt(2097152),
        "Max shared memory pages",
        true,
    );
    register(
        "kernel.msgmax",
        SysctlValue::Int(8192),
        "Max message size",
        true,
    );
    register(
        "kernel.msgmnb",
        SysctlValue::Int(16384),
        "Max message queue size",
        true,
    );
    register(
        "kernel.sem",
        SysctlValue::String(String::from("250 32000 32 128")),
        "Semaphore limits",
        true,
    );
    register(
        "kernel.core_pattern",
        SysctlValue::String(String::from("core.%p")),
        "Core dump filename pattern",
        true,
    );
    register(
        "kernel.core_uses_pid",
        SysctlValue::Bool(true),
        "Append PID to core filename",
        true,
    );
    register(
        "kernel.randomize_va_space",
        SysctlValue::Int(2),
        "ASLR mode (0=off, 1=stack, 2=full)",
        true,
    );
    register(
        "kernel.dmesg_restrict",
        SysctlValue::Bool(false),
        "Restrict dmesg to root",
        true,
    );
    register(
        "kernel.printk",
        SysctlValue::String(String::from("4 4 1 7")),
        "Printk log levels",
        true,
    );
    register(
        "kernel.sysrq",
        SysctlValue::Int(1),
        "SysRq enabled (bitmask)",
        true,
    );
    register(
        "kernel.ngroups_max",
        SysctlValue::Int(65536),
        "Max supplementary groups",
        false,
    );
    register(
        "kernel.tainted",
        SysctlValue::Int(0),
        "Kernel taint flags",
        false,
    );

    // vm.*
    register(
        "vm.swappiness",
        SysctlValue::Int(60),
        "How aggressively to swap",
        true,
    );
    register(
        "vm.dirty_ratio",
        SysctlValue::Int(20),
        "Percentage of RAM for dirty pages",
        true,
    );
    register(
        "vm.dirty_background_ratio",
        SysctlValue::Int(10),
        "Background dirty page ratio",
        true,
    );
    register(
        "vm.dirty_writeback_centisecs",
        SysctlValue::Int(500),
        "Writeback interval (centiseconds)",
        true,
    );
    register(
        "vm.dirty_expire_centisecs",
        SysctlValue::Int(3000),
        "Dirty page expiry (centiseconds)",
        true,
    );
    register(
        "vm.overcommit_memory",
        SysctlValue::Int(0),
        "Memory overcommit mode (0=heuristic, 1=always, 2=never)",
        true,
    );
    register(
        "vm.overcommit_ratio",
        SysctlValue::Int(50),
        "Overcommit ratio percentage",
        true,
    );
    register(
        "vm.oom_kill_allocating_task",
        SysctlValue::Bool(false),
        "Kill the allocating task on OOM",
        true,
    );
    register(
        "vm.panic_on_oom",
        SysctlValue::Bool(false),
        "Panic on OOM",
        true,
    );
    register(
        "vm.max_map_count",
        SysctlValue::Int(65530),
        "Max memory map areas per process",
        true,
    );
    register(
        "vm.mmap_min_addr",
        SysctlValue::UInt(65536),
        "Min address for mmap",
        true,
    );
    register(
        "vm.min_free_kbytes",
        SysctlValue::Int(16384),
        "Min free memory in KB",
        true,
    );
    register(
        "vm.vfs_cache_pressure",
        SysctlValue::Int(100),
        "VFS cache reclaim pressure",
        true,
    );

    // net.*
    register(
        "net.core.somaxconn",
        SysctlValue::Int(4096),
        "Max socket listen backlog",
        true,
    );
    register(
        "net.core.netdev_max_backlog",
        SysctlValue::Int(1000),
        "Max network device backlog",
        true,
    );
    register(
        "net.core.rmem_default",
        SysctlValue::Int(212992),
        "Default receive buffer size",
        true,
    );
    register(
        "net.core.rmem_max",
        SysctlValue::Int(212992),
        "Max receive buffer size",
        true,
    );
    register(
        "net.core.wmem_default",
        SysctlValue::Int(212992),
        "Default send buffer size",
        true,
    );
    register(
        "net.core.wmem_max",
        SysctlValue::Int(212992),
        "Max send buffer size",
        true,
    );
    register(
        "net.ipv4.ip_forward",
        SysctlValue::Bool(false),
        "Enable IP forwarding",
        true,
    );
    register(
        "net.ipv4.tcp_syncookies",
        SysctlValue::Bool(true),
        "Enable TCP SYN cookies",
        true,
    );
    register(
        "net.ipv4.tcp_max_syn_backlog",
        SysctlValue::Int(128),
        "Max SYN backlog",
        true,
    );
    register(
        "net.ipv4.tcp_fin_timeout",
        SysctlValue::Int(60),
        "TCP FIN timeout (seconds)",
        true,
    );
    register(
        "net.ipv4.tcp_keepalive_time",
        SysctlValue::Int(7200),
        "TCP keepalive time (seconds)",
        true,
    );
    register(
        "net.ipv4.tcp_keepalive_probes",
        SysctlValue::Int(9),
        "TCP keepalive probes",
        true,
    );
    register(
        "net.ipv4.tcp_keepalive_intvl",
        SysctlValue::Int(75),
        "TCP keepalive interval (seconds)",
        true,
    );
    register(
        "net.ipv4.tcp_tw_reuse",
        SysctlValue::Bool(false),
        "Reuse TIME_WAIT sockets",
        true,
    );
    register(
        "net.ipv4.ip_local_port_range",
        SysctlValue::String(String::from("32768 60999")),
        "Ephemeral port range",
        true,
    );
    register(
        "net.ipv4.icmp_echo_ignore_all",
        SysctlValue::Bool(false),
        "Ignore all ICMP echo",
        true,
    );

    // fs.*
    register(
        "fs.file-max",
        SysctlValue::Int(65536),
        "Max open files system-wide",
        true,
    );
    register(
        "fs.file-nr",
        SysctlValue::String(String::from("0 0 65536")),
        "File handle info (allocated, free, max)",
        false,
    );
    register(
        "fs.inode-nr",
        SysctlValue::String(String::from("0 0")),
        "Inode info (allocated, free)",
        false,
    );
    register(
        "fs.nr_open",
        SysctlValue::Int(1048576),
        "Max fd number per process",
        true,
    );
    register(
        "fs.pipe-max-size",
        SysctlValue::Int(1048576),
        "Max pipe buffer size",
        true,
    );
    register(
        "fs.protected_hardlinks",
        SysctlValue::Bool(true),
        "Protect hard links",
        true,
    );
    register(
        "fs.protected_symlinks",
        SysctlValue::Bool(true),
        "Protect symlinks",
        true,
    );
    register(
        "fs.aio-max-nr",
        SysctlValue::UInt(65536),
        "Max AIO requests",
        true,
    );

    serial_println!("[KnoxOS] Sysctl subsystem initialized ({} parameters)", {
        let t = SYSCTL.lock();
        t.entries.len()
    });
}
