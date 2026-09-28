use crate::serial_println;

use super::markers::*;
use super::{reap_child, run_until_desktop, spawn_or_log};

pub(super) fn run_gate_d1() {
    serial_println!("[user_task] Gate D1: loopback sockets send/recv");
    let kernel_ok = crate::net::loopback_self_test();
    if !kernel_ok {
        serial_println!("[user_task] Gate D1 FAILED: kernel loopback self-test");
        return;
    }

    let elf = crate::init::loopback_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "loopback") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    if reap_child(pid) {
        serial_println!("[user_task] {} (pid={})", GATE_D1_MARKER, pid);
    } else {
        serial_println!(
            "[user_task] Gate D1 FAILED: pid {} still present after yield",
            pid
        );
    }
}
