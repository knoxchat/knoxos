use crate::process::Pid;

/// Init process PID (always 1)
pub const INIT_PID: Pid = 1;

/// Auxiliary vector entry types (for ELF)
#[repr(u64)]
#[derive(Debug, Clone, Copy)]
pub enum AuxvType {
    Null = 0,
    Phdr = 3,         // AT_PHDR — program headers address
    Phent = 4,        // AT_PHENT — program header entry size
    Phnum = 5,        // AT_PHNUM — number of program headers
    Pagesz = 6,       // AT_PAGESZ — system page size
    Base = 7,         // AT_BASE — interpreter base address
    Flags = 8,        // AT_FLAGS
    Entry = 9,        // AT_ENTRY — program entry point
    Uid = 11,         // AT_UID
    Euid = 12,        // AT_EUID
    Gid = 13,         // AT_GID
    Egid = 14,        // AT_EGID
    Platform = 15,    // AT_PLATFORM — "x86_64"
    Hwcap = 16,       // AT_HWCAP — hardware capabilities
    Clktck = 17,      // AT_CLKTCK — clock ticks per second
    Secure = 23,      // AT_SECURE — is suid/sgid?
    Random = 25,      // AT_RANDOM — address of 16 random bytes
    Hwcap2 = 26,      // AT_HWCAP2
    Execfn = 31,      // AT_EXECFN — filename of executed program
    SysinfoEhdr = 33, // AT_SYSINFO_EHDR — vDSO base address
}
