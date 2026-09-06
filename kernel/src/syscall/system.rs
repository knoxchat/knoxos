use super::{SyscallError, SyscallResult, read_user_string};
use crate::serial_println;
/// Syscall implementations — System operations
/// uname, sysinfo, reboot, syslog, modules, kpm, cgroups, dmesg,
/// block devices, mount/umount, namespaces, prlimit64, getrandom, seccomp
use alloc::string::String;
use alloc::vec::Vec;

pub fn sys_uname(buf_ptr: u64) -> SyscallResult {
    #[repr(C)]
    struct Utsname {
        sysname: [u8; 65],
        nodename: [u8; 65],
        release: [u8; 65],
        version: [u8; 65],
        machine: [u8; 65],
    }
    fn fill(f: &mut [u8; 65], v: &str) {
        let b = v.as_bytes();
        let l = b.len().min(64);
        f[..l].copy_from_slice(&b[..l]);
        f[l] = 0;
    }
    let mut u = Utsname {
        sysname: [0; 65],
        nodename: [0; 65],
        release: [0; 65],
        version: [0; 65],
        machine: [0; 65],
    };
    fill(&mut u.sysname, "KnoxOS");
    fill(&mut u.nodename, "knoxos");
    fill(&mut u.release, "0.1.0-knoxos");
    fill(&mut u.version, "#1 SMP PREEMPT_DYNAMIC");
    fill(&mut u.machine, "x86_64");
    unsafe {
        core::ptr::write(buf_ptr as *mut Utsname, u);
    }
    Ok(0)
}

pub fn sys_sysinfo(buf_ptr: u64) -> SyscallResult {
    #[repr(C)]
    struct Si {
        uptime: i64,
        loads: [u64; 3],
        totalram: u64,
        freeram: u64,
        sharedram: u64,
        bufferram: u64,
        totalswap: u64,
        freeswap: u64,
        procs: u16,
        pad: u16,
        totalhigh: u64,
        freehigh: u64,
        mem_unit: u32,
    }
    let ticks = crate::interrupts::get_ticks();
    let mem = crate::allocator::HEAP_SIZE as u64;
    let procs = crate::process::PROCESS_TABLE.lock().count() as u16;
    let si = Si {
        uptime: (ticks as f64 / 18.2) as i64,
        loads: [0; 3],
        totalram: mem,
        freeram: mem / 2,
        sharedram: 0,
        bufferram: 0,
        totalswap: 0,
        freeswap: 0,
        procs,
        pad: 0,
        totalhigh: 0,
        freehigh: 0,
        mem_unit: 1,
    };
    unsafe {
        core::ptr::write(buf_ptr as *mut Si, si);
    }
    Ok(0)
}

pub fn sys_reboot(_magic1: u32, cmd: u32) -> SyscallResult {
    match cmd {
        0x01234567 => {
            // LINUX_REBOOT_CMD_RESTART
            serial_println!("[KnoxOS] Reboot requested via syscall");
            crate::acpi::reboot();
        }
        0x4321FEDC => {
            // LINUX_REBOOT_CMD_POWER_OFF
            serial_println!("[KnoxOS] Shutdown requested via syscall");
            crate::acpi::shutdown();
        }
        0xD000FCE2 => {
            // LINUX_REBOOT_CMD_HALT
            serial_println!("[KnoxOS] Halt requested via syscall");
            crate::acpi::shutdown();
        }
        _ => Err(SyscallError::InvalidArgument),
    }
}

pub fn sys_syslog(log_type: i32, buf_ptr: u64, len: usize) -> SyscallResult {
    match log_type {
        2 | 3 => {
            let entries = crate::syslog::dmesg();
            let mut output = String::new();
            for entry in &entries {
                use core::fmt::Write;
                let _ = writeln!(output, "{}", crate::syslog::format_entry(entry));
            }
            let bytes = output.as_bytes();
            let copy_len = core::cmp::min(bytes.len(), len);
            if buf_ptr != 0 && copy_len > 0 {
                let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, copy_len) };
                buf.copy_from_slice(&bytes[..copy_len]);
            }
            Ok(copy_len as u64)
        }
        5 => {
            crate::syslog::clear();
            Ok(0)
        }
        10 => {
            let (used, _total) = crate::syslog::stats();
            Ok(used as u64)
        }
        _ => Ok(0),
    }
}

