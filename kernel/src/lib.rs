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

pub mod arch_compat;

pub mod acpi;
pub mod ai;
#[cfg(feature = "stub-drivers")]
pub mod aio;
pub mod allocator;
pub mod audit;
pub mod bcache;
#[cfg(feature = "stub-drivers")]
pub mod binfmt;
pub mod block;
pub mod boot_splash;
pub mod cabi;
pub mod capabilities;
pub mod cgroup2;
pub mod cgroups;
pub mod clipboard;
pub mod clock;
pub mod context;
#[cfg(feature = "stub-drivers")]
pub mod coredump;
pub mod crash_reporter;
pub mod cred;
pub mod devfs;
#[cfg(feature = "net")]
pub mod dhcp;
#[cfg(feature = "net")]
pub mod dns;
pub mod dynlink;
pub mod e1000;
pub mod ebpf;
pub mod elf;
pub mod epoll;
pub mod eventfd;
pub mod ext2;
#[cfg(feature = "fs-ext4")]
pub mod ext4;
pub mod fat32;
pub mod fd;
pub mod fifo;
pub mod file_manager;
#[cfg(feature = "net")]
pub mod firewall;
#[cfg(feature = "stub-drivers")]
pub mod flatpak;
pub mod flock;
pub mod fsck;
pub mod futex;
pub mod gdt;
pub mod gguf;
pub mod gpu;
#[cfg(feature = "gui")]
pub mod gui;
pub mod http;
pub mod i915;
pub mod init;
pub mod inotify;
pub mod interrupts;
pub mod io_uring;
pub mod ipc;
pub mod kaslr;
#[cfg(feature = "stub-drivers")]
pub mod kcov;
pub mod kobject;
#[cfg(feature = "stub-drivers")]
pub mod kpanic;
pub mod kpm;
pub mod landlock;
pub mod lazy_init;
pub mod ldknoxos;
pub mod memfd;
pub mod memory;
pub mod midi;
pub mod mmap;
pub mod modules;
pub mod mount;
pub mod mqueue;
pub mod namespaces;
#[cfg(feature = "net")]
pub mod net;
#[cfg(feature = "net")]
pub mod netfilter;
#[cfg(feature = "net")]
pub mod netint;
pub mod numa;
pub mod oom;
#[cfg(feature = "stub-drivers")]
pub mod partition;
pub mod path;
pub mod pci;
pub mod pdf;
pub mod perf;
pub mod persist;
pub mod pgrp;
pub mod pidfd;
pub mod pipe;
pub mod power;
pub mod process;
pub mod procfs;
pub mod ptrace;
pub mod pty;
pub mod quota;
pub mod random;
pub mod recovery;
pub mod release_sign;
pub mod repo_server;
pub mod rlimit;
pub mod rseq;
pub mod rtc;
pub mod sched_ext;
pub mod scheduler;
pub mod seccomp;
pub mod security;
pub mod serial;
pub mod shell;
pub mod shm;
pub mod signalfd;
pub mod signals;
pub mod smp;
pub mod sound;
pub mod splice;
pub mod syscall;
pub mod sysctl;
pub mod sysfs;
pub mod syslog;
pub mod task;
pub mod terminal;
#[cfg(test)]
pub mod tests;
pub mod threads;
pub mod timerfd;
pub mod tmpfs;
pub mod tty;
pub mod uds;
pub mod user_task;
pub mod userfaultfd;
pub mod usermode;
pub mod users;
pub mod vfs;
pub mod vga_buffer;
pub mod virtio_blk;
pub mod virtio_input;
#[cfg(feature = "net")]
pub mod virtio_net;
pub mod virtio_tablet;
pub mod vmm;
pub mod vterm;
pub mod wait_queue;
pub mod workqueue;
pub mod xattr;
// Phase 7+ modules
pub mod ahci;
pub mod cpio;
pub mod crypto;
pub mod drm;
pub mod input_event;
pub mod nvme;
pub mod ssh;
pub mod ssp;
pub mod swap;
pub mod tls;
pub mod tmux;
pub mod usb;
pub mod usb_hub;
pub mod vdso;
pub mod wayland;
pub mod wpa3_sae;

