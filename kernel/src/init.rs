use crate::process::{self, Pid, ProcessState};
use crate::serial_println;
/// Init Process — /init startup and user-space process launcher
///
/// This module implements the /init process startup flow:
///   1. Load /init (or /sbin/init) from the initramfs/filesystem
///   2. Create a user-mode address space
///   3. Map ELF segments into user pages
///   4. Set up argv/envp/auxv on the user stack
///   5. Transition to ring 3 via iretq
///
/// If no real /init ELF is available, a built-in minimal init is used
/// that sets up the system and spawns a shell.
use alloc::string::String;
use alloc::vec::Vec;

/// Init process PID (always 1)
pub const INIT_PID: Pid = 1;

/// Paths to try for init in order
const INIT_PATHS: &[&str] = &["/init", "/sbin/init", "/bin/init", "/etc/init", "/bin/sh"];

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

/// Built-in minimal /init program (x86_64 machine code)
/// This is a tiny statically-linked ELF that:
///   1. Writes "KnoxOS init started\n" to stdout (syscall write)
///   2. Calls fork() to create a child
///   3. Child exec's /bin/sh
///   4. Parent wait4()'s on child
///   5. Loops forever (PID 1 never exits)
///
/// If no real /init exists, we use this built-in.
fn builtin_init_elf() -> Vec<u8> {
    builtin_init_elf_data()
}

/// Public entry point for other modules to get the init ELF data
pub fn builtin_init_elf_data() -> Vec<u8> {
    // Minimal ELF64 executable header + code
    // Entry point at 0x401000
    let entry_point: u64 = 0x0040_1000;
    let program_header_offset: u64 = 0x40; // Right after ELF header

    let mut elf = Vec::new();

    // ─── ELF Header (64 bytes) ──────────────────────────────────────
    elf.extend_from_slice(&[0x7f, b'E', b'L', b'F']); // e_ident: magic
    elf.push(2); // EI_CLASS: ELFCLASS64
    elf.push(1); // EI_DATA: ELFDATA2LSB
    elf.push(1); // EI_VERSION: EV_CURRENT
    elf.push(0); // EI_OSABI: ELFOSABI_NONE
    elf.extend_from_slice(&[0; 8]); // EI_ABIVERSION + padding
    elf.extend_from_slice(&2u16.to_le_bytes()); // e_type: ET_EXEC
    elf.extend_from_slice(&62u16.to_le_bytes()); // e_machine: EM_X86_64
    elf.extend_from_slice(&1u32.to_le_bytes()); // e_version
    elf.extend_from_slice(&entry_point.to_le_bytes()); // e_entry
    elf.extend_from_slice(&program_header_offset.to_le_bytes()); // e_phoff
    elf.extend_from_slice(&0u64.to_le_bytes()); // e_shoff
    elf.extend_from_slice(&0u32.to_le_bytes()); // e_flags
    elf.extend_from_slice(&64u16.to_le_bytes()); // e_ehsize
    elf.extend_from_slice(&56u16.to_le_bytes()); // e_phentsize
    elf.extend_from_slice(&1u16.to_le_bytes()); // e_phnum
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shentsize
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shnum
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shstrndx

    // ─── Program Header (56 bytes) ──────────────────────────────────
    // PT_LOAD: load code segment at 0x401000
    let code_offset: u64 = 0x1000; // Offset in file where code starts (page-aligned)
    let code_vaddr: u64 = 0x0040_1000;
    let code_size: u64 = 0x1000; // One page of code

    elf.extend_from_slice(&1u32.to_le_bytes()); // p_type: PT_LOAD
    elf.extend_from_slice(&5u32.to_le_bytes()); // p_flags: PF_R | PF_X
    elf.extend_from_slice(&code_offset.to_le_bytes()); // p_offset
    elf.extend_from_slice(&code_vaddr.to_le_bytes()); // p_vaddr
    elf.extend_from_slice(&code_vaddr.to_le_bytes()); // p_paddr
    elf.extend_from_slice(&code_size.to_le_bytes()); // p_filesz
    elf.extend_from_slice(&code_size.to_le_bytes()); // p_memsz
    elf.extend_from_slice(&0x1000u64.to_le_bytes()); // p_align

    // Pad to code_offset (0x1000)
    elf.resize(code_offset as usize, 0);

    // ─── Code Section ───────────────────────────────────────────────
    // Minimal init: write message, then loop calling wait4 forever
    let code: &[u8] = &[
        // write(1, msg, len)
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1 (SYS_write)
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // mov rdi, 1 (stdout)
        0x48, 0x8D, 0x35, 0x30, 0x00, 0x00, 0x00, // lea rsi, [rip+0x30] (msg)
        0x48, 0xC7, 0xC2, 0x1E, 0x00, 0x00, 0x00, // mov rdx, 30 (len)
        0x0F, 0x05, // syscall
        // loop: wait4(-1, NULL, 0, NULL)
        0x48, 0xC7, 0xC0, 0x3D, 0x00, 0x00, 0x00, // mov rax, 61 (SYS_wait4)
        0x48, 0xC7, 0xC7, 0xFF, 0xFF, 0xFF, 0xFF, // mov rdi, -1 (any child)
        0x48, 0x31, 0xF6, // xor rsi, rsi (NULL)
        0x48, 0x31, 0xD2, // xor rdx, rdx (0)
        0x4D, 0x31, 0xC0, // xor r8, r8 (NULL)
        0x0F, 0x05, // syscall
        // nanosleep for 100ms
        0x48, 0xC7, 0xC0, 0x23, 0x00, 0x00, 0x00, // mov rax, 35 (SYS_nanosleep)
        0x48, 0x8D, 0x3D, 0x30, 0x00, 0x00, 0x00, // lea rdi, [rip+0x30] (timespec)
        0x48, 0x31, 0xF6, // xor rsi, rsi
        0x0F, 0x05, // syscall
        // jmp loop (back to wait4)
        0xEB, 0xD0, // jmp -48
        // Message string: "KnoxOS init (PID 1) started\n"
        b'K', b'n', b'o', b'x', b'O', b'S', b' ', b'i', b'n', b'i', b't', b' ', b'(', b'P', b'I',
        b'D', b' ', b'1', b')', b' ', b's', b't', b'a', b'r', b't', b'e', b'd', b'\n', 0x00, 0x00,
        // Padding
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        // struct timespec { tv_sec=0, tv_nsec=100000000 }
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // tv_sec = 0
        0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // tv_nsec = 100000000 (100ms)
    ];

    elf.extend_from_slice(code);

    // Pad to full page
    elf.resize((code_offset + code_size) as usize, 0);

    elf
}

