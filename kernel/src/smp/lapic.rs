use core::sync::atomic::Ordering;

use crate::serial_println;

use super::regs::*;
use super::state::{APIC_AVAILABLE, CPU_DATA, PHYS_OFFSET, TIMER_FREQUENCY};

// ─── Local APIC ─────────────────────────────────────────────────────────

/// Read a Local APIC register
unsafe fn lapic_read(offset: u32) -> u32 {
    let phys_offset = PHYS_OFFSET.load(Ordering::Relaxed);
    let addr = (phys_offset + LAPIC_BASE + offset as u64) as *const u32;
    core::ptr::read_volatile(addr)
}

/// Write a Local APIC register
pub(crate) unsafe fn lapic_write(offset: u32, value: u32) {
    let phys_offset = PHYS_OFFSET.load(Ordering::Relaxed);
    let addr = (phys_offset + LAPIC_BASE + offset as u64) as *mut u32;
    core::ptr::write_volatile(addr, value);
}

/// Wait for the ICR delivery status bit to clear (bit 12 of ICR_LO).
/// Must be called before writing a new IPI to the ICR.
pub(crate) unsafe fn wait_icr_idle() {
    for _ in 0..100_000u32 {
        if lapic_read(LAPIC_ICR_LO) & (1 << 12) == 0 {
            return;
        }
        core::hint::spin_loop();
    }
    // Timeout — proceed anyway (best-effort)
}

/// Get the current CPU's APIC ID
pub fn get_apic_id() -> u32 {
    if !APIC_AVAILABLE.load(Ordering::Relaxed) {
        return 0;
    }
    unsafe { (lapic_read(LAPIC_ID) >> 24) & 0xFF }
}

/// Check if APIC mode is active (IOAPIC handles IRQ routing instead of PIC)
pub fn is_apic_mode() -> bool {
    APIC_AVAILABLE.load(Ordering::Relaxed)
}

/// Send End-of-Interrupt to the Local APIC
pub fn eoi() {
    if APIC_AVAILABLE.load(Ordering::Relaxed) {
        unsafe { lapic_write(LAPIC_EOI, 0) };
    }
}

/// Initialize the Local APIC on the current CPU
pub fn init_lapic() {
    unsafe {
        // Enable the APIC via MSR
        let msr_val = rdmsr(MSR_APIC_BASE);
        wrmsr(MSR_APIC_BASE, msr_val | (1 << 11)); // Enable APIC

        // Set the Spurious Interrupt Vector Register
        // Enable APIC + set spurious vector
        lapic_write(LAPIC_SVR, LAPIC_SVR_ENABLE | SPURIOUS_VECTOR);

        // Clear error status
        lapic_write(LAPIC_ESR, 0);
        lapic_write(LAPIC_ESR, 0); // Write twice per spec

        // Set task priority to 0 (accept all interrupts)
        lapic_write(LAPIC_TPR, 0);

        // Send EOI for any pending interrupts
        lapic_write(LAPIC_EOI, 0);
    }

    serial_println!("[APIC] Local APIC initialized, ID={}", get_apic_id());
}

/// Configure the APIC timer for periodic interrupts
pub fn init_timer(frequency_hz: u32) {
    unsafe {
        // Set divide configuration
        lapic_write(LAPIC_TIMER_DIVIDE, TIMER_DIVIDE_16);

        // Calibrate: use PIT to measure APIC timer speed
        // Set a large initial count
        lapic_write(LAPIC_TIMER, TIMER_MASKED); // Mask during calibration
        lapic_write(LAPIC_TIMER_INIT, 0xFFFF_FFFF);

        // Wait approximately 10ms using PIT
        pit_wait_10ms();

        // Read how many ticks elapsed
        let remaining = lapic_read(LAPIC_TIMER_CURRENT);
        let elapsed = 0xFFFF_FFFFu32.wrapping_sub(remaining);

        // Calculate ticks per period
        let ticks_per_second = elapsed * 100; // 10ms * 100 = 1 second
        let ticks_per_interrupt = ticks_per_second / frequency_hz;

        TIMER_FREQUENCY.store(ticks_per_second, Ordering::Relaxed);

        // Leave the timer masked after calibration.
        // The `apic_timer` module will start the periodic timer later
        // with the correct vector (0x40) and proper APIC EOI handling.
        // Starting it here with vector 0x20 would route interrupts to the
        // PIT timer handler which sends PIC EOI instead of APIC EOI,
        // leaving the APIC in-service register stuck.
        lapic_write(LAPIC_TIMER, TIMER_MASKED);
        lapic_write(LAPIC_TIMER_INIT, 0);

        serial_println!(
            "[APIC] Timer: {} ticks/sec, interrupt every {} ticks ({}Hz)",
            ticks_per_second,
            ticks_per_interrupt,
            frequency_hz
        );
    }
}

/// Wait approximately 10ms using the PIT (Channel 2)
pub(crate) fn pit_wait_10ms() {
    // Use a simple spin-loop delay instead of PIT Channel 2.
    // PIT Ch2 bit-5 polling can hang in some QEMU configurations because
    // the speaker-gate / output-pin emulation isn't always reliable.
    //
    // In QEMU, PAUSE is trapped to the hypervisor and can take 100-1000ns
    // per iteration.  50_000 iterations ≈ 5-50 ms, plenty for the 10 ms
    // INIT delay required by the Intel MP specification.
    for _ in 0..50_000u32 {
        core::hint::spin_loop();
    }
}

/// Send an IPI (Inter-Processor Interrupt) to another CPU
pub fn send_ipi(dest_apic_id: u32, vector: u32) {
    unsafe {
        lapic_write(LAPIC_ICR_HI, dest_apic_id << 24);
        lapic_write(LAPIC_ICR_LO, IPI_FIXED | vector);
    }
}

/// Send an IPI to all CPUs except self
pub fn send_ipi_all_except_self(vector: u32) {
    unsafe {
        lapic_write(LAPIC_ICR_LO, IPI_ALL_EXCL | IPI_FIXED | vector);
    }
}

// ─── MSR helpers ────────────────────────────────────────────────────────

unsafe fn rdmsr(msr: u32) -> u64 {
    let (mut lo, mut hi): (u32, u32) = (0, 0);
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("rdmsr", in("ecx") msr, out("eax") lo, out("edx") hi, options(nomem, nostack));
    ((hi as u64) << 32) | (lo as u64)
}

unsafe fn wrmsr(msr: u32, value: u64) {
    let lo = value as u32;
    let hi = (value >> 32) as u32;
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("wrmsr", in("ecx") msr, in("eax") lo, in("edx") hi, options(nomem, nostack));
}

/// Handle APIC timer interrupt (called from interrupt handler)
pub fn timer_interrupt() {
    if APIC_AVAILABLE.load(Ordering::Relaxed) {
        // Update per-CPU stats
        let apic_id = get_apic_id();
        {
            let mut cpus = CPU_DATA.lock();
            for cpu in cpus.iter_mut() {
                if cpu.apic_id == apic_id && cpu.online {
                    cpu.timer_ticks += 1;
                    break;
                }
            }
        }

        // Trigger scheduler
        crate::context::schedule_tick();

        // Send EOI
        eoi();
    }
}