// Phase 9+ modules — Advanced subsystems
#[cfg(feature = "stub-drivers")]
pub mod cgroup_psi;
pub mod hda;
pub mod kvm;
#[cfg(feature = "stub-drivers")]
pub mod multimon;
#[cfg(feature = "stub-drivers")]
pub mod netns;
pub mod onnx;
pub mod posix_timer;
pub mod preempt_rt;
#[cfg(feature = "stub-drivers")]
pub mod rwlock;
pub mod soloader;
pub mod usb_hid;

// Phase 10+ modules — Linux-compatible subsystems
pub mod alsa;
pub mod bluetooth;
pub mod container;
#[cfg(feature = "stub-drivers")]
pub mod deployment;
#[cfg(feature = "net")]
pub mod ipv6;
pub mod llm;
pub mod overlayfs;
#[cfg(feature = "stub-drivers")]
pub mod p9fs;
pub mod virtio_gpu;
pub mod wifi;

// Phase 11+ modules — Full Linux binary compat, HW acceleration, cluster
#[cfg(feature = "stub-drivers")]
pub mod cluster;
pub mod gpu_compute;
pub mod musl;
#[cfg(feature = "stub-drivers")]
pub mod rust_std;

// Phase 12+ modules — Vulkan, video codec, NFS, I/O scheduler, SCTP, federated ML, compiler, KDB
#[cfg(feature = "stub-drivers")]
pub mod compiler;
#[cfg(feature = "stub-drivers")]
pub mod federated;
#[cfg(feature = "stub-drivers")]
pub mod io_sched;
#[cfg(feature = "stub-drivers")]
pub mod kdb;
#[cfg(feature = "net")]
pub mod nfs;
#[cfg(feature = "stub-drivers")]
pub mod sctp;
#[cfg(feature = "stub-drivers")]
pub mod video_codec;
#[cfg(feature = "stub-drivers")]
pub mod vulkan;

// Phase 13+ modules — Self-hosting, QUIC, NFS server, RTL NIC, desktop apps, POSIX compliance
pub mod desktop_apps;
#[cfg(feature = "stub-drivers")]
pub mod nfs_server;
pub mod posix_ext;
#[cfg(feature = "stub-drivers")]
pub mod quic;
pub mod rtl8139;
#[cfg(feature = "stub-drivers")]
pub mod selfhost;

// Phase 14+ modules — Full Linux binary compat, production features, conformance
pub mod acpi_tables;
#[cfg(feature = "stub-drivers")]
pub mod apparmor;
#[cfg(feature = "stub-drivers")]
pub mod dm_crypt;
pub mod hpet;
#[cfg(feature = "stub-drivers")]
pub mod io_uring_advanced;
#[cfg(feature = "stub-drivers")]
pub mod ktrace;
pub mod libc_funcs;
pub mod pcie_ecam;
#[cfg(feature = "stub-drivers")]
pub mod posix_tests;
#[cfg(feature = "stub-drivers")]
pub mod xdp;

// Phase 15+ modules — Full Linux binary compatibility, production networking, GPU HW
#[cfg(feature = "stub-drivers")]
pub mod cgroups_v2_advanced;
pub mod close_range;
pub mod fanotify;
pub mod futex2;
pub mod gpu_hw;
pub mod hwtest;
pub mod i18n;
pub mod io_prio;
pub mod iommu;
pub mod kcmp;
pub mod membarrier;
pub mod mount_api;
pub mod net_production;
pub mod pidns;
pub mod procfs_extended;
#[cfg(feature = "stub-drivers")]
pub mod sched_debug;
pub mod selinux;

