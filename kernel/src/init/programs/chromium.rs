//! Chromium slice 2–10 Ring 3 fixtures: `SCM_RIGHTS`, zygote, Ozone/Wayland, HTTPS, software GL, media, desktop, sandbox, TLS, Vivaldi, polish, `io_uring`.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::init::elf::{GATE_CHR_TLS_MAGIC, build_static_user_elf, build_static_user_elf_with_tls};

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

fn mov_imm(a: &mut Asm, reg: u8, imm: u32) {
    a.emit(&[0x48, 0xC7, 0xC0 + reg]);
    a.emit(&imm.to_le_bytes());
}

fn xor_reg(a: &mut Asm, reg: u8) {
    a.emit(&[0x48, 0x31, 0xC0 + (reg << 3) + reg]);
}

fn syscall(a: &mut Asm) {
    a.emit(&[0x0F, 0x05]);
}

fn js(a: &mut Asm, name: &'static str) {
    a.emit(&[0x0F, 0x88]);
    a.rel32(name);
}

fn jne(a: &mut Asm, name: &'static str) {
    a.emit(&[0x0F, 0x85]);
    a.rel32(name);
}

fn jnz(a: &mut Asm, name: &'static str) {
    jne(a, name);
}

fn lea_rip(a: &mut Asm, reg: u8, name: &'static str) {
    // lea r64, [rip+disp32] — rex.W 8D /r with RIP-relative (modrm 05 + reg<<3)
    a.emit(&[0x48, 0x8D, 0x05 + (reg << 3)]);
    a.rel32(name);
}

fn emit_fail_exit(a: &mut Asm) {
    a.label("fail");
    mov_imm(a, 0, 60);
    mov_imm(a, 7, 1);
    syscall(a);
}

fn emit_ok_write_exit(a: &mut Asm, marker: &'static str, marker_len: u32) {
    mov_imm(a, 0, 1);
    mov_imm(a, 7, 1);
    lea_rip(a, 6, "marker");
    mov_imm(a, 2, marker_len);
    syscall(a);
    xor_reg(a, 7);
    mov_imm(a, 0, 60);
    syscall(a);
    a.label("marker");
    a.emit(marker.as_bytes());
}

