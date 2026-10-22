// Ring 3 musl CRT / `/lib/libc.so` instruction bytes.
// Included into `kernel/src/init/dynlink.rs` so the kernel can embed the ELF.

/// musl `crt1` ABI: `_start` calls `__libc_start_main(main, argc, argv, …)`.
/// This is not the Gate B2 hello fixture (that `write`s then `exit`s from `_start`).
fn emit_musl_text() -> Vec<u8> {
    let mut a = Asm::new();

    a.emit(&[0x31, 0xED]); // xor ebp, ebp
    a.emit(&[0x48, 0x8D, 0x3D]);
    a.rel32("main");
    a.emit(&[0x48, 0x8B, 0x34, 0x24]); // mov rsi, [rsp] argc
    a.emit(&[0x48, 0x8D, 0x54, 0x24, 0x08]); // lea rdx, [rsp+8] argv
    a.emit(&[0x31, 0xC9]); // xor ecx, ecx  init
    a.emit(&[0x4D, 0x31, 0xC0]); // xor r8, r8  fini
    a.emit(&[0x4D, 0x31, 0xC9]); // xor r9, r9  ldso_fini
    a.emit(&[0x48, 0x83, 0xE4, 0xF0]); // and rsp, -16
    a.emit(&[0xE8]);
    a.rel32("libc_start_main");
    a.emit(&[0xF4]); // hlt if libc_start_main returns

    a.label("libc_start_main");
    a.emit(&[0x57]); // push rdi (main)
    a.emit(&[0x48, 0x89, 0xF0]); // mov rax, rsi (argc)
    a.emit(&[0x48, 0xFF, 0xC0]); // inc rax
    a.emit(&[0x48, 0x8D, 0x0C, 0xC2]); // lea rcx, [rdx+rax*8] envp
    a.emit(&[0x48, 0x89, 0xF7]); // mov rdi, rsi (argc)
    a.emit(&[0x48, 0x89, 0xD6]); // mov rsi, rdx (argv)
    a.emit(&[0x48, 0x89, 0xCA]); // mov rdx, rcx (envp)
    a.emit(&[0x58]); // pop rax (main)
    a.emit(&[0xFF, 0xD0]); // call main
    a.emit(&[0x48, 0x89, 0xC7]); // mov rdi, rax
    rex_mov_imm(&mut a, 0, 60);
    syscall(&mut a);

    a.label("main");
    rex_mov_imm(&mut a, 0, 1);
    rex_mov_imm(&mut a, 7, 1);
    a.emit(&[0x48, 0x8D, 0x35]);
    a.rel32("marker");
    rex_mov_imm(&mut a, 2, GATE_BM3_MARKER.len() as u64 + 1);
    syscall(&mut a);
    a.emit(&[0x31, 0xC0]); // xor eax, eax
    a.emit(&[0xC3]);
    a.label("marker");
    a.emit(GATE_BM3_MARKER.as_bytes());
    a.emit(b"\n");

    a.finish()
}

/// Static musl-CRT ELF: `_start` → `__libc_start_main` → `main`.
pub fn musl_static_user_elf_data() -> Vec<u8> {
    let text = emit_musl_text();
    assert!(text.len() < 0x1000, "musl text overflow {}", text.len());
    super::elf::build_static_user_elf(&text)
}

