use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::init::elf::build_static_user_elf;

/// Gate F1: mmap a 64×64 BGRA buffer, fill a magic pixel, ioctl(/dev/wl0) present.
pub fn display_client_elf_data() -> Vec<u8> {
    shm_present_client_elf(0xFFC0_FFEE, b"GATE_F1 client isolated\n")
}

/// Gate F3: same SHM present path, dark terminal pixels, isolated from
/// `WindowContentType::TerminalSurface`.
pub fn terminal_client_elf_data() -> Vec<u8> {
    shm_present_client_elf(0xFF1A_1A1A, b"GATE_F3 terminal isolated\n")
}

/// Gate F4: userspace launcher client (Paint) via SHM, not an Empty stub.
pub fn launcher_client_elf_data() -> Vec<u8> {
    shm_present_client_elf(0xFF33_66CC, b"GATE_F4 launcher userspace\n")
}

/// Gate F6: Calculator is a Ring 3 SHM client, not `WindowContentType::Gpui(ViewKind::Calculator)`.
pub fn calculator_client_elf_data() -> Vec<u8> {
    shm_present_client_elf(0xFF20_C997, b"GATE_F6 calculator userspace\n")
}

/// Gate F7: Files is a Ring 3 SHM client, not `WindowContentType::Gpui(ViewKind::Files)`.
pub fn files_client_elf_data() -> Vec<u8> {
    shm_present_client_elf(0xFF4E_8A26, b"GATE_F7 files userspace\n")
}

/// D15.2: window-sized Button SHM via `/run/wayland-0`.
pub fn gpui_button_client_elf_data() -> Vec<u8> {
    gpui_shm_unix_client_elf(
        u32::from_le_bytes(*b"GPUI"),
        crate::wayland::GATE_GPUI_BUTTON_W,
        crate::wayland::GATE_GPUI_BUTTON_H,
        b"GATE_GPUI ring3 button\n",
    )
}

/// D15.3: window-sized Calculator SHM via `/run/wayland-0`.
pub fn gpui_calc_client_elf_data() -> Vec<u8> {
    gpui_shm_unix_client_elf(
        u32::from_le_bytes(*b"GPUC"),
        crate::wayland::GATE_GPUI_CALC_W,
        crate::wayland::GATE_GPUI_CALC_H,
        b"GATE_GPUI ring3 calc\n",
    )
}

