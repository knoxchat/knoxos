// Chromium-shaped glibc ELF fixtures. Included into `dynlink.rs` so `Asm`
// and the ELF packers are in scope. This is not a full Chromium binary —
// it is a valid ELF64 with PT_INTERP `/lib64/ld-linux-x86-64.so.2` and
// DT_NEEDED `libc.so.6` whose `_start` calls `__libc_start_main@plt`.

const GATE_CHR_EXEC_MARKER: &str = "GATE_CHR exec";
const GATE_CHR_GLIBC_MARKER: &str = "GATE_CHR glibc";

/// crt1 `_start` writes `GATE_CHR exec`, then calls `__libc_start_main@plt`.
/// `main` writes `GATE_CHR glibc`.
fn emit_chrome_dyn_text() -> Vec<u8> {
    let mut a = Asm::new();

    rex_mov_imm(&mut a, 0, 1);
    rex_mov_imm(&mut a, 7, 1);
    a.emit(&[0x48, 0x8D, 0x35]);
    a.rel32("exec_marker");
    rex_mov_imm(&mut a, 2, GATE_CHR_EXEC_MARKER.len() as u64 + 1);
    syscall(&mut a);

    a.emit(&[0x31, 0xED]); // xor ebp, ebp
    a.emit(&[0x48, 0x8D, 0x3D]);
    a.rel32("main");
    a.emit(&[0x48, 0x8B, 0x34, 0x24]); // mov rsi, [rsp] argc
    a.emit(&[0x48, 0x8D, 0x54, 0x24, 0x08]); // lea rdx, [rsp+8] argv
    a.emit(&[0x31, 0xC9]);
    a.emit(&[0x4D, 0x31, 0xC0]);
    a.emit(&[0x4D, 0x31, 0xC9]);
    a.emit(&[0x48, 0x83, 0xE4, 0xF0]);
    let call_off = a.buf.len();
    a.emit(&[0xE8, 0, 0, 0, 0]);
    a.emit(&[0xF4]);

    a.label("main");
    rex_mov_imm(&mut a, 0, 1);
    rex_mov_imm(&mut a, 7, 1);
    a.emit(&[0x48, 0x8D, 0x35]);
    a.rel32("glibc_marker");
    rex_mov_imm(&mut a, 2, GATE_CHR_GLIBC_MARKER.len() as u64 + 1);
    syscall(&mut a);
    a.emit(&[0x31, 0xC0]);
    a.emit(&[0xC3]);
    a.label("exec_marker");
    a.emit(GATE_CHR_EXEC_MARKER.as_bytes());
    a.emit(b"\n");
    a.label("glibc_marker");
    a.emit(GATE_CHR_GLIBC_MARKER.as_bytes());
    a.emit(b"\n");

    let mut text = a.finish();
    let next = 0x1090i32 + call_off as i32 + 5;
    let disp = 0x1080i32 - next;
    text[call_off + 1..call_off + 5].copy_from_slice(&disp.to_le_bytes());
    text
}

