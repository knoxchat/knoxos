use crate::serial_println;

use super::{reap_child, run_until_desktop, spawn_or_log};

pub(super) fn run_gate_ac3() {
    serial_println!("[user_task] Gate AC3: getuid is 0");
    let elf = crate::init::getuid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getuid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AC3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ad3() {
    serial_println!("[user_task] Gate AD3: getgid is 1000");
    let elf = crate::init::getgid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getgid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AD3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ae3() {
    serial_println!("[user_task] Gate AE3: geteuid is 0");
    let elf = crate::init::geteuid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "geteuid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AE3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_af3() {
    serial_println!("[user_task] Gate AF3: getegid is 1000");
    let elf = crate::init::getegid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getegid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AF3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ag3() {
    serial_println!("[user_task] Gate AG3: getppid is non-zero");
    let elf = crate::init::getppid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getppid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AG3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ah3() {
    serial_println!("[user_task] Gate AH3: getpgid is non-zero");
    let elf = crate::init::getpgid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getpgid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AH3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ai3() {
    serial_println!("[user_task] Gate AI3: getsid is non-zero");
    let elf = crate::init::getsid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getsid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AI3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_aj3() {
    serial_println!("[user_task] Gate AJ3: setuid then getuid is 1000");
    let elf = crate::init::setuid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "setuid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AJ3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ak3() {
    serial_println!("[user_task] Gate AK3: setgid then getgid is 2000");
    let elf = crate::init::setgid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "setgid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AK3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_al3() {
    serial_println!("[user_task] Gate AL3: setresuid then geteuid is 1000");
    let elf = crate::init::setresuid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "setresuid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AL3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_am3() {
    serial_println!("[user_task] Gate AM3: setresgid then getegid is 2000");
    let elf = crate::init::setresgid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "setresgid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AM3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_an2() {
    serial_println!("[user_task] Gate AN2: setreuid then geteuid is 1000");
    let elf = crate::init::setreuid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "setreuid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AN2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_an3() {
    serial_println!("[user_task] Gate AN3: setgroups then getgroups is 2000");
    let elf = crate::init::getgroups_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getgroups-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AN3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ao2() {
    serial_println!("[user_task] Gate AO2: setregid then getegid is 2000");
    let elf = crate::init::setregid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "setregid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AO2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ao3() {
    serial_println!("[user_task] Gate AO3: setuid then getresuid is 1000");
    let elf = crate::init::getresuid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getresuid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AO3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ap2() {
    serial_println!("[user_task] Gate AP2: setgid then getresgid is 2000");
    let elf = crate::init::getresgid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getresgid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AP2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ap3() {
    serial_println!("[user_task] Gate AP3: setpgid then getpgrp equals getpid");
    let elf = crate::init::setpgid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "setpgid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AP3 parent pid={} reaped={}", pid, reaped);
}
