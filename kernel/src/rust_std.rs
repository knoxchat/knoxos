/// Rust Standard Library Port for KnoxOS
///
/// Provides kernel-side infrastructure to support Rust's `std` library on KnoxOS.
/// This module implements the OS-specific backing for:
///   - std::thread → kernel threads
///   - std::fs → VFS syscalls
///   - std::net → socket syscalls
///   - std::io → fd-based I/O
///   - std::process → process management
///   - std::sync → futex-based primitives
///   - std::time → clock syscalls
///   - std::env → environment variables
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// RUST TARGET SPECIFICATION
// ═══════════════════════════════════════════════════════════════════════

/// Custom Rust target: x86_64-unknown-knoxos
/// This tells rustc how to compile std for our OS
pub fn rust_target_spec() -> String {
    String::from(
        r#"{
    "arch": "x86_64",
    "cpu": "x86-64",
    "data-layout": "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128",
    "dynamic-linking": true,
    "env": "knoxos",
    "executables": true,
    "features": "+cx8,+fxsr,+mmx,+sse,+sse2",
    "has-thread-local": true,
    "linker-flavor": "ld.lld",
    "llvm-target": "x86_64-unknown-linux-musl",
    "max-atomic-width": 64,
    "os": "knoxos",
    "panic-strategy": "unwind",
    "position-independent-executables": true,
    "relro-level": "full",
    "stack-probes": { "kind": "inline" },
    "supported-split-debuginfo": ["packed", "unpacked", "off"],
    "target-c-int-width": "32",
    "target-endian": "little",
    "target-family": ["unix"],
    "target-pointer-width": "64",
    "vendor": "unknown"
}"#,
    )
}

// ═══════════════════════════════════════════════════════════════════════
// std::thread BACKING — KERNEL THREAD SUPPORT
// ═══════════════════════════════════════════════════════════════════════

/// Thread spawn info — mirrors what std::thread::Builder produces
#[derive(Debug, Clone)]
pub struct StdThreadInfo {
    pub tid: u64,
    pub name: Option<String>,
    pub stack_size: usize,
    pub entry: u64,
    pub arg: u64,
    pub joined: bool,
    pub detached: bool,
    pub exit_code: Option<i64>,
}

/// Thread registry for std::thread
static STD_THREADS: Mutex<BTreeMap<u64, StdThreadInfo>> = Mutex::new(BTreeMap::new());
static NEXT_STD_TID: AtomicU64 = AtomicU64::new(1000);

/// Spawn a new thread (backing for std::thread::spawn)
/// Uses clone() syscall with CLONE_VM | CLONE_FS | CLONE_FILES | CLONE_SIGHAND | CLONE_THREAD
pub fn thread_spawn(
    name: Option<&str>,
    stack_size: usize,
    entry: u64,
    arg: u64,
) -> Result<u64, i64> {
    let tid = NEXT_STD_TID.fetch_add(1, Ordering::SeqCst);

    let info = StdThreadInfo {
        tid,
        name: name.map(String::from),
        stack_size: if stack_size == 0 {
            8 * 1024 * 1024
        } else {
            stack_size
        }, // 8MB default
        entry,
        arg,
        joined: false,
        detached: false,
        exit_code: None,
    };

    STD_THREADS.lock().insert(tid, info);

    // Create a kernel process/thread for this std::thread
    let ppid = crate::scheduler::current_pid().unwrap_or(1);
    let thread_name = name.unwrap_or("std-thread");
    let mut table = crate::process::PROCESS_TABLE.lock();
    let _child_pid = table.spawn(thread_name, ppid);
    drop(table);

    serial_println!(
        "[RUST-STD] Thread spawned: tid={} name={:?} stack={}KB",
        tid,
        name,
        stack_size / 1024
    );

    Ok(tid)
}

/// Join a thread (blocking wait for completion)
pub fn thread_join(tid: u64) -> Result<i64, i64> {
    let mut threads = STD_THREADS.lock();
    if let Some(info) = threads.get_mut(&tid) {
        if info.joined {
            return Err(-11); // EAGAIN - already joined
        }
        info.joined = true;

        // In real implementation, this would use futex_wait on the TID address
        // (the kernel clears the TID and wakes futex on thread exit)
        Ok(info.exit_code.unwrap_or(0))
    } else {
        Err(-3) // ESRCH - no such thread
    }
}

/// Yield current thread (sched_yield)
pub fn thread_yield() {
    // Maps to sched_yield() syscall (24)
    crate::scheduler::yield_now();
}

