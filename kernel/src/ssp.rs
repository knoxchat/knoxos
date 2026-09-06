/// SSP — Stack Smashing Protection (Stack Canaries)
///
/// Provides kernel-side stack guard support:
///   - Random canary generation using RDRAND/RDSEED
///   - Per-thread stack guard values
///   - __stack_chk_fail handler
///   - Canary verification helpers
///
/// For user-space, musl-libc provides its own __stack_chk_guard.
/// This module provides the kernel-side implementation.
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── Constants ─────────────────────────────────────────────────────────

/// Default canary value (used before RDRAND-based init)
const DEFAULT_CANARY: u64 = 0x0000_0D0A_0000_FF0A_0DFF; // Contains newlines/null for string overflow detection

/// Maximum tracked threads
const MAX_THREADS: usize = 1024;

// ─── Global canary ─────────────────────────────────────────────────────

/// The global stack canary value.
/// This is the value that `__stack_chk_guard` points to.
static STACK_CANARY: AtomicU64 = AtomicU64::new(DEFAULT_CANARY);

/// Per-CPU/thread canary (for future SMP per-thread guards)
static THREAD_CANARIES: Mutex<[u64; MAX_THREADS]> = Mutex::new([0u64; MAX_THREADS]);

// ─── RDRAND/RDSEED ─────────────────────────────────────────────────────

/// Check if RDRAND is supported
fn has_rdrand() -> bool {
    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
    cpuid
        .get_feature_info()
        .map(|f| f.has_rdrand())
        .unwrap_or(false)
}

/// Check if RDSEED is supported
fn has_rdseed() -> bool {
    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
    cpuid
        .get_extended_feature_info()
        .map(|f| f.has_rdseed())
        .unwrap_or(false)
}

/// Get a random u64 via RDRAND
fn rdrand64() -> Option<u64> {
    if !has_rdrand() {
        return None;
    }
    let mut val: u64 = 0;
    let mut ok: u8 = 0;
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "rdrand {val}",
            "setc {ok}",
            val = out(reg) val,
            ok = out(reg_byte) ok,
        );
    }
    if ok != 0 { Some(val) } else { None }
}

/// Get a random u64 via RDSEED (higher entropy)
fn rdseed64() -> Option<u64> {
    if !has_rdseed() {
        return None;
    }
    let mut val: u64 = 0;
    let mut ok: u8 = 0;
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "rdseed {val}",
            "setc {ok}",
            val = out(reg) val,
            ok = out(reg_byte) ok,
        );
    }
    if ok != 0 { Some(val) } else { None }
}

/// Generate a random canary value
fn generate_canary() -> u64 {
    // Prefer RDSEED > RDRAND > TSC-based
    if let Some(val) = rdseed64() {
        // Ensure lowest byte is 0x00 (catches string overflows)
        return val & !0xFF;
    }
    if let Some(val) = rdrand64() {
        return val & !0xFF;
    }
    // Fallback: TSC-based pseudo-random
    let tsc = crate::arch_compat::read_tsc();
    // Mix bits
    let mut v = tsc;
    v ^= v >> 33;
    v = v.wrapping_mul(0xFF51AFD7ED558CCD);
    v ^= v >> 33;
    v = v.wrapping_mul(0xC4CEB9FE1A85EC53);
    v ^= v >> 33;
    v & !0xFF
}

// ─── Stack guard symbol ────────────────────────────────────────────────

/// This is the symbol that GCC/LLVM `-fstack-protector` references.
/// It must be a global mutable with C linkage.
#[unsafe(no_mangle)]
pub static mut __stack_chk_guard: u64 = DEFAULT_CANARY;

/// Called by compiler-inserted code when a stack smash is detected.
#[unsafe(no_mangle)]
pub extern "C" fn __stack_chk_fail() -> ! {
    serial_println!("[SSP] *** STACK SMASHING DETECTED ***");

    // Get return address (approximate caller)
    let mut rip: u64 = 0;
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("lea {}, [rip]", out(reg) rip);
    }
    serial_println!("[SSP] Fault near RIP: {:#018x}", rip);

    // In a real kernel, we'd send SIGABRT to the process or panic
    panic!("stack smashing detected");
}

// ─── Per-thread canary management ──────────────────────────────────────

/// Set a unique canary for a given thread/process
pub fn set_thread_canary(tid: usize) {
    if tid >= MAX_THREADS {
        return;
    }
    let canary = generate_canary();
    let mut canaries = THREAD_CANARIES.lock();
    canaries[tid] = canary;
}

/// Get the canary for a given thread
pub fn get_thread_canary(tid: usize) -> u64 {
    if tid >= MAX_THREADS {
        return STACK_CANARY.load(Ordering::Relaxed);
    }
    let canaries = THREAD_CANARIES.lock();
    if canaries[tid] == 0 {
        STACK_CANARY.load(Ordering::Relaxed)
    } else {
        canaries[tid]
    }
}

/// Verify the canary for a given thread is intact
pub fn verify_thread_canary(tid: usize, value: u64) -> bool {
    let expected = get_thread_canary(tid);
    value == expected
}

// ─── DEP/NX enforcement helper ─────────────────────────────────────────

/// Check if NX bit is supported
pub fn has_nx_support() -> bool {
    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
    cpuid
        .get_extended_processor_and_feature_identifiers()
        .map(|f| f.has_execute_disable())
        .unwrap_or(false)
}

/// Ensure NX/XD bit is enabled in IA32_EFER MSR
pub fn enable_nx() {
    if !has_nx_support() {
        serial_println!("[SSP] NX/XD not supported by CPU");
        return;
    }
    unsafe {
        let efer = crate::arch_compat::registers::model_specific::Efer::read_raw();
        if efer & (1 << 11) == 0 {
            crate::arch_compat::registers::model_specific::Efer::write_raw(efer | (1 << 11));
            serial_println!("[SSP] NX/XD bit enabled in IA32_EFER");
        } else {
            serial_println!("[SSP] NX/XD bit already enabled");
        }
    }
}

// ─── Initialization ───────────────────────────────────────────────────

/// Initialize stack smashing protection
pub fn init() {
    // Generate a random canary
    let canary = generate_canary();
    STACK_CANARY.store(canary, Ordering::SeqCst);

    // Update the compiler-referenced symbol
    unsafe {
        __stack_chk_guard = canary;
    }

    serial_println!(
        "[SSP] Stack canary initialized (RDRAND={}  RDSEED={})",
        has_rdrand(),
        has_rdseed()
    );

    // Enable NX/XD for DEP
    enable_nx();

    serial_println!("[SSP] Stack protection subsystem ready");
}