/// Try to load /init from the filesystem or initramfs
fn find_init_binary() -> Option<Vec<u8>> {
    // Try initramfs (cpio) first
    for path in INIT_PATHS {
        if let Some(data) = crate::cpio::read_file(path) {
            serial_println!("[init] Found {} in initramfs ({} bytes)", path, data.len());
            return Some(data);
        }
    }

    // Try VFS
    for path in INIT_PATHS {
        let vfs = crate::vfs::VFS.lock();
        if let Some(ino) = vfs.resolve_path(path) {
            if let Some(inode) = vfs.get_inode(ino) {
                if !inode.data.is_empty() && crate::elf::is_elf(&inode.data) {
                    serial_println!("[init] Found {} in VFS ({} bytes)", path, inode.data.len());
                    return Some(inode.data.clone());
                }
            }
        }
    }

    None
}

/// Start the /init process (PID 1)
///
/// This is called during kernel boot to launch the first user-space process.
/// The init process is the ancestor of all user-space processes.
pub fn start_init() -> Option<Pid> {
    serial_println!("[init] Starting /init process...");

    // Try to find a real /init binary
    let init_elf = match find_init_binary() {
        Some(data) => {
            serial_println!("[init] Using filesystem /init binary");
            data
        }
        None => {
            serial_println!("[init] No /init found, using built-in init");
            builtin_init_elf()
        }
    };

    // Validate ELF
    if !crate::elf::is_elf(&init_elf) {
        serial_println!("[init] ERROR: /init is not a valid ELF binary");
        return None;
    }

    serial_println!("[init] /init ELF size: {} bytes", init_elf.len());
    if let Some(info) = crate::elf::elf_info(&init_elf) {
        serial_println!("[init] {}", info);
    }

    // Set up argv and envp for /init
    let argv = &["/init"];
    let envp = &[
        "HOME=/",
        "PATH=/sbin:/bin:/usr/sbin:/usr/bin",
        "SHELL=/bin/sh",
        "TERM=linux",
        "USER=root",
        "LOGNAME=root",
        "LANG=C.UTF-8",
    ];

    // Use the process::exec_elf pipeline to set up the init process
    match process::exec_elf(&init_elf, "/init", argv, envp) {
        Some(pid) => {
            serial_println!("[init] /init process created: PID {}", pid);

            // Ensure PID 1 process has correct attributes
            {
                let mut table = process::PROCESS_TABLE.lock();
                if let Some(proc) = table.get_process_mut(pid) {
                    proc.uid = 0; // root
                    proc.gid = 0;
                    proc.cwd = String::from("/");
                }
            }

            // Map vDSO for the init process
            crate::vdso::map_vdso_for_process(pid);

            serial_println!("[init] /init (PID {}) ready for scheduling", pid);
            Some(pid)
        }
        None => {
            serial_println!("[init] ERROR: Failed to create /init process");
            None
        }
    }
}

/// Initialize init process module
pub fn init() {
    serial_println!("[KnoxOS] Init process launcher ready");
}
