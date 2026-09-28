use alloc::vec::Vec;

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
