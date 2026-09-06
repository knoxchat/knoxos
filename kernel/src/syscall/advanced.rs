/// Syscall implementations — Advanced Linux subsystems
/// pidfd, memfd, userfaultfd, io_uring, rseq, xattr, close_range,
/// membarrier, io_prio, kcmp, fanotify, ptrace, perf, landlock,
/// mount_api, aio, capabilities, madvise, msync, mincore,
/// pread/pwrite, fallocate, sync, prctl, arch_prctl, etc.
use super::{SyscallError, SyscallResult, read_user_string};
use crate::serial_println;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

// ── SysV IPC State ──────────────────────────────────────────────────

/// SysV Semaphore set
struct SysVSemSet {
    key: u32,
    id: i32,
    values: Vec<i32>,
    mode: i32,
}

/// SysV Message queue
struct SysVMsgQueue {
    key: u32,
    id: i32,
    messages: alloc::collections::VecDeque<SysVMsg>,
    mode: i32,
    max_bytes: usize,
    current_bytes: usize,
}

struct SysVMsg {
    mtype: i64,
    data: Vec<u8>,
}

lazy_static::lazy_static! {
    static ref SYSV_SEMS: Mutex<BTreeMap<i32, SysVSemSet>> = Mutex::new(BTreeMap::new());
    static ref SYSV_MSGS: Mutex<BTreeMap<i32, SysVMsgQueue>> = Mutex::new(BTreeMap::new());
}
static NEXT_SEM_ID: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(1);
static NEXT_MSG_ID: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(1);

// ── pidfd operations ────────────────────────────────────────────────

