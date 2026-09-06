#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(knoxos_kernel::test_runner)]
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

extern crate alloc;

#[cfg(target_arch = "x86_64")]
use bootloader_api::{
    BootInfo, BootloaderConfig,
    config::{Mapping, Mappings},
    entry_point,
};
use core::panic::PanicInfo;
#[cfg(not(target_arch = "x86_64"))]
use knoxos_kernel::arch_compat::VirtAddr;
#[cfg(not(target_arch = "x86_64"))]
use knoxos_kernel::arch_compat::bootloader_shim::config::BootloaderConfig;
#[cfg(not(target_arch = "x86_64"))]
use knoxos_kernel::arch_compat::bootloader_shim::{
    BootInfo,
    config::{Mapping, Mappings},
};
#[cfg(target_arch = "x86_64")]
use knoxos_kernel::arch_compat::structures::paging::VirtAddr;
use knoxos_kernel::{
    // Phase 33: Architecture ports, Chromebook, tablet/2-in-1
    aarch64_boot,
    acpi,
    // Phase 14+ modules
    acpi_tables,
    // Phase 7+ modules
    ahci,
    ai,
    // Phase 30 modules
    ai_suggest,
    aio,
    allocator,
    // Phase 10+ modules
    alsa,
    apic_timer,
    // Phase 17+ modules
    app_store,
    apparmor,
    // Phase 16+ modules
    arch,
    audit,
    battery,
    bcache,
    benchmark,
    // Phase 6 modules
    binfmt,
    block,
    bluetooth,
    boot_splash,
    btrfs,
    cabi,
    capabilities,
    // Phase 9+ modules
    cgroup_psi,
    // Phase 18+ modules
    cgroup_v2_controllers,
    cgroup2,
    cgroups,
    // Phase 15+ modules
    cgroups_v2_advanced,
    // Phase 19+ modules — Vivaldi Browser / Chromium support
    chromium_sandbox,
    clock,
    close_range,
    // Phase 11+ modules
    cluster,
    cmdline,
    // Phase 12+ modules
    compiler,
    container,
    context,
    coredump,
    cpio,
    cpufreq,
    cred,
    cron,
    crypto,
    css_engine,
    dbus,
    deployment,
    depthcharge,
    // Phase 13+ modules
    desktop_apps,
    desktop_apps_ext,
    desktop_ops,
    devfs,
    dhcp,
    dm_crypt,
    dmesg,
    dns,
    dpkg,
    drm,
    dynlink,
    e1000,
    ebpf,
    elf,
    epoll,
    eventfd,
    ext2,
    ext4,
    fanotify,
    fat32,
    fd,
    federated,
    fifo,
    file_manager,
    firewall,
    flock,
    fontconfig,
    futex,
    futex2,
    fuzz,
    gamepad,
    gdt,
    glibc_compat,
    gpu,
    gpu_compute,
    gpu_hw,
    gui,
    hardening,
    hda,
    help_system,
    hlt_loop,
    hpet,
    http,
    huge_pages,
    hw_bringup,
    hwtest,
    i18n,
    inference_api,
    init,
    inotify,
    input_event,
    interrupts,
    io_prio,
    io_sched,
    io_uring,
    io_uring_advanced,
    iommu,
    ipc,
    ipv6,
    js_engine,
    k8s,
    kcmp,
    kcov,
    kdb,
    keyring,
    kobject,
    kpanic,
    kpm,
    ktrace,
    kvm,
    landlock,
    ldknoxos,
    libc_funcs,
    live_usb,
    llm,
    man_pages,
    media_player,
    membarrier,
    memfd,
    memory,
    memory_pool,
    midi,
    mmap,
    modules,
    mount,
    mount_api,
    mqueue,
    multiboot,
    multimon,
    musl,
    namespaces,
    nested_irq,
    net,
    net_production,
    netfilter,
    netint,
    netns,
    nfs,
    nfs_server,
    ntp,
    numa,
    nvme,
    onnx,
    oom,
    ota_update,
    overlayfs,
    p9fs,
    package_manager,
    partition,
    path,
    pci,
    pcie_ecam,
    perf,
    persist,
    pgo,
    pgrp,
    pidfd,
    pidns,
    pipe,
    pkg_dist,
    posix_ext,
    posix_tests,
    posix_timer,
    posix2024,
    power,
    power_events,
    preempt_rt,
    println,
    process,
    procfs,
    procfs_extended,
    procfs_full,
    ptrace,
    pty,
    pulseaudio,
    quic,
    quota,
    random,
    // Phase 5 modules
    recovery,
    release_sign,
    repo_server,
    riscv64_boot,
    rlimit,
    rseq,
    rtc,
    rtl8139,
    rust_std,
    rwlock,
    sched_debug,
    sched_ext,
    scheduler,
    screen_record,
    sctp,
    // Phase 4 modules
    seccomp,
    secure_boot,
    security,
    security_cert,
    selfhost,
    selinux,
    serial_println,
    service_manager,
    session,
    shell,
    shell_startup,
    shm,
    signalfd,
    signals,
    slab,
    smp,
    socket_activation,
    soloader,
    sound,
    splice,
    ssp,
    stack_guard,
    strace,
    stress_test,
    suspend,
    swap,
    sysctl,
    sysfs,
    sysinfo,
    syslog,
    tablet_mode,
    task::{Task, executor::Executor, keyboard},
    terminal,
    thermal,
    threads,
    timerfd,
    tls,
    tmpfs,
    tokenizer,
    touchscreen,
    tty,
    uds,
    usb,
    usb_hid,
    userfaultfd,
    usermode,
    users,
    utsname,
    vdso,
    verify,
    vfs,
    video_codec,
    virtio_blk,
    virtio_gpu,
    virtio_input,
    virtio_net,
    virtio_tablet,
    visual_test,
    vivaldi,
    vmm,
    voice,
    vterm,
    vulkan,
    wayland,
    webcam,
    wifi,
    workqueue,
    xattr,
    xdg,
    xdp,
    zfs,
};

