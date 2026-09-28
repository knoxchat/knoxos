use crate::serial_println;

use super::{reap_child, run_until_desktop, spawn_or_log};

pub(super) fn run_gate_m2() {
    serial_println!("[user_task] Gate M2: pipe() write/read round-trip");
    let elf = crate::init::pipe_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "pipe-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate M2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_m3() {
    serial_println!("[user_task] Gate M3: futex wait/wake");
    let elf = crate::init::futex_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "futex-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate M3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_n2() {
    serial_println!("[user_task] Gate N2: socketpair write/read round-trip");
    let elf = crate::init::socketpair_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "socketpair-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate N2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_n3() {
    serial_println!("[user_task] Gate N3: eventfd write/read");
    let elf = crate::init::eventfd_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "eventfd-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate N3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_o2() {
    serial_println!("[user_task] Gate O2: epoll wait on a pipe");
    let elf = crate::init::epoll_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "epoll-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate O2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_o3() {
    serial_println!("[user_task] Gate O3: memfd write/read");
    let elf = crate::init::memfd_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "memfd-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate O3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_p2() {
    serial_println!("[user_task] Gate P2: timerfd expire/read");
    let elf = crate::init::timerfd_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "timerfd-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate P2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_p3() {
    serial_println!("[user_task] Gate P3: signalfd SIGUSR1");
    let elf = crate::init::signalfd_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "signalfd-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate P3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_q2() {
    serial_println!("[user_task] Gate Q2: poll on a pipe");
    let elf = crate::init::poll_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "poll-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate Q2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_q3() {
    serial_println!("[user_task] Gate Q3: inotify_init1 + add_watch + read");
    let elf = crate::init::inotify_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "inotify-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate Q3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_r2() {
    serial_println!("[user_task] Gate R2: splice pipe to pipe");
    let elf = crate::init::splice_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "splice-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate R2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_r3() {
    serial_println!("[user_task] Gate R3: flock exclusive lock then unlock");
    let elf = crate::init::flock_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "flock-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate R3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_s2() {
    serial_println!("[user_task] Gate S2: sendfile file to pipe");
    let elf = crate::init::sendfile_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "sendfile-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate S2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_s3() {
    serial_println!("[user_task] Gate S3: tee without consuming the source");
    let elf = crate::init::tee_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "tee-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate S3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_t2() {
    serial_println!("[user_task] Gate T2: copy_file_range file to file");
    let elf = crate::init::copy_file_range_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "copyfr-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate T2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_t3() {
    serial_println!("[user_task] Gate T3: vmsplice user page into pipe");
    let elf = crate::init::vmsplice_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "vmsplice-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate T3 parent pid={} reaped={}", pid, reaped);
}