/// Ring 3 `openpty` + SHM present — interactive PTY path is userspace-shaped.
pub fn pty_terminal_client_elf_data() -> Vec<u8> {
    let marker = b"GATE_TERM pty userspace\n";
    let fill = 0xFF2B_1A0Du32.to_le_bytes();
    let mut code = Vec::new();
    let mut rels: Vec<(usize, &'static str)> = Vec::new();
    let mut labels = BTreeMap::new();

    let mut emit = |code: &mut Vec<u8>, bytes: &[u8]| code.extend_from_slice(bytes);
    let mut js = |code: &mut Vec<u8>, rels: &mut Vec<(usize, &'static str)>, name: &'static str| {
        emit(code, &[0x0F, 0x88]);
        rels.push((code.len(), name));
        emit(code, &[0, 0, 0, 0]);
    };
    let mut lea_rdi =
        |code: &mut Vec<u8>, rels: &mut Vec<(usize, &'static str)>, name: &'static str| {
            emit(code, &[0x48, 0x8D, 0x3D]);
            rels.push((code.len(), name));
            emit(code, &[0, 0, 0, 0]);
        };
    let mut lea_rsi =
        |code: &mut Vec<u8>, rels: &mut Vec<(usize, &'static str)>, name: &'static str| {
            emit(code, &[0x48, 0x8D, 0x35]);
            rels.push((code.len(), name));
            emit(code, &[0, 0, 0, 0]);
        };

    // mmap(NULL, 0x4000, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0)
    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x09, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0x31, 0xFF]);
    emit(&mut code, &[0x48, 0xC7, 0xC6, 0x00, 0x40, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0xC7, 0xC2, 0x03, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x49, 0xC7, 0xC2, 0x22, 0x80, 0x00, 0x00]);
    emit(&mut code, &[0x49, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]);
    emit(&mut code, &[0x4D, 0x31, 0xC9]);
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x85, 0xC0]);
    js(&mut code, &mut rels, "fail");
    emit(&mut code, &[0x49, 0x89, 0xC4]); // r12 = buf
    emit(&mut code, &[0x4C, 0x89, 0xE7]);
    emit(&mut code, &[0x48, 0xC7, 0xC1, 0x00, 0x10, 0x00, 0x00]);
    emit(&mut code, &[0xB8, fill[0], fill[1], fill[2], fill[3]]);
    emit(&mut code, &[0xF3, 0xAB]);

    let mut jle =
        |code: &mut Vec<u8>, rels: &mut Vec<(usize, &'static str)>, name: &'static str| {
            emit(code, &[0x0F, 0x8E]);
            rels.push((code.len(), name));
            emit(code, &[0, 0, 0, 0]);
        };

    // openpty(&master, &name)  syscall 0x3051
    emit(&mut code, &[0x48, 0x81, 0xEC, 0x80, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x51, 0x30, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0x89, 0xE7]);
    emit(&mut code, &[0x48, 0x8D, 0x74, 0x24, 0x08]);
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x85, 0xC0]);
    js(&mut code, &mut rels, "fail");

    // open(slave_name, O_RDWR)
    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x02, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0x8D, 0x7C, 0x24, 0x08]);
    emit(&mut code, &[0x48, 0xC7, 0xC6, 0x02, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0x31, 0xD2]);
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x85, 0xC0]);
    js(&mut code, &mut rels, "fail");
    emit(&mut code, &[0x49, 0x89, 0xC6]); // r14 = slave fd

    // write(slave, "ok\n", 3)
    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x4C, 0x89, 0xF7]);
    lea_rsi(&mut code, &mut rels, "echo");
    emit(&mut code, &[0x48, 0xC7, 0xC2, 0x03, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x85, 0xC0]);
    js(&mut code, &mut rels, "fail");

    // read(master, [rsp+0x40], 8)
    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x00, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0x63, 0x3C, 0x24]); // movsxd rdi, [rsp]
    emit(&mut code, &[0x48, 0x8D, 0x74, 0x24, 0x40]);
    emit(&mut code, &[0x48, 0xC7, 0xC2, 0x08, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x85, 0xC0]);
    jle(&mut code, &mut rels, "fail");

    // open("/dev/wl0", O_RDWR)
    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x02, 0x00, 0x00, 0x00]);
    lea_rdi(&mut code, &mut rels, "path");
    emit(&mut code, &[0x48, 0xC7, 0xC6, 0x02, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0x31, 0xD2]);
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x85, 0xC0]);
    js(&mut code, &mut rels, "fail");
    emit(&mut code, &[0x49, 0x89, 0xC5]); // r13 = fd

    // ioctl(fd, 0x574C0001, buf)
    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x10, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x4C, 0x89, 0xEF]);
    emit(&mut code, &[0x48, 0xC7, 0xC6, 0x01, 0x00, 0x4C, 0x57]);
    emit(&mut code, &[0x4C, 0x89, 0xE2]);
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x85, 0xC0]);
    js(&mut code, &mut rels, "fail");

    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00]);
    lea_rsi(&mut code, &mut rels, "marker");
    let n = marker.len() as u32;
    emit(&mut code, &[0x48, 0xC7, 0xC2]);
    emit(&mut code, &n.to_le_bytes());
    emit(&mut code, &[0x0F, 0x05]);
    let interactive = b"GATE_TERM pty interactive\n";
    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00]);
    lea_rsi(&mut code, &mut rels, "interactive");
    let ni = interactive.len() as u32;
    emit(&mut code, &[0x48, 0xC7, 0xC2]);
    emit(&mut code, &ni.to_le_bytes());
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x31, 0xFF]);
    emit(&mut code, &[0xEB, 0x07]);
    labels.insert("fail", code.len());
    emit(&mut code, &[0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x0F, 0x05]);
    labels.insert("path", code.len());
    emit(&mut code, b"/dev/wl0\0");
    labels.insert("marker", code.len());
    emit(&mut code, marker);
    labels.insert("interactive", code.len());
    emit(&mut code, interactive);
    labels.insert("echo", code.len());
    emit(&mut code, b"ok\n");

    for (off, name) in rels {
        let target = *labels.get(name).expect(name);
        let disp = (target as i32).wrapping_sub((off as i32) + 4);
        code[off..off + 4].copy_from_slice(&disp.to_le_bytes());
    }
    build_static_user_elf(&code)
}

