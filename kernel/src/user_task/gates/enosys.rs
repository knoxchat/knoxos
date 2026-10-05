use crate::serial_println;

use super::{reap_child, run_until_desktop, spawn_or_log};

pub(super) fn run_gate_j4() {
    serial_println!("[user_task] Gate J4: bpf returns ENOSYS");
    let elf = crate::init::enosys_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "enosys-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate J4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_l4() {
    serial_println!("[user_task] Gate L4: quotactl returns ENOSYS");
    let elf = crate::init::quotactl_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "quotactl-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate L4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_m4() {
    serial_println!("[user_task] Gate M4: io_uring_setup returns ENOSYS");
    let elf = crate::init::io_uring_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "io-uring-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate M4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_n4() {
    serial_println!("[user_task] Gate N4: userfaultfd returns ENOSYS");
    let elf = crate::init::userfaultfd_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "uffd-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate N4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_o4() {
    serial_println!("[user_task] Gate O4: perf_event_open returns ENOSYS");
    let elf = crate::init::perf_event_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "perf-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate O4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_p4() {
    serial_println!("[user_task] Gate P4: fanotify_init returns ENOSYS");
    let elf = crate::init::fanotify_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "fanotify-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate P4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_q4() {
    serial_println!("[user_task] Gate Q4: io_setup returns ENOSYS");
    let elf = crate::init::io_setup_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "aio-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate Q4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_r4() {
    serial_println!("[user_task] Gate R4: kexec_load returns ENOSYS");
    let elf = crate::init::kexec_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "kexec-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate R4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_s4() {
    serial_println!("[user_task] Gate S4: init_module returns ENOSYS");
    let elf = crate::init::init_module_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "initmod-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate S4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_t4() {
    serial_println!("[user_task] Gate T4: mount_setattr returns ENOSYS");
    let elf = crate::init::mount_setattr_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "mountattr-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate T4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_u4() {
    serial_println!("[user_task] Gate U4: fsopen returns ENOSYS");
    let elf = crate::init::fsopen_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "fsopen-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate U4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_v4() {
    serial_println!("[user_task] Gate V4: keyctl returns ENOSYS");
    let elf = crate::init::keyctl_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "keyctl-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate V4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_w4() {
    serial_println!("[user_task] Gate W4: ioperm returns ENOSYS");
    let elf = crate::init::ioperm_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "ioperm-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate W4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_x4() {
    serial_println!("[user_task] Gate X4: iopl returns ENOSYS");
    let elf = crate::init::iopl_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "iopl-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate X4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_y4() {
    serial_println!("[user_task] Gate Y4: acct returns ENOSYS");
    let elf = crate::init::acct_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "acct-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate Y4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_z4() {
    serial_println!("[user_task] Gate Z4: swapon returns ENOSYS");
    let elf = crate::init::swapon_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "swapon-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate Z4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_aa4() {
    serial_println!("[user_task] Gate AA4: modify_ldt returns ENOSYS");
    let elf = crate::init::modify_ldt_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "modify-ldt-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AA4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ab4() {
    serial_println!("[user_task] Gate AB4: sysfs returns ENOSYS");
    let elf = crate::init::sysfs_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "sysfs-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AB4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ac4() {
    serial_println!("[user_task] Gate AC4: vhangup returns ENOSYS");
    let elf = crate::init::vhangup_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "vhangup-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AC4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ad4() {
    serial_println!("[user_task] Gate AD4: lookup_dcookie returns ENOSYS");
    let elf = crate::init::lookup_dcookie_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "lookup-dcookie-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AD4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ae4() {
    serial_println!("[user_task] Gate AE4: memfd_secret returns ENOSYS");
    let elf = crate::init::memfd_secret_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "memfd-secret-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AE4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_af4() {
    serial_println!("[user_task] Gate AF4: uselib returns ENOSYS");
    let elf = crate::init::uselib_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "uselib-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AF4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ag4() {
    serial_println!("[user_task] Gate AG4: pkey_alloc returns ENOSYS");
    let elf = crate::init::pkey_alloc_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "pkey-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AG4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ah4() {
    serial_println!("[user_task] Gate AH4: pkey_mprotect returns ENOSYS");
    let elf = crate::init::pkey_mprotect_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "pkey-mprotect-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AH4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ai4() {
    serial_println!("[user_task] Gate AI4: pkey_free returns ENOSYS");
    let elf = crate::init::pkey_free_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "pkey-free-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AI4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_aj4() {
    serial_println!("[user_task] Gate AJ4: process_mrelease returns ENOSYS");
    let elf = crate::init::process_mrelease_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "process-mrelease-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AJ4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ak4() {
    serial_println!("[user_task] Gate AK4: set_mempolicy returns ENOSYS");
    let elf = crate::init::set_mempolicy_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "set-mempolicy-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AK4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_al4() {
    serial_println!("[user_task] Gate AL4: get_mempolicy returns ENOSYS");
    let elf = crate::init::get_mempolicy_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "get-mempolicy-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AL4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_am4() {
    serial_println!("[user_task] Gate AM4: mbind returns ENOSYS");
    let elf = crate::init::mbind_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "mbind-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AM4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_an4() {
    serial_println!("[user_task] Gate AN4: sched_setattr returns ENOSYS");
    let elf = crate::init::sched_setattr_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "sched-setattr-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AN4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ao4() {
    serial_println!("[user_task] Gate AO4: futex_waitv returns ENOSYS");
    let elf = crate::init::futex_waitv_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "futex-waitv-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AO4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ap4() {
    serial_println!("[user_task] Gate AP4: open_by_handle_at returns ENOSYS");
    let elf = crate::init::open_by_handle_at_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "open-by-handle-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AP4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_aq4() {
    serial_println!("[user_task] Gate AQ4: name_to_handle_at returns ENOSYS");
    let elf = crate::init::name_to_handle_at_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "name-to-handle-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AQ4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ar4() {
    serial_println!("[user_task] Gate AR4: map_shadow_stack returns ENOSYS");
    let elf = crate::init::map_shadow_stack_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "map-shadow-stack-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AR4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_as4() {
    serial_println!("[user_task] Gate AS4: ustat returns ENOSYS");
    let elf = crate::init::ustat_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "ustat-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AS4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_at4() {
    serial_println!("[user_task] Gate AT4: migrate_pages returns ENOSYS");
    let elf = crate::init::migrate_pages_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "migrate-pages-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AT4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_au4() {
    serial_println!("[user_task] Gate AU4: swapoff returns ENOSYS");
    let elf = crate::init::swapoff_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "swapoff-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AU4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_av4() {
    serial_println!("[user_task] Gate AV4: move_pages returns ENOSYS");
    let elf = crate::init::move_pages_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "move-pages-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AV4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_aw4() {
    serial_println!("[user_task] Gate AW4: remap_file_pages returns ENOSYS");
    let elf = crate::init::remap_file_pages_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "remap-pages-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AW4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ax4() {
    serial_println!("[user_task] Gate AX4: sched_getattr returns ENOSYS");
    let elf = crate::init::sched_getattr_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "sched-getattr-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AX4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ay4() {
    serial_println!("[user_task] Gate AY4: io_destroy returns ENOSYS");
    let elf = crate::init::io_destroy_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "io-destroy-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AY4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_az4() {
    serial_println!("[user_task] Gate AZ4: set_thread_area returns ENOSYS");
    let elf = crate::init::set_thread_area_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "set-thread-area-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AZ4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_ba4() {
    serial_println!("[user_task] Gate BA4: io_cancel returns ENOSYS");
    let elf = crate::init::io_cancel_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "io-cancel-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BA4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bb4() {
    serial_println!("[user_task] Gate BB4: add_key returns ENOSYS");
    let elf = crate::init::add_key_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "add-key-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BB4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bc4() {
    serial_println!("[user_task] Gate BC4: request_key returns ENOSYS");
    let elf = crate::init::request_key_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "request-key-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BC4 parent pid={} reaped={}", pid, reaped);
}

pub(super) fn run_gate_bd4() {
    serial_println!("[user_task] Gate BD4: io_submit returns ENOSYS");
    let elf = crate::init::io_submit_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "io-submit-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate BD4 parent pid={} reaped={}", pid, reaped);
}
