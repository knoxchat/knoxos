use crate::serial_println;
use crate::user_task::lifecycle::terminate;

use super::markers::*;
use super::{reap_child, run_until_desktop, spawn_or_log};

pub(super) fn run_gate_b3() {
    serial_println!("[user_task] Gate B3: execve(/bin/hello) + waitpid");
    let elf = crate::init::exec_hello_elf_data();
    let Some(pid) = spawn_or_log(&elf, "init-hello") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    if reap_child(pid) {
        serial_println!("[user_task] {} (pid={})", GATE_B3_MARKER, pid);
    } else {
        serial_println!(
            "[user_task] Gate B3 FAILED: pid {} still present after yield",
            pid
        );
    }
}

pub(super) fn run_gate_b4() {
    serial_println!("[user_task] Gate B4: fork + child runs + wait4");
    let elf = crate::init::fork_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "fork-demo") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate B4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_b5() {
    serial_println!("[user_task] Gate B5: SIGKILL, SIGSEGV, PTY SIGINT");
    let mut ok_kill = false;
    let mut ok_segv = false;
    let mut ok_int = false;

    // SIGKILL a parked pause() task.
    let pause = crate::init::pause_userspace_elf_data();
    if let Some(pid) = spawn_or_log(&pause, "pause-kill") {
        // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
        unsafe {
            run_until_desktop(pid);
        }
        let _ = crate::signals::kill(pid, crate::signals::Signal::SIGKILL, 0);
        ok_kill = reap_child(pid);
        serial_println!("[user_task] Gate B5 SIGKILL pid={} reaped={}", pid, ok_kill);
    }

    // SIGSEGV: load a null pointer in Ring 3.
    let boom = crate::init::segfault_userspace_elf_data();
    if let Some(pid) = spawn_or_log(&boom, "segfault") {
        // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
        unsafe {
            run_until_desktop(pid);
        }
        ok_segv = reap_child(pid);
        serial_println!("[user_task] Gate B5 SIGSEGV pid={} reaped={}", pid, ok_segv);
    }

    // SIGINT from PTY Ctrl+C to the foreground process group.
    if let Some(pid) = spawn_or_log(&pause, "pause-int") {
        let _ = crate::pgrp::setpgid(pid, pid);
        if let Ok((pty, _)) = crate::pty::openpty() {
            let _ = crate::pty::open_slave(pty);
            let _ = crate::pty::pty_ioctl(pty, crate::pty::TIOCSPGRP, pid as u64);
            // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
            unsafe {
                run_until_desktop(pid);
            }
            let _ = crate::pty::write_master(pty, &[0x03]);
            crate::signals::deliver_signals(pid);
            ok_int = reap_child(pid);
            serial_println!("[user_task] Gate B5 SIGINT pid={} reaped={}", pid, ok_int);
        }
    }

    if ok_kill && ok_segv && ok_int {
        serial_println!("[user_task] {}", GATE_B5_MARKER);
    } else {
        serial_println!(
            "[user_task] Gate B5 partial: kill={} segv={} int={}",
            ok_kill,
            ok_segv,
            ok_int
        );
    }
}

