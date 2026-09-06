/// Nested Interrupt Handling — Re-entrant interrupt support for x86_64
///
/// Enables interrupt nesting so high-priority interrupts can preempt
/// lower-priority handlers. Uses interrupt priority levels (IPL) and
/// the Local APIC Task Priority Register (TPR) to manage nesting.
///
/// Priority scheme:
///   IPL 0 — Normal execution (all interrupts enabled)
///   IPL 1 — Timer interrupt (preemption tick)
///   IPL 2 — Device interrupts (disk, network, USB)
///   IPL 3 — Keyboard/Mouse (input devices)
///   IPL 15 — NMI/Machine Check (non-maskable, highest priority)
use core::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use crate::serial_println;

/// Current Interrupt Priority Level (per-CPU in SMP; single for now)
static CURRENT_IPL: AtomicU8 = AtomicU8::new(0);

/// Nesting depth counter
static NESTING_DEPTH: AtomicU8 = AtomicU8::new(0);

/// Statistics
static NESTED_COUNT: AtomicU64 = AtomicU64::new(0);
static MAX_DEPTH_SEEN: AtomicU8 = AtomicU8::new(0);

/// Interrupt Priority Levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Ipl {
    /// Normal thread execution
    Normal = 0,
    /// Soft interrupts / deferred work
    SoftIrq = 1,
    /// Timer interrupts
    Timer = 2,
    /// Block device interrupts
    Block = 3,
    /// Network device interrupts
    Network = 4,
    /// Input device interrupts (keyboard, mouse)
    Input = 5,
    /// Serial / UART
    Serial = 6,
    /// Inter-processor interrupts
    Ipi = 12,
    /// Machine check / NMI
    MachineCheck = 15,
}

/// Raise the IPL to the given level, returning the previous IPL.
/// Interrupts at or below the new level will be masked.
pub fn raise_ipl(new_ipl: Ipl) -> u8 {
    let old = CURRENT_IPL.load(Ordering::Relaxed);
    let new = new_ipl as u8;
    if new > old {
        CURRENT_IPL.store(new, Ordering::Relaxed);
        // In a real APIC-based system, we'd write to TPR here
        // to mask lower-priority interrupts
        update_tpr(new);
    }
    old
}

/// Lower the IPL to the given level, potentially unmasking interrupts.
pub fn lower_ipl(ipl: u8) {
    CURRENT_IPL.store(ipl, Ordering::Relaxed);
    update_tpr(ipl);
}

/// Restore the IPL to a previously saved value
pub fn restore_ipl(saved_ipl: u8) {
    lower_ipl(saved_ipl);
}

/// Get the current IPL
pub fn current_ipl() -> u8 {
    CURRENT_IPL.load(Ordering::Relaxed)
}

/// Update the APIC Task Priority Register
fn update_tpr(ipl: u8) {
    // TPR[7:4] = priority class, TPR[3:0] = sub-priority
    // Map our IPL to APIC priority classes
    let tpr_value = (ipl as u32) << 4;

    // Write to APIC TPR if available (use LAPIC_BASE from smp)
    let apic_base = crate::apic_timer::APIC_BASE.load(core::sync::atomic::Ordering::Relaxed);
    if apic_base != 0 {
        unsafe {
            let tpr_ptr = (apic_base + 0x80) as *mut u32; // TPR offset = 0x80
            core::ptr::write_volatile(tpr_ptr, tpr_value);
        }
    }
}

/// Enter a nested interrupt context
/// Returns the saved IPL for restore on exit
pub fn enter_nested_interrupt(irq_ipl: Ipl) -> u8 {
    let depth = NESTING_DEPTH.fetch_add(1, Ordering::Relaxed);
    if depth > 0 {
        NESTED_COUNT.fetch_add(1, Ordering::Relaxed);
    }

    // Track max depth
    let new_depth = depth + 1;
    let mut current_max = MAX_DEPTH_SEEN.load(Ordering::Relaxed);
    while new_depth > current_max {
        match MAX_DEPTH_SEEN.compare_exchange_weak(
            current_max,
            new_depth,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(v) => current_max = v,
        }
    }

    // Raise IPL and re-enable interrupts for higher-priority sources
    let saved = raise_ipl(irq_ipl);

    // Re-enable interrupts (allows nesting of higher-priority IRQs)
    if irq_ipl as u8 > 0 {
        crate::arch_compat::instructions::interrupts::enable();
    }

    saved
}

/// Exit a nested interrupt context
pub fn exit_nested_interrupt(saved_ipl: u8) {
    // Disable interrupts while we restore state
    crate::arch_compat::instructions::interrupts::disable();

    NESTING_DEPTH.fetch_sub(1, Ordering::Relaxed);
    restore_ipl(saved_ipl);
}

/// Get nesting statistics
pub fn nesting_stats() -> (u64, u8) {
    (
        NESTED_COUNT.load(Ordering::Relaxed),
        MAX_DEPTH_SEEN.load(Ordering::Relaxed),
    )
}

/// Get current nesting depth
pub fn nesting_depth() -> u8 {
    NESTING_DEPTH.load(Ordering::Relaxed)
}

/// Initialize nested interrupt support
pub fn init() {
    CURRENT_IPL.store(0, Ordering::Relaxed);
    NESTING_DEPTH.store(0, Ordering::Relaxed);
    serial_println!("[KnoxOS] Nested interrupt handling initialized");
}