// Phase 16+ modules — Multi-arch, container orchestration, package distribution, certification
pub mod arch;
#[cfg(feature = "stub-drivers")]
pub mod hw_bringup;
#[cfg(feature = "stub-drivers")]
pub mod k8s;
#[cfg(feature = "stub-drivers")]
pub mod pkg_dist;
#[cfg(feature = "stub-drivers")]
pub mod security_cert;

// Phase 17+ modules — Formal verification, POSIX.1-2024, enterprise storage, app store
#[cfg(feature = "stub-drivers")]
pub mod app_store;
pub mod btrfs;
#[cfg(feature = "stub-drivers")]
pub mod posix2024;
#[cfg(feature = "stub-drivers")]
pub mod verify;
pub mod zfs;

// Phase 18+ modules — Full Linux /proc, strace, dmesg, sysinfo, UTS
#[cfg(feature = "stub-drivers")]
pub mod cgroup_v2_controllers;
pub mod dmesg;
pub mod procfs_full;
pub mod strace;
pub mod sysinfo;
pub mod utsname;

// Phase 19+ modules — Vivaldi Browser / Chromium support
pub mod chromium_sandbox;
pub mod dbus;
pub mod dpkg;
pub mod fontconfig;
pub mod glibc_compat;
pub mod pulseaudio;
pub mod vivaldi;
pub mod xdg;

// Phase 24+ modules — Full POSIX libc, Debian package management, initramfs
#[cfg(feature = "stub-drivers")]
pub mod apt;
pub mod dpkg_scripts;
#[cfg(feature = "stub-drivers")]
pub mod initramfs_boot;
#[cfg(feature = "stub-drivers")]
pub mod posix_libc;

// Phase 28+ modules — Production hardening, stress testing
pub mod hardening;
pub mod stress_test;

// Phase 29+ modules — Distribution & ISO image builder
pub mod arch_port;
pub mod bt_hci_usb;
#[cfg(feature = "stub-drivers")]
pub mod formal_verify;
#[cfg(feature = "stub-drivers")]
pub mod iso9660;
pub mod timezone;
pub mod virtio_wifi;
#[cfg(feature = "stub-drivers")]
pub mod vmx;

// Phase 30+ modules — Remaining ❌ items from status.md
// Kernel core
pub mod apic_timer;
pub mod cmdline;
pub mod livepatch;
pub mod multiboot;
pub mod nested_irq;
// Memory management
#[cfg(feature = "stub-drivers")]
pub mod checkpoint;
pub mod huge_pages;
pub mod ksm;
#[cfg(feature = "stub-drivers")]
pub mod memory_pool;
pub mod page_cache;
pub mod slab;
pub mod stack_guard;
pub mod thp;
// System services
pub mod cron;
pub mod ntp;
pub mod service_manager;
pub mod session;
#[cfg(feature = "stub-drivers")]
pub mod session_restore;
pub mod socket_activation;
// Security
#[cfg(feature = "stub-drivers")]
pub mod keyring;
#[cfg(feature = "stub-drivers")]
pub mod secure_boot;
// Power management
pub mod battery;
pub mod cpufreq;
#[cfg(feature = "stub-drivers")]
pub mod power_events;
pub mod suspend;
pub mod thermal;
// AI subsystem
#[cfg(feature = "stub-drivers")]
pub mod ai_suggest;
pub mod inference_api;
#[cfg(feature = "stub-drivers")]
pub mod tokenizer;
// Device drivers
#[cfg(feature = "stub-drivers")]
pub mod gamepad;
#[cfg(feature = "stub-drivers")]
pub mod sdmmc;
#[cfg(feature = "stub-drivers")]
pub mod touchscreen;
#[cfg(feature = "stub-drivers")]
pub mod usb_audio;
// Desktop applications
#[cfg(feature = "stub-drivers")]
pub mod clock_app;
#[cfg(feature = "stub-drivers")]
pub mod color_blindness;
#[cfg(feature = "stub-drivers")]
pub mod color_picker;
#[cfg(feature = "stub-drivers")]
pub mod contacts;
#[cfg(feature = "stub-drivers")]
pub mod css_engine;
#[cfg(feature = "stub-drivers")]
pub mod desktop_apps_ext;
#[cfg(feature = "stub-drivers")]
pub mod desktop_ops;
#[cfg(feature = "stub-drivers")]
pub mod email;
#[cfg(feature = "stub-drivers")]
pub mod font_viewer;
#[cfg(feature = "stub-drivers")]
pub mod js_engine;
pub mod media_player;
pub mod shell_startup;
#[cfg(feature = "stub-drivers")]
pub mod task_manager;
#[cfg(feature = "stub-drivers")]
pub mod video_player;
#[cfg(feature = "stub-drivers")]
pub mod voice;
// Package management
pub mod package_manager;
pub mod pam;
// Testing & QA
pub mod benchmark;
#[cfg(feature = "stub-drivers")]
pub mod fuzz;
pub mod visual_test;
// Documentation
#[cfg(feature = "stub-drivers")]
pub mod help_system;
#[cfg(feature = "stub-drivers")]
pub mod man_pages;
// Build & distribution
#[cfg(feature = "stub-drivers")]
pub mod live_usb;
pub mod ota_update;
// Performance
pub mod pgo;
// Screen recording / webcam
pub mod screen_record;
#[cfg(feature = "stub-drivers")]
pub mod webcam;