/// Settings is a Ring 3 SHM client, not `WindowContentType::Gpui(ViewKind::Settings)`.
pub fn settings_client_elf_data() -> Vec<u8> {
    shm_present_client_elf(0xFF38_3840, b"GATE_F8 settings userspace\n")
}

/// Task Manager is a Ring 3 SHM client, not `WindowContentType::Gpui(ViewKind::Tasks)`.
pub fn taskmgr_client_elf_data() -> Vec<u8> {
    shm_present_client_elf(0xFF48_3248, b"GATE_F9 taskmgr userspace\n")
}

/// Editor is a Ring 3 SHM client, not `WindowContentType::Gpui(ViewKind::Editor)`.
pub fn editor_client_elf_data() -> Vec<u8> {
    shm_present_client_elf(0xFF1E_1E1E, b"GATE_F10 editor userspace\n")
}

/// Browser is a Ring 3 SHM client, not `WindowContentType::Gpui(ViewKind::Browser)`.
pub fn browser_client_elf_data() -> Vec<u8> {
    shm_present_client_elf(0xFF28_2D3A, b"GATE_F11 browser userspace\n")
}

/// AI Assistant is a Ring 3 SHM client, not `WindowContentType::Gpui(ViewKind::Chat)`.
pub fn ai_client_elf_data() -> Vec<u8> {
    shm_present_client_elf(0xFF12_1626, b"GATE_F12 ai userspace\n")
}

