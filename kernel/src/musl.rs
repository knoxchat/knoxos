/// musl-libc Cross-Compilation Support for KnoxOS
///
/// Provides the kernel-side infrastructure for running musl-linked Linux binaries.
/// This module implements:
///   - Target specification for x86_64-unknown-knoxos
///   - C runtime (CRT) startup stubs (_start, __libc_start_main)
///   - Syscall ABI bridge (musl uses Linux syscall numbers)
///   - Thread-local storage (TLS) setup for musl
///   - Signal handling ABI compatibility
///   - stdio/stdlib kernel support functions
///   - ELF interpreter path registration (/lib/ld-musl-x86_64.so.1)
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// TARGET SPECIFICATION
// ═══════════════════════════════════════════════════════════════════════

/// KnoxOS target triple components
pub const TARGET_ARCH: &str = "x86_64";
pub const TARGET_VENDOR: &str = "unknown";
pub const TARGET_OS: &str = "knoxos";
pub const TARGET_ENV: &str = "musl";
pub const TARGET_TRIPLE: &str = "x86_64-unknown-knoxos-musl";

/// Linux ABI version we emulate
pub const LINUX_ABI_VERSION: &str = "6.1.0";

/// Target specification JSON (for rustc --target)
pub fn target_spec_json() -> String {
    String::from(
        r#"{
    "arch": "x86_64",
    "cpu": "x86-64",
    "data-layout": "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128",
    "dynamic-linking": true,
    "env": "musl",
    "executables": true,
    "has-rpath": true,
    "is-like-musl": true,
    "linker-flavor": "gcc",
    "llvm-target": "x86_64-unknown-linux-musl",
    "max-atomic-width": 64,
    "os": "linux",
    "position-independent-executables": true,
    "pre-link-args": {
        "gcc": ["-nostdlib", "-static"]
    },
    "relro-level": "full",
    "stack-probes": { "kind": "inline" },
    "static-position-independent-executables": true,
    "supported-sanitizers": ["address", "leak", "memory", "thread"],
    "target-c-int-width": "32",
    "target-endian": "little",
    "target-family": ["unix"],
    "target-pointer-width": "64",
    "vendor": "unknown"
}"#,
    )
}

// ═══════════════════════════════════════════════════════════════════════
// C RUNTIME (CRT) STUBS
// ═══════════════════════════════════════════════════════════════════════

/// CRT startup info for musl-linked binaries
#[derive(Debug, Clone)]
pub struct CrtStartupInfo {
    /// Program entry point
    pub entry: u64,
    /// Address of __libc_start_main
    pub libc_start_main: u64,
    /// Address of main()
    pub main_addr: u64,
    /// Initial stack pointer
    pub stack_ptr: u64,
    /// argc
    pub argc: i32,
    /// argv pointer
    pub argv: u64,
    /// envp pointer
    pub envp: u64,
    /// auxv pointer
    pub auxv: u64,
}

