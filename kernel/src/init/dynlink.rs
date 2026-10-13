//! Userspace dynamic linker fixtures (Gate BM1).
//!
//! Builds Ring 3 ELFs that the kernel maps on `execve`/`spawn`:
//!   * `/lib/ld-knoxos.so` — a real userspace linker (auxv, DT_NEEDED, lazy PLT, RELRO)
//!   * `/lib/libgate_bm.so` — shared object exporting `gate_hello`
//!   * a `DT_NEEDED` executable that calls `gate_hello@plt` (Gate BM1)
//!   * a static `dlopen` program that `mmap`s the `.so` through the VMM (Gate BM2)
//!   * a static musl-CRT binary (`_start` → `__libc_start_main` → `main`) (Gate BM3)
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

pub const LDSO_PATH: &str = "/lib/ld-knoxos.so";
pub const LIBGATE_PATH: &str = "/lib/libgate_bm.so";
pub const DYNBIN_PATH: &str = "/bin/gate_dyn";
pub const DLOPEN_BIN_PATH: &str = "/bin/gate_dlopen";
pub const MUSL_BIN_PATH: &str = "/bin/gate_musl";
pub const GATE_BM1_MARKER: &str = "GATE_BM1 dynlink complete";
pub const GATE_BM2_MARKER: &str = "GATE_BM2 dlopen complete";
pub const GATE_BM3_MARKER: &str = "GATE_BM3 musl abi";

const MAIN_BASE: u64 = 0x0000_0080_0000_0000;
const INTERP_BASE: u64 = 0x0000_0090_0000_0000;
const INTERP_RW: u64 = INTERP_BASE + 0x2000;

const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const PT_INTERP: u32 = 3;
const PT_PHDR: u32 = 6;
const PT_GNU_RELRO: u32 = 0x6474_E552;

const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;

const DT_NULL: i64 = 0;
const DT_NEEDED: i64 = 1;
const DT_PLTRELSZ: i64 = 2;
const DT_PLTGOT: i64 = 3;
const DT_STRTAB: i64 = 5;
const DT_SYMTAB: i64 = 6;
const DT_SYMENT: i64 = 11;
const DT_STRSZ: i64 = 10;
const DT_SONAME: i64 = 14;
const DT_PLTREL: i64 = 20;
const DT_JMPREL: i64 = 23;
const DT_RELA: i64 = 7;

const R_X86_64_JUMP_SLOT: u64 = 7;

const S_PHDR: i32 = 0x00;
const S_PHNUM: i32 = 0x08;
const S_PHENT: i32 = 0x10;
const S_ENTRY: i32 = 0x18;
const S_DYN: i32 = 0x20;
const S_RELRO: i32 = 0x28;
const S_RELROSZ: i32 = 0x30;
const S_STRTAB: i32 = 0x38;
const S_SYMTAB: i32 = 0x40;
const S_JMPREL: i32 = 0x48;
const S_PLTGOT: i32 = 0x50;
const S_NEEDED: i32 = 0x58;
const S_LIBBASE: i32 = 0x60;
const S_SOSYM: i32 = 0x68;
const S_SOSTR: i32 = 0x70;
const S_FD: i32 = 0x78;
const S_WANT: i32 = 0x80;
const S_GOTSLOT: i32 = 0x88;
const S_PATH: i32 = 0x90;

// ─── Tiny assembler ─────────────────────────────────────────────────────

struct Asm {
    buf: Vec<u8>,
    labels: BTreeMap<&'static str, usize>,
    rel32: Vec<(usize, &'static str)>,
}

impl Asm {
    fn new() -> Self {
        Self {
            buf: Vec::new(),
            labels: BTreeMap::new(),
            rel32: Vec::new(),
        }
    }

