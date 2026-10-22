//! KnoxOS kernel: scheduled Ring 3, VFS persist, VirtIO devices, in-kernel desktop.
//!
//! Only the **live path** (what runs after `./run.sh`) counts. Remaining
//! work is listed in the repo `status/next.md`.
#![no_std]
#![cfg_attr(test, no_main)]
#![cfg_attr(target_arch = "x86_64", feature(abi_x86_interrupt))]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![allow(
    unused_imports,
    unused_doc_comments,
    unused_variables,
    dead_code,
    unused_mut,
    unused_assignments,
    unsafe_op_in_unsafe_fn,
    deprecated,
    unreachable_code,
    suspicious_runtime_symbol_definitions
)]
// Clippy: suppress low-priority stylistic warnings across the crate.
// These are all intentional patterns in OS kernel code:
// `new_without_default` — many kernel types use `new()` without needing Default.
// `result_unit_err` — kernel APIs commonly return Result<_, ()> for simple errors.
// `too_many_arguments` — kernel init functions inherently need many params.
// `type_complexity` — kernel data structures can be complex by nature.
// `should_implement_trait` — some `from_str`/`default` methods intentionally diverge.
// `vec_init_then_push` — builder patterns are clearer for readability.
// `useless_format` — format!("literal") is idiomatic in alloc for String creation.
// `manual_is_multiple_of` — explicit `x % n == 0` is clearer in low-level code.
// `needless_range_loop` — index-based loops are clearer for raw pointer/buffer code.
#![allow(
    clippy::new_without_default,
    clippy::result_unit_err,
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::should_implement_trait,
    clippy::vec_init_then_push,
    clippy::useless_format,
    clippy::manual_is_multiple_of,
    clippy::needless_range_loop,
    clippy::byte_char_slices,
    clippy::chunks_exact_to_as_chunks,
    clippy::collapsible_if,
    clippy::collapsible_match,
    clippy::doc_lazy_continuation,
    clippy::drain_collect,
    clippy::empty_line_after_doc_comments,
    clippy::excessive_precision,
    clippy::for_kv_map,
    clippy::let_and_return,
    clippy::manual_range_patterns,
    clippy::manual_slice_fill,
    clippy::missing_safety_doc,
    clippy::needless_return,
    clippy::question_mark,
    clippy::redundant_closure,
    clippy::single_match,
    clippy::unnecessary_map_or,
    clippy::vec_box
)]

extern crate alloc;

pub mod drivers;
#[cfg(feature = "net")]
pub use drivers::virtio_net;
pub use drivers::{
    ahci, alsa, amdgpu, ax211, battery, block, bluetooth, broadcom_wifi, bt_hci_usb, drm, e1000,
    gpio, gpu, gpu_compositor, gpu_compute, gpu_hw, gpu_matmul, hda, hw_nic, i2c, i915,
    input_event, iommu, midi, nvme, pci, pcie_ecam, rtl8139, sound, thermal, thunderbolt, tpm2,
    usb, usb_hid, usb_hub, vga_buffer, virtio_blk, virtio_gpu, virtio_input, virtio_tablet,
    virtio_wifi, wifi,
};
#[cfg(feature = "stub-drivers")]
pub use drivers::{
    bt_audio, device_mapper, dp_mst, floppy, gamepad, hdmi_cec, lvm, md_raid, nouveau, partition,
    pcie_hotplug, rtl8125, sdmmc, touchscreen, trackpad, usb_audio, usb_ethernet, usb_installer,
    usb_pd, usb_serial, uvc, vulkan, vulkan_compositor, wacom,
};

pub mod fs;
#[cfg(feature = "fs-ext4")]
pub use fs::ext4;
#[cfg(feature = "net")]
pub use fs::nfs;
pub use fs::{
    bcache, btrfs, cifs, cpio, devfs, dm_verity, exfat, ext2, fat32, fd, fsck, fuse, iso9660,
    journal_replay, mount, mount_api, ntfs, overlayfs, path, persist, procfs, procfs_extended,
    procfs_full, quota, sysfs, tmpfs, vfs, xattr, xfs, zfs,
};
#[cfg(feature = "stub-drivers")]
pub use fs::{dm_crypt, p9fs};

pub mod mm;
pub use mm::{
    allocator, huge_pages, ksm, memfd, memory, mmap, numa, oom, page_cache, slab, stack_guard,
    swap, thp, userfaultfd, vmm,
};
#[cfg(feature = "stub-drivers")]
pub use mm::{checkpoint, memory_pool};