/// `socketpair` + `memfd` + `sendmsg(SCM_RIGHTS)` + `recvmsg` + read the fd.
pub fn scmrights_userspace_elf_data() -> Vec<u8> {
    let mut a = Asm::new();

    // sub rsp, 0x100; mov r15, rsp
    a.emit(&[0x48, 0x81, 0xEC, 0x00, 0x01, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0xE7]);

    // socketpair(AF_UNIX, SOCK_STREAM, 0, r15)
    mov_imm(&mut a, 7, 1);
    mov_imm(&mut a, 6, 1);
    xor_reg(&mut a, 2);
    a.emit(&[0x4D, 0x8D, 0x17]); // lea r10, [r15]
    mov_imm(&mut a, 0, 53);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");

    // memfd_create("scm", 0)
    lea_rip(&mut a, 7, "name");
    xor_reg(&mut a, 6);
    mov_imm(&mut a, 0, 319);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");
    a.emit(&[0x89, 0xC3]); // mov ebx, eax

    // write(memfd, "x", 1)
    a.emit(&[0x89, 0xDF]); // mov edi, ebx
    lea_rip(&mut a, 6, "payload");
    mov_imm(&mut a, 2, 1);
    mov_imm(&mut a, 0, 1);
    syscall(&mut a);
    a.emit(&[0x48, 0x83, 0xF8, 0x01]);
    jne(&mut a, "fail");

    // lseek(memfd, 0, SEEK_SET)
    a.emit(&[0x89, 0xDF]);
    xor_reg(&mut a, 6);
    xor_reg(&mut a, 2);
    mov_imm(&mut a, 0, 8);
    syscall(&mut a);

    // iov at r15+0x10
    lea_rip(&mut a, 0, "payload");
    a.emit(&[0x49, 0x89, 0x47, 0x10]); // mov [r15+0x10], rax
    a.emit(&[0x49, 0xC7, 0x47, 0x18, 0x01, 0x00, 0x00, 0x00]); // iov_len = 1

    // cmsg at r15+0x20: len=20, SOL_SOCKET, SCM_RIGHTS, fd=ebx
    a.emit(&[0x49, 0xC7, 0x47, 0x20, 0x14, 0x00, 0x00, 0x00]);
    a.emit(&[0x41, 0xC7, 0x47, 0x28, 0x01, 0x00, 0x00, 0x00]);
    a.emit(&[0x41, 0xC7, 0x47, 0x2C, 0x01, 0x00, 0x00, 0x00]);
    a.emit(&[0x41, 0x89, 0x5F, 0x30]); // mov [r15+0x30], ebx

    // zero msghdr at r15+0x40 (56 bytes)
    xor_reg(&mut a, 0);
    a.emit(&[0x49, 0x89, 0x47, 0x40]);
    a.emit(&[0x49, 0x89, 0x47, 0x48]);
    a.emit(&[0x49, 0x89, 0x47, 0x50]);
    a.emit(&[0x49, 0x89, 0x47, 0x58]);
    a.emit(&[0x49, 0x89, 0x47, 0x60]);
    a.emit(&[0x49, 0x89, 0x47, 0x68]);
    a.emit(&[0x49, 0x89, 0x47, 0x70]);
    // msg_iov = r15+0x10
    a.emit(&[0x49, 0x8D, 0x47, 0x10]);
    a.emit(&[0x49, 0x89, 0x47, 0x50]);
    a.emit(&[0x49, 0xC7, 0x47, 0x58, 0x01, 0x00, 0x00, 0x00]); // iovlen
    a.emit(&[0x49, 0x8D, 0x47, 0x20]);
    a.emit(&[0x49, 0x89, 0x47, 0x60]); // msg_control
    a.emit(&[0x49, 0xC7, 0x47, 0x68, 0x18, 0x00, 0x00, 0x00]); // controllen 24

    // sendmsg(sv[0], msghdr, 0)
    a.emit(&[0x41, 0x8B, 0x3F]); // mov edi, [r15]
    a.emit(&[0x49, 0x8D, 0x77, 0x40]); // lea rsi, [r15+0x40]
    xor_reg(&mut a, 2);
    mov_imm(&mut a, 0, 46);
    syscall(&mut a);
    a.emit(&[0x48, 0x83, 0xF8, 0x01]);
    jne(&mut a, "fail");

    // recv iov -> r15+0xF0
    a.emit(&[0x41, 0xC6, 0x87, 0xF0, 0x00, 0x00, 0x00, 0x00]); // mov byte [r15+0xF0], 0
    a.emit(&[0x49, 0x8D, 0x87, 0xF0, 0x00, 0x00, 0x00]); // lea rax, [r15+0xF0]
    a.emit(&[0x49, 0x89, 0x47, 0x10]);
    a.emit(&[0x49, 0xC7, 0x47, 0x18, 0x01, 0x00, 0x00, 0x00]);

    // zero recv cmsg (24 bytes) and reset msghdr control
    xor_reg(&mut a, 0);
    a.emit(&[0x49, 0x89, 0x47, 0x20]);
    a.emit(&[0x49, 0x89, 0x47, 0x28]);
    a.emit(&[0x49, 0x89, 0x47, 0x30]);
    a.emit(&[0x49, 0x89, 0x47, 0x40]);
    a.emit(&[0x49, 0x89, 0x47, 0x48]);
    a.emit(&[0x49, 0x89, 0x47, 0x50]);
    a.emit(&[0x49, 0x89, 0x47, 0x58]);
    a.emit(&[0x49, 0x89, 0x47, 0x60]);
    a.emit(&[0x49, 0x89, 0x47, 0x68]);
    a.emit(&[0x49, 0x89, 0x47, 0x70]);
    a.emit(&[0x49, 0x8D, 0x47, 0x10]);
    a.emit(&[0x49, 0x89, 0x47, 0x50]);
    a.emit(&[0x49, 0xC7, 0x47, 0x58, 0x01, 0x00, 0x00, 0x00]);
    a.emit(&[0x49, 0x8D, 0x47, 0x20]);
    a.emit(&[0x49, 0x89, 0x47, 0x60]);
    a.emit(&[0x49, 0xC7, 0x47, 0x68, 0x18, 0x00, 0x00, 0x00]);

    // recvmsg(sv[1], msghdr, 0)
    a.emit(&[0x41, 0x8B, 0x7F, 0x04]); // mov edi, [r15+4]
    a.emit(&[0x49, 0x8D, 0x77, 0x40]);
    xor_reg(&mut a, 2);
    mov_imm(&mut a, 0, 47);
    syscall(&mut a);
    a.emit(&[0x48, 0x83, 0xF8, 0x01]);
    jne(&mut a, "fail");
    a.emit(&[0x41, 0x80, 0xBF, 0xF0, 0x00, 0x00, 0x00, 0x78]); // cmp byte [r15+0xF0], 'x'
    jne(&mut a, "fail");

    // received fd at cmsg data r15+0x30
    a.emit(&[0x41, 0x8B, 0x7F, 0x30]); // mov edi, [r15+0x30]
    a.emit(&[0x83, 0xFF, 0x00]);
    a.emit(&[0x0F, 0x8C]);
    a.rel32("fail");
    xor_reg(&mut a, 6);
    xor_reg(&mut a, 2);
    mov_imm(&mut a, 0, 8);
    syscall(&mut a);

    a.emit(&[0x41, 0x8B, 0x7F, 0x30]);
    a.emit(&[0x49, 0x8D, 0xB7, 0xF1, 0x00, 0x00, 0x00]); // lea rsi, [r15+0xF1]
    mov_imm(&mut a, 2, 1);
    xor_reg(&mut a, 0);
    syscall(&mut a);
    a.emit(&[0x48, 0x83, 0xF8, 0x01]);
    jne(&mut a, "fail");
    a.emit(&[0x41, 0x80, 0xBF, 0xF1, 0x00, 0x00, 0x00, 0x78]);
    jne(&mut a, "fail");

    emit_ok_write_exit(&mut a, "GATE_CHR scmrights\n", 19);
    emit_fail_exit(&mut a);
    a.label("name");
    a.emit(b"scm\0");
    a.label("payload");
    a.emit(b"x");

    let text = a.finish();
    assert!(
        text.len() < 0x1000,
        "scmrights text overflow {}",
        text.len()
    );
    build_static_user_elf(&text)
}