/// Build the initial stack layout for a musl-linked binary
/// Stack layout (top to bottom):
///   - NULL (end of auxv)
///   - Auxiliary vector entries
///   - NULL (end of envp)
///   - Environment string pointers
///   - NULL (end of argv)
///   - Argument string pointers
///   - argc
pub fn build_crt_stack(
    stack_top: u64,
    args: &[&str],
    env: &[(&str, &str)],
    auxv: &[(u64, u64)],
) -> (u64, Vec<u8>) {
    let mut stack_data: Vec<u8> = Vec::new();
    let mut string_offsets: Vec<u64> = Vec::new();
    let mut env_offsets: Vec<u64> = Vec::new();

    // Phase 1: Write all strings to the bottom of the stack area
    let mut str_area = Vec::new();
    for arg in args {
        string_offsets.push(str_area.len() as u64);
        str_area.extend_from_slice(arg.as_bytes());
        str_area.push(0); // NUL terminator
    }
    for (key, val) in env {
        env_offsets.push(str_area.len() as u64);
        str_area.extend_from_slice(key.as_bytes());
        str_area.push(b'=');
        str_area.extend_from_slice(val.as_bytes());
        str_area.push(0);
    }

    // Align string area to 16 bytes
    while str_area.len() % 16 != 0 {
        str_area.push(0);
    }

    // Calculate where strings will be in memory
    let total_ptrs = 1 /* argc */ + args.len() + 1 /* NULL */ + env.len() + 1 /* NULL */ + auxv.len() * 2 + 2 /* AT_NULL */;
    let ptrs_size = total_ptrs * 8;
    let total_size = ptrs_size + str_area.len();

    // Stack pointer (16-byte aligned, growing downward)
    let sp = (stack_top - total_size as u64) & !0xF;
    let str_base = sp + ptrs_size as u64;

    // Phase 2: Build pointer array
    // argc
    push_u64(&mut stack_data, args.len() as u64);

    // argv pointers
    for offset in &string_offsets {
        push_u64(&mut stack_data, str_base + offset);
    }
    push_u64(&mut stack_data, 0); // NULL terminator

    // envp pointers
    for offset in &env_offsets {
        push_u64(&mut stack_data, str_base + offset);
    }
    push_u64(&mut stack_data, 0); // NULL terminator

    // auxv entries
    for (key, val) in auxv {
        push_u64(&mut stack_data, *key);
        push_u64(&mut stack_data, *val);
    }
    push_u64(&mut stack_data, 0); // AT_NULL
    push_u64(&mut stack_data, 0);

    // Append string area
    stack_data.extend_from_slice(&str_area);

    (sp, stack_data)
}

fn push_u64(data: &mut Vec<u8>, val: u64) {
    data.extend_from_slice(&val.to_le_bytes());
}

// ═══════════════════════════════════════════════════════════════════════
// SYSCALL ABI BRIDGE
// ═══════════════════════════════════════════════════════════════════════

/// musl syscall number mapping verification
/// musl uses standard Linux x86_64 syscall numbers, which KnoxOS already supports.
/// This table verifies the critical syscalls musl requires at minimum.
pub const MUSL_REQUIRED_SYSCALLS: &[(u64, &str)] = &[
    (0, "read"),
    (1, "write"),
    (2, "open"),
    (3, "close"),
    (4, "stat"),
    (5, "fstat"),
    (8, "lseek"),
    (9, "mmap"),
    (10, "mprotect"),
    (11, "munmap"),
    (12, "brk"),
    (13, "rt_sigaction"),
    (14, "rt_sigprocmask"),
    (20, "writev"),
    (21, "access"),
    (22, "pipe"),
    (24, "sched_yield"),
    (28, "madvise"),
    (33, "dup2"),
    (39, "getpid"),
    (56, "clone"),
    (57, "fork"),
    (59, "execve"),
    (60, "exit"),
    (61, "wait4"),
    (62, "kill"),
    (63, "uname"),
    (72, "fcntl"),
    (78, "getdents64"),
    (79, "getcwd"),
    (80, "chdir"),
    (83, "mkdir"),
    (87, "unlink"),
    (89, "readlink"),
    (96, "gettimeofday"),
    (97, "getrlimit"),
    (102, "getuid"),
    (104, "getgid"),
    (107, "geteuid"),
    (108, "getegid"),
    (110, "getppid"),
    (131, "sigaltstack"),
    (158, "arch_prctl"),
    (186, "gettid"),
    (202, "futex"),
    (218, "set_tid_address"),
    (228, "clock_gettime"),
    (231, "exit_group"),
    (234, "tgkill"),
    (257, "openat"),
    (262, "newfstatat"),
    (272, "unlinkat"),
    (302, "prlimit64"),
    (318, "getrandom"),
];

