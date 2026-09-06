/// APIC Timer — Per-core local APIC timer for preemptive scheduling
///
/// Provides per-CPU timer interrupts via the Local APIC, enabling:
///   - Per-core tick-based preemption
///   - High-resolution one-shot timers for scheduling
///   - TSC deadline mode for precise wakeups
///   - Calibrated tick rate for consistent scheduling quanta
///
/// This replaces the PIT (IRQ0) for scheduling on APIC-enabled systems,
/// giving each CPU core its own independent timer interrupt.
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── Local APIC Timer Registers (offsets from APIC base) ────────────

/// APIC Timer LVT (Local Vector Table) register offset
const APIC_LVT_TIMER: usize = 0x320;
/// APIC Timer Initial Count register
const APIC_TIMER_INIT_COUNT: usize = 0x380;
/// APIC Timer Current Count register
const APIC_TIMER_CURRENT_COUNT: usize = 0x390;
/// APIC Timer Divide Configuration register
const APIC_TIMER_DIVIDE_CONFIG: usize = 0x3E0;

/// APIC Spurious Interrupt Vector Register
const APIC_SVR: usize = 0x0F0;
/// APIC EOI register
const APIC_EOI: usize = 0x0B0;

// Timer modes (bits 17-18 of LVT Timer)
const TIMER_MODE_ONESHOT: u32 = 0b00 << 17;
const TIMER_MODE_PERIODIC: u32 = 0b01 << 17;
const TIMER_MODE_TSC_DEADLINE: u32 = 0b10 << 17;

// LVT mask bit (bit 16)
const LVT_MASKED: u32 = 1 << 16;

// Divide values for APIC_TIMER_DIVIDE_CONFIG
const DIVIDE_BY_1: u32 = 0b1011;
const DIVIDE_BY_2: u32 = 0b0000;
const DIVIDE_BY_4: u32 = 0b0001;
const DIVIDE_BY_8: u32 = 0b0010;
const DIVIDE_BY_16: u32 = 0b0011;
const DIVIDE_BY_32: u32 = 0b1000;
const DIVIDE_BY_64: u32 = 0b1001;
const DIVIDE_BY_128: u32 = 0b1010;

/// Timer interrupt vector number (choose unused vector, e.g. 0x40)
pub const TIMER_VECTOR: u8 = 0x40;

/// Default scheduling quantum in microseconds (10ms = 100Hz)
const DEFAULT_QUANTUM_US: u64 = 10_000;

// ─── Global State ───────────────────────────────────────────────────

pub static APIC_BASE: AtomicU64 = AtomicU64::new(0);
static TICKS_PER_US: AtomicU32 = AtomicU32::new(0);
static INITIALIZED: AtomicBool = AtomicBool::new(false);
static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);

