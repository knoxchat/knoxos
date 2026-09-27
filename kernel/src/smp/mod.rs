/// APIC & SMP — Advanced Programmable Interrupt Controller and Symmetric Multi-Processing
///
/// This module provides:
///   - Local APIC initialization (replacing PIC for per-CPU interrupts)
///   - APIC timer for per-CPU preemptive scheduling
///   - I/O APIC for routing external interrupts
///   - Application Processor (AP) startup via SIPI
///   - Per-CPU data structures and run queues
///   - SMP-safe locking primitives: TicketLock, RwSpinLock
///   - Lock ordering enforcement and deadlock detection
///
/// The APIC provides the foundation for SMP by giving each CPU its own
/// interrupt controller and timer, enabling true parallel execution.
///
/// Module layout:
///   locks      — TicketLock, RwSpinLock
///   lock_audit — lock class order checking
///   barrier    — memory fences and IRQ save/restore
///   regs       — Local APIC / I/O APIC / IPI / MSR constants
///   state      — per-CPU data and SMP globals
///   lapic      — Local APIC MMIO, timer, IPI
///   ioapic     — I/O APIC routing and MADT apply
///   detect     — MADT/CPUID CPU count
///   startup    — AP trampoline, SIPI, Gate I1 self-test
///   idle       — AP idle loop and user-task handoff
///   runqueue   — per-CPU run queues and load helpers
use core::sync::atomic::Ordering;

use crate::serial_println;

mod barrier;
mod detect;
mod idle;
mod ioapic;
mod lapic;
mod lock_audit;
mod locks;
mod regs;
mod runqueue;
mod startup;
mod state;

pub use barrier::*;
pub use detect::*;
pub use idle::*;
pub use ioapic::*;
pub use lapic::*;
pub use lock_audit::*;
pub use locks::*;
pub use regs::*;
pub use runqueue::*;
pub use startup::*;
pub use state::*;

/// Initialize APIC and SMP subsystem
pub fn init(phys_mem_offset: u64) {
    state::PHYS_OFFSET.store(phys_mem_offset, Ordering::Relaxed);

    // Check if APIC is supported via CPUID
    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
    let apic_supported = cpuid
        .get_feature_info()
        .map(|f| f.has_apic())
        .unwrap_or(false);

    if !apic_supported {
        serial_println!("[APIC] APIC not supported, using PIC mode");
        return;
    }

    // Detect number of CPUs
    let num = detect_cpus();
    state::NUM_CPUS.store(num, Ordering::Relaxed);
    serial_println!("[APIC] Detected {} CPU(s)", num);

    // Initialize BSP APIC
    init_lapic();
    state::APIC_AVAILABLE.store(true, Ordering::Relaxed);

    // Set up BSP per-CPU data
    {
        let mut cpus = CPU_DATA.lock();
        cpus[0] = PerCpuData {
            apic_id: get_apic_id(),
            cpu_index: 0,
            is_bsp: true,
            online: true,
            current_pid: 2, // knoxos-desktop
            context_switches: 0,
            timer_ticks: 0,
            idle_ticks: 0,
        };
    }

    // Initialize I/O APIC — routes external IRQs through APIC instead of legacy PIC.
    // The PIC is kept initialized as fallback but the I/O APIC takes priority
    // for interrupt routing when APIC is available.
    init_ioapic();

    // Disable legacy PIC by masking all IRQs — I/O APIC handles routing now.
    // We remap PIC to vectors 0x20-0x2F but mask everything so only APIC delivers.
    disable_legacy_pic();

    // Flush any LAPIC ISR bits stuck from virtual-wire mode.
    // During early boot, the LAPIC forwards PIC interrupts (LINT0 = ExtINT).
    // If those handlers only sent PIC EOI (not LAPIC EOI), the LAPIC ISR
    // bit remains set, blocking further interrupts in the same priority class.
    // Send multiple EOIs to clear all potentially stuck vectors.
    for _ in 0..8 {
        unsafe { lapic::lapic_write(LAPIC_EOI, 0) };
    }

    // Initialize APIC timer (100 Hz for preemptive scheduling)
    init_timer(100);

    // Start Application Processors
    if num > 1 {
        serial_println!("[SMP] Starting {} Application Processor(s)...", num - 1);
        crate::context::create_ap_idle_contexts(num);

        // Identity-map low memory for the AP trampoline
        let mut cr3: u64 = 0;
        unsafe {
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack));
        }
        startup::identity_map_low_memory(phys_mem_offset, cr3);

        // Install AP trampoline code in low memory
        install_trampoline(phys_mem_offset, cr3);

        // Start each AP (APIC IDs 1..num-1, assuming sequential IDs)
        for i in 1..num {
            start_ap(i, i);
        }

        serial_println!(
            "[SMP] {} of {} AP(s) started successfully",
            state::CPUS_STARTED.load(Ordering::Relaxed) - 1,
            num - 1
        );
    }

    serial_println!("[APIC] APIC/SMP subsystem initialized (full APIC mode)");
    serial_println!("[APIC]   BSP APIC ID: {}", get_apic_id());
    serial_println!("[APIC]   Total CPUs: {}", num);
    serial_println!(
        "[APIC]   Online CPUs: {}",
        state::CPUS_STARTED.load(Ordering::Relaxed)
    );
    serial_println!("[APIC]   I/O APIC: enabled, PIC: masked");
    let _ = smp_self_test();
}