/// Parent: `fork`, child `execve`s the zygote with Chromium-like argv/env, parent `wait4`s.
pub fn zygote_parent_elf_data() -> Vec<u8> {
    let mut a = Asm::new();
    a.emit(&[0x48, 0x83, 0xEC, 0x50]); // sub rsp, 0x50

    mov_imm(&mut a, 0, 57); // fork
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    jnz(&mut a, "parent");

    // child: argv/env pointer table on the stack
    lea_rip(&mut a, 0, "path");
    a.emit(&[0x48, 0x89, 0x04, 0x24]); // argv[0]
    lea_rip(&mut a, 0, "arg1");
    a.emit(&[0x48, 0x89, 0x44, 0x24, 0x08]);
    lea_rip(&mut a, 0, "arg2");
    a.emit(&[0x48, 0x89, 0x44, 0x24, 0x10]);
    lea_rip(&mut a, 0, "arg3");
    a.emit(&[0x48, 0x89, 0x44, 0x24, 0x18]);
    xor_reg(&mut a, 0);
    a.emit(&[0x48, 0x89, 0x44, 0x24, 0x20]);
    lea_rip(&mut a, 0, "env0");
    a.emit(&[0x48, 0x89, 0x44, 0x24, 0x28]);
    lea_rip(&mut a, 0, "env1");
    a.emit(&[0x48, 0x89, 0x44, 0x24, 0x30]);
    lea_rip(&mut a, 0, "env2");
    a.emit(&[0x48, 0x89, 0x44, 0x24, 0x38]);
    xor_reg(&mut a, 0);
    a.emit(&[0x48, 0x89, 0x44, 0x24, 0x40]);

    lea_rip(&mut a, 7, "path");
    a.emit(&[0x48, 0x89, 0xE6]); // mov rsi, rsp
    a.emit(&[0x48, 0x8D, 0x54, 0x24, 0x28]); // lea rdx, [rsp+0x28]
    mov_imm(&mut a, 0, 59);
    syscall(&mut a);
    mov_imm(&mut a, 7, 1);
    mov_imm(&mut a, 0, 60);
    syscall(&mut a);

    a.label("parent");
    a.emit(&[0x48, 0x89, 0xC7]); // mov rdi, rax
    xor_reg(&mut a, 6);
    xor_reg(&mut a, 2);
    a.emit(&[0x4D, 0x31, 0xD2]); // xor r10, r10
    mov_imm(&mut a, 0, 61);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");
    xor_reg(&mut a, 7);
    mov_imm(&mut a, 0, 60);
    syscall(&mut a);

    emit_fail_exit(&mut a);
    a.label("path");
    a.emit(b"/opt/chromium/chrome-zygote\0");
    a.label("arg1");
    a.emit(b"--type=zygote\0");
    a.label("arg2");
    a.emit(b"--ozone-platform=wayland\0");
    a.label("arg3");
    a.emit(b"--disable-dev-shm-usage\0");
    a.label("env0");
    a.emit(b"WAYLAND_DISPLAY=wayland-0\0");
    a.label("env1");
    a.emit(b"XDG_RUNTIME_DIR=/run/user/1000\0");
    a.label("env2");
    a.emit(b"XDG_CONFIG_HOME=/root/.config\0");

    let text = a.finish();
    assert!(text.len() < 0x1000, "zygote parent overflow {}", text.len());
    build_static_user_elf(&text)
}

/// Zygote child image: write `GATE_CHR fork` and exit. Scheduler task, not an atomic.
pub fn zygote_child_elf_data() -> Vec<u8> {
    let mut a = Asm::new();
    emit_ok_write_exit(&mut a, "GATE_CHR fork\n", 14);
    let text = a.finish();
    build_static_user_elf(&text)
}