pub mod arch;
#[cfg(any(target_arch = "aarch64", feature = "stub-drivers"))]
pub use arch::aarch64_boot;
#[cfg(any(target_arch = "riscv64", feature = "stub-drivers"))]
pub use arch::riscv64_boot;
pub use arch::{
    acpi, acpi_tables, apic_timer, arch_compat, arch_port, cmdline, context, fw_cfg, gdt, hpet,
    interrupts, kaslr, lazy_init, livepatch, multiboot, nested_irq, preempt_rt, rtc, serial, smp,
    usermode,
};

pub mod sched;
pub use sched::{
    capabilities, cgroup2, cgroups, cred, elf, namespaces, pgrp, pidfd, pidns, posix_timer,
    process, rlimit, rseq, sched_ext, scheduler, session, signals, task, threads, timerfd,
    user_task, users, wait_queue, workqueue,
};
#[cfg(feature = "stub-drivers")]
pub use sched::{cgroup_psi, cgroup_v2_controllers, cgroups_v2_advanced, sched_debug};

pub mod ipc;
#[cfg(feature = "stub-drivers")]
pub use ipc::{aio, io_uring_advanced};
pub use ipc::{
    close_range, epoll, eventfd, fanotify, fifo, flock, futex, futex2, inotify, io_uring, kcmp,
    membarrier, mqueue, pipe, pty, shm, signalfd, splice, tty, uds,
};

pub mod security;
pub use security::{
    apparmor, audit, chromium_sandbox, hardening, ima, landlock, random, seccomp, selinux, ssp,
};
#[cfg(feature = "stub-drivers")]
pub use security::{firejail, home_encryption, keyring, sandbox_data, secure_boot};

#[cfg(feature = "net")]
pub mod net;
#[cfg(all(feature = "net", feature = "stub-drivers"))]
pub use net::{bonding, mdns, netns, quic, sctp, traffic_shaping, vpn, xdp};
#[cfg(feature = "net")]
pub use net::{
    bridge, dhcp, dns, firewall, http, ipv6, net_production, netfilter, netint, ntp, vlan,
};

pub mod crypto;
pub use crypto::tls;

pub mod compat;
#[cfg(feature = "stub-drivers")]
pub use compat::{
    aac, ai_model_cache, ai_suggest, app_store, apt, bare_metal_test, binfmt, boot_time,
    browser_launch, clock_app, cluster, color_blindness, color_picker, compiler, connection_pool,
    contacts, container_runtime, coredump, crash_dump, css_engine, damage_tracking, date_format,
    deployment, depthcharge, desktop_apps_ext, desktop_ops, disk_install, email, federated,
    firmware_update, flac, flatpak, font_viewer, formal_verify, fuzz, gif, handwriting,
    health_check, help_system, hw_bringup, image_gen, ime, initramfs_boot, js_engine, k8s, kcov,
    kdb, keyboard_layout, kpanic, ktrace, lazy_render, live_patch, live_usb, local_pkg_cache,
    low_latency_audio, man_pages, mp3, mp4_container, multimon, ogg, online_accounts, onnx_ops,
    parental_controls, perf_monitor, pkg_advanced, pkg_dist, posix_libc, posix_tests, posix2024,
    power_saving, pxe, runlevel, rust_std, safe_mode, security_cert, selfhost, service_advanced,
    session_restore, spell_check, subpixel_font, svg, system_restore, tablet_mode, task_manager,
    test_framework, test_images, unattended_upgrades, verify, video_player, voice, webcam, webp,
};
pub use compat::{
    ai, atspi, benchmark, cabi, container, crash_reporter, cron, dbus, desktop_apps, dpkg,
    dpkg_scripts, file_manager, fontconfig, gettext, gguf, glibc_compat, hwtest, i18n,
    inference_api, kpm, ldknoxos, libc_funcs, linux_compat, llm, media_player, musl,
    network_manager, onnx, ota_update, package_manager, pam, pdf, perf, pgo, posix_ext, pulseaudio,
    repo_server, screen_record, service_manager, shell, shell_startup, socket_activation, soloader,
    ssh, stress_test, timezone, tmux, tokenizer, visual_test, vivaldi, wpa3_sae, xdg,
};