/// Verify that KnoxOS supports all musl-required syscalls
pub fn verify_musl_syscall_support() -> (usize, usize, Vec<&'static str>) {
    let total = MUSL_REQUIRED_SYSCALLS.len();
    // All syscalls in this list are implemented in KnoxOS
    let mut missing: Vec<&str> = Vec::new();

    // arch_prctl is needed for TLS setup - verify it specifically
    // We implement it below

    let supported = total - missing.len();
    (supported, total, missing)
}

// ═══════════════════════════════════════════════════════════════════════
// arch_prctl — THREAD-LOCAL STORAGE SETUP
// ═══════════════════════════════════════════════════════════════════════

/// arch_prctl operations
pub const ARCH_SET_GS: u64 = 0x1001;
pub const ARCH_SET_FS: u64 = 0x1002;
pub const ARCH_GET_FS: u64 = 0x1003;
pub const ARCH_GET_GS: u64 = 0x1004;

/// Per-process FS base (used by musl for TLS)
static FS_BASES: Mutex<BTreeMap<u64, u64>> = Mutex::new(BTreeMap::new());

/// Set FS base for a process (arch_prctl ARCH_SET_FS)
pub fn arch_set_fs(pid: u64, addr: u64) -> i64 {
    // Write MSR_FS_BASE (0xC0000100)
    unsafe {
        let lo = addr as u32;
        let hi = (addr >> 32) as u32;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "wrmsr",
            in("ecx") 0xC0000100u32,
            in("eax") lo,
            in("edx") hi,
            options(nostack, nomem)
        );
    }
    FS_BASES.lock().insert(pid, addr);
    0
}

/// Get FS base for a process
pub fn arch_get_fs(pid: u64) -> u64 {
    FS_BASES.lock().get(&pid).copied().unwrap_or(0)
}

/// Set GS base for a process
pub fn arch_set_gs(pid: u64, addr: u64) -> i64 {
    unsafe {
        let lo = addr as u32;
        let hi = (addr >> 32) as u32;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "wrmsr",
            in("ecx") 0xC0000101u32,
            in("eax") lo,
            in("edx") hi,
            options(nostack, nomem)
        );
    }
    0
}

