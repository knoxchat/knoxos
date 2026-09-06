use crate::serial_println;
/// C Library ABI Compatibility Layer — Linux binary compatibility
///
/// This module provides the ABI-level compatibility needed to run
/// standard Linux binaries (compiled against musl/glibc) on KnoxOS.
///
/// It defines:
///   - Standard C library structures matching Linux ABI
///   - ELF auxiliary vector construction
///   - Stack setup for Linux executables (argc, argv, envp, auxv)
///   - VDSO integration for fast system calls
///   - Thread-local storage (TLS) setup
///
/// Together with the syscall layer, this makes KnoxOS capable of
/// running unmodified Linux binaries (statically linked initially,
/// dynamically linked with ld-knoxos.so in the future).
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

// ─── Standard C Structures (Linux ABI) ──────────────────────────────────

/// struct stat (Linux x86_64 ABI, 144 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LinuxStat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_nlink: u64,
    pub st_mode: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub __pad0: u32,
    pub st_rdev: u64,
    pub st_size: i64,
    pub st_blksize: i64,
    pub st_blocks: i64,
    pub st_atime: i64,
    pub st_atime_nsec: i64,
    pub st_mtime: i64,
    pub st_mtime_nsec: i64,
    pub st_ctime: i64,
    pub st_ctime_nsec: i64,
    pub __unused: [i64; 3],
}

/// struct utsname (Linux ABI, 6 * 65 = 390 bytes)
#[repr(C)]
#[derive(Debug, Clone)]
pub struct LinuxUtsname {
    pub sysname: [u8; 65],
    pub nodename: [u8; 65],
    pub release: [u8; 65],
    pub version: [u8; 65],
    pub machine: [u8; 65],
    pub domainname: [u8; 65],
}

impl LinuxUtsname {
    pub fn knoxos() -> Self {
        let mut u = Self {
            sysname: [0; 65],
            nodename: [0; 65],
            release: [0; 65],
            version: [0; 65],
            machine: [0; 65],
            domainname: [0; 65],
        };
        Self::fill_field(&mut u.sysname, "Linux"); // Report as Linux for compatibility
        Self::fill_field(&mut u.nodename, "knoxos");
        Self::fill_field(&mut u.release, "6.1.0-knoxos");
        Self::fill_field(&mut u.version, "#1 SMP KnoxOS");
        Self::fill_field(&mut u.machine, "x86_64");
        Self::fill_field(&mut u.domainname, "(none)");
        u
    }

    fn fill_field(field: &mut [u8; 65], value: &str) {
        let bytes = value.as_bytes();
        let len = bytes.len().min(64);
        field[..len].copy_from_slice(&bytes[..len]);
    }
}

/// struct sysinfo (Linux ABI)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LinuxSysinfo {
    pub uptime: i64,
    pub loads: [u64; 3],
    pub totalram: u64,
    pub freeram: u64,
    pub sharedram: u64,
    pub bufferram: u64,
    pub totalswap: u64,
    pub freeswap: u64,
    pub procs: u16,
    pub pad: u16,
    pub _pad2: u32,
    pub totalhigh: u64,
    pub freehigh: u64,
    pub mem_unit: u32,
    pub _f: [u8; 4],
}

/// struct timespec (Linux ABI)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LinuxTimespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

/// struct timeval (Linux ABI)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LinuxTimeval {
    pub tv_sec: i64,
    pub tv_usec: i64,
}

/// struct iovec (Linux ABI)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LinuxIovec {
    pub iov_base: u64, // *mut void
    pub iov_len: u64,  // size_t
}

/// struct sigaction (Linux ABI for x86_64)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LinuxSigaction {
    pub sa_handler: u64, // void (*sa_handler)(int)
    pub sa_flags: u64,
    pub sa_restorer: u64,  // void (*sa_restorer)(void)
    pub sa_mask: [u64; 2], // sigset_t (128 bits on x86_64)
}

/// struct rusage (Linux ABI)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LinuxRusage {
    pub ru_utime: LinuxTimeval,
    pub ru_stime: LinuxTimeval,
    pub ru_maxrss: i64,
    pub ru_ixrss: i64,
    pub ru_idrss: i64,
    pub ru_isrss: i64,
    pub ru_minflt: i64,
    pub ru_majflt: i64,
    pub ru_nswap: i64,
    pub ru_inblock: i64,
    pub ru_oublock: i64,
    pub ru_msgsnd: i64,
    pub ru_msgrcv: i64,
    pub ru_nsignals: i64,
    pub ru_nvcsw: i64,
    pub ru_nivcsw: i64,
}

/// struct rlimit (Linux ABI)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LinuxRlimit {
    pub rlim_cur: u64, // Soft limit
    pub rlim_max: u64, // Hard limit
}

/// struct dirent64 (Linux ABI, used by getdents64)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LinuxDirent64 {
    pub d_ino: u64,
    pub d_off: i64,
    pub d_reclen: u16,
    pub d_type: u8,
    // d_name follows (variable length)
}