pub mod boot_splash;
pub mod clipboard;
pub mod clock;
pub mod dynlink;
pub mod ebpf;
#[cfg(feature = "gui")]
pub mod gui;
pub mod init;
pub mod kobject;
pub mod modules;
pub mod power;
pub mod ptrace;
pub mod recovery;
pub mod release_sign;
pub mod syscall;
pub mod sysctl;
pub mod syslog;
pub mod terminal;
#[cfg(test)]
pub mod tests;
pub mod vterm;
// Phase 7+ modules
pub mod vdso;
pub mod wayland;

pub mod cpufreq;
pub mod dmesg;
pub mod io_prio;
#[cfg(feature = "stub-drivers")]
pub mod io_sched;
pub mod kvm;
#[cfg(feature = "stub-drivers")]
pub mod nfs_server;
#[cfg(feature = "stub-drivers")]
pub mod power_events;
#[cfg(feature = "stub-drivers")]
pub mod rwlock;
pub mod strace;
pub mod suspend;
pub mod sysinfo;
pub mod utsname;
#[cfg(feature = "stub-drivers")]
pub mod video_codec;
#[cfg(feature = "stub-drivers")]
pub mod vmx;

use core::panic::PanicInfo;

/// Initialize heap - re-export for main
#[cfg(target_arch = "x86_64")]
#[allow(clippy::result_unit_err)]
pub fn init_heap(
    mapper: &mut impl crate::arch_compat::structures::paging::Mapper<
        crate::arch_compat::structures::paging::Size4KiB,
    >,
    frame_allocator: &mut impl crate::arch_compat::structures::paging::FrameAllocator<
        crate::arch_compat::structures::paging::Size4KiB,
    >,
) -> Result<(), ()> {
    allocator::init_heap(mapper, frame_allocator).map_err(|_| ())
}

/// Halt loop - energy efficient idle
pub fn hlt_loop() -> ! {
    loop {
        #[cfg(target_arch = "x86_64")]
        crate::arch_compat::instructions::interrupts::hlt();
        #[cfg(target_arch = "aarch64")]
        // SAFETY: WFE is the architecture idle instruction; no memory is accessed.
        unsafe {
            core::arch::asm!("wfe")
        };
        #[cfg(target_arch = "riscv64")]
        // SAFETY: WFI is the architecture idle instruction; no memory is accessed.
        unsafe {
            core::arch::asm!("wfi")
        };
    }
}

// ─── Testing Infrastructure ─────────────────────────────────────────

pub trait Testable {
    fn run(&self);
}

impl<T> Testable for T
where
    T: Fn(),
{
    fn run(&self) {
        serial_print!("{}...\t", core::any::type_name::<T>());
        self();
        serial_println!("[ok]");
    }
}

pub fn test_runner(tests: &[&dyn Testable]) {
    serial_println!("Running {} tests", tests.len());
    for test in tests {
        test.run();
    }
    exit_qemu(QemuExitCode::Success);
}

pub fn test_panic_handler(info: &PanicInfo) -> ! {
    serial_println!("[failed]\n");
    serial_println!("Error: {}\n", info);
    exit_qemu(QemuExitCode::Failed);
    hlt_loop();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    test_panic_handler(info)
}

#[cfg(test)]
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // `x86_64-unknown-none` has no libtest, so Cargo.toml sets `harness = false`.
    // `test_main` is not generated; this artifact still compiles every
    // `#[cfg(test)]` module (the pre-commit gate).
    serial_println!("[test] compiled (libtest not available on this target)");
    exit_qemu(QemuExitCode::Success);
    hlt_loop();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

pub fn exit_qemu(exit_code: QemuExitCode) {
    #[cfg(target_arch = "x86_64")]
    {
        #[cfg(target_arch = "x86_64")]
        use crate::arch_compat::instructions::port::Port;
        #[cfg(not(target_arch = "x86_64"))]
        use crate::arch_compat::instructions::port::Port;
        // SAFETY: QEMU isa-debug-exit at 0xf4 is mapped for this guest.
        unsafe {
            let mut port = Port::new(0xf4);
            port.write(exit_code as u32);
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = exit_code;
    }
}

/// Yields control once: returns Pending on first poll, Ready on second.
/// This allows the async executor to schedule other tasks and come back.
pub fn yield_once() -> impl core::future::Future<Output = ()> {
    let mut yielded = false;
    core::future::poll_fn(move |_cx| {
        if yielded {
            core::task::Poll::Ready(())
        } else {
            yielded = true;
            core::task::Poll::Pending
        }
    })
}
