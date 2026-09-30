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

pub(super) fn run_gate_aq2() {
    serial_println!("[user_task] Gate AQ2: setsid then getsid equals getpid");
    let elf = crate::init::setsid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "setsid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AQ2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_aq3() {
    serial_println!("[user_task] Gate AQ3: setpriority then getpriority is 15");
    let elf = crate::init::setpriority_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "setpriority-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AQ3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ar2() {
    serial_println!("[user_task] Gate AR2: getrusage ru_maxrss is 4096");
    let elf = crate::init::getrusage_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getrusage-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AR2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ar3() {
    serial_println!("[user_task] Gate AR3: clock_gettime CLOCK_MONOTONIC nsec in range");
    let elf = crate::init::clock_gettime_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "clock-gettime-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AR3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_as2() {
    serial_println!("[user_task] Gate AS2: clock_getres is 1ns");
    let elf = crate::init::clock_getres_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "clock-getres-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AS2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_as3() {
    serial_println!("[user_task] Gate AS3: times returns non-zero ticks");
    let elf = crate::init::times_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "times-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AS3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_at2() {
    serial_println!("[user_task] Gate AT2: gettimeofday usec in range");
    let elf = crate::init::gettimeofday_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "gettimeofday-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AT2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_at3() {
    serial_println!("[user_task] Gate AT3: sysinfo totalram is non-zero");
    let elf = crate::init::sysinfo_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "sysinfo-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AT3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_au2() {
    serial_println!("[user_task] Gate AU2: sched_yield returns 0");
    let elf = crate::init::sched_yield_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "sched-yield-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AU2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_au3() {
    serial_println!("[user_task] Gate AU3: alarm(0) is non-negative");
    let elf = crate::init::alarm_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "alarm-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AU3 parent pid={} reaped={}", pid, reaped);
}
