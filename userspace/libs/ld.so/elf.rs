// Ring 3 `/lib/ld-knoxos.so` instruction bytes.
// Included into `kernel/src/init/dynlink.rs` so the kernel can embed the ELF.

fn emit_ldso_text() -> Vec<u8> {
    let mut a = Asm::new();

    rex_mov_imm(&mut a, 15, INTERP_RW);

    a.emit(&[0x48, 0x89, 0xE3]); // mov rbx, rsp
    a.emit(&[0x48, 0x8B, 0x0B]); // mov rcx, [rbx]
    a.emit(&[0x48, 0x8D, 0x73, 0x08]); // lea rsi, [rbx+8]
    a.emit(&[0x48, 0x8D, 0x74, 0xCE, 0x08]); // lea rsi, [rsi+rcx*8+8]
    a.label("skip_env");
    a.emit(&[0x48, 0x8B, 0x06]);
    a.emit(&[0x48, 0x83, 0xC6, 0x08]);
    a.emit(&[0x48, 0x85, 0xC0]);
    a.jnz("skip_env");

    a.label("parse_auxv");
    a.emit(&[0x48, 0x8B, 0x06]);
    a.emit(&[0x48, 0x8B, 0x56, 0x08]);
    a.emit(&[0x48, 0x83, 0xC6, 0x10]);
    a.emit(&[0x48, 0x85, 0xC0]);
    a.jz("auxv_done");
    a.emit(&[0x48, 0x83, 0xF8, 0x03]);
    a.jne("au_phent");
    mov_to_r15(&mut a, 2, S_PHDR);
    a.jmp("parse_auxv");
    a.label("au_phent");
    a.emit(&[0x48, 0x83, 0xF8, 0x04]);
    a.jne("au_phnum");
    mov_to_r15(&mut a, 2, S_PHENT);
    a.jmp("parse_auxv");
    a.label("au_phnum");
    a.emit(&[0x48, 0x83, 0xF8, 0x05]);
    a.jne("au_entry");
    mov_to_r15(&mut a, 2, S_PHNUM);
    a.jmp("parse_auxv");
    a.label("au_entry");
    a.emit(&[0x48, 0x83, 0xF8, 0x09]);
    a.jne("parse_auxv");
    mov_to_r15(&mut a, 2, S_ENTRY);
    a.jmp("parse_auxv");

    a.label("auxv_done");
    mov_from_r15(&mut a, 0, S_PHENT);
    a.emit(&[0x48, 0x85, 0xC0]);
    a.jnz("ph_setup");
    rex_mov_imm(&mut a, 0, 56);
    mov_to_r15(&mut a, 0, S_PHENT);

    a.label("ph_setup");
    a.emit(&[0x4D, 0x31, 0xE4]); // xor r12, r12
    a.label("ph_loop");
    mov_from_r15(&mut a, 0, S_PHNUM);
    a.emit(&[0x4C, 0x39, 0xE0]); // cmp rax, r12
    a.je("ph_done");
    mov_from_r15(&mut a, 7, S_PHDR);
    mov_from_r15(&mut a, 0, S_PHENT);
    a.emit(&[0x49, 0x0F, 0xAF, 0xC4]); // imul rax, r12
    a.emit(&[0x48, 0x01, 0xC7]);
    a.emit(&[0x8B, 0x07]); // p_type
    a.emit(&[0x48, 0x83, 0xF8, 0x02]);
    a.jne("ph_relro");
    a.emit(&[0x48, 0x8B, 0x47, 0x10]);
    mov_to_r15(&mut a, 0, S_DYN);
    a.jmp("ph_next");
    a.label("ph_relro");
    a.emit(&[0x8B, 0x07]);
    rex_mov_imm(&mut a, 1, PT_GNU_RELRO as u64);
    a.emit(&[0x48, 0x39, 0xC8]);
    a.jne("ph_next");
    a.emit(&[0x48, 0x8B, 0x47, 0x10]);
    mov_to_r15(&mut a, 0, S_RELRO);
    a.emit(&[0x48, 0x8B, 0x47, 0x28]);
    mov_to_r15(&mut a, 0, S_RELROSZ);
    a.label("ph_next");
    a.emit(&[0x49, 0xFF, 0xC4]);
    a.jmp("ph_loop");

    a.label("ph_done");
    mov_from_r15(&mut a, 7, S_DYN);
    a.emit(&[0x48, 0x85, 0xFF]);
    a.jz("fail");

    a.label("dyn_loop");
    a.emit(&[0x48, 0x8B, 0x07]);
    a.emit(&[0x48, 0x8B, 0x57, 0x08]);
    a.emit(&[0x48, 0x85, 0xC0]);
    a.jz("dyn_done");
    a.emit(&[0x48, 0x83, 0xF8, 0x01]);
    a.jne("d_str");
    mov_to_r15(&mut a, 2, S_NEEDED);
    a.jmp("dyn_next");
    a.label("d_str");
    a.emit(&[0x48, 0x83, 0xF8, 0x05]);
    a.jne("d_sym");
    mov_to_r15(&mut a, 2, S_STRTAB);
    a.jmp("dyn_next");
    a.label("d_sym");
    a.emit(&[0x48, 0x83, 0xF8, 0x06]);
    a.jne("d_jmp");
    mov_to_r15(&mut a, 2, S_SYMTAB);
    a.jmp("dyn_next");
    a.label("d_jmp");
    a.emit(&[0x48, 0x83, 0xF8, 0x17]);
    a.jne("d_got");
    mov_to_r15(&mut a, 2, S_JMPREL);
    a.jmp("dyn_next");
    a.label("d_got");
    a.emit(&[0x48, 0x83, 0xF8, 0x03]);
    a.jne("dyn_next");
    mov_to_r15(&mut a, 2, S_PLTGOT);
    a.label("dyn_next");
    a.emit(&[0x48, 0x83, 0xC7, 0x10]);
    a.jmp("dyn_loop");

    a.label("dyn_done");
    mov_from_r15(&mut a, 0, S_STRTAB);
    a.emit(&[0x49, 0x03, 0x47, S_NEEDED as u8]);
    mov_to_r15(&mut a, 0, S_NEEDED);

    lea_from_r15(&mut a, 7, S_PATH);
    mov_from_r15(&mut a, 6, S_NEEDED);
    a.emit(&[0x80, 0x3E, b'/']);
    a.je("copy_need");
    a.emit(&[0xC6, 0x07, b'/']);
    a.emit(&[0xC6, 0x47, 0x01, b'l']);
    a.emit(&[0xC6, 0x47, 0x02, b'i']);
    a.emit(&[0xC6, 0x47, 0x03, b'b']);
    a.emit(&[0xC6, 0x47, 0x04, b'/']);
    a.emit(&[0x48, 0x83, 0xC7, 0x05]);
    a.label("copy_need");
    a.emit(&[0x8A, 0x06]);
    a.emit(&[0x88, 0x07]);
    a.emit(&[0x48, 0xFF, 0xC6]);
    a.emit(&[0x48, 0xFF, 0xC7]);
    a.emit(&[0x84, 0xC0]);
    a.jnz("copy_need");

    rex_mov_imm(&mut a, 0, 2);
    lea_from_r15(&mut a, 7, S_PATH);
    a.emit(&[0x48, 0x31, 0xF6]);
    a.emit(&[0x48, 0x31, 0xD2]);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    a.js("fail");
    mov_to_r15(&mut a, 0, S_FD);

    rex_mov_imm(&mut a, 0, 9);
    a.emit(&[0x48, 0x31, 0xFF]);
    rex_mov_imm(&mut a, 6, 0x3000);
    rex_mov_imm(&mut a, 2, 5);
    rex_mov_imm(&mut a, 10, 0x02);
    mov_from_r15(&mut a, 8, S_FD);
    a.emit(&[0x4D, 0x31, 0xC9]);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    a.jns("mapped");

    rex_mov_imm(&mut a, 0, 9);
    a.emit(&[0x48, 0x31, 0xFF]);
    rex_mov_imm(&mut a, 6, 0x3000);
    rex_mov_imm(&mut a, 2, 3);
    rex_mov_imm(&mut a, 10, 0x8022);
    rex_mov_imm(&mut a, 8, u64::MAX);
    a.emit(&[0x4D, 0x31, 0xC9]);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    a.js("fail");
    a.emit(&[0x49, 0x89, 0xC5]); // mov r13, rax
    rex_mov_imm(&mut a, 0, 0);
    mov_from_r15(&mut a, 7, S_FD);
    a.emit(&[0x4C, 0x89, 0xEE]);
    rex_mov_imm(&mut a, 2, 0x3000);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    a.js("fail");
    rex_mov_imm(&mut a, 0, 10);
    a.emit(&[0x4C, 0x89, 0xEF]);
    rex_mov_imm(&mut a, 6, 0x2000);
    rex_mov_imm(&mut a, 2, 5);
    syscall(&mut a);
    a.jmp("have_lib");

    a.label("mapped");
    a.emit(&[0x49, 0x89, 0xC5]);

    a.label("have_lib");
    mov_to_r15(&mut a, 13, S_LIBBASE);
    rex_mov_imm(&mut a, 0, 3);
    mov_from_r15(&mut a, 7, S_FD);
    syscall(&mut a);

    a.emit(&[0x49, 0x8B, 0x7D, 0x20]); // e_phoff
    a.emit(&[0x4C, 0x01, 0xEF]);
    a.emit(&[0x41, 0x0F, 0xB7, 0x45, 0x36]);
    a.emit(&[0x49, 0x89, 0xC4]); // r12 = phent
    a.emit(&[0x41, 0x0F, 0xB7, 0x45, 0x38]);
    a.emit(&[0x49, 0x89, 0xC1]); // r9 = phnum
    a.emit(&[0x4D, 0x31, 0xC0]);
    a.label("so_ph");
    a.emit(&[0x4D, 0x39, 0xC1]);
    a.je("fail");
    a.emit(&[0x8B, 0x07]);
    a.emit(&[0x48, 0x83, 0xF8, 0x02]);
    a.je("so_dyn");
    a.emit(&[0x4C, 0x01, 0xE7]);
    a.emit(&[0x49, 0xFF, 0xC0]);
    a.jmp("so_ph");
    a.label("so_dyn");
    a.emit(&[0x48, 0x8B, 0x47, 0x10]);
    a.emit(&[0x4C, 0x01, 0xE8]);
    a.emit(&[0x48, 0x89, 0xC7]);

    a.label("so_dyn_loop");
    a.emit(&[0x48, 0x8B, 0x07]);
    a.emit(&[0x48, 0x8B, 0x57, 0x08]);
    a.emit(&[0x48, 0x85, 0xC0]);
    a.jz("so_dyn_done");
    a.emit(&[0x48, 0x83, 0xF8, 0x06]);
    a.jne("so_str");
    a.emit(&[0x4C, 0x01, 0xEA]);
    mov_to_r15(&mut a, 2, S_SOSYM);
    a.jmp("so_dn");
    a.label("so_str");
    a.emit(&[0x48, 0x83, 0xF8, 0x05]);
    a.jne("so_dn");
    a.emit(&[0x4C, 0x01, 0xEA]);
    mov_to_r15(&mut a, 2, S_SOSTR);
    a.label("so_dn");
    a.emit(&[0x48, 0x83, 0xC7, 0x10]);
    a.jmp("so_dyn_loop");

    a.label("so_dyn_done");
    mov_from_r15(&mut a, 0, S_PLTGOT);
    a.emit(&[0x4C, 0x89, 0x78, 0x08]); // GOT[1] = r15
    a.emit(&[0x48, 0xB9]);
    let resolver_imm_off = a.buf.len();
    a.emit(&[0, 0, 0, 0, 0, 0, 0, 0]);
    a.emit(&[0x48, 0x89, 0x48, 0x10]); // GOT[2] = resolver

    mov_from_r15(&mut a, 7, S_RELRO);
    a.emit(&[0x48, 0x85, 0xFF]);
    a.jz("go_entry");
    rex_mov_imm(&mut a, 0, 10);
    mov_from_r15(&mut a, 6, S_RELROSZ);
    rex_mov_imm(&mut a, 2, 1);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    a.js("fail");

    a.label("go_entry");
    mov_from_r15(&mut a, 0, S_ENTRY);
    a.emit(&[0xFF, 0xE0]);

    a.label("resolver");
    // Preserve SysV argument registers across lazy bind so musl crt1's
    // `__libc_start_main(main, argc, argv, …)` arrives intact (BM4).
    a.emit(&[0x57]); // push rdi
    a.emit(&[0x56]); // push rsi
    a.emit(&[0x52]); // push rdx
    a.emit(&[0x51]); // push rcx
    a.emit(&[0x41, 0x50]); // push r8
    a.emit(&[0x41, 0x51]); // push r9
    a.emit(&[0x48, 0x8B, 0x5C, 0x24, 0x38]); // mov rbx, [rsp+56] reloc index
    mov_from_r15(&mut a, 0, S_JMPREL);
    a.emit(&[0x48, 0x6B, 0xCB, 0x18]);
    a.emit(&[0x48, 0x01, 0xC8]);
    a.emit(&[0x48, 0x8B, 0x38]);
    mov_to_r15(&mut a, 7, S_GOTSLOT);
    a.emit(&[0x48, 0x8B, 0x50, 0x08]);
    a.emit(&[0x48, 0xC1, 0xEA, 0x20]);
    mov_from_r15(&mut a, 0, S_SYMTAB);
    a.emit(&[0x48, 0x6B, 0xCA, 0x18]);
    a.emit(&[0x48, 0x01, 0xC8]);
    a.emit(&[0x8B, 0x00]);
    a.emit(&[0x49, 0x03, 0x47, S_STRTAB as u8]);
    mov_to_r15(&mut a, 0, S_WANT);

    mov_from_r15(&mut a, 6, S_SOSYM);
    a.emit(&[0x48, 0x83, 0xC6, 0x18]);
    a.emit(&[0xB1, 0x40]);
    a.label("sym_loop");
    a.emit(&[0x8B, 0x06]);
    mov_from_r15(&mut a, 2, S_SOSTR);
    a.emit(&[0x48, 0x01, 0xC2]);
    mov_from_r15(&mut a, 7, S_WANT);
    a.label("scmp");
    a.emit(&[0x8A, 0x07]);
    a.emit(&[0x3A, 0x02]);
    a.jne("sym_next");
    a.emit(&[0x84, 0xC0]);
    a.jz("sym_hit");
    a.emit(&[0x48, 0xFF, 0xC7]);
    a.emit(&[0x48, 0xFF, 0xC2]);
    a.jmp("scmp");
    a.label("sym_next");
    a.emit(&[0x48, 0x83, 0xC6, 0x18]);
    a.emit(&[0xFE, 0xC9]);
    a.jnz("sym_loop");
    a.jmp("fail");

    a.label("sym_hit");
    a.emit(&[0x48, 0x8B, 0x46, 0x08]);
    a.emit(&[0x49, 0x03, 0x47, S_LIBBASE as u8]);
    mov_from_r15(&mut a, 7, S_GOTSLOT);
    a.emit(&[0x48, 0x89, 0x07]);
    a.emit(&[0x41, 0x59]); // pop r9
    a.emit(&[0x41, 0x58]); // pop r8
    a.emit(&[0x59]); // pop rcx
    a.emit(&[0x5A]); // pop rdx
    a.emit(&[0x5E]); // pop rsi
    a.emit(&[0x5F]); // pop rdi
    a.emit(&[0x48, 0x83, 0xC4, 0x10]); // add rsp, 16  (GOT[1] + reloc index)
    a.emit(&[0xFF, 0xE0]);

    a.label("fail");
    rex_mov_imm(&mut a, 0, 1);
    rex_mov_imm(&mut a, 7, 1);
    a.emit(&[0x48, 0x8D, 0x35]);
    a.rel32("failmsg");
    rex_mov_imm(&mut a, 2, 10);
    syscall(&mut a);
    rex_mov_imm(&mut a, 0, 60);
    rex_mov_imm(&mut a, 7, 1);
    syscall(&mut a);
    a.label("failmsg");
    a.emit(b"ld.so fail\n");

    let resolver_at = *a.labels.get("resolver").expect("resolver");
    let mut text = a.finish();
    let resolver_va = INTERP_BASE + 0x1000 + resolver_at as u64;
    text[resolver_imm_off..resolver_imm_off + 8].copy_from_slice(&resolver_va.to_le_bytes());
    text
}

/// Userspace `/lib/ld-knoxos.so` — Ring 3 ELF interpreter.
pub fn ldso_elf_data() -> Vec<u8> {
    let text = emit_ldso_text();
    assert!(text.len() < 0x1000, "ld.so text overflow {}", text.len());

    let mut elf = Vec::new();
    let entry = INTERP_BASE + 0x1000;
    push_ehdr(&mut elf, 2, entry, 0x40, 2);
    push_phdr(
        &mut elf,
        PT_LOAD,
        PF_R | PF_X,
        0,
        INTERP_BASE,
        0x2000,
        0x2000,
        0x1000,
    );
    push_phdr(
        &mut elf,
        PT_LOAD,
        PF_R | PF_W,
        0x2000,
        INTERP_RW,
        0x1000,
        0x1000,
        0x1000,
    );
    elf.resize(0x1000, 0);
    elf.extend_from_slice(&text);
    elf.resize(0x3000, 0);
    elf
}