    fn emit(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    fn label(&mut self, name: &'static str) {
        self.labels.insert(name, self.buf.len());
    }

    fn rel32(&mut self, name: &'static str) {
        self.rel32.push((self.buf.len(), name));
        self.emit(&[0, 0, 0, 0]);
    }

    fn jmp(&mut self, name: &'static str) {
        self.emit(&[0xE9]);
        self.rel32(name);
    }

    fn je(&mut self, name: &'static str) {
        self.emit(&[0x0F, 0x84]);
        self.rel32(name);
    }

    fn jne(&mut self, name: &'static str) {
        self.emit(&[0x0F, 0x85]);
        self.rel32(name);
    }

    fn jz(&mut self, name: &'static str) {
        self.je(name);
    }

    fn jnz(&mut self, name: &'static str) {
        self.jne(name);
    }

    fn js(&mut self, name: &'static str) {
        self.emit(&[0x0F, 0x88]);
        self.rel32(name);
    }

    fn jns(&mut self, name: &'static str) {
        self.emit(&[0x0F, 0x89]);
        self.rel32(name);
    }

    fn finish(self) -> Vec<u8> {
        let mut out = self.buf;
        for (off, name) in self.rel32 {
            let target = *self
                .labels
                .get(name)
                .unwrap_or_else(|| panic!("missing label {name}"));
            let disp = (target as i32).wrapping_sub((off as i32) + 4);
            out[off..off + 4].copy_from_slice(&disp.to_le_bytes());
        }
        out
    }
}

fn rex_mov_imm(asm: &mut Asm, reg: u8, imm: u64) {
    if reg >= 8 {
        asm.emit(&[0x49, 0xB8 + (reg - 8)]);
    } else {
        asm.emit(&[0x48, 0xB8 + reg]);
    }
    asm.emit(&imm.to_le_bytes());
}

fn syscall(asm: &mut Asm) {
    asm.emit(&[0x0F, 0x05]);
}

/// `reg` is 0–15. Loads `reg` from `[r15+off]`.
fn mov_from_r15(asm: &mut Asm, dest: u8, off: i32) {
    let rex = 0x49 | if dest >= 8 { 0x04 } else { 0 };
    let reg = dest & 7;
    if (-128..128).contains(&off) {
        asm.emit(&[rex, 0x8B, 0x40 | (reg << 3) | 7, off as u8]);
    } else {
        asm.emit(&[rex, 0x8B, 0x80 | (reg << 3) | 7]);
        asm.emit(&off.to_le_bytes());
    }
}

fn mov_to_r15(asm: &mut Asm, src: u8, off: i32) {
    let rex = 0x49 | if src >= 8 { 0x04 } else { 0 };
    let reg = src & 7;
    if (-128..128).contains(&off) {
        asm.emit(&[rex, 0x89, 0x40 | (reg << 3) | 7, off as u8]);
    } else {
        asm.emit(&[rex, 0x89, 0x80 | (reg << 3) | 7]);
        asm.emit(&off.to_le_bytes());
    }
}

fn lea_from_r15(asm: &mut Asm, dest: u8, off: i32) {
    let rex = 0x49 | if dest >= 8 { 0x04 } else { 0 };
    let reg = dest & 7;
    if (-128..128).contains(&off) {
        asm.emit(&[rex, 0x8D, 0x40 | (reg << 3) | 7, off as u8]);
    } else {
        asm.emit(&[rex, 0x8D, 0x80 | (reg << 3) | 7]);
        asm.emit(&off.to_le_bytes());
    }
}

fn push_ehdr(elf: &mut Vec<u8>, e_type: u16, entry: u64, phoff: u64, phnum: u16) {
    elf.extend_from_slice(&[0x7F, b'E', b'L', b'F', 2, 1, 1, 0]);
    elf.extend_from_slice(&[0; 8]);
    elf.extend_from_slice(&e_type.to_le_bytes());
    elf.extend_from_slice(&62u16.to_le_bytes());
    elf.extend_from_slice(&1u32.to_le_bytes());
    elf.extend_from_slice(&entry.to_le_bytes());
    elf.extend_from_slice(&phoff.to_le_bytes());
    elf.extend_from_slice(&0u64.to_le_bytes());
    elf.extend_from_slice(&0u32.to_le_bytes());
    elf.extend_from_slice(&64u16.to_le_bytes());
    elf.extend_from_slice(&56u16.to_le_bytes());
    elf.extend_from_slice(&phnum.to_le_bytes());
    elf.extend_from_slice(&0u16.to_le_bytes());
    elf.extend_from_slice(&0u16.to_le_bytes());
    elf.extend_from_slice(&0u16.to_le_bytes());
}

fn push_phdr(
    elf: &mut Vec<u8>,
    p_type: u32,
    flags: u32,
    offset: u64,
    vaddr: u64,
    filesz: u64,
    memsz: u64,
    align: u64,
) {
    elf.extend_from_slice(&p_type.to_le_bytes());
    elf.extend_from_slice(&flags.to_le_bytes());
    elf.extend_from_slice(&offset.to_le_bytes());
    elf.extend_from_slice(&vaddr.to_le_bytes());
    elf.extend_from_slice(&vaddr.to_le_bytes());
    elf.extend_from_slice(&filesz.to_le_bytes());
    elf.extend_from_slice(&memsz.to_le_bytes());
    elf.extend_from_slice(&align.to_le_bytes());
}

fn push_dyn(elf: &mut Vec<u8>, tag: i64, val: u64) {
    elf.extend_from_slice(&tag.to_le_bytes());
    elf.extend_from_slice(&val.to_le_bytes());
}

fn push_sym(elf: &mut Vec<u8>, name: u32, info: u8, shndx: u16, value: u64, size: u64) {
    elf.extend_from_slice(&name.to_le_bytes());
    elf.push(info);
    elf.push(0);
    elf.extend_from_slice(&shndx.to_le_bytes());
    elf.extend_from_slice(&value.to_le_bytes());
    elf.extend_from_slice(&size.to_le_bytes());
}

// ─── Shared library ─────────────────────────────────────────────────────

/// ET_DYN `/lib/libgate_bm.so` exporting `gate_hello` → `mov eax, 0x42; ret`.
pub fn libgate_elf_data() -> Vec<u8> {
    let mut elf = Vec::new();
    push_ehdr(&mut elf, 3, 0, 0x40, 2);

    // PT_LOAD covers the whole file at vaddr 0 so load_bias + st_value is the
    // mapped function address.
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
    // gate_hello
    elf.extend_from_slice(&[0xB8, 0x42, 0x00, 0x00, 0x00, 0xC3]); // mov eax, 0x42; ret

    elf.resize(0x1080, 0);
    // .dynsym
    let dynsym = 0x1080u64;
    push_sym(&mut elf, 0, 0, 0, 0, 0);
    push_sym(&mut elf, 15, 0x12, 1, 0x1000, 6); // gate_hello GLOBAL FUNC

    // .dynstr at 0x10B0
    let dynstr = 0x10B0u64;
    elf.resize(dynstr as usize, 0);
    elf.push(0);
    elf.extend_from_slice(b"libgate_bm.so\0");
    elf.extend_from_slice(b"gate_hello\0");

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

// ─── Dynamically linked executable ──────────────────────────────────────

/// ET_EXEC with PT_INTERP, DT_NEEDED `libgate_bm.so`, lazy PLT, GNU_RELRO.
pub fn dyn_user_elf_data() -> Vec<u8> {
    let interp = b"/lib/ld-knoxos.so\0";
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
    // .dynsym at 0x1000
    push_sym(&mut elf, 0, 0, 0, 0, 0);
    push_sym(&mut elf, 15, 0x12, 0, 0, 0); // undef gate_hello

    // .dynstr at 0x1030
    elf.resize(0x1030, 0);
    elf.push(0);
    elf.extend_from_slice(b"libgate_bm.so\0");
    elf.extend_from_slice(b"gate_hello\0");

    // .rela.plt at 0x1050
    elf.resize(0x1050, 0);
    elf.extend_from_slice(&(MAIN_BASE + 0x3018).to_le_bytes());
    elf.extend_from_slice(&((1u64 << 32) | R_X86_64_JUMP_SLOT).to_le_bytes());
    elf.extend_from_slice(&0i64.to_le_bytes());

    // PLT0 at 0x1070
    elf.resize(0x1070, 0);
    // push QWORD PTR [rip+disp]  GOT[1]
    let disp_got1 = (MAIN_BASE + 0x3008) as i32 - (MAIN_BASE as i32 + 0x1076);
    elf.extend_from_slice(&[0xFF, 0x35]);
    elf.extend_from_slice(&disp_got1.to_le_bytes());
    // jmp QWORD PTR [rip+disp]   GOT[2]
    let disp_got2 = (MAIN_BASE + 0x3010) as i32 - (MAIN_BASE as i32 + 0x107C);
    elf.extend_from_slice(&[0xFF, 0x25]);
    elf.extend_from_slice(&disp_got2.to_le_bytes());

    // PLT1 at 0x1080
    elf.resize(0x1080, 0);
    let disp_got3 = (MAIN_BASE + 0x3018) as i32 - (MAIN_BASE as i32 + 0x1086);
    elf.extend_from_slice(&[0xFF, 0x25]);
    elf.extend_from_slice(&disp_got3.to_le_bytes());
    elf.extend_from_slice(&[0x68, 0, 0, 0, 0]); // push 0
    let rel_plt0 = 0x1070i32 - 0x1090;
    elf.push(0xE9);
    elf.extend_from_slice(&rel_plt0.to_le_bytes());

    // _start at 0x1090
    elf.resize(0x1090, 0);
    let rel_plt1 = 0x1080i32 - 0x1095;
    elf.push(0xE8);
    elf.extend_from_slice(&rel_plt1.to_le_bytes());
    // cmp rax, 0x42
    elf.extend_from_slice(&[0x48, 0x83, 0xF8, 0x42]);
    // jne fail (after marker write + exit)
    // write(1, marker, len); exit(0); fail: exit(1); marker
    let marker = GATE_BM1_MARKER.as_bytes();
    // jne is 6 bytes, then write is 27 bytes, exit is 12, fail is 12, marker follows
    // We'll emit with known sizes:
    // 0x1090: call (5)
    // 0x1095: cmp (4)
    // 0x1099: jne fail (6) → 0x109F
    // 0x109F: mov rax,1 (7)
    // 0x10A6: mov rdi,1 (7)
    // 0x10AD: lea rsi, [rip+marker] (7)
    // 0x10B4: mov rdx, len (7)
    // 0x10BB: syscall (2)
    // 0x10BD: mov rax,60 (7)
    // 0x10C4: xor rdi,rdi (3)
    // 0x10C7: syscall (2)
    // 0x10C9: fail
    // 0x10C9: mov rax,60 (7)
    // 0x10D0: mov rdi,1 (7)
    // 0x10D7: syscall (2)
    // 0x10D9: marker
    elf.extend_from_slice(&[0x0F, 0x85]);
    let fail_rel = 0x10C9i32 - 0x109F;
    elf.extend_from_slice(&fail_rel.to_le_bytes());
    elf.extend_from_slice(&[0x48, 0xC7, 0xC0, 1, 0, 0, 0]); // mov rax, 1
    elf.extend_from_slice(&[0x48, 0xC7, 0xC7, 1, 0, 0, 0]); // mov rdi, 1
    // lea rsi, [rip+disp]; next rip = 0x10B4
    elf.extend_from_slice(&[0x48, 0x8D, 0x35]);
    let marker_rel = 0x10D9i32 - 0x10B4;
    elf.extend_from_slice(&marker_rel.to_le_bytes());
    let mlen = marker.len() as u32;
    elf.extend_from_slice(&[0x48, 0xC7, 0xC2]);
    elf.extend_from_slice(&mlen.to_le_bytes());
    elf.extend_from_slice(&[0x0F, 0x05]);
    elf.extend_from_slice(&[0x48, 0xC7, 0xC0, 0x3C, 0, 0, 0]);
    elf.extend_from_slice(&[0x48, 0x31, 0xFF]);
    elf.extend_from_slice(&[0x0F, 0x05]);
    // fail
    elf.extend_from_slice(&[0x48, 0xC7, 0xC0, 0x3C, 0, 0, 0]);
    elf.extend_from_slice(&[0x48, 0xC7, 0xC7, 1, 0, 0, 0]);
    elf.extend_from_slice(&[0x0F, 0x05]);
    elf.extend_from_slice(marker);
    elf.push(b'\n');

    // .dynamic at 0x2000
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

    // .got.plt at 0x3000 (writable; not covered by GNU_RELRO)
    elf.resize(0x3000, 0);
    elf.extend_from_slice(&(MAIN_BASE + 0x2000).to_le_bytes()); // GOT[0] = _DYNAMIC
    elf.extend_from_slice(&0u64.to_le_bytes()); // GOT[1] link_map
    elf.extend_from_slice(&0u64.to_le_bytes()); // GOT[2] resolver
    elf.extend_from_slice(&(MAIN_BASE + 0x1086).to_le_bytes()); // GOT[3] lazy stub

    elf.resize(0x4000, 0);
    elf
}

// ─── Userspace ld.so ────────────────────────────────────────────────────

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
    a.emit(&[0x58]);
    a.emit(&[0x5B]);
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

pub fn install_dynlink_files() {
    let mut vfs = crate::vfs::VFS.lock();
    vfs.write_file(LDSO_PATH, &ldso_elf_data());
    vfs.write_file(LIBGATE_PATH, &libgate_elf_data());
    vfs.write_file(DYNBIN_PATH, &dyn_user_elf_data());
    vfs.write_file(DLOPEN_BIN_PATH, &dlopen_user_elf_data());
    vfs.write_file(MUSL_BIN_PATH, &musl_static_user_elf_data());
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

// ─── Runtime dlopen (Gate BM2) ──────────────────────────────────────────

/// Ring 3 `dlopen`: `open` + VMM `mmap` of `/lib/libgate_bm.so`, dynsym
/// lookup of `gate_hello`, call (expects `0x42`), then the BM2 marker.
///
/// This is not `DT_NEEDED` at exec (BM1) and not `ldknoxos.rs`'s fake base.
fn emit_dlopen_text() -> Vec<u8> {
    let mut a = Asm::new();

    a.emit(&[0x48, 0x8D, 0x3D]);
    a.rel32("path");
    a.emit(&[0x48, 0x31, 0xF6]);
    a.emit(&[0x48, 0x31, 0xD2]);
    rex_mov_imm(&mut a, 0, 2);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    a.js("fail");
    a.emit(&[0x49, 0x89, 0xC4]); // r12 = fd

    rex_mov_imm(&mut a, 0, 9);
    a.emit(&[0x48, 0x31, 0xFF]);
    rex_mov_imm(&mut a, 6, 0x3000);
    rex_mov_imm(&mut a, 2, 5);
    rex_mov_imm(&mut a, 10, 0x02);
    a.emit(&[0x4D, 0x89, 0xE0]); // r8 = fd
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
    a.emit(&[0x49, 0x89, 0xC5]); // r13 = base
    rex_mov_imm(&mut a, 0, 0);
    a.emit(&[0x4C, 0x89, 0xE7]);
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
    rex_mov_imm(&mut a, 0, 3);
    a.emit(&[0x4C, 0x89, 0xE7]);
    syscall(&mut a);

    a.emit(&[0x41, 0x8B, 0x45, 0x00]);
    rex_mov_imm(&mut a, 1, 0x464C457F);
    a.emit(&[0x48, 0x39, 0xC8]);
    a.jne("fail");

    a.emit(&[0x49, 0x8B, 0x5D, 0x20]); // rbx = e_phoff
    a.emit(&[0x4C, 0x01, 0xEB]);
    a.emit(&[0x41, 0x0F, 0xB7, 0x45, 0x36]);
    a.emit(&[0x49, 0x89, 0xC0]); // r8 = phent
    a.emit(&[0x41, 0x0F, 0xB7, 0x45, 0x38]);
    a.emit(&[0x49, 0x89, 0xC1]); // r9 = phnum
    a.emit(&[0x4D, 0x31, 0xF6]);
    a.label("ph_loop");
    a.emit(&[0x4D, 0x39, 0xF1]);
    a.je("fail");
    a.emit(&[0x8B, 0x03]);
    a.emit(&[0x48, 0x83, 0xF8, 0x02]);
    a.je("found_dyn");
    a.emit(&[0x4C, 0x01, 0xC3]);
    a.emit(&[0x49, 0xFF, 0xC6]);
    a.jmp("ph_loop");

    a.label("found_dyn");
    a.emit(&[0x48, 0x8B, 0x7B, 0x10]);
    a.emit(&[0x4C, 0x01, 0xEF]);
    a.emit(&[0x4D, 0x31, 0xF6]);
    a.emit(&[0x4D, 0x31, 0xFF]);

    a.label("dyn_loop");
    a.emit(&[0x48, 0x8B, 0x07]);
    a.emit(&[0x48, 0x8B, 0x57, 0x08]);
    a.emit(&[0x48, 0x85, 0xC0]);
    a.jz("dyn_done");
    a.emit(&[0x48, 0x83, 0xF8, 0x06]);
    a.jne("d_str");
    a.emit(&[0x4C, 0x01, 0xEA]);
    a.emit(&[0x49, 0x89, 0xD6]);
    a.jmp("dyn_next");
    a.label("d_str");
    a.emit(&[0x48, 0x83, 0xF8, 0x05]);
    a.jne("dyn_next");
    a.emit(&[0x4C, 0x01, 0xEA]);
    a.emit(&[0x49, 0x89, 0xD7]);
    a.label("dyn_next");
    a.emit(&[0x48, 0x83, 0xC7, 0x10]);
    a.jmp("dyn_loop");

    a.label("dyn_done");
    a.emit(&[0x4D, 0x85, 0xF6]);
    a.jz("fail");
    a.emit(&[0x4D, 0x85, 0xFF]);
    a.jz("fail");

    a.emit(&[0x49, 0x83, 0xC6, 0x18]);
    a.emit(&[0xB1, 0x40]);
    a.label("sym_loop");
    a.emit(&[0x41, 0x8B, 0x06]);
    a.emit(&[0x4C, 0x01, 0xF8]);
    a.emit(&[0x48, 0x89, 0xC6]);
    a.emit(&[0x48, 0x8D, 0x3D]);
    a.rel32("want");
    a.label("scmp");
    a.emit(&[0x8A, 0x07]);
    a.emit(&[0x3A, 0x06]);
    a.jne("sym_next");
    a.emit(&[0x84, 0xC0]);
    a.jz("sym_hit");
    a.emit(&[0x48, 0xFF, 0xC7]);
    a.emit(&[0x48, 0xFF, 0xC6]);
    a.jmp("scmp");
    a.label("sym_next");
    a.emit(&[0x49, 0x83, 0xC6, 0x18]);
    a.emit(&[0xFE, 0xC9]);
    a.jnz("sym_loop");
    a.jmp("fail");

    a.label("sym_hit");
    a.emit(&[0x49, 0x8B, 0x46, 0x08]);
    a.emit(&[0x4C, 0x01, 0xE8]);
    a.emit(&[0xFF, 0xD0]);
    a.emit(&[0x48, 0x83, 0xF8, 0x42]);
    a.jne("fail");

    rex_mov_imm(&mut a, 0, 1);
    rex_mov_imm(&mut a, 7, 1);
    a.emit(&[0x48, 0x8D, 0x35]);
    a.rel32("marker");
    rex_mov_imm(&mut a, 2, GATE_BM2_MARKER.len() as u64 + 1);
    syscall(&mut a);
    rex_mov_imm(&mut a, 0, 60);
    a.emit(&[0x48, 0x31, 0xFF]);
    syscall(&mut a);

    a.label("fail");
    rex_mov_imm(&mut a, 0, 60);
    rex_mov_imm(&mut a, 7, 1);
    syscall(&mut a);
    a.label("path");
    a.emit(b"/lib/libgate_bm.so\0");
    a.label("want");
    a.emit(b"gate_hello\0");
    a.label("marker");
    a.emit(GATE_BM2_MARKER.as_bytes());
    a.emit(b"\n");

    a.finish()
}

/// Static Ring 3 program that `dlopen`s `libgate_bm.so` through the VMM.
pub fn dlopen_user_elf_data() -> Vec<u8> {
    let text = emit_dlopen_text();
    assert!(text.len() < 0x1000, "dlopen text overflow {}", text.len());
    super::elf::build_static_user_elf(&text)
}

// ─── Static musl CRT (Gate BM3) ─────────────────────────────────────────

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
