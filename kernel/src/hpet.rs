/// HPET (High Precision Event Timer) Driver
///
/// Provides high-resolution timer functionality using the HPET hardware.
/// Supports periodic and one-shot timer modes with nanosecond precision.
/// Used for accurate timing, profiling, and kernel tick generation.
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── HPET Register Offsets ──────────────────────────────────────────

const HPET_GENERAL_CAPABILITIES: usize = 0x000;
const HPET_GENERAL_CONFIG: usize = 0x010;
const HPET_GENERAL_INT_STATUS: usize = 0x020;
const HPET_MAIN_COUNTER: usize = 0x0F0;

const HPET_TIMER_CONFIG_BASE: usize = 0x100;
const HPET_TIMER_COMPARATOR_BASE: usize = 0x108;
const HPET_TIMER_FSB_ROUTE_BASE: usize = 0x110;
const HPET_TIMER_STRIDE: usize = 0x020;

// General Configuration bits
const HPET_ENABLE_CNF: u64 = 1 << 0;
const HPET_LEG_RT_CNF: u64 = 1 << 1;

// Timer Configuration bits
const HPET_TN_INT_TYPE_CNF: u64 = 1 << 1; // Level-triggered
const HPET_TN_INT_ENB_CNF: u64 = 1 << 2; // Interrupt enable
const HPET_TN_TYPE_CNF: u64 = 1 << 3; // Periodic mode
const HPET_TN_PER_INT_CAP: u64 = 1 << 4; // Periodic capable
const HPET_TN_SIZE_CAP: u64 = 1 << 5; // 64-bit capable
const HPET_TN_VAL_SET_CNF: u64 = 1 << 6; // Force value set
const HPET_TN_32MODE_CNF: u64 = 1 << 8; // 32-bit mode
const HPET_TN_FSB_EN_CNF: u64 = 1 << 14; // FSB interrupt delivery
const HPET_TN_FSB_INT_DEL_CAP: u64 = 1 << 15; // FSB delivery capable

