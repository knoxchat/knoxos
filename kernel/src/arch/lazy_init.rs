//! Lazy Initialization — Deferred subsystem init for faster boot
//!
//! Wraps subsystem `init()` calls so they execute on first use rather than at
//! boot time. This reduces cold-boot latency by deferring initialization of
//! subsystems that aren't needed immediately (e.g., audio, Bluetooth, AI,
//! package manager) until their first API call.
//!
//! Usage:
//!   lazy_init!(alsa, crate::alsa::init());
//!   // Later, on first ALSA API call:
//!   alsa::ensure_init();  // No-op if already called

use crate::serial_println;
use core::sync::atomic::{AtomicBool, Ordering};

/// Macro to define a lazily-initialized subsystem.
///
/// Creates a module with `ensure_init()` that calls the real init function
/// exactly once, on first invocation.
#[macro_export]
macro_rules! lazy_init_subsystem {
    ($name:ident, $init_fn:expr) => {
        pub mod $name {
            use core::sync::atomic::{AtomicBool, Ordering};
            static INITIALIZED: AtomicBool = AtomicBool::new(false);

            /// Ensure the subsystem is initialized. No-op after first call.
            #[inline]
            pub fn ensure_init() {
                if !INITIALIZED.load(Ordering::Acquire) {
                    // Use compare_exchange to ensure only one thread inits
                    if INITIALIZED
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                    {
                        $init_fn;
                    }
                }
            }

            /// Check if the subsystem has been initialized
            #[inline]
            pub fn is_initialized() -> bool {
                INITIALIZED.load(Ordering::Acquire)
            }
        }
    };
}

/// Subsystems eligible for lazy initialization (not needed during early boot):
/// These were previously called unconditionally in kernel_main().
///
/// Critical path (must init at boot):
///   GDT, IDT, PIC, heap, VFS, scheduler, process, context, net, usermode
///
/// Deferrable (can init on first use):
///   alsa, ai, kpm, bluetooth, gamepad, compiler, chromium_sandbox,
///   app_store, federated, quic, nfs, container, vm
///
/// This module provides the list and a helper to check which ones are deferred.///
/// List of subsystem names that are deferred
pub const DEFERRED_SUBSYSTEMS: &[&str] = &[
    "alsa",
    "ai_suggest",
    "app_store",
    "bluetooth",
    "chromium_sandbox",
    "compiler",
    "container",
    "federated",
    "gamepad",
    "nfs",
    "quic",
    "vm",
];

/// Count how many deferred subsystems have been lazily initialized
pub fn deferred_init_count() -> usize {
    // This is a summary metric; individual modules track their own state.
    // We just count how many we know about for the system monitor.
    0 // Placeholder — individual modules report via their AtomicBool
}

pub fn init() {
    serial_println!(
        "[KnoxOS] Lazy init framework ready ({} subsystems deferrable)",
        DEFERRED_SUBSYSTEMS.len()
    );
}
