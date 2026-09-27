use crate::serial_println;
/// ptrace, perf_event_open, kcmp, fanotify
use crate::syscall::{SyscallError, SyscallResult, read_user_string};

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