pub(super) fn run_gate_b6() {
    serial_println!("[user_task] Gate B6: /bin/sh on a PTY");
    let elf = {
        let vfs = crate::vfs::VFS.lock();
        match vfs.read_file("/bin/sh") {
            Some(data) => data.to_vec(),
            None => {
                serial_println!("[user_task] Gate B6 FAILED: /bin/sh missing");
                return;
            }
        }
    };
    let Some(pid) = spawn_or_log(&elf, "sh") else {
        return;
    };
    let _ = crate::pgrp::setpgid(pid, pid);
    let Ok((pty, _)) = crate::pty::openpty() else {
        serial_println!("[user_task] Gate B6 FAILED: openpty");
        return;
    };
    let _ = crate::pty::open_slave(pty);
    let _ = crate::pty::pty_ioctl(pty, crate::pty::TIOCSPGRP, pid as u64);
    crate::fd::attach_pty_stdio(pid, pty);
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let mut prompt = [0u8; 8];
    let n = crate::pty::read_master(pty, &mut prompt).unwrap_or(0);
    let saw_prompt = n >= 2 && &prompt[..2] == b"$ ";
    let reaped = reap_child(pid);
    if reaped && saw_prompt {
        serial_println!("[user_task] {} (pid={} pty={})", GATE_B6_MARKER, pid, pty);
    } else {
        serial_println!(
            "[user_task] Gate B6 partial: pid={} reaped={} prompt={} n={}",
            pid,
            reaped,
            saw_prompt,
            n
        );
    }
}

pub(super) fn run_gate_sh_parser() {
    serial_println!("[user_task] Ring 3 /bin/sh parser via ShellExecute");
    let elf = {
        let vfs = crate::vfs::VFS.lock();
        match vfs.read_file("/bin/sh") {
            Some(data) => data.to_vec(),
            None => {
                serial_println!("[user_task] Gate SH parser FAILED: /bin/sh missing");
                return;
            }
        }
    };
    let Some(pid) = spawn_or_log(&elf, "sh-parser") else {
        return;
    };
    let _ = crate::pgrp::setpgid(pid, pid);
    let Ok((pty, _)) = crate::pty::openpty() else {
        serial_println!("[user_task] Gate SH parser FAILED: openpty");
        return;
    };
    let _ = crate::pty::open_slave(pty);
    let _ = crate::pty::pty_ioctl(pty, crate::pty::TIOCSPGRP, pid as u64);
    crate::fd::attach_pty_stdio_rw(pid, pty);
    let cmd = b"echo GATE_SH ring3 parser\n";
    let _ = crate::pty::write_master(pty, cmd);
    // SAFETY: live Ring 3 context; switch until the desktop is current.
    unsafe {
        run_until_desktop(pid);
    }
    let mut out = [0u8; 256];
    let n = crate::pty::read_master(pty, &mut out).unwrap_or(0);
    let text = core::str::from_utf8(&out[..n]).unwrap_or("");
    // PTY echo of the typed line also contains the marker string; require the
    // builtin output after that echo so this is ShellExecute, not just ECHO.
    let saw = text
        .split_once("echo GATE_SH ring3 parser")
        .is_some_and(|(_, rest)| rest.contains(GATE_SH_RING3_PARSER));
    let _ = reap_child(pid);
    if saw {
        serial_println!(
            "[user_task] {} (pid={} pty={})",
            GATE_SH_RING3_PARSER,
            pid,
            pty
        );
    } else {
        serial_println!(
            "[user_task] Gate SH parser FAILED: pid={} n={} out={:?}",
            pid,
            n,
            text
        );
    }
}

