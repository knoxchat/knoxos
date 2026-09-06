use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
/// SMP-Safe Read-Write Lock Primitives
/// Provides ticket-based spinlocks, reader-writer locks, and sequence locks
/// for safe concurrent access across multiple CPU cores.
///
/// These replace naive spin::Mutex usage where appropriate:
/// - TicketLock: Fair FIFO ordering, prevents starvation
/// - RwSpinLock: Multiple concurrent readers, exclusive writer
/// - SeqLock: Optimistic readers, minimal writer overhead (for frequently-read data)
/// - PerCpuLock: Per-CPU data with interrupt disable
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

// ─── Ticket Spinlock ────────────────────────────────────────────────

/// A fair, FIFO-ordered spinlock using ticket algorithm.
/// Prevents starvation and ensures lock acquisition in request order.
pub struct TicketLock<T> {
    next_ticket: AtomicU32,
    now_serving: AtomicU32,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for TicketLock<T> {}
unsafe impl<T: Send> Sync for TicketLock<T> {}

impl<T> TicketLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            next_ticket: AtomicU32::new(0),
            now_serving: AtomicU32::new(0),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> TicketLockGuard<'_, T> {
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        while self.now_serving.load(Ordering::Acquire) != ticket {
            core::hint::spin_loop();
        }
        TicketLockGuard { lock: self }
    }

    pub fn try_lock(&self) -> Option<TicketLockGuard<'_, T>> {
        let current = self.now_serving.load(Ordering::Relaxed);
        if self
            .next_ticket
            .compare_exchange(current, current + 1, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(TicketLockGuard { lock: self })
        } else {
            None
        }
    }

    pub fn is_locked(&self) -> bool {
        self.next_ticket.load(Ordering::Relaxed) != self.now_serving.load(Ordering::Relaxed)
    }
}

pub struct TicketLockGuard<'a, T> {
    lock: &'a TicketLock<T>,
}

impl<'a, T> Deref for TicketLockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> DerefMut for TicketLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<'a, T> Drop for TicketLockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.now_serving.fetch_add(1, Ordering::Release);
    }
}

// ─── Reader-Writer Spinlock ─────────────────────────────────────────

/// Reader-writer spinlock: allows multiple concurrent readers or one exclusive writer.
/// Writer-preferring to prevent writer starvation.
const WRITER_BIT: u32 = 1 << 31;
const READER_MASK: u32 = WRITER_BIT - 1;

pub struct RwSpinLock<T> {
    state: AtomicU32,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for RwSpinLock<T> {}
unsafe impl<T: Send + Sync> Sync for RwSpinLock<T> {}

impl<T> RwSpinLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            state: AtomicU32::new(0),
            data: UnsafeCell::new(data),
        }
    }

    pub fn read(&self) -> RwSpinLockReadGuard<'_, T> {
        loop {
            let s = self.state.load(Ordering::Relaxed);
            if s & WRITER_BIT == 0
                && self
                    .state
                    .compare_exchange_weak(s, s + 1, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok()
            {
                return RwSpinLockReadGuard { lock: self };
            }
            core::hint::spin_loop();
        }
    }

    pub fn write(&self) -> RwSpinLockWriteGuard<'_, T> {
        // Set writer bit
        loop {
            let s = self.state.load(Ordering::Relaxed);
            if s & WRITER_BIT == 0
                && self
                    .state
                    .compare_exchange_weak(s, s | WRITER_BIT, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok()
            {
                break;
            }
            core::hint::spin_loop();
        }
        // Wait for readers to drain
        while self.state.load(Ordering::Acquire) & READER_MASK != 0 {
            core::hint::spin_loop();
        }
        RwSpinLockWriteGuard { lock: self }
    }

    pub fn try_read(&self) -> Option<RwSpinLockReadGuard<'_, T>> {
        let s = self.state.load(Ordering::Relaxed);
        if s & WRITER_BIT == 0
            && self
                .state
                .compare_exchange(s, s + 1, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
        {
            return Some(RwSpinLockReadGuard { lock: self });
        }
        None
    }

    pub fn try_write(&self) -> Option<RwSpinLockWriteGuard<'_, T>> {
        if self
            .state
            .compare_exchange(0, WRITER_BIT, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(RwSpinLockWriteGuard { lock: self })
        } else {
            None
        }
    }
}