/// Gate F5: mmap SHM, `connect(/run/wayland-0)`, `sendto` the buffer vaddr.
pub fn wayland_unix_client_elf_data() -> Vec<u8> {
    let marker = b"GATE_F5 wayland unix\n";
    let mut code = Vec::new();
    let mut rels: Vec<(usize, &'static str)> = Vec::new();
    let mut labels = BTreeMap::new();

    let mut emit = |code: &mut Vec<u8>, bytes: &[u8]| code.extend_from_slice(bytes);
    let mut js = |code: &mut Vec<u8>, rels: &mut Vec<(usize, &'static str)>, name: &'static str| {
        emit(code, &[0x0F, 0x88]);
        rels.push((code.len(), name));
        emit(code, &[0, 0, 0, 0]);
    };
    let mut jne =
        |code: &mut Vec<u8>, rels: &mut Vec<(usize, &'static str)>, name: &'static str| {
            emit(code, &[0x0F, 0x85]);
            rels.push((code.len(), name));
            emit(code, &[0, 0, 0, 0]);
        };
    let mut lea_rsi =
        |code: &mut Vec<u8>, rels: &mut Vec<(usize, &'static str)>, name: &'static str| {
            emit(code, &[0x48, 0x8D, 0x35]);
            rels.push((code.len(), name));
            emit(code, &[0, 0, 0, 0]);
        };

    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x09, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0x31, 0xFF]);
    emit(&mut code, &[0x48, 0xC7, 0xC6, 0x00, 0x40, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0xC7, 0xC2, 0x03, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x49, 0xC7, 0xC2, 0x22, 0x80, 0x00, 0x00]);
    emit(&mut code, &[0x49, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]);
    emit(&mut code, &[0x4D, 0x31, 0xC9]);
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x85, 0xC0]);
    js(&mut code, &mut rels, "fail");
    emit(&mut code, &[0x49, 0x89, 0xC5]);
    emit(&mut code, &[0x4C, 0x89, 0xEF]);
    emit(&mut code, &[0x48, 0xC7, 0xC1, 0x00, 0x10, 0x00, 0x00]);
    emit(&mut code, &[0xB8, 0xEE, 0xFF, 0xC0, 0xFF]);
    emit(&mut code, &[0xF3, 0xAB]);

    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x29, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0xC7, 0xC6, 0x01, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0x31, 0xD2]);
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x85, 0xC0]);
    js(&mut code, &mut rels, "fail");
    emit(&mut code, &[0x49, 0x89, 0xC4]);

    emit(&mut code, &[0x48, 0x81, 0xEC, 0x80, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0x31, 0xC0]);
    emit(&mut code, &[0x48, 0x89, 0xE7]);
    emit(&mut code, &[0x48, 0xC7, 0xC1, 0x10, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0xF3, 0x48, 0xAB]);
    emit(&mut code, &[0x66, 0xC7, 0x04, 0x24, 0x01, 0x00]);
    emit(&mut code, &[0x48, 0x8D, 0x3C, 0x24]);
    emit(&mut code, &[0x48, 0x83, 0xC7, 0x02]);
    lea_rsi(&mut code, &mut rels, "path");
    emit(&mut code, &[0x48, 0xC7, 0xC1, 0x0F, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0xF3, 0xA4]);

    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x2A, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x4C, 0x89, 0xE7]);
    emit(&mut code, &[0x48, 0x89, 0xE6]);
    emit(&mut code, &[0x48, 0xC7, 0xC2, 0x6E, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x85, 0xC0]);
    js(&mut code, &mut rels, "fail");

    emit(&mut code, &[0x4C, 0x89, 0x2C, 0x24]);
    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x2C, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x4C, 0x89, 0xE7]);
    emit(&mut code, &[0x48, 0x89, 0xE6]);
    emit(&mut code, &[0x48, 0xC7, 0xC2, 0x08, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x4D, 0x31, 0xD2]);
    emit(&mut code, &[0x4D, 0x31, 0xC0]);
    emit(&mut code, &[0x4D, 0x31, 0xC9]);
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x83, 0xF8, 0x08]);
    jne(&mut code, &mut rels, "fail");

    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00]);
    lea_rsi(&mut code, &mut rels, "marker");
    let n = marker.len() as u32;
    emit(&mut code, &[0x48, 0xC7, 0xC2]);
    emit(&mut code, &n.to_le_bytes());
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x31, 0xFF]);
    emit(&mut code, &[0xEB, 0x07]);
    labels.insert("fail", code.len());
    emit(&mut code, &[0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x0F, 0x05]);
    labels.insert("path", code.len());
    emit(&mut code, b"/run/wayland-0\0");
    labels.insert("marker", code.len());
    emit(&mut code, marker);

    for (off, name) in rels {
        let target = *labels.get(name).expect(name);
        let disp = (target as i32).wrapping_sub((off as i32) + 4);
        code[off..off + 4].copy_from_slice(&disp.to_le_bytes());
    }
    build_static_user_elf(&code)
}