pub(super) fn run_gate_j1() {
    serial_println!("[user_task] Gate J1: clone(CLONE_VM) child shares CR3");
    let elf = crate::init::clone_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "clone-demo") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate J1 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_k1() {
    serial_println!("[user_task] Gate K1: clone(CLONE_THREAD) + thread_join");
    let elf = crate::init::clone_thread_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "clone-thread") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate K1 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bl2() {
    serial_println!("[user_task] Gate BL2: pidfd_open of self");
    let elf = crate::init::pidfd_open_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "pidfd-open-demo") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BL2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bl3() {
    serial_println!("[user_task] Gate BL3: clone3 child then wait4");
    let elf = crate::init::clone3_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "clone3-demo") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BL3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bm1() {
    serial_println!("[user_task] Gate BM1: userspace ld.so + DT_NEEDED");
    crate::init::install_dynlink_files();
    let elf = crate::init::dyn_user_elf_data();
    let Some(pid) = spawn_or_log(&elf, "gate-dyn") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BM1 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bm2() {
    serial_println!("[user_task] Gate BM2: dlopen via VMM mmap");
    crate::init::install_dynlink_files();
    let elf = crate::init::dlopen_user_elf_data();
    let Some(pid) = spawn_or_log(&elf, "gate-dlopen") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BM2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bm3() {
    serial_println!("[user_task] Gate BM3: static musl CRT ABI");
    crate::init::install_dynlink_files();
    let elf = crate::init::musl_static_user_elf_data();
    let Some(pid) = spawn_or_log(&elf, "gate-musl") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BM3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bm4() {
    serial_println!("[user_task] Gate BM4: dynamically linked musl CRT");
    crate::init::install_dynlink_files();
    let elf = crate::init::musl_dyn_user_elf_data();
    let Some(pid) = spawn_or_log(&elf, "gate-musl-dyn") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BM4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_chr() {
    serial_println!("[user_task] Chromium slice 1: unpack + glibc exec");
    crate::init::install_chromium_files();
    if !crate::init::unpack_self_test() {
        serial_println!("[user_task] GATE_CHR unpack FAILED");
        return;
    }
    let elf = {
        let vfs = crate::vfs::VFS.lock();
        match vfs.read_file(crate::init::CHROME_PATH) {
            Some(data) => data.to_vec(),
            None => crate::init::chrome_elf_data(),
        }
    };
    let Some(pid) = spawn_or_log(&elf, "chrome") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate CHR parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_chr_fork() {
    serial_println!("[user_task] Chromium slice 2: zygote fork+execve");
    crate::init::install_chromium_files();
    if !crate::init::fork_self_test() {
        serial_println!("[user_task] Chromium zygote fork FAILED");
        return;
    }
    let elf = crate::init::zygote_parent_elf_data();
    let Some(pid) = spawn_or_log(&elf, "chrome-zygote-parent") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!(
        "[user_task] Gate CHR fork parent pid={} reaped={}",
        pid,
        reaped
    );
}

pub(super) fn run_gate_chr_linux_shape() {
    serial_println!("[user_task] Chromium slice 3: /proc /dev /etc /XDG");
    crate::init::install_chromium_linux_shape();
    if !crate::init::linux_shape_self_test() {
        serial_println!("[user_task] Chromium linux-shape FAILED");
        return;
    }
    let elf = crate::init::linux_shape_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "chrome-linux-shape") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!(
        "[user_task] Gate CHR linux-shape pid={} reaped={}",
        pid,
        reaped
    );
}

pub(super) fn run_gate_chr_ozone() {
    serial_println!("[user_task] Chromium slice 3: Ozone/Wayland SHM");
    crate::init::install_chromium_files();
    if !crate::init::ozone_self_test() {
        serial_println!("[user_task] Chromium ozone FAILED");
        return;
    }
    let elf = crate::init::ozone_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "chrome-ozone") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate CHR ozone pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_chr_https() {
    serial_println!("[user_task] Chromium slice 4: userspace HTTPS");
    crate::init::install_chromium_files();
    if !crate::init::https_self_test() {
        serial_println!("[user_task] Chromium https FAILED");
        return;
    }
    let elf = crate::init::https_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "chrome-https") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate CHR https pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_chr_gl() {
    serial_println!("[user_task] Chromium slice 5: software GL / chrome://gpu");
    crate::init::install_chromium_files();
    if !crate::init::gl_self_test() {
        serial_println!("[user_task] Chromium gl FAILED");
        return;
    }
    let elf = crate::init::gl_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "chrome-gl") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate CHR gl pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_chr_media() {
    serial_println!("[user_task] Chromium slice 6: fonts/ICU/media");
    crate::init::install_chromium_files();
    if !crate::init::media_self_test() {
        serial_println!("[user_task] Chromium media FAILED");
        return;
    }
    let elf = crate::init::media_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "chrome-media") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate CHR media pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_chr_desktop() {
    serial_println!("[user_task] Chromium slice 7: desktop launch");
    crate::init::install_chromium_files();
    if !crate::init::desktop_self_test() {
        serial_println!("[user_task] Chromium desktop FAILED");
        return;
    }
    let elf = crate::init::desktop_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "chrome-desktop") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate CHR desktop pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_chr_sandbox() {
    serial_println!("[user_task] Chromium slice 8: sandbox");
    crate::init::install_chromium_files();
    if !crate::init::sandbox_self_test() {
        serial_println!("[user_task] Chromium sandbox FAILED");
        return;
    }
    let elf = crate::init::sandbox_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "chrome-sandbox") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate CHR sandbox pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_chr_loader() {
    serial_println!("[user_task] Chromium leftover: TLS + DT_NEEDED + skip-GTK + ldd");
    crate::init::install_chromium_files();
    if !crate::init::loader_self_test() {
        serial_println!("[user_task] Chromium loader FAILED");
        return;
    }
    let elf = crate::init::glibc_tls_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "chrome-tls") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate CHR tls pid={} reaped={}", pid, reaped);
    let elf = crate::init::needed_userspace_elf_data();
    if let Some(pid) = spawn_or_log(&elf, "chrome-needed") {
        // SAFETY: live Ring 3 context; switch until the desktop is current.
        unsafe {
            run_until_desktop(pid);
        }
        let _ = reap_child(pid);
    }
}

pub(super) fn run_gate_chr_mojo() {
    serial_println!("[user_task] Chromium leftover: Mojo without --single-process");
    crate::init::install_chromium_files();
    if !crate::init::mojo_self_test() {
        serial_println!("[user_task] Chromium mojo FAILED");
        return;
    }
    let elf = crate::init::mojo_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "chrome-mojo") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate CHR mojo pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_viv() {
    serial_println!("[user_task] Chromium slice 9: Vivaldi .deb");
    crate::init::install_chromium_files();
    if !crate::init::vivaldi_self_test() {
        serial_println!("[user_task] Vivaldi slice FAILED");
        return;
    }
    let elf = crate::init::vivaldi_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "vivaldi") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate VIV pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_chr_iouring() {
    serial_println!("[user_task] Chromium leftover: Ring 3 io_uring_setup");
    if !crate::init::iouring_self_test() {
        serial_println!("[user_task] Chromium iouring FAILED");
        return;
    }
    let elf = crate::init::iouring_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "chrome-iouring") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate CHR iouring pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_chr_polish() {
    serial_println!("[user_task] Chromium slice 10: polish");
    crate::init::install_chromium_files();
    if !crate::init::polish_self_test() {
        serial_println!("[user_task] Chromium polish FAILED");
        return;
    }
    let elf = crate::init::polish_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "chrome-polish") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate CHR polish pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_musl_ui() {
    serial_println!("[user_task] musl CRT calls KnoxUiRaster");
    crate::init::install_dynlink_files();
    let elf = crate::init::musl_ui_user_elf_data();
    let Some(pid) = spawn_or_log(&elf, "gate-musl-ui") else {
        return;
    };
    // SAFETY: live Ring 3 context; switch until the desktop is current.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate musl UI pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_k3() {
    serial_println!("[user_task] Gate K3: /sbin/init in Ring 3");
    let elf = {
        let vfs = crate::vfs::VFS.lock();
        match vfs.read_file("/sbin/init") {
            Some(data) => data.to_vec(),
            None => crate::init::builtin_init_elf_data(),
        }
    };
    let Some(pid) = spawn_or_log(&elf, "init") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let alive = crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .map(|p| p.state != crate::process::ProcessState::Zombie)
        .unwrap_or(false);
    if alive {
        serial_println!("[user_task] {} (pid={})", GATE_K3_MARKER, pid);
        terminate(pid, 0);
        let _ = reap_child(pid);
    } else {
        serial_println!(
            "[user_task] Gate K3 FAILED: /sbin/init pid={} did not stay parked",
            pid
        );
        let _ = reap_child(pid);
    }
}

pub(super) fn run_gate_l1() {
    serial_println!("[user_task] Gate L1: arch_prctl ARCH_SET_FS + %fs:0");
    let elf = crate::init::tls_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "tls-fs") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate L1 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_b7() {
    serial_println!("[user_task] Gate B7: SIGINT handler + rt_sigreturn");
    let elf = crate::init::sigreturn_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "sigreturn") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let _ = crate::signals::kill(pid, crate::signals::Signal::SIGINT, 0);
    crate::signals::deliver_signals(pid);
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!(
        "[user_task] Gate B7 pid={} reaped={} (marker from userspace)",
        pid,
        reaped
    );
}

pub(super) fn run_gate_b8() {
    serial_println!("[user_task] Gate B8: timer preempt spinning Ring 3");
    let spin_elf = crate::init::spin_userspace_elf_data();
    let writer_elf = crate::init::preempt_writer_elf_data();
    let Some(spinner) = spawn_or_log(&spin_elf, "spin") else {
        return;
    };
    let Some(writer) = spawn_or_log(&writer_elf, "preempt-w") else {
        terminate(spinner, -(crate::signals::Signal::SIGKILL as i32));
        let _ = reap_child(spinner);
        return;
    };

    serial_println!(
        "[user_task] Gate B8: switching to spinner pid={} (writer={}) ticks={}",
        spinner,
        writer,
        crate::apic_timer::total_ticks()
    );
    let before = crate::scheduler::user_preempt_count();
    // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
    unsafe {
        crate::context::switch_to(spinner);
    }
    serial_println!(
        "[user_task] Gate B8: returned from spinner ticks={}",
        crate::apic_timer::total_ticks()
    );
    let preempts = crate::scheduler::user_preempt_count().saturating_sub(before);

    let snap = crate::context::snapshot(spinner);
    let gpr_ok = snap
        .as_ref()
        .map(|c| {
            c.rbx == crate::init::SPIN_RBX_MAGIC
                && c.fpu_initialized
                && c.fxsave_area.iter().any(|&b| b != 0)
        })
        .unwrap_or(false);
    if gpr_ok {
        serial_println!(
            "[user_task] {} (rbx={:#x} fxsave)",
            GATE_I2_MARKER,
            crate::init::SPIN_RBX_MAGIC
        );
    } else {
        let rbx = snap.map(|c| c.rbx).unwrap_or(0);
        serial_println!(
            "[user_task] Gate I2 FAILED: spinner rbx={:#x} want={:#x}",
            rbx,
            crate::init::SPIN_RBX_MAGIC
        );
    }

    // Spinner never exits; kill it so it cannot steal the CPU again.
    terminate(spinner, -(crate::signals::Signal::SIGKILL as i32));
    if crate::context::has_runnable_context(writer) {
        // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
        unsafe {
            crate::context::switch_to(writer);
        }
    }
    let writer_reaped = reap_child(writer);
    let spinner_reaped = reap_child(spinner);
    if preempts > 0 {
        serial_println!(
            "[user_task] {} (preempts={} writer_reaped={} spinner_reaped={})",
            GATE_B8_MARKER,
            preempts,
            writer_reaped,
            spinner_reaped
        );
    } else {
        serial_println!(
            "[user_task] Gate B8 FAILED: no timer preempt (writer_reaped={})",
            writer_reaped
        );
    }
}

pub(super) fn run_gate_i3() {
    serial_println!("[user_task] Gate I3: AP runs Ring 3");
    if crate::smp::online_cpus() < 2 {
        serial_println!("[user_task] Gate I3 skipped: only one CPU online");
        return;
    }

    crate::smp::clear_last_user_cpu();
    let elf = crate::init::ap_ring3_elf_data();
    let Some(pid) = spawn_or_log(&elf, "ap-ring3") else {
        return;
    };
    if crate::scheduler::set_cpu_affinity(pid, 1 << 1).is_err() {
        serial_println!("[user_task] Gate I3 FAILED: affinity pin");
        terminate(pid, -(crate::signals::Signal::SIGKILL as i32));
        let _ = reap_child(pid);
        return;
    }
    crate::smp::enqueue_on_cpu(1, pid);

    let start = crate::apic_timer::total_ticks();
    loop {
        let cpu = crate::smp::last_user_cpu();
        let idle = crate::smp::ap_in_idle(1);
        let zombie = crate::process::PROCESS_TABLE
            .lock()
            .get_process(pid)
            .is_none_or(|p| p.state == crate::process::ProcessState::Zombie);
        if cpu == 1 && idle && zombie {
            break;
        }
        if crate::apic_timer::total_ticks().saturating_sub(start) > 2000 {
            serial_println!(
                "[user_task] Gate I3 timeout: last_cpu={} idle={} zombie={}",
                cpu,
                idle,
                zombie
            );
            break;
        }
        core::hint::spin_loop();
    }

    let cpu = crate::smp::last_user_cpu();
    let reaped = reap_child(pid);
    if cpu == 1 && reaped {
        serial_println!(
            "[user_task] {} (pid={} cpu={} last_pid={})",
            GATE_I3_MARKER,
            pid,
            cpu,
            crate::smp::last_user_pid()
        );
    } else {
        serial_println!(
            "[user_task] Gate I3 FAILED: pid={} reaped={} cpu={} (want 1)",
            pid,
            reaped,
            cpu
        );
        if crate::context::has_runnable_context(pid) {
            terminate(pid, -(crate::signals::Signal::SIGKILL as i32));
            let _ = reap_child(pid);
        }
    }
}

pub(super) fn run_gate_i4() {
    serial_println!("[user_task] Gate I4: SMP load balance onto idle AP");
    if crate::smp::online_cpus() < 2 {
        serial_println!("[user_task] Gate I4 skipped: only one CPU online");
        return;
    }

    crate::smp::clear_last_user_cpu();
    let elf = crate::init::ap_ring3_elf_data();
    let Some(pid) = spawn_or_log(&elf, "smp-balance") else {
        return;
    };
    crate::scheduler::set_placed_cpu(pid, 1);
    crate::smp::enqueue_on_cpu(1, pid);

    let start = crate::apic_timer::total_ticks();
    loop {
        let cpu = crate::smp::last_user_cpu();
        let idle = crate::smp::ap_in_idle(1);
        let zombie = crate::process::PROCESS_TABLE
            .lock()
            .get_process(pid)
            .is_none_or(|p| p.state == crate::process::ProcessState::Zombie);
        if cpu == 1 && idle && zombie {
            break;
        }
        if crate::apic_timer::total_ticks().saturating_sub(start) > 2000 {
            serial_println!(
                "[user_task] Gate I4 timeout: last_cpu={} idle={} zombie={}",
                cpu,
                idle,
                zombie
            );
            break;
        }
        core::hint::spin_loop();
    }

    let cpu = crate::smp::last_user_cpu();
    let reaped = reap_child(pid);
    if cpu == 1 && reaped {
        serial_println!(
            "[user_task] {} (pid={} cpu={} last_pid={})",
            GATE_I4_MARKER,
            pid,
            cpu,
            crate::smp::last_user_pid()
        );
    } else {
        serial_println!(
            "[user_task] Gate I4 FAILED: pid={} reaped={} cpu={} (want 1)",
            pid,
            reaped,
            cpu
        );
        if crate::context::has_runnable_context(pid) {
            terminate(pid, -(crate::signals::Signal::SIGKILL as i32));
            let _ = reap_child(pid);
        }
    }
}