pub fn sys_init_module(name_ptr: u64, _param_ptr: u64) -> SyscallResult {
    let name = unsafe { read_user_string(name_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let module = crate::modules::KernelModule {
        name: name.clone(),
        description: String::from("User-loaded module"),
        author: String::from("user"),
        version: String::from("0.1.0"),
        license: String::from("GPL"),
        state: crate::modules::ModuleState::Live,
        size_bytes: 0,
        dependencies: Vec::new(),
        ref_count: 0,
        parameters: alloc::collections::BTreeMap::new(),
    };
    crate::modules::insert_module(module).map_err(|_| SyscallError::FileExists)?;
    Ok(0)
}

pub fn sys_delete_module(name_ptr: u64) -> SyscallResult {
    let name = unsafe { read_user_string(name_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    crate::modules::remove_module(&name).map_err(|_| SyscallError::FileNotFound)?;
    Ok(0)
}

// ── KPM (Package manager) ──────────────────────────────────────────

pub fn sys_kpm_install(name_ptr: u64) -> SyscallResult {
    let name = unsafe { read_user_string(name_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    match crate::kpm::install(&name) {
        Ok(()) => Ok(0),
        Err(_) => Err(SyscallError::FileNotFound),
    }
}

pub fn sys_kpm_remove(name_ptr: u64) -> SyscallResult {
    let name = unsafe { read_user_string(name_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    match crate::kpm::remove(&name) {
        Ok(()) => Ok(0),
        Err(_) => Err(SyscallError::FileNotFound),
    }
}

pub fn sys_kpm_list(buf_ptr: u64, buf_len: usize) -> SyscallResult {
    let list = crate::kpm::list_installed();
    let mut output = String::new();
    for pkg in &list {
        use core::fmt::Write;
        let _ = writeln!(output, "{} {}", pkg.name, pkg.version);
    }
    let bytes = output.as_bytes();
    let copy_len = core::cmp::min(bytes.len(), buf_len);
    if buf_ptr != 0 && copy_len > 0 {
        let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, copy_len) };
        buf.copy_from_slice(&bytes[..copy_len]);
    }
    Ok(copy_len as u64)
}

pub fn sys_kpm_search(query_ptr: u64, buf_ptr: u64, buf_len: usize) -> SyscallResult {
    let query = unsafe { read_user_string(query_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let results = crate::kpm::search(&query);
    let mut output = String::new();
    for pkg in &results {
        use core::fmt::Write;
        let _ = writeln!(output, "{} {} - {}", pkg.name, pkg.version, pkg.description);
    }
    let bytes = output.as_bytes();
    let copy_len = core::cmp::min(bytes.len(), buf_len);
    if buf_ptr != 0 && copy_len > 0 {
        let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, copy_len) };
        buf.copy_from_slice(&bytes[..copy_len]);
    }
    Ok(copy_len as u64)
}

// ── Cgroups ─────────────────────────────────────────────────────────

pub fn sys_cgroup_create(path_ptr: u64) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    match crate::cgroups::create(&path) {
        Ok(()) => Ok(0),
        Err(_) => Err(SyscallError::FileExists),
    }
}

pub fn sys_cgroup_attach(path_ptr: u64, pid: u32) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    match crate::cgroups::attach_pid(&path, pid) {
        Ok(()) => Ok(0),
        Err(_) => Err(SyscallError::FileNotFound),
    }
}

// ── Dmesg / block devices / mount ───────────────────────────────────

pub fn sys_dmesg(buf_ptr: u64, buf_len: usize) -> SyscallResult {
    let entries = crate::syslog::dmesg();
    let mut output = String::new();
    for entry in &entries {
        use core::fmt::Write;
        let _ = writeln!(output, "{}", crate::syslog::format_entry(entry));
    }
    let bytes = output.as_bytes();
    let copy_len = core::cmp::min(bytes.len(), buf_len);
    if buf_ptr != 0 && copy_len > 0 {
        let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, copy_len) };
        buf.copy_from_slice(&bytes[..copy_len]);
    }
    Ok(copy_len as u64)
}

pub fn sys_lsblk(buf_ptr: u64, buf_len: usize) -> SyscallResult {
    let devices = crate::block::list_devices();
    let mut output = String::new();
    for dev in &devices {
        use core::fmt::Write;
        let size_mb = (dev.total_blocks * dev.block_size as u64) / (1024 * 1024);
        let _ = writeln!(
            output,
            "{} {}MB {:?} {}",
            dev.name, size_mb, dev.device_type, dev.model
        );
    }
    let bytes = output.as_bytes();
    let copy_len = core::cmp::min(bytes.len(), buf_len);
    if buf_ptr != 0 && copy_len > 0 {
        let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, copy_len) };
        buf.copy_from_slice(&bytes[..copy_len]);
    }
    Ok(copy_len as u64)
}

