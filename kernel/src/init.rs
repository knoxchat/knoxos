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

/// Built-in Gate B2 hello: `write(1, "hello from userspace\n", 21)` then `exit(0)`.
///
/// Loaded at 512 GiB (L4 index 1) so it does not collide with the kernel's
/// own PT_LOAD at vaddr 0 (which occupies 0x401000 in the current CR3).
pub fn hello_userspace_elf_data() -> Vec<u8> {
    let entry_point: u64 = 0x0000_0080_0000_1000;
    let program_header_offset: u64 = 0x40;
    let mut elf = Vec::new();

    elf.extend_from_slice(&[0x7f, b'E', b'L', b'F']);
    elf.push(2); // ELFCLASS64
    elf.push(1); // ELFDATA2LSB
    elf.push(1); // EV_CURRENT
    elf.push(0);
    elf.extend_from_slice(&[0; 8]);
    elf.extend_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    elf.extend_from_slice(&62u16.to_le_bytes()); // EM_X86_64
    elf.extend_from_slice(&1u32.to_le_bytes());
    elf.extend_from_slice(&entry_point.to_le_bytes());
    elf.extend_from_slice(&program_header_offset.to_le_bytes());
    elf.extend_from_slice(&0u64.to_le_bytes());
    elf.extend_from_slice(&0u32.to_le_bytes());
    elf.extend_from_slice(&64u16.to_le_bytes());
    elf.extend_from_slice(&56u16.to_le_bytes());
    elf.extend_from_slice(&1u16.to_le_bytes());
    elf.extend_from_slice(&0u16.to_le_bytes());
    elf.extend_from_slice(&0u16.to_le_bytes());
    elf.extend_from_slice(&0u16.to_le_bytes());

    let code_offset: u64 = 0x1000;
    let code_vaddr: u64 = 0x0000_0080_0000_1000;
    let code_size: u64 = 0x1000;

    elf.extend_from_slice(&1u32.to_le_bytes()); // PT_LOAD
    elf.extend_from_slice(&5u32.to_le_bytes()); // PF_R | PF_X
    elf.extend_from_slice(&code_offset.to_le_bytes());
    elf.extend_from_slice(&code_vaddr.to_le_bytes());
    elf.extend_from_slice(&code_vaddr.to_le_bytes());
    elf.extend_from_slice(&code_size.to_le_bytes());
    elf.extend_from_slice(&code_size.to_le_bytes());
    elf.extend_from_slice(&0x1000u64.to_le_bytes());

    elf.resize(code_offset as usize, 0);

    // write(1, msg, 21); exit(0);
    // lea rsi, [rip+0x15] — RIP after lea is +0x15, message is at +0x2A.
    let code: &[u8] = &[
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1 (SYS_write)
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // mov rdi, 1 (stdout)
        0x48, 0x8D, 0x35, 0x15, 0x00, 0x00, 0x00, // lea rsi, [rip+0x15]
        0x48, 0xC7, 0xC2, 0x15, 0x00, 0x00, 0x00, // mov rdx, 21
        0x0F, 0x05, // syscall
        0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, // mov rax, 60 (SYS_exit)
        0x48, 0x31, 0xFF, // xor rdi, rdi
        0x0F, 0x05, // syscall
        b'h', b'e', b'l', b'l', b'o', b' ', b'f', b'r', b'o', b'm', b' ', b'u', b's', b'e', b'r',
        b's', b'p', b'a', b'c', b'e', b'\n',
    ];
    elf.extend_from_slice(code);
    elf.resize((code_offset + code_size) as usize, 0);
    elf
}