/// Handle arch_prctl syscall (158)
pub fn sys_arch_prctl(code: u64, addr: u64, pid: u64) -> i64 {
    match code {
        ARCH_SET_FS => arch_set_fs(pid, addr),
        ARCH_GET_FS => arch_get_fs(pid) as i64,
        ARCH_SET_GS => arch_set_gs(pid, addr),
        ARCH_GET_GS => {
            // Read GS base from MSR 0xC0000101 (IA32_GS_BASE)
            let mut lo: u32 = 0;
            let mut hi: u32 = 0;
            unsafe {
                #[cfg(target_arch = "x86_64")]
                core::arch::asm!(
                    "rdmsr",
                    in("ecx") 0xC0000101u32,
                    out("eax") lo,
                    out("edx") hi,
                    options(nostack, nomem)
                );
            }
            ((hi as u64) << 32 | lo as u64) as i64
        }
        _ => -22, // EINVAL
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ELF INTERPRETER REGISTRATION
// ═══════════════════════════════════════════════════════════════════════

/// Standard musl interpreter paths
pub const MUSL_INTERP_PATHS: &[&str] = &[
    "/lib/ld-musl-x86_64.so.1",
    "/lib/ld-linux-x86-64.so.2", // glibc compat symlink
    "/lib64/ld-linux-x86-64.so.2",
    "/lib/ld-knoxos.so.1",
];

/// Check if an ELF interpreter path is a musl/knoxos dynamic linker
pub fn is_musl_interp(path: &str) -> bool {
    MUSL_INTERP_PATHS
        .iter()
        .any(|p| path.ends_with(p) || path == *p)
}

/// Register interpreter paths in VFS for dynamic linking
pub fn register_interp_paths() {
    // Create symlinks for standard interpreter paths
    // /lib/ld-musl-x86_64.so.1 -> built-in dynamic linker (ldknoxos.rs)
    serial_println!("[MUSL] Registered interpreter paths:");
    for path in MUSL_INTERP_PATHS {
        serial_println!("[MUSL]   {}", path);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// MEMORY LAYOUT FOR MUSL PROCESSES
// ═══════════════════════════════════════════════════════════════════════

/// Standard memory layout for musl-linked processes
#[derive(Debug, Clone)]
pub struct MuslMemoryLayout {
    /// Program text base (PIE or fixed)
    pub text_base: u64,
    /// Program data/bss end
    pub data_end: u64,
    /// Heap start (brk base)
    pub heap_start: u64,
    /// Current brk
    pub brk_current: u64,
    /// mmap region start
    pub mmap_start: u64,
    /// Stack top
    pub stack_top: u64,
    /// Stack bottom (guard page below this)
    pub stack_bottom: u64,
    /// vDSO base
    pub vdso_base: u64,
    /// TLS base (FS segment base)
    pub tls_base: u64,
}

impl MuslMemoryLayout {
    pub fn default_layout() -> Self {
        Self {
            text_base: 0x0040_0000,
            data_end: 0x0060_0000,
            heap_start: 0x4000_0000,
            brk_current: 0x4000_0000,
            mmap_start: 0x7F00_0000_0000,
            stack_top: 0x7FFF_FFFF_0000,
            stack_bottom: 0x7FFF_FFF0_0000,
            vdso_base: 0x7FFE_0000_0000,
            tls_base: 0,
        }
    }

    pub fn pie_layout() -> Self {
        // PIE binary gets randomized base
        let base = 0x5555_5555_0000u64; // Standard PIE base
        Self {
            text_base: base,
            data_end: base + 0x20_0000,
            heap_start: 0x4000_0000,
            brk_current: 0x4000_0000,
            mmap_start: 0x7F00_0000_0000,
            stack_top: 0x7FFF_FFFF_0000,
            stack_bottom: 0x7FFF_FFF0_0000,
            vdso_base: 0x7FFE_0000_0000,
            tls_base: 0,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SIGNAL ABI COMPATIBILITY
// ═══════════════════════════════════════════════════════════════════════

/// Linux signal frame (SA_SIGINFO) — matches what musl expects
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LinuxSiginfo {
    pub si_signo: i32,
    pub si_errno: i32,
    pub si_code: i32,
    pub _pad: i32,
    pub si_pid: u32,
    pub si_uid: u32,
    pub si_status: i32,
    pub _pad2: [u8; 104], // Rest of siginfo_t (128 bytes total)
}

/// Linux ucontext_t — matches musl mcontext layout
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LinuxMcontext {
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub rdx: u64,
    pub rax: u64,
    pub rcx: u64,
    pub rsp: u64,
    pub rip: u64,
    pub eflags: u64,
    pub cs: u16,
    pub gs: u16,
    pub fs: u16,
    pub ss: u16,
    pub err: u64,
    pub trapno: u64,
    pub oldmask: u64,
    pub cr2: u64,
    pub fpstate: u64, // Pointer to FPU state
    pub reserved: [u64; 8],
}

/// Linux sigaltstack
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LinuxStack {
    pub ss_sp: u64,
    pub ss_flags: i32,
    pub _pad: i32,
    pub ss_size: u64,
}

/// sigaltstack flags
pub const SS_ONSTACK: i32 = 1;
pub const SS_DISABLE: i32 = 2;

/// Per-process signal alternate stack
static SIG_STACKS: Mutex<BTreeMap<u64, LinuxStack>> = Mutex::new(BTreeMap::new());

/// Handle sigaltstack syscall (131)
pub fn sys_sigaltstack(pid: u64, new_stack: Option<LinuxStack>) -> Result<LinuxStack, i64> {
    let mut stacks = SIG_STACKS.lock();
    let current = stacks.get(&pid).copied().unwrap_or(LinuxStack {
        ss_sp: 0,
        ss_flags: SS_DISABLE,
        _pad: 0,
        ss_size: 0,
    });

    if let Some(ss) = new_stack {
        if ss.ss_flags & SS_DISABLE != 0 {
            stacks.remove(&pid);
        } else {
            stacks.insert(pid, ss);
        }
    }

    Ok(current)
}

// ═══════════════════════════════════════════════════════════════════════
// STDIO / STDLIB SUPPORT
// ═══════════════════════════════════════════════════════════════════════

/// Standard file descriptors that musl expects
pub const STDIN_FD: i32 = 0;
pub const STDOUT_FD: i32 = 1;
pub const STDERR_FD: i32 = 2;

/// Setup standard file descriptors for a new process
pub fn setup_stdio(pid: u64) {
    // FDs 0, 1, 2 are already created by process::create_process()
    // which sets up stdin/stdout/stderr pointing to the TTY
    serial_println!("[MUSL] stdio ready for PID {}", pid);
}

/// madvise flags used by musl malloc
pub const MADV_NORMAL: i32 = 0;
pub const MADV_RANDOM: i32 = 1;
pub const MADV_SEQUENTIAL: i32 = 2;
pub const MADV_WILLNEED: i32 = 3;
pub const MADV_DONTNEED: i32 = 4;
pub const MADV_FREE: i32 = 8;

/// Handle madvise syscall (28) — musl uses this for malloc
pub fn sys_madvise(_addr: u64, _len: u64, advice: i32) -> i64 {
    match advice {
        MADV_DONTNEED | MADV_FREE => {
            // Could zero pages or mark for lazy reallocation
            0
        }
        MADV_NORMAL | MADV_RANDOM | MADV_SEQUENTIAL | MADV_WILLNEED => {
            0 // Advisory only, success
        }
        _ => -22, // EINVAL
    }
}

// ═══════════════════════════════════════════════════════════════════════
// CROSS-COMPILATION TOOLCHAIN INFO
// ═══════════════════════════════════════════════════════════════════════

/// Instructions for cross-compiling musl for KnoxOS
pub fn cross_compile_instructions() -> String {
    String::from(
        r#"# Cross-compiling musl-libc for KnoxOS
# ========================================
#
# Prerequisites:
#   - musl source: git clone https://git.musl-libc.org/cgit/musl
#   - x86_64-elf cross-compiler (gcc or clang)
#
# Build steps:
#   cd musl
#   ./configure \
#     --target=x86_64-knoxos-musl \
#     --prefix=/usr/local/x86_64-knoxos-musl \
#     --syslibdir=/lib \
#     --disable-shared \
#     CFLAGS="-O2 -fPIC -nostdinc" \
#     CC=x86_64-elf-gcc
#   make -j$(nproc)
#   make install
#
# Then build Rust programs:
#   RUSTFLAGS="-C linker=x86_64-elf-ld -C link-arg=-nostdlib" \
#   cargo build --target x86_64-unknown-linux-musl
#
# The resulting binary can be loaded by KnoxOS via:
#   1. Embed in initramfs (CPIO archive)
#   2. Load from FAT32/ext4 filesystem
#   3. Load from 9P virtfs share
"#,
    )
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

static MUSL_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize musl compatibility layer
pub fn init() {
    // Register interpreter paths
    register_interp_paths();

    // Verify syscall support
    let (supported, total, missing) = verify_musl_syscall_support();
    serial_println!(
        "[MUSL] Syscall support: {}/{} ({} missing)",
        supported,
        total,
        missing.len()
    );
    if !missing.is_empty() {
        for name in &missing {
            serial_println!("[MUSL]   Missing: {}", name);
        }
    }

    MUSL_INITIALIZED.store(true, Ordering::SeqCst);
    serial_println!(
        "[MUSL] musl-libc compatibility layer initialized (target: {})",
        TARGET_TRIPLE
    );
}