/// mmap window-sized SHM, write GPUI header, `connect(/run/wayland-0)`, send vaddr.
fn gpui_shm_unix_client_elf(magic: u32, width: u32, height: u32, marker: &[u8]) -> Vec<u8> {
    let bytes = (width as u64)
        .saturating_mul(height as u64)
        .saturating_mul(4);
    let mmap_len = ((bytes + 0xFFF) & !0xFFF).max(0x4000) as u32;
    let mut code = Vec::new();
    let mut rels: Vec<(usize, &'static str)> = Vec::new();
    let mut labels = BTreeMap::new();

    let mut emit = |code: &mut Vec<u8>, bytes: &[u8]| code.extend_from_slice(bytes);
    let mut js = |code: &mut Vec<u8>, rels: &mut Vec<(usize, &'static str)>, name: &'static str| {
        emit(code, &[0x0F, 0x88]);
        rels.push((code.len(), name));
        emit(code, &[0, 0, 0, 0]);
    };
    let mut jne =
        |code: &mut Vec<u8>, rels: &mut Vec<(usize, &'static str)>, name: &'static str| {
            emit(code, &[0x0F, 0x85]);
            rels.push((code.len(), name));
            emit(code, &[0, 0, 0, 0]);
        };
    let mut lea_rsi =
        |code: &mut Vec<u8>, rels: &mut Vec<(usize, &'static str)>, name: &'static str| {
            emit(code, &[0x48, 0x8D, 0x35]);
            rels.push((code.len(), name));
            emit(code, &[0, 0, 0, 0]);
        };

    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x09, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0x31, 0xFF]);
    emit(&mut code, &[0x48, 0xC7, 0xC6]);
    emit(&mut code, &mmap_len.to_le_bytes());
    emit(&mut code, &[0x48, 0xC7, 0xC2, 0x03, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x49, 0xC7, 0xC2, 0x22, 0x80, 0x00, 0x00]);
    emit(&mut code, &[0x49, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]);
    emit(&mut code, &[0x4D, 0x31, 0xC9]);
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x85, 0xC0]);
    js(&mut code, &mut rels, "fail");
    emit(&mut code, &[0x49, 0x89, 0xC5]);
    emit(&mut code, &[0x41, 0xC7, 0x45, 0x00]);
    emit(&mut code, &magic.to_le_bytes());
    emit(&mut code, &[0x41, 0xC7, 0x45, 0x04]);
    emit(&mut code, &width.to_le_bytes());
    emit(&mut code, &[0x41, 0xC7, 0x45, 0x08]);
    emit(&mut code, &height.to_le_bytes());

    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x29, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0xC7, 0xC6, 0x01, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0x31, 0xD2]);
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x85, 0xC0]);
    js(&mut code, &mut rels, "fail");
    emit(&mut code, &[0x49, 0x89, 0xC4]);

    emit(&mut code, &[0x48, 0x81, 0xEC, 0x80, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0x31, 0xC0]);
    emit(&mut code, &[0x48, 0x89, 0xE7]);
    emit(&mut code, &[0x48, 0xC7, 0xC1, 0x10, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0xF3, 0x48, 0xAB]);
    emit(&mut code, &[0x66, 0xC7, 0x04, 0x24, 0x01, 0x00]);
    emit(&mut code, &[0x48, 0x8D, 0x3C, 0x24]);
    emit(&mut code, &[0x48, 0x83, 0xC7, 0x02]);
    lea_rsi(&mut code, &mut rels, "path");
    emit(&mut code, &[0x48, 0xC7, 0xC1, 0x0F, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0xF3, 0xA4]);

    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x2A, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x4C, 0x89, 0xE7]);
    emit(&mut code, &[0x48, 0x89, 0xE6]);
    emit(&mut code, &[0x48, 0xC7, 0xC2, 0x6E, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x85, 0xC0]);
    js(&mut code, &mut rels, "fail");

    emit(&mut code, &[0x4C, 0x89, 0x2C, 0x24]);
    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x2C, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x4C, 0x89, 0xE7]);
    emit(&mut code, &[0x48, 0x89, 0xE6]);
    emit(&mut code, &[0x48, 0xC7, 0xC2, 0x08, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x4D, 0x31, 0xD2]);
    emit(&mut code, &[0x4D, 0x31, 0xC0]);
    emit(&mut code, &[0x4D, 0x31, 0xC9]);
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x83, 0xF8, 0x08]);
    jne(&mut code, &mut rels, "fail");

    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00]);
    lea_rsi(&mut code, &mut rels, "marker");
    let n = marker.len() as u32;
    emit(&mut code, &[0x48, 0xC7, 0xC2]);
    emit(&mut code, &n.to_le_bytes());
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x31, 0xFF]);
    emit(&mut code, &[0xEB, 0x07]);
    labels.insert("fail", code.len());
    emit(&mut code, &[0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x0F, 0x05]);
    labels.insert("path", code.len());
    emit(&mut code, b"/run/wayland-0\0");
    labels.insert("marker", code.len());
    emit(&mut code, marker);

    for (off, name) in rels {
        let target = *labels.get(name).expect(name);
        let disp = (target as i32).wrapping_sub((off as i32) + 4);
        code[off..off + 4].copy_from_slice(&disp.to_le_bytes());
    }
    build_static_user_elf(&code)
}