/// Pack `code` into a static ELF64 loaded at the Gate B2 vaddr (L4 index 1).
pub fn build_static_user_elf(code: &[u8]) -> Vec<u8> {
    let entry_point: u64 = 0x0000_0080_0000_1000;
    let program_header_offset: u64 = 0x40;
    let mut elf = Vec::new();

    elf.extend_from_slice(&[0x7f, b'E', b'L', b'F']);
    elf.push(2);
    elf.push(1);
    elf.push(1);
    elf.push(0);
    elf.extend_from_slice(&[0; 8]);
    elf.extend_from_slice(&2u16.to_le_bytes());
    elf.extend_from_slice(&62u16.to_le_bytes());
    elf.extend_from_slice(&1u32.to_le_bytes());
    elf.extend_from_slice(&entry_point.to_le_bytes());
    elf.extend_from_slice(&program_header_offset.to_le_bytes());
    elf.extend_from_slice(&0u64.to_le_bytes());
    elf.extend_from_slice(&0u32.to_le_bytes());
    elf.extend_from_slice(&64u16.to_le_bytes());
    elf.extend_from_slice(&56u16.to_le_bytes());
    elf.extend_from_slice(&1u16.to_le_bytes());
    elf.extend_from_slice(&0u16.to_le_bytes());
    elf.extend_from_slice(&0u16.to_le_bytes());
    elf.extend_from_slice(&0u16.to_le_bytes());

    let code_offset: u64 = 0x1000;
    let code_vaddr: u64 = 0x0000_0080_0000_1000;
    let code_size: u64 = 0x1000;

    elf.extend_from_slice(&1u32.to_le_bytes());
    elf.extend_from_slice(&5u32.to_le_bytes());
    elf.extend_from_slice(&code_offset.to_le_bytes());
    elf.extend_from_slice(&code_vaddr.to_le_bytes());
    elf.extend_from_slice(&code_vaddr.to_le_bytes());
    elf.extend_from_slice(&code_size.to_le_bytes());
    elf.extend_from_slice(&code_size.to_le_bytes());
    elf.extend_from_slice(&0x1000u64.to_le_bytes());

    elf.resize(code_offset as usize, 0);
    elf.extend_from_slice(code);
    elf.resize((code_offset + code_size) as usize, 0);
    elf
}

/// `execve("/bin/hello", NULL, NULL)` then `exit(1)` if execve returns.
pub fn exec_hello_elf_data() -> Vec<u8> {
    // lea rdi, [rip+path]; xor rsi,rsi; xor rdx,rdx; mov rax,59; syscall;
    // mov rax,60; mov rdi,1; syscall; "/bin/hello\0"
    build_static_user_elf(&[
        0x48, 0x8D, 0x3D, 0x1F, 0x00, 0x00, 0x00, // lea rdi, [rip+0x1F]
        0x48, 0x31, 0xF6, // xor rsi, rsi
        0x48, 0x31, 0xD2, // xor rdx, rdx
        0x48, 0xC7, 0xC0, 0x3B, 0x00, 0x00, 0x00, // mov rax, 59
        0x0F, 0x05, // syscall
        0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, // mov rax, 60
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // mov rdi, 1
        0x0F, 0x05, // syscall
        b'/', b'b', b'i', b'n', b'/', b'h', b'e', b'l', b'l', b'o', 0,
    ])
}