/// musl CRT `main` calls `KnoxUiRaster` then presents GPUM via `/dev/wl0`.
fn emit_musl_ui_text() -> Vec<u8> {
    const MMAP_LEN: u64 = 0x8A000;
    const WIDTH: u32 = 320;
    const HEIGHT: u32 = 440;
    let mut a = Asm::new();

    a.emit(&[0x31, 0xED]); // xor ebp, ebp
    a.emit(&[0x48, 0x8D, 0x3D]);
    a.rel32("main");
    a.emit(&[0x48, 0x8B, 0x34, 0x24]); // mov rsi, [rsp] argc
    a.emit(&[0x48, 0x8D, 0x54, 0x24, 0x08]); // lea rdx, [rsp+8] argv
    a.emit(&[0x31, 0xC9]); // xor ecx, ecx
    a.emit(&[0x4D, 0x31, 0xC0]); // xor r8, r8
    a.emit(&[0x4D, 0x31, 0xC9]); // xor r9, r9
    a.emit(&[0x48, 0x83, 0xE4, 0xF0]); // and rsp, -16
    a.emit(&[0xE8]);
    a.rel32("libc_start_main");
    a.emit(&[0xF4]);

    a.label("libc_start_main");
    a.emit(&[0x57]);
    a.emit(&[0x48, 0x89, 0xF0]);
    a.emit(&[0x48, 0xFF, 0xC0]);
    a.emit(&[0x48, 0x8D, 0x0C, 0xC2]);
    a.emit(&[0x48, 0x89, 0xF7]);
    a.emit(&[0x48, 0x89, 0xD6]);
    a.emit(&[0x48, 0x89, 0xCA]);
    a.emit(&[0x58]);
    a.emit(&[0xFF, 0xD0]);
    a.emit(&[0x48, 0x89, 0xC7]);
    rex_mov_imm(&mut a, 0, 60);
    syscall(&mut a);

    a.label("main");
    // mmap(NULL, 0x8A000, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0)
    rex_mov_imm(&mut a, 0, 9);
    a.emit(&[0x48, 0x31, 0xFF]);
    rex_mov_imm(&mut a, 6, MMAP_LEN);
    rex_mov_imm(&mut a, 2, 3);
    a.emit(&[0x49, 0xC7, 0xC2, 0x22, 0x80, 0x00, 0x00]);
    a.emit(&[0x49, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]);
    a.emit(&[0x4D, 0x31, 0xC9]);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    a.js("fail");
    a.emit(&[0x49, 0x89, 0xC7]); // mov r15, rax

    // KnoxUiRaster(kind=1, dest, len, 320, 440)
    rex_mov_imm(&mut a, 7, 1);
    a.emit(&[0x4C, 0x89, 0xFE]); // mov rsi, r15
    rex_mov_imm(&mut a, 2, MMAP_LEN);
    a.emit(&[0x49, 0xC7, 0xC2]);
    a.emit(&WIDTH.to_le_bytes());
    rex_mov_imm(&mut a, 8, HEIGHT as u64);
    rex_mov_imm(&mut a, 0, 0x3052);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    a.js("fail");

    // Stamp GPUM + width/height after the client raster.
    a.emit(&[0x41, 0xC7, 0x07, 0x47, 0x50, 0x55, 0x4D]); // mov dword [r15], 'GPUM'
    a.emit(&[0x41, 0xC7, 0x47, 0x04]);
    a.emit(&WIDTH.to_le_bytes());
    a.emit(&[0x41, 0xC7, 0x47, 0x08]);
    a.emit(&HEIGHT.to_le_bytes());

    // open("/dev/wl0", O_RDWR)
    a.emit(&[0x48, 0x8D, 0x3D]);
    a.rel32("path");
    rex_mov_imm(&mut a, 6, 2);
    a.emit(&[0x48, 0x31, 0xD2]);
    rex_mov_imm(&mut a, 0, 2);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    a.js("fail");
    a.emit(&[0x49, 0x89, 0xC6]); // mov r14, rax

    // ioctl(fd, 0x574C0001, buf)
    a.emit(&[0x4C, 0x89, 0xF7]);
    rex_mov_imm(&mut a, 6, 0x574C_0001);
    a.emit(&[0x4C, 0x89, 0xFA]);
    rex_mov_imm(&mut a, 0, 16);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    a.js("fail");

    rex_mov_imm(&mut a, 0, 1);
    rex_mov_imm(&mut a, 7, 1);
    a.emit(&[0x48, 0x8D, 0x35]);
    a.rel32("marker");
    rex_mov_imm(&mut a, 2, GATE_GPUI_MUSL_RASTER.len() as u64 + 1);
    syscall(&mut a);
    a.emit(&[0x31, 0xC0]);
    a.emit(&[0xC3]);

    a.label("fail");
    rex_mov_imm(&mut a, 0, 60);
    rex_mov_imm(&mut a, 7, 1);
    syscall(&mut a);
    a.label("path");
    a.emit(b"/dev/wl0\0");
    a.label("marker");
    a.emit(GATE_GPUI_MUSL_RASTER.as_bytes());
    a.emit(b"\n");

    a.finish()
}

/// Static musl CRT that rasters Knox UI itself.
pub fn musl_ui_user_elf_data() -> Vec<u8> {
    let text = emit_musl_ui_text();
    assert!(text.len() < 0x1000, "musl ui text overflow {}", text.len());
    super::elf::build_static_user_elf(&text)
}

// ─── Dynamically linked musl (Gate BM4) ─────────────────────────────────

