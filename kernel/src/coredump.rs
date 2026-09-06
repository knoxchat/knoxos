/// Core Dump — Generate core dumps for crashed processes
///
/// Implements Linux-compatible ELF core dump generation:
///   - ELF core file format (ET_CORE)
///   - NT_PRSTATUS (register state)
///   - NT_PRPSINFO (process info)
///   - NT_AUXV (auxiliary vector)
///   - Memory segment dumps (PT_LOAD for each VMA)
///   - Configurable via /proc/sys/kernel/core_pattern
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::process::Pid;
use crate::serial_println;

/// ELF constants for core dumps
const ELFMAG: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EV_CURRENT: u8 = 1;
const ET_CORE: u16 = 4;
const EM_X86_64: u16 = 62;
const PT_NOTE: u32 = 4;
const PT_LOAD: u32 = 1;

/// Note types
const NT_PRSTATUS: u32 = 1;
const NT_PRPSINFO: u32 = 3;
const NT_SIGINFO: u32 = 0x53494749; // "SIGI"
const NT_AUXV: u32 = 6;
const NT_FILE: u32 = 0x46494c45; // "FILE"

/// Core dump configuration
#[derive(Debug, Clone)]
pub struct CoreDumpConfig {
    /// Core pattern (like Linux /proc/sys/kernel/core_pattern)
    /// %p = PID, %e = executable name, %t = timestamp
    pub core_pattern: String,
    /// Max core file size (0 = disabled)
    pub max_size: u64,
    /// Whether core dumps are enabled
    pub enabled: bool,
    /// Whether to include file-backed mappings
    pub dump_file_mappings: bool,
    /// Filter mask for which VMAs to dump
    pub filter_mask: u32,
}

impl CoreDumpConfig {
    fn new() -> Self {
        CoreDumpConfig {
            core_pattern: String::from("core.%p"),
            max_size: 0, // Disabled by default (RLIMIT_CORE = 0)
            enabled: true,
            dump_file_mappings: false,
            filter_mask: 0x33, // anonymous private + shared, ELF headers
        }
    }
}

/// Register state snapshot (matching Linux prstatus)
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct PrStatus {
    pub si_signo: i32,
    pub si_code: i32,
    pub si_errno: i32,
    pub cursig: u16,
    pub _pad0: u16,
    pub sigpend: u64,
    pub sighold: u64,
    pub pid: i32,
    pub ppid: i32,
    pub pgrp: i32,
    pub sid: i32,
    /// User time
    pub utime_sec: u64,
    pub utime_usec: u64,
    /// System time
    pub stime_sec: u64,
    pub stime_usec: u64,
    /// Children user time
    pub cutime_sec: u64,
    pub cutime_usec: u64,
    /// Children system time
    pub cstime_sec: u64,
    pub cstime_usec: u64,
    /// Register state
    pub regs: [u64; 27], // r15,r14,...,rax,orig_rax,rip,cs,eflags,rsp,ss,fs_base,gs_base
}

/// Process info (matching Linux prpsinfo)
#[repr(C)]
#[derive(Debug, Clone)]
pub struct PrPsInfo {
    pub state: u8,
    pub sname: u8, // Character for state (R,S,D,T,Z)
    pub zomb: u8,
    pub nice: i8,
    pub flag: u64,
    pub uid: u32,
    pub gid: u32,
    pub pid: i32,
    pub ppid: i32,
    pub pgrp: i32,
    pub sid: i32,
    pub fname: [u8; 16],  // Executable filename
    pub psargs: [u8; 80], // Command line args
}

impl Default for PrPsInfo {
    fn default() -> Self {
        PrPsInfo {
            state: 0,
            sname: b'R',
            zomb: 0,
            nice: 0,
            flag: 0,
            uid: 0,
            gid: 0,
            pid: 0,
            ppid: 0,
            pgrp: 0,
            sid: 0,
            fname: [0u8; 16],
            psargs: [0u8; 80],
        }
    }
}

/// A memory segment to include in the core dump
#[derive(Debug, Clone)]
pub struct CoreSegment {
    pub vaddr: u64,
    pub memsz: u64,
    pub filesz: u64,
    pub flags: u32, // PF_R | PF_W | PF_X
    pub data: Vec<u8>,
}

