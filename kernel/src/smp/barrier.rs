use core::sync::atomic::Ordering;

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