/// Configure the bootloader to map all physical memory
#[cfg(target_arch = "x86_64")]
#[allow(deprecated)]
pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings = {
        let mut mappings = Mappings::new_default();
        mappings.physical_memory = Some(Mapping::Dynamic);
        mappings
    };
    // Request 1920×1080 framebuffer (falls back to smaller if unavailable)
    config.frame_buffer = {
        let mut fb = bootloader_api::config::FrameBuffer::new_default();
        fb.minimum_framebuffer_width = Some(1920);
        fb.minimum_framebuffer_height = Some(1080);
        fb
    };
    config
};

#[cfg(target_arch = "x86_64")]
entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

/// Kernel entry point - called by bootloader
fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial_println!("╔══════════════════════════════════════════════════════╗");
    serial_println!("║  KnoxOS - AI Operating System v0.2.1                 ║");
    serial_println!("║  Linux-compatible | Rust-powered | AI-native         ║");
    serial_println!("╚══════════════════════════════════════════════════════╝");
    serial_println!("[KnoxOS] Booting KnoxOS...");
    serial_println!("[KnoxOS] Initializing kernel subsystems...");

    // Initialize GDT (Global Descriptor Table)
    gdt::init();
    serial_println!("[KnoxOS] GDT initialized");

    // Initialize IDT (Interrupt Descriptor Table)
    interrupts::init_idt();
    serial_println!("[KnoxOS] IDT initialized");

    // Initialize PIC (Programmable Interrupt Controller)
    #[cfg(target_arch = "x86_64")]
    unsafe {
        interrupts::PICS.lock().initialize()
    };
    // Unmask IRQs on both PICs:
    // PIC1 (IRQ 0-7): Timer(0), Keyboard(1), Cascade(2) = bits 0,1,2
    // PIC2 (IRQ 8-15): Free1/virtio(9→bit1), Free2/virtio(10→bit2),
    //   Free3/virtio(11→bit3), Mouse(12→bit4), PrimaryATA(14→bit6), SecondaryATA(15→bit7)
    unsafe {
        #[cfg(target_arch = "x86_64")]
        use knoxos_kernel::arch_compat::instructions::port::Port;
        #[cfg(not(target_arch = "x86_64"))]
        use knoxos_kernel::arch_compat::instructions::port::Port;
        let mut pic1_data: Port<u8> = Port::new(0x21);
        let mut pic2_data: Port<u8> = Port::new(0xA1);
        pic1_data.write(0xF8); // Enable timer, keyboard, cascade
        // Enable mouse(bit4), virtio IRQ9(bit1), IRQ10(bit2), IRQ11(bit3),
        // ATA primary IRQ14(bit6), ATA secondary IRQ15(bit7)
        pic2_data.write(0b0010_0001); // 0x21 — unmask bits 1,2,3,4,6,7
    }
    serial_println!("[KnoxOS] PIC initialized (IRQ 0,1,2,9,10,11,12,14,15 unmasked)");

    // NOTE: Don't enable interrupts yet - need heap for input queues first
    // Interrupts will be enabled after heap init + queue init

    // Detect system info
    detect_system_info();

    // Initialize memory management (needed before heap)
    let phys_mem_offset = VirtAddr::new(
        boot_info
            .physical_memory_offset
            .into_option()
            .expect("Physical memory mapping not enabled"),
    );
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator =
        unsafe { memory::BootInfoFrameAllocator::init(&boot_info.memory_regions) };
    serial_println!("[KnoxOS] Memory management initialized");

    // Initialize heap allocator (must be before framebuffer - it allocates a Vec)
    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("Heap initialization failed");
    serial_println!("[KnoxOS] Heap allocator initialized (512 MiB heap)");

    // Parse kernel command line AFTER heap init (parse allocates String/BTreeMap)
    cmdline::parse("root=/dev/vda1 init=/sbin/init console=ttyS0,115200 loglevel=4");

    // Initialize input queues AFTER heap but BEFORE enabling interrupts
    // so interrupt handlers can push data immediately
    keyboard::init_scancode_queue();
    gui::input::init_mouse_queue();
    serial_println!("[KnoxOS] Input queues initialized");

    // NOW enable hardware interrupts (queues are ready)
    knoxos_kernel::arch_compat::instructions::interrupts::enable();
    serial_println!("[KnoxOS] Hardware interrupts enabled");

    // Initialize framebuffer and GUI (needs heap for back buffer allocation)
    serial_println!("[KnoxOS] Initializing framebuffer GUI...");
    gui::init_framebuffer(&mut boot_info.framebuffer, phys_mem_offset.as_u64());

    // ── Boot splash: show early graphical feedback ──────────────────
    {
        let mut fb_guard = gui::FRAMEBUFFER.lock();
        if let Some(ref mut fb) = *fb_guard {
            boot_splash::draw_splash(fb);
        }
    }

    // Initialize virtual filesystem
    vfs::init();
    {
        let mut g = gui::FRAMEBUFFER.lock();
        if let Some(ref mut fb) = *g {
            boot_splash::update_progress(fb, 10);
        }
    }

    // Initialize Real-Time Clock
    rtc::init();

    // Initialize Unix path subsystem
    path::init();

    // Initialize file manager (needs VFS + RTC)
    file_manager::init();

    // Initialize syslog
    syslog::init();

    // Initialize process management
    process::init();

    // Initialize scheduler
    scheduler::init();

    // Enable preemptive scheduling.
    // The context-switch guards in context.rs prevent switching into processes
    // whose RSP or RIP is 0, so it is safe to enable preemption even before
    // user-mode processes are spawned.  The scheduler will simply keep the
    // current (desktop) process running until a process with a fully
    // initialised context is added.
    scheduler::enable_preemption();
    {
        let mut g = gui::FRAMEBUFFER.lock();
        if let Some(ref mut fb) = *g {
            boot_splash::update_progress(fb, 25);
        }
    }

    // Initialize signal handling
    signals::init();

    // Initialize IPC (pipes + message queues)
    ipc::init();

    // Initialize TTY subsystem
    tty::init();

    // Initialize ELF loader
    elf::init();

    // Initialize /proc and /sys pseudo-filesystems
    procfs::init();

    // Initialize context switching
    context::init();

    // Initialize networking stack (TCP/IP)
    net::init();

    // Initialize shared memory (SysV IPC)
    shm::init();

    // Initialize threading (pthreads + futex)
    threads::init();

    // Initialize VT100 terminal emulator
    vterm::init();

    // Initialize usermode support (ring 3 + syscall/sysret)
    usermode::init();

    // Initialize block device layer (ATA/IDE)
    block::init();

    // Initialize block-level buffer cache
    bcache::init();

    // Initialize ext2 filesystem driver
    ext2::init();

    // Initialize sound subsystem (PC Speaker)
    sound::init();

    // Initialize AI inference engine
    ai::init();

    // Initialize package manager
    kpm::init();

    // Initialize package repository server
    repo_server::init();

    // Initialize release signing subsystem
    release_sign::init();

    // Initialize ACPI power management
    acpi::init();

    // Initialize cgroups resource management
    cgroups::init();

    // Initialize security module (LSM)
    security::init();

    // Initialize kernel module system
    modules::init();

    // Initialize GPU driver
    gpu::init();

    // Initialize I/O multiplexing (epoll/poll/select)
    epoll::init();

    // Initialize eventfd
    eventfd::init();

    // Initialize inotify
    inotify::init();

    // Initialize timerfd
    timerfd::init();

    // Initialize signalfd
    signalfd::init();

    // Initialize tmpfs
    tmpfs::init();

    // Initialize Unix domain sockets
    uds::init();

    // Initialize process groups and sessions
    pgrp::init();

    // Initialize user/group authentication
    users::init();

    // Initialize Linux capabilities
    capabilities::init();

    // Initialize namespaces (containers)
    namespaces::init();

    // Initialize Virtual Memory Manager (per-process page tables)
    vmm::init(phys_mem_offset.as_u64());
    vmm::populate_frame_pool(&mut frame_allocator, 8192); // Pre-allocate 32 MiB of frames

    // Initialize APIC & SMP (multi-core support)
    // Respect cmdline: nosmp disables SMP, noapic disables APIC
    if !cmdline::has_flag("noapic") {
        smp::init(phys_mem_offset.as_u64());
    } else {
        serial_println!("[KnoxOS] APIC disabled by kernel command line (noapic)");
    }

    // Initialize virtio network device
    virtio_net::init();

    // Initialize E1000 NIC driver (fallback if no virtio-net)
    e1000::init();

    // Initialize DHCP client (needs NIC)
    dhcp::init();

    // Initialize DNS resolver (needs dhcp)
    dns::init();

    // Initialize network integration (NIC ↔ TCP/IP wiring)
    netint::init();
    {
        let mut g = gui::FRAMEBUFFER.lock();
        if let Some(ref mut fb) = *g {
            boot_splash::update_progress(fb, 50);
        }
    }

    // Initialize virtio block device
    virtio_blk::init();

    // Initialize FAT32 filesystem (needs virtio_blk)
    fat32::init();

    // Initialize ext4 filesystem (journal + extents)
    ext4::init();

    // Initialize partition table parsing (MBR/GPT)
    partition::init();

    // ─── Phase 22: Persistent Storage ─────────────────────────────────
    // Try to mount ext4 rootfs from virtio-blk device
    mount::try_mount_ext4_root();

    // Restore persisted user files from virtio-blk disk
    persist::init();

    // Initialize dynamic linker
    dynlink::init();

    // ─── New subsystems ──────────────────────────────────────────────

    // Initialize device filesystem (/dev)
    devfs::init();

    // Initialize sysfs (/sys)
    sysfs::init();

    // Initialize process credentials
    cred::init();

    // Initialize POSIX clocks
    clock::init();

    // Initialize seccomp (syscall filtering)
    seccomp::init();

    // Initialize FIFO (named pipes)
    fifo::init();

    // Initialize audit logging
    audit::init();

    // Initialize firewall
    firewall::init();
    {
        let mut g = gui::FRAMEBUFFER.lock();
        if let Some(ref mut fb) = *g {
            boot_splash::update_progress(fb, 75);
        }
    }

    // Initialize netfilter framework (conntrack + NAT)
    netfilter::init();

    // Initialize PTY subsystem
    pty::init();

    // Initialize enhanced futex
    futex::init();

    // Initialize POSIX message queues
    mqueue::init();

    // Initialize extended attributes
    xattr::init();

    // Initialize async I/O
    aio::init();

    // Initialize disk quotas
    quota::init();

    // Initialize file locking
    flock::init();

    // Initialize zero-copy I/O (splice/sendfile)
    splice::init();

    // Initialize HTTP server
    http::init();

    // Initialize extended scheduler
    sched_ext::init();

    // ─── Phase 5 subsystems ─────────────────────────────────────────

    // Initialize PCI bus (centralized device enumeration)
    pci::init();

    // Initialize kernel random number generator
    random::init();

    // Initialize sysctl (/proc/sys parameters)
    sysctl::init();

    // Initialize kernel object model (sysfs backing)
    kobject::init();

    // Initialize resource limits (RLIMIT_*)
    rlimit::init();

    // Initialize mount subsystem
    mount::init();

    // Initialize work queue subsystem (deferred work)
    workqueue::init();

    // Initialize OOM killer
    oom::init();

    // Initialize core dump subsystem
    coredump::init();

    // Initialize power management
    power::init();

    // Initialize kernel panic handler
    kpanic::init();

    // ─── Phase 6: Advanced Linux Subsystems ─────────────────────────
    serial_println!("[KnoxOS] Phase 6: Advanced Linux subsystems...");

    // Initialize pipe infrastructure
    pipe::init();

    // Initialize memfd subsystem
    memfd::init();

    // Initialize pidfd subsystem
    pidfd::init();

    // Initialize io_uring subsystem
    io_uring::init();

    // Initialize binary format handlers
    binfmt::init();

    // Initialize ptrace debugging
    ptrace::init();

    // Initialize performance counters
    perf::init();

    // Initialize Landlock LSM
    landlock::init();

    // Initialize userfaultfd
    userfaultfd::init();

    // Initialize restartable sequences
    rseq::init();

    // Initialize kernel code coverage
    kcov::init();

    // Initialize cgroup v2
    cgroup2::init();

    // Initialize NUMA subsystem
    numa::init();

    // Initialize eBPF subsystem
    ebpf::init();

    // ─── Phase 7: Hardware Drivers & Security Hardening ─────────────
    serial_println!("[KnoxOS] Phase 7: Hardware drivers & security hardening...");

    // Initialize stack smashing protection & NX/DEP
    ssp::init();

    // Initialize kernel cryptographic primitives
    crypto::init();

    // Initialize vDSO (fast user-space clock_gettime)
    vdso::init();

    // Initialize TLS/SSL subsystem
    tls::init();

    // Initialize NVMe storage driver
    nvme::init();

    // Initialize AHCI/SATA storage driver
    ahci::init();

    // Initialize USB (XHCI) host controller
    usb::init();

    // Initialize virtual memory swapping
    swap::init();

    // Initialize memory-mapped files (mmap VFS bridge)
    mmap::init();

    // Initialize DRM (Direct Rendering Manager)
    drm::init();

    // Initialize Wayland compositor layer
    wayland::init();

    // Initialize Linux evdev-compatible input subsystem
    input_event::init();

    // Initialize CPIO/initramfs support
    cpio::init();

    // ─── Phase 8: Linux Binary Compatibility & Init ──────────────────
    serial_println!("[KnoxOS] Phase 8: Linux binary compatibility...");

    // Initialize C library ABI compatibility layer
    cabi::init();

    // Initialize dynamic linker (ld-knoxos.so)
    ldknoxos::init();

    // Start /init (PID 1) process
    init::start_init();

    // ─── Phase 9: Advanced Subsystems ───────────────────────────────
    serial_println!("[KnoxOS] Phase 9: Advanced subsystems...");

    // Initialize SMP-safe locking primitives (RwLock, SeqLock, RCU)
    rwlock::init();

    // Initialize PREEMPT_RT real-time scheduling extensions
    preempt_rt::init();

    // Initialize POSIX per-process timers & interval timers
    posix_timer::init();

    // Initialize Intel HD Audio codec driver
    hda::init();

    // Initialize MIDI subsystem (SMF parser + sequencer)
    midi::init();

    // Initialize USB HID driver (keyboard/mouse over USB)
    usb_hid::init();

    // Initialize KVM hardware virtualization
    kvm::init();

    // Initialize network namespace support
    netns::init();

    // Initialize multi-monitor DRM extensions
    multimon::init();

    // Initialize shared library loader (dlopen/dlsym)
    soloader::init();

    // Initialize ONNX ML inference runtime
    onnx::init();

    // Initialize cgroup v2 PSI & resource controllers
    cgroup_psi::init();

    // ─── Phase 11: Full Linux Compat, HW Acceleration, Cluster ──────
    serial_println!("[KnoxOS] Phase 11: Full Linux binary compat & cluster...");

    // Initialize ALSA DMA audio streaming engine
    alsa::init();

    // Initialize musl-libc cross-compilation support
    musl::init();

    // Initialize Rust std library OS backing
    rust_std::init();

    // Initialize GPU compute shader framework
    gpu_compute::init();

    // Initialize cluster / distributed computing
    cluster::init();

    // Enable SMP lock order auditing
    smp::enable_lock_audit();

    // ─── Phase 12: Vulkan, Codecs, NFS, I/O Scheduler, SCTP, FedML, Compiler, KDB
    serial_println!("[KnoxOS] Phase 12: Advanced drivers, protocols & toolchain...");

    // Initialize Vulkan 1.3 driver framework
    vulkan::init();

    // Initialize video codec subsystem (H.264/H.265/VP9/AV1/MJPEG)
    video_codec::init();

    // Initialize NFS v4 client
    nfs::init();

    // Initialize I/O scheduler subsystem (deadline, CFQ, BFQ, kyber, mq-deadline)
    io_sched::init();

    // Initialize SCTP transport protocol
    sctp::init();

    // Initialize federated machine learning
    federated::init();

    // Initialize self-hosting compiler toolchain
    compiler::init();

    // Initialize kernel debugger (KDB)
    kdb::init();

    // ─── Phase 13: Self-hosting, QUIC, NFS server, RTL NIC, Desktop Apps ────
    serial_println!("[KnoxOS] Phase 13: Self-hosting, desktop apps & advanced networking...");

    // Initialize QUIC transport protocol (RFC 9000)
    quic::init();

    // Initialize NFS v4 server
    nfs_server::init();

    // Initialize Realtek RTL8139/RTL8169 NIC driver
    rtl8139::init();

    // Initialize self-hosting build system
    selfhost::init();

    // Initialize desktop applications framework
    desktop_apps::init();

    // Initialize enhanced POSIX compliance layer
    posix_ext::init();

    // ─── End Phase 13 subsystems ────────────────────────────────────

    // ─── Phase 14: Full Linux binary compat, production features, conformance ────
    serial_println!("[KnoxOS] Phase 14: Production features & conformance...");

    // Initialize ACPI table parsing (RSDP/RSDT/XSDT/MADT/FADT/HPET/MCFG)
    acpi_tables::init(phys_mem_offset.as_u64());

    // Initialize HPET high precision event timer
    hpet::init();

    // Initialize PCIe ECAM subsystem
    pcie_ecam::init();

    // Initialize dm-crypt disk encryption
    dm_crypt::init();

    // Initialize XDP (eXpress Data Path) packet processing
    xdp::init();

    // Initialize io_uring advanced features
    io_uring_advanced::init();

    // Initialize C standard library functions (libc compat)
    libc_funcs::init();

    // Initialize AppArmor MAC subsystem
    apparmor::init();

    // Initialize kernel tracing infrastructure (ftrace/kprobes)
    ktrace::init();

    // Initialize POSIX conformance test infrastructure
    posix_tests::init();

    // ─── Phase 15: Full Linux Binary Compatibility & Production ─────
    serial_println!("[KnoxOS] Phase 15: Full Linux binary compat & production...");

    // Initialize SELinux mandatory access control
    selinux::init();

    // Initialize advanced cgroup v2 features (memory reclaim, IO latency/cost, CPU burst)
    cgroups_v2_advanced::init();

    // Initialize scheduler debugging & PELT metrics
    sched_debug::init();

    // Initialize next-gen futex (futex2: variable-width, NUMA-aware, waitv)
    futex2::init();

    // Initialize extended /proc filesystem entries
    procfs_extended::init();

    // Initialize IOMMU (DMA remapping, device isolation)
    iommu::init();

    // Initialize fanotify filesystem notifications
    fanotify::init();

    // Initialize I/O priority subsystem (ioprio classes)
    io_prio::init();

    // Initialize close_range() FD management
    close_range::init();

    // Initialize membarrier expedited memory barriers
    membarrier::init();

    // Initialize new mount API (fsopen/fsconfig/fsmount/move_mount)
    mount_api::init();

    // Initialize production-ready TCP networking (congestion control, SACK, ECN, TFO)
    net_production::init();

    // Initialize process comparison (kcmp for CRIU)
    kcmp::init();

    // Initialize PID namespace extensions (hierarchical, translation)
    pidns::init();

    // Initialize hardware GPU acceleration (GEM, DMA-BUF, compute)
    gpu_hw::init();

    // Initialize hardware testing framework
    hwtest::init();

    // ─── Phase 16: Multi-Architecture, Container Orchestration, Distribution ─
    serial_println!("[KnoxOS] Phase 16: Multi-arch, orchestration & certification...");

    // Initialize multi-architecture abstraction layer (x86_64/AArch64/RISC-V)
    arch::init();

    // Initialize Kubernetes-compatible container orchestration
    k8s::init();

    // Initialize package distribution network
    pkg_dist::init();

    // Initialize hardware bring-up & platform abstraction
    hw_bringup::init();

    // Initialize security certification framework (CC EAL4, FIPS 140-3, CIS)
    security_cert::init();

    // ─── Phase 17: Formal Verification, Enterprise Storage, POSIX.1-2024 ────
    serial_println!("[KnoxOS] Phase 17: Formal verification, enterprise storage & POSIX.1-2024...");

    // Initialize ZFS enterprise filesystem (pools, datasets, snapshots, ARC, RAIDZ)
    zfs::init();

    // Initialize Btrfs copy-on-write filesystem (subvolumes, snapshots, RAID)
    btrfs::init();

    // Initialize formal verification subsystem (invariants, contracts, state machines)
    verify::init();

    // Initialize POSIX.1-2024 full compliance layer (semaphores, barriers, spawn, shm)
    posix2024::init();

    // Initialize KnoxOS Application Store (package marketplace, signing, dependencies)
    app_store::init();

    // ─── End Phase 17 subsystems ────────────────────────────────────

    // ─── Phase 18: Full Linux /proc, Tracing, System Info ───────────
    serial_println!("[KnoxOS] Phase 18: Full /proc, tracing, system info...");

    // Initialize full /proc filesystem (meminfo, cpuinfo, stat, vmstat, etc.)
    procfs_full::init();

    // Initialize kernel ring buffer (dmesg)
    dmesg::init();

    // Initialize syscall tracing (strace)
    strace::init();

    // Initialize system information subsystem (sysinfo)
    sysinfo::init();

    // Initialize UTS namespace (uname, hostname)
    utsname::init();

    // Initialize cgroup v2 resource controllers (CPU, memory, IO, PID, PSI)
    cgroup_v2_controllers::init();

    // ─── End Phase 18 subsystems ────────────────────────────────────

    // ─── Phase 19: Vivaldi Browser / Chromium Compatibility ─────────
    serial_println!("[KnoxOS] Phase 19: Vivaldi browser compatibility layer...");

    // D-Bus message bus (system + session)
    dbus::init();

    // Debian package manager (dpkg .deb support)
    dpkg::init();

    // Font discovery and configuration
    fontconfig::init();

    // XDG desktop integration (Base Directory, MIME, .desktop files)
    xdg::init();

    // PulseAudio audio server
    pulseaudio::init();

    // Chromium process sandbox (seccomp-BPF + namespaces)
    chromium_sandbox::init();

    // glibc ABI compatibility layer (versioned symbols, TLS, CRT)
    glibc_compat::init();

    // Vivaldi browser integration (ties all subsystems together)
    vivaldi::init();

    // ─── End Phase 19 subsystems ────────────────────────────────────

    // ─── End Phase 16 subsystems ────────────────────────────────────

    // ─── End Phase 15 subsystems ────────────────────────────────────

    // ─── End Phase 14 subsystems ────────────────────────────────────

    // ─── End Phase 12 subsystems ────────────────────────────────────

    // ─── End Phase 11 subsystems ────────────────────────────────────

    // ─── End Phase 9 subsystems ─────────────────────────────────────

    // ─── End Phase 7 subsystems ─────────────────────────────────────

    // ─── End new subsystems ──────────────────────────────────────────

    // ─── Phase 28: Production Hardening ─────────────────────────────
    serial_println!("[KnoxOS] Phase 28: Production hardening...");

    // Initialize security hardening (ASLR, DEP/W^X, lockdown, fuzzer, unsafe audit)
    hardening::init();

    // Initialize stress testing framework (leak detector, lockdep, test suite)
    stress_test::init();

    // ─── End Phase 28 subsystems ────────────────────────────────────

    // ─── Phase 30: Remaining Status Items ───────────────────────────
    serial_println!("[KnoxOS] Phase 30: Remaining status.md items...");

    // Kernel core
    apic_timer::init();
    // Start the APIC timer in periodic mode for per-core preemptive scheduling
    // The IDT handler for vector 0x40 was registered during IDT init above.
    if apic_timer::is_initialized() {
        apic_timer::start_periodic(10_000); // 10ms = 100Hz quantum
        serial_println!("[KnoxOS] APIC timer started (100Hz periodic, 10ms quantum)");
    }
    cmdline::init();
    multiboot::init();
    nested_irq::init();

    // Disable preemption during remaining init to prevent deadlocks
    // (timer interrupt handler acquires SCHEDULER/PROCESS_TABLE locks)
    crate::scheduler::disable_preemption();

    // Memory management
    serial_println!("[KnoxOS] Phase 30: slab::init...");
    slab::init();
    serial_println!("[KnoxOS] Phase 30: huge_pages::init...");
    huge_pages::init();
    serial_println!("[KnoxOS] Phase 30: stack_guard::init...");
    stack_guard::init();
    serial_println!("[KnoxOS] Phase 30: memory_pool::init...");
    memory_pool::init();
    serial_println!("[KnoxOS] Phase 30: page_cache::init...");
    knoxos_kernel::page_cache::init();
    serial_println!("[KnoxOS] Phase 30: checkpoint::init...");
    knoxos_kernel::checkpoint::init();

    // System services
    serial_println!("[KnoxOS] Phase 30: service_manager::init...");
    service_manager::init();
    serial_println!("[KnoxOS] Phase 30: socket_activation::init...");
    socket_activation::init();
    // Boot all enabled services (dependency-ordered)
    serial_println!("[KnoxOS] Phase 30: service_manager boot...");
    crate::service_manager::SERVICE_MANAGER.lock().boot();
    serial_println!("[KnoxOS] Phase 30: cron::init...");
    cron::init();
    serial_println!("[KnoxOS] Phase 30: session::init...");
    session::init();
    serial_println!("[KnoxOS] Phase 30: ntp::init...");
    ntp::init();

    // Security
    serial_println!("[KnoxOS] Phase 30: keyring::init...");
    keyring::init();
    serial_println!("[KnoxOS] Phase 30: secure_boot::init...");
    secure_boot::init();

    // Power management
    serial_println!("[KnoxOS] Phase 30: cpufreq...");
    cpufreq::init();
    suspend::init();
    battery::init();
    power_events::init();
    thermal::init();

    // AI subsystem
    serial_println!("[KnoxOS] Phase 30: AI subsystem...");
    tokenizer::init();
    inference_api::init();
    ai_suggest::init();

    // Device drivers
    serial_println!("[KnoxOS] Phase 30: device drivers...");
    touchscreen::init();
    gamepad::init();

    // Desktop applications & browser
    serial_println!("[KnoxOS] Phase 30: desktop apps...");
    desktop_ops::init();
    desktop_apps_ext::init();
    shell_startup::init();
    media_player::init();
    css_engine::init();
    js_engine::init();
    voice::init();

    // Package management
    serial_println!("[KnoxOS] Phase 30: package_manager...");
    package_manager::init();

    // Testing & QA
    serial_println!("[KnoxOS] Phase 30: testing & QA...");
    benchmark::init();
    fuzz::init();
    visual_test::init();

    // Documentation
    serial_println!("[KnoxOS] Phase 30: documentation...");
    man_pages::init();
    help_system::init();

    // Build & distribution
    serial_println!("[KnoxOS] Phase 30: build & distribution...");
    live_usb::init();
    ota_update::init();

    // Performance & optimization
    serial_println!("[KnoxOS] Phase 30: performance...");
    pgo::init();

    // Multimedia
    serial_println!("[KnoxOS] Phase 30: multimedia...");
    screen_record::init();
    webcam::init();

    // GUI extensions
    serial_println!("[KnoxOS] Phase 30: GUI extensions...");
    gui::font_scale::init();
    gui::custom_wallpaper::init();
    gui::theme_switch::init();
    gui::multi_dpi::init();
    gui::hw_cursor::init();
    gui::multi_monitor_wm::init();
    gui::settings_ext::init();
    gui::guided_installer::init();
    gui::glyph_cache::init();
    gui::backing_store::init();
    gui::gpu_render::init();

    // Re-enable preemption now that init is done
    crate::scheduler::enable_preemption();

    serial_println!("[KnoxOS] Phase 30 complete — all status.md items initialized");

    // ─── Phase 33: Architecture Ports, Chromebook & Tablet Support ──
    serial_println!("[KnoxOS] Phase 33: Architecture ports & device support...");
    aarch64_boot::init();
    riscv64_boot::init();
    depthcharge::init();
    tablet_mode::init();
    serial_println!("[KnoxOS] Phase 33 complete — multi-arch & tablet support initialized");

    // ─── End Phase 30 subsystems ────────────────────────────────────

    // Initialize shell
    shell::init();

    // Initialize integrated terminal (Fish-style UX, Alacritty-inspired renderer)
    terminal::init();

    // Initialize PS/2 mouse
    gui::input::init_mouse();

    // Initialize USB tablet / absolute pointer for seamless QEMU cursor
    virtio_tablet::init();

    // Initialize VirtIO mouse input (replaces USB tablet for cursor movement)
    virtio_input::init();

    // Clear boot splash before transitioning to desktop
    {
        let mut fb_guard = gui::FRAMEBUFFER.lock();
        if let Some(ref mut fb) = *fb_guard {
            boot_splash::update_progress(fb, 100);
            boot_splash::clear_splash(fb);
        }
    }

    // Load persisted settings before drawing desktop
    gui::settings_persist::init();

    // Draw the desktop environment
    serial_println!("[KnoxOS] Drawing desktop environment...");
    gui::desktop::draw_desktop();

    // Fire welcome notification
    gui::notifications::system(
        "Welcome to KnoxOS",
        "Desktop environment loaded successfully",
    );

    serial_println!("[KnoxOS] ════════════════════════════════════════════════");
    serial_println!("[KnoxOS]   KnoxOS Desktop Environment ready!");
    serial_println!(
        "[KnoxOS]   Resolution: {}x{}",
        gui::screen_size().0,
        gui::screen_size().1
    );
    serial_println!(
        "[KnoxOS]   Mouse: {} (seamless cursor)",
        if virtio_tablet::is_active() {
            "USB Tablet (absolute)"
        } else {
            "PS/2 (relative)"
        }
    );
    serial_println!("[KnoxOS]   Keyboard active");
    serial_println!("[KnoxOS] ════════════════════════════════════════════════");

    // Run async executor for keyboard/mouse input handling
    let mut executor = Executor::new();
    executor.spawn(Task::new(keyboard::process_keypresses()));
    executor.spawn(Task::new(gui::input::process_mouse_events()));
    executor.spawn(Task::new(gui_redraw_loop()));
    executor.run();
}