// Phase 31+ modules — Final ❌ items and production hardening
#[cfg(feature = "stub-drivers")]
pub mod bare_metal_test;
#[cfg(feature = "stub-drivers")]
pub mod bonding;
pub mod bridge;
#[cfg(feature = "stub-drivers")]
pub mod browser_launch;
pub mod cifs;
#[cfg(feature = "stub-drivers")]
pub mod device_mapper;
#[cfg(feature = "stub-drivers")]
pub mod disk_install;
pub mod exfat;
pub mod fuse;
pub mod gpu_compositor;
pub mod hw_nic;
pub mod linux_compat;
#[cfg(feature = "stub-drivers")]
pub mod lvm;
#[cfg(feature = "stub-drivers")]
pub mod md_raid;
#[cfg(feature = "stub-drivers")]
pub mod mdns;
pub mod network_manager;
pub mod ntfs;
#[cfg(feature = "stub-drivers")]
pub mod test_images;
#[cfg(feature = "stub-drivers")]
pub mod traffic_shaping;
pub mod vlan;
#[cfg(feature = "stub-drivers")]
pub mod vpn;
pub mod xfs;

// Phase 32+ modules — Production completeness (device drivers, desktop, security, AI, services)
// Device drivers (additional)
pub mod amdgpu;
pub mod ax211;
pub mod broadcom_wifi;
#[cfg(feature = "stub-drivers")]
pub mod bt_audio;
#[cfg(feature = "stub-drivers")]
pub mod dp_mst;
#[cfg(feature = "stub-drivers")]
pub mod floppy;
pub mod gpio;
#[cfg(feature = "stub-drivers")]
pub mod hdmi_cec;
pub mod i2c;
#[cfg(feature = "stub-drivers")]
pub mod nouveau;
#[cfg(feature = "stub-drivers")]
pub mod pcie_hotplug;
#[cfg(feature = "stub-drivers")]
pub mod rtl8125;
pub mod thunderbolt;
pub mod tpm2;
#[cfg(feature = "stub-drivers")]
pub mod trackpad;
#[cfg(feature = "stub-drivers")]
pub mod usb_ethernet;
#[cfg(feature = "stub-drivers")]
pub mod usb_pd;
#[cfg(feature = "stub-drivers")]
pub mod usb_serial;
#[cfg(feature = "stub-drivers")]
pub mod uvc;
#[cfg(feature = "stub-drivers")]
pub mod wacom;
// Compositor & rendering
#[cfg(feature = "stub-drivers")]
pub mod damage_tracking;
#[cfg(feature = "stub-drivers")]
pub mod lazy_render;
#[cfg(feature = "stub-drivers")]
pub mod subpixel_font;
#[cfg(feature = "stub-drivers")]
pub mod vulkan_compositor;
// Accessibility
pub mod atspi;
// Input
#[cfg(feature = "stub-drivers")]
pub mod handwriting;
#[cfg(feature = "stub-drivers")]
pub mod ime;
#[cfg(feature = "stub-drivers")]
pub mod keyboard_layout;
// Security (additional)
#[cfg(feature = "stub-drivers")]
pub mod dm_verity;
#[cfg(feature = "stub-drivers")]
pub mod firejail;
#[cfg(feature = "stub-drivers")]
pub mod home_encryption;
pub mod ima;
#[cfg(feature = "stub-drivers")]
pub mod sandbox_data;
// Multimedia codecs
#[cfg(feature = "stub-drivers")]
pub mod aac;
#[cfg(feature = "stub-drivers")]
pub mod flac;
#[cfg(feature = "stub-drivers")]
pub mod gif;
#[cfg(feature = "stub-drivers")]
pub mod low_latency_audio;
#[cfg(feature = "stub-drivers")]
pub mod mp3;
#[cfg(feature = "stub-drivers")]
pub mod mp4_container;
#[cfg(feature = "stub-drivers")]
pub mod ogg;
#[cfg(feature = "stub-drivers")]
pub mod svg;
#[cfg(feature = "stub-drivers")]
pub mod webp;
// AI subsystem (additional)
#[cfg(feature = "stub-drivers")]
pub mod ai_model_cache;
pub mod gpu_matmul;
#[cfg(feature = "stub-drivers")]
pub mod image_gen;
#[cfg(feature = "stub-drivers")]
pub mod onnx_ops;
// Internationalization
#[cfg(feature = "stub-drivers")]
pub mod date_format;
pub mod gettext;
#[cfg(feature = "stub-drivers")]
pub mod spell_check;
// Service management (additional)
#[cfg(feature = "stub-drivers")]
pub mod runlevel;
#[cfg(feature = "stub-drivers")]
pub mod service_advanced;
// Package management (additional)
#[cfg(feature = "stub-drivers")]
pub mod local_pkg_cache;
#[cfg(feature = "stub-drivers")]
pub mod pkg_advanced;
#[cfg(feature = "stub-drivers")]
pub mod unattended_upgrades;
// User services
#[cfg(feature = "stub-drivers")]
pub mod online_accounts;
#[cfg(feature = "stub-drivers")]
pub mod parental_controls;
// System reliability
#[cfg(feature = "stub-drivers")]
pub mod boot_time;
#[cfg(feature = "stub-drivers")]
pub mod connection_pool;
#[cfg(feature = "stub-drivers")]
pub mod crash_dump;
#[cfg(feature = "stub-drivers")]
pub mod health_check;
pub mod journal_replay;
#[cfg(feature = "stub-drivers")]
pub mod live_patch;
#[cfg(feature = "stub-drivers")]
pub mod perf_monitor;
#[cfg(feature = "stub-drivers")]
pub mod power_saving;
#[cfg(feature = "stub-drivers")]
pub mod safe_mode;
#[cfg(feature = "stub-drivers")]
pub mod system_restore;
// Deployment
#[cfg(feature = "stub-drivers")]
pub mod container_runtime;
#[cfg(feature = "stub-drivers")]
pub mod firmware_update;
#[cfg(feature = "stub-drivers")]
pub mod pxe;
#[cfg(feature = "stub-drivers")]
pub mod usb_installer;
// Test framework
#[cfg(feature = "stub-drivers")]
pub mod test_framework;

// Phase 33+ modules — Architecture ports, Chromebook, tablet/2-in-1 support
#[cfg(any(target_arch = "aarch64", feature = "stub-drivers"))]
pub mod aarch64_boot;
#[cfg(feature = "stub-drivers")]
pub mod depthcharge;
#[cfg(any(target_arch = "riscv64", feature = "stub-drivers"))]
pub mod riscv64_boot;
#[cfg(feature = "stub-drivers")]
pub mod tablet_mode;

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