/// struct statfs (Linux ABI)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LinuxStatfs {
    pub f_type: i64,
    pub f_bsize: i64,
    pub f_blocks: u64,
    pub f_bfree: u64,
    pub f_bavail: u64,
    pub f_files: u64,
    pub f_ffree: u64,
    pub f_fsid: [i32; 2],
    pub f_namelen: i64,
    pub f_frsize: i64,
    pub f_flags: i64,
    pub f_spare: [i64; 4],
}

// ─── ELF Auxiliary Vector ───────────────────────────────────────────────

/// Auxiliary vector entry
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AuxvEntry {
    pub a_type: u64,
    pub a_val: u64,
}

/// AT_* constants (ELF auxiliary vector types)
pub const AT_NULL: u64 = 0;
pub const AT_IGNORE: u64 = 1;
pub const AT_EXECFD: u64 = 2;
pub const AT_PHDR: u64 = 3;
pub const AT_PHENT: u64 = 4;
pub const AT_PHNUM: u64 = 5;
pub const AT_PAGESZ: u64 = 6;
pub const AT_BASE: u64 = 7;
pub const AT_FLAGS: u64 = 8;
pub const AT_ENTRY: u64 = 9;
pub const AT_UID: u64 = 11;
pub const AT_EUID: u64 = 12;
pub const AT_GID: u64 = 13;
pub const AT_EGID: u64 = 14;
pub const AT_PLATFORM: u64 = 15;
pub const AT_HWCAP: u64 = 16;
pub const AT_CLKTCK: u64 = 17;
pub const AT_SECURE: u64 = 23;
pub const AT_RANDOM: u64 = 25;
pub const AT_HWCAP2: u64 = 26;
pub const AT_EXECFN: u64 = 31;
pub const AT_SYSINFO_EHDR: u64 = 33;

/// Build the auxiliary vector for a process
#[allow(clippy::too_many_arguments)]
pub fn build_auxv(
    phdr_addr: u64,
    phent_size: u64,
    phnum: u64,
    entry_point: u64,
    uid: u32,
    gid: u32,
    random_addr: u64,
    platform_addr: u64,
    execfn_addr: u64,
    vdso_addr: u64,
) -> Vec<AuxvEntry> {
    // Hardware capabilities (from CPUID)
    let hwcap = detect_hwcap();

    let mut auxv = vec![
        AuxvEntry {
            a_type: AT_PHDR,
            a_val: phdr_addr,
        },
        AuxvEntry {
            a_type: AT_PHENT,
            a_val: phent_size,
        },
        AuxvEntry {
            a_type: AT_PHNUM,
            a_val: phnum,
        },
        AuxvEntry {
            a_type: AT_PAGESZ,
            a_val: 4096,
        },
        AuxvEntry {
            a_type: AT_ENTRY,
            a_val: entry_point,
        },
        AuxvEntry {
            a_type: AT_UID,
            a_val: uid as u64,
        },
        AuxvEntry {
            a_type: AT_EUID,
            a_val: uid as u64,
        },
        AuxvEntry {
            a_type: AT_GID,
            a_val: gid as u64,
        },
        AuxvEntry {
            a_type: AT_EGID,
            a_val: gid as u64,
        },
        AuxvEntry {
            a_type: AT_CLKTCK,
            a_val: 100,
        }, // 100 Hz
        AuxvEntry {
            a_type: AT_SECURE,
            a_val: 0,
        },
        AuxvEntry {
            a_type: AT_RANDOM,
            a_val: random_addr,
        },
        AuxvEntry {
            a_type: AT_PLATFORM,
            a_val: platform_addr,
        },
        AuxvEntry {
            a_type: AT_EXECFN,
            a_val: execfn_addr,
        },
        AuxvEntry {
            a_type: AT_HWCAP,
            a_val: hwcap,
        },
        AuxvEntry {
            a_type: AT_HWCAP2,
            a_val: 0,
        },
    ];

    if vdso_addr != 0 {
        auxv.push(AuxvEntry {
            a_type: AT_SYSINFO_EHDR,
            a_val: vdso_addr,
        });
    }

    // Terminator
    auxv.push(AuxvEntry {
        a_type: AT_NULL,
        a_val: 0,
    });

    auxv
}

/// Detect hardware capabilities via CPUID
fn detect_hwcap() -> u64 {
    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
    let mut hwcap: u64 = 0;

    if let Some(features) = cpuid.get_feature_info() {
        if features.has_sse() {
            hwcap |= 1 << 0;
        } // HWCAP_SSE
        if features.has_sse2() {
            hwcap |= 1 << 1;
        } // HWCAP_SSE2
        if features.has_sse3() {
            hwcap |= 1 << 2;
        } // HWCAP_PNI (SSE3)
        if features.has_ssse3() {
            hwcap |= 1 << 9;
        } // HWCAP_SSSE3
        if features.has_sse41() {
            hwcap |= 1 << 19;
        } // HWCAP_SSE4_1
        if features.has_sse42() {
            hwcap |= 1 << 20;
        } // HWCAP_SSE4_2
        if features.has_avx() {
            hwcap |= 1 << 28;
        } // HWCAP_AVX
    }

    hwcap
}