/// Async task that periodically redraws the desktop when needed
async fn gui_redraw_loop() {
    let mut last_clock_second: u8 = 0xFF;
    let mut last_blink_second: u8 = 0xFF;

    serial_println!("[REDRAW] gui_redraw_loop task started");

    loop {
        // ── Drag-and-drop lifecycle ──
        gui::drag_and_drop::on_begin_frame();

        // ── Deferred preemptive scheduling ──
        scheduler::deferred_schedule();

        // ── RTC-based time checks ──
        let dt = crate::rtc::read_rtc();

        // Terminal cursor blink
        let blink_phase = dt.second & 1;
        if dt.second != last_blink_second && blink_phase == 0 {
            last_blink_second = dt.second;
            terminal::tick_all_blinks();
        }

        // Clock display: update when the second changes
        if dt.second != last_clock_second {
            last_clock_second = dt.second;
            let ticks = crate::interrupts::get_ticks();
            gui::taskbar::update_clock(ticks);
            gui::request_redraw();

            cron::CRON
                .lock()
                .tick(dt.minute, dt.hour, dt.day, dt.month, dt.day_of_week);

            ntp::periodic_sync();
        }

        virtio_input::poll();
        gui::input::drain_mouse_queue();

        let term_dirty = terminal::is_dirty();
        if term_dirty {
            let wm = gui::window::WINDOW_MANAGER.lock();
            for win in wm.windows.iter() {
                if win.content_type == gui::window::WindowContentType::Terminal {
                    gui::push_damage(win.rect);
                }
            }
        }

        let need_full = gui::take_redraw();
        let need_cursor = gui::take_cursor_redraw();

        if need_full {
            let now = gui::read_tsc_public();
            let last = gui::last_present_tsc();
            let min = gui::min_frame_ticks();
            if last > 0 && now.wrapping_sub(last) < min {
                gui::NEEDS_REDRAW.store(true, core::sync::atomic::Ordering::Relaxed);
            } else {
                let _ = gui::take_cursor_redraw();
                let damage = gui::take_damage();
                if damage.is_empty() {
                    let (sw, sh) = gui::cached_screen_size();
                    gui::desktop::draw_desktop_damaged(&[gui::framebuffer::Rect::new(
                        0, 0, sw as u32, sh as u32,
                    )]);
                } else {
                    gui::desktop::draw_desktop_damaged(&damage);
                }
                gui::set_last_present_tsc(gui::read_tsc_public());
            }
        } else if need_cursor {
            gui::update_cursor_only();
        }

        gui::drag_and_drop::on_end_frame();

        knoxos_kernel::yield_once().await;
    }
}