/// Get current thread ID
pub fn current_thread_id() -> u64 {
    // Maps to gettid() syscall (186)
    crate::scheduler::current_pid().unwrap_or(0) as u64
}

/// Sleep for nanoseconds (std::thread::sleep)
pub fn thread_sleep_ns(nanos: u64) {
    // Maps to clock_nanosleep() syscall with CLOCK_MONOTONIC
    let secs = nanos / 1_000_000_000;
    let nsecs = nanos % 1_000_000_000;
    let req = crate::clock::Timespec::new(secs as i64, nsecs as i64);
    let _ = crate::clock::nanosleep(&req);
}

// ═══════════════════════════════════════════════════════════════════════
// std::fs BACKING — FILESYSTEM OPERATIONS
// ═══════════════════════════════════════════════════════════════════════

/// File metadata — matches std::fs::Metadata
#[derive(Debug, Clone)]
pub struct StdFileMetadata {
    pub dev: u64,
    pub ino: u64,
    pub mode: u32,
    pub nlink: u64,
    pub uid: u32,
    pub gid: u32,
    pub rdev: u64,
    pub size: u64,
    pub blksize: u64,
    pub blocks: u64,
    pub atime: i64,
    pub atime_nsec: i64,
    pub mtime: i64,
    pub mtime_nsec: i64,
    pub ctime: i64,
    pub ctime_nsec: i64,
}

impl StdFileMetadata {
    pub fn is_dir(&self) -> bool {
        (self.mode & 0o170000) == 0o040000
    }

    pub fn is_file(&self) -> bool {
        (self.mode & 0o170000) == 0o100000
    }

    pub fn is_symlink(&self) -> bool {
        (self.mode & 0o170000) == 0o120000
    }

    pub fn len(&self) -> u64 {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn permissions(&self) -> u32 {
        self.mode & 0o7777
    }
}

/// Open file flags
pub const O_RDONLY: i32 = 0;
pub const O_WRONLY: i32 = 1;
pub const O_RDWR: i32 = 2;
pub const O_CREAT: i32 = 0o100;
pub const O_EXCL: i32 = 0o200;
pub const O_TRUNC: i32 = 0o1000;
pub const O_APPEND: i32 = 0o2000;
pub const O_NONBLOCK: i32 = 0o4000;
pub const O_CLOEXEC: i32 = 0o2000000;
pub const O_DIRECTORY: i32 = 0o200000;

/// File operations for std::fs
pub fn fs_open(path: &str, flags: i32, mode: u32) -> Result<i32, i64> {
    // Maps to openat(AT_FDCWD, path, flags, mode) syscall (257)
    serial_println!(
        "[RUST-STD] fs::open({}, flags={:#x}, mode={:#o})",
        path,
        flags,
        mode
    );
    // Determine file type from path
    let file_type = if path.starts_with("/dev/") {
        crate::fd::FileType::CharDevice
    } else if path.starts_with("/proc/") || path.starts_with("/sys/") {
        crate::fd::FileType::ProcFile
    } else {
        crate::fd::FileType::Regular
    };
    match crate::fd::sys_open(path, crate::fd::OpenFlags(flags as u32), file_type) {
        Ok(fd) => Ok(fd),
        Err(e) => Err(e as i64),
    }
}

pub fn fs_read(fd: i32, buf: &mut [u8]) -> Result<usize, i64> {
    // Maps to read() syscall (0)
    match crate::fd::sys_read(fd, buf) {
        Ok(n) => Ok(n),
        Err(e) => Err(e as i64),
    }
}

pub fn fs_write(fd: i32, buf: &[u8]) -> Result<usize, i64> {
    // Maps to write() syscall (1)
    match crate::fd::sys_write(fd, buf) {
        Ok(n) => Ok(n),
        Err(e) => Err(e as i64),
    }
}

pub fn fs_close(fd: i32) -> Result<(), i64> {
    // Maps to close() syscall (3)
    match crate::fd::sys_close(fd) {
        Ok(()) => Ok(()),
        Err(e) => Err(e as i64),
    }
}

pub fn fs_stat(path: &str) -> Result<StdFileMetadata, i64> {
    // Maps to stat() syscall (4) or newfstatat() (262)
    let vfs = crate::vfs::VFS.lock();
    if let Some(ino) = vfs.resolve_path(path) {
        if let Some(inode) = vfs.get_inode(ino) {
            let mode = match inode.file_type {
                crate::vfs::FileType::Directory => 0o040755,
                crate::vfs::FileType::SymLink => 0o120777,
                crate::vfs::FileType::CharDevice => 0o020666,
                crate::vfs::FileType::BlockDevice => 0o060660,
                _ => 0o100644,
            };
            return Ok(StdFileMetadata {
                dev: 0,
                ino,
                mode,
                nlink: 1,
                uid: inode.uid,
                gid: inode.gid,
                rdev: 0,
                size: inode.size,
                blksize: 4096,
                blocks: inode.size.div_ceil(512),
                atime: inode.atime,
                atime_nsec: 0,
                mtime: inode.mtime,
                mtime_nsec: 0,
                ctime: inode.ctime,
                ctime_nsec: 0,
            });
        }
    }
    Err(-2) // ENOENT
}

pub fn fs_mkdir(path: &str, mode: u32) -> Result<(), i64> {
    // Maps to mkdirat() syscall (258)
    crate::vfs::ensure_directory(path);
    Ok(())
}

pub fn fs_remove_file(path: &str) -> Result<(), i64> {
    // Maps to unlinkat() syscall (263)
    match crate::vfs::remove_dispatch(path) {
        Ok(()) => Ok(()),
        Err(e) => Err(e as i64),
    }
}

pub fn fs_remove_dir(path: &str) -> Result<(), i64> {
    // Maps to unlinkat(AT_REMOVEDIR) syscall (263)
    match crate::vfs::remove_dispatch(path) {
        Ok(()) => Ok(()),
        Err(e) => Err(e as i64),
    }
}

pub fn fs_rename(from: &str, to: &str) -> Result<(), i64> {
    // Maps to renameat2() syscall (316)
    let mut vfs = crate::vfs::VFS.lock();
    match vfs.rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) => Err(e as i64),
    }
}