/// Per-CPU timer state
pub struct ApicTimerState {
    pub quantum_us: u64,
    pub ticks: u64,
    pub mode: TimerMode,
    pub calibrated_freq_hz: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerMode {
    OneShot,
    Periodic,
    TscDeadline,
}

lazy_static::lazy_static! {
    static ref TIMER_STATE: Mutex<ApicTimerState> = Mutex::new(ApicTimerState {
        quantum_us: DEFAULT_QUANTUM_US,
        ticks: 0,
        mode: TimerMode::Periodic,
        calibrated_freq_hz: 0,
    });
}

// ─── MMIO Access ────────────────────────────────────────────────────

unsafe fn apic_read(offset: usize) -> u32 {
    let base = APIC_BASE.load(Ordering::Relaxed);
    if base == 0 {
        return 0;
    }
    let ptr = (base + offset as u64) as *const u32;
    core::ptr::read_volatile(ptr)
}

unsafe fn apic_write(offset: usize, value: u32) {
    let base = APIC_BASE.load(Ordering::Relaxed);
    if base == 0 {
        return;
    }
    let ptr = (base + offset as u64) as *mut u32;
    core::ptr::write_volatile(ptr, value);
}

// ─── Calibration ────────────────────────────────────────────────────

/// Calibrate the APIC timer using PIT channel 2 as reference
/// Returns ticks per microsecond
fn calibrate_timer() -> u32 {
    // Use PIT to measure APIC timer frequency
    // PIT frequency = 1,193,182 Hz
    // We'll measure for ~10ms (11932 PIT ticks)
    const PIT_FREQ: u64 = 1_193_182;
    const CALIBRATION_MS: u64 = 10;
    const PIT_TICKS: u16 = (PIT_FREQ * CALIBRATION_MS / 1000) as u16;

    unsafe {
        // Set divide to 16
        apic_write(APIC_TIMER_DIVIDE_CONFIG, DIVIDE_BY_16);

        // Set initial count to max
        apic_write(APIC_TIMER_INIT_COUNT, 0xFFFF_FFFF);

        // Program PIT channel 2 for one-shot countdown
        #[cfg(target_arch = "x86_64")]
        use crate::arch_compat::instructions::port::Port;
        #[cfg(not(target_arch = "x86_64"))]
        use crate::arch_compat::instructions::port::Port;
        let mut pit_cmd: Port<u8> = Port::new(0x43);
        let mut pit_ch2: Port<u8> = Port::new(0x42);
        let mut pit_gate: Port<u8> = Port::new(0x61);

        // Gate on (bit 0), speaker off (bit 1 = 0)
        let gate = pit_gate.read();
        pit_gate.write((gate & 0xFD) | 0x01);

        // Channel 2, mode 0 (interrupt on terminal count), binary
        pit_cmd.write(0b10110000);

        // Load count
        pit_ch2.write((PIT_TICKS & 0xFF) as u8);
        pit_ch2.write((PIT_TICKS >> 8) as u8);

        // Wait for PIT to count down (bit 5 of port 0x61 goes high)
        // Add timeout to prevent infinite loop if PIT doesn't work
        let mut timeout: u32 = 100_000_000; // ~100M iterations ≈ a few seconds
        while pit_gate.read() & 0x20 == 0 {
            timeout -= 1;
            if timeout == 0 {
                // PIT calibration timed out — stop APIC timer and use fallback
                apic_write(APIC_TIMER_INIT_COUNT, 0);
                return 62; // Fallback ~1GHz bus / div16
            }
            core::hint::spin_loop();
        }

        // Read APIC timer current count
        let elapsed = 0xFFFF_FFFFu32 - apic_read(APIC_TIMER_CURRENT_COUNT);

        // Stop the timer
        apic_write(APIC_TIMER_INIT_COUNT, 0);

        // ticks_per_us = elapsed / (CALIBRATION_MS * 1000) / divide_factor
        let ticks_per_us = (elapsed as u64 / (CALIBRATION_MS * 1000)) as u32;
        if ticks_per_us == 0 {
            // Fallback: assume ~1GHz bus clock with div16 → ~62.5 ticks/us
            return 62;
        }
        ticks_per_us
    }
}

// ─── Public API ─────────────────────────────────────────────────────

/// Start the APIC timer in periodic mode with the given quantum
pub fn start_periodic(quantum_us: u64) {
    if !INITIALIZED.load(Ordering::Relaxed) {
        return;
    }

    let ticks_per_us = TICKS_PER_US.load(Ordering::Relaxed) as u64;
    if ticks_per_us == 0 {
        return;
    }

    let count = (quantum_us * ticks_per_us) as u32;

    unsafe {
        apic_write(APIC_TIMER_DIVIDE_CONFIG, DIVIDE_BY_16);
        apic_write(APIC_LVT_TIMER, TIMER_MODE_PERIODIC | TIMER_VECTOR as u32);
        apic_write(APIC_TIMER_INIT_COUNT, count);
    }

    let mut state = TIMER_STATE.lock();
    state.quantum_us = quantum_us;
    state.mode = TimerMode::Periodic;

    serial_println!(
        "[APIC Timer] Periodic mode: {}us quantum, count={}",
        quantum_us,
        count
    );
}

/// Start the APIC timer in one-shot mode
pub fn start_oneshot(timeout_us: u64) {
    if !INITIALIZED.load(Ordering::Relaxed) {
        return;
    }

    let ticks_per_us = TICKS_PER_US.load(Ordering::Relaxed) as u64;
    if ticks_per_us == 0 {
        return;
    }

    let count = (timeout_us * ticks_per_us) as u32;

    unsafe {
        apic_write(APIC_TIMER_DIVIDE_CONFIG, DIVIDE_BY_16);
        apic_write(APIC_LVT_TIMER, TIMER_MODE_ONESHOT | TIMER_VECTOR as u32);
        apic_write(APIC_TIMER_INIT_COUNT, count);
    }
}

/// Stop the APIC timer
pub fn stop() {
    if !INITIALIZED.load(Ordering::Relaxed) {
        return;
    }
    unsafe {
        apic_write(APIC_TIMER_INIT_COUNT, 0);
        apic_write(APIC_LVT_TIMER, LVT_MASKED);
    }
}

/// Send End-of-Interrupt to the local APIC
pub fn send_eoi() {
    unsafe {
        apic_write(APIC_EOI, 0);
    }
}

/// Handle APIC timer interrupt — called from IDT handler
pub fn handle_interrupt() {
    let apic_ticks = TIMER_TICKS.fetch_add(1, Ordering::Relaxed) + 1;

    // Increment the global TICK_COUNT that get_ticks() reads.
    // The PIT (IRQ0) no longer fires once the local APIC takes over
    // interrupt delivery, so we must drive TICK_COUNT from here.
    // APIC fires at 100Hz; PIT was ~18Hz. We increment every tick
    // and adjust thresholds in the redraw loop accordingly.
    crate::interrupts::increment_ticks();

    // Drive RTC monotonic counter (atomic, no locks)
    crate::rtc::tick();

    // Request cursor redraw periodically (~6Hz, similar to PIT's ticks%6)
    if apic_ticks % 17 == 0 {
        crate::gui::request_cursor_redraw();
    }

    // Only update timer state if the lock is not already held
    // (avoid deadlock if we interrupted code that holds TIMER_STATE)
    if let Some(mut state) = TIMER_STATE.try_lock() {
        state.ticks += 1;
    }

    // Use the ISR-safe (lock-free) timer tick instead of the full
    // scheduler::timer_tick() which acquires Mutex locks.
    // Acquiring spin::Mutex from interrupt context deadlocks if the
    // interrupted code on the same core already holds that lock.
    crate::scheduler::isr_timer_tick();

    send_eoi();
}

/// Get total timer ticks
pub fn total_ticks() -> u64 {
    TIMER_TICKS.load(Ordering::Relaxed)
}

/// Get the calibrated timer frequency in Hz
pub fn frequency_hz() -> u64 {
    TIMER_STATE.lock().calibrated_freq_hz
}

/// Check if APIC timer is initialized
pub fn is_initialized() -> bool {
    INITIALIZED.load(Ordering::Relaxed)
}

/// Initialize the APIC timer (calibrate only — do NOT start periodic mode).
///
/// Starting the periodic timer requires an IDT handler for vector 0x40.
/// Call `start_periodic()` only after the IDT entry is registered.
pub fn init() {
    // Get APIC base from MSR
    let mut lo: u32 = 0;
    let mut hi: u32 = 0;
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "rdmsr",
            in("ecx") 0x1Bu32,
            out("eax") lo,
            out("edx") hi,
        );
    }
    let apic_base_msr = ((hi as u64) << 32) | (lo as u64);
    // Check APIC enable bit (bit 11)
    if apic_base_msr & (1 << 11) == 0 {
        serial_println!("[APIC Timer] Local APIC not enabled, skipping");
        return;
    }
    // APIC base is bits [12:35] of the MSR, page-aligned
    let phys_base = apic_base_msr & 0xFFFF_F000;

    // Map through physical memory offset
    let phys_offset = crate::vmm::get_phys_mem_offset();
    if phys_offset == 0 {
        serial_println!("[APIC Timer] Physical memory not mapped, skipping");
        return;
    }
    let virt_base = phys_offset + phys_base;
    APIC_BASE.store(virt_base, Ordering::Relaxed);

    // Calibrate using PIT
    let ticks_per_us = calibrate_timer();
    TICKS_PER_US.store(ticks_per_us, Ordering::Relaxed);

    let freq_hz = ticks_per_us as u64 * 1_000_000 * 16; // ×16 for divide config
    TIMER_STATE.lock().calibrated_freq_hz = freq_hz;

    INITIALIZED.store(true, Ordering::Relaxed);

    // NOTE: Do NOT start the periodic timer here.
    // There is no IDT handler for vector 0x40 yet.
    // The scheduler will call start_periodic() when ready.

    serial_println!(
        "[KnoxOS] APIC timer calibrated: {} ticks/us, ~{} MHz bus (not started)",
        ticks_per_us,
        freq_hz / 1_000_000
    );
}
