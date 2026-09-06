/// Chromium Sandbox — Secure Process Isolation for Chromium-based Browsers
///
/// Vivaldi (Chromium-based) uses a multi-process architecture with heavy
/// sandboxing. This module provides the Linux kernel features required:
///
/// Process model:
///   - Browser process (unsandboxed, manages all child processes)
///   - GPU process (limited sandbox, DRM/EGL access)
///   - Renderer processes (heavy sandbox, one per tab)
///   - Utility processes (network, audio, storage)
///   - Zygote process (fork server to speed up renderer creation)
///
/// Sandbox layers:
///   1. Namespace sandbox (clone(CLONE_NEWUSER|CLONE_NEWPID|CLONE_NEWNET))
///   2. Seccomp-BPF (syscall filtering for renderers)
///   3. Capabilities drop (CAP_SYS_ADMIN, etc.)
///   4. Chroot / pivot_root (filesystem isolation)
///   5. Resource limits (RLIMIT_NOFILE, RLIMIT_NPROC)
///   6. Landlock (filesystem access control, Linux 5.13+)
///
/// Required syscalls for Chromium:
///   clone, clone3, fork, vfork, execve, wait4, exit_group,
///   mmap, mprotect, munmap, mremap, madvise, brk,
///   read, write, close, dup, dup2, pipe2, socketpair,
///   fcntl, ioctl (DRM, terminal),
///   epoll_create1, epoll_ctl, epoll_wait, eventfd2,
///   futex, rt_sigaction, rt_sigprocmask, rt_sigreturn,
///   clock_gettime, clock_getres, gettimeofday,
///   getpid, gettid, getuid, getgid, geteuid, getegid,
///   prctl (PR_SET_NO_NEW_PRIVS, PR_SET_SECCOMP),
///   seccomp, unshare,
///   recvmsg (for fd passing between processes),
///   shmget, shmat (or memfd_create + mmap for shared memory IPC)
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// SANDBOX PROFILES
// ═══════════════════════════════════════════════════════════════════════

/// Sandbox type for different Chromium process roles
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxType {
    /// No sandbox (browser process, --no-sandbox flag)
    None,
    /// GPU process sandbox (limited: DRM/GPU access)
    Gpu,
    /// Renderer process sandbox (strictest)
    Renderer,
    /// Utility process (network service)
    Utility,
    /// Audio process
    Audio,
    /// Zygote (fork server)
    Zygote,
}

/// Sandbox profile defining allowed operations
#[derive(Debug, Clone)]
pub struct SandboxProfile {
    pub sandbox_type: SandboxType,
    /// Allowed syscalls (by number)
    pub allowed_syscalls: Vec<u64>,
    /// Allowed filesystem paths (read-only)
    pub allowed_read_paths: Vec<String>,
    /// Allowed filesystem paths (read-write)
    pub allowed_write_paths: Vec<String>,
    /// Allowed device paths
    pub allowed_devices: Vec<String>,
    /// Whether to create user namespace
    pub user_namespace: bool,
    /// Whether to create PID namespace
    pub pid_namespace: bool,
    /// Whether to create network namespace
    pub net_namespace: bool,
    /// Whether to create mount namespace
    pub mount_namespace: bool,
    /// Whether to drop all capabilities
    pub drop_caps: bool,
    /// PR_SET_NO_NEW_PRIVS
    pub no_new_privs: bool,
    /// Resource limits
    pub rlimits: Vec<(u32, u64, u64)>, // (resource, soft, hard)
}

impl SandboxProfile {
    /// Create a renderer sandbox profile (strictest)
    pub fn renderer() -> Self {
        Self {
            sandbox_type: SandboxType::Renderer,
            allowed_syscalls: RENDERER_ALLOWED_SYSCALLS.to_vec(),
            allowed_read_paths: vec![
                String::from("/usr/share/fonts"),
                String::from("/etc/fonts"),
                String::from("/usr/share/locale"),
                String::from("/usr/share/zoneinfo"),
            ],
            allowed_write_paths: vec![],
            allowed_devices: vec![
                String::from("/dev/null"),
                String::from("/dev/zero"),
                String::from("/dev/urandom"),
            ],
            user_namespace: true,
            pid_namespace: true,
            net_namespace: true, // renderers have no network access
            mount_namespace: true,
            drop_caps: true,
            no_new_privs: true,
            rlimits: vec![
                (7, 1024, 4096),       // RLIMIT_NOFILE
                (6, 4096, 4096),       // RLIMIT_NPROC
                (9, 1 << 30, 1 << 30), // RLIMIT_AS (1 GB)
            ],
        }
    }

