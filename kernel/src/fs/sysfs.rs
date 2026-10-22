use alloc::format;
/// sysfs — Linux-compatible /sys filesystem
/// Exposes kernel/hardware information through a virtual filesystem
///
/// Structure:
///   /sys/kernel/      — Kernel parameters and info
///   /sys/devices/     — Device hierarchy
///   /sys/class/       — Device classes (net, block, tty)
///   /sys/fs/          — Filesystem information
///   /sys/module/      — Loaded kernel modules
///   /sys/power/       — Power management
///   /sys/firmware/    — Firmware interfaces
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Read a sysfs file
pub fn read_sysfs(path: &str) -> Option<String> {
    let path = path.trim_start_matches("/sys/");
    let parts: Vec<&str> = path.split('/').collect();

    if parts.is_empty() {
        return None;
    }

    match parts[0] {
        "kernel" => read_kernel_sysfs(&parts[1..]),
        "devices" => read_devices_sysfs(&parts[1..]),
        "class" => read_class_sysfs(&parts[1..]),
        "fs" => read_fs_sysfs(&parts[1..]),
        "module" => read_module_sysfs(&parts[1..]),
        "power" => read_power_sysfs(&parts[1..]),
        "firmware" => read_firmware_sysfs(&parts[1..]),
        _ => None,
    }
}

/// List a sysfs directory
pub fn list_sysfs(path: &str) -> Option<Vec<String>> {
    let path = path.trim_start_matches("/sys");
    let path = path.trim_start_matches('/');

    if path.is_empty() {
        return Some(alloc::vec![
            String::from("kernel"),
            String::from("devices"),
            String::from("class"),
            String::from("fs"),
            String::from("module"),
            String::from("power"),
            String::from("firmware"),
        ]);
    }

    let parts: Vec<&str> = path.split('/').collect();

    match parts[0] {
        "kernel" => list_kernel_sysfs(&parts[1..]),
        "devices" => list_devices_sysfs(&parts[1..]),
        "class" => list_class_sysfs(&parts[1..]),
        "fs" => list_fs_sysfs(&parts[1..]),
        "module" => list_module_sysfs(&parts[1..]),
        "power" => Some(alloc::vec![
            String::from("state"),
            String::from("wakeup_count"),
        ]),
        _ => None,
    }
}

// ─── /sys/kernel ─────────────────────────────────────────────────────