pub fn sys_mount(device_ptr: u64, _target_ptr: u64) -> SyscallResult {
    let _device = unsafe { read_user_string(device_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    crate::ext2::mount(0).map_err(|_| SyscallError::IoError)?;
    Ok(0)
}

pub fn sys_umount(_target_ptr: u64) -> SyscallResult {
    Ok(0)
}

// ── Namespaces & hostname ───────────────────────────────────────────

pub fn sys_sethostname(name_ptr: u64, len: usize) -> SyscallResult {
    if name_ptr == 0 || len == 0 || len > 64 {
        return Err(SyscallError::InvalidArgument);
    }
    let bytes = unsafe { core::slice::from_raw_parts(name_ptr as *const u8, len) };
    let name = core::str::from_utf8(bytes).map_err(|_| SyscallError::InvalidArgument)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    crate::namespaces::sethostname(pid, name)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::PermissionDenied)
}

pub fn sys_gethostname(buf_ptr: u64, len: usize) -> SyscallResult {
    if buf_ptr == 0 || len == 0 {
        return Err(SyscallError::InvalidArgument);
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let hostname = crate::namespaces::gethostname(pid);
    let bytes = hostname.as_bytes();
    let copy_len = core::cmp::min(bytes.len(), len);
    let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, copy_len) };
    buf.copy_from_slice(&bytes[..copy_len]);
    Ok(copy_len as u64)
}

pub fn sys_unshare(flags: u32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    crate::namespaces::unshare(pid, flags)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::PermissionDenied)
}

// ── Resource limits & random ────────────────────────────────────────

pub fn sys_prlimit64(pid: u32, resource: i32, new_limit: u64, old_limit: u64) -> SyscallResult {
    let _ = (pid, new_limit);
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Rlimit {
        rlim_cur: u64,
        rlim_max: u64,
    }
    let default = match resource {
        0 => Rlimit {
            rlim_cur: u64::MAX,
            rlim_max: u64::MAX,
        },
        1 => Rlimit {
            rlim_cur: u64::MAX,
            rlim_max: u64::MAX,
        },
        2 => Rlimit {
            rlim_cur: u64::MAX,
            rlim_max: u64::MAX,
        },
        3 => Rlimit {
            rlim_cur: 8388608,
            rlim_max: u64::MAX,
        },
        4 => Rlimit {
            rlim_cur: 0,
            rlim_max: u64::MAX,
        },
        5 => Rlimit {
            rlim_cur: u64::MAX,
            rlim_max: u64::MAX,
        },
        6 => Rlimit {
            rlim_cur: u64::MAX,
            rlim_max: u64::MAX,
        },
        7 => Rlimit {
            rlim_cur: 1024,
            rlim_max: 4096,
        },
        _ => Rlimit {
            rlim_cur: u64::MAX,
            rlim_max: u64::MAX,
        },
    };
    if old_limit != 0 {
        let out = unsafe { &mut *(old_limit as *mut Rlimit) };
        *out = default;
    }
    Ok(0)
}

pub fn sys_getrandom(buf_ptr: u64, len: usize, flags: u32) -> SyscallResult {
    if buf_ptr == 0 || len == 0 {
        return Err(SyscallError::InvalidArgument);
    }
    let _ = flags;
    let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, len) };
    let mut seed = crate::arch_compat::read_tsc();
    for byte in buf.iter_mut() {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *byte = (seed >> 33) as u8;
    }
    Ok(len as u64)
}

// ── Seccomp ─────────────────────────────────────────────────────────

pub fn sys_seccomp(operation: u32, _flags: u32, _args: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    match operation {
        1 => {
            // SECCOMP_SET_MODE_STRICT
            crate::seccomp::seccomp_set_mode_strict(pid)
                .map(|_| 0u64)
                .map_err(|_| SyscallError::InvalidArgument)
        }
        2 => {
            // SECCOMP_SET_MODE_FILTER
            crate::seccomp::seccomp_set_mode_filter(pid, alloc::vec![])
                .map(|_| 0u64)
                .map_err(|_| SyscallError::InvalidArgument)
        }
        _ => Err(SyscallError::InvalidArgument),
    }
}