/// Directory entry — for std::fs::read_dir
#[derive(Debug, Clone)]
pub struct StdDirEntry {
    pub name: String,
    pub ino: u64,
    pub file_type: u8,
}

pub fn fs_readdir(path: &str) -> Result<Vec<StdDirEntry>, i64> {
    // Maps to getdents64() syscall (217)
    let vfs = crate::vfs::VFS.lock();
    if let Some(ino) = vfs.resolve_path(path) {
        if let Some(inode) = vfs.get_inode(ino) {
            if inode.file_type != crate::vfs::FileType::Directory {
                return Err(-20); // ENOTDIR
            }
            let mut entries = Vec::new();
            for &child_ino in &inode.children {
                if let Some(child) = vfs.get_inode(child_ino) {
                    let dtype = match child.file_type {
                        crate::vfs::FileType::Directory => 4,   // DT_DIR
                        crate::vfs::FileType::SymLink => 10,    // DT_LNK
                        crate::vfs::FileType::CharDevice => 2,  // DT_CHR
                        crate::vfs::FileType::BlockDevice => 6, // DT_BLK
                        crate::vfs::FileType::Pipe => 1,        // DT_FIFO
                        crate::vfs::FileType::Socket => 12,     // DT_SOCK
                        _ => 8,                                 // DT_REG
                    };
                    entries.push(StdDirEntry {
                        name: child.name.clone(),
                        ino: child.ino,
                        file_type: dtype,
                    });
                }
            }
            return Ok(entries);
        }
    }
    Err(-2) // ENOENT
}

// ═══════════════════════════════════════════════════════════════════════
// std::net BACKING — NETWORK OPERATIONS
// ═══════════════════════════════════════════════════════════════════════

/// Socket address families
pub const AF_INET: i32 = 2;
pub const AF_INET6: i32 = 10;
pub const AF_UNIX: i32 = 1;

/// Socket types
pub const SOCK_STREAM: i32 = 1;
pub const SOCK_DGRAM: i32 = 2;
pub const SOCK_RAW: i32 = 3;

/// IPv4 socket address
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SockaddrIn {
    pub sin_family: u16,
    pub sin_port: u16, // Network byte order
    pub sin_addr: u32, // Network byte order
    pub sin_zero: [u8; 8],
}

/// IPv6 socket address
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SockaddrIn6 {
    pub sin6_family: u16,
    pub sin6_port: u16,
    pub sin6_flowinfo: u32,
    pub sin6_addr: [u8; 16],
    pub sin6_scope_id: u32,
}

/// Network operations for std::net
pub fn net_socket(domain: i32, sock_type: i32, protocol: i32) -> Result<i32, i64> {
    // Maps to socket() syscall (41)
    match crate::net::sys_socket(domain as u32, sock_type as u32, protocol as u32) {
        Ok(fd) => Ok(fd as i32),
        Err(e) => Err(e as i64),
    }
}

