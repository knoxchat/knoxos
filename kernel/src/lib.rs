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
pub mod aio;
pub mod allocator;
pub mod audit;
pub mod bcache;
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
pub mod coredump;
pub mod crash_reporter;
pub mod cred;
pub mod devfs;
pub mod dhcp;
pub mod dns;
pub mod dynlink;
pub mod e1000;
pub mod ebpf;
pub mod elf;
pub mod epoll;
pub mod eventfd;
pub mod ext2;
pub mod ext4;
pub mod fat32;
pub mod fd;
pub mod fifo;
pub mod file_manager;
pub mod firewall;
pub mod flatpak;
pub mod flock;
pub mod fsck;
pub mod futex;
pub mod gdt;
pub mod gguf;
pub mod gpu;
pub mod gui;
pub mod http;
pub mod i915;
pub mod init;
pub mod inotify;
pub mod interrupts;
pub mod io_uring;
pub mod ipc;
pub mod kcov;
pub mod kobject;
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
pub mod net;
pub mod netfilter;
pub mod netint;
pub mod numa;
pub mod oom;
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
pub mod userfaultfd;
pub mod usermode;
pub mod users;
pub mod vfs;
pub mod vga_buffer;
pub mod virtio_blk;
pub mod virtio_input;
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
pub mod cgroup_psi;
pub mod hda;
pub mod kvm;
pub mod multimon;
pub mod netns;
pub mod onnx;
pub mod posix_timer;
pub mod preempt_rt;
pub mod rwlock;
pub mod soloader;
pub mod usb_hid;

// Phase 10+ modules — Linux-compatible subsystems
pub mod alsa;
pub mod bluetooth;
pub mod container;
pub mod deployment;
pub mod ipv6;
pub mod llm;
pub mod overlayfs;
pub mod p9fs;
pub mod virtio_gpu;
pub mod wifi;

// Phase 11+ modules — Full Linux binary compat, HW acceleration, cluster
pub mod cluster;
pub mod gpu_compute;
pub mod musl;
pub mod rust_std;

// Phase 12+ modules — Vulkan, video codec, NFS, I/O scheduler, SCTP, federated ML, compiler, KDB
pub mod compiler;
pub mod federated;
pub mod io_sched;
pub mod kdb;
pub mod nfs;
pub mod sctp;
pub mod video_codec;
pub mod vulkan;

// Phase 13+ modules — Self-hosting, QUIC, NFS server, RTL NIC, desktop apps, POSIX compliance
pub mod desktop_apps;
pub mod nfs_server;
pub mod posix_ext;
pub mod quic;
pub mod rtl8139;
pub mod selfhost;

// Phase 14+ modules — Full Linux binary compat, production features, conformance
pub mod acpi_tables;
pub mod apparmor;
pub mod dm_crypt;
pub mod hpet;
pub mod io_uring_advanced;
pub mod ktrace;
pub mod libc_funcs;
pub mod pcie_ecam;
pub mod posix_tests;
pub mod xdp;

// Phase 15+ modules — Full Linux binary compatibility, production networking, GPU HW
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
pub mod sched_debug;
pub mod selinux;

// Phase 16+ modules — Multi-arch, container orchestration, package distribution, certification
pub mod arch;
pub mod hw_bringup;
pub mod k8s;
pub mod pkg_dist;
pub mod security_cert;

// Phase 17+ modules — Formal verification, POSIX.1-2024, enterprise storage, app store
pub mod app_store;
pub mod btrfs;
pub mod posix2024;
pub mod verify;
pub mod zfs;

// Phase 18+ modules — Full Linux /proc, strace, dmesg, sysinfo, UTS
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
pub mod apt;
pub mod dpkg_scripts;
pub mod initramfs_boot;
pub mod posix_libc;

// Phase 28+ modules — Production hardening, stress testing
pub mod hardening;
pub mod stress_test;

// Phase 29+ modules — Distribution & ISO image builder
pub mod arch_port;
pub mod bt_hci_usb;
pub mod formal_verify;
pub mod iso9660;
pub mod timezone;
pub mod virtio_wifi;
pub mod vmx;