/// `fork`; child writes "fork child ran\n" and exits; parent `wait4`s then
/// writes `GATE_B4 fork complete\n`.
pub fn fork_userspace_elf_data() -> Vec<u8> {
    build_static_user_elf(&[
        0x48, 0xC7, 0xC0, 0x39, 0x00, 0x00, 0x00, // mov rax, 57
        0x0F, 0x05, // syscall
        0x48, 0x85, 0xC0, // test rax, rax
        0x75, 0x2A, // jnz parent (0x38)
        // child
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // mov rdi, 1
        0x48, 0x8D, 0x35, 0x54, 0x00, 0x00, 0x00, // lea rsi, [rip+0x54]
        0x48, 0xC7, 0xC2, 0x0F, 0x00, 0x00, 0x00, // mov rdx, 15
        0x0F, 0x05, // syscall
        0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, // mov rax, 60
        0x48, 0x31, 0xFF, // xor rdi, rdi
        0x0F, 0x05, // syscall
        // parent at 0x38
        0x48, 0x89, 0xC7, // mov rdi, rax
        0x48, 0x31, 0xF6, // xor rsi, rsi
        0x48, 0x31, 0xD2, // xor rdx, rdx
        0x4D, 0x31, 0xD2, // xor r10, r10
        0x48, 0xC7, 0xC0, 0x3D, 0x00, 0x00, 0x00, // mov rax, 61
        0x0F, 0x05, // syscall
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // mov rdi, 1
        0x48, 0x8D, 0x35, 0x24, 0x00, 0x00, 0x00, // lea rsi, [rip+0x24]
        0x48, 0xC7, 0xC2, 0x16, 0x00, 0x00, 0x00, // mov rdx, 22
        0x0F, 0x05, // syscall
        0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, // mov rax, 60
        0x48, 0x31, 0xFF, // xor rdi, rdi
        0x0F, 0x05, // syscall
        b'f', b'o', b'r', b'k', b' ', b'c', b'h', b'i', b'l', b'd', b' ', b'r', b'a', b'n', b'\n',
        b'G', b'A', b'T', b'E', b'_', b'B', b'4', b' ', b'f', b'o', b'r', b'k', b' ', b'c', b'o',
        b'm', b'p', b'l', b'e', b't', b'e', b'\n',
    ])
}

/// Magic RBX the Gate I2 spinner loads before `jmp $`.
pub const SPIN_RBX_MAGIC: u64 = 0x0123_4567_89AB_CDEF;

/// Infinite `jmp $` — never syscalls, so only a timer can switch it out.
/// Loads a distinctive RBX so IRQ preemption can prove GPR save (Gate I2).
pub fn spin_userspace_elf_data() -> Vec<u8> {
    // mov rbx, SPIN_RBX_MAGIC; jmp $
    // Do not `sti` here: Ring 3 STI #GPs unless IOPL=3.
    build_static_user_elf(&[
        0x48, 0xBB, 0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23, 0x01, // mov rbx, magic
        0xEB, 0xFE, // jmp $
    ])
}

/// Write `preempt writer ran` then `exit(0)`.
pub fn preempt_writer_elf_data() -> Vec<u8> {
    build_static_user_elf(&[
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // mov rdi, 1
        0x48, 0x8D, 0x35, 0x15, 0x00, 0x00, 0x00, // lea rsi, [rip+0x15]
        0x48, 0xC7, 0xC2, 0x13, 0x00, 0x00, 0x00, // mov rdx, 19
        0x0F, 0x05, // syscall
        0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, // mov rax, 60
        0x48, 0x31, 0xFF, // xor rdi, rdi
        0x0F, 0x05, // syscall
        b'p', b'r', b'e', b'e', b'm', b'p', b't', b' ', b'w', b'r', b'i', b't', b'e', b'r', b' ',
        b'r', b'a', b'n', b'\n',
    ])
}

/// `pause()` then `exit(1)` if it ever returns.
pub fn pause_userspace_elf_data() -> Vec<u8> {
    build_static_user_elf(&[
        0x48, 0xC7, 0xC0, 0x22, 0x00, 0x00, 0x00, // mov rax, 34
        0x0F, 0x05, // syscall
        0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, // mov rax, 60
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // mov rdi, 1
        0x0F, 0x05, // syscall
    ])
}

/// `mov rax, [0]` — #PF in Ring 3, delivered as SIGSEGV.
pub fn segfault_userspace_elf_data() -> Vec<u8> {
    build_static_user_elf(&[
        0x48, 0x8B, 0x04, 0x25, 0x00, 0x00, 0x00, 0x00, // mov rax, [0]
    ])
}