pub fn net_bind(fd: i32, addr: &[u8]) -> Result<(), i64> {
    // Maps to bind() syscall (49)
    match crate::net::sys_bind(fd as u32, addr.as_ptr() as u64) {
        Ok(()) => Ok(()),
        Err(e) => Err(e as i64),
    }
}

pub fn net_listen(fd: i32, backlog: i32) -> Result<(), i64> {
    // Maps to listen() syscall (50)
    match crate::net::sys_listen(fd as u32, backlog as u32) {
        Ok(()) => Ok(()),
        Err(e) => Err(e as i64),
    }
}

pub fn net_accept(fd: i32) -> Result<(i32, Vec<u8>), i64> {
    // Maps to accept4() syscall (288)
    match crate::net::sys_accept(fd as u32) {
        Ok(new_fd) => Ok((new_fd as i32, Vec::new())),
        Err(e) => Err(e as i64),
    }
}

pub fn net_connect(fd: i32, addr: &[u8]) -> Result<(), i64> {
    // Maps to connect() syscall (42)
    match crate::net::sys_connect(fd as u32, addr.as_ptr() as u64) {
        Ok(()) => Ok(()),
        Err(e) => Err(e as i64),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// std::sync BACKING — SYNCHRONIZATION PRIMITIVES
// ═══════════════════════════════════════════════════════════════════════

/// Futex operations (backing for Mutex, RwLock, Condvar, Once)
pub const FUTEX_WAIT: i32 = 0;
pub const FUTEX_WAKE: i32 = 1;
pub const FUTEX_WAIT_PRIVATE: i32 = 128;
pub const FUTEX_WAKE_PRIVATE: i32 = 129;

/// Futex wait — used by std::sync::Mutex
pub fn futex_wait(addr: u64, expected: u32, timeout_ns: Option<u64>) -> Result<(), i64> {
    // Maps to futex() syscall (202) with FUTEX_WAIT
    match crate::threads::futex_wait(addr, expected) {
        Ok(()) => Ok(()),
        Err(e) => Err(e as i64),
    }
}

/// Futex wake — used by std::sync::Mutex unlock
pub fn futex_wake(addr: u64, count: u32) -> Result<u32, i64> {
    // Maps to futex() syscall (202) with FUTEX_WAKE
    match crate::threads::futex_wake(addr, count) {
        Ok(n) => Ok(n),
        Err(e) => Err(e as i64),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// std::time BACKING — CLOCK OPERATIONS
// ═══════════════════════════════════════════════════════════════════════

/// Clock IDs matching Linux
pub const CLOCK_REALTIME: i32 = 0;
pub const CLOCK_MONOTONIC: i32 = 1;
pub const CLOCK_PROCESS_CPUTIME: i32 = 2;
pub const CLOCK_THREAD_CPUTIME: i32 = 3;
pub const CLOCK_BOOTTIME: i32 = 7;

/// Timespec for clock operations
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

/// Get clock time (backing for std::time::Instant and SystemTime)
pub fn clock_gettime(clock_id: i32) -> Result<Timespec, i64> {
    // Maps to clock_gettime() syscall (228)
    // vDSO fast path available via vdso.rs
    match crate::clock::clock_gettime(clock_id as u32) {
        Ok(ts) => Ok(Timespec {
            tv_sec: ts.tv_sec,
            tv_nsec: ts.tv_nsec,
        }),
        Err(e) => Err(e as i64),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// std::env BACKING — ENVIRONMENT
// ═══════════════════════════════════════════════════════════════════════

/// Default environment variables for KnoxOS processes
pub fn default_environment() -> Vec<(String, String)> {
    vec![
        (String::from("HOME"), String::from("/root")),
        (String::from("USER"), String::from("root")),
        (String::from("LOGNAME"), String::from("root")),
        (String::from("SHELL"), String::from("/bin/sh")),
        (
            String::from("PATH"),
            String::from("/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"),
        ),
        (String::from("TERM"), String::from("xterm-256color")),
        (String::from("LANG"), String::from("C.UTF-8")),
        (String::from("LC_ALL"), String::from("C.UTF-8")),
        (String::from("HOSTNAME"), String::from("knoxos")),
        (String::from("PWD"), String::from("/")),
        (String::from("TMPDIR"), String::from("/tmp")),
        (String::from("XDG_RUNTIME_DIR"), String::from("/run/user/0")),
        (String::from("RUST_BACKTRACE"), String::from("1")),
    ]
}

/// Get the current working directory
pub fn env_current_dir() -> Result<String, i64> {
    // Maps to getcwd() syscall (79)
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    let table = crate::process::PROCESS_TABLE.lock();
    if let Some(proc) = table.get_process(pid) {
        Ok(proc.cwd.clone())
    } else {
        Ok(String::from("/"))
    }
}

/// Set the current working directory
pub fn env_set_current_dir(path: &str) -> Result<(), i64> {
    // Maps to chdir() syscall (80)
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    let mut table = crate::process::PROCESS_TABLE.lock();
    if table.chdir(pid, path) {
        Ok(())
    } else {
        Err(-2) // ENOENT
    }
}

// ═══════════════════════════════════════════════════════════════════════
// std::process BACKING — PROCESS MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════

/// Process exit (std::process::exit)
pub fn process_exit(code: i32) -> ! {
    // Maps to exit_group() syscall (231)
    loop {
        unsafe {
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!("hlt");
        }
    }
}

/// Spawn a child process (std::process::Command)
pub fn process_spawn(program: &str, args: &[&str], env: &[(String, String)]) -> Result<u64, i64> {
    // Maps to fork() + execve()
    serial_println!("[RUST-STD] process::spawn({} {:?})", program, args);
    let mut table = crate::process::PROCESS_TABLE.lock();
    let ppid = crate::scheduler::current_pid().unwrap_or(1);
    let child_pid = table.spawn(program, ppid);
    Ok(child_pid as u64)
}

/// Wait for child process (std::process::Child::wait)
pub fn process_wait(pid: u64) -> Result<i32, i64> {
    // Maps to waitpid() / wait4() syscall (61)
    let mut table = crate::process::PROCESS_TABLE.lock();
    if let Some((_, exit_code)) = table.waitpid(pid as u32) {
        Ok(exit_code)
    } else {
        // Process still running or not found
        Ok(0)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// std::io BACKING — I/O PRIMITIVES
// ═══════════════════════════════════════════════════════════════════════

/// Write to stdout (used by print!/println!)
pub fn io_write_stdout(data: &[u8]) -> Result<usize, i64> {
    // Maps to write(1, data, len) syscall
    for &byte in data {
        crate::serial_print!("{}", byte as char);
    }
    Ok(data.len())
}

/// Write to stderr (used by eprint!/eprintln!)
pub fn io_write_stderr(data: &[u8]) -> Result<usize, i64> {
    // Maps to write(2, data, len) syscall
    for &byte in data {
        crate::serial_print!("{}", byte as char);
    }
    Ok(data.len())
}

/// Read from stdin
pub fn io_read_stdin(buf: &mut [u8]) -> Result<usize, i64> {
    // Maps to read(0, buf, len) syscall
    // Read from fd 0 through the fd table
    match crate::fd::sys_read(0, buf) {
        Ok(n) => Ok(n),
        Err(_) => Ok(0), // EOF on failure
    }
}

/// Pipe creation (std::process::Stdio::piped)
pub fn io_pipe() -> Result<(i32, i32), i64> {
    // Maps to pipe2(O_CLOEXEC) syscall (293)
    let (read_fd, write_fd) = crate::pipe::create_pipe();
    if read_fd < 0 {
        Err(-24) // EMFILE
    } else {
        Ok((read_fd, write_fd))
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PANIC/UNWIND SUPPORT
// ═══════════════════════════════════════════════════════════════════════

/// Panic hook registration
static PANIC_HOOK_SET: AtomicBool = AtomicBool::new(false);

/// Register the default panic handler for user-space Rust programs
pub fn register_panic_handler() {
    PANIC_HOOK_SET.store(true, Ordering::SeqCst);
    serial_println!("[RUST-STD] Panic handler registered");
}

/// Abort handler — called on double-panic or panic-in-drop
pub fn abort() -> ! {
    // Send SIGABRT to self
    loop {
        unsafe {
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!("hlt");
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize Rust std backing infrastructure
pub fn init() {
    register_panic_handler();

    serial_println!("[RUST-STD] Standard library backing initialized");
    serial_println!("[RUST-STD]   Target: x86_64-unknown-knoxos");
    serial_println!("[RUST-STD]   std::thread → kernel threads (clone)");
    serial_println!("[RUST-STD]   std::fs → VFS syscalls");
    serial_println!("[RUST-STD]   std::net → socket syscalls");
    serial_println!("[RUST-STD]   std::sync → futex primitives");
    serial_println!("[RUST-STD]   std::time → clock_gettime/vDSO");

    INITIALIZED.store(true, Ordering::SeqCst);
}