/// Detect and print system information
fn detect_system_info() {
    let cpuid = knoxos_kernel::arch_compat::raw_cpuid::CpuId::new();

    if let Some(vendor) = cpuid.get_vendor_info() {
        serial_println!("[KnoxOS] CPU Vendor: {}", vendor.as_str());
    }

    if let Some(brand) = cpuid.get_processor_brand_string() {
        serial_println!("[KnoxOS] CPU: {}", brand.as_str());
    }

    if let Some(features) = cpuid.get_feature_info() {
        serial_println!(
            "[KnoxOS] CPU Family: {}, Model: {}, Stepping: {}",
            features.family_id(),
            features.model_id(),
            features.stepping_id()
        );
        if features.has_sse() {
            serial_println!("[KnoxOS] SSE support detected");
        }
        if features.has_sse2() {
            serial_println!("[KnoxOS] SSE2 support detected");
        }
    }

    // Memory info
    serial_println!(
        "[KnoxOS] Heap: {} KiB at {:#x}",
        knoxos_kernel::allocator::HEAP_SIZE / 1024,
        knoxos_kernel::allocator::HEAP_START
    );
}

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("[KnoxOS PANIC] {}", info);
    println!("[KERNEL PANIC] {}", info);
    hlt_loop();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    knoxos_kernel::test_panic_handler(info)
}