/// Gate D1: two UDP sockets, bind 127.0.0.1:4242, sendto "ping", recvfrom, write marker.
pub fn loopback_userspace_elf_data() -> Vec<u8> {
    build_static_user_elf(&[
        0x48, 0xC7, 0xC0, 0x29, 0x00, 0x00, 0x00, 0x48, 0xC7, 0xC7, 0x02, 0x00, 0x00, 0x00, 0x48,
        0xC7, 0xC6, 0x02, 0x00, 0x00, 0x00, 0x48, 0x31, 0xD2, 0x0F, 0x05, 0x48, 0x85, 0xC0, 0x0F,
        0x88, 0xFA, 0x00, 0x00, 0x00, 0x49, 0x89, 0xC4, 0x48, 0xC7, 0xC0, 0x29, 0x00, 0x00, 0x00,
        0x48, 0xC7, 0xC7, 0x02, 0x00, 0x00, 0x00, 0x48, 0xC7, 0xC6, 0x02, 0x00, 0x00, 0x00, 0x48,
        0x31, 0xD2, 0x0F, 0x05, 0x48, 0x85, 0xC0, 0x0F, 0x88, 0xD4, 0x00, 0x00, 0x00, 0x49, 0x89,
        0xC5, 0x48, 0x83, 0xEC, 0x20, 0x66, 0xC7, 0x04, 0x24, 0x02, 0x00, 0x66, 0xC7, 0x44, 0x24,
        0x02, 0x10, 0x92, 0xC7, 0x44, 0x24, 0x04, 0x7F, 0x00, 0x00, 0x01, 0x48, 0xC7, 0x44, 0x24,
        0x08, 0x00, 0x00, 0x00, 0x00, 0x48, 0xC7, 0xC0, 0x31, 0x00, 0x00, 0x00, 0x4C, 0x89, 0xEF,
        0x48, 0x89, 0xE6, 0x48, 0xC7, 0xC2, 0x10, 0x00, 0x00, 0x00, 0x0F, 0x05, 0x48, 0x85, 0xC0,
        0x0F, 0x88, 0x90, 0x00, 0x00, 0x00, 0x48, 0xC7, 0xC0, 0x2C, 0x00, 0x00, 0x00, 0x4C, 0x89,
        0xE7, 0x48, 0x8D, 0x35, 0x8F, 0x00, 0x00, 0x00, 0x48, 0xC7, 0xC2, 0x04, 0x00, 0x00, 0x00,
        0x4D, 0x31, 0xD2, 0x49, 0x89, 0xE0, 0x49, 0xC7, 0xC1, 0x10, 0x00, 0x00, 0x00, 0x0F, 0x05,
        0x48, 0x83, 0xF8, 0x04, 0x0F, 0x85, 0x5F, 0x00, 0x00, 0x00, 0x48, 0xC7, 0xC0, 0x2D, 0x00,
        0x00, 0x00, 0x4C, 0x89, 0xEF, 0x48, 0x8D, 0x74, 0x24, 0x10, 0x48, 0xC7, 0xC2, 0x08, 0x00,
        0x00, 0x00, 0x4D, 0x31, 0xD2, 0x4D, 0x31, 0xC0, 0x4D, 0x31, 0xC9, 0x0F, 0x05, 0x48, 0x83,
        0xF8, 0x04, 0x0F, 0x85, 0x34, 0x00, 0x00, 0x00, 0x81, 0x7C, 0x24, 0x10, 0x70, 0x69, 0x6E,
        0x67, 0x0F, 0x85, 0x26, 0x00, 0x00, 0x00, 0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, 0x48,
        0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, 0x48, 0x8D, 0x35, 0x25, 0x00, 0x00, 0x00, 0x48, 0xC7,
        0xC2, 0x1A, 0x00, 0x00, 0x00, 0x0F, 0x05, 0x48, 0x31, 0xFF, 0xE9, 0x07, 0x00, 0x00, 0x00,
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, 0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, 0x0F,
        0x05, 0x70, 0x69, 0x6E, 0x67, 0x47, 0x41, 0x54, 0x45, 0x5F, 0x44, 0x31, 0x20, 0x6C, 0x6F,
        0x6F, 0x70, 0x62, 0x61, 0x63, 0x6B, 0x20, 0x63, 0x6F, 0x6D, 0x70, 0x6C, 0x65, 0x74, 0x65,
        0x0A,
    ])
}