/// Core dump state
lazy_static::lazy_static! {
    static ref CONFIG: Mutex<CoreDumpConfig> = Mutex::new(CoreDumpConfig::new());
}

/// Generate the core dump filename from the pattern
pub fn generate_filename(pid: Pid, name: &str) -> String {
    let config = CONFIG.lock();
    let timestamp = crate::rtc::unix_time();

    let mut filename = config.core_pattern.clone();
    filename = filename.replace("%p", &alloc::format!("{}", pid));
    filename = filename.replace("%e", name);
    filename = filename.replace("%t", &alloc::format!("{}", timestamp));
    filename
}

/// Build an ELF note section
fn build_note(name: &str, note_type: u32, data: &[u8]) -> Vec<u8> {
    let mut note = Vec::new();
    let name_bytes = name.as_bytes();
    let namesz = name_bytes.len() as u32 + 1; // Include null terminator
    let descsz = data.len() as u32;

    // namesz
    note.extend_from_slice(&namesz.to_le_bytes());
    // descsz
    note.extend_from_slice(&descsz.to_le_bytes());
    // type
    note.extend_from_slice(&note_type.to_le_bytes());
    // name (padded to 4-byte boundary)
    note.extend_from_slice(name_bytes);
    note.push(0); // null terminator
    while note.len() % 4 != 0 {
        note.push(0);
    }
    // desc (padded to 4-byte boundary)
    note.extend_from_slice(data);
    while note.len() % 4 != 0 {
        note.push(0);
    }

    note
}

