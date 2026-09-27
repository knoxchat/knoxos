use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

use crate::serial_println;

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
