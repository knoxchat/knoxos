use alloc::vec::Vec;

/// Embed a committed ELF from `userspace/apps/`.
/// Instruction bytes are authored there; the kernel only includes the blob.
macro_rules! userspace_app_elf {
    ($rel:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../userspace/apps/",
            $rel
        ))
        .to_vec()
    };
}

/// Built-in Gate B2 hello: `write` then `exit`. Source: `userspace/apps/hello/`.
///
/// Loaded at 512 GiB (L4 index 1) so it does not collide with the kernel's
/// own PT_LOAD at vaddr 0 (which occupies 0x401000 in the current CR3).
pub fn hello_userspace_elf_data() -> Vec<u8> {
    userspace_app_elf!("hello/hello.elf")
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

/// Magic stored in PT_TLS and read via `%fs:0` (initial-exec / general-dynamic).
pub const GATE_CHR_TLS_MAGIC: u64 = 0x4348_5254_4C53_0001;

/// Static ELF with PT_TLS. `%fs:0` is the template; no `arch_prctl` (glibc IE/GD).
pub fn build_static_user_elf_with_tls(code: &[u8], tls_init: &[u8]) -> Vec<u8> {
    let entry_point: u64 = 0x0000_0080_0000_1000;
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
    elf.extend_from_slice(&0x40u64.to_le_bytes());
    elf.extend_from_slice(&0u64.to_le_bytes());
    elf.extend_from_slice(&0u32.to_le_bytes());
    elf.extend_from_slice(&64u16.to_le_bytes());
    elf.extend_from_slice(&56u16.to_le_bytes());
    elf.extend_from_slice(&2u16.to_le_bytes());
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

    let tls_offset: u64 = 0x2000;
    let tls_filesz = tls_init.len() as u64;
    elf.extend_from_slice(&7u32.to_le_bytes());
    elf.extend_from_slice(&4u32.to_le_bytes());
    elf.extend_from_slice(&tls_offset.to_le_bytes());
    elf.extend_from_slice(&0u64.to_le_bytes());
    elf.extend_from_slice(&0u64.to_le_bytes());
    elf.extend_from_slice(&tls_filesz.to_le_bytes());
    elf.extend_from_slice(&tls_filesz.to_le_bytes());
    elf.extend_from_slice(&8u64.to_le_bytes());

    elf.resize(code_offset as usize, 0);
    elf.extend_from_slice(code);
    elf.resize(tls_offset as usize, 0);
    elf.extend_from_slice(tls_init);
    elf.resize((tls_offset + 0x1000) as usize, 0);
    elf
}