/// Minimal `/bin/sh`: print `$ `, read stdin (EOF → exit), echo, loop.
/// Loaded at the same high vaddr as the other Gate B ELFs so it does not
/// share L4[0] with any kernel identity map.
pub fn sh_userspace_elf_data() -> Vec<u8> {
    build_static_user_elf(&[
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // mov rdi, 1
        0x48, 0x8D, 0x35, 0x55, 0x00, 0x00, 0x00, // lea rsi, [rip+0x55] ; "$ "
        0x48, 0xC7, 0xC2, 0x02, 0x00, 0x00, 0x00, // mov rdx, 2
        0x0F, 0x05, // syscall
        0x48, 0xC7, 0xC0, 0x00, 0x00, 0x00, 0x00, // mov rax, 0
        0x48, 0x31, 0xFF, // xor rdi, rdi
        0x48, 0x8D, 0xB4, 0x24, 0x00, 0xFF, 0xFF, 0xFF, // lea rsi, [rsp-0x100]
        0x48, 0xC7, 0xC2, 0xFF, 0x00, 0x00, 0x00, // mov rdx, 255
        0x0F, 0x05, // syscall
        0x48, 0x85, 0xC0, // test rax, rax
        0x7E, 0x20, // jle exit
        0x49, 0x89, 0xC0, // mov r8, rax
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // mov rdi, 1
        0x48, 0x8D, 0xB4, 0x24, 0x00, 0xFF, 0xFF, 0xFF, // lea rsi, [rsp-0x100]
        0x4C, 0x89, 0xC2, // mov rdx, r8
        0x0F, 0x05, // syscall
        0xEB, 0xA2, // jmp prompt
        0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, // mov rax, 60
        0x48, 0x31, 0xFF, // xor rdi, rdi
        0x0F, 0x05, // syscall
        b'$', b' ',
    ])
}

/// Gate B7: `rt_sigaction(SIGINT, handler)` then `pause`. Handler invokes
/// `rt_sigreturn` from RX ELF text (the stack is W^X); then the program writes
/// the marker.
pub fn sigreturn_userspace_elf_data() -> Vec<u8> {
    // lea handler offset: insn at 0x1A, next at 0x21, handler at 0x87 → 0x66
    // lea marker offset: insn at 0x62, next at 0x69, marker at 0x90 → 0x27
    // js fail: insn at 0x45, next at 0x4B, fail at 0x77 → 0x2C
    build_static_user_elf(&[
        0x48, 0x83, 0xEC, 0x20, // sub rsp, 32
        0x48, 0x31, 0xC0, // xor rax, rax
        0x48, 0x89, 0x04, 0x24, // mov [rsp], rax
        0x48, 0x89, 0x44, 0x24, 0x08, // mov [rsp+8], rax
        0x48, 0x89, 0x44, 0x24, 0x10, // mov [rsp+16], rax
        0x48, 0x89, 0x44, 0x24, 0x18, // mov [rsp+24], rax
        0x48, 0x8D, 0x05, 0x66, 0x00, 0x00, 0x00, // lea rax, [rip+handler]
        0x48, 0x89, 0x04, 0x24, // mov [rsp], rax
        0x48, 0xC7, 0xC0, 0x0D, 0x00, 0x00, 0x00, // mov rax, 13 (rt_sigaction)
        0x48, 0xC7, 0xC7, 0x02, 0x00, 0x00, 0x00, // mov rdi, 2 (SIGINT)
        0x48, 0x89, 0xE6, // mov rsi, rsp
        0x48, 0x31, 0xD2, // xor rdx, rdx
        0x49, 0xC7, 0xC2, 0x08, 0x00, 0x00, 0x00, // mov r10, 8
        0x0F, 0x05, // syscall
        0x48, 0x85, 0xC0, // test rax, rax
        0x0F, 0x88, 0x2C, 0x00, 0x00, 0x00, // js fail
        0x48, 0xC7, 0xC0, 0x22, 0x00, 0x00, 0x00, // mov rax, 34 (pause)
        0x0F, 0x05, // syscall
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // mov rdi, 1
        0x48, 0x8D, 0x35, 0x27, 0x00, 0x00, 0x00, // lea rsi, [rip+marker]
        0x48, 0xC7, 0xC2, 0x1B, 0x00, 0x00, 0x00, // mov rdx, 27
        0x0F, 0x05, // syscall
        0x48, 0x31, 0xFF, // xor rdi, rdi
        0xEB, 0x07, // jmp exit
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // fail: mov rdi, 1
        0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, // exit: mov rax, 60
        0x0F, 0x05, // syscall
        0x48, 0xC7, 0xC0, 0x0F, 0x00, 0x00, 0x00, // handler: mov rax, 15
        0x0F, 0x05, // syscall (rt_sigreturn)
        b'G', b'A', b'T', b'E', b'_', b'B', b'7', b' ', b's', b'i', b'g', b'r', b'e', b't', b'u',
        b'r', b'n', b' ', b'c', b'o', b'm', b'p', b'l', b'e', b't', b'e', b'\n',
    ])
}