fn emit_open_read_ok(a: &mut Asm, path_label: &'static str, fail: &'static str) {
    lea_rip(a, 7, path_label);
    xor_reg(a, 6);
    xor_reg(a, 2);
    mov_imm(a, 0, 2); // open
    syscall(a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(a, fail);
    a.emit(&[0x89, 0xC7]); // mov edi, eax
    a.emit(&[0x49, 0x8D, 0xB7, 0x80, 0x00, 0x00, 0x00]); // lea rsi, [r15+0x80]
    mov_imm(a, 2, 32);
    xor_reg(a, 0); // read
    syscall(a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(a, fail);
}

/// Open Linux-shaped `/proc` `/dev` `/etc` / XDG paths Chromium probes.
pub fn linux_shape_userspace_elf_data() -> Vec<u8> {
    let mut a = Asm::new();
    a.emit(&[0x48, 0x81, 0xEC, 0x00, 0x01, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0xE7]);

    emit_open_read_ok(&mut a, "proc_status", "fail");
    emit_open_read_ok(&mut a, "cpuinfo", "fail");
    emit_open_read_ok(&mut a, "dev_null", "fail");
    emit_open_read_ok(&mut a, "etc_hosts", "fail");

    emit_open_read_ok(&mut a, "xdg_run", "fail");

    emit_ok_write_exit(&mut a, "GATE_CHR linux-shape\n", 21);
    emit_fail_exit(&mut a);
    a.label("proc_status");
    a.emit(b"/proc/self/status\0");
    a.label("cpuinfo");
    a.emit(b"/proc/cpuinfo\0");
    a.label("dev_null");
    a.emit(b"/dev/null\0");
    a.label("etc_hosts");
    a.emit(b"/etc/hosts\0");
    a.label("xdg_run");
    a.emit(b"/run/user/1000/.keep\0");

    let text = a.finish();
    assert!(
        text.len() < 0x1000,
        "linux-shape text overflow {}",
        text.len()
    );
    build_static_user_elf(&text)
}

/// memfd + connect `/run/wayland-0` + Linux `wl_shm.create_pool` via `SCM_RIGHTS`.
pub fn ozone_userspace_elf_data() -> Vec<u8> {
    let mut a = Asm::new();

    // sub rsp, 0x400; mov r15, rsp
    a.emit(&[0x48, 0x81, 0xEC, 0x00, 0x04, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0xE7]);

    // memfd_create("chr", 0)
    lea_rip(&mut a, 7, "name");
    xor_reg(&mut a, 6);
    mov_imm(&mut a, 0, 319);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");
    a.emit(&[0x89, 0xC3]); // ebx = memfd

    // write(memfd, magic, 4)
    a.emit(&[0x89, 0xDF]);
    lea_rip(&mut a, 6, "magic");
    mov_imm(&mut a, 2, 4);
    mov_imm(&mut a, 0, 1);
    syscall(&mut a);
    a.emit(&[0x48, 0x83, 0xF8, 0x04]);
    jne(&mut a, "fail");

    // open /opt/chromium/ozone.wl and read proto to r15+0x80
    lea_rip(&mut a, 7, "wlpath");
    xor_reg(&mut a, 6);
    xor_reg(&mut a, 2);
    mov_imm(&mut a, 0, 2);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");
    a.emit(&[0x89, 0xC7]);
    a.emit(&[0x49, 0x8D, 0xB7, 0x80, 0x00, 0x00, 0x00]);
    mov_imm(&mut a, 2, 0x300);
    xor_reg(&mut a, 0);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");
    a.emit(&[0x89, 0xC1]); // ecx = proto len

    // socket(AF_UNIX, SOCK_STREAM, 0)
    mov_imm(&mut a, 7, 1);
    mov_imm(&mut a, 6, 1);
    xor_reg(&mut a, 2);
    mov_imm(&mut a, 0, 41);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");
    a.emit(&[0x89, 0xC5]); // ebp = sock

    // sockaddr_un at r15: family=AF_UNIX, path=/run/wayland-0
    xor_reg(&mut a, 0);
    a.emit(&[0x49, 0x89, 0x07]);
    a.emit(&[0x49, 0x89, 0x47, 0x08]);
    a.emit(&[0x49, 0x89, 0x47, 0x10]);
    a.emit(&[0x66, 0x41, 0xC7, 0x07, 0x01, 0x00]);
    lea_rip(&mut a, 6, "path");
    a.emit(&[0x49, 0x8D, 0x7F, 0x02]);
    mov_imm(&mut a, 2, 15);
    a.emit(&[0xF3, 0xA4]);

    // connect(sock, r15, 110)
    a.emit(&[0x89, 0xEF]);
    a.emit(&[0x4C, 0x89, 0xFE]);
    mov_imm(&mut a, 2, 110);
    mov_imm(&mut a, 0, 42);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");

    // iov at r15+0x40
    a.emit(&[0x49, 0x8D, 0x87, 0x80, 0x00, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0x47, 0x40]);
    a.emit(&[0x49, 0x89, 0x4F, 0x48]); // iov_len = ecx

    // cmsg at r15+0x50
    a.emit(&[0x49, 0xC7, 0x47, 0x50, 0x14, 0x00, 0x00, 0x00]);
    a.emit(&[0x41, 0xC7, 0x47, 0x58, 0x01, 0x00, 0x00, 0x00]);
    a.emit(&[0x41, 0xC7, 0x47, 0x5C, 0x01, 0x00, 0x00, 0x00]);
    a.emit(&[0x41, 0x89, 0x5F, 0x60]);

    // msghdr at r15+0x280
    xor_reg(&mut a, 0);
    a.emit(&[0x49, 0x89, 0x87, 0x80, 0x02, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0x87, 0x88, 0x02, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0x87, 0x90, 0x02, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0x87, 0x98, 0x02, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0x87, 0xA0, 0x02, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0x87, 0xA8, 0x02, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0x87, 0xB0, 0x02, 0x00, 0x00]);
    a.emit(&[0x49, 0x8D, 0x47, 0x40]);
    a.emit(&[0x49, 0x89, 0x87, 0x90, 0x02, 0x00, 0x00]);
    a.emit(&[
        0x49, 0xC7, 0x87, 0x98, 0x02, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    ]);
    a.emit(&[0x49, 0x8D, 0x47, 0x50]);
    a.emit(&[0x49, 0x89, 0x87, 0xA0, 0x02, 0x00, 0x00]);
    a.emit(&[
        0x49, 0xC7, 0x87, 0xA8, 0x02, 0x00, 0x00, 0x18, 0x00, 0x00, 0x00,
    ]);

    // sendmsg(sock, msghdr, 0)
    a.emit(&[0x89, 0xEF]);
    a.emit(&[0x49, 0x8D, 0xB7, 0x80, 0x02, 0x00, 0x00]);
    xor_reg(&mut a, 2);
    mov_imm(&mut a, 0, 46);
    syscall(&mut a);
    a.emit(&[0x48, 0x39, 0xC8]); // cmp rax, rcx
    jne(&mut a, "fail");

    xor_reg(&mut a, 7);
    mov_imm(&mut a, 0, 60);
    syscall(&mut a);

    emit_fail_exit(&mut a);
    a.label("name");
    a.emit(b"chr\0");
    a.label("magic");
    a.emit(&crate::wayland::GATE_CHR_PAINT_MAGIC);
    a.label("path");
    a.emit(b"/run/wayland-0\0");
    a.label("wlpath");
    a.emit(b"/opt/chromium/ozone.wl\0");

    let text = a.finish();
    assert!(text.len() < 0x1000, "ozone text overflow {}", text.len());
    build_static_user_elf(&text)
}

fn xor_r10(a: &mut Asm) {
    a.emit(&[0x4D, 0x31, 0xD2]);
}

fn xor_r8(a: &mut Asm) {
    a.emit(&[0x4D, 0x31, 0xC0]);
}

fn xor_r9(a: &mut Asm) {
    a.emit(&[0x4D, 0x31, 0xC9]);
}

fn emit_sockaddr_in(a: &mut Asm, port_be: u16, ip: [u8; 4]) {
    xor_reg(a, 0);
    a.emit(&[0x49, 0x89, 0x07]);
    a.emit(&[0x49, 0x89, 0x47, 0x08]);
    a.emit(&[0x66, 0x41, 0xC7, 0x07, 0x02, 0x00]);
    a.emit(&[0x66, 0x41, 0xC7, 0x47, 0x02]);
    a.emit(&port_be.to_le_bytes());
    a.emit(&[0x41, 0xC7, 0x47, 0x04]);
    a.emit(&ip);
}

/// Ring 3 Chromium HTTPS: DNS `sendto`/`recvfrom`, TLS ClientHello, h2 preface.
pub fn https_userspace_elf_data() -> Vec<u8> {
    let query = crate::init::chrome_dns_query();
    let qlen = query.len() as u32;
    let ch = [22u8, 0x03, 0x03, 0x00, 0x01, 0x01];
    let h2 = crate::init::H2_PREFACE;

    let mut a = Asm::new();
    a.emit(&[0x48, 0x81, 0xEC, 0x00, 0x02, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0xE7]);

    emit_open_read_ok(&mut a, "ca", "fail");

    mov_imm(&mut a, 7, 2);
    mov_imm(&mut a, 6, 2);
    xor_reg(&mut a, 2);
    mov_imm(&mut a, 0, 41);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");
    a.emit(&[0x89, 0xC3]);

    emit_sockaddr_in(&mut a, 0x3500, [1, 1, 1, 1]);
    a.emit(&[0x89, 0xDF]);
    lea_rip(&mut a, 6, "query");
    mov_imm(&mut a, 2, qlen);
    xor_r10(&mut a);
    a.emit(&[0x4D, 0x89, 0xF8]);
    a.emit(&[0x49, 0xC7, 0xC1, 0x10, 0x00, 0x00, 0x00]);
    mov_imm(&mut a, 0, 44);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");

    a.emit(&[0x89, 0xDF]);
    a.emit(&[0x49, 0x8D, 0x77, 0x20]);
    mov_imm(&mut a, 2, 0x1C0);
    xor_r10(&mut a);
    xor_r8(&mut a);
    xor_r9(&mut a);
    mov_imm(&mut a, 0, 45);
    syscall(&mut a);
    a.emit(&[0x48, 0x83, 0xF8, 0x2C]);
    a.emit(&[0x0F, 0x8C]);
    a.rel32("fail");
    a.emit(&[0x41, 0x80, 0x7F, 0x49, 0x5D]);
    jne(&mut a, "fail");

    mov_imm(&mut a, 7, 2);
    mov_imm(&mut a, 6, 1);
    xor_reg(&mut a, 2);
    mov_imm(&mut a, 0, 41);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");
    a.emit(&[0x89, 0xC5]);

    emit_sockaddr_in(&mut a, 0xBB01, [93, 184, 216, 34]);
    a.emit(&[0x89, 0xEF]);
    a.emit(&[0x4C, 0x89, 0xFE]);
    mov_imm(&mut a, 2, 16);
    mov_imm(&mut a, 0, 42);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");

    a.emit(&[0x89, 0xEF]);
    lea_rip(&mut a, 6, "ch");
    mov_imm(&mut a, 2, ch.len() as u32);
    xor_r10(&mut a);
    xor_r8(&mut a);
    xor_r9(&mut a);
    mov_imm(&mut a, 0, 44);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");

    a.emit(&[0x89, 0xEF]);
    a.emit(&[0x49, 0x8D, 0x77, 0x20]);
    mov_imm(&mut a, 2, 0x1C0);
    xor_r10(&mut a);
    xor_r8(&mut a);
    xor_r9(&mut a);
    mov_imm(&mut a, 0, 45);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");
    a.emit(&[0x41, 0x80, 0x7F, 0x20, 0x16]);
    jne(&mut a, "fail");

    a.emit(&[0x89, 0xEF]);
    lea_rip(&mut a, 6, "h2");
    mov_imm(&mut a, 2, h2.len() as u32);
    xor_r10(&mut a);
    xor_r8(&mut a);
    xor_r9(&mut a);
    mov_imm(&mut a, 0, 44);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");

    xor_reg(&mut a, 7);
    mov_imm(&mut a, 0, 60);
    syscall(&mut a);

    emit_fail_exit(&mut a);
    a.label("ca");
    a.emit(b"/etc/ssl/certs/ca-certificates.crt\0");
    a.label("query");
    a.emit(&query);
    a.label("ch");
    a.emit(&ch);
    a.label("h2");
    a.emit(h2);

    let text = a.finish();
    assert!(
        text.len() < 0x1000,
        "https userspace text overflow {}",
        text.len()
    );
    build_static_user_elf(&text)
}

fn mov_esi(a: &mut Asm, imm: u32) {
    a.emit(&[0xBE]);
    a.emit(&imm.to_le_bytes());
}

fn jz(a: &mut Asm, name: &'static str) {
    a.emit(&[0x0F, 0x84]);
    a.rel32(name);
}

/// Ring 3 `drmGetVersion` + GEM + PRIME + `zwp_linux_dmabuf_v1`.
pub fn gl_userspace_elf_data() -> Vec<u8> {
    let mut a = Asm::new();
    a.emit(&[0x48, 0x81, 0xEC, 0x00, 0x04, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0xE7]);

    lea_rip(&mut a, 7, "dri");
    mov_imm(&mut a, 6, 2);
    xor_reg(&mut a, 2);
    mov_imm(&mut a, 0, 2);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");
    a.emit(&[0x89, 0xC3]);

    a.emit(&[0x49, 0xC7, 0x47, 0x10, 0x20, 0x00, 0x00, 0x00]);
    a.emit(&[0x49, 0x8D, 0x47, 0x80]);
    a.emit(&[0x49, 0x89, 0x47, 0x18]);
    a.emit(&[0x49, 0xC7, 0x47, 0x20, 0x10, 0x00, 0x00, 0x00]);
    a.emit(&[0x49, 0x8D, 0x87, 0xA0, 0x00, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0x47, 0x28]);
    a.emit(&[0x49, 0xC7, 0x47, 0x30, 0x20, 0x00, 0x00, 0x00]);
    a.emit(&[0x49, 0x8D, 0x87, 0xC0, 0x00, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0x47, 0x38]);

    a.emit(&[0x89, 0xDF]);
    mov_esi(&mut a, 0xC040_6400);
    a.emit(&[0x4C, 0x89, 0xFA]);
    mov_imm(&mut a, 0, 16);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");
    a.emit(&[0x41, 0x83, 0x3F, 0x00]);
    jz(&mut a, "fail");

    a.emit(&[
        0x49, 0xC7, 0x87, 0x00, 0x01, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00,
    ]);
    a.emit(&[
        0x49, 0xC7, 0x87, 0x08, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ]);
    a.emit(&[0x89, 0xDF]);
    mov_esi(&mut a, 0xC010_6409);
    a.emit(&[0x49, 0x8D, 0x97, 0x00, 0x01, 0x00, 0x00]);
    mov_imm(&mut a, 0, 16);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");

    a.emit(&[0x41, 0x8B, 0x87, 0x08, 0x01, 0x00, 0x00]);
    a.emit(&[0x41, 0x89, 0x87, 0x10, 0x01, 0x00, 0x00]);
    a.emit(&[
        0x41, 0xC7, 0x87, 0x14, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ]);
    a.emit(&[
        0x41, 0xC7, 0x87, 0x18, 0x01, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF,
    ]);
    a.emit(&[0x89, 0xDF]);
    mov_esi(&mut a, 0xC00C_642D);
    a.emit(&[0x49, 0x8D, 0x97, 0x10, 0x01, 0x00, 0x00]);
    mov_imm(&mut a, 0, 16);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");
    a.emit(&[0x41, 0x8B, 0x9F, 0x18, 0x01, 0x00, 0x00]);

    lea_rip(&mut a, 7, "wlpath");
    xor_reg(&mut a, 6);
    xor_reg(&mut a, 2);
    mov_imm(&mut a, 0, 2);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");
    a.emit(&[0x89, 0xC7]);
    a.emit(&[0x49, 0x8D, 0xB7, 0x80, 0x00, 0x00, 0x00]);
    mov_imm(&mut a, 2, 0x200);
    xor_reg(&mut a, 0);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");
    a.emit(&[0x89, 0xC1]);

    mov_imm(&mut a, 7, 1);
    mov_imm(&mut a, 6, 1);
    xor_reg(&mut a, 2);
    mov_imm(&mut a, 0, 41);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");
    a.emit(&[0x89, 0xC5]);

    xor_reg(&mut a, 0);
    a.emit(&[0x49, 0x89, 0x07]);
    a.emit(&[0x49, 0x89, 0x47, 0x08]);
    a.emit(&[0x49, 0x89, 0x47, 0x10]);
    a.emit(&[0x66, 0x41, 0xC7, 0x07, 0x01, 0x00]);
    lea_rip(&mut a, 6, "path");
    a.emit(&[0x49, 0x8D, 0x7F, 0x02]);
    mov_imm(&mut a, 2, 15);
    a.emit(&[0xF3, 0xA4]);

    a.emit(&[0x89, 0xEF]);
    a.emit(&[0x4C, 0x89, 0xFE]);
    mov_imm(&mut a, 2, 110);
    mov_imm(&mut a, 0, 42);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");

    a.emit(&[0x49, 0x8D, 0x87, 0x80, 0x00, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0x47, 0x40]);
    a.emit(&[0x49, 0x89, 0x4F, 0x48]);

    a.emit(&[0x49, 0xC7, 0x47, 0x50, 0x14, 0x00, 0x00, 0x00]);
    a.emit(&[0x41, 0xC7, 0x47, 0x58, 0x01, 0x00, 0x00, 0x00]);
    a.emit(&[0x41, 0xC7, 0x47, 0x5C, 0x01, 0x00, 0x00, 0x00]);
    a.emit(&[0x41, 0x89, 0x5F, 0x60]);

    xor_reg(&mut a, 0);
    a.emit(&[0x49, 0x89, 0x87, 0x80, 0x02, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0x87, 0x88, 0x02, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0x87, 0x90, 0x02, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0x87, 0x98, 0x02, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0x87, 0xA0, 0x02, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0x87, 0xA8, 0x02, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0x87, 0xB0, 0x02, 0x00, 0x00]);
    a.emit(&[0x49, 0x8D, 0x47, 0x40]);
    a.emit(&[0x49, 0x89, 0x87, 0x90, 0x02, 0x00, 0x00]);
    a.emit(&[
        0x49, 0xC7, 0x87, 0x98, 0x02, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    ]);
    a.emit(&[0x49, 0x8D, 0x47, 0x50]);
    a.emit(&[0x49, 0x89, 0x87, 0xA0, 0x02, 0x00, 0x00]);
    a.emit(&[
        0x49, 0xC7, 0x87, 0xA8, 0x02, 0x00, 0x00, 0x18, 0x00, 0x00, 0x00,
    ]);

    a.emit(&[0x89, 0xEF]);
    a.emit(&[0x49, 0x8D, 0xB7, 0x80, 0x02, 0x00, 0x00]);
    xor_reg(&mut a, 2);
    mov_imm(&mut a, 0, 46);
    syscall(&mut a);
    a.emit(&[0x48, 0x39, 0xC8]);
    jne(&mut a, "fail");

    xor_reg(&mut a, 7);
    mov_imm(&mut a, 0, 60);
    syscall(&mut a);

    emit_fail_exit(&mut a);
    a.label("dri");
    a.emit(b"/dev/dri/card0\0");
    a.label("path");
    a.emit(b"/run/wayland-0\0");
    a.label("wlpath");
    a.emit(b"/opt/chromium/gpu.wl\0");

    let text = a.finish();
    assert!(
        text.len() < 0x1000,
        "gl userspace text overflow {}",
        text.len()
    );
    build_static_user_elf(&text)
}

/// Ring 3 ICU + fonts.conf + libffmpeg + local MP4 + ALSA pcm + Pulse native AUTH.
pub fn media_userspace_elf_data() -> Vec<u8> {
    let auth = crate::init::pa_auth_packet();
    let mut a = Asm::new();
    a.emit(&[0x48, 0x81, 0xEC, 0x00, 0x02, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0xE7]);

    emit_open_read_ok(&mut a, "icu", "fail");
    emit_open_read_ok(&mut a, "fonts", "fail");
    emit_open_read_ok(&mut a, "ffmpeg", "fail");
    emit_open_read_ok(&mut a, "mp4", "fail");
    emit_open_read_ok(&mut a, "pcm", "fail");

    mov_imm(&mut a, 7, 1);
    mov_imm(&mut a, 6, 1);
    xor_reg(&mut a, 2);
    mov_imm(&mut a, 0, 41);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");
    a.emit(&[0x89, 0xC5]);

    xor_reg(&mut a, 0);
    a.emit(&[0x49, 0x89, 0x07]);
    a.emit(&[0x49, 0x89, 0x47, 0x08]);
    a.emit(&[0x49, 0x89, 0x47, 0x10]);
    a.emit(&[0x49, 0x89, 0x47, 0x18]);
    a.emit(&[0x66, 0x41, 0xC7, 0x07, 0x01, 0x00]);
    lea_rip(&mut a, 6, "pulse");
    a.emit(&[0x49, 0x8D, 0x7F, 0x02]);
    mov_imm(&mut a, 2, 29);
    a.emit(&[0xF3, 0xA4]);

    a.emit(&[0x89, 0xEF]);
    a.emit(&[0x4C, 0x89, 0xFE]);
    mov_imm(&mut a, 2, 110);
    mov_imm(&mut a, 0, 42);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");

    a.emit(&[0x89, 0xEF]);
    lea_rip(&mut a, 6, "auth");
    mov_imm(&mut a, 2, auth.len() as u32);
    xor_r10(&mut a);
    xor_r8(&mut a);
    xor_r9(&mut a);
    mov_imm(&mut a, 0, 44);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");

    a.emit(&[0x89, 0xEF]);
    a.emit(&[0x49, 0x8D, 0x77, 0x40]);
    mov_imm(&mut a, 2, 0x40);
    xor_r10(&mut a);
    xor_r8(&mut a);
    xor_r9(&mut a);
    mov_imm(&mut a, 0, 45);
    syscall(&mut a);
    a.emit(&[0x48, 0x83, 0xF8, 0x0C]);
    a.emit(&[0x0F, 0x8C]);
    a.rel32("fail");

    xor_reg(&mut a, 7);
    mov_imm(&mut a, 0, 60);
    syscall(&mut a);

    emit_fail_exit(&mut a);
    a.label("icu");
    a.emit(b"/opt/chromium/icudtl.dat\0");
    a.label("fonts");
    a.emit(b"/etc/fonts/fonts.conf\0");
    a.label("ffmpeg");
    a.emit(b"/opt/chromium/libffmpeg.so\0");
    a.label("mp4");
    a.emit(b"/opt/chromium/sample.mp4\0");
    a.label("pcm");
    a.emit(b"/dev/snd/pcmC0D0p\0");
    a.label("pulse");
    a.emit(b"/run/user/1000/pulse/native\0");
    a.label("auth");
    a.emit(&auth);

    let text = a.finish();
    assert!(
        text.len() < 0x1000,
        "media userspace text overflow {}",
        text.len()
    );
    build_static_user_elf(&text)
}

/// Ring 3 chrome ELF + `.desktop` + `libcups` + session bus connect.
pub fn desktop_userspace_elf_data() -> Vec<u8> {
    let mut a = Asm::new();
    a.emit(&[0x48, 0x81, 0xEC, 0x00, 0x01, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0xE7]);

    emit_open_read_ok(&mut a, "chrome", "fail");
    emit_open_read_ok(&mut a, "desktop", "fail");
    emit_open_read_ok(&mut a, "cups", "fail");
    emit_open_read_ok(&mut a, "flags", "fail");

    mov_imm(&mut a, 7, 1);
    mov_imm(&mut a, 6, 1);
    xor_reg(&mut a, 2);
    mov_imm(&mut a, 0, 41);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");
    a.emit(&[0x89, 0xC5]);

    xor_reg(&mut a, 0);
    a.emit(&[0x49, 0x89, 0x07]);
    a.emit(&[0x49, 0x89, 0x47, 0x08]);
    a.emit(&[0x49, 0x89, 0x47, 0x10]);
    a.emit(&[0x49, 0x89, 0x47, 0x18]);
    a.emit(&[0x66, 0x41, 0xC7, 0x07, 0x01, 0x00]);
    lea_rip(&mut a, 6, "session");
    a.emit(&[0x49, 0x8D, 0x7F, 0x02]);
    mov_imm(&mut a, 2, 22);
    a.emit(&[0xF3, 0xA4]);

    a.emit(&[0x89, 0xEF]);
    a.emit(&[0x4C, 0x89, 0xFE]);
    mov_imm(&mut a, 2, 110);
    mov_imm(&mut a, 0, 42);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");

    xor_reg(&mut a, 7);
    mov_imm(&mut a, 0, 60);
    syscall(&mut a);

    emit_fail_exit(&mut a);
    a.label("chrome");
    a.emit(b"/opt/chromium/chrome\0");
    a.label("desktop");
    a.emit(b"/usr/share/applications/chromium.desktop\0");
    a.label("cups");
    a.emit(b"/lib/libcups.so.2\0");
    a.label("flags");
    a.emit(b"/opt/chromium/chrome-flags\0");
    a.label("session");
    a.emit(b"/run/user/1000/bus\0");

    let text = a.finish();
    assert!(
        text.len() < 0x1000,
        "desktop userspace text overflow {}",
        text.len()
    );
    build_static_user_elf(&text)
}

/// Ring 3 chrome-sandbox helper + flags + crashpad minidump + `chrome://sandbox`.
pub fn sandbox_userspace_elf_data() -> Vec<u8> {
    let mut a = Asm::new();
    a.emit(&[0x48, 0x81, 0xEC, 0x00, 0x01, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0xE7]);

    emit_open_read_ok(&mut a, "helper", "fail");
    emit_open_read_ok(&mut a, "page", "fail");
    emit_open_read_ok(&mut a, "flags", "fail");
    emit_open_read_ok(&mut a, "dump", "fail");

    xor_reg(&mut a, 7);
    mov_imm(&mut a, 0, 60);
    syscall(&mut a);

    emit_fail_exit(&mut a);
    a.label("helper");
    a.emit(b"/opt/chromium/chrome-sandbox\0");
    a.label("page");
    a.emit(b"/opt/chromium/sandbox.html\0");
    a.label("flags");
    a.emit(b"/opt/chromium/chrome-flags\0");
    a.label("dump");
    a.emit(b"/home/user/.config/chromium/Crash Reports/chromium.dmp\0");

    let text = a.finish();
    assert!(
        text.len() < 0x1000,
        "sandbox userspace text overflow {}",
        text.len()
    );
    build_static_user_elf(&text)
}

/// glibc TLS: `%fs:0` is PT_TLS (initial-exec), then a GD-style `%fs` helper.
pub fn glibc_tls_userspace_elf_data() -> Vec<u8> {
    let mut a = Asm::new();
    // mov rax, %fs:0
    a.emit(&[0x64, 0x48, 0x8B, 0x04, 0x25, 0x00, 0x00, 0x00, 0x00]);
    // mov rcx, MAGIC
    a.emit(&[0x48, 0xB9]);
    a.emit(&GATE_CHR_TLS_MAGIC.to_le_bytes());
    // cmp rax, rcx ; jne fail
    a.emit(&[0x48, 0x39, 0xC8]);
    a.emit(&[0x0F, 0x85]);
    a.rel32("fail");
    // call tls_get_addr
    a.emit(&[0xE8]);
    a.rel32("gd");
    // cmp rax, rcx ; jne fail
    a.emit(&[0x48, 0x39, 0xC8]);
    a.emit(&[0x0F, 0x85]);
    a.rel32("fail");
    emit_ok_write_exit(&mut a, "GATE_CHR tls\n", 13);
    emit_fail_exit(&mut a);
    a.label("gd");
    a.emit(&[0x64, 0x48, 0x8B, 0x04, 0x25, 0x00, 0x00, 0x00, 0x00]);
    a.emit(&[0xC3]);
    let text = a.finish();
    assert!(
        text.len() < 0x1000,
        "glibc TLS text overflow {}",
        text.len()
    );
    build_static_user_elf_with_tls(&text, &GATE_CHR_TLS_MAGIC.to_le_bytes())
}

/// Ring 3 chrome.needed + libstdc++ / libgcc / libz.
pub fn needed_userspace_elf_data() -> Vec<u8> {
    let mut a = Asm::new();
    a.emit(&[0x48, 0x81, 0xEC, 0x00, 0x01, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0xE7]);
    emit_open_read_ok(&mut a, "needed", "fail");
    emit_open_read_ok(&mut a, "stdcxx", "fail");
    emit_open_read_ok(&mut a, "gcc", "fail");
    xor_reg(&mut a, 7);
    mov_imm(&mut a, 0, 60);
    syscall(&mut a);
    emit_fail_exit(&mut a);
    a.label("needed");
    a.emit(b"/opt/chromium/chrome.needed\0");
    a.label("stdcxx");
    a.emit(b"/lib/libstdc++.so.6\0");
    a.label("gcc");
    a.emit(b"/lib/libgcc_s.so.1\0");
    let text = a.finish();
    build_static_user_elf(&text)
}

/// Ring 3 vivaldi-bin + UI resources.
pub fn vivaldi_userspace_elf_data() -> Vec<u8> {
    let mut a = Asm::new();
    a.emit(&[0x48, 0x81, 0xEC, 0x00, 0x01, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0xE7]);
    emit_open_read_ok(&mut a, "bin", "fail");
    emit_open_read_ok(&mut a, "ui", "fail");
    xor_reg(&mut a, 7);
    mov_imm(&mut a, 0, 60);
    syscall(&mut a);
    emit_fail_exit(&mut a);
    a.label("bin");
    a.emit(b"/opt/vivaldi/vivaldi-bin\0");
    a.label("ui");
    a.emit(b"/opt/vivaldi/resources/vivaldi/vivaldi.html\0");
    let text = a.finish();
    build_static_user_elf(&text)
}

/// Ring 3 Mojo: chrome flags without `--single-process` plus zygote + renderer.
pub fn mojo_userspace_elf_data() -> Vec<u8> {
    let mut a = Asm::new();
    a.emit(&[0x48, 0x81, 0xEC, 0x00, 0x01, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0xE7]);
    emit_open_read_ok(&mut a, "flags", "fail");
    emit_open_read_ok(&mut a, "zygote", "fail");
    emit_open_read_ok(&mut a, "renderer", "fail");
    xor_reg(&mut a, 7);
    mov_imm(&mut a, 0, 60);
    syscall(&mut a);
    emit_fail_exit(&mut a);
    a.label("flags");
    a.emit(b"/opt/chromium/chrome-flags\0");
    a.label("zygote");
    a.emit(b"/opt/chromium/chrome-zygote\0");
    a.label("renderer");
    a.emit(b"/opt/chromium/chrome-renderer\0");
    let text = a.finish();
    build_static_user_elf(&text)
}

/// Ring 3 `io_uring_setup(8)` + `io_uring_enter` (not ENOSYS).
pub fn iouring_userspace_elf_data() -> Vec<u8> {
    let mut a = Asm::new();
    // sub rsp, 0x80; mov r15, rsp
    a.emit(&[0x48, 0x81, 0xEC, 0x80, 0x00, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0xE7]);
    // zero params: xor eax,eax; mov rdi,rsp; mov rcx,16; rep stosq
    a.emit(&[
        0x31, 0xC0, 0x48, 0x89, 0xE7, 0x48, 0xC7, 0xC1, 0x10, 0x00, 0x00, 0x00, 0xF3, 0x48, 0xAB,
    ]);
    // io_uring_setup(8, rsp)
    mov_imm(&mut a, 7, 8);
    a.emit(&[0x48, 0x89, 0xE6]); // mov rsi, rsp
    mov_imm(&mut a, 0, 425);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");
    a.emit(&[0x89, 0xC3]); // mov ebx, eax
    // io_uring_enter(fd, 0, 0, 0)
    a.emit(&[0x89, 0xDF]); // mov edi, ebx
    xor_reg(&mut a, 6);
    xor_reg(&mut a, 2);
    a.emit(&[0x4D, 0x31, 0xD2]); // xor r10, r10
    mov_imm(&mut a, 0, 426);
    syscall(&mut a);
    a.emit(&[0x48, 0x85, 0xC0]);
    js(&mut a, "fail");
    emit_ok_write_exit(&mut a, "GATE_CHR iouring\n", 17);
    emit_fail_exit(&mut a);
    let text = a.finish();
    assert!(
        text.len() < 0x1000,
        "iouring userspace text overflow {}",
        text.len()
    );
    build_static_user_elf(&text)
}

/// Ring 3 polish: unpacked extension, Downloads, DevTools page.
pub fn polish_userspace_elf_data() -> Vec<u8> {
    let mut a = Asm::new();
    a.emit(&[0x48, 0x81, 0xEC, 0x00, 0x01, 0x00, 0x00]);
    a.emit(&[0x49, 0x89, 0xE7]);
    emit_open_read_ok(&mut a, "ext", "fail");
    emit_open_read_ok(&mut a, "dl", "fail");
    emit_open_read_ok(&mut a, "dt", "fail");
    xor_reg(&mut a, 7);
    mov_imm(&mut a, 0, 60);
    syscall(&mut a);
    emit_fail_exit(&mut a);
    a.label("ext");
    a.emit(b"/home/user/.config/chromium/Default/Extensions/knoxos/manifest.json\0");
    a.label("dl");
    a.emit(b"/home/user/Downloads/example.html\0");
    a.label("dt");
    a.emit(b"/opt/chromium/devtools.html\0");
    let text = a.finish();
    build_static_user_elf(&text)
}