// Phase 30+ modules — Remaining ❌ items from status.md
// Kernel core
pub mod apic_timer;
pub mod cmdline;
pub mod livepatch;
pub mod multiboot;
pub mod nested_irq;
// Memory management
pub mod checkpoint;
pub mod huge_pages;
pub mod ksm;
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
pub mod session_restore;
pub mod socket_activation;
// Security
pub mod keyring;
pub mod secure_boot;
// Power management
pub mod battery;
pub mod cpufreq;
pub mod power_events;
pub mod suspend;
pub mod thermal;
// AI subsystem
pub mod ai_suggest;
pub mod inference_api;
pub mod tokenizer;
// Device drivers
pub mod gamepad;
pub mod sdmmc;
pub mod touchscreen;
pub mod usb_audio;
// Desktop applications
pub mod clock_app;
pub mod color_blindness;
pub mod color_picker;
pub mod contacts;
pub mod css_engine;
pub mod desktop_apps_ext;
pub mod desktop_ops;
pub mod email;
pub mod font_viewer;
pub mod js_engine;
pub mod media_player;
pub mod shell_startup;
pub mod task_manager;
pub mod video_player;
pub mod voice;
// Package management
pub mod package_manager;
pub mod pam;
// Testing & QA
pub mod benchmark;
pub mod fuzz;
pub mod visual_test;
// Documentation
pub mod help_system;
pub mod man_pages;
// Build & distribution
pub mod live_usb;
pub mod ota_update;
// Performance
pub mod pgo;
// Screen recording / webcam
pub mod screen_record;
pub mod webcam;

// Phase 31+ modules — Final ❌ items and production hardening
pub mod bare_metal_test;
pub mod bonding;
pub mod bridge;
pub mod browser_launch;
pub mod cifs;
pub mod device_mapper;
pub mod disk_install;
pub mod exfat;
pub mod fuse;
pub mod gpu_compositor;
pub mod hw_nic;
pub mod linux_compat;
pub mod lvm;
pub mod md_raid;
pub mod mdns;
pub mod network_manager;
pub mod ntfs;
pub mod test_images;
pub mod traffic_shaping;
pub mod vlan;
pub mod vpn;
pub mod xfs;

// Phase 32+ modules — Production completeness (device drivers, desktop, security, AI, services)
// Device drivers (additional)
pub mod amdgpu;
pub mod ax211;
pub mod broadcom_wifi;
pub mod bt_audio;
pub mod dp_mst;
pub mod floppy;
pub mod gpio;
pub mod hdmi_cec;
pub mod i2c;
pub mod nouveau;
pub mod pcie_hotplug;
pub mod rtl8125;
pub mod thunderbolt;
pub mod tpm2;
pub mod trackpad;
pub mod usb_ethernet;
pub mod usb_pd;
pub mod usb_serial;
pub mod uvc;
pub mod wacom;
// Compositor & rendering
pub mod damage_tracking;
pub mod lazy_render;
pub mod subpixel_font;
pub mod vulkan_compositor;
// Accessibility
pub mod atspi;
// Input
pub mod handwriting;
pub mod ime;
pub mod keyboard_layout;
// Security (additional)
pub mod dm_verity;
pub mod firejail;
pub mod home_encryption;
pub mod ima;
pub mod sandbox_data;
// Multimedia codecs
pub mod aac;
pub mod flac;
pub mod gif;
pub mod low_latency_audio;
pub mod mp3;
pub mod mp4_container;
pub mod ogg;
pub mod svg;
pub mod webp;
// AI subsystem (additional)
pub mod ai_model_cache;
pub mod gpu_matmul;
pub mod image_gen;
pub mod onnx_ops;
// Internationalization
pub mod date_format;
pub mod gettext;
pub mod spell_check;
// Service management (additional)
pub mod runlevel;
pub mod service_advanced;
// Package management (additional)
pub mod local_pkg_cache;
pub mod pkg_advanced;
pub mod unattended_upgrades;
// User services
pub mod online_accounts;
pub mod parental_controls;
// System reliability
pub mod boot_time;
pub mod connection_pool;
pub mod crash_dump;
pub mod health_check;
pub mod journal_replay;
pub mod live_patch;
pub mod perf_monitor;
pub mod power_saving;
pub mod safe_mode;
pub mod system_restore;
// Deployment
pub mod container_runtime;
pub mod firmware_update;
pub mod pxe;
pub mod usb_installer;
// Test framework
pub mod test_framework;

// Phase 33+ modules — Architecture ports, Chromebook, tablet/2-in-1 support
pub mod aarch64_boot;
pub mod depthcharge;
pub mod riscv64_boot;
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
        unsafe {
            core::arch::asm!("wfe")
        };
        #[cfg(target_arch = "riscv64")]
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
    test_main();
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