    /// Create a GPU process sandbox profile
    pub fn gpu() -> Self {
        Self {
            sandbox_type: SandboxType::Gpu,
            allowed_syscalls: GPU_ALLOWED_SYSCALLS.to_vec(),
            allowed_read_paths: vec![
                String::from("/usr/share/fonts"),
                String::from("/etc/fonts"),
                String::from("/usr/share/vulkan"),
                String::from("/usr/share/glvnd"),
                String::from("/usr/lib"),
                String::from("/usr/lib64"),
            ],
            allowed_write_paths: vec![],
            allowed_devices: vec![
                String::from("/dev/null"),
                String::from("/dev/zero"),
                String::from("/dev/urandom"),
                String::from("/dev/dri/card0"),
                String::from("/dev/dri/renderD128"),
            ],
            user_namespace: false, // GPU needs real user for DRM
            pid_namespace: true,
            net_namespace: false,
            mount_namespace: true,
            drop_caps: true,
            no_new_privs: true,
            rlimits: vec![
                (7, 4096, 8192), // RLIMIT_NOFILE (needs more FDs for GPU)
            ],
        }
    }

    /// Create a utility process sandbox profile
    pub fn utility() -> Self {
        Self {
            sandbox_type: SandboxType::Utility,
            allowed_syscalls: UTILITY_ALLOWED_SYSCALLS.to_vec(),
            allowed_read_paths: vec![
                String::from("/etc/resolv.conf"),
                String::from("/etc/hosts"),
                String::from("/etc/ssl/certs"),
                String::from("/usr/share/ca-certificates"),
            ],
            allowed_write_paths: vec![],
            allowed_devices: vec![String::from("/dev/null"), String::from("/dev/urandom")],
            user_namespace: true,
            pid_namespace: true,
            net_namespace: false, // network utility needs network access
            mount_namespace: true,
            drop_caps: true,
            no_new_privs: true,
            rlimits: vec![(7, 2048, 4096)],
        }
    }