/// Gate WL: `wl_display` object protocol over `/run/wayland-0` (not a raw SHM vaddr).
pub fn wayland_protocol_client_elf_data() -> Vec<u8> {
    let marker = b"GATE_WL display protocol\n";
    let (proto, vaddr_off) = wayland_protocol_blob();
    let proto_len = proto.len() as u32;
    let mut code = Vec::new();
    let mut rels: Vec<(usize, &'static str)> = Vec::new();
    let mut labels = BTreeMap::new();

    let mut emit = |code: &mut Vec<u8>, bytes: &[u8]| code.extend_from_slice(bytes);
    let mut js = |code: &mut Vec<u8>, rels: &mut Vec<(usize, &'static str)>, name: &'static str| {
        emit(code, &[0x0F, 0x88]);
        rels.push((code.len(), name));
        emit(code, &[0, 0, 0, 0]);
    };
    let mut jne =
        |code: &mut Vec<u8>, rels: &mut Vec<(usize, &'static str)>, name: &'static str| {
            emit(code, &[0x0F, 0x85]);
            rels.push((code.len(), name));
            emit(code, &[0, 0, 0, 0]);
        };
    let mut lea_rsi =
        |code: &mut Vec<u8>, rels: &mut Vec<(usize, &'static str)>, name: &'static str| {
            emit(code, &[0x48, 0x8D, 0x35]);
            rels.push((code.len(), name));
            emit(code, &[0, 0, 0, 0]);
        };

    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x09, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0x31, 0xFF]);
    emit(&mut code, &[0x48, 0xC7, 0xC6, 0x00, 0x40, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0xC7, 0xC2, 0x03, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x49, 0xC7, 0xC2, 0x22, 0x80, 0x00, 0x00]);
    emit(&mut code, &[0x49, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]);
    emit(&mut code, &[0x4D, 0x31, 0xC9]);
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x85, 0xC0]);
    js(&mut code, &mut rels, "fail");
    emit(&mut code, &[0x49, 0x89, 0xC5]);
    emit(&mut code, &[0x4C, 0x89, 0xEF]);
    emit(&mut code, &[0x48, 0xC7, 0xC1, 0x00, 0x10, 0x00, 0x00]);
    emit(&mut code, &[0xB8, 0xEE, 0xFF, 0xC0, 0xFF]);
    emit(&mut code, &[0xF3, 0xAB]);

    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x29, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0xC7, 0xC6, 0x01, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0x31, 0xD2]);
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x85, 0xC0]);
    js(&mut code, &mut rels, "fail");
    emit(&mut code, &[0x49, 0x89, 0xC4]);

    emit(&mut code, &[0x48, 0x81, 0xEC, 0x00, 0x02, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0x31, 0xC0]);
    emit(&mut code, &[0x48, 0x89, 0xE7]);
    emit(&mut code, &[0x48, 0xC7, 0xC1, 0x40, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0xF3, 0x48, 0xAB]);
    emit(&mut code, &[0x66, 0xC7, 0x04, 0x24, 0x01, 0x00]);
    emit(&mut code, &[0x48, 0x8D, 0x3C, 0x24]);
    emit(&mut code, &[0x48, 0x83, 0xC7, 0x02]);
    lea_rsi(&mut code, &mut rels, "path");
    emit(&mut code, &[0x48, 0xC7, 0xC1, 0x0F, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0xF3, 0xA4]);

    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x2A, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x4C, 0x89, 0xE7]);
    emit(&mut code, &[0x48, 0x89, 0xE6]);
    emit(&mut code, &[0x48, 0xC7, 0xC2, 0x6E, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x85, 0xC0]);
    js(&mut code, &mut rels, "fail");

    // copy protocol blob onto the stack and patch the SHM vaddr
    lea_rsi(&mut code, &mut rels, "proto");
    emit(&mut code, &[0x48, 0x89, 0xE7]);
    emit(&mut code, &[0x48, 0xC7, 0xC1]);
    emit(&mut code, &proto_len.to_le_bytes());
    emit(&mut code, &[0xF3, 0xA4]);
    emit(&mut code, &[0x4C, 0x89, 0xAC, 0x24]); // mov [rsp+off], r13
    emit(&mut code, &(vaddr_off as u32).to_le_bytes());

    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x2C, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x4C, 0x89, 0xE7]);
    emit(&mut code, &[0x48, 0x89, 0xE6]);
    emit(&mut code, &[0x48, 0xC7, 0xC2]);
    emit(&mut code, &proto_len.to_le_bytes());
    emit(&mut code, &[0x4D, 0x31, 0xD2]);
    emit(&mut code, &[0x4D, 0x31, 0xC0]);
    emit(&mut code, &[0x4D, 0x31, 0xC9]);
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x3D]);
    emit(&mut code, &proto_len.to_le_bytes());
    jne(&mut code, &mut rels, "fail");

    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00]);
    lea_rsi(&mut code, &mut rels, "marker");
    let n = marker.len() as u32;
    emit(&mut code, &[0x48, 0xC7, 0xC2]);
    emit(&mut code, &n.to_le_bytes());
    emit(&mut code, &[0x0F, 0x05]);
    emit(&mut code, &[0x48, 0x31, 0xFF]);
    emit(&mut code, &[0xEB, 0x07]);
    labels.insert("fail", code.len());
    emit(&mut code, &[0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00]);
    emit(&mut code, &[0x0F, 0x05]);
    labels.insert("path", code.len());
    emit(&mut code, b"/run/wayland-0\0");
    labels.insert("marker", code.len());
    emit(&mut code, marker);
    labels.insert("proto", code.len());
    emit(&mut code, &proto);

    for (off, name) in rels {
        let target = *labels.get(name).expect(name);
        let disp = (target as i32).wrapping_sub((off as i32) + 4);
        code[off..off + 4].copy_from_slice(&disp.to_le_bytes());
    }
    build_static_user_elf(&code)
}