/// Gate F1: mmap a 64×64 BGRA buffer, fill a magic pixel, ioctl(/dev/wl0) present.
pub fn display_client_elf_data() -> Vec<u8> {
    shm_present_client_elf(0xFFC0_FFEE, b"GATE_F1 client isolated\n")
}

/// Gate F3: same SHM present path, dark terminal pixels, isolated from
/// `WindowContentType::Terminal`.
pub fn terminal_client_elf_data() -> Vec<u8> {
    shm_present_client_elf(0xFF1A_1A1A, b"GATE_F3 terminal isolated\n")
}

/// Gate F4: userspace launcher client (Paint) via SHM, not an Empty stub.
pub fn launcher_client_elf_data() -> Vec<u8> {
    shm_present_client_elf(0xFF33_66CC, b"GATE_F4 launcher userspace\n")
}

/// mmap + fill + ioctl(/dev/wl0) + write marker. Layout matches the Gate F1
/// RIP offsets; only the fill dword and marker string/length change.
fn shm_present_client_elf(fill: u32, marker: &[u8]) -> Vec<u8> {
    let f = fill.to_le_bytes();
    let n = (marker.len() as u32).to_le_bytes();
    let mut code = alloc::vec![
        0x48, 0xC7, 0xC0, 0x09, 0x00, 0x00, 0x00, // mov rax, 9 (mmap)
        0x48, 0x31, 0xFF, // xor rdi, rdi
        0x48, 0xC7, 0xC6, 0x00, 0x40, 0x00, 0x00, // mov rsi, 0x4000
        0x48, 0xC7, 0xC2, 0x03, 0x00, 0x00, 0x00, // mov rdx, 3
        0x49, 0xC7, 0xC2, 0x22, 0x80, 0x00, 0x00, // mov r10, 0x8022
        0x49, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF, // mov r8, -1
        0x4D, 0x31, 0xC9, // xor r9, r9
        0x0F, 0x05, // syscall
        0x48, 0x85, 0xC0, // test rax, rax
        0x0F, 0x88, 0x7C, 0x00, 0x00, 0x00, // js fail
        0x49, 0x89, 0xC4, // mov r12, rax
        0x4C, 0x89, 0xE7, // mov rdi, r12
        0x48, 0xC7, 0xC1, 0x00, 0x10, 0x00, 0x00, // mov rcx, 0x1000
        0xB8, f[0], f[1], f[2], f[3], // mov eax, fill
        0xF3, 0xAB, // rep stosd
        0x48, 0xC7, 0xC0, 0x02, 0x00, 0x00, 0x00, // mov rax, 2 (open)
        0x48, 0x8D, 0x3D, 0x6A, 0x00, 0x00, 0x00, // lea rdi, [rip+path]
        0x48, 0xC7, 0xC6, 0x02, 0x00, 0x00, 0x00, // mov rsi, 2
        0x48, 0x31, 0xD2, // xor rdx, rdx
        0x0F, 0x05, // syscall
        0x48, 0x85, 0xC0, // test rax, rax
        0x0F, 0x88, 0x45, 0x00, 0x00, 0x00, // js fail
        0x49, 0x89, 0xC5, // mov r13, rax
        0x48, 0xC7, 0xC0, 0x10, 0x00, 0x00, 0x00, // mov rax, 16 (ioctl)
        0x4C, 0x89, 0xEF, // mov rdi, r13
        0x48, 0xC7, 0xC6, 0x01, 0x00, 0x4C, 0x57, // mov rsi, 0x574C0001
        0x4C, 0x89, 0xE2, // mov rdx, r12
        0x0F, 0x05, // syscall
        0x48, 0x85, 0xC0, // test rax, rax
        0x0F, 0x88, 0x23, 0x00, 0x00, 0x00, // js fail
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // mov rdi, 1
        0x48, 0x8D, 0x35, 0x27, 0x00, 0x00, 0x00, // lea rsi, [rip+marker]
        0x48, 0xC7, 0xC2, n[0], n[1], n[2], n[3], // mov rdx, marker.len
        0x0F, 0x05, // syscall
        0x48, 0x31, 0xFF, // xor rdi, rdi
        0xEB, 0x07, // jmp exit
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // fail: mov rdi, 1
        0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, // exit: mov rax, 60
        0x0F, 0x05, // syscall
        b'/', b'd', b'e', b'v', b'/', b'w', b'l', b'0', 0, // path
    ];
    code.extend_from_slice(marker);
    build_static_user_elf(&code)
}

