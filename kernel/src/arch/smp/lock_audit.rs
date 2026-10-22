use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use crate::serial_println;

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