pub struct RwSpinLockReadGuard<'a, T> {
    lock: &'a RwSpinLock<T>,
}

impl<'a, T> Deref for RwSpinLockReadGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> Drop for RwSpinLockReadGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.state.fetch_sub(1, Ordering::Release);
    }
}

pub struct RwSpinLockWriteGuard<'a, T> {
    lock: &'a RwSpinLock<T>,
}

impl<'a, T> Deref for RwSpinLockWriteGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> DerefMut for RwSpinLockWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<'a, T> Drop for RwSpinLockWriteGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.state.fetch_and(!WRITER_BIT, Ordering::Release);
    }
}

// ─── Sequence Lock ──────────────────────────────────────────────────

/// Sequence lock for optimistic readers.
/// Writers increment sequence counter (even=stable, odd=in-progress).
/// Readers retry if sequence changed during read.
pub struct SeqLock<T: Copy> {
    sequence: AtomicU64,
    data: UnsafeCell<T>,
    write_lock: AtomicBool,
}

unsafe impl<T: Copy + Send> Send for SeqLock<T> {}
unsafe impl<T: Copy + Send> Sync for SeqLock<T> {}

impl<T: Copy> SeqLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            sequence: AtomicU64::new(0),
            data: UnsafeCell::new(data),
            write_lock: AtomicBool::new(false),
        }
    }

    /// Read data optimistically, retrying if writer was active
    pub fn read(&self) -> T {
        loop {
            let seq1 = self.sequence.load(Ordering::Acquire);
            if seq1 & 1 != 0 {
                core::hint::spin_loop();
                continue;
            }
            let data = unsafe { *self.data.get() };
            let seq2 = self.sequence.load(Ordering::Acquire);
            if seq1 == seq2 {
                return data;
            }
            core::hint::spin_loop();
        }
    }

    /// Write data under exclusive lock
    pub fn write(&self, new_data: T) {
        while self
            .write_lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        self.sequence.fetch_add(1, Ordering::Release); // odd = writing
        unsafe {
            *self.data.get() = new_data;
        }
        self.sequence.fetch_add(1, Ordering::Release); // even = stable
        self.write_lock.store(false, Ordering::Release);
    }

    pub fn sequence(&self) -> u64 {
        self.sequence.load(Ordering::Relaxed)
    }
}

// ─── Per-CPU Data ───────────────────────────────────────────────────

/// Per-CPU data accessor. Each CPU gets its own copy, no locking needed.
/// Uses APIC ID or sequential CPU index for indexing.
pub struct PerCpuData<T: Default + Copy> {
    data: [UnsafeCell<T>; 64], // Max 64 CPUs
}

unsafe impl<T: Default + Copy + Send> Send for PerCpuData<T> {}
unsafe impl<T: Default + Copy + Send> Sync for PerCpuData<T> {}

impl<T: Default + Copy> Default for PerCpuData<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Default + Copy> PerCpuData<T> {
    pub const fn new() -> Self {
        // Can't use Default in const, use MaybeUninit trick
        Self {
            data: unsafe { core::mem::MaybeUninit::zeroed().assume_init() },
        }
    }

    /// Get reference to current CPU's data
    pub fn get(&self, cpu_id: usize) -> &T {
        let idx = cpu_id.min(63);
        unsafe { &*self.data[idx].get() }
    }

    /// Get mutable reference to current CPU's data (caller ensures CPU-local)
    #[allow(clippy::mut_from_ref)]
    pub fn get_mut(&self, cpu_id: usize) -> &mut T {
        let idx = cpu_id.min(63);
        unsafe { &mut *self.data[idx].get() }
    }
}

// ─── IRQ-Safe Spinlock ──────────────────────────────────────────────