// ─── HPET Structures ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct HpetCapabilities {
    pub revision: u8,
    pub num_timers: u8,
    pub counter_64bit: bool,
    pub legacy_replacement: bool,
    pub vendor_id: u16,
    pub period_femtoseconds: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct HpetTimer {
    pub index: u8,
    pub periodic_capable: bool,
    pub size_64bit: bool,
    pub fsb_capable: bool,
    pub int_route_capabilities: u32,
    pub enabled: bool,
    pub periodic: bool,
    pub irq: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimerMode {
    OneShot,
    Periodic,
}

// ─── Global State ───────────────────────────────────────────────────

static HPET_BASE: AtomicU64 = AtomicU64::new(0);
static HPET_PERIOD_FS: AtomicU64 = AtomicU64::new(0);
static HPET_INITIALIZED: AtomicBool = AtomicBool::new(false);

lazy_static::lazy_static! {
    static ref HPET_CAPS: Mutex<Option<HpetCapabilities>> = Mutex::new(None);
    static ref HPET_TIMERS: Mutex<[Option<HpetTimer>; 8]> = Mutex::new([None; 8]);
    static ref TICK_COUNTER: AtomicU64 = AtomicU64::new(0);
}

// ─── MMIO Access ────────────────────────────────────────────────────

unsafe fn hpet_read(offset: usize) -> u64 {
    let base = HPET_BASE.load(Ordering::Relaxed);
    if base == 0 {
        return 0;
    }
    let ptr = (base + offset as u64) as *const u64;
    core::ptr::read_volatile(ptr)
}

unsafe fn hpet_write(offset: usize, value: u64) {
    let base = HPET_BASE.load(Ordering::Relaxed);
    if base == 0 {
        return;
    }
    let ptr = (base + offset as u64) as *mut u64;
    core::ptr::write_volatile(ptr, value);
}

// ─── HPET Functions ────────────────────────────────────────────────

/// Read HPET capabilities from the general capabilities register
pub fn read_capabilities() -> Option<HpetCapabilities> {
    if !HPET_INITIALIZED.load(Ordering::Relaxed) {
        return None;
    }

    let caps = unsafe { hpet_read(HPET_GENERAL_CAPABILITIES) };
    let period = (caps >> 32) as u32;

    Some(HpetCapabilities {
        revision: (caps & 0xFF) as u8,
        num_timers: ((caps >> 8) & 0x1F) as u8 + 1,
        counter_64bit: (caps >> 13) & 1 == 1,
        legacy_replacement: (caps >> 15) & 1 == 1,
        vendor_id: ((caps >> 16) & 0xFFFF) as u16,
        period_femtoseconds: period,
    })
}

/// Read timer N configuration
pub fn read_timer_config(timer_index: u8) -> Option<HpetTimer> {
    if !HPET_INITIALIZED.load(Ordering::Relaxed) {
        return None;
    }

    let offset = HPET_TIMER_CONFIG_BASE + (timer_index as usize) * HPET_TIMER_STRIDE;
    let config = unsafe { hpet_read(offset) };

    Some(HpetTimer {
        index: timer_index,
        periodic_capable: config & HPET_TN_PER_INT_CAP != 0,
        size_64bit: config & HPET_TN_SIZE_CAP != 0,
        fsb_capable: config & HPET_TN_FSB_INT_DEL_CAP != 0,
        int_route_capabilities: (config >> 32) as u32,
        enabled: config & HPET_TN_INT_ENB_CNF != 0,
        periodic: config & HPET_TN_TYPE_CNF != 0,
        irq: ((config >> 9) & 0x1F) as u8,
    })
}

/// Enable the HPET main counter
pub fn enable_counter() {
    if !HPET_INITIALIZED.load(Ordering::Relaxed) {
        return;
    }

    unsafe {
        let config = hpet_read(HPET_GENERAL_CONFIG);
        hpet_write(HPET_GENERAL_CONFIG, config | HPET_ENABLE_CNF);
    }

    serial_println!("[HPET] Main counter enabled");
}

/// Disable the HPET main counter
pub fn disable_counter() {
    if !HPET_INITIALIZED.load(Ordering::Relaxed) {
        return;
    }

    unsafe {
        let config = hpet_read(HPET_GENERAL_CONFIG);
        hpet_write(HPET_GENERAL_CONFIG, config & !HPET_ENABLE_CNF);
    }
}

/// Enable legacy replacement route (timer 0 → IRQ0/IRQ2, timer 1 → IRQ8)
pub fn enable_legacy_mode() {
    if !HPET_INITIALIZED.load(Ordering::Relaxed) {
        return;
    }

    unsafe {
        let config = hpet_read(HPET_GENERAL_CONFIG);
        hpet_write(HPET_GENERAL_CONFIG, config | HPET_LEG_RT_CNF);
    }

    serial_println!("[HPET] Legacy replacement mode enabled");
}

/// Read the main counter value
pub fn read_counter() -> u64 {
    if !HPET_INITIALIZED.load(Ordering::Relaxed) {
        return 0;
    }

    unsafe { hpet_read(HPET_MAIN_COUNTER) }
}

/// Reset the main counter to zero
pub fn reset_counter() {
    if !HPET_INITIALIZED.load(Ordering::Relaxed) {
        return;
    }

    unsafe {
        // Must disable counter before writing
        let config = hpet_read(HPET_GENERAL_CONFIG);
        hpet_write(HPET_GENERAL_CONFIG, config & !HPET_ENABLE_CNF);
        hpet_write(HPET_MAIN_COUNTER, 0);
        hpet_write(HPET_GENERAL_CONFIG, config | HPET_ENABLE_CNF);
    }
}

/// Configure and arm a timer
pub fn configure_timer(timer_index: u8, mode: TimerMode, comparator_value: u64, irq: u8) -> bool {
    if !HPET_INITIALIZED.load(Ordering::Relaxed) {
        return false;
    }

    let config_offset = HPET_TIMER_CONFIG_BASE + (timer_index as usize) * HPET_TIMER_STRIDE;
    let comparator_offset = HPET_TIMER_COMPARATOR_BASE + (timer_index as usize) * HPET_TIMER_STRIDE;

    unsafe {
        let mut config = hpet_read(config_offset);

        // Clear existing configuration
        config &=
            !(HPET_TN_INT_ENB_CNF | HPET_TN_TYPE_CNF | HPET_TN_VAL_SET_CNF | HPET_TN_32MODE_CNF);

        // Set interrupt enable
        config |= HPET_TN_INT_ENB_CNF;

        // Set IRQ routing (bits 13:9)
        config &= !(0x1F << 9);
        config |= (irq as u64 & 0x1F) << 9;

        // Set timer mode
        match mode {
            TimerMode::Periodic => {
                if config & HPET_TN_PER_INT_CAP == 0 {
                    serial_println!(
                        "[HPET] Timer {} does not support periodic mode",
                        timer_index
                    );
                    return false;
                }
                config |= HPET_TN_TYPE_CNF | HPET_TN_VAL_SET_CNF;
            }
            TimerMode::OneShot => {}
        }

        // Use level-triggered interrupts
        config |= HPET_TN_INT_TYPE_CNF;

        // Write configuration
        hpet_write(config_offset, config);

        // Set comparator value
        hpet_write(comparator_offset, comparator_value);
    }

    // Update timer state
    let timer = HpetTimer {
        index: timer_index,
        periodic_capable: true,
        size_64bit: true,
        fsb_capable: false,
        int_route_capabilities: 0,
        enabled: true,
        periodic: mode == TimerMode::Periodic,
        irq,
    };
    HPET_TIMERS.lock()[timer_index as usize] = Some(timer);

    serial_println!(
        "[HPET] Timer {} configured: {:?} mode, IRQ {}, comparator {}",
        timer_index,
        mode,
        irq,
        comparator_value
    );

    true
}

/// Disable a timer
pub fn disable_timer(timer_index: u8) {
    if !HPET_INITIALIZED.load(Ordering::Relaxed) {
        return;
    }

    let config_offset = HPET_TIMER_CONFIG_BASE + (timer_index as usize) * HPET_TIMER_STRIDE;

    unsafe {
        let config = hpet_read(config_offset);
        hpet_write(config_offset, config & !HPET_TN_INT_ENB_CNF);
    }

    HPET_TIMERS.lock()[timer_index as usize] = None;
}

/// Convert femtosecond period to frequency in Hz
pub fn frequency_hz() -> u64 {
    let period_fs = HPET_PERIOD_FS.load(Ordering::Relaxed);
    if period_fs == 0 {
        return 0;
    }
    // 1 second = 1e15 femtoseconds
    1_000_000_000_000_000 / period_fs
}

/// Convert counter ticks to nanoseconds
pub fn ticks_to_ns(ticks: u64) -> u64 {
    let period_fs = HPET_PERIOD_FS.load(Ordering::Relaxed);
    if period_fs == 0 {
        return 0;
    }
    ticks * period_fs / 1_000_000 // fs → ns
}

/// Convert nanoseconds to counter ticks
pub fn ns_to_ticks(ns: u64) -> u64 {
    let period_fs = HPET_PERIOD_FS.load(Ordering::Relaxed);
    if period_fs == 0 {
        return 0;
    }
    ns * 1_000_000 / period_fs // ns → fs → ticks
}

/// Get elapsed time in nanoseconds since HPET was enabled
pub fn elapsed_ns() -> u64 {
    ticks_to_ns(read_counter())
}

/// Busy-wait for a given number of nanoseconds
pub fn delay_ns(ns: u64) {
    if !HPET_INITIALIZED.load(Ordering::Relaxed) {
        return;
    }

    let start = read_counter();
    let ticks_needed = ns_to_ticks(ns);
    while read_counter().wrapping_sub(start) < ticks_needed {
        core::hint::spin_loop();
    }
}

/// Busy-wait for a given number of microseconds
pub fn delay_us(us: u64) {
    delay_ns(us * 1_000);
}

/// Busy-wait for a given number of milliseconds
pub fn delay_ms(ms: u64) {
    delay_ns(ms * 1_000_000);
}

/// Handle HPET timer interrupt
pub fn handle_interrupt(timer_index: u8) {
    TICK_COUNTER.fetch_add(1, Ordering::Relaxed);

    // Clear interrupt status
    if HPET_INITIALIZED.load(Ordering::Relaxed) {
        unsafe {
            let status = hpet_read(HPET_GENERAL_INT_STATUS);
            hpet_write(HPET_GENERAL_INT_STATUS, status | (1u64 << timer_index));
        }
    }
}

/// Get total timer ticks received
pub fn total_ticks() -> u64 {
    TICK_COUNTER.load(Ordering::Relaxed)
}

/// Initialize HPET with a given physical MMIO base address
pub fn init_with_address(phys_address: u64) {
    if phys_address == 0 {
        serial_println!("[HPET] No HPET base address provided");
        return;
    }

    // Convert physical address to virtual address via the physical memory offset
    let virt_address = phys_address + crate::vmm::get_phys_mem_offset();
    HPET_BASE.store(virt_address, Ordering::Relaxed);
    HPET_INITIALIZED.store(true, Ordering::Relaxed);

    // Read capabilities
    if let Some(caps) = read_capabilities() {
        HPET_PERIOD_FS.store(caps.period_femtoseconds as u64, Ordering::Relaxed);
        *HPET_CAPS.lock() = Some(caps);

        serial_println!(
            "[HPET] Rev {}, {} timers, {}bit counter, period {} fs",
            caps.revision,
            caps.num_timers,
            if caps.counter_64bit { 64 } else { 32 },
            caps.period_femtoseconds
        );
        serial_println!("[HPET] Frequency: {} Hz", frequency_hz());
        serial_println!("[HPET] Vendor: 0x{:04X}", caps.vendor_id);

        // Enumerate timers
        for i in 0..caps.num_timers.min(8) {
            if let Some(timer) = read_timer_config(i) {
                serial_println!(
                    "[HPET]   Timer {}: periodic={}, 64bit={}, fsb={}",
                    i,
                    timer.periodic_capable,
                    timer.size_64bit,
                    timer.fsb_capable
                );
            }
        }
    }

    // Enable the main counter
    enable_counter();
}

/// Initialize HPET (auto-detect from ACPI if available)
pub fn init() {
    serial_println!("[KnoxOS] HPET high precision event timer initialized");

    // Try to get HPET address from ACPI tables
    if let Some(hpet_addr) = crate::acpi_tables::hpet_address() {
        init_with_address(hpet_addr);
    } else {
        // Fallback: try well-known HPET address
        let default_base: u64 = 0xFED00000;
        serial_println!("[HPET] No ACPI HPET, trying default 0x{:X}", default_base);
        init_with_address(default_base);
    }
}