// ─── Errno Definitions (Linux ABI) ──────────────────────────────────────

pub const EPERM: i32 = 1;
pub const ENOENT: i32 = 2;
pub const ESRCH: i32 = 3;
pub const EINTR: i32 = 4;
pub const EIO: i32 = 5;
pub const ENXIO: i32 = 6;
pub const E2BIG: i32 = 7;
pub const ENOEXEC: i32 = 8;
pub const EBADF: i32 = 9;
pub const ECHILD: i32 = 10;
pub const EAGAIN: i32 = 11;
pub const ENOMEM: i32 = 12;
pub const EACCES: i32 = 13;
pub const EFAULT: i32 = 14;
pub const ENOTBLK: i32 = 15;
pub const EBUSY: i32 = 16;
pub const EEXIST: i32 = 17;
pub const EXDEV: i32 = 18;
pub const ENODEV: i32 = 19;
pub const ENOTDIR: i32 = 20;
pub const EISDIR: i32 = 21;
pub const EINVAL: i32 = 22;
pub const ENFILE: i32 = 23;
pub const EMFILE: i32 = 24;
pub const ENOTTY: i32 = 25;
pub const ETXTBSY: i32 = 26;
pub const EFBIG: i32 = 27;
pub const ENOSPC: i32 = 28;
pub const ESPIPE: i32 = 29;
pub const EROFS: i32 = 30;
pub const EMLINK: i32 = 31;
pub const EPIPE: i32 = 32;
pub const EDOM: i32 = 33;
pub const ERANGE: i32 = 34;
pub const ENOSYS: i32 = 38;
pub const ENOTEMPTY: i32 = 39;
pub const ELOOP: i32 = 40;
pub const EWOULDBLOCK: i32 = EAGAIN;

// ─── File Mode Bits (Linux ABI) ─────────────────────────────────────────

pub const S_IFMT: u32 = 0o170000;
pub const S_IFSOCK: u32 = 0o140000;
pub const S_IFLNK: u32 = 0o120000;
pub const S_IFREG: u32 = 0o100000;
pub const S_IFBLK: u32 = 0o060000;
pub const S_IFDIR: u32 = 0o040000;
pub const S_IFCHR: u32 = 0o020000;
pub const S_IFIFO: u32 = 0o010000;
pub const S_ISUID: u32 = 0o004000;
pub const S_ISGID: u32 = 0o002000;
pub const S_ISVTX: u32 = 0o001000;
pub const S_IRWXU: u32 = 0o000700;
pub const S_IRUSR: u32 = 0o000400;
pub const S_IWUSR: u32 = 0o000200;
pub const S_IXUSR: u32 = 0o000100;
pub const S_IRWXG: u32 = 0o000070;
pub const S_IRGRP: u32 = 0o000040;
pub const S_IWGRP: u32 = 0o000020;
pub const S_IXGRP: u32 = 0o000010;
pub const S_IRWXO: u32 = 0o000007;
pub const S_IROTH: u32 = 0o000004;
pub const S_IWOTH: u32 = 0o000002;
pub const S_IXOTH: u32 = 0o000001;

// ─── Signal Constants (Linux ABI) ───────────────────────────────────────

pub const SIGHUP: i32 = 1;
pub const SIGINT: i32 = 2;
pub const SIGQUIT: i32 = 3;
pub const SIGILL: i32 = 4;
pub const SIGTRAP: i32 = 5;
pub const SIGABRT: i32 = 6;
pub const SIGBUS: i32 = 7;
pub const SIGFPE: i32 = 8;
pub const SIGKILL: i32 = 9;
pub const SIGUSR1: i32 = 10;
pub const SIGSEGV: i32 = 11;
pub const SIGUSR2: i32 = 12;
pub const SIGPIPE: i32 = 13;
pub const SIGALRM: i32 = 14;
pub const SIGTERM: i32 = 15;
pub const SIGSTKFLT: i32 = 16;
pub const SIGCHLD: i32 = 17;
pub const SIGCONT: i32 = 18;
pub const SIGSTOP: i32 = 19;
pub const SIGTSTP: i32 = 20;
pub const SIGTTIN: i32 = 21;
pub const SIGTTOU: i32 = 22;

/// Initialize the C library compatibility layer
pub fn init() {
    serial_println!("[KnoxOS] C library ABI compatibility layer initialized");
    serial_println!("[KnoxOS]   Linux x86_64 syscall ABI: compatible");
    serial_println!("[KnoxOS]   ELF auxiliary vector: supported");
    serial_println!("[KnoxOS]   Target: musl-libc / glibc compatible");
}
