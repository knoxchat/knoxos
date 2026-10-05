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

pub(super) fn run_gate_av2() {
    serial_println!("[user_task] Gate AV2: getpid is non-zero");
    let elf = crate::init::getpid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getpid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AV2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_av3() {
    serial_println!("[user_task] Gate AV3: gettid is non-zero");
    let elf = crate::init::gettid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "gettid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AV3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_aw2() {
    serial_println!("[user_task] Gate AW2: sched_getscheduler is 0");
    let elf = crate::init::sched_getscheduler_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getsched-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AW2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_aw3() {
    serial_println!("[user_task] Gate AW3: sched_getparam returns 0");
    let elf = crate::init::sched_getparam_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getparam-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AW3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ax2() {
    serial_println!("[user_task] Gate AX2: sched_get_priority_max is 99");
    let elf = crate::init::sched_get_priority_max_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "prio-max-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AX2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ax3() {
    serial_println!("[user_task] Gate AX3: sched_get_priority_min is 0");
    let elf = crate::init::sched_get_priority_min_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "prio-min-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AX3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ay2() {
    serial_println!("[user_task] Gate AY2: sched_rr_get_interval is 100ms");
    let elf = crate::init::sched_rr_get_interval_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "rr-interval-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AY2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ay3() {
    serial_println!("[user_task] Gate AY3: getcpu returns 0");
    let elf = crate::init::getcpu_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getcpu-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AY3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_az2() {
    serial_println!("[user_task] Gate AZ2: set_robust_list returns 0");
    let elf = crate::init::set_robust_list_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "set-robust-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AZ2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_az3() {
    serial_println!("[user_task] Gate AZ3: get_robust_list returns 0");
    let elf = crate::init::get_robust_list_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "get-robust-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AZ3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ba2() {
    serial_println!("[user_task] Gate BA2: personality(-1) is 0");
    let elf = crate::init::personality_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "personality-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BA2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ba3() {
    serial_println!("[user_task] Gate BA3: nanosleep({{0,1}}) returns 0");
    let elf = crate::init::nanosleep_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "nanosleep-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BA3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bb2() {
    serial_println!("[user_task] Gate BB2: capget returns 0");
    let elf = crate::init::capget_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "capget-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BB2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bb3() {
    serial_println!("[user_task] Gate BB3: ioprio_get is 4");
    let elf = crate::init::ioprio_get_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "ioprio-get-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BB3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bc2() {
    serial_println!("[user_task] Gate BC2: capset returns 0");
    let elf = crate::init::capset_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "capset-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BC2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bc3() {
    serial_println!("[user_task] Gate BC3: ioprio_set returns 0");
    let elf = crate::init::ioprio_set_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "ioprio-set-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BC3 parent pid={} reaped={}", pid, reaped);
}
