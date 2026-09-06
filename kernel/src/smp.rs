use alloc::vec::Vec;
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
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// SMP-SAFE LOCKING PRIMITIVES
// ═══════════════════════════════════════════════════════════════════════

/// Ticket-based spinlock providing strict FIFO fairness
///
/// Unlike a simple spinlock, a ticket lock guarantees that waiters
/// acquire the lock in the order they requested it, preventing starvation.
pub struct TicketLock<T> {
    next_ticket: AtomicU32,
    now_serving: AtomicU32,
    data: spin::Mutex<T>,
}

impl<T> TicketLock<T> {
    pub const fn new(value: T) -> Self {
        Self {
            next_ticket: AtomicU32::new(0),
            now_serving: AtomicU32::new(0),
            data: spin::Mutex::new(value),
        }
    }

    /// Acquire the lock. Returns a guard that releases the lock when dropped.
    pub fn lock(&self) -> TicketLockGuard<'_, T> {
        let my_ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        let mut spins = 0u64;
        while self.now_serving.load(Ordering::Acquire) != my_ticket {
            core::hint::spin_loop();
            spins += 1;
            if spins > 10_000_000 {
                serial_println!(
                    "[LOCK] TicketLock contention: {} spins (ticket={})",
                    spins,
                    my_ticket
                );
                spins = 0;
            }
        }
        TicketLockGuard {
            lock: self,
            guard: self.data.lock(),
        }
    }

    /// Try to acquire the lock without waiting.
    pub fn try_lock(&self) -> Option<TicketLockGuard<'_, T>> {
        let current = self.now_serving.load(Ordering::Relaxed);
        if self
            .next_ticket
            .compare_exchange(current, current + 1, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(TicketLockGuard {
                lock: self,
                guard: self.data.lock(),
            })
        } else {
            None
        }
    }
}

unsafe impl<T: Send> Send for TicketLock<T> {}
unsafe impl<T: Send> Sync for TicketLock<T> {}

pub struct TicketLockGuard<'a, T> {
    lock: &'a TicketLock<T>,
    guard: spin::MutexGuard<'a, T>,
}

impl<'a, T> core::ops::Deref for TicketLockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.guard
    }
}

impl<'a, T> core::ops::DerefMut for TicketLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.guard
    }
}

impl<'a, T> Drop for TicketLockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.now_serving.fetch_add(1, Ordering::Release);
    }
}

/// Reader-Writer Spinlock for SMP
///
/// Allows multiple concurrent readers or one exclusive writer.
/// Uses an atomic counter: positive = reader count, -1 = writer held.
pub struct RwSpinLock<T> {
    state: AtomicI32,
    data: core::cell::UnsafeCell<T>,
}

use core::sync::atomic::AtomicI32;

impl<T> RwSpinLock<T> {
    pub const fn new(value: T) -> Self {
        Self {
            state: AtomicI32::new(0),
            data: core::cell::UnsafeCell::new(value),
        }
    }

    /// Acquire read lock. Multiple readers can hold simultaneously.
    pub fn read(&self) -> RwReadGuard<'_, T> {
        loop {
            let s = self.state.load(Ordering::Relaxed);
            if s >= 0
                && self
                    .state
                    .compare_exchange_weak(s, s + 1, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok()
            {
                return RwReadGuard { lock: self };
            }
            core::hint::spin_loop();
        }
    }

    /// Acquire write lock. Exclusive access.
    pub fn write(&self) -> RwWriteGuard<'_, T> {
        loop {
            if self
                .state
                .compare_exchange_weak(0, -1, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return RwWriteGuard { lock: self };
            }
            core::hint::spin_loop();
        }
    }
}

unsafe impl<T: Send> Send for RwSpinLock<T> {}
unsafe impl<T: Send + Sync> Sync for RwSpinLock<T> {}

pub struct RwReadGuard<'a, T> {
    lock: &'a RwSpinLock<T>,
}

impl<'a, T> core::ops::Deref for RwReadGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> Drop for RwReadGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.state.fetch_sub(1, Ordering::Release);
    }
}

pub struct RwWriteGuard<'a, T> {
    lock: &'a RwSpinLock<T>,
}

impl<'a, T> core::ops::Deref for RwWriteGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> core::ops::DerefMut for RwWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<'a, T> Drop for RwWriteGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.state.store(0, Ordering::Release);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// LOCK ORDERING & DEADLOCK DETECTION
// ═══════════════════════════════════════════════════════════════════════

/// Lock ordering classes (lower number = acquired first)
///
/// Enforcing a global lock order prevents ABBA-style deadlocks.
/// Locks must always be acquired in ascending order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum LockClass {
    Irq = 0,         // IRQ-disabled critical sections
    Scheduler = 10,  // Scheduler / run queue locks
    Memory = 20,     // Memory allocator
    Process = 30,    // Process table
    FileSystem = 40, // VFS / filesystem
    Network = 50,    // Network stack
    Device = 60,     // Device drivers
    User = 100,      // User-space facing locks
}

/// Lock order tracking (debug only)
static LOCK_AUDIT_ENABLED: AtomicBool = AtomicBool::new(false);
static LOCK_VIOLATIONS: AtomicU64 = AtomicU64::new(0);

/// Per-CPU held lock stack (simplified: tracks last class on BSP)
static HELD_LOCK_CLASS: AtomicU32 = AtomicU32::new(0);

/// Check lock ordering (should be called before acquiring a lock)
pub fn check_lock_order(class: LockClass) {
    if !LOCK_AUDIT_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let current = HELD_LOCK_CLASS.load(Ordering::Relaxed);
    let new = class as u32;
    if new < current && current != 0 {
        LOCK_VIOLATIONS.fetch_add(1, Ordering::Relaxed);
        serial_println!(
            "[LOCK AUDIT] WARNING: Lock order violation! Acquiring class {:?} ({}) while holding class {}",
            class,
            new,
            current
        );
    }
    HELD_LOCK_CLASS.store(new, Ordering::Relaxed);
}

/// Release a lock class (call after dropping a lock)
pub fn release_lock_class(_class: LockClass) {
    if LOCK_AUDIT_ENABLED.load(Ordering::Relaxed) {
        // Simplified: just reset. Full implementation would use a stack.
        HELD_LOCK_CLASS.store(0, Ordering::Relaxed);
    }
}

/// Enable lock auditing
pub fn enable_lock_audit() {
    LOCK_AUDIT_ENABLED.store(true, Ordering::Relaxed);
    serial_println!("[LOCK AUDIT] Lock order auditing enabled");
}

/// Get lock audit statistics
pub fn lock_audit_stats() -> u64 {
    LOCK_VIOLATIONS.load(Ordering::Relaxed)
}