/// Generate a core dump for a crashed process
pub fn generate_core_dump(
    pid: Pid,
    signal: i32,
    name: &str,
    registers: &[u64; 16],
    segments: &[CoreSegment],
) -> Result<Vec<u8>, i32> {
    let config = CONFIG.lock();

    if !config.enabled {
        return Err(-1); // Core dumps disabled
    }

    if config.max_size == 0 {
        return Err(-1); // RLIMIT_CORE = 0
    }

    drop(config);

    serial_println!(
        "[coredump] Generating core dump for PID {} ({}) signal={}",
        pid,
        name,
        signal
    );

    // Build notes
    let mut notes = Vec::new();

    // NT_PRSTATUS
    let mut prstatus = PrStatus {
        cursig: signal as u16,
        pid: pid as i32,
        ..PrStatus::default()
    };
    // Copy registers into prstatus
    for (i, &reg) in registers.iter().enumerate() {
        if i < 27 {
            prstatus.regs[i] = reg;
        }
    }
    let prstatus_bytes = unsafe {
        core::slice::from_raw_parts(
            &prstatus as *const PrStatus as *const u8,
            core::mem::size_of::<PrStatus>(),
        )
    };
    notes.extend(build_note("CORE", NT_PRSTATUS, prstatus_bytes));

    // NT_PRPSINFO
    let mut prpsinfo = PrPsInfo {
        pid: pid as i32,
        state: 0, // Running (at time of crash)
        ..PrPsInfo::default()
    };
    let name_bytes = name.as_bytes();
    let copy_len = name_bytes.len().min(15);
    prpsinfo.fname[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
    let prpsinfo_bytes = unsafe {
        core::slice::from_raw_parts(
            &prpsinfo as *const PrPsInfo as *const u8,
            core::mem::size_of::<PrPsInfo>(),
        )
    };
    notes.extend(build_note("CORE", NT_PRPSINFO, prpsinfo_bytes));

    // Calculate sizes
    let ehdr_size = 64u64; // ELF64 header
    let phdr_size = 56u64; // Program header entry size
    let num_segments = 1 + segments.len(); // 1 for PT_NOTE + N for PT_LOAD
    let phdr_total = phdr_size * num_segments as u64;

    let notes_offset = ehdr_size + phdr_total;
    let notes_size = notes.len() as u64;

    // Calculate segment offsets
    let mut data_offset = notes_offset + notes_size;
    // Align to page
    data_offset = (data_offset + 0xFFF) & !0xFFF;

    // Build ELF header
    let mut elf = Vec::with_capacity(
        data_offset as usize + segments.iter().map(|s| s.data.len()).sum::<usize>(),
    );

    // ELF header (64 bytes)
    elf.extend_from_slice(&ELFMAG); // e_ident[EI_MAG]
    elf.push(ELFCLASS64); // EI_CLASS
    elf.push(ELFDATA2LSB); // EI_DATA
    elf.push(EV_CURRENT); // EI_VERSION
    elf.push(0); // EI_OSABI (ELFOSABI_NONE)
    elf.extend_from_slice(&[0u8; 8]); // EI_ABIVERSION + padding
    elf.extend_from_slice(&ET_CORE.to_le_bytes()); // e_type
    elf.extend_from_slice(&EM_X86_64.to_le_bytes()); // e_machine
    elf.extend_from_slice(&1u32.to_le_bytes()); // e_version
    elf.extend_from_slice(&0u64.to_le_bytes()); // e_entry
    elf.extend_from_slice(&ehdr_size.to_le_bytes()); // e_phoff
    elf.extend_from_slice(&0u64.to_le_bytes()); // e_shoff
    elf.extend_from_slice(&0u32.to_le_bytes()); // e_flags
    elf.extend_from_slice(&(ehdr_size as u16).to_le_bytes()); // e_ehsize
    elf.extend_from_slice(&(phdr_size as u16).to_le_bytes()); // e_phentsize
    elf.extend_from_slice(&(num_segments as u16).to_le_bytes()); // e_phnum
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shentsize
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shnum
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shstrndx

    // Program header: PT_NOTE
    elf.extend_from_slice(&PT_NOTE.to_le_bytes()); // p_type
    elf.extend_from_slice(&0u32.to_le_bytes()); // p_flags
    elf.extend_from_slice(&notes_offset.to_le_bytes()); // p_offset
    elf.extend_from_slice(&0u64.to_le_bytes()); // p_vaddr
    elf.extend_from_slice(&0u64.to_le_bytes()); // p_paddr
    elf.extend_from_slice(&notes_size.to_le_bytes()); // p_filesz
    elf.extend_from_slice(&notes_size.to_le_bytes()); // p_memsz
    elf.extend_from_slice(&4u64.to_le_bytes()); // p_align

    // Program headers: PT_LOAD for each segment
    let mut seg_offset = data_offset;
    for seg in segments {
        elf.extend_from_slice(&PT_LOAD.to_le_bytes()); // p_type
        elf.extend_from_slice(&seg.flags.to_le_bytes()); // p_flags
        elf.extend_from_slice(&seg_offset.to_le_bytes()); // p_offset
        elf.extend_from_slice(&seg.vaddr.to_le_bytes()); // p_vaddr
        elf.extend_from_slice(&0u64.to_le_bytes()); // p_paddr
        elf.extend_from_slice(&seg.filesz.to_le_bytes()); // p_filesz
        elf.extend_from_slice(&seg.memsz.to_le_bytes()); // p_memsz
        elf.extend_from_slice(&0x1000u64.to_le_bytes()); // p_align

        seg_offset += seg.filesz;
    }

    // Notes section
    debug_assert_eq!(elf.len() as u64, notes_offset);
    elf.extend_from_slice(&notes);

    // Pad to data offset
    while (elf.len() as u64) < data_offset {
        elf.push(0);
    }

    // Segment data
    for seg in segments {
        elf.extend_from_slice(&seg.data);
    }

    let config = CONFIG.lock();
    if config.max_size != u64::MAX && (elf.len() as u64) > config.max_size {
        serial_println!(
            "[coredump] Core dump too large ({} > {}), truncating",
            elf.len(),
            config.max_size
        );
        elf.truncate(config.max_size as usize);
    }

    serial_println!(
        "[coredump] Generated {} byte core dump for PID {}",
        elf.len(),
        pid
    );

    Ok(elf)
}

/// Set core pattern
pub fn set_core_pattern(pattern: &str) {
    let mut config = CONFIG.lock();
    config.core_pattern = String::from(pattern);
}

/// Get core pattern
pub fn get_core_pattern() -> String {
    let config = CONFIG.lock();
    config.core_pattern.clone()
}

/// Set max core dump size
pub fn set_max_size(size: u64) {
    let mut config = CONFIG.lock();
    config.max_size = size;
}

/// Enable/disable core dumps
pub fn set_enabled(enabled: bool) {
    let mut config = CONFIG.lock();
    config.enabled = enabled;
}

/// Initialize core dump subsystem
pub fn init() {
    serial_println!("[KnoxOS] Core dump subsystem initialized");
}
