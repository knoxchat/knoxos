use crate::serial_println;

use super::{reap_child, run_until_desktop, spawn_or_log};

pub(super) fn run_gate_f1() {
    serial_println!("[user_task] Gate F1: Ring 3 SHM client present");
    let elf = crate::init::display_client_elf_data();
    let Some(pid) = spawn_or_log(&elf, "wl-client") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!(
        "[user_task] Gate F1 pid={} reaped={} (marker from userspace)",
        pid,
        reaped
    );
}

pub(super) fn run_gate_f3() {
    serial_println!("[user_task] Gate F3: Ring 3 terminal SHM client");
    let elf = crate::init::terminal_client_elf_data();
    let Some(pid) = spawn_or_log(&elf, "term") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!(
        "[user_task] Gate F3 pid={} reaped={} (marker from userspace)",
        pid,
        reaped
    );
}

pub(super) fn run_gate_f4() {
    serial_println!("[user_task] Gate F4: Ring 3 launcher SHM client");
    let elf = crate::init::launcher_client_elf_data();
    let Some(pid) = spawn_or_log(&elf, "paint") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!(
        "[user_task] Gate F4 pid={} reaped={} (marker from userspace)",
        pid,
        reaped
    );
}

pub(super) fn run_gate_f6() {
    serial_println!("[user_task] Gate F6: Ring 3 calculator SHM client");
    let elf = crate::init::calculator_client_elf_data();
    let Some(pid) = spawn_or_log(&elf, "calc") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!(
        "[user_task] Gate F6 pid={} reaped={} (marker from userspace)",
        pid,
        reaped
    );
}

pub(super) fn run_gate_f7() {
    serial_println!("[user_task] Gate F7: Ring 3 Files SHM client");
    let elf = crate::init::files_client_elf_data();
    let Some(pid) = spawn_or_log(&elf, "files") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!(
        "[user_task] Gate F7 pid={} reaped={} (marker from userspace)",
        pid,
        reaped
    );
}

pub(super) fn run_gate_gpui_ring3() {
    serial_println!("[user_task] D15: Ring 3 Knox GPUI SHM at window size");
    let elf = crate::init::gpui_button_client_elf_data();
    if let Some(pid) = spawn_or_log(&elf, "gpui-btn") {
        // SAFETY: live Ring 3 context; switch until the desktop is current.
        unsafe {
            run_until_desktop(pid);
        }
        let _ = reap_child(pid);
    }
    let elf = crate::init::gpui_calc_client_elf_data();
    if let Some(pid) = spawn_or_log(&elf, "gpui-calc") {
        // SAFETY: live Ring 3 context; switch until the desktop is current.
        unsafe {
            run_until_desktop(pid);
        }
        let _ = reap_child(pid);
    }
    let _ = crate::wayland::ring3_calc_key_self_test();
}

pub(super) fn spawn_parked_server(elf: &[u8], name: &str) -> bool {
    let Some(pid) = spawn_or_log(elf, name) else {
        return false;
    };
    // SAFETY: live Ring 3 context; switch until the desktop is current.
    unsafe {
        run_until_desktop(pid);
    }
    crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .map(|p| p.state != crate::process::ProcessState::Zombie)
        .unwrap_or(false)
}

pub(super) fn run_gate_wayland_server() {
    serial_println!("[user_task] Ring 3 Wayland listen on /run/wayland-0");
    crate::vfs::ensure_directory("/run");
    let elf = crate::init::wayland_server_elf_data();
    if spawn_parked_server(&elf, "wayland-server") {
        serial_println!(
            "[user_task] {} listening={}",
            super::GATE_WL_SERVER_USERSPACE,
            crate::net::unix_path_is_listening(crate::wayland::WAYLAND_SOCKET_PATH)
        );
    } else {
        serial_println!("[user_task] Gate WL server FAILED: process did not stay parked");
    }
}

pub(super) fn run_gate_dbus_daemon() {
    serial_println!("[user_task] Ring 3 D-Bus listen on /run/dbus/system_bus_socket");
    crate::vfs::ensure_directory("/run/dbus");
    let elf = crate::init::dbus_daemon_elf_data();
    if spawn_parked_server(&elf, "dbus-daemon") {
        serial_println!(
            "[user_task] {} listening={}",
            super::GATE_DBUS_DAEMON_USERSPACE,
            crate::net::unix_path_is_listening(crate::dbus::SYSTEM_BUS_PATH)
        );
    } else {
        serial_println!("[user_task] Gate DBUS daemon FAILED: process did not stay parked");
    }
}

pub(super) fn run_gate_term() {
    serial_println!("[user_task] Gate TERM: Ring 3 openpty + SHM terminal");
    let elf = crate::init::pty_terminal_client_elf_data();
    let Some(pid) = spawn_or_log(&elf, "term-pty") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!(
        "[user_task] Gate TERM pid={} reaped={} (marker from userspace)",
        pid,
        reaped
    );
}

pub(super) fn run_gate_f5() {
    serial_println!("[user_task] Gate F5: Wayland AF_UNIX present");
    let elf = crate::init::wayland_unix_client_elf_data();
    let Some(pid) = spawn_or_log(&elf, "wl-unix") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!(
        "[user_task] Gate F5 pid={} reaped={} (marker from userspace)",
        pid,
        reaped
    );
}

pub(super) fn run_gate_wl() {
    serial_println!("[user_task] Gate WL: wl_display object protocol");
    let elf = crate::init::wayland_protocol_client_elf_data();
    let Some(pid) = spawn_or_log(&elf, "wl-proto") else {
        return;
    };
    // SAFETY: `pid` was spawned with a live Ring 3 context; we only context-switch until the desktop is current again.
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!(
        "[user_task] Gate WL pid={} reaped={} (marker from userspace)",
        pid,
        reaped
    );
}