/// musl-style `/lib/libc.so` exporting `__libc_start_main`.
///
/// Signature matches musl crt1: `__libc_start_main(main, argc, argv, init, fini, ldso_fini)`.
/// Calls `main(argc, argv, envp)` then `exit(status)`.
pub fn libc_musl_elf_data() -> Vec<u8> {
    let func = emit_libc_start_main();
    assert!(
        func.len() < 0x80,
        "libc __libc_start_main overflow {}",
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
    push_sym(&mut elf, 9, 0x12, 1, 0x1000, func.len() as u64);

    let dynstr = 0x10B0u64;
    elf.resize(dynstr as usize, 0);
    elf.push(0);
    elf.extend_from_slice(b"libc.so\0");
    elf.extend_from_slice(b"__libc_start_main\0");

    elf.resize(0x1200, 0);
    push_dyn(&mut elf, DT_SONAME, 1);
    push_dyn(&mut elf, DT_SYMTAB, dynsym);
    push_dyn(&mut elf, DT_STRTAB, dynstr);
    push_dyn(&mut elf, DT_STRSZ, 27);
    push_dyn(&mut elf, DT_SYMENT, 24);
    push_dyn(&mut elf, DT_NULL, 0);

    elf.resize(0x2000, 0);
    elf
}

fn emit_libc_start_main() -> Vec<u8> {
    let mut a = Asm::new();
    a.emit(&[0x57]); // push rdi (main)
    a.emit(&[0x48, 0x89, 0xF0]); // mov rax, rsi (argc)
    a.emit(&[0x48, 0xFF, 0xC0]); // inc rax
    a.emit(&[0x48, 0x8D, 0x0C, 0xC2]); // lea rcx, [rdx+rax*8] envp
    a.emit(&[0x48, 0x89, 0xF7]); // mov rdi, rsi (argc)
    a.emit(&[0x48, 0x89, 0xD6]); // mov rsi, rdx (argv)
    a.emit(&[0x48, 0x89, 0xCA]); // mov rdx, rcx (envp)
    a.emit(&[0x58]); // pop rax (main)
    a.emit(&[0xFF, 0xD0]); // call main
    a.emit(&[0x48, 0x89, 0xC7]); // mov rdi, rax
    rex_mov_imm(&mut a, 0, 60);
    syscall(&mut a);
    a.finish()
}

/// musl crt1 `_start` + `main`, placed at file offset 0x1090.
/// Calls `__libc_start_main@plt` (PLT1 at 0x1080).
fn emit_musl_dyn_text() -> Vec<u8> {
    let mut a = Asm::new();

    a.emit(&[0x31, 0xED]); // xor ebp, ebp
    a.emit(&[0x48, 0x8D, 0x3D]);
    a.rel32("main");
    a.emit(&[0x48, 0x8B, 0x34, 0x24]); // mov rsi, [rsp] argc
    a.emit(&[0x48, 0x8D, 0x54, 0x24, 0x08]); // lea rdx, [rsp+8] argv
    a.emit(&[0x31, 0xC9]); // xor ecx, ecx  init
    a.emit(&[0x4D, 0x31, 0xC0]); // xor r8, r8  fini
    a.emit(&[0x4D, 0x31, 0xC9]); // xor r9, r9  ldso_fini
    a.emit(&[0x48, 0x83, 0xE4, 0xF0]); // and rsp, -16
    let call_off = a.buf.len();
    a.emit(&[0xE8, 0, 0, 0, 0]); // call __libc_start_main@plt
    a.emit(&[0xF4]); // hlt if it returns

    a.label("main");
    rex_mov_imm(&mut a, 0, 1);
    rex_mov_imm(&mut a, 7, 1);
    a.emit(&[0x48, 0x8D, 0x35]);
    a.rel32("marker");
    rex_mov_imm(&mut a, 2, GATE_BM4_MARKER.len() as u64 + 1);
    syscall(&mut a);
    a.emit(&[0x31, 0xC0]); // xor eax, eax
    a.emit(&[0xC3]);
    a.label("marker");
    a.emit(GATE_BM4_MARKER.as_bytes());
    a.emit(b"\n");

    let mut text = a.finish();
    let next = 0x1090i32 + call_off as i32 + 5;
    let disp = 0x1080i32 - next;
    text[call_off + 1..call_off + 5].copy_from_slice(&disp.to_le_bytes());
    text
}

/// ET_EXEC with PT_INTERP, DT_NEEDED `libc.so`, musl crt1, lazy PLT.
pub fn musl_dyn_user_elf_data() -> Vec<u8> {
    let interp = b"/lib/ld-knoxos.so\0";
    let text = emit_musl_dyn_text();
    assert!(text.len() < 0xF70, "musl dyn text overflow {}", text.len());

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
    push_sym(&mut elf, 9, 0x12, 0, 0, 0); // undef __libc_start_main

    elf.resize(0x1030, 0);
    elf.push(0);
    elf.extend_from_slice(b"libc.so\0");
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
    push_dyn(&mut elf, DT_STRSZ, 27);
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