fn launch_hello_userspace() -> Result<(), &'static str> {
    let elf = hello_userspace_elf_data();
    if !crate::elf::is_elf(&elf) {
        return Err("hello ELF is invalid");
    }
    serial_println!(
        "[init] Mapping static hello ELF ({} bytes) into current page tables",
        elf.len()
    );
    let (entry, rsp) = crate::vmm::map_static_elf_into_current(&elf)?;
    serial_println!("[init] hello mapped: entry={:#x} rsp={:#x}", entry, rsp);
    unsafe {
        crate::usermode::run_userspace_once(entry, rsp);
    }
    Ok(())
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

/// Start the first user-space program.
///
/// Gate B2: map a static hello ELF into the current page tables, `iretq` to
/// Ring 3, `sys_write` to serial, `sys_exit` back to the kernel.
/// Gate B3–B6: scheduled tasks with their own CR3 — `execve`+`waitpid`,
/// `fork`+child, SIGKILL / SIGSEGV / PTY SIGINT, then `/bin/sh` on a PTY.
/// Gate B7: custom SIGINT handler + live `rt_sigreturn`.
/// Gate B8: timer preempts a spinning Ring 3 program that never syscalls.
/// Gate D1: loopback `send`/`recv` from a Ring 3 UDP pair.
/// Gate F1/F2: Ring 3 SHM client presents a buffer the compositor scans out.
pub fn start_init() -> Option<Pid> {
    serial_println!("[init] Starting first userspace (Gate B2 hello)...");

    match launch_hello_userspace() {
        Ok(()) => {
            serial_println!("[init] Ring 3 hello returned to kernel");
        }
        Err(e) => {
            serial_println!("[init] Ring 3 hello failed: {}", e);
        }
    }

    crate::user_task::run_gate_demos();

    None
}

/// Initialize init process module
pub fn init() {
    serial_println!("[KnoxOS] Init process launcher ready");
}