pub fn sys_pidfd_open(pid: u64, flags: u32) -> SyscallResult {
    crate::pidfd::sys_pidfd_open(pid, flags).map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_pidfd_send_signal(pidfd: u64, sig: i32, info: u64, flags: u32) -> SyscallResult {
    crate::pidfd::sys_pidfd_send_signal(pidfd, sig, info, flags)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_pidfd_getfd(pidfd: u64, target_fd: i32, flags: u32) -> SyscallResult {
    crate::pidfd::sys_pidfd_getfd(pidfd, target_fd, flags)
        .map(|fd| fd as u64)
        .map_err(|_| SyscallError::BadFileDescriptor)
}

// ── memfd operations ────────────────────────────────────────────────

pub fn sys_memfd_create(name_ptr: u64, flags: u32) -> SyscallResult {
    let name = unsafe { read_user_string(name_ptr) }.unwrap_or_default();
    crate::memfd::sys_memfd_create(&name, flags).map_err(|_| SyscallError::TooManyFiles)
}

// ── userfaultfd ─────────────────────────────────────────────────────

pub fn sys_userfaultfd(flags: u32) -> SyscallResult {
    crate::userfaultfd::sys_userfaultfd(flags).map_err(|_| SyscallError::TooManyFiles)
}

// ── io_uring ────────────────────────────────────────────────────────

pub fn sys_io_uring_setup(entries: u32, params_ptr: u64) -> SyscallResult {
    let flags = if params_ptr != 0 {
        unsafe { *(params_ptr as *const u32) }
    } else {
        0
    };
    crate::io_uring::sys_io_uring_setup(entries, flags).map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_io_uring_enter(fd: u64, to_submit: u32, min_complete: u32, flags: u32) -> SyscallResult {
    crate::io_uring::sys_io_uring_enter(fd, to_submit, min_complete, flags)
        .map(|n| n as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_io_uring_register(fd: u64, opcode: u32, arg: u64, nr_args: u32) -> SyscallResult {
    crate::io_uring::sys_io_uring_register(fd, opcode, arg, nr_args)
        .map(|n| n as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── rseq ────────────────────────────────────────────────────────────

pub fn sys_rseq(rseq_ptr: u64, rseq_len: u32, flags: i32, sig: u32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    crate::rseq::sys_rseq(pid as u64, rseq_ptr, rseq_len, flags as u32, sig)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── xattr operations ────────────────────────────────────────────────

pub fn sys_setxattr(
    path_ptr: u64,
    name_ptr: u64,
    value_ptr: u64,
    size: usize,
    flags: i32,
) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let name = unsafe { read_user_string(name_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let value = if size > 0 && value_ptr != 0 {
        unsafe { core::slice::from_raw_parts(value_ptr as *const u8, size) }
    } else {
        &[]
    };
    // Resolve inode from path
    let vfs = crate::vfs::VFS.lock();
    let ino = vfs.resolve_path(&path).ok_or(SyscallError::FileNotFound)?;
    drop(vfs);
    crate::xattr::setxattr(ino, &name, value, flags)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_getxattr(path_ptr: u64, name_ptr: u64, value_ptr: u64, size: usize) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let name = unsafe { read_user_string(name_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let vfs = crate::vfs::VFS.lock();
    let ino = vfs.resolve_path(&path).ok_or(SyscallError::FileNotFound)?;
    drop(vfs);
    if size == 0 {
        // Query size
        let mut tmp = [0u8; 4096];
        crate::xattr::getxattr(ino, &name, &mut tmp)
            .map(|n| n as u64)
            .map_err(|_| SyscallError::InvalidArgument)
    } else {
        let buf = unsafe { core::slice::from_raw_parts_mut(value_ptr as *mut u8, size) };
        crate::xattr::getxattr(ino, &name, buf)
            .map(|n| n as u64)
            .map_err(|_| SyscallError::InvalidArgument)
    }
}

pub fn sys_listxattr(path_ptr: u64, list_ptr: u64, size: usize) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let vfs = crate::vfs::VFS.lock();
    let ino = vfs.resolve_path(&path).ok_or(SyscallError::FileNotFound)?;
    drop(vfs);
    if size == 0 {
        let mut tmp = [0u8; 4096];
        crate::xattr::listxattr(ino, &mut tmp)
            .map(|n| n as u64)
            .map_err(|_| SyscallError::InvalidArgument)
    } else {
        let buf = unsafe { core::slice::from_raw_parts_mut(list_ptr as *mut u8, size) };
        crate::xattr::listxattr(ino, buf)
            .map(|n| n as u64)
            .map_err(|_| SyscallError::InvalidArgument)
    }
}

pub fn sys_removexattr(path_ptr: u64, name_ptr: u64) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let name = unsafe { read_user_string(name_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let vfs = crate::vfs::VFS.lock();
    let ino = vfs.resolve_path(&path).ok_or(SyscallError::FileNotFound)?;
    drop(vfs);
    crate::xattr::removexattr(ino, &name)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_fsetxattr(
    fd: i32,
    name_ptr: u64,
    value_ptr: u64,
    size: usize,
    flags: i32,
) -> SyscallResult {
    let name = unsafe { read_user_string(name_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let value = if size > 0 && value_ptr != 0 {
        unsafe { core::slice::from_raw_parts(value_ptr as *const u8, size) }
    } else {
        &[]
    };
    let _ = fd; // Would resolve fd to inode
    crate::xattr::setxattr(fd as u64, &name, value, flags)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_fgetxattr(fd: i32, name_ptr: u64, value_ptr: u64, size: usize) -> SyscallResult {
    let name = unsafe { read_user_string(name_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    if size == 0 {
        let mut tmp = [0u8; 4096];
        crate::xattr::getxattr(fd as u64, &name, &mut tmp)
            .map(|n| n as u64)
            .map_err(|_| SyscallError::InvalidArgument)
    } else {
        let buf = unsafe { core::slice::from_raw_parts_mut(value_ptr as *mut u8, size) };
        crate::xattr::getxattr(fd as u64, &name, buf)
            .map(|n| n as u64)
            .map_err(|_| SyscallError::InvalidArgument)
    }
}

pub fn sys_flistxattr(fd: i32, list_ptr: u64, size: usize) -> SyscallResult {
    if size == 0 {
        let mut tmp = [0u8; 4096];
        crate::xattr::listxattr(fd as u64, &mut tmp)
            .map(|n| n as u64)
            .map_err(|_| SyscallError::InvalidArgument)
    } else {
        let buf = unsafe { core::slice::from_raw_parts_mut(list_ptr as *mut u8, size) };
        crate::xattr::listxattr(fd as u64, buf)
            .map(|n| n as u64)
            .map_err(|_| SyscallError::InvalidArgument)
    }
}

pub fn sys_fremovexattr(fd: i32, name_ptr: u64) -> SyscallResult {
    let name = unsafe { read_user_string(name_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    crate::xattr::removexattr(fd as u64, &name)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── close_range ─────────────────────────────────────────────────────

pub fn sys_close_range(first: u32, last: u32, flags: u32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    crate::close_range::close_range(pid, first, last, flags)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── membarrier ──────────────────────────────────────────────────────

pub fn sys_membarrier(cmd: u32, flags: u32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    crate::membarrier::sys_membarrier(cmd, flags, pid)
        .map(|v| v as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── ioprio ──────────────────────────────────────────────────────────

pub fn sys_ioprio_set(which: u32, who: u32, ioprio: u32) -> SyscallResult {
    crate::io_prio::ioprio_set(which, who, ioprio as u16)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_ioprio_get(which: u32, who: u32) -> SyscallResult {
    crate::io_prio::ioprio_get(which, who)
        .map(|v| v as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── kcmp ────────────────────────────────────────────────────────────

pub fn sys_kcmp(pid1: u32, pid2: u32, cmp_type: u32, idx1: u64, idx2: u64) -> SyscallResult {
    crate::kcmp::sys_kcmp(pid1, pid2, cmp_type, idx1, idx2)
        .map(|v| v as u64)
        .map_err(|_| SyscallError::PermissionDenied)
}

// ── fanotify ────────────────────────────────────────────────────────

pub fn sys_fanotify_init(flags: u32, event_f_flags: u32) -> SyscallResult {
    crate::fanotify::fanotify_init(flags, event_f_flags)
        .map(|fd| fd as u64)
        .map_err(|_| SyscallError::TooManyFiles)
}

pub fn sys_fanotify_mark(
    fanotify_fd: i32,
    flags: u32,
    mask: u64,
    dirfd: i32,
    pathname_ptr: u64,
) -> SyscallResult {
    let pathname = if pathname_ptr != 0 {
        unsafe { read_user_string(pathname_ptr) }.unwrap_or_default()
    } else {
        alloc::string::String::new()
    };
    let _ = dirfd;
    crate::fanotify::fanotify_mark(fanotify_fd, flags, mask, &pathname)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── ptrace ──────────────────────────────────────────────────────────

pub fn sys_ptrace(request: u64, pid: u64, addr: u64, data: u64) -> SyscallResult {
    let tracer_pid = crate::scheduler::current_pid().unwrap_or(1) as u64;
    match request {
        0 => {
            // PTRACE_TRACEME
            crate::ptrace::sys_ptrace_traceme(pid)
                .map(|_| 0u64)
                .map_err(|_| SyscallError::PermissionDenied)
        }
        1 => {
            // PTRACE_PEEKTEXT / PTRACE_PEEKDATA
            crate::ptrace::sys_ptrace_peek(pid, addr).map_err(|_| SyscallError::IoError)
        }
        4 | 5 => {
            // PTRACE_POKETEXT / PTRACE_POKEDATA
            crate::ptrace::sys_ptrace_poke(pid, addr, data)
                .map(|_| 0u64)
                .map_err(|_| SyscallError::IoError)
        }
        7 => {
            // PTRACE_CONT
            crate::ptrace::sys_ptrace_cont(pid, data)
                .map(|_| 0u64)
                .map_err(|_| SyscallError::NoSuchProcess)
        }
        9 => {
            // PTRACE_SINGLESTEP
            crate::ptrace::sys_ptrace_singlestep(pid, data)
                .map(|_| 0u64)
                .map_err(|_| SyscallError::NoSuchProcess)
        }
        12 => {
            // PTRACE_GETREGS
            crate::ptrace::sys_ptrace_getregs(pid)
                .map(|regs| {
                    if data != 0 {
                        unsafe {
                            core::ptr::write(data as *mut crate::ptrace::UserRegs, regs);
                        }
                    }
                    0u64
                })
                .map_err(|_| SyscallError::NoSuchProcess)
        }
        13 => {
            // PTRACE_SETREGS
            if data == 0 {
                return Err(SyscallError::InvalidArgument);
            }
            let regs = unsafe { core::ptr::read(data as *const crate::ptrace::UserRegs) };
            crate::ptrace::sys_ptrace_setregs(pid, regs)
                .map(|_| 0u64)
                .map_err(|_| SyscallError::NoSuchProcess)
        }
        16 => {
            // PTRACE_ATTACH
            crate::ptrace::sys_ptrace_attach(tracer_pid, pid)
                .map(|_| 0u64)
                .map_err(|_| SyscallError::PermissionDenied)
        }
        17 => {
            // PTRACE_DETACH
            crate::ptrace::sys_ptrace_detach(pid)
                .map(|_| 0u64)
                .map_err(|_| SyscallError::NoSuchProcess)
        }
        24 => {
            // PTRACE_SYSCALL
            crate::ptrace::sys_ptrace_syscall(pid, data)
                .map(|_| 0u64)
                .map_err(|_| SyscallError::NoSuchProcess)
        }
        0x4200 => {
            // PTRACE_SEIZE
            crate::ptrace::sys_ptrace_seize(tracer_pid, pid, data)
                .map(|_| 0u64)
                .map_err(|_| SyscallError::PermissionDenied)
        }
        0x4206 => {
            // PTRACE_SETOPTIONS
            crate::ptrace::sys_ptrace_setoptions(pid, data)
                .map(|_| 0u64)
                .map_err(|_| SyscallError::InvalidArgument)
        }
        0x4201 => {
            // PTRACE_GETEVENTMSG
            crate::ptrace::sys_ptrace_geteventmsg(pid)
                .map(|msg| {
                    if data != 0 {
                        unsafe {
                            *(data as *mut u64) = msg;
                        }
                    }
                    0u64
                })
                .map_err(|_| SyscallError::NoSuchProcess)
        }
        _ => {
            serial_println!("[KnoxOS] ptrace: unhandled request {}", request);
            Ok(0)
        }
    }
}

// ── perf_event_open ─────────────────────────────────────────────────

pub fn sys_perf_event_open(
    attr_ptr: u64,
    pid: i64,
    cpu: i32,
    group_fd: i64,
    flags: u64,
) -> SyscallResult {
    let _ = flags;
    // Read minimal attr fields
    let (event_type, config) = if attr_ptr != 0 {
        unsafe {
            let type_val = *(attr_ptr as *const u32);
            let config_val = *((attr_ptr as usize + 8) as *const u64);
            (type_val, config_val)
        }
    } else {
        (0, 0)
    };
    let perf_type = match event_type {
        0 => crate::perf::PerfType::Hardware,
        1 => crate::perf::PerfType::Software,
        2 => crate::perf::PerfType::Tracepoint,
        3 => crate::perf::PerfType::HwCache,
        4 => crate::perf::PerfType::Raw,
        5 => crate::perf::PerfType::Breakpoint,
        _ => crate::perf::PerfType::Software,
    };
    let attr = crate::perf::PerfEventAttr::new(perf_type, config);
    crate::perf::sys_perf_event_open(attr, pid, cpu, group_fd, flags)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── landlock ────────────────────────────────────────────────────────

pub fn sys_landlock_create_ruleset(attr_ptr: u64, size: usize, flags: u32) -> SyscallResult {
    let attr = if attr_ptr != 0 && size > 0 {
        unsafe { &*(attr_ptr as *const crate::landlock::LandlockRulesetAttr) }
    } else {
        return if flags & 1 != 0 {
            // LANDLOCK_CREATE_RULESET_VERSION
            Ok(3) // Landlock ABI version
        } else {
            Err(SyscallError::InvalidArgument)
        };
    };
    crate::landlock::sys_landlock_create_ruleset(attr, size, flags)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_landlock_add_rule(
    ruleset_fd: i32,
    rule_type: u32,
    rule_attr_ptr: u64,
    flags: u32,
) -> SyscallResult {
    let rt = match rule_type {
        1 => crate::landlock::LandlockRuleType::PathBeneath,
        2 => crate::landlock::LandlockRuleType::NetPort,
        _ => return Err(SyscallError::InvalidArgument),
    };
    let rule = match rt {
        crate::landlock::LandlockRuleType::PathBeneath => {
            if rule_attr_ptr == 0 {
                return Err(SyscallError::InvalidArgument);
            }
            let attr = unsafe { &*(rule_attr_ptr as *const crate::landlock::PathBeneathRule) };
            crate::landlock::LandlockRule::PathBeneath(attr.clone())
        }
        crate::landlock::LandlockRuleType::NetPort => {
            if rule_attr_ptr == 0 {
                return Err(SyscallError::InvalidArgument);
            }
            let attr = unsafe { &*(rule_attr_ptr as *const crate::landlock::NetPortRule) };
            crate::landlock::LandlockRule::NetPort(attr.clone())
        }
    };
    crate::landlock::sys_landlock_add_rule(ruleset_fd as u64, rt, rule, flags)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_landlock_restrict_self(ruleset_fd: i32, flags: u32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1) as u64;
    crate::landlock::sys_landlock_restrict_self(ruleset_fd as u64, flags, pid)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── New mount API ───────────────────────────────────────────────────

pub fn sys_fsopen(fstype_ptr: u64, flags: u32) -> SyscallResult {
    let fstype = unsafe { read_user_string(fstype_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    crate::mount_api::fsopen(&fstype, flags)
        .map(|fd| fd as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_fsconfig(fd: i32, cmd: u32, key_ptr: u64, value_ptr: u64, aux: i32) -> SyscallResult {
    let key = if key_ptr != 0 {
        unsafe { read_user_string(key_ptr) }
    } else {
        None
    };
    let value = if value_ptr != 0 {
        unsafe { read_user_string(value_ptr) }
    } else {
        None
    };
    let fs_value = value.map(crate::mount_api::FsConfigValue::String);
    crate::mount_api::fsconfig(fd, cmd, key.as_deref(), fs_value)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_fsmount(fs_fd: i32, flags: u32, mount_attrs: u64) -> SyscallResult {
    crate::mount_api::fsmount(fs_fd, flags, mount_attrs)
        .map(|fd| fd as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_move_mount(
    from_dfd: i32,
    from_path_ptr: u64,
    to_dfd: i32,
    to_path_ptr: u64,
    flags: u32,
) -> SyscallResult {
    let from = unsafe { read_user_string(from_path_ptr) }.unwrap_or_default();
    let to = unsafe { read_user_string(to_path_ptr) }.unwrap_or_default();
    crate::mount_api::move_mount(from_dfd, &from, to_dfd, &to, flags)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_open_tree(dfd: i32, path_ptr: u64, flags: u32) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.unwrap_or_default();
    crate::mount_api::open_tree(dfd, &path, flags)
        .map(|fd| fd as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_fspick(dfd: i32, path_ptr: u64, flags: u32) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.unwrap_or_default();
    crate::mount_api::fspick(dfd, &path, flags)
        .map(|fd| fd as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── AIO ─────────────────────────────────────────────────────────────

pub fn sys_io_setup(max_events: u32, ctx_ptr: u64) -> SyscallResult {
    let ctx = crate::aio::io_setup(max_events).map_err(|_| SyscallError::OutOfMemory)?;
    if ctx_ptr != 0 {
        unsafe {
            *(ctx_ptr as *mut u64) = ctx;
        }
    }
    Ok(0)
}

pub fn sys_io_destroy(ctx: u64) -> SyscallResult {
    crate::aio::io_destroy(ctx)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_io_getevents(
    ctx: u64,
    min_nr: i64,
    max_nr: i64,
    _events_ptr: u64,
    _timeout_ptr: u64,
) -> SyscallResult {
    let events = crate::aio::io_getevents(ctx, min_nr as i32, max_nr as i32)
        .map_err(|_| SyscallError::InvalidArgument)?;
    Ok(events.len() as u64)
}

pub fn sys_io_submit(ctx: u64, nr: i64, _iocbpp: u64) -> SyscallResult {
    // Submit empty batch (simplified)
    let iocbs = alloc::vec::Vec::new();
    crate::aio::io_submit(ctx, iocbs)
        .map(|n| n as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_io_cancel(_ctx: u64, _iocb: u64, _result: u64) -> SyscallResult {
    // AIO cancel is notoriously unreliable even in Linux
    Err(SyscallError::NotImplemented)
}

// ── signalfd ────────────────────────────────────────────────────────

pub fn sys_signalfd(fd: i32, mask_ptr: u64, flags: i32) -> SyscallResult {
    let mask = if mask_ptr != 0 {
        unsafe { *(mask_ptr as *const u64) }
    } else {
        0
    };
    if fd == -1 {
        crate::signalfd::signalfd_create(mask, flags)
            .map(|fd| fd as u64)
            .map_err(|_| SyscallError::TooManyFiles)
    } else {
        crate::signalfd::signalfd_update(fd, mask)
            .map(|_| fd as u64)
            .map_err(|_| SyscallError::BadFileDescriptor)
    }
}

// ── prctl ───────────────────────────────────────────────────────────

pub fn sys_prctl(option: i32, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    match option {
        1 => {
            // PR_SET_PDEATHSIG
            Ok(0)
        }
        2 => {
            // PR_GET_PDEATHSIG
            if arg2 != 0 {
                unsafe {
                    *(arg2 as *mut i32) = 0;
                }
            }
            Ok(0)
        }
        4 => {
            // PR_GET_UNALIGN
            Ok(0)
        }
        6 => {
            // PR_GET_FPEMU
            Ok(0)
        }
        9 => {
            // PR_GET_KEEPCAPS
            Ok(0)
        }
        15 => {
            // PR_SET_NAME — set process/thread name
            let name = unsafe { read_user_string(arg2) }.unwrap_or_default();
            let mut table = crate::process::PROCESS_TABLE.lock();
            if let Some(proc) = table.get_process_mut(pid) {
                let truncated = if name.len() > 15 { &name[..15] } else { &name };
                proc.name = alloc::string::String::from(truncated);
            }
            Ok(0)
        }
        16 => {
            // PR_GET_NAME — get process/thread name
            let table = crate::process::PROCESS_TABLE.lock();
            if let Some(proc) = table.get_process(pid) {
                if arg2 != 0 {
                    let bytes = proc.name.as_bytes();
                    let len = bytes.len().min(16);
                    unsafe {
                        core::ptr::copy_nonoverlapping(bytes.as_ptr(), arg2 as *mut u8, len);
                        *((arg2 as usize + len) as *mut u8) = 0;
                    }
                }
            }
            Ok(0)
        }
        22 => {
            // PR_SET_SECCOMP
            if arg2 == 1 {
                crate::seccomp::seccomp_set_mode_strict(pid)
                    .map(|_| 0u64)
                    .map_err(|_| SyscallError::InvalidArgument)
            } else {
                Ok(0)
            }
        }
        28 => {
            // PR_SET_NO_NEW_PRIVS
            Ok(0) // Accept and ignore
        }
        35 => {
            // PR_GET_NO_NEW_PRIVS
            Ok(0)
        }
        36 => {
            // PR_GET_THP_DISABLE
            Ok(0)
        }
        38 => {
            // PR_SET_CHILD_SUBREAPER
            Ok(0)
        }
        40 | 41 => {
            // PR_CAP_AMBIENT
            Ok(0)
        }
        _ => {
            serial_println!("[KnoxOS] prctl: unhandled option {}", option);
            let _ = (arg3, arg4, arg5);
            Ok(0)
        }
    }
}

// ── arch_prctl ──────────────────────────────────────────────────────

pub fn sys_arch_prctl(code: i32, addr: u64) -> SyscallResult {
    match code {
        0x1001 => {
            // ARCH_SET_GS
            crate::arch_compat::registers::model_specific::GsBase::write(
                crate::arch_compat::structures::paging::VirtAddr::new(addr),
            );
            Ok(0)
        }
        0x1002 => {
            // ARCH_SET_FS
            crate::arch_compat::registers::model_specific::FsBase::write(
                crate::arch_compat::structures::paging::VirtAddr::new(addr),
            );
            Ok(0)
        }
        0x1003 => {
            // ARCH_GET_FS
            let fs = crate::arch_compat::registers::model_specific::FsBase::read();
            if addr != 0 {
                unsafe {
                    *(addr as *mut u64) = fs.as_u64();
                }
            }
            Ok(fs.as_u64())
        }
        0x1004 => {
            // ARCH_GET_GS
            let gs = crate::arch_compat::registers::model_specific::GsBase::read();
            if addr != 0 {
                unsafe {
                    *(addr as *mut u64) = gs.as_u64();
                }
            }
            Ok(gs.as_u64())
        }
        _ => Err(SyscallError::InvalidArgument),
    }
}

// ── madvise / msync / mincore ───────────────────────────────────────

pub fn sys_madvise(addr: u64, len: u64, advice: i32) -> SyscallResult {
    let _ = (addr, len, advice);
    // Accept all madvise hints silently
    Ok(0)
}

pub fn sys_msync(addr: u64, len: u64, flags: i32) -> SyscallResult {
    let _ = (addr, len, flags);
    // Sync is a no-op for in-memory filesystems
    Ok(0)
}

pub fn sys_mincore(addr: u64, len: u64, vec_ptr: u64) -> SyscallResult {
    // Report all pages as resident
    let pages = len.div_ceil(4096) as usize;
    if vec_ptr != 0 {
        let vec = unsafe { core::slice::from_raw_parts_mut(vec_ptr as *mut u8, pages) };
        for b in vec.iter_mut() {
            *b = 1; // Page is in core
        }
    }
    let _ = addr;
    Ok(0)
}

// ── pread64 / pwrite64 ─────────────────────────────────────────────

pub fn sys_pread64(fd: u64, buf_ptr: u64, count: u64, offset: i64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables
        .get_mut(&pid)
        .ok_or(SyscallError::BadFileDescriptor)?;

    // Save current position, seek to offset, read, restore position
    let saved = fd_table
        .lseek(fd as i32, 0, crate::fd::SeekFrom::Current)
        .unwrap_or(0);
    let _ = fd_table.lseek(fd as i32, offset, crate::fd::SeekFrom::Start);
    let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, count as usize) };
    let result = fd_table
        .read(fd as i32, buf)
        .map(|n| n as u64)
        .map_err(|_| SyscallError::IoError);
    let _ = fd_table.lseek(fd as i32, saved as i64, crate::fd::SeekFrom::Start);
    result
}

pub fn sys_pwrite64(fd: u64, buf_ptr: u64, count: u64, offset: i64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables
        .get_mut(&pid)
        .ok_or(SyscallError::BadFileDescriptor)?;

    let saved = fd_table
        .lseek(fd as i32, 0, crate::fd::SeekFrom::Current)
        .unwrap_or(0);
    let _ = fd_table.lseek(fd as i32, offset, crate::fd::SeekFrom::Start);
    let buf = unsafe { core::slice::from_raw_parts(buf_ptr as *const u8, count as usize) };
    let result = fd_table
        .write(fd as i32, buf)
        .map(|n| n as u64)
        .map_err(|_| SyscallError::IoError);
    let _ = fd_table.lseek(fd as i32, saved as i64, crate::fd::SeekFrom::Start);
    result
}

// ── fallocate ───────────────────────────────────────────────────────

pub fn sys_fallocate(fd: i32, mode: i32, offset: i64, len: i64) -> SyscallResult {
    if len <= 0 || offset < 0 {
        return Err(SyscallError::InvalidArgument);
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables.get(&pid).ok_or(SyscallError::BadFileDescriptor)?;
    let file = fd_table.get(fd).ok_or(SyscallError::BadFileDescriptor)?;
    let path = file.path.clone();
    drop(tables);

    let target_size = (offset + len) as usize;
    // FALLOC_FL_KEEP_SIZE (0x01) means don't change file size
    if mode & 0x01 == 0 {
        let mut vfs = crate::vfs::VFS.lock();
        if let Some(ino) = vfs.resolve_path(&path) {
            if let Some(inode) = vfs.get_inode_mut(ino) {
                if inode.data.len() < target_size {
                    inode.data.resize(target_size, 0);
                    inode.size = target_size as u64;
                }
            }
        }
    }
    Ok(0)
}

// ── sync / fdatasync / syncfs / sync_file_range ─────────────────────

pub fn sys_sync() -> SyscallResult {
    Ok(0) // In-memory FS, nothing to sync
}

pub fn sys_fsync(fd: i32) -> SyscallResult {
    let _ = fd;
    Ok(0)
}

pub fn sys_fdatasync(fd: i32) -> SyscallResult {
    let _ = fd;
    Ok(0)
}

pub fn sys_syncfs(fd: i32) -> SyscallResult {
    let _ = fd;
    Ok(0)
}

pub fn sys_sync_file_range(fd: i32, offset: i64, nbytes: i64, flags: u32) -> SyscallResult {
    let _ = (fd, offset, nbytes, flags);
    Ok(0)
}

// ── clock_settime / clock_getres / clock_adjtime ────────────────────

pub fn sys_clock_settime(clock_id: u32, tp_ptr: u64) -> SyscallResult {
    let _ = (clock_id, tp_ptr);
    // Setting the clock requires CAP_SYS_TIME; accept as no-op for now
    Ok(0)
}

pub fn sys_clock_getres(clock_id: u32, res_ptr: u64) -> SyscallResult {
    if res_ptr != 0 {
        // Report 1ns resolution
        unsafe {
            let p = res_ptr as *mut [i64; 2];
            (*p)[0] = 0;
            (*p)[1] = 1;
        }
    }
    let _ = clock_id;
    Ok(0)
}

// ── wait / waitpid ──────────────────────────────────────────────────

pub fn sys_tgkill(tgid: u32, tid: u32, sig: u32) -> SyscallResult {
    let signal = crate::signals::Signal::from_number(sig).ok_or(SyscallError::InvalidArgument)?;
    let sender = crate::scheduler::current_pid().unwrap_or(0);
    let _ = tgid;
    crate::signals::kill(tid, signal, sender).map_err(|_| SyscallError::NoSuchProcess)?;
    Ok(0)
}

pub fn sys_tkill(tid: u32, sig: u32) -> SyscallResult {
    let signal = crate::signals::Signal::from_number(sig).ok_or(SyscallError::InvalidArgument)?;
    let sender = crate::scheduler::current_pid().unwrap_or(0);
    crate::signals::kill(tid, signal, sender).map_err(|_| SyscallError::NoSuchProcess)?;
    Ok(0)
}

// ── rt_sigaction / rt_sigprocmask / rt_sigpending / rt_sigsuspend / rt_sigreturn / rt_sigtimedwait / rt_sigqueueinfo

pub fn sys_rt_sigreturn() -> SyscallResult {
    // The sigreturn trampoline restores the interrupted context
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    serial_println!("[KnoxOS] rt_sigreturn for PID {}", pid);
    Ok(0)
}

pub fn sys_rt_sigpending(set_ptr: u64, sigsetsize: usize) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let signals = crate::signals::PROCESS_SIGNALS.lock();
    let mut pending_mask: u64 = 0;
    if let Some(ps) = signals.get(&pid) {
        for sig in &ps.pending {
            pending_mask |= 1u64 << (sig.signal as u32);
        }
    }
    if set_ptr != 0 && sigsetsize >= 8 {
        unsafe {
            *(set_ptr as *mut u64) = pending_mask;
        }
    }
    Ok(0)
}

pub fn sys_rt_sigsuspend(mask_ptr: u64, sigsetsize: usize) -> SyscallResult {
    let _ = (mask_ptr, sigsetsize);
    // Suspend until a signal is delivered
    crate::arch_compat::instructions::interrupts::hlt();
    Err(SyscallError::Interrupted) // Always returns EINTR
}

pub fn sys_rt_sigtimedwait(
    set_ptr: u64,
    info_ptr: u64,
    timeout_ptr: u64,
    sigsetsize: usize,
) -> SyscallResult {
    let _ = (info_ptr, timeout_ptr, sigsetsize);
    // Simplified: immediate check for pending signals
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let signals = crate::signals::PROCESS_SIGNALS.lock();
    if let Some(ps) = signals.get(&pid) {
        let mask = if set_ptr != 0 {
            unsafe { *(set_ptr as *const u64) }
        } else {
            0
        };
        for sig in &ps.pending {
            let signum = sig.signal as u32;
            if mask & (1u64 << signum) != 0 {
                return Ok(signum as u64);
            }
        }
    }
    Err(SyscallError::WouldBlock)
}

pub fn sys_rt_sigqueueinfo(pid: u32, sig: u32, _info_ptr: u64) -> SyscallResult {
    let signal = crate::signals::Signal::from_number(sig).ok_or(SyscallError::InvalidArgument)?;
    let sender = crate::scheduler::current_pid().unwrap_or(0);
    crate::signals::kill(pid, signal, sender).map_err(|_| SyscallError::NoSuchProcess)?;
    Ok(0)
}

// ── getdents (old, non-64 version) ──────────────────────────────────

pub fn sys_getdents(fd: i32, dirp: u64, count: u32) -> SyscallResult {
    // Redirect to getdents64 implementation
    super::fs::sys_getdents64(fd, dirp, count)
}

// ── newfstatat ──────────────────────────────────────────────────────

pub fn sys_newfstatat(dirfd: i32, path_ptr: u64, stat_buf: u64, flags: i32) -> SyscallResult {
    let _ = (dirfd, flags);
    super::fs::sys_stat(path_ptr, stat_buf)
}

// ── unlinkat / renameat / renameat2 / linkat / fchmodat / fchownat / futimesat / utimensat

pub fn sys_unlinkat(dirfd: i32, path_ptr: u64, flags: i32) -> SyscallResult {
    let _ = dirfd;
    if flags & 0x200 != 0 {
        // AT_REMOVEDIR
        super::fs::sys_rmdir(path_ptr)
    } else {
        super::fs::sys_unlink(path_ptr)
    }
}

pub fn sys_renameat(
    olddirfd: i32,
    oldpath_ptr: u64,
    newdirfd: i32,
    newpath_ptr: u64,
) -> SyscallResult {
    let _ = (olddirfd, newdirfd);
    super::fs::sys_rename(oldpath_ptr, newpath_ptr)
}

pub fn sys_renameat2(
    olddirfd: i32,
    oldpath_ptr: u64,
    newdirfd: i32,
    newpath_ptr: u64,
    flags: u32,
) -> SyscallResult {
    let _ = (olddirfd, newdirfd, flags);
    super::fs::sys_rename(oldpath_ptr, newpath_ptr)
}

pub fn sys_linkat(
    olddirfd: i32,
    oldpath_ptr: u64,
    newdirfd: i32,
    newpath_ptr: u64,
    flags: i32,
) -> SyscallResult {
    let _ = (olddirfd, newdirfd, flags);
    let oldpath = unsafe { read_user_string(oldpath_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let newpath = unsafe { read_user_string(newpath_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    // Create a hard link (simplified: copy data)
    let data = {
        let vfs = crate::vfs::VFS.lock();
        vfs.read_file(&oldpath)
            .map(|d| d.to_vec())
            .ok_or(SyscallError::FileNotFound)?
    };
    crate::vfs::VFS.lock().write_file(&newpath, &data);
    Ok(0)
}

pub fn sys_fchmodat(dirfd: i32, path_ptr: u64, mode: u32, flags: i32) -> SyscallResult {
    let _ = (dirfd, flags);
    super::fs::sys_chmod(path_ptr, mode as u16)
}

pub fn sys_fchownat(dirfd: i32, path_ptr: u64, uid: u32, gid: u32, flags: i32) -> SyscallResult {
    let _ = (dirfd, flags);
    super::fs::sys_chown(path_ptr, uid, gid)
}

pub fn sys_utimensat(dirfd: i32, path_ptr: u64, times_ptr: u64, flags: i32) -> SyscallResult {
    let _ = (dirfd, flags);
    // Validate path exists if provided
    if path_ptr != 0 {
        let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
        let vfs = crate::vfs::VFS.lock();
        vfs.resolve_path(&path).ok_or(SyscallError::FileNotFound)?;
    }
    // Accept timestamp changes (in-memory VFS doesn't track timestamps)
    let _ = times_ptr;
    Ok(0)
}

// ── faccessat / faccessat2 ──────────────────────────────────────────

pub fn sys_faccessat(dirfd: i32, path_ptr: u64, mode: u32, flags: i32) -> SyscallResult {
    let _ = (dirfd, flags);
    super::fs::sys_access(path_ptr, mode)
}

// ── accept4 ─────────────────────────────────────────────────────────

pub fn sys_accept4(sockfd: i32, addr_ptr: u64, addrlen_ptr: u64, flags: i32) -> SyscallResult {
    let _ = flags; // Would handle SOCK_NONBLOCK, SOCK_CLOEXEC
    super::net::sys_accept(sockfd, addr_ptr, addrlen_ptr)
}

// ── recvmsg / sendmsg ───────────────────────────────────────────────

pub fn sys_sendmsg(sockfd: i32, msg_ptr: u64, flags: i32) -> SyscallResult {
    super::net::sys_sendmsg(sockfd, msg_ptr, flags)
}

pub fn sys_recvmsg(sockfd: i32, msg_ptr: u64, flags: i32) -> SyscallResult {
    super::net::sys_recvmsg(sockfd, msg_ptr, flags)
}

// ── getsockname / getpeername ───────────────────────────────────────

pub fn sys_getsockname(sockfd: i32, addr_ptr: u64, addrlen_ptr: u64) -> SyscallResult {
    // Look up the socket's local address from the fd table
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables.get(&pid).ok_or(SyscallError::BadFileDescriptor)?;
    let file = fd_table
        .get(sockfd)
        .ok_or(SyscallError::BadFileDescriptor)?;
    let sock_id = file.inode;
    drop(tables);

    let sockets = crate::net::SOCKETS.lock();
    if let Some(sock) = sockets.get(&(sock_id as u32)) {
        if addr_ptr != 0 {
            match &sock.local_addr {
                Some(crate::net::SocketAddress::Inet(ip, port)) => {
                    // sockaddr_in: sa_family(2) + port(2) + addr(4) + zero(8)
                    unsafe {
                        core::ptr::write_bytes(addr_ptr as *mut u8, 0, 16);
                        *(addr_ptr as *mut u16) = 2; // AF_INET
                        *((addr_ptr + 2) as *mut u16) = port.to_be();
                        *((addr_ptr + 4) as *mut u32) = u32::from_be_bytes(ip.0);
                    }
                    if addrlen_ptr != 0 {
                        unsafe {
                            *(addrlen_ptr as *mut u32) = 16;
                        }
                    }
                }
                Some(crate::net::SocketAddress::Unix(path)) => {
                    let path_bytes = path.as_bytes();
                    let copy_len = path_bytes.len().min(107);
                    unsafe {
                        core::ptr::write_bytes(addr_ptr as *mut u8, 0, 110);
                        *(addr_ptr as *mut u16) = 1; // AF_UNIX
                        core::ptr::copy_nonoverlapping(
                            path_bytes.as_ptr(),
                            (addr_ptr + 2) as *mut u8,
                            copy_len,
                        );
                    }
                    if addrlen_ptr != 0 {
                        unsafe {
                            *(addrlen_ptr as *mut u32) = (2 + copy_len + 1) as u32;
                        }
                    }
                }
                None => {
                    // Unbound socket: return zeroed address
                    if addr_ptr != 0 {
                        unsafe {
                            core::ptr::write_bytes(addr_ptr as *mut u8, 0, 16);
                        }
                    }
                    if addrlen_ptr != 0 {
                        unsafe {
                            *(addrlen_ptr as *mut u32) = 0;
                        }
                    }
                }
            }
        }
        Ok(0)
    } else {
        // Fallback: return zeroed for sockets not in SOCKETS table
        if addr_ptr != 0 {
            unsafe {
                core::ptr::write_bytes(addr_ptr as *mut u8, 0, 16);
            }
        }
        if addrlen_ptr != 0 {
            unsafe {
                *(addrlen_ptr as *mut u32) = 16;
            }
        }
        Ok(0)
    }
}

pub fn sys_getpeername(sockfd: i32, addr_ptr: u64, addrlen_ptr: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables.get(&pid).ok_or(SyscallError::BadFileDescriptor)?;
    let file = fd_table
        .get(sockfd)
        .ok_or(SyscallError::BadFileDescriptor)?;
    let sock_id = file.inode;
    drop(tables);

    let sockets = crate::net::SOCKETS.lock();
    if let Some(sock) = sockets.get(&(sock_id as u32)) {
        if sock.state != crate::net::SocketState::Connected {
            return Err(SyscallError::NotConnected);
        }
        if addr_ptr != 0 {
            match &sock.remote_addr {
                Some(crate::net::SocketAddress::Inet(ip, port)) => {
                    unsafe {
                        core::ptr::write_bytes(addr_ptr as *mut u8, 0, 16);
                        *(addr_ptr as *mut u16) = 2; // AF_INET
                        *((addr_ptr + 2) as *mut u16) = port.to_be();
                        *((addr_ptr + 4) as *mut u32) = u32::from_be_bytes(ip.0);
                    }
                    if addrlen_ptr != 0 {
                        unsafe {
                            *(addrlen_ptr as *mut u32) = 16;
                        }
                    }
                }
                Some(crate::net::SocketAddress::Unix(path)) => {
                    let path_bytes = path.as_bytes();
                    let copy_len = path_bytes.len().min(107);
                    unsafe {
                        core::ptr::write_bytes(addr_ptr as *mut u8, 0, 110);
                        *(addr_ptr as *mut u16) = 1; // AF_UNIX
                        core::ptr::copy_nonoverlapping(
                            path_bytes.as_ptr(),
                            (addr_ptr + 2) as *mut u8,
                            copy_len,
                        );
                    }
                    if addrlen_ptr != 0 {
                        unsafe {
                            *(addrlen_ptr as *mut u32) = (2 + copy_len + 1) as u32;
                        }
                    }
                }
                None => {
                    return Err(SyscallError::NotConnected);
                }
            }
        }
        Ok(0)
    } else {
        Err(SyscallError::BadFileDescriptor)
    }
}

// ── gettid ──────────────────────────────────────────────────────────

pub fn sys_gettid() -> SyscallResult {
    // In many cases tid == pid for the main thread
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    Ok(pid as u64)
}

// ── clone3 ──────────────────────────────────────────────────────────

pub fn sys_clone3(cl_args_ptr: u64, size: u64) -> SyscallResult {
    // clone3 uses a clone_args struct
    // Parse the flags from clone_args (first u64 field)
    let flags = if cl_args_ptr != 0 && size >= 8 {
        unsafe { *(cl_args_ptr as *const u64) }
    } else {
        0
    };

    const CLONE_VM: u64 = 0x00000100;
    const CLONE_FS: u64 = 0x00000200;
    const CLONE_FILES: u64 = 0x00000400;
    const CLONE_SIGHAND: u64 = 0x00000800;
    const CLONE_THREAD: u64 = 0x00010000;
    const CLONE_NEWNS: u64 = 0x00020000;
    const CLONE_NEWPID: u64 = 0x20000000;

    // If CLONE_THREAD + CLONE_VM — this is a thread creation
    if flags & CLONE_THREAD != 0 && flags & CLONE_VM != 0 {
        // Extract stack pointer from clone_args (offset 0x28 = 40)
        let stack = if cl_args_ptr != 0 && size >= 48 {
            unsafe { *((cl_args_ptr + 40) as *const u64) }
        } else {
            0
        };
        // Extract entry point (child_tid or tls can indicate where to start)
        let tls = if cl_args_ptr != 0 && size >= 88 {
            unsafe { *((cl_args_ptr + 80) as *const u64) }
        } else {
            0
        };
        let entry = if tls != 0 { tls } else { 0 };
        return super::thread::sys_thread_create(entry, stack);
    }

    // For namespace flags, log them but still do fork
    if flags & CLONE_NEWNS != 0 || flags & CLONE_NEWPID != 0 {
        serial_println!("[KnoxOS] clone3 with namespace flags {:#x}", flags);
    }

    // Default: fall back to fork (creates a full child process)
    let child_pid = super::process::sys_fork()?;

    // If CLONE_FILES, share the fd table rather than copying
    if flags & CLONE_FILES != 0 {
        serial_println!("[KnoxOS] clone3: CLONE_FILES for PID {}", child_pid);
        // FD table is already copied by fork; CLONE_FILES means sharing
        // which we approximate by keeping the copy
    }

    Ok(child_pid)
}

// ── setresuid / getresuid / setresgid / getresgid ───────────────────

pub fn sys_setresuid(ruid: u32, euid: u32, suid: u32) -> SyscallResult {
    let _ = suid;
    crate::users::setreuid(ruid, euid)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::PermissionDenied)
}

pub fn sys_getresuid(ruid_ptr: u64, euid_ptr: u64, suid_ptr: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    let uid = crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .map(|p| p.uid)
        .unwrap_or(0);
    if ruid_ptr != 0 {
        unsafe {
            *(ruid_ptr as *mut u32) = uid;
        }
    }
    if euid_ptr != 0 {
        unsafe {
            *(euid_ptr as *mut u32) = uid;
        }
    }
    if suid_ptr != 0 {
        unsafe {
            *(suid_ptr as *mut u32) = uid;
        }
    }
    Ok(0)
}

pub fn sys_setresgid(rgid: u32, egid: u32, sgid: u32) -> SyscallResult {
    let _ = sgid;
    crate::users::setregid(rgid, egid)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::PermissionDenied)
}

pub fn sys_getresgid(rgid_ptr: u64, egid_ptr: u64, sgid_ptr: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    let gid = crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .map(|p| p.gid)
        .unwrap_or(0);
    if rgid_ptr != 0 {
        unsafe {
            *(rgid_ptr as *mut u32) = gid;
        }
    }
    if egid_ptr != 0 {
        unsafe {
            *(egid_ptr as *mut u32) = gid;
        }
    }
    if sgid_ptr != 0 {
        unsafe {
            *(sgid_ptr as *mut u32) = gid;
        }
    }
    Ok(0)
}

// ── setns ───────────────────────────────────────────────────────────

pub fn sys_setns(fd: i32, nstype: i32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    serial_println!(
        "[KnoxOS] setns: PID {} joining namespace fd={} type={:#x}",
        pid,
        fd,
        nstype
    );
    // Validate fd exists
    let tables = crate::fd::PROCESS_FD_TABLES.lock();
    if let Some(fd_table) = tables.get(&pid) {
        if fd_table.get(fd).is_none() {
            return Err(SyscallError::BadFileDescriptor);
        }
    }
    // Accept namespace join (detailed namespace tracking in namespaces.rs)
    Ok(0)
}

// ── pivot_root ──────────────────────────────────────────────────────

pub fn sys_pivot_root(new_root_ptr: u64, put_old_ptr: u64) -> SyscallResult {
    let new_root =
        unsafe { read_user_string(new_root_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let put_old = unsafe { read_user_string(put_old_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    serial_println!("[KnoxOS] pivot_root({}, {})", new_root, put_old);

    // Verify both paths exist
    let vfs = crate::vfs::VFS.lock();
    vfs.resolve_path(&new_root)
        .ok_or(SyscallError::FileNotFound)?;
    vfs.resolve_path(&put_old)
        .ok_or(SyscallError::FileNotFound)?;
    drop(vfs);

    // Update current process root to new_root
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    crate::process::PROCESS_TABLE.lock().chdir(pid, &new_root);
    Ok(0)
}

// ── mount / umount2 (Linux standard) ────────────────────────────────

pub fn sys_mount_linux(
    source_ptr: u64,
    target_ptr: u64,
    fstype_ptr: u64,
    flags: u64,
    data_ptr: u64,
) -> SyscallResult {
    let source = unsafe { read_user_string(source_ptr) }.unwrap_or_default();
    let target = unsafe { read_user_string(target_ptr) }.unwrap_or_default();
    let fstype = unsafe { read_user_string(fstype_ptr) }.unwrap_or_default();
    let _ = (data_ptr, flags);
    serial_println!("[KnoxOS] mount({}, {}, {})", source, target, fstype);
    // Create mount point in VFS
    let mut vfs = crate::vfs::VFS.lock();
    vfs.mkdir(&target, 0o755).ok();
    Ok(0)
}

pub fn sys_umount2(target_ptr: u64, flags: i32) -> SyscallResult {
    let _ = flags;
    let target = unsafe { read_user_string(target_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    serial_println!("[KnoxOS] umount2({})", target);
    Ok(0)
}

// ── swapon / swapoff ────────────────────────────────────────────────

pub fn sys_swapon(path_ptr: u64, flags: i32) -> SyscallResult {
    let _ = (path_ptr, flags);
    Ok(0)
}

pub fn sys_swapoff(path_ptr: u64) -> SyscallResult {
    let _ = path_ptr;
    Ok(0)
}

// ── mlock / munlock / mlockall / munlockall ─────────────────────────

pub fn sys_mlock(addr: u64, len: u64) -> SyscallResult {
    let _ = (addr, len);
    Ok(0)
}

pub fn sys_munlock(addr: u64, len: u64) -> SyscallResult {
    let _ = (addr, len);
    Ok(0)
}

pub fn sys_mlockall(flags: i32) -> SyscallResult {
    let _ = flags;
    Ok(0)
}

pub fn sys_munlockall() -> SyscallResult {
    Ok(0)
}

pub fn sys_mlock2(addr: u64, len: u64, flags: i32) -> SyscallResult {
    let _ = (addr, len, flags);
    Ok(0)
}

// ── mremap ──────────────────────────────────────────────────────────

pub fn sys_mremap(
    old_addr: u64,
    old_size: u64,
    new_size: u64,
    flags: i32,
    new_addr: u64,
) -> SyscallResult {
    let _ = (flags, new_addr);
    // Simplified: allocate new region and copy
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let has_as = crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .map(|p| p.has_address_space)
        .unwrap_or(false);
    if has_as {
        let result = crate::vmm::mmap(pid, 0, new_size, 0x7, 0x22); // PROT_READ|WRITE|EXEC, MAP_PRIVATE|ANONYMOUS
        if result >= 0 {
            // Copy old data
            let copy_len = old_size.min(new_size) as usize;
            unsafe {
                core::ptr::copy_nonoverlapping(old_addr as *const u8, result as *mut u8, copy_len);
            }
            let _ = crate::vmm::munmap(pid, old_addr, old_size);
            Ok(result as u64)
        } else {
            Err(SyscallError::OutOfMemory)
        }
    } else {
        // Kernel-mode mmap fallback
        let new = super::memory::sys_mmap(0, new_size, 0x3, 0x22, -1, 0)?;
        let copy_len = old_size.min(new_size) as usize;
        unsafe {
            core::ptr::copy_nonoverlapping(old_addr as *const u8, new as *mut u8, copy_len);
        }
        let _ = super::memory::sys_munmap(old_addr, old_size);
        Ok(new)
    }
}

// ── sched_rr_get_interval ───────────────────────────────────────────

pub fn sys_sched_rr_get_interval(pid: u32, tp_ptr: u64) -> SyscallResult {
    let _ = pid;
    if tp_ptr != 0 {
        unsafe {
            let p = tp_ptr as *mut [i64; 2];
            (*p)[0] = 0;
            (*p)[1] = 100_000_000; // 100ms default quantum
        }
    }
    Ok(0)
}

// ── personality ─────────────────────────────────────────────────────

pub fn sys_personality(persona: u64) -> SyscallResult {
    if persona == 0xFFFFFFFF {
        // Query current personality
        Ok(0) // PER_LINUX
    } else {
        Ok(0) // Accept, return old personality
    }
}

// ── capget / capset ─────────────────────────────────────────────────

pub fn sys_capget(header_ptr: u64, data_ptr: u64) -> SyscallResult {
    let _ = header_ptr;
    if data_ptr != 0 {
        let pid = crate::scheduler::current_pid().unwrap_or(1);
        let caps = crate::capabilities::get_capabilities(pid);
        if let Some(caps) = caps {
            unsafe {
                // Effective, permitted, inheritable (each u32)
                let p = data_ptr as *mut u32;
                *p = caps.effective.0 as u32;
                *p.add(1) = caps.permitted.0 as u32;
                *p.add(2) = caps.inheritable.0 as u32;
            }
        } else {
            unsafe {
                core::ptr::write_bytes(data_ptr as *mut u8, 0xFF, 12);
            }
        }
    }
    Ok(0)
}

pub fn sys_capset(header_ptr: u64, data_ptr: u64) -> SyscallResult {
    let _ = (header_ptr, data_ptr);
    // Accept capability changes
    Ok(0)
}

// ── getitimer / setitimer (Linux standard numbers) ──────────────────

pub fn sys_getitimer_linux(which: i32, curr_value: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let itimer_which = match which {
        0 => crate::posix_timer::ITimerWhich::Real,
        1 => crate::posix_timer::ITimerWhich::Virtual,
        2 => crate::posix_timer::ITimerWhich::Prof,
        _ => return Err(SyscallError::InvalidArgument),
    };

    if curr_value != 0 {
        match crate::posix_timer::getitimer(pid, itimer_which) {
            Ok(val) => {
                // itimerval: {it_interval: {tv_sec, tv_usec}, it_value: {tv_sec, tv_usec}}
                unsafe {
                    let p = curr_value as *mut i64;
                    *p = val.interval.tv_sec;
                    *p.add(1) = val.interval.tv_nsec / 1000; // nsec -> usec
                    *p.add(2) = val.value.tv_sec;
                    *p.add(3) = val.value.tv_nsec / 1000;
                }
            }
            Err(_) => unsafe {
                core::ptr::write_bytes(curr_value as *mut u8, 0, 32);
            },
        }
    }
    Ok(0)
}

pub fn sys_setitimer_linux(which: i32, new_value: u64, old_value: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let itimer_which = match which {
        0 => crate::posix_timer::ITimerWhich::Real,
        1 => crate::posix_timer::ITimerWhich::Virtual,
        2 => crate::posix_timer::ITimerWhich::Prof,
        _ => return Err(SyscallError::InvalidArgument),
    };

    let new_val = if new_value != 0 {
        unsafe {
            let p = new_value as *const i64;
            crate::posix_timer::ITimerVal {
                interval: crate::posix_timer::Timespec {
                    tv_sec: *p,
                    tv_nsec: *p.add(1) * 1000,
                },
                value: crate::posix_timer::Timespec {
                    tv_sec: *p.add(2),
                    tv_nsec: *p.add(3) * 1000,
                },
            }
        }
    } else {
        crate::posix_timer::ITimerVal::default()
    };

    match crate::posix_timer::setitimer(pid, itimer_which, new_val) {
        Ok(old_val) => {
            if old_value != 0 {
                unsafe {
                    let p = old_value as *mut i64;
                    *p = old_val.interval.tv_sec;
                    *p.add(1) = old_val.interval.tv_nsec / 1000;
                    *p.add(2) = old_val.value.tv_sec;
                    *p.add(3) = old_val.value.tv_nsec / 1000;
                }
            }
            Ok(0)
        }
        Err(_) => Err(SyscallError::InvalidArgument),
    }
}

// ── alarm (Linux standard number 37) ────────────────────────────────

pub fn sys_alarm_linux(seconds: u32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let prev = crate::posix_timer::alarm(pid, seconds);
    Ok(prev as u64)
}

// ── timer_create / timer_settime / timer_gettime / timer_getoverrun / timer_delete

pub fn sys_timer_create_linux(clockid: u32, sevp: u64, timerid: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let notify = if sevp != 0 {
        let sigev_notify = unsafe { *(sevp as *const i32) };
        let sigev_signo = unsafe { *((sevp as usize + 4) as *const i32) };
        match sigev_notify {
            0 => crate::posix_timer::TimerNotify::Signal(sigev_signo as u32),
            1 => crate::posix_timer::TimerNotify::Signal(sigev_signo as u32),
            _ => crate::posix_timer::TimerNotify::Signal(14),
        }
    } else {
        crate::posix_timer::TimerNotify::Signal(14)
    };
    let id = crate::posix_timer::timer_create(pid, clockid, notify)
        .map_err(|_| SyscallError::InvalidArgument)?;
    if timerid != 0 {
        unsafe {
            *(timerid as *mut u32) = id;
        }
    }
    Ok(0)
}

pub fn sys_timer_settime_linux(
    timerid: u32,
    flags: i32,
    new_value: u64,
    old_value: u64,
) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);

    // Parse itimerspec from new_value: {interval: {tv_sec, tv_nsec}, value: {tv_sec, tv_nsec}}
    let (interval, value) = if new_value != 0 {
        let p = new_value as *const i64;
        unsafe {
            let it_interval = crate::posix_timer::Timespec {
                tv_sec: *p,
                tv_nsec: *p.add(1),
            };
            let it_value = crate::posix_timer::Timespec {
                tv_sec: *p.add(2),
                tv_nsec: *p.add(3),
            };
            (it_interval, it_value)
        }
    } else {
        let zero = crate::posix_timer::Timespec::default();
        (zero, zero)
    };

    let absolute = flags & 1 != 0; // TIMER_ABSTIME
    match crate::posix_timer::timer_settime(pid, timerid, interval, value, absolute) {
        Ok(old_val) => {
            if old_value != 0 {
                // Write old itimerspec: {interval(zeroed for simplicity), value}
                unsafe {
                    let p = old_value as *mut i64;
                    *p = 0; // old interval.tv_sec
                    *p.add(1) = 0; // old interval.tv_nsec
                    *p.add(2) = old_val.tv_sec;
                    *p.add(3) = old_val.tv_nsec;
                }
            }
            Ok(0)
        }
        Err(_) => Err(SyscallError::InvalidArgument),
    }
}

pub fn sys_timer_gettime_linux(timerid: u32, curr_value: u64) -> SyscallResult {
    if curr_value != 0 {
        // Zero out by default; posix_timer doesn't expose a timer_gettime API directly
        // so write back zero values (timer disarmed)
        unsafe {
            core::ptr::write_bytes(curr_value as *mut u8, 0, 32);
        }
    }
    Ok(0)
}

pub fn sys_timer_getoverrun_linux(timerid: u32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    match crate::posix_timer::timer_getoverrun(pid, timerid) {
        Ok(count) => Ok(count as u64),
        Err(_) => Ok(0),
    }
}

pub fn sys_timer_delete_linux(timerid: u32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    crate::posix_timer::timer_delete(pid, timerid)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── futex2 ──────────────────────────────────────────────────────────

pub fn sys_futex_waitv(
    waiters: u64,
    nr_futexes: u32,
    flags: u32,
    timeout: u64,
    clockid: u32,
) -> SyscallResult {
    let _ = (waiters, nr_futexes, flags, timeout, clockid);
    // Simplified: return immediately
    Ok(0)
}

// ── sysctl (obsolete but some programs use it) ──────────────────────

pub fn sys_sysctl_old(args_ptr: u64) -> SyscallResult {
    let _ = args_ptr;
    // Deprecated syscall
    Err(SyscallError::NotImplemented)
}

// ── copy_file_range already handled, but add preadv2/pwritev2 ──────

pub fn sys_preadv2(fd: i32, iov: u64, iovcnt: i32, offset: i64, flags: i32) -> SyscallResult {
    let _ = (offset, flags);
    super::io::sys_readv(fd, iov, iovcnt as usize)
}

pub fn sys_pwritev2(fd: i32, iov: u64, iovcnt: i32, offset: i64, flags: i32) -> SyscallResult {
    let _ = (offset, flags);
    super::io::sys_writev(fd, iov, iovcnt as usize)
}

// ── statx ───────────────────────────────────────────────────────────

pub fn sys_statx(dirfd: i32, path_ptr: u64, flags: i32, mask: u32, statxbuf: u64) -> SyscallResult {
    let _ = (dirfd, flags, mask);
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let vfs = crate::vfs::VFS.lock();
    let s = vfs.stat(&path).map_err(|_| SyscallError::FileNotFound)?;
    if statxbuf != 0 {
        // Fill statx structure (256 bytes)
        let buf = unsafe { core::slice::from_raw_parts_mut(statxbuf as *mut u8, 256) };
        buf.fill(0);
        // stx_mask
        unsafe {
            *(statxbuf as *mut u32) = 0x7FF;
        } // STATX_BASIC_STATS
        // stx_blksize
        unsafe {
            *((statxbuf as usize + 4) as *mut u32) = 4096;
        }
        // stx_nlink
        unsafe {
            *((statxbuf as usize + 16) as *mut u32) = s.nlink as u32;
        }
        // stx_uid
        unsafe {
            *((statxbuf as usize + 20) as *mut u32) = s.uid;
        }
        // stx_gid
        unsafe {
            *((statxbuf as usize + 24) as *mut u32) = s.gid;
        }
        // stx_mode
        let mode: u16 = match s.file_type {
            crate::vfs::FileType::Regular => 0o100000,
            crate::vfs::FileType::Directory => 0o040000,
            crate::vfs::FileType::CharDevice => 0o020000,
            crate::vfs::FileType::BlockDevice => 0o060000,
            crate::vfs::FileType::Pipe => 0o010000,
            crate::vfs::FileType::Socket => 0o140000,
            crate::vfs::FileType::SymLink => 0o120000,
        } as u16
            | s.permissions;
        unsafe {
            *((statxbuf as usize + 28) as *mut u16) = mode;
        }
        // stx_ino
        unsafe {
            *((statxbuf as usize + 32) as *mut u64) = s.ino;
        }
        // stx_size
        unsafe {
            *((statxbuf as usize + 40) as *mut u64) = s.size;
        }
        // stx_blocks
        unsafe {
            *((statxbuf as usize + 48) as *mut u64) = s.size.div_ceil(512);
        }
    }
    Ok(0)
}

// ── name_to_handle_at / open_by_handle_at ───────────────────────────

pub fn sys_name_to_handle_at(
    dirfd: i32,
    path_ptr: u64,
    handle: u64,
    mount_id: u64,
    flags: i32,
) -> SyscallResult {
    let _ = (dirfd, flags);
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let vfs = crate::vfs::VFS.lock();
    let ino = vfs.resolve_path(&path).ok_or(SyscallError::FileNotFound)?;
    if mount_id != 0 {
        unsafe {
            *(mount_id as *mut i32) = 0;
        }
    }
    if handle != 0 {
        // file_handle: u32 handle_bytes, i32 handle_type, then bytes
        unsafe {
            *(handle as *mut u32) = 8;
            *((handle as usize + 4) as *mut i32) = 1;
            *((handle as usize + 8) as *mut u64) = ino;
        }
    }
    Ok(0)
}

pub fn sys_open_by_handle_at(mount_fd: i32, handle: u64, flags: i32) -> SyscallResult {
    let _ = (mount_fd, handle, flags);
    Err(SyscallError::NotImplemented)
}

// ── Miscellaneous stubs ─────────────────────────────────────────────

pub fn sys_lookup_dcookie(cookie: u64, buf: u64, len: usize) -> SyscallResult {
    let _ = (cookie, buf, len);
    Err(SyscallError::NotImplemented)
}

pub fn sys_ioperm(from: u64, num: u64, turn_on: i32) -> SyscallResult {
    let _ = (from, num, turn_on);
    Ok(0) // Kernel mode: always has I/O permissions
}

pub fn sys_iopl(level: i32) -> SyscallResult {
    let _ = level;
    Ok(0) // Accept IOPL changes
}

pub fn sys_vhangup() -> SyscallResult {
    Ok(0)
}

pub fn sys_modify_ldt(func: i32, ptr: u64, bytecount: u64) -> SyscallResult {
    let _ = (func, ptr, bytecount);
    Ok(0)
}

pub fn sys_adjtimex(buf: u64) -> SyscallResult {
    let _ = buf;
    Ok(0) // TIME_OK
}

pub fn sys_acct(filename_ptr: u64) -> SyscallResult {
    let _ = filename_ptr;
    Ok(0) // Process accounting enable/disable
}

pub fn sys_settimeofday(tv: u64, tz: u64) -> SyscallResult {
    let _ = (tv, tz);
    Ok(0)
}

pub fn sys_sysfs(option: i32, arg1: u64, arg2: u64) -> SyscallResult {
    let _ = (option, arg1, arg2);
    Ok(0)
}

pub fn sys_set_robust_list(head: u64, len: usize) -> SyscallResult {
    let _ = (head, len);
    Ok(0)
}

pub fn sys_get_robust_list(pid: i32, head_ptr: u64, len_ptr: u64) -> SyscallResult {
    let _ = pid;
    if head_ptr != 0 {
        unsafe {
            *(head_ptr as *mut u64) = 0;
        }
    }
    if len_ptr != 0 {
        unsafe {
            *(len_ptr as *mut u64) = 0;
        }
    }
    Ok(0)
}

pub fn sys_getcpu(cpu: u64, node: u64, _cache: u64) -> SyscallResult {
    if cpu != 0 {
        unsafe {
            *(cpu as *mut u32) = 0;
        }
    }
    if node != 0 {
        unsafe {
            *(node as *mut u32) = 0;
        }
    }
    Ok(0)
}

pub fn sys_set_mempolicy(mode: i32, nodemask: u64, maxnode: u64) -> SyscallResult {
    let _ = (mode, nodemask, maxnode);
    Ok(0)
}

pub fn sys_get_mempolicy(
    policy: u64,
    nodemask: u64,
    maxnode: u64,
    addr: u64,
    flags: u64,
) -> SyscallResult {
    if policy != 0 {
        unsafe {
            *(policy as *mut i32) = 0;
        }
    } // MPOL_DEFAULT
    let _ = (nodemask, maxnode, addr, flags);
    Ok(0)
}

pub fn sys_mbind(
    addr: u64,
    len: u64,
    mode: i32,
    nodemask: u64,
    maxnode: u64,
    flags: u32,
) -> SyscallResult {
    let _ = (addr, len, mode, nodemask, maxnode, flags);
    Ok(0)
}

pub fn sys_migrate_pages(pid: u32, maxnode: u64, old_nodes: u64, new_nodes: u64) -> SyscallResult {
    let _ = (pid, maxnode, old_nodes, new_nodes);
    Ok(0)
}

pub fn sys_move_pages(
    pid: u32,
    count: u64,
    pages: u64,
    nodes: u64,
    status: u64,
    flags: i32,
) -> SyscallResult {
    let _ = (pid, count, pages, nodes, status, flags);
    Ok(0)
}

lazy_static::lazy_static! {
    static ref KEYRING: Mutex<BTreeMap<i32, (alloc::string::String, Vec<u8>)>> = Mutex::new(BTreeMap::new());
}
static NEXT_KEY_ID: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(1);

pub fn sys_request_key(
    type_ptr: u64,
    desc_ptr: u64,
    _callout: u64,
    _keyring: i32,
) -> SyscallResult {
    let desc = unsafe { read_user_string(desc_ptr) }.unwrap_or_default();
    let _ = type_ptr;
    // Search for key by description
    let keys = KEYRING.lock();
    for (id, (name, _)) in keys.iter() {
        if *name == desc {
            return Ok(*id as u64);
        }
    }
    Err(SyscallError::NoKey)
}

pub fn sys_keyctl(operation: i32, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> SyscallResult {
    const KEYCTL_GET_KEYRING_ID: i32 = 0;
    const KEYCTL_REVOKE: i32 = 3;
    const KEYCTL_READ: i32 = 11;
    const KEYCTL_DESCRIBE: i32 = 6;

    match operation {
        KEYCTL_GET_KEYRING_ID => Ok(arg2), // Return the keyring serial
        KEYCTL_REVOKE => {
            KEYRING.lock().remove(&(arg2 as i32));
            Ok(0)
        }
        KEYCTL_READ => {
            let keys = KEYRING.lock();
            if let Some((_, payload)) = keys.get(&(arg2 as i32)) {
                let copy_len = payload.len().min(arg4 as usize);
                if arg3 != 0 && copy_len > 0 {
                    unsafe {
                        core::ptr::copy_nonoverlapping(payload.as_ptr(), arg3 as *mut u8, copy_len);
                    }
                }
                Ok(payload.len() as u64)
            } else {
                Err(SyscallError::NoKey)
            }
        }
        KEYCTL_DESCRIBE => {
            let keys = KEYRING.lock();
            if let Some((name, _)) = keys.get(&(arg2 as i32)) {
                let bytes = name.as_bytes();
                let copy_len = bytes.len().min(arg4 as usize);
                if arg3 != 0 && copy_len > 0 {
                    unsafe {
                        core::ptr::copy_nonoverlapping(bytes.as_ptr(), arg3 as *mut u8, copy_len);
                    }
                }
                Ok(bytes.len() as u64)
            } else {
                Err(SyscallError::NoKey)
            }
        }
        _ => {
            let _ = arg5;
            Ok(0)
        }
    }
}

pub fn sys_add_key(
    type_ptr: u64,
    desc_ptr: u64,
    payload: u64,
    plen: usize,
    _keyring: i32,
) -> SyscallResult {
    let _ = type_ptr;
    let desc = unsafe { read_user_string(desc_ptr) }.unwrap_or_default();
    let data = if payload != 0 && plen > 0 {
        unsafe { core::slice::from_raw_parts(payload as *const u8, plen) }.to_vec()
    } else {
        Vec::new()
    };
    let id = NEXT_KEY_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    KEYRING.lock().insert(id, (desc, data));
    Ok(id as u64)
}

pub fn sys_mq_timedreceive(
    mqd: i32,
    msg_ptr: u64,
    msg_len: usize,
    prio_ptr: u64,
    timeout_ptr: u64,
) -> SyscallResult {
    let _ = (prio_ptr, timeout_ptr);
    super::io::sys_mq_receive(mqd, msg_ptr, msg_len)
}

pub fn sys_mq_timedsend(
    mqd: i32,
    msg_ptr: u64,
    msg_len: usize,
    prio: u32,
    timeout_ptr: u64,
) -> SyscallResult {
    let _ = timeout_ptr;
    super::io::sys_mq_send(mqd, msg_ptr, msg_len, prio)
}

pub fn sys_mq_notify(mqd: i32, sevp: u64) -> SyscallResult {
    let _ = (mqd, sevp);
    Ok(0)
}

pub fn sys_mq_getsetattr(mqd: i32, newattr: u64, oldattr: u64) -> SyscallResult {
    let _ = (mqd, newattr);
    if oldattr != 0 {
        unsafe {
            core::ptr::write_bytes(oldattr as *mut u8, 0, 64);
        }
    }
    Ok(0)
}

pub fn sys_semget(key: u32, nsems: i32, semflg: i32) -> SyscallResult {
    let ipc_creat = 0o1000;
    let ipc_excl = 0o2000;
    let ipc_private = 0u32;

    let mut sems = SYSV_SEMS.lock();

    // IPC_PRIVATE always creates a new set
    if key == ipc_private || semflg & ipc_creat != 0 {
        // Check for existing key (unless IPC_PRIVATE)
        if key != ipc_private {
            if let Some(existing) = sems.values().find(|s| s.key == key) {
                if semflg & ipc_excl != 0 {
                    return Err(SyscallError::FileExists);
                }
                return Ok(existing.id as u64);
            }
        }
        let id = NEXT_SEM_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        let n = if nsems > 0 { nsems as usize } else { 1 };
        sems.insert(
            id,
            SysVSemSet {
                key,
                id,
                values: alloc::vec![0; n],
                mode: semflg & 0o777,
            },
        );
        serial_println!(
            "[KnoxOS] semget: created set {} (key={}, nsems={})",
            id,
            key,
            n
        );
        Ok(id as u64)
    } else {
        // Lookup existing
        sems.values()
            .find(|s| s.key == key)
            .map(|s| s.id as u64)
            .ok_or(SyscallError::FileNotFound)
    }
}

pub fn sys_semop(semid: i32, sops: u64, nsops: usize) -> SyscallResult {
    if sops == 0 || nsops == 0 {
        return Err(SyscallError::InvalidArgument);
    }
    // struct sembuf { u16 sem_num, i16 sem_op, i16 sem_flg }
    let mut sems = SYSV_SEMS.lock();
    let set = sems.get_mut(&semid).ok_or(SyscallError::InvalidArgument)?;

    for i in 0..nsops {
        let base = (sops as usize) + i * 6;
        let sem_num = unsafe { *(base as *const u16) } as usize;
        let sem_op = unsafe { *((base + 2) as *const i16) } as i32;
        // sem_flg at base+4 (IPC_NOWAIT etc.)

        if sem_num >= set.values.len() {
            return Err(SyscallError::InvalidArgument);
        }

        if sem_op > 0 {
            // Increment (V/signal)
            set.values[sem_num] += sem_op;
        } else if sem_op < 0 {
            // Decrement (P/wait)
            let needed = -sem_op;
            if set.values[sem_num] >= needed {
                set.values[sem_num] -= needed;
            } else {
                return Err(SyscallError::WouldBlock);
            }
        }
        // sem_op == 0: wait for zero (simplified: check if already zero)
    }
    Ok(0)
}

pub fn sys_semctl(semid: i32, semnum: i32, cmd: i32, arg: u64) -> SyscallResult {
    const IPC_RMID: i32 = 0;
    const IPC_SET: i32 = 1;
    const IPC_STAT: i32 = 2;
    const GETVAL: i32 = 12;
    const SETVAL: i32 = 16;
    const GETALL: i32 = 13;
    const SETALL: i32 = 17;

    match cmd {
        IPC_RMID => {
            SYSV_SEMS.lock().remove(&semid);
            Ok(0)
        }
        GETVAL => {
            let sems = SYSV_SEMS.lock();
            let set = sems.get(&semid).ok_or(SyscallError::InvalidArgument)?;
            let idx = semnum as usize;
            if idx >= set.values.len() {
                return Err(SyscallError::InvalidArgument);
            }
            Ok(set.values[idx] as u64)
        }
        SETVAL => {
            let mut sems = SYSV_SEMS.lock();
            let set = sems.get_mut(&semid).ok_or(SyscallError::InvalidArgument)?;
            let idx = semnum as usize;
            if idx >= set.values.len() {
                return Err(SyscallError::InvalidArgument);
            }
            set.values[idx] = arg as i32;
            Ok(0)
        }
        GETALL => {
            let sems = SYSV_SEMS.lock();
            let set = sems.get(&semid).ok_or(SyscallError::InvalidArgument)?;
            if arg != 0 {
                let dst = arg as *mut u16;
                for (i, val) in set.values.iter().enumerate() {
                    unsafe {
                        *dst.add(i) = *val as u16;
                    }
                }
            }
            Ok(0)
        }
        SETALL => {
            let mut sems = SYSV_SEMS.lock();
            let set = sems.get_mut(&semid).ok_or(SyscallError::InvalidArgument)?;
            if arg != 0 {
                let src = arg as *const u16;
                for (i, val) in set.values.iter_mut().enumerate() {
                    *val = unsafe { *src.add(i) } as i32;
                }
            }
            Ok(0)
        }
        IPC_STAT | IPC_SET => {
            // Accept stat/set as no-op for compatibility
            let _ = arg;
            Ok(0)
        }
        _ => Ok(0),
    }
}

pub fn sys_semtimedop(semid: i32, sops: u64, nsops: usize, _timeout: u64) -> SyscallResult {
    // Forward to semop (timeout handling simplified)
    sys_semop(semid, sops, nsops)
}

pub fn sys_msgget(key: u32, msgflg: i32) -> SyscallResult {
    let ipc_creat = 0o1000;
    let ipc_excl = 0o2000;
    let ipc_private = 0u32;

    let mut msgs = SYSV_MSGS.lock();

    if key == ipc_private || msgflg & ipc_creat != 0 {
        if key != ipc_private {
            if let Some(existing) = msgs.values().find(|m| m.key == key) {
                if msgflg & ipc_excl != 0 {
                    return Err(SyscallError::FileExists);
                }
                return Ok(existing.id as u64);
            }
        }
        let id = NEXT_MSG_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        msgs.insert(
            id,
            SysVMsgQueue {
                key,
                id,
                messages: alloc::collections::VecDeque::new(),
                mode: msgflg & 0o777,
                max_bytes: 16384,
                current_bytes: 0,
            },
        );
        serial_println!("[KnoxOS] msgget: created queue {} (key={})", id, key);
        Ok(id as u64)
    } else {
        msgs.values()
            .find(|m| m.key == key)
            .map(|m| m.id as u64)
            .ok_or(SyscallError::FileNotFound)
    }
}

pub fn sys_msgsnd(msqid: i32, msgp: u64, msgsz: usize, msgflg: i32) -> SyscallResult {
    if msgp == 0 {
        return Err(SyscallError::InvalidArgument);
    }
    // msgbuf: { long mtype; char mtext[msgsz]; }
    let mtype = unsafe { *(msgp as *const i64) };
    if mtype < 1 {
        return Err(SyscallError::InvalidArgument);
    }
    let data = if msgsz > 0 {
        unsafe { core::slice::from_raw_parts((msgp + 8) as *const u8, msgsz) }.to_vec()
    } else {
        Vec::new()
    };

    let mut msgs = SYSV_MSGS.lock();
    let queue = msgs.get_mut(&msqid).ok_or(SyscallError::InvalidArgument)?;

    // Check capacity
    if queue.current_bytes + msgsz > queue.max_bytes {
        if msgflg & 0x800 != 0 {
            // IPC_NOWAIT
            return Err(SyscallError::WouldBlock);
        }
        return Err(SyscallError::WouldBlock); // Simplified: no blocking
    }

    queue.current_bytes += msgsz;
    queue.messages.push_back(SysVMsg { mtype, data });
    Ok(0)
}

pub fn sys_msgrcv(msqid: i32, msgp: u64, msgsz: usize, msgtyp: i64, msgflg: i32) -> SyscallResult {
    if msgp == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    let mut msgs = SYSV_MSGS.lock();
    let queue = msgs.get_mut(&msqid).ok_or(SyscallError::InvalidArgument)?;

    // Find matching message
    let pos = if msgtyp == 0 {
        // Any message type
        if queue.messages.is_empty() {
            None
        } else {
            Some(0)
        }
    } else if msgtyp > 0 {
        // Exact type match
        queue.messages.iter().position(|m| m.mtype == msgtyp)
    } else {
        // Lowest type <= |msgtyp|
        let abs_typ = -msgtyp;
        queue
            .messages
            .iter()
            .enumerate()
            .filter(|(_, m)| m.mtype <= abs_typ)
            .min_by_key(|(_, m)| m.mtype)
            .map(|(i, _)| i)
    };

    if let Some(idx) = pos {
        let msg = queue.messages.remove(idx).unwrap();
        queue.current_bytes = queue.current_bytes.saturating_sub(msg.data.len());

        let copy_len = msg.data.len().min(msgsz);
        if msg.data.len() > msgsz && msgflg & 0x1000 == 0 {
            // MSG_NOERROR
            return Err(SyscallError::InvalidArgument); // E2BIG
        }

        unsafe {
            *(msgp as *mut i64) = msg.mtype;
            if copy_len > 0 {
                core::ptr::copy_nonoverlapping(msg.data.as_ptr(), (msgp + 8) as *mut u8, copy_len);
            }
        }
        Ok(copy_len as u64)
    } else {
        if msgflg & 0x800 != 0 {
            // IPC_NOWAIT
            return Err(SyscallError::NoMessage);
        }
        Err(SyscallError::WouldBlock) // Simplified: no blocking
    }
}

pub fn sys_msgctl(msqid: i32, cmd: i32, buf: u64) -> SyscallResult {
    const IPC_RMID: i32 = 0;
    const IPC_STAT: i32 = 2;
    const IPC_SET: i32 = 1;

    match cmd {
        IPC_RMID => {
            SYSV_MSGS.lock().remove(&msqid);
            Ok(0)
        }
        IPC_STAT => {
            let msgs = SYSV_MSGS.lock();
            if let Some(queue) = msgs.get(&msqid) {
                if buf != 0 {
                    // Write msqid_ds structure (simplified: write msg count and bytes)
                    unsafe {
                        core::ptr::write_bytes(buf as *mut u8, 0, 120);
                        // msg_qnum at offset 64
                        *((buf + 64) as *mut u64) = queue.messages.len() as u64;
                        // msg_qbytes at offset 72
                        *((buf + 72) as *mut u64) = queue.max_bytes as u64;
                    }
                }
                Ok(0)
            } else {
                Err(SyscallError::InvalidArgument)
            }
        }
        IPC_SET => {
            // Accept set as no-op
            Ok(0)
        }
        _ => Ok(0),
    }
}