// ═══════════════════════════════════════════════════════════════════════
// MEMORY BARRIERS & CPU FENCE
// ═══════════════════════════════════════════════════════════════════════

/// Full memory barrier (mfence)
#[inline(always)]
pub fn memory_barrier() {
    core::sync::atomic::fence(Ordering::SeqCst);
}

/// Store fence (sfence) — orders all stores before this point
#[inline(always)]
pub fn store_barrier() {
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("sfence", options(nostack, nomem));
    }
}

/// Load fence (lfence) — orders all loads before this point
#[inline(always)]
pub fn load_barrier() {
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("lfence", options(nostack, nomem));
    }
}

/// Disable interrupts and return previous state (for critical sections)
#[inline(always)]
pub fn irq_save() -> bool {
    let mut flags: u64 = 0;
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("pushfq; pop {}", out(reg) flags, options(nomem));
    }
    let was_enabled = flags & (1 << 9) != 0;
    if was_enabled {
        crate::arch_compat::instructions::interrupts::disable();
    }
    was_enabled
}

/// Restore interrupt state
#[inline(always)]
pub fn irq_restore(was_enabled: bool) {
    if was_enabled {
        crate::arch_compat::instructions::interrupts::enable();
    }
}

// ─── APIC Constants ─────────────────────────────────────────────────────

/// Default Local APIC base address (memory-mapped)
pub const LAPIC_BASE: u64 = 0xFEE0_0000;

/// Local APIC register offsets
pub const LAPIC_ID: u32 = 0x020; // APIC ID
pub const LAPIC_VERSION: u32 = 0x030; // APIC Version
pub const LAPIC_TPR: u32 = 0x080; // Task Priority Register
pub const LAPIC_EOI: u32 = 0x0B0; // End of Interrupt
pub const LAPIC_SVR: u32 = 0x0F0; // Spurious Interrupt Vector
pub const LAPIC_ESR: u32 = 0x280; // Error Status Register
pub const LAPIC_ICR_LO: u32 = 0x300; // Interrupt Command Register (low)
pub const LAPIC_ICR_HI: u32 = 0x310; // Interrupt Command Register (high)
pub const LAPIC_TIMER: u32 = 0x320; // Timer LVT
pub const LAPIC_LINT0: u32 = 0x350; // Local Interrupt 0
pub const LAPIC_LINT1: u32 = 0x360; // Local Interrupt 1
pub const LAPIC_ERROR_LVT: u32 = 0x370; // Error LVT
pub const LAPIC_TIMER_INIT: u32 = 0x380; // Timer Initial Count
pub const LAPIC_TIMER_CURRENT: u32 = 0x390; // Timer Current Count
pub const LAPIC_TIMER_DIVIDE: u32 = 0x3E0; // Timer Divide Configuration

/// APIC SVR bit: APIC enabled
pub const LAPIC_SVR_ENABLE: u32 = 0x100;
/// Spurious vector number
pub const SPURIOUS_VECTOR: u32 = 0xFF;

/// APIC Timer modes
pub const TIMER_PERIODIC: u32 = 0x20000; // Periodic mode
pub const TIMER_ONE_SHOT: u32 = 0x00000; // One-shot mode
pub const TIMER_MASKED: u32 = 0x10000; // Masked (disabled)

/// Timer divide values
pub const TIMER_DIVIDE_1: u32 = 0xB;
pub const TIMER_DIVIDE_2: u32 = 0x0;
pub const TIMER_DIVIDE_4: u32 = 0x1;
pub const TIMER_DIVIDE_8: u32 = 0x2;
pub const TIMER_DIVIDE_16: u32 = 0x3;
pub const TIMER_DIVIDE_32: u32 = 0x8;
pub const TIMER_DIVIDE_64: u32 = 0x9;
pub const TIMER_DIVIDE_128: u32 = 0xA;

/// Timer interrupt vector
pub const TIMER_VECTOR: u32 = 0x20; // Same as PIT for compatibility

/// IPI delivery modes
pub const IPI_INIT: u32 = 0x500; // INIT IPI
pub const IPI_STARTUP: u32 = 0x600; // Startup IPI (SIPI)
pub const IPI_FIXED: u32 = 0x000; // Fixed delivery
pub const IPI_ALL_EXCL: u32 = 0xC0000; // All excluding self

/// I/O APIC base address
pub const IOAPIC_BASE: u64 = 0xFEC0_0000;

/// I/O APIC registers
pub const IOAPIC_REG_SELECT: u32 = 0x00;
pub const IOAPIC_REG_DATA: u32 = 0x10;
pub const IOAPIC_REG_ID: u32 = 0x00;
pub const IOAPIC_REG_VERSION: u32 = 0x01;
pub const IOAPIC_RED_TABLE_BASE: u32 = 0x10; // Redirection table starts here

/// MSR addresses
pub const MSR_APIC_BASE: u32 = 0x1B;

// ─── Per-CPU Data ───────────────────────────────────────────────────────

/// Maximum number of CPUs supported
pub const MAX_CPUS: usize = 16;

/// Per-CPU data structure
#[derive(Debug)]
pub struct PerCpuData {
    /// APIC ID
    pub apic_id: u32,
    /// CPU index (0-based)
    pub cpu_index: u32,
    /// Is this the BSP (Bootstrap Processor)?
    pub is_bsp: bool,
    /// Is this CPU online?
    pub online: bool,
    /// Current process PID on this CPU
    pub current_pid: u32,
    /// Number of context switches on this CPU
    pub context_switches: u64,
    /// Number of timer interrupts
    pub timer_ticks: u64,
    /// CPU idle ticks
    pub idle_ticks: u64,
}

impl Default for PerCpuData {
    fn default() -> Self {
        Self::new()
    }
}

impl PerCpuData {
    pub const fn new() -> Self {
        Self {
            apic_id: 0,
            cpu_index: 0,
            is_bsp: false,
            online: false,
            current_pid: 0,
            context_switches: 0,
            timer_ticks: 0,
            idle_ticks: 0,
        }
    }
}

// ─── Global State ───────────────────────────────────────────────────────

/// Number of CPUs detected
static NUM_CPUS: AtomicU32 = AtomicU32::new(1);
/// Number of CPUs that have started
static CPUS_STARTED: AtomicU32 = AtomicU32::new(1);
/// Is APIC available and initialized?
static APIC_AVAILABLE: AtomicBool = AtomicBool::new(false);
/// Physical memory offset for MMIO access
static PHYS_OFFSET: AtomicU64 = AtomicU64::new(0);
/// APIC timer frequency (ticks per second)
static TIMER_FREQUENCY: AtomicU32 = AtomicU32::new(0);