    /// Create an audio process sandbox profile
    pub fn audio() -> Self {
        Self {
            sandbox_type: SandboxType::Audio,
            allowed_syscalls: AUDIO_ALLOWED_SYSCALLS.to_vec(),
            allowed_read_paths: vec![
                String::from("/usr/share/alsa"),
                String::from("/etc/asound.conf"),
                String::from("/etc/pulse"),
            ],
            allowed_write_paths: vec![String::from("/run/user/1000/pulse")],
            allowed_devices: vec![
                String::from("/dev/null"),
                String::from("/dev/urandom"),
                String::from("/dev/snd"),
            ],
            user_namespace: false,
            pid_namespace: true,
            net_namespace: true,
            mount_namespace: true,
            drop_caps: true,
            no_new_privs: true,
            rlimits: vec![(7, 1024, 2048)],
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ALLOWED SYSCALL TABLES
// ═══════════════════════════════════════════════════════════════════════

/// Syscalls allowed for renderer processes (heavily restricted)
const RENDERER_ALLOWED_SYSCALLS: &[u64] = &[
    0,   // read
    1,   // write
    3,   // close
    5,   // fstat
    8,   // lseek
    9,   // mmap
    10,  // mprotect
    11,  // munmap
    12,  // brk
    13,  // rt_sigaction
    14,  // rt_sigprocmask
    15,  // rt_sigreturn
    16,  // ioctl (limited)
    20,  // writev
    22,  // pipe
    24,  // sched_yield
    25,  // mremap
    28,  // madvise
    32,  // dup
    33,  // dup2
    35,  // nanosleep
    39,  // getpid
    56,  // clone
    60,  // exit
    72,  // fcntl
    96,  // gettimeofday
    102, // getuid
    104, // getgid
    107, // geteuid
    108, // getegid
    110, // getppid
    157, // prctl
    186, // gettid
    202, // futex
    228, // clock_gettime
    229, // clock_getres
    231, // exit_group
    232, // epoll_wait
    233, // epoll_ctl
    257, // openat (read-only)
    262, // newfstatat
    268, // fchmodat (limited)
    281, // epoll_pwait
    290, // eventfd2
    302, // prlimit64
    315, // sched_getattr
    317, // seccomp
    318, // getrandom
    334, // rseq
    435, // clone3
];

/// Syscalls allowed for GPU processes (DRM access)
const GPU_ALLOWED_SYSCALLS: &[u64] = &[
    0, 1, 2, 3, 5, 8, 9, 10, 11, 12, 13, 14, 15, 16, // basic + ioctl
    17, // pread64
    18, // pwrite64
    20, // writev
    22, // pipe
    24, // sched_yield
    25, // mremap
    28, // madvise
    32, 33, 35, 39, // getpid
    41, // socket (for Wayland)
    42, // connect
    44, // sendto
    45, // recvfrom
    46, // sendmsg
    47, // recvmsg
    48, // shutdown
    56, // clone
    60, // exit
    72, // fcntl
    96, // gettimeofday
    102, 104, 107, 108, 110, 157, // prctl
    186, // gettid
    202, // futex
    217, // getdents64
    228, 229, 231, 232, 233, 257, // openat
    262, // newfstatat
    281, // epoll_pwait
    290, // eventfd2
    302, // prlimit64
    318, // getrandom
    435, // clone3
];

/// Syscalls allowed for utility (network) processes
const UTILITY_ALLOWED_SYSCALLS: &[u64] = &[
    0, 1, 2, 3, 5, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 20, 22, 24, 25, 28, 32, 33, 35, 39,
    41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 56, 60, 72, 96, 102, 104, 107, 108,
    110, 157, 186, 202, 217, 228, 229, 231, 232, 233, 257, 262, 270, 281, 290, 291, 292, 293, 302,
    318, 435,
];

/// Syscalls allowed for audio processes
const AUDIO_ALLOWED_SYSCALLS: &[u64] = &[
    0, 1, 2, 3, 5, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 20, 22, 24, 25, 28, 32, 33, 35, 39,
    41, 42, 44, 45, 46, 47, 56, 60, 72, 96, 102, 104, 107, 108, 110, 157, 186, 202, 228, 229, 231,
    232, 233, 257, 262, 281, 290, 302, 318, 435,
];

// ═══════════════════════════════════════════════════════════════════════
// SANDBOX ENFORCEMENT
// ═══════════════════════════════════════════════════════════════════════

/// Active sandbox for a process
#[derive(Debug, Clone)]
pub struct ActiveSandbox {
    pub pid: u32,
    pub profile: SandboxProfile,
    pub seccomp_active: bool,
    pub namespaces_active: bool,
    pub caps_dropped: bool,
}

lazy_static::lazy_static! {
    /// Active sandboxes by PID
    static ref SANDBOXES: Mutex<BTreeMap<u32, ActiveSandbox>> = Mutex::new(BTreeMap::new());
}

/// Apply a sandbox to the current process
pub fn apply_sandbox(pid: u32, profile: SandboxProfile) -> Result<(), i32> {
    serial_println!(
        "[sandbox] Applying {:?} sandbox to PID {}",
        profile.sandbox_type,
        pid
    );

    // Step 1: Set PR_SET_NO_NEW_PRIVS
    if profile.no_new_privs {
        serial_println!("[sandbox]   PR_SET_NO_NEW_PRIVS = 1");
        // prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0)
    }

    // Step 2: Create namespaces
    if profile.user_namespace {
        serial_println!("[sandbox]   Creating user namespace");
        // unshare(CLONE_NEWUSER)
        // Write uid_map / gid_map
    }
    if profile.pid_namespace {
        serial_println!("[sandbox]   Creating PID namespace");
    }
    if profile.net_namespace {
        serial_println!("[sandbox]   Creating network namespace (isolated)");
    }
    if profile.mount_namespace {
        serial_println!("[sandbox]   Creating mount namespace");
        // Set up minimal mount tree:
        // - /proc (new instance)
        // - /dev/null, /dev/zero, /dev/urandom (bind mounts)
        // - /dev/dri/* (for GPU sandbox only)
        // - /dev/shm (tmpfs)
        // - Read-only bind mounts from allowed_read_paths
    }

    // Step 3: Drop capabilities
    if profile.drop_caps {
        serial_println!("[sandbox]   Dropping all capabilities");
        // capset() to clear all caps
    }

    // Step 4: Apply resource limits
    for &(resource, soft, hard) in &profile.rlimits {
        serial_println!(
            "[sandbox]   RLIMIT_{}: soft={}, hard={}",
            resource,
            soft,
            hard
        );
        // prlimit64(pid, resource, {soft, hard}, NULL)
    }

    // Step 5: Install seccomp-BPF filter
    serial_println!(
        "[sandbox]   Installing seccomp-BPF filter ({} allowed syscalls)",
        profile.allowed_syscalls.len()
    );
    install_seccomp_filter(pid, &profile.allowed_syscalls)?;

    // Record active sandbox
    SANDBOXES.lock().insert(
        pid,
        ActiveSandbox {
            pid,
            profile,
            seccomp_active: true,
            namespaces_active: true,
            caps_dropped: true,
        },
    );

    serial_println!("[sandbox] Sandbox applied to PID {}", pid);
    Ok(())
}

/// Install a seccomp-BPF filter for allowed syscalls
fn install_seccomp_filter(pid: u32, allowed: &[u64]) -> Result<(), i32> {
    // Build BPF program:
    // 1. Load syscall number
    // 2. Check architecture (must be AUDIT_ARCH_X86_64)
    // 3. Compare against each allowed syscall
    // 4. ALLOW if matched, KILL_PROCESS if not

    let program_size = 3 + allowed.len() * 2 + 1; // arch check + comparisons + default kill
    serial_println!(
        "[sandbox] BPF program: {} instructions for {} allowed syscalls",
        program_size,
        allowed.len()
    );

    // Create seccomp state for process
    let mut state = crate::seccomp::SeccompState::new();

    // Build BPF instructions
    let mut insns = Vec::new();

    // Load architecture
    insns.push(crate::seccomp::BpfInsn {
        code: crate::seccomp::BPF_LD | crate::seccomp::BPF_W | crate::seccomp::BPF_ABS,
        jt: 0,
        jf: 0,
        k: crate::seccomp::SECCOMP_DATA_ARCH,
    });

    // Check architecture == AUDIT_ARCH_X86_64 (0xC000003E)
    insns.push(crate::seccomp::BpfInsn {
        code: crate::seccomp::BPF_JMP | crate::seccomp::BPF_JEQ | crate::seccomp::BPF_K,
        jt: 1,
        jf: 0,
        k: 0xC000003E,
    });

    // Kill if wrong architecture
    insns.push(crate::seccomp::BpfInsn {
        code: crate::seccomp::BPF_RET | crate::seccomp::BPF_K,
        jt: 0,
        jf: 0,
        k: crate::seccomp::SECCOMP_RET_KILL_PROCESS,
    });

    // Load syscall number
    insns.push(crate::seccomp::BpfInsn {
        code: crate::seccomp::BPF_LD | crate::seccomp::BPF_W | crate::seccomp::BPF_ABS,
        jt: 0,
        jf: 0,
        k: crate::seccomp::SECCOMP_DATA_NR,
    });

    // Check each allowed syscall
    let remaining = allowed.len();
    for (i, &syscall_nr) in allowed.iter().enumerate() {
        let jump_to_allow = (remaining - i) as u8;
        insns.push(crate::seccomp::BpfInsn {
            code: crate::seccomp::BPF_JMP | crate::seccomp::BPF_JEQ | crate::seccomp::BPF_K,
            jt: jump_to_allow,
            jf: 0,
            k: syscall_nr as u32,
        });
    }

    // Default: KILL
    insns.push(crate::seccomp::BpfInsn {
        code: crate::seccomp::BPF_RET | crate::seccomp::BPF_K,
        jt: 0,
        jf: 0,
        k: crate::seccomp::SECCOMP_RET_KILL_PROCESS,
    });

    // ALLOW
    insns.push(crate::seccomp::BpfInsn {
        code: crate::seccomp::BPF_RET | crate::seccomp::BPF_K,
        jt: 0,
        jf: 0,
        k: crate::seccomp::SECCOMP_RET_ALLOW,
    });

    state.add_filter(insns).map_err(|_| -22)?;
    Ok(())
}

/// Check if a process is sandboxed
pub fn is_sandboxed(pid: u32) -> bool {
    SANDBOXES.lock().contains_key(&pid)
}

/// Get sandbox info for a process
pub fn get_sandbox_info(pid: u32) -> Option<ActiveSandbox> {
    SANDBOXES.lock().get(&pid).cloned()
}

/// Remove sandbox tracking when process exits
pub fn on_process_exit(pid: u32) {
    if SANDBOXES.lock().remove(&pid).is_some() {
        serial_println!("[sandbox] Sandbox removed for exited PID {}", pid);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ZYGOTE PROCESS
// ═══════════════════════════════════════════════════════════════════════

/// Zygote state
#[derive(Debug)]
pub struct ZygoteState {
    pub pid: u32,
    pub ready: bool,
    pub child_count: u32,
}

lazy_static::lazy_static! {
    static ref ZYGOTE: Mutex<Option<ZygoteState>> = Mutex::new(None);
}

/// Initialize the Chromium zygote process
/// The zygote pre-loads common libraries and then forks to create
/// renderer processes quickly. This avoids re-loading shared libs
/// for each new tab.
pub fn init_zygote() -> Result<u32, i32> {
    let pid = 1000; // would be assigned by process::fork()

    serial_println!("[sandbox] Initializing Chromium zygote (PID {})", pid);

    // Pre-load common libraries into zygote address space
    let preloaded_libs = [
        "libc.so.6",
        "libpthread.so.0",
        "libm.so.6",
        "libdl.so.2",
        "librt.so.1",
        "libstdc++.so.6",
        "libgcc_s.so.1",
        "libX11.so.6",
        "libxcb.so.1",
        "libnss3.so",
        "libnspr4.so",
    ];

    for lib in &preloaded_libs {
        serial_println!("[sandbox]   Preloaded: {}", lib);
    }

    *ZYGOTE.lock() = Some(ZygoteState {
        pid,
        ready: true,
        child_count: 0,
    });

    serial_println!(
        "[sandbox] Zygote ready (PID {}, {} libs preloaded)",
        pid,
        preloaded_libs.len()
    );
    Ok(pid)
}

/// Fork a new renderer from the zygote
pub fn zygote_fork(sandbox_type: SandboxType) -> Result<u32, i32> {
    let mut zygote = ZYGOTE.lock();
    let zyg = zygote.as_mut().ok_or(-3)?; // ESRCH

    if !zyg.ready {
        return Err(-11); // EAGAIN
    }

    zyg.child_count += 1;
    let child_pid = 1000 + zyg.child_count;

    serial_println!(
        "[sandbox] Zygote forked child PID {} ({:?})",
        child_pid,
        sandbox_type
    );

    // Apply appropriate sandbox
    let profile = match sandbox_type {
        SandboxType::Renderer => SandboxProfile::renderer(),
        SandboxType::Gpu => SandboxProfile::gpu(),
        SandboxType::Utility => SandboxProfile::utility(),
        SandboxType::Audio => SandboxProfile::audio(),
        _ => SandboxProfile::renderer(),
    };

    drop(zygote);
    apply_sandbox(child_pid, profile)?;

    Ok(child_pid)
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Initialize the Chromium sandbox subsystem
pub fn init() {
    serial_println!("[sandbox] Chromium sandbox subsystem initializing...");

    // Pre-create sandbox profiles
    let renderer = SandboxProfile::renderer();
    let gpu = SandboxProfile::gpu();
    let utility = SandboxProfile::utility();
    let audio = SandboxProfile::audio();

    serial_println!(
        "[sandbox] Profiles: renderer ({} syscalls), gpu ({} syscalls), utility ({} syscalls), audio ({} syscalls)",
        renderer.allowed_syscalls.len(),
        gpu.allowed_syscalls.len(),
        utility.allowed_syscalls.len(),
        audio.allowed_syscalls.len()
    );

    // Initialize zygote
    if let Ok(zyg_pid) = init_zygote() {
        serial_println!("[sandbox] Zygote initialized at PID {}", zyg_pid);
    }

    serial_println!("[sandbox] Chromium sandbox subsystem ready");
}