fn wayland_protocol_blob() -> (Vec<u8>, usize) {
    fn hdr(buf: &mut Vec<u8>, object: u32, opcode: u16, size: u16) {
        buf.extend_from_slice(&object.to_le_bytes());
        buf.extend_from_slice(&opcode.to_le_bytes());
        buf.extend_from_slice(&size.to_le_bytes());
    }
    fn put_str(buf: &mut Vec<u8>, s: &str) {
        let n = (s.len() + 1) as u32;
        buf.extend_from_slice(&n.to_le_bytes());
        buf.extend_from_slice(s.as_bytes());
        buf.push(0);
        while buf.len() % 4 != 0 {
            buf.push(0);
        }
    }
    let mut buf = Vec::new();
    // wl_display.get_registry(new_id=2)
    hdr(&mut buf, 1, 1, 12);
    buf.extend_from_slice(&2u32.to_le_bytes());
    // wl_registry.bind(name=1, "wl_compositor", ver=4, new_id=3)
    let mut args = Vec::new();
    args.extend_from_slice(&1u32.to_le_bytes());
    put_str(&mut args, "wl_compositor");
    args.extend_from_slice(&4u32.to_le_bytes());
    args.extend_from_slice(&3u32.to_le_bytes());
    hdr(&mut buf, 2, 0, (8 + args.len()) as u16);
    buf.extend_from_slice(&args);
    // wl_registry.bind(name=2, "wl_shm", ver=1, new_id=4)
    let mut args = Vec::new();
    args.extend_from_slice(&2u32.to_le_bytes());
    put_str(&mut args, "wl_shm");
    args.extend_from_slice(&1u32.to_le_bytes());
    args.extend_from_slice(&4u32.to_le_bytes());
    hdr(&mut buf, 2, 0, (8 + args.len()) as u16);
    buf.extend_from_slice(&args);
    // wl_shm.create_pool(new_id=5, size=0x4000, vaddr)
    hdr(&mut buf, 4, 0, 24);
    buf.extend_from_slice(&5u32.to_le_bytes());
    buf.extend_from_slice(&0x4000u32.to_le_bytes());
    let vaddr_off = buf.len();
    buf.extend_from_slice(&0u64.to_le_bytes());
    // wl_shm_pool.create_buffer(new_id=6, offset=0, 64, 64, 256, format=0)
    hdr(&mut buf, 5, 0, 32);
    buf.extend_from_slice(&6u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&64u32.to_le_bytes());
    buf.extend_from_slice(&64u32.to_le_bytes());
    buf.extend_from_slice(&256u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    // wl_compositor.create_surface(new_id=7)
    hdr(&mut buf, 3, 0, 12);
    buf.extend_from_slice(&7u32.to_le_bytes());
    // wl_surface.attach(buffer=6, x=0, y=0)
    hdr(&mut buf, 7, 1, 20);
    buf.extend_from_slice(&6u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    // wl_surface.commit()
    hdr(&mut buf, 7, 6, 8);
    (buf, vaddr_off)
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