fn read_kernel_sysfs(parts: &[&str]) -> Option<String> {
    if parts.is_empty() {
        return None;
    }
    match parts[0] {
        "hostname" => {
            let pid = crate::scheduler::current_pid().unwrap_or(1);
            Some(crate::namespaces::gethostname(pid))
        }
        "osrelease" => Some(String::from("0.6.0-knoxos")),
        "ostype" => Some(String::from("KnoxOS")),
        "version" => Some(String::from("#1 SMP PREEMPT_DYNAMIC")),
        "ngroups_max" => Some(String::from("65536")),
        "pid_max" => Some(String::from("32768")),
        "threads-max" => Some(String::from("4096")),
        "random" => {
            if parts.len() > 1 {
                match parts[1] {
                    "entropy_avail" => Some(String::from("256")),
                    "poolsize" => Some(String::from("4096")),
                    "uuid" => {
                        // Generate a pseudo-random UUID
                        let tsc = crate::arch_compat::read_tsc();
                        Some(format!(
                            "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
                            tsc as u32,
                            (tsc >> 32) as u16,
                            (tsc >> 48) as u16 & 0xfff,
                            ((tsc >> 16) as u16 & 0x3fff) | 0x8000,
                            tsc & 0xffffffffffff,
                        ))
                    }
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn list_kernel_sysfs(parts: &[&str]) -> Option<Vec<String>> {
    if parts.is_empty() {
        return Some(alloc::vec![
            String::from("hostname"),
            String::from("osrelease"),
            String::from("ostype"),
            String::from("version"),
            String::from("ngroups_max"),
            String::from("pid_max"),
            String::from("threads-max"),
            String::from("random"),
        ]);
    }
    match parts[0] {
        "random" => Some(alloc::vec![
            String::from("entropy_avail"),
            String::from("poolsize"),
            String::from("uuid"),
        ]),
        _ => None,
    }
}

// ─── /sys/devices ────────────────────────────────────────────────────

fn read_devices_sysfs(parts: &[&str]) -> Option<String> {
    if parts.is_empty() {
        return None;
    }
    match parts[0] {
        "system" => {
            if parts.len() > 1 {
                match parts[1] {
                    "cpu" => {
                        if parts.len() > 2 {
                            match parts[2] {
                                "present" => Some(String::from("0-1")),
                                "online" => Some(String::from("0-1")),
                                "possible" => Some(String::from("0-1")),
                                "modalias" => Some(String::from(
                                    "x86cpu:vendor:0000:family:0006:model:003A:feature:,0000,",
                                )),
                                _ => None,
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            } else {
                None
            }
        }
        "platform" => Some(String::from("platform devices")),
        "pci0000:00" => Some(String::from("PCI bus 0000:00")),
        _ => None,
    }
}

fn list_devices_sysfs(parts: &[&str]) -> Option<Vec<String>> {
    if parts.is_empty() {
        return Some(alloc::vec![
            String::from("system"),
            String::from("platform"),
            String::from("pci0000:00"),
        ]);
    }
    match parts[0] {
        "system" => {
            if parts.len() > 1 && parts[1] == "cpu" {
                Some(alloc::vec![
                    String::from("present"),
                    String::from("online"),
                    String::from("possible"),
                    String::from("cpu0"),
                    String::from("cpu1"),
                ])
            } else {
                Some(alloc::vec![
                    String::from("cpu"),
                    String::from("clocksource"),
                    String::from("memory"),
                ])
            }
        }
        _ => None,
    }
}

// ─── /sys/class ──────────────────────────────────────────────────────

fn read_class_sysfs(parts: &[&str]) -> Option<String> {
    if parts.len() < 2 {
        return None;
    }
    match parts[0] {
        "net" => match parts[1] {
            "lo" => {
                if parts.len() > 2 {
                    match parts[2] {
                        "address" => Some(String::from("00:00:00:00:00:00")),
                        "mtu" => Some(String::from("65536")),
                        "type" => Some(String::from("772")),
                        "operstate" => Some(String::from("unknown")),
                        "speed" => Some(String::from("-1")),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            "eth0" => {
                if parts.len() > 2 {
                    match parts[2] {
                        "address" => Some(String::from("52:54:00:12:34:56")),
                        "mtu" => Some(String::from("1500")),
                        "type" => Some(String::from("1")),
                        "operstate" => Some(String::from("up")),
                        "speed" => Some(String::from("1000")),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        },
        "block" => {
            match parts[1] {
                "vda" => {
                    if parts.len() > 2 {
                        match parts[2] {
                            "size" => Some(String::from("2097152")), // 1GB in 512-byte sectors
                            "queue" => Some(String::from("scheduler: [none]")),
                            _ => None,
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        "tty" => Some(String::from("tty device")),
        _ => None,
    }
}

fn list_class_sysfs(parts: &[&str]) -> Option<Vec<String>> {
    if parts.is_empty() {
        return Some(alloc::vec![
            String::from("net"),
            String::from("block"),
            String::from("tty"),
            String::from("input"),
            String::from("sound"),
            String::from("graphics"),
        ]);
    }
    match parts[0] {
        "net" => {
            if parts.len() > 1 {
                Some(alloc::vec![
                    String::from("address"),
                    String::from("mtu"),
                    String::from("type"),
                    String::from("operstate"),
                    String::from("speed"),
                ])
            } else {
                Some(alloc::vec![String::from("lo"), String::from("eth0"),])
            }
        }
        "block" => Some(alloc::vec![String::from("vda")]),
        "tty" => Some(alloc::vec![
            String::from("tty0"),
            String::from("ttyS0"),
            String::from("console"),
        ]),
        _ => None,
    }
}

// ─── /sys/fs ─────────────────────────────────────────────────────────

fn read_fs_sysfs(parts: &[&str]) -> Option<String> {
    if parts.is_empty() {
        return None;
    }
    match parts[0] {
        "cgroup" => Some(String::from("cgroup v2")),
        "ext4" => {
            if parts.len() > 1 {
                match parts[1] {
                    "features" => Some(String::from("ext4 filesystem support")),
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn list_fs_sysfs(parts: &[&str]) -> Option<Vec<String>> {
    if parts.is_empty() {
        return Some(alloc::vec![
            String::from("cgroup"),
            String::from("ext4"),
            String::from("fuse"),
        ]);
    }
    None
}

// ─── /sys/module ─────────────────────────────────────────────────────

fn read_module_sysfs(parts: &[&str]) -> Option<String> {
    if parts.is_empty() {
        return None;
    }
    // Check loaded modules
    let modules = crate::modules::list_modules();
    for module in &modules {
        if module.name == parts[0] {
            if parts.len() > 1 {
                match parts[1] {
                    "version" => return Some(module.version.clone()),
                    "description" => return Some(module.description.clone()),
                    "refcnt" => return Some(format!("{}", module.ref_count)),
                    _ => return None,
                }
            }
            return Some(format!(
                "{}: {} ({})",
                module.name, module.description, module.version
            ));
        }
    }
    None
}

fn list_module_sysfs(parts: &[&str]) -> Option<Vec<String>> {
    if parts.is_empty() {
        let modules = crate::modules::list_modules();
        Some(modules.iter().map(|m| m.name.clone()).collect())
    } else {
        Some(alloc::vec![
            String::from("version"),
            String::from("description"),
            String::from("refcnt"),
            String::from("parameters"),
        ])
    }
}

// ─── /sys/power ──────────────────────────────────────────────────────

fn read_power_sysfs(parts: &[&str]) -> Option<String> {
    if parts.is_empty() {
        return None;
    }
    match parts[0] {
        "state" => Some(String::from("freeze mem disk")),
        "wakeup_count" => Some(String::from("0")),
        _ => None,
    }
}

// ─── /sys/firmware ───────────────────────────────────────────────────

fn read_firmware_sysfs(parts: &[&str]) -> Option<String> {
    if parts.is_empty() {
        return None;
    }
    match parts[0] {
        "acpi" => Some(String::from("ACPI firmware interface")),
        _ => None,
    }
}

/// Initialize sysfs
pub fn init() {
    // Create /sys directory tree in VFS
    let mut vfs = crate::vfs::VFS.lock();
    let _ = vfs.mkdir("/sys/kernel", 0o555);
    let _ = vfs.mkdir("/sys/devices", 0o555);
    let _ = vfs.mkdir("/sys/class", 0o555);
    let _ = vfs.mkdir("/sys/class/net", 0o555);
    let _ = vfs.mkdir("/sys/class/block", 0o555);
    let _ = vfs.mkdir("/sys/class/tty", 0o555);
    let _ = vfs.mkdir("/sys/fs", 0o555);
    let _ = vfs.mkdir("/sys/module", 0o555);
    let _ = vfs.mkdir("/sys/power", 0o555);
    let _ = vfs.mkdir("/sys/firmware", 0o555);
    drop(vfs);

    serial_println!("[KnoxOS] sysfs initialized (/sys)");
}
