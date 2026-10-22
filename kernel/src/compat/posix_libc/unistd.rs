// Miscellaneous POSIX functions (unistd.h / string.h)
use crate::serial_println;

/// getpid — get process ID
#[unsafe(no_mangle)]
pub extern "C" fn getpid() -> i32 {
    crate::scheduler::current_pid().unwrap_or(1) as i32
}

/// getppid — get parent process ID
#[unsafe(no_mangle)]
pub extern "C" fn getppid() -> i32 {
    1 // init is always parent for now
}

/// getuid / geteuid — get user ID
#[unsafe(no_mangle)]
pub extern "C" fn getuid() -> u32 {
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn geteuid() -> u32 {
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn getgid() -> u32 {
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn getegid() -> u32 {
    0
}

/// sysconf — get configurable system variables
#[unsafe(no_mangle)]
pub extern "C" fn sysconf(name: i32) -> i64 {
    match name {
        30 => 4096,  // _SC_PAGESIZE
        84 => 4096,  // _SC_PAGE_SIZE
        11 => 1,     // _SC_NPROCESSORS_ONLN (conservative)
        83 => 1,     // _SC_NPROCESSORS_CONF
        2 => 200809, // _SC_VERSION (POSIX.1-2008)
        4 => 2048,   // _SC_OPEN_MAX
        29 => 4096,  // _SC_ARG_MAX
        0 => 2048,   // _SC_ARG_MAX
        8 => 256,    // _SC_NAME_MAX
        12 => 4096,  // _SC_LINE_MAX
        _ => -1,
    }
}

/// sysconf — get host/domain name length
#[unsafe(no_mangle)]
pub extern "C" fn get_nprocs() -> i32 {
    1
}

/// getpagesize
#[unsafe(no_mangle)]
pub extern "C" fn getpagesize() -> i32 {
    4096
}

/// isatty — test whether fd is a terminal
#[unsafe(no_mangle)]
pub extern "C" fn isatty(fd: i32) -> i32 {
    if (0..=2).contains(&fd) { 1 } else { 0 }
}

/// getcwd — get current working directory
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getcwd(buf: *mut u8, size: usize) -> *mut u8 {
    let cwd = b"/home/user\0";
    if buf.is_null() || size < cwd.len() {
        return core::ptr::null_mut();
    }
    core::ptr::copy_nonoverlapping(cwd.as_ptr(), buf, cwd.len());
    buf
}

/// strerror — get string description of error number
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strerror(errnum: i32) -> *const u8 {
    match errnum {
        0 => c"Success".as_ptr() as *const u8,
        1 => c"Operation not permitted".as_ptr() as *const u8,
        2 => c"No such file or directory".as_ptr() as *const u8,
        3 => c"No such process".as_ptr() as *const u8,
        4 => c"Interrupted system call".as_ptr() as *const u8,
        5 => c"Input/output error".as_ptr() as *const u8,
        9 => c"Bad file descriptor".as_ptr() as *const u8,
        11 => c"Resource temporarily unavailable".as_ptr() as *const u8,
        12 => c"Cannot allocate memory".as_ptr() as *const u8,
        13 => c"Permission denied".as_ptr() as *const u8,
        14 => c"Bad address".as_ptr() as *const u8,
        17 => c"File exists".as_ptr() as *const u8,
        20 => c"Not a directory".as_ptr() as *const u8,
        21 => c"Is a directory".as_ptr() as *const u8,
        22 => c"Invalid argument".as_ptr() as *const u8,
        28 => c"No space left on device".as_ptr() as *const u8,
        36 => c"Numerical result out of range".as_ptr() as *const u8,
        38 => c"Function not implemented".as_ptr() as *const u8,
        _ => c"Unknown error".as_ptr() as *const u8,
    }
}

/// perror — print error message
#[unsafe(no_mangle)]
pub unsafe extern "C" fn perror(s: *const u8) {
    let errno = *crate::libc_funcs::__errno_location();
    let msg = strerror(errno);
    if !s.is_null() && *s != 0 {
        let mut len = 0;
        while *s.add(len) != 0 {
            len += 1;
        }
        let prefix = core::slice::from_raw_parts(s, len);
        if let Ok(p) = core::str::from_utf8(prefix) {
            let mut mlen = 0;
            while *msg.add(mlen) != 0 {
                mlen += 1;
            }
            let msg_str = core::str::from_utf8_unchecked(core::slice::from_raw_parts(msg, mlen));
            serial_println!("{}: {}", p, msg_str);
        }
    }
}

/// access — check file accessibility
#[unsafe(no_mangle)]
pub unsafe extern "C" fn access(path: *const u8, _mode: i32) -> i32 {
    if path.is_null() {
        return -1;
    }
    let mut len = 0;
    while *path.add(len) != 0 {
        len += 1;
    }
    let p = core::str::from_utf8_unchecked(core::slice::from_raw_parts(path, len));
    if crate::vfs::read_file_dispatch(p).is_some() {
        0
    } else {
        -1
    }
}

/// pipe — create a pipe
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pipe(pipefd: *mut i32) -> i32 {
    if pipefd.is_null() {
        return -1;
    }
    let (read_fd, write_fd) = crate::pipe::create_pipe();
    *pipefd = read_fd;
    *pipefd.add(1) = write_fd;
    0
}

/// dup — duplicate a file descriptor
#[unsafe(no_mangle)]
pub extern "C" fn dup(oldfd: i32) -> i32 {
    crate::fd::dup(oldfd as usize)
        .map(|fd| fd as i32)
        .unwrap_or(-1)
}

/// dup2 — duplicate a file descriptor to a specific number
#[unsafe(no_mangle)]
pub extern "C" fn dup2(oldfd: i32, newfd: i32) -> i32 {
    crate::fd::dup2(oldfd as usize, newfd as usize)
        .map(|fd| fd as i32)
        .unwrap_or(-1)
}