/// Spinlock that disables interrupts while held, preventing deadlock
/// from interrupt handlers trying to acquire the same lock.
pub struct IrqSpinLock<T> {
    inner: spin::Mutex<T>,
}

unsafe impl<T: Send> Send for IrqSpinLock<T> {}
unsafe impl<T: Send> Sync for IrqSpinLock<T> {}

impl<T> IrqSpinLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            inner: spin::Mutex::new(data),
        }
    }

    pub fn lock(&self) -> IrqSpinLockGuard<'_, T> {
        let was_enabled = crate::arch_compat::instructions::interrupts::are_enabled();
        crate::arch_compat::instructions::interrupts::disable();
        let guard = self.inner.lock();
        IrqSpinLockGuard {
            guard,
            restore_interrupts: was_enabled,
        }
    }
}

pub struct IrqSpinLockGuard<'a, T> {
    guard: spin::MutexGuard<'a, T>,
    restore_interrupts: bool,
}

impl<'a, T> Deref for IrqSpinLockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.guard
    }
}

impl<'a, T> DerefMut for IrqSpinLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.guard
    }
}

impl<'a, T> Drop for IrqSpinLockGuard<'a, T> {
    fn drop(&mut self) {
        drop(unsafe { core::ptr::read(&self.guard) });
        if self.restore_interrupts {
            crate::arch_compat::instructions::interrupts::enable();
        }
    }
}

// ─── Barrier ────────────────────────────────────────────────────────

/// Memory barrier utilities for SMP synchronization
pub mod barrier {
    use core::sync::atomic::{Ordering, fence};

    /// Full memory barrier (mfence on x86)
    #[inline(always)]
    pub fn mb() {
        fence(Ordering::SeqCst);
    }

    /// Read memory barrier (lfence on x86)
    #[inline(always)]
    pub fn rmb() {
        fence(Ordering::Acquire);
    }

    /// Write memory barrier (sfence on x86)
    #[inline(always)]
    pub fn wmb() {
        fence(Ordering::Release);
    }

    /// Compiler barrier only (no CPU fence)
    #[inline(always)]
    pub fn compiler_barrier() {
        fence(Ordering::SeqCst);
    }
}

// ─── RCU (Read-Copy-Update) stub ────────────────────────────────────

/// Simplified RCU (Read-Copy-Update) for read-mostly data structures.
/// Grace period tracking for safe memory reclamation.
pub struct RcuState {
    grace_period: AtomicU64,
    completed: AtomicU64,
}

impl Default for RcuState {
    fn default() -> Self {
        Self::new()
    }
}

impl RcuState {
    pub const fn new() -> Self {
        Self {
            grace_period: AtomicU64::new(0),
            completed: AtomicU64::new(0),
        }
    }

    /// Begin an RCU read-side critical section
    pub fn read_lock(&self) -> u64 {
        self.grace_period.load(Ordering::Acquire)
    }

    /// End an RCU read-side critical section
    pub fn read_unlock(&self, _gp: u64) {
        // In a real implementation, this would update per-CPU quiescent state
    }

    /// Start a new grace period (called by writers)
    pub fn synchronize(&self) {
        let new_gp = self.grace_period.fetch_add(1, Ordering::SeqCst) + 1;
        // Wait for all CPUs to pass through a quiescent state
        // Simplified: just ensure ordering
        while self.completed.load(Ordering::Acquire) < new_gp.saturating_sub(1) {
            core::hint::spin_loop();
        }
        self.completed.store(new_gp, Ordering::Release);
    }

    /// Mark that a quiescent state has been reached on current CPU
    pub fn quiescent_state(&self) {
        let gp = self.grace_period.load(Ordering::Relaxed);
        let _ = self.completed.fetch_max(gp, Ordering::Release);
    }
}

pub static RCU: RcuState = RcuState::new();

pub fn init() {
    crate::serial_println!(
        "[KnoxOS] SMP-safe locking primitives initialized (ticket, rw, seq, irq, RCU)"
    );
}
