use crate::serial_println;

use super::{reap_child, run_until_desktop, spawn_or_log};

pub(super) fn run_gate_f1() {
    serial_println!("[user_task] Gate F1: Ring 3 SHM client present");
    let elf = crate::init::display_client_elf_data();
    let Some(pid) = spawn_or_log(&elf, "wl-client") else {
        return;
    };
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