lazy_static::lazy_static! {
    /// Per-CPU data array
    pub static ref CPU_DATA: Mutex<[PerCpuData; MAX_CPUS]> = {
        const INIT: PerCpuData = PerCpuData::new();
        Mutex::new([INIT; MAX_CPUS])
    };
}

// ─── Local APIC ─────────────────────────────────────────────────────────

/// Read a Local APIC register
unsafe fn lapic_read(offset: u32) -> u32 {
    let phys_offset = PHYS_OFFSET.load(Ordering::Relaxed);
    let addr = (phys_offset + LAPIC_BASE + offset as u64) as *const u32;
    core::ptr::read_volatile(addr)
}

/// Write a Local APIC register
unsafe fn lapic_write(offset: u32, value: u32) {
    let phys_offset = PHYS_OFFSET.load(Ordering::Relaxed);
    let addr = (phys_offset + LAPIC_BASE + offset as u64) as *mut u32;
    core::ptr::write_volatile(addr, value);
}

/// Wait for the ICR delivery status bit to clear (bit 12 of ICR_LO).
/// Must be called before writing a new IPI to the ICR.
unsafe fn wait_icr_idle() {
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
fn pit_wait_10ms() {
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

// ─── I/O APIC ───────────────────────────────────────────────────────────

/// Disable the legacy 8259 PIC by masking all IRQ lines.
/// Called after I/O APIC is configured to take over interrupt routing.
pub fn disable_legacy_pic() {
    unsafe {
        #[cfg(target_arch = "x86_64")]
        use crate::arch_compat::instructions::port::Port;
        #[cfg(not(target_arch = "x86_64"))]
        use crate::arch_compat::instructions::port::Port;
        let mut pic1_data: Port<u8> = Port::new(0x21);
        let mut pic2_data: Port<u8> = Port::new(0xA1);
        // Mask all IRQs on both PICs
        pic1_data.write(0xFF);
        pic2_data.write(0xFF);
    }
    serial_println!("[APIC] Legacy 8259 PIC disabled (all IRQs masked)");
}

/// Read an I/O APIC register
unsafe fn ioapic_read(reg: u32) -> u32 {
    let phys_offset = PHYS_OFFSET.load(Ordering::Relaxed);
    let base = phys_offset + IOAPIC_BASE;
    let select = base as *mut u32;
    let data = (base + IOAPIC_REG_DATA as u64) as *mut u32;
    core::ptr::write_volatile(select, reg);
    core::ptr::read_volatile(data)
}

/// Write an I/O APIC register
unsafe fn ioapic_write(reg: u32, value: u32) {
    let phys_offset = PHYS_OFFSET.load(Ordering::Relaxed);
    let base = phys_offset + IOAPIC_BASE;
    let select = base as *mut u32;
    let data = (base + IOAPIC_REG_DATA as u64) as *mut u32;
    core::ptr::write_volatile(select, reg);
    core::ptr::write_volatile(data, value);
}

/// Set up an I/O APIC redirection entry
pub fn ioapic_route_irq(irq: u8, vector: u8, dest_apic_id: u8) {
    unsafe {
        let reg_lo = IOAPIC_RED_TABLE_BASE + (irq as u32) * 2;
        let reg_hi = reg_lo + 1;

        // Low 32 bits: vector, delivery mode, etc.
        let lo = vector as u32; // Fixed delivery, physical destination, active high, edge-triggered
        // High 32 bits: destination APIC ID
        let hi = (dest_apic_id as u32) << 24;

        ioapic_write(reg_lo, lo);
        ioapic_write(reg_hi, hi);
    }
}

/// Initialize the I/O APIC
pub fn init_ioapic() {
    unsafe {
        let version = ioapic_read(IOAPIC_REG_VERSION);
        let max_redir = ((version >> 16) & 0xFF) as u8;
        let id = (ioapic_read(IOAPIC_REG_ID) >> 24) & 0xFF;

        serial_println!(
            "[IOAPIC] ID={}, version={:#x}, max redirections={}",
            id,
            version & 0xFF,
            max_redir + 1
        );

        // Mask all interrupts first
        for i in 0..=max_redir {
            let reg = IOAPIC_RED_TABLE_BASE + (i as u32) * 2;
            ioapic_write(reg, 0x10000); // Masked
        }

        // Route standard IRQs to BSP (matching PIC vector assignments)
        let bsp_id = get_apic_id() as u8;

        // Timer (IRQ 0) -> Vector 0x20
        ioapic_route_irq(0, 0x20, bsp_id);
        // Keyboard (IRQ 1) -> Vector 0x21
        ioapic_route_irq(1, 0x21, bsp_id);
        // Cascade (IRQ 2) - not needed with I/O APIC, leave masked
        // COM2 (IRQ 3) -> Vector 0x23
        ioapic_route_irq(3, 0x23, bsp_id);
        // COM1 (IRQ 4) -> Vector 0x24
        ioapic_route_irq(4, 0x24, bsp_id);
        // RTC (IRQ 8) -> Vector 0x28
        ioapic_route_irq(8, 0x28, bsp_id);
        // Virtio / PCI IRQ lines (IRQ 9, 10, 11)
        ioapic_route_irq(9, 0x29, bsp_id);
        ioapic_route_irq(10, 0x2A, bsp_id);
        ioapic_route_irq(11, 0x2B, bsp_id);
        // Mouse (IRQ 12) -> Vector 0x2C
        ioapic_route_irq(12, 0x2C, bsp_id);
        // ATA Primary (IRQ 14) -> Vector 0x2E
        ioapic_route_irq(14, 0x2E, bsp_id);
        // ATA Secondary (IRQ 15) -> Vector 0x2F
        ioapic_route_irq(15, 0x2F, bsp_id);
    }

    serial_println!("[IOAPIC] I/O APIC initialized, IRQs routed to BSP");
}

// ─── SMP — Application Processor Startup ────────────────────────────────

/// AP trampoline code location (must be < 1MB, page-aligned)
pub const AP_TRAMPOLINE_ADDR: u64 = 0x8000;

/// AP trampoline data area (parameters passed from BSP to AP)
pub const AP_DATA_ADDR: u64 = 0x7000;

/// AP trampoline stack base (each AP gets its own stack)
pub const AP_STACK_SIZE: usize = 16 * 1024; // 16KB stack per AP
pub const AP_STACK_BASE: u64 = 0x0000_0000_0080_0000; // 8MB mark

/// Flag set by AP to signal it's alive
static AP_ALIVE: AtomicBool = AtomicBool::new(false);

/// Real-mode → protected-mode → long-mode AP trampoline code
/// This machine code is copied to AP_TRAMPOLINE_ADDR (below 1MB)
/// The AP starts executing in 16-bit real mode at this address.
///
/// Layout at AP_DATA_ADDR (0x7000):
///   0x7000: u32 — cr3 (page table base, from BSP, low 32 bits)
///   0x7004: u32 — reserved (zero)
///   0x7008: u64 — 64-bit entry point (ap_entry_64)
///   0x7010: u64 — stack top for this AP
///   0x7018: u32 — APIC ID of this AP
///   0x701C: u32 — reserved (zero)
///   0x7020: GDT pointer (6 bytes: limit + base)
///   0x7026: GDT entries (null, code32, data32, code64, data64)
///   0x704E: far pointer for 16→32 jump (6 bytes: offset32 + selector16)
///   0x7054: far pointer for 32→64 jump (6 bytes: offset32 + selector16)
///
/// Trampoline byte layout (base = 0x8000):
///   Offset 0x00: 16-bit real-mode entry
///   Offset 0x22: 32-bit protected-mode code
///   Offset 0x6E: 64-bit long-mode code
///
/// Debug markers output to serial port 0x3F8:
///   'A' = AP entered real mode
///   'B' = AP entered protected mode
///   'C' = AP about to enable paging+longmode
///   'D' = AP reached 64-bit mode
///
/// IMPORTANT: In 32-bit and 64-bit modes, `mov edx, imm` (opcode 0xBA)
/// takes a 32-bit immediate (5 bytes total), NOT 16-bit.  All serial port
/// address loads must use the full `BA F8 03 00 00` encoding.
static AP_TRAMPOLINE_CODE: &[u8] = &[
    // ═══ 16-bit real mode (offset 0x00) ═══
    // [0x00]
    0xFA, //  cli
    // [0x01]
    0x31, 0xC0, //  xor ax, ax  (in 16-bit mode this is ax)
    // [0x03]
    0x8E, 0xD8, //  mov ds, ax
    // [0x05]
    0x8E, 0xC0, //  mov es, ax
    // [0x07]
    0x8E, 0xD0, //  mov ss, ax
    // [0x09]  Output 'A' to serial port 0x3F8
    0xB0, 0x41, //  mov al, 'A'
    0xBA, 0xF8, 0x03, //  mov dx, 0x3F8  (16-bit: BA = mov dx, imm16)
    0xEE, //  out dx, al
    // [0x0F]  Load GDT from AP_DATA_ADDR + 0x20 = 0x7020
    0x0F, 0x01, 0x16, 0x20, 0x70, //  lgdt [0x7020]
    // [0x14]  Enable protected mode: CR0.PE = 1
    0x0F, 0x20, 0xC0, //  mov eax, cr0
    // [0x17]
    0x0C, 0x01, //  or al, 1
    // [0x19]
    0x0F, 0x22, 0xC0, //  mov cr0, eax
    // [0x1C]  Indirect far jump to 32-bit code via pointer at [0x704E]
    //         The far pointer (offset32 + selector16) is written by
    //         install_trampoline() — safer than inline 66 EA in real mode.
    0x66, 0xFF, 0x2E, 0x4E, 0x70, //  jmp far [0x704E]  (o32 in 16-bit mode)
    // [0x21]  nop padding (not reached)
    0x90,
    // ═══ 32-bit protected mode (offset 0x22) ═══
    // [0x22]  Load data segments
    0x66, 0xB8, 0x10, 0x00, //  mov ax, 0x10  (data32 segment selector)
    // [0x26]
    0x8E, 0xD8, //  mov ds, ax
    // [0x28]
    0x8E, 0xC0, //  mov es, ax
    // [0x2A]
    0x8E, 0xD0, //  mov ss, ax
    // [0x2C]  Output 'B' to serial port 0x3F8
    //         NOTE: In 32-bit mode, BA = mov edx, imm32 (5 bytes, NOT 3!)
    0xB0, 0x42, //  mov al, 'B'
    0xBA, 0xF8, 0x03, 0x00, 0x00, //  mov edx, 0x000003F8
    // [0x33]
    0xEE, //  out dx, al
    // [0x34]  Enable PAE (CR4 bit 5)
    0x0F, 0x20, 0xE0, //  mov eax, cr4
    // [0x37]
    0x0D, 0x20, 0x00, 0x00, 0x00, //  or eax, 0x20
    // [0x3C]
    0x0F, 0x22, 0xE0, //  mov cr4, eax
    // [0x3F]  Load CR3 from [0x7000] (page table base)
    0xA1, 0x00, 0x70, 0x00, 0x00, //  mov eax, [0x7000]
    // [0x44]
    0x0F, 0x22, 0xD8, //  mov cr3, eax
    // [0x47]  Enable long mode via MSR IA32_EFER (0xC0000080), bit 8 (LME)
    //         Also enable NXE (bit 11) so the NX bit in page tables is valid.
    //         Without NXE, bit 63 in PTEs is "reserved" and causes #PF.
    0xB9, 0x80, 0x00, 0x00, 0xC0, //  mov ecx, 0xC0000080
    // [0x4C]
    0x0F, 0x32, //  rdmsr
    // [0x4E]
    0x0D, 0x00, 0x09, 0x00, 0x00, //  or eax, 0x900  (LME=bit8 | NXE=bit11)
    // [0x53]
    0x0F, 0x30, //  wrmsr
    // [0x55]  Output 'C' to serial port
    0xB0, 0x43, //  mov al, 'C'
    0xBA, 0xF8, 0x03, 0x00, 0x00, //  mov edx, 0x000003F8
    // [0x5C]
    0xEE, //  out dx, al
    // [0x5D]  Enable paging: CR0.PG = bit 31
    0x0F, 0x20, 0xC0, //  mov eax, cr0
    // [0x60]
    0x0D, 0x00, 0x00, 0x00, 0x80, //  or eax, 0x80000000
    // [0x65]
    0x0F, 0x22, 0xC0, //  mov cr0, eax
    // [0x68]  Indirect far jump to 64-bit code via pointer at [0x7054]
    //         Far pointer written by install_trampoline()
    0xFF, 0x2D, 0x54, 0x70, 0x00, 0x00, //  jmp far [0x7054]  (32-bit indirect)
    // ═══ 64-bit long mode (offset 0x6E) ═══
    // [0x6E]  Set up 64-bit data segments
    0x66, 0xB8, 0x20, 0x00, //  mov ax, 0x20  (data64 segment selector)
    // [0x72]
    0x8E, 0xD8, //  mov ds, ax
    // [0x74]
    0x8E, 0xC0, //  mov es, ax
    // [0x76]
    0x8E, 0xD0, //  mov ss, ax
    // [0x78]  Output 'D' to serial port
    //         NOTE: In 64-bit mode, BA = mov edx, imm32 (5 bytes, NOT 3!)
    0xB0, 0x44, //  mov al, 'D'
    0xBA, 0xF8, 0x03, 0x00, 0x00, //  mov edx, 0x000003F8
    // [0x7F]
    0xEE, //  out dx, al
    // [0x80]  Load stack from [0x7010]
    0x48, 0x8B, 0x24, 0x25, 0x10, 0x70, 0x00, 0x00, //  mov rsp, qword [0x7010]
    // [0x88]  Load 64-bit entry point from [0x7008]
    0x48, 0x8B, 0x04, 0x25, 0x08, 0x70, 0x00, 0x00, //  mov rax, qword [0x7008]
    // [0x90]  Load APIC ID argument from [0x7018]
    0x8B, 0x3C, 0x25, 0x18, 0x70, 0x00, 0x00, //  mov edi, dword [0x7018]
    // [0x97]  Jump to 64-bit Rust entry point
    0xFF, 0xE0, //  jmp rax
    // [0x99]  Halt fallback
    0xF4, //  hlt
    0xEB, 0xFD, //  jmp $-1
];

/// Set up the AP trampoline GDT at AP_DATA_ADDR + 0x20
/// GDT layout: null(0), code32(0x08), data32(0x10), code64(0x18), data64(0x20)
fn setup_trampoline_gdt(phys_offset: u64) {
    unsafe {
        let gdt_base = (phys_offset + AP_DATA_ADDR + 0x26) as *mut u64;
        let gdtr = (phys_offset + AP_DATA_ADDR + 0x20) as *mut u8;

        // GDT entries
        let gdt_entries: [u64; 5] = [
            0x0000_0000_0000_0000, // Null descriptor
            0x00CF_9A00_0000_FFFF, // 32-bit code segment (base=0, limit=4G, DPL=0, exec/read)
            0x00CF_9200_0000_FFFF, // 32-bit data segment (base=0, limit=4G, DPL=0, read/write)
            0x00AF_9A00_0000_FFFF, // 64-bit code segment (L=1, D=0, DPL=0, exec/read)
            0x00CF_9200_0000_FFFF, // 64-bit data segment (base=0, limit=4G, DPL=0, read/write)
        ];

        for (i, entry) in gdt_entries.iter().enumerate() {
            core::ptr::write_volatile(gdt_base.add(i), *entry);
        }

        // GDTR: 2-byte limit + 4-byte base (use 32-bit base for real/protected mode)
        let limit: u16 = (gdt_entries.len() * 8 - 1) as u16;
        core::ptr::write_volatile(gdtr as *mut u16, limit);
        let gdt_addr = (AP_DATA_ADDR + 0x26) as u32;
        core::ptr::write_volatile(gdtr.add(2) as *mut u32, gdt_addr);
    }
}

/// Identity-map the first 2 MiB of physical memory so the AP trampoline
/// code (running at physical addresses 0x7000-0x8FFF) can access itself
/// and its data area after paging is enabled with the BSP's CR3.
///
/// We walk the 4-level page table manually and insert a single 2 MiB
/// huge page entry (PDE with PS=1) mapping virtual 0..0x200000 →
/// physical 0..0x200000.
///
/// This is safe because:
///   - The first 2 MiB is typically unused in the virtual address space
///   - We only need it during AP startup
fn identity_map_low_memory(phys_offset: u64, cr3: u64) {
    unsafe {
        let pml4_phys = cr3 & !0xFFF; // strip flags
        let pml4 = (phys_offset + pml4_phys) as *mut u64;

        // PML4 entry 0 (covers virtual 0..512 GiB)
        let mut pml4e = core::ptr::read_volatile(pml4);
        let pdpt_phys;
        if pml4e & 1 != 0 {
            // Already present — use existing PDPT
            pdpt_phys = pml4e & 0x000F_FFFF_FFFF_F000;
        } else {
            // We need to allocate a PDPT frame. Use a fixed safe physical
            // address in low memory that we know is free (0x6000).
            // This is below our data area at 0x7000.
            pdpt_phys = 0x6000;
            // Zero the new page table
            let pdpt = (phys_offset + pdpt_phys) as *mut u8;
            core::ptr::write_bytes(pdpt, 0, 4096);
            // Present + Writable
            pml4e = pdpt_phys | 0x03;
            core::ptr::write_volatile(pml4, pml4e);
        }

        let pdpt = (phys_offset + pdpt_phys) as *mut u64;

        // PDPT entry 0 (covers virtual 0..1 GiB)
        let mut pdpte = core::ptr::read_volatile(pdpt);
        let pd_phys;
        if pdpte & 1 != 0 {
            if pdpte & 0x80 != 0 {
                // 1 GiB huge page already present — low memory is identity-mapped
                serial_println!("[SMP] Low memory already identity-mapped (1G page)");
                return;
            }
            pd_phys = pdpte & 0x000F_FFFF_FFFF_F000;
        } else {
            // Allocate PD at 0x5000
            pd_phys = 0x5000;
            let pd = (phys_offset + pd_phys) as *mut u8;
            core::ptr::write_bytes(pd, 0, 4096);
            pdpte = pd_phys | 0x03; // Present + Writable
            core::ptr::write_volatile(pdpt, pdpte);
        }

        let pd = (phys_offset + pd_phys) as *mut u64;

        // PD entry 0: map virtual 0..2 MiB → physical 0..2 MiB as a 2 MiB huge page
        let pde = core::ptr::read_volatile(pd);
        if pde & 0x83 == 0x83 {
            // Already a 2 MiB huge page at physical 0 — check it maps to phys 0
            let mapped_phys = pde & 0x000F_FFFF_FFE0_0000;
            if mapped_phys == 0 {
                serial_println!("[SMP] Low memory already identity-mapped (2M page)");
                return;
            }
        }

        // Either not present, or not a correct identity map.
        // (Over)write PDE entry 0 with a 2 MiB identity-map page.
        // 2 MiB page: Present(1) + Writable(2) + PageSize(0x80) = 0x83
        // Physical address = 0 (mapping physical 0..0x200000)
        core::ptr::write_volatile(pd, 0x0000_0000_0000_0083u64);

        // Flush TLB for the region
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("invlpg [{}]", in(reg) 0u64, options(nostack, preserves_flags));

        serial_println!("[SMP] Identity-mapped first 2 MiB for AP trampoline");
    }
}

/// Install AP trampoline code and data into low memory
pub fn install_trampoline(phys_offset: u64, cr3: u64) {
    unsafe {
        // Zero the entire data area first to avoid stale data
        let data_base_ptr = (phys_offset + AP_DATA_ADDR) as *mut u8;
        core::ptr::write_bytes(data_base_ptr, 0, 0x5E); // zero 0x7000..0x705D

        // Copy trampoline code to AP_TRAMPOLINE_ADDR
        let trampoline_dest = (phys_offset + AP_TRAMPOLINE_ADDR) as *mut u8;
        core::ptr::copy_nonoverlapping(
            AP_TRAMPOLINE_CODE.as_ptr(),
            trampoline_dest,
            AP_TRAMPOLINE_CODE.len(),
        );

        // Set up data area at AP_DATA_ADDR
        let data_base = phys_offset + AP_DATA_ADDR;
        // CR3 at offset 0x00 (32-bit — page table must be below 4GB)
        assert!(
            cr3 <= 0xFFFF_FFFF,
            "AP trampoline: CR3 {:#x} exceeds 32-bit range",
            cr3
        );
        core::ptr::write_volatile(data_base as *mut u32, cr3 as u32);
        // Entry point at offset 0x08 (set per-AP before SIPI)
        core::ptr::write_volatile(
            (data_base + 0x08) as *mut u64,
            ap_entry_64 as *const () as u64,
        );

        // Set up GDT
        setup_trampoline_gdt(phys_offset);

        // Set up far jump pointer for 16→32 bit transition at 0x704E
        // Format: offset32 (4 bytes) + selector16 (2 bytes)
        // Target: offset 0x22 within trampoline → physical 0x8022
        // Selector: 0x0008 (code32 GDT entry)
        core::ptr::write_volatile((data_base + 0x4E) as *mut u32, 0x0000_8022u32);
        core::ptr::write_volatile((data_base + 0x52) as *mut u16, 0x0008u16);

        // Set up far jump pointer for 32→64 bit transition at 0x7054
        // Format: offset32 (4 bytes) + selector16 (2 bytes)
        // Target: offset 0x6E within trampoline → physical 0x806E
        // Selector: 0x0018 (code64 GDT entry)
        core::ptr::write_volatile((data_base + 0x54) as *mut u32, 0x0000_806Eu32);
        core::ptr::write_volatile((data_base + 0x58) as *mut u16, 0x0018u16);

        // Ensure all writes are visible to other CPUs.
        // On x86, stores are ordered and caches are coherent (MESI), but
        // wbinvd guarantees write-back of all modified cache lines.  This
        // is cheap (we only touched ~256 bytes) and eliminates any doubt.
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("wbinvd", options(nomem, nostack));
    }

    serial_println!(
        "[SMP] AP trampoline installed at {:#x}, data at {:#x}",
        AP_TRAMPOLINE_ADDR,
        AP_DATA_ADDR
    );
}

/// Allocate a stack for an AP
///
/// The stack must be in a virtual address range that is mapped in the BSP's
/// page tables (which the AP shares).  The simplest way to guarantee this is
/// to allocate from the kernel heap, which is already mapped.
fn allocate_ap_stack(_cpu_index: u32) -> u64 {
    use alloc::alloc::{Layout, alloc};
    let layout = Layout::from_size_align(AP_STACK_SIZE, 16).unwrap();
    let ptr = unsafe { alloc(layout) };
    if ptr.is_null() {
        // Fallback to the old static scheme (may not be mapped)
        let stack_bottom = AP_STACK_BASE + (_cpu_index as u64) * AP_STACK_SIZE as u64;
        return stack_bottom + AP_STACK_SIZE as u64;
    }
    // Stack grows downward — return the TOP of the allocation
    (ptr as u64) + AP_STACK_SIZE as u64
}

/// Start an Application Processor
pub fn start_ap(apic_id: u32, cpu_index: u32) {
    serial_println!("[SMP] start_ap: APIC ID {}, index {}", apic_id, cpu_index);

    // Prepare AP-specific data
    let phys_offset = PHYS_OFFSET.load(Ordering::Relaxed);
    let stack_top = allocate_ap_stack(cpu_index);
    serial_println!("[SMP]   stack_top = {:#x}", stack_top);

    unsafe {
        let data_base = phys_offset + AP_DATA_ADDR;
        // Stack top at offset 0x10
        core::ptr::write_volatile((data_base + 0x10) as *mut u64, stack_top);
        // APIC ID at offset 0x18
        core::ptr::write_volatile((data_base + 0x18) as *mut u32, apic_id);
    }

    AP_ALIVE.store(false, Ordering::SeqCst);

    serial_println!("[SMP]   sending INIT IPI...");
    unsafe {
        // Send INIT IPI (level-triggered assert)
        // Bits: delivery mode INIT (0x500) | level assert (bit14=1, 0x4000)
        //       | trigger mode level (bit15=1, 0x8000) = 0xC500
        wait_icr_idle();
        lapic_write(LAPIC_ICR_HI, apic_id << 24);
        lapic_write(LAPIC_ICR_LO, 0x0000_C500); // INIT, level, assert

        // Short delay, then de-assert INIT
        for _ in 0..1_000u32 {
            core::hint::spin_loop();
        }
        wait_icr_idle();
        lapic_write(LAPIC_ICR_HI, apic_id << 24);
        lapic_write(LAPIC_ICR_LO, 0x0000_8500); // INIT, level, de-assert

        // Wait 10ms (Intel MP spec requirement after INIT)
        pit_wait_10ms();
        serial_println!(
            "[SMP]   sending SIPI (vector={:#x})...",
            AP_TRAMPOLINE_ADDR / 4096
        );

        // Send two STARTUP IPIs (SIPI)
        for _ in 0..2 {
            let sipi_vector = (AP_TRAMPOLINE_ADDR / 4096) as u32;
            wait_icr_idle();
            lapic_write(LAPIC_ICR_HI, apic_id << 24);
            lapic_write(LAPIC_ICR_LO, IPI_STARTUP | sipi_vector);

            // Wait ~200µs between SIPIs (Intel MP spec)
            for _ in 0..20_000u32 {
                core::hint::spin_loop();
            }
        }
    }

    serial_println!("[SMP]   waiting for AP alive signal...");
    // Wait for AP to signal it's alive (generous timeout for slower emulators)
    for _ in 0..5_000_000u32 {
        if AP_ALIVE.load(Ordering::SeqCst) {
            serial_println!(
                "[SMP] AP {} (APIC ID {}) started successfully",
                cpu_index,
                apic_id
            );
            return;
        }
        core::hint::spin_loop();
    }

    serial_println!(
        "[SMP] AP {} (APIC ID {}) did not respond (timeout, continuing)",
        cpu_index,
        apic_id
    );
}

/// 64-bit AP entry point — called from trampoline after mode switch
/// This runs on the AP's own stack in long mode.
///
/// All AP initialization is done here (not in a separate function) to
/// avoid issues with function prologues when the GDT/IDT haven't been
/// loaded yet.
extern "C" fn ap_entry_64(apic_id: u32) {
    // Load the kernel's GDT so segment selectors match the BSP.
    // Use init_ap() to skip TSS loading (BSP's ltr set the Busy bit).
    crate::gdt::init_ap();

    // Load the kernel's IDT so exceptions are handled properly.
    crate::interrupts::init_idt();

    // Initialize this AP's local APIC.
    init_lapic();

    // Set up per-CPU data
    let cpu_index = CPUS_STARTED.fetch_add(1, Ordering::Relaxed);
    {
        let mut cpus = CPU_DATA.lock();
        if (cpu_index as usize) < MAX_CPUS {
            cpus[cpu_index as usize] = PerCpuData {
                apic_id,
                cpu_index,
                is_bsp: false,
                online: true,
                current_pid: 0,
                context_switches: 0,
                timer_ticks: 0,
                idle_ticks: 0,
            };
        }
    }

    // Signal BSP that we're alive — BEFORE timer calibration so the BSP
    // doesn't time out during the 10ms calibration spin.
    AP_ALIVE.store(true, Ordering::SeqCst);

    // Initialize APIC timer on this AP
    init_timer(100); // 100 Hz

    serial_println!("[SMP] AP {} online (APIC ID {})", cpu_index, apic_id);

    // AP idle loop — will be scheduled by the per-CPU scheduler
    loop {
        let cpu_idx = cpu_index as usize;
        if let Some(pid) = dequeue_from_cpu(cpu_idx) {
            let mut cpus = CPU_DATA.lock();
            if let Some(cpu) = cpus.get_mut(cpu_idx) {
                cpu.current_pid = pid;
                cpu.context_switches += 1;
            }
            drop(cpus);
        }
        crate::arch_compat::instructions::interrupts::hlt();
    }
}

/// AP entry point (called when an AP starts up in 64-bit mode)
/// Kept as a public API entry point; delegates to ap_entry_64's logic.
pub fn ap_entry(_apic_id: u32) {
    // All logic is now in ap_entry_64 directly.
    // This stub is kept for any external callers.
}

// ─── Per-CPU Run Queues ─────────────────────────────────────────────────

/// Per-CPU run queue for SMP-aware scheduling
struct PerCpuRunQueue {
    queue: alloc::collections::VecDeque<u32>, // PIDs
    load: u64,                                // Approximate load metric
}

impl PerCpuRunQueue {
    const fn new() -> Self {
        Self {
            queue: alloc::collections::VecDeque::new(),
            load: 0,
        }
    }
}

lazy_static::lazy_static! {
    /// Per-CPU run queues (one per CPU)
    static ref PER_CPU_RUNQUEUES: Mutex<Vec<PerCpuRunQueue>> = {
        let mut queues = Vec::new();
        for _ in 0..MAX_CPUS {
            queues.push(PerCpuRunQueue {
                queue: alloc::collections::VecDeque::new(),
                load: 0,
            });
        }
        Mutex::new(queues)
    };
}

/// Enqueue a process on the least-loaded CPU
pub fn enqueue_balanced(pid: u32) {
    let mut queues = PER_CPU_RUNQUEUES.lock();
    let online = CPUS_STARTED.load(Ordering::Relaxed) as usize;
    let online = online.max(1).min(queues.len());

    // Find the CPU with the lowest load
    let mut min_load = u64::MAX;
    let mut target_cpu = 0;
    for i in 0..online {
        if queues[i].load < min_load {
            min_load = queues[i].load;
            target_cpu = i;
        }
    }

    queues[target_cpu].queue.push_back(pid);
    queues[target_cpu].load += 1;
}

/// Enqueue a process on a specific CPU
pub fn enqueue_on_cpu(cpu: usize, pid: u32) {
    let mut queues = PER_CPU_RUNQUEUES.lock();
    if cpu < queues.len() {
        queues[cpu].queue.push_back(pid);
        queues[cpu].load += 1;
    }
}

/// Dequeue the next process from a CPU's run queue
pub fn dequeue_from_cpu(cpu: usize) -> Option<u32> {
    let mut queues = PER_CPU_RUNQUEUES.lock();
    if cpu < queues.len() {
        if let Some(pid) = queues[cpu].queue.pop_front() {
            queues[cpu].load = queues[cpu].load.saturating_sub(1);
            return Some(pid);
        }
    }
    None
}

/// CPU load balancing — steal work from overloaded CPUs
/// Called periodically (e.g., every 100ms) from BSP timer
pub fn balance_load() {
    let mut queues = PER_CPU_RUNQUEUES.lock();
    let online = CPUS_STARTED.load(Ordering::Relaxed) as usize;
    let online = online.max(1).min(queues.len());

    if online < 2 {
        return; // Nothing to balance with one CPU
    }

    // Find most-loaded and least-loaded CPUs
    let mut max_load = 0u64;
    let mut max_cpu = 0;
    let mut min_load = u64::MAX;
    let mut min_cpu = 0;

    for i in 0..online {
        if queues[i].load > max_load {
            max_load = queues[i].load;
            max_cpu = i;
        }
        if queues[i].load < min_load {
            min_load = queues[i].load;
            min_cpu = i;
        }
    }

    // Steal half the difference if imbalance > 1
    if max_load > min_load + 1 && max_cpu != min_cpu {
        let steal_count = (max_load - min_load) / 2;
        for _ in 0..steal_count {
            if let Some(pid) = queues[max_cpu].queue.pop_back() {
                queues[min_cpu].queue.push_back(pid);
                queues[max_cpu].load = queues[max_cpu].load.saturating_sub(1);
                queues[min_cpu].load += 1;
            }
        }
    }
}

/// Get per-CPU load statistics
pub fn cpu_load_stats() -> Vec<(u32, u64, usize)> {
    let queues = PER_CPU_RUNQUEUES.lock();
    let cpus = CPU_DATA.lock();
    let online = CPUS_STARTED.load(Ordering::Relaxed) as usize;
    let mut stats = Vec::new();

    for i in 0..online.min(queues.len()) {
        stats.push((i as u32, queues[i].load, queues[i].queue.len()));
    }
    stats
}

/// Detect the number of CPUs via CPUID
pub fn detect_cpus() -> u32 {
    // Try ACPI MADT first — it lists exactly the CPUs the firmware describes.
    // The MADT is populated by QEMU to match the `-smp N` setting.
    if let Some(count) = detect_cpus_from_madt() {
        if count >= 1 {
            return count;
        }
    }

    // Fallback: CPUID max_logical_processor_ids reports the maximum the
    // package *could* support, which may be larger than the actual number
    // of vCPUs.  On QEMU with -smp 1 this would report 2 for AMD K8,
    // causing us to try to boot a non-existent AP.  Use 1 as safe default.
    1
}

/// Scan the ACPI MADT (Multiple APIC Description Table) for Local APIC
/// entries to determine the true CPU count.
fn detect_cpus_from_madt() -> Option<u32> {
    let phys_offset = PHYS_OFFSET.load(Ordering::Relaxed);
    if phys_offset == 0 {
        return None;
    }

    // Search for the RSDP in the EBDA (0x040E pointer) and BIOS area (0xE0000-0xFFFFF)
    let rsdp_addr = find_rsdp(phys_offset)?;

    // Read RSDP to find RSDT
    let rsdp = unsafe { &*((phys_offset + rsdp_addr) as *const AcpiRsdp) };
    if &rsdp.signature != b"RSD PTR " {
        return None;
    }
    let rsdt_addr = rsdp.rsdt_address as u64;

    // Read RSDT header
    let rsdt = unsafe { &*((phys_offset + rsdt_addr) as *const AcpiSdtHeader) };
    if &rsdt.signature != b"RSDT" {
        return None;
    }

    // Iterate RSDT entries (array of u32 physical pointers after the header)
    let entry_count = (rsdt.length as usize - core::mem::size_of::<AcpiSdtHeader>()) / 4;
    let entries = unsafe {
        core::slice::from_raw_parts(
            ((phys_offset + rsdt_addr) as usize + core::mem::size_of::<AcpiSdtHeader>())
                as *const u32,
            entry_count,
        )
    };

    for &entry_phys in entries {
        let hdr = unsafe { &*((phys_offset + entry_phys as u64) as *const AcpiSdtHeader) };
        if &hdr.signature == b"APIC" {
            // Found the MADT
            return parse_madt(phys_offset, entry_phys as u64, hdr.length);
        }
    }

    None
}

/// Parse the MADT to count Local APIC entries with the Enabled flag set.
fn parse_madt(phys_offset: u64, madt_phys: u64, total_len: u32) -> Option<u32> {
    let base = (phys_offset + madt_phys) as usize;
    // MADT header: SDT header (36 bytes) + local APIC address (4) + flags (4) = 44 bytes
    let mut offset = 44usize;
    let mut cpu_count = 0u32;

    while offset + 2 <= total_len as usize {
        let entry_type = unsafe { *(base.wrapping_add(offset) as *const u8) };
        let entry_len = unsafe { *(base.wrapping_add(offset + 1) as *const u8) } as usize;
        if entry_len < 2 {
            break;
        }

        if entry_type == 0 && entry_len >= 8 {
            // Type 0 = Processor Local APIC
            // Byte 4 = APIC ID, Byte 8 (offset+4) = flags
            let flags = unsafe { *((base + offset + 4) as *const u32) };
            // Bit 0 = Processor Enabled, Bit 1 = Online Capable
            if flags & 0x01 != 0 {
                cpu_count += 1;
            }
        }

        offset += entry_len;
    }

    if cpu_count > 0 { Some(cpu_count) } else { None }
}

/// Search for the ACPI RSDP signature in standard locations
fn find_rsdp(phys_offset: u64) -> Option<u64> {
    // Check EBDA pointer at 0x040E
    let ebda_seg = unsafe { *((phys_offset + 0x040E) as *const u16) } as u64;
    let ebda_base = ebda_seg << 4;
    if ebda_base > 0 && ebda_base < 0xA0000 {
        for addr in (ebda_base..ebda_base + 1024).step_by(16) {
            let sig = unsafe { &*((phys_offset + addr) as *const [u8; 8]) };
            if sig == b"RSD PTR " {
                return Some(addr);
            }
        }
    }

    // Search BIOS read-only area 0xE0000 - 0xFFFFF
    for addr in (0xE0000u64..0x100000).step_by(16) {
        let sig = unsafe { &*((phys_offset + addr) as *const [u8; 8]) };
        if sig == b"RSD PTR " {
            return Some(addr);
        }
    }

    None
}

/// ACPI RSDP structure (v1)
#[repr(C, packed)]
struct AcpiRsdp {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_address: u32,
}

/// ACPI SDT header (common to all ACPI tables)
#[repr(C, packed)]
struct AcpiSdtHeader {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
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

// ─── Public API ─────────────────────────────────────────────────────────

/// Is APIC available?
pub fn is_available() -> bool {
    APIC_AVAILABLE.load(Ordering::Relaxed)
}

/// Get number of CPUs
pub fn num_cpus() -> u32 {
    NUM_CPUS.load(Ordering::Relaxed)
}

/// Get number of online CPUs
pub fn online_cpus() -> u32 {
    CPUS_STARTED.load(Ordering::Relaxed)
}

/// Get CPU statistics
pub fn cpu_stats() -> Vec<(u32, u32, bool, u64, u64)> {
    let cpus = CPU_DATA.lock();
    let mut stats = Vec::new();
    for cpu in cpus.iter() {
        if cpu.online {
            stats.push((
                cpu.cpu_index,
                cpu.apic_id,
                cpu.is_bsp,
                cpu.timer_ticks,
                cpu.context_switches,
            ));
        }
    }
    stats
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

/// Initialize APIC and SMP subsystem
pub fn init(phys_mem_offset: u64) {
    PHYS_OFFSET.store(phys_mem_offset, Ordering::Relaxed);

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
    NUM_CPUS.store(num, Ordering::Relaxed);
    serial_println!("[APIC] Detected {} CPU(s)", num);

    // Initialize BSP APIC
    init_lapic();
    APIC_AVAILABLE.store(true, Ordering::Relaxed);

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
        unsafe { lapic_write(LAPIC_EOI, 0) };
    }

    // Initialize APIC timer (100 Hz for preemptive scheduling)
    init_timer(100);

    // Start Application Processors
    if num > 1 {
        serial_println!("[SMP] Starting {} Application Processor(s)...", num - 1);

        // Identity-map low memory for the AP trampoline
        let mut cr3: u64 = 0;
        unsafe {
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack));
        }
        identity_map_low_memory(phys_mem_offset, cr3);

        // Install AP trampoline code in low memory
        install_trampoline(phys_mem_offset, cr3);

        // Start each AP (APIC IDs 1..num-1, assuming sequential IDs)
        for i in 1..num {
            start_ap(i, i);
        }

        serial_println!(
            "[SMP] {} of {} AP(s) started successfully",
            CPUS_STARTED.load(Ordering::Relaxed) - 1,
            num - 1
        );
    }

    serial_println!("[APIC] APIC/SMP subsystem initialized (full APIC mode)");
    serial_println!("[APIC]   BSP APIC ID: {}", get_apic_id());
    serial_println!("[APIC]   Total CPUs: {}", num);
    serial_println!(
        "[APIC]   Online CPUs: {}",
        CPUS_STARTED.load(Ordering::Relaxed)
    );
    serial_println!("[APIC]   I/O APIC: enabled, PIC: masked");
}