/// ET_EXEC Chromium stand-in: PT_INTERP ld-linux, DT_NEEDED libc.so.6.
pub fn chrome_elf_data() -> Vec<u8> {
    let interp = b"/lib64/ld-linux-x86-64.so.2\0";
    let text = emit_chrome_dyn_text();
    assert!(
        text.len() < 0xF70,
        "chrome dyn text overflow {}",
        text.len()
    );

    let mut elf = Vec::new();
    let entry = MAIN_BASE + 0x1090;
    push_ehdr(&mut elf, 2, entry, 0x40, 6);

    push_phdr(
        &mut elf,
        PT_PHDR,
        PF_R,
        0x40,
        MAIN_BASE + 0x40,
        6 * 56,
        6 * 56,
        8,
    );
    push_phdr(
        &mut elf,
        PT_INTERP,
        PF_R,
        0x190,
        MAIN_BASE + 0x190,
        interp.len() as u64,
        interp.len() as u64,
        1,
    );
    push_phdr(
        &mut elf,
        PT_LOAD,
        PF_R | PF_X,
        0,
        MAIN_BASE,
        0x2000,
        0x2000,
        0x1000,
    );
    push_phdr(
        &mut elf,
        PT_LOAD,
        PF_R | PF_W,
        0x2000,
        MAIN_BASE + 0x2000,
        0x2000,
        0x2000,
        0x1000,
    );
    push_phdr(
        &mut elf,
        PT_DYNAMIC,
        PF_R | PF_W,
        0x2000,
        MAIN_BASE + 0x2000,
        10 * 16,
        10 * 16,
        8,
    );
    push_phdr(
        &mut elf,
        PT_GNU_RELRO,
        PF_R,
        0x2000,
        MAIN_BASE + 0x2000,
        0x1000,
        0x1000,
        1,
    );

    elf.resize(0x190, 0);
    elf.extend_from_slice(interp);

    elf.resize(0x1000, 0);
    push_sym(&mut elf, 0, 0, 0, 0, 0);
    push_sym(&mut elf, 11, 0x12, 0, 0, 0); // undef __libc_start_main

    elf.resize(0x1030, 0);
    elf.push(0);
    elf.extend_from_slice(b"libc.so.6\0");
    elf.extend_from_slice(b"__libc_start_main\0");

    elf.resize(0x1050, 0);
    elf.extend_from_slice(&(MAIN_BASE + 0x3018).to_le_bytes());
    elf.extend_from_slice(&((1u64 << 32) | R_X86_64_JUMP_SLOT).to_le_bytes());
    elf.extend_from_slice(&0i64.to_le_bytes());

    elf.resize(0x1070, 0);
    let disp_got1 = (MAIN_BASE + 0x3008) as i32 - (MAIN_BASE as i32 + 0x1076);
    elf.extend_from_slice(&[0xFF, 0x35]);
    elf.extend_from_slice(&disp_got1.to_le_bytes());
    let disp_got2 = (MAIN_BASE + 0x3010) as i32 - (MAIN_BASE as i32 + 0x107C);
    elf.extend_from_slice(&[0xFF, 0x25]);
    elf.extend_from_slice(&disp_got2.to_le_bytes());

    elf.resize(0x1080, 0);
    let disp_got3 = (MAIN_BASE + 0x3018) as i32 - (MAIN_BASE as i32 + 0x1086);
    elf.extend_from_slice(&[0xFF, 0x25]);
    elf.extend_from_slice(&disp_got3.to_le_bytes());
    elf.extend_from_slice(&[0x68, 0, 0, 0, 0]);
    let rel_plt0 = 0x1070i32 - 0x1090;
    elf.push(0xE9);
    elf.extend_from_slice(&rel_plt0.to_le_bytes());

    elf.resize(0x1090, 0);
    elf.extend_from_slice(&text);

    elf.resize(0x2000, 0);
    push_dyn(&mut elf, DT_NEEDED, 1);
    push_dyn(&mut elf, DT_STRTAB, MAIN_BASE + 0x1030);
    push_dyn(&mut elf, DT_SYMTAB, MAIN_BASE + 0x1000);
    push_dyn(&mut elf, DT_STRSZ, 29);
    push_dyn(&mut elf, DT_SYMENT, 24);
    push_dyn(&mut elf, DT_JMPREL, MAIN_BASE + 0x1050);
    push_dyn(&mut elf, DT_PLTRELSZ, 24);
    push_dyn(&mut elf, DT_PLTREL, DT_RELA as u64);
    push_dyn(&mut elf, DT_PLTGOT, MAIN_BASE + 0x3000);
    push_dyn(&mut elf, DT_NULL, 0);

    elf.resize(0x3000, 0);
    elf.extend_from_slice(&(MAIN_BASE + 0x2000).to_le_bytes());
    elf.extend_from_slice(&0u64.to_le_bytes());
    elf.extend_from_slice(&0u64.to_le_bytes());
    elf.extend_from_slice(&(MAIN_BASE + 0x1086).to_le_bytes());

    elf.resize(0x4000, 0);
    elf
}

/// `/lib/libc.so.6` — same `__libc_start_main` as musl libc, glibc soname.
pub fn libc_glibc_elf_data() -> Vec<u8> {
    let func = emit_libc_start_main();
    assert!(
        func.len() < 0x80,
        "libc.so.6 __libc_start_main overflow {}",
        func.len()
    );

    let mut elf = Vec::new();
    push_ehdr(&mut elf, 3, 0, 0x40, 2);
    push_phdr(&mut elf, PT_LOAD, PF_R | PF_X, 0, 0, 0x2000, 0x2000, 0x1000);
    push_phdr(
        &mut elf,
        PT_DYNAMIC,
        PF_R,
        0x1200,
        0x1200,
        6 * 16,
        6 * 16,
        8,
    );

    elf.resize(0x1000, 0);
    elf.extend_from_slice(&func);

    elf.resize(0x1080, 0);
    let dynsym = 0x1080u64;
    push_sym(&mut elf, 0, 0, 0, 0, 0);
    push_sym(&mut elf, 11, 0x12, 1, 0x1000, func.len() as u64);

    let dynstr = 0x10B0u64;
    elf.resize(dynstr as usize, 0);
    elf.push(0);
    elf.extend_from_slice(b"libc.so.6\0");
    elf.extend_from_slice(b"__libc_start_main\0");

    elf.resize(0x1200, 0);
    push_dyn(&mut elf, DT_SONAME, 1);
    push_dyn(&mut elf, DT_SYMTAB, dynsym);
    push_dyn(&mut elf, DT_STRTAB, dynstr);
    push_dyn(&mut elf, DT_STRSZ, 29);
    push_dyn(&mut elf, DT_SYMENT, 24);
    push_dyn(&mut elf, DT_NULL, 0);

    elf.resize(0x2000, 0);
    elf
}
