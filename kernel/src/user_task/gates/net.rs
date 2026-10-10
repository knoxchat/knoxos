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

pub(super) fn run_gate_bg2() {
    serial_println!("[user_task] Gate BG2: accept4 listen/connect");
    let elf = crate::init::accept4_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "accept4-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BG2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bh2() {
    serial_println!("[user_task] Gate BH2: getsockname after bind");
    let elf = crate::init::getsockname_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getsockname-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BH2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bh3() {
    serial_println!("[user_task] Gate BH3: getpeername after connect");
    let elf = crate::init::getpeername_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getpeername-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BH3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bi1() {
    serial_println!("[user_task] Gate BI1: sendmsg UDP byte");
    let elf = crate::init::sendmsg_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "sendmsg-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BI1 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bi2() {
    serial_println!("[user_task] Gate BI2: recvmsg UDP byte");
    let elf = crate::init::recvmsg_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "recvmsg-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BI2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bi3() {
    serial_println!("[user_task] Gate BI3: shutdown SHUT_RDWR");
    let elf = crate::init::shutdown_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "shutdown-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BI3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bj1() {
    serial_println!("[user_task] Gate BJ1: sendmmsg UDP");
    let elf = crate::init::sendmmsg_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "sendmmsg-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BJ1 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bj2() {
    serial_println!("[user_task] Gate BJ2: recvmmsg UDP");
    let elf = crate::init::recvmmsg_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "recvmmsg-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BJ2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bj3() {
    serial_println!("[user_task] Gate BJ3: getsockopt SO_TYPE");
    let elf = crate::init::getsockopt_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getsockopt-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BJ3 parent pid={} reaped={}", pid, reaped);
}
