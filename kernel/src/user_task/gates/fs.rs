use crate::serial_println;

use super::{reap_child, run_until_desktop, spawn_or_log};

pub(super) fn run_gate_u2() {
    serial_println!("[user_task] Gate U2: setxattr/getxattr round-trip");
    let elf = crate::init::xattr_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "xattr-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate U2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_u3() {
    serial_println!("[user_task] Gate U3: statx file size");
    let elf = crate::init::statx_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "statx-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate U3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_v2() {
    serial_println!("[user_task] Gate V2: fallocate then statx size");
    let elf = crate::init::fallocate_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "fallocate-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate V2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_v3() {
    serial_println!("[user_task] Gate V3: utimensat then statx mtime");
    let elf = crate::init::utimensat_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "utimensat-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate V3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_w2() {
    serial_println!("[user_task] Gate W2: umask applied on creat");
    let elf = crate::init::umask_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "umask-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate W2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_w3() {
    serial_println!("[user_task] Gate W3: symlink then readlink");
    let elf = crate::init::symlink_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "symlink-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate W3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_x2() {
    serial_println!("[user_task] Gate X2: rename then read new path");
    let elf = crate::init::rename_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "rename-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate X2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_x3() {
    serial_println!("[user_task] Gate X3: truncate then statx size");
    let elf = crate::init::truncate_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "truncate-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate X3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_y2() {
    serial_println!("[user_task] Gate Y2: chown then statx uid");
    let elf = crate::init::chown_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "chown-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate Y2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_y3() {
    serial_println!("[user_task] Gate Y3: mkdir then statx S_IFDIR");
    let elf = crate::init::mkdir_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "mkdir-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate Y3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_z2() {
    serial_println!("[user_task] Gate Z2: unlink then open fails");
    let elf = crate::init::unlink_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "unlink-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate Z2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_z3() {
    serial_println!("[user_task] Gate Z3: chdir then getcwd");
    let elf = crate::init::chdir_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "chdir-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate Z3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_aa2() {
    serial_println!("[user_task] Gate AA2: fchdir then getcwd");
    let elf = crate::init::fchdir_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "fchdir-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AA2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_aa3() {
    serial_println!("[user_task] Gate AA3: access F_OK then missing fails");
    let elf = crate::init::access_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "access-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AA3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ab2() {
    serial_println!("[user_task] Gate AB2: dup2 then read via new fd");
    let elf = crate::init::dup2_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "dup2-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AB2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ab3() {
    serial_println!("[user_task] Gate AB3: uname sysname KnoxOS");
    let elf = crate::init::uname_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "uname-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AB3 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ac2() {
    serial_println!("[user_task] Gate AC2: pread64 at offset without moving pos");
    let elf = crate::init::pread64_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "pread64-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AC2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ad2() {
    serial_println!("[user_task] Gate AD2: pwrite64 at offset without moving pos");
    let elf = crate::init::pwrite64_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "pwrite64-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AD2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ae2() {
    serial_println!("[user_task] Gate AE2: ftruncate then statx size 1");
    let elf = crate::init::ftruncate_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "ftruncate-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AE2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_af2() {
    serial_println!("[user_task] Gate AF2: lseek SET 1 then read y");
    let elf = crate::init::lseek_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "lseek-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AF2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ag2() {
    serial_println!("[user_task] Gate AG2: fdatasync after write");
    let elf = crate::init::fdatasync_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "fdatasync-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AG2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ah2() {
    serial_println!("[user_task] Gate AH2: sync after write");
    let elf = crate::init::sync_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "sync-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AH2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ai2() {
    serial_println!("[user_task] Gate AI2: fchown then statx uid");
    let elf = crate::init::fchown_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "fchown-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AI2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_aj2() {
    serial_println!("[user_task] Gate AJ2: statfs reports f_bsize");
    let elf = crate::init::statfs_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "statfs-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AJ2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ak2() {
    serial_println!("[user_task] Gate AK2: getrlimit reports RLIMIT_NOFILE");
    let elf = crate::init::getrlimit_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getrlimit-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AK2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_al2() {
    serial_println!("[user_task] Gate AL2: setrlimit then getrlimit is 512");
    let elf = crate::init::setrlimit_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "setrlimit-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AL2 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_am2() {
    serial_println!("[user_task] Gate AM2: prlimit64 then getrlimit is 256");
    let elf = crate::init::prlimit64_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "prlimit64-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AM2 parent pid={} reaped={}", pid, reaped);
}
