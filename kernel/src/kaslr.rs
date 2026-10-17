//! Kernel Address Space Layout Randomization.
//!
//! The running image is already loaded by the bootloader, so this randomizes
//! the kernel virtual base used for loadable `.ko` text and other late kernel
//! mappings (2 MiB aligned, non-zero slide).
use core::sync::atomic::{AtomicU64, Ordering};

use crate::serial_println;

/// Canonical kernel VA for loadable module text (sign-extended bit 47).
pub const MODULE_BASE: u64 = 0xFFFF_FFC0_0000_0000;
const SLIDE_GRANULE: u64 = 2 * 1024 * 1024;
const SLIDE_SLOTS: u64 = 128;

static SLIDE: AtomicU64 = AtomicU64::new(0);

pub const GATE_KASLR_MARKER: &str = "GATE_KASLR slide";

/// Draw a non-zero 2 MiB-aligned slide from the CSPRNG (TSC fallback).
pub fn init() {
    let mut entropy = crate::arch_compat::read_tsc();
    if crate::random::is_initialized() {
        entropy ^= crate::random::random_u64();
    }
    if entropy == 0 {
        entropy = 0xA5A5_5A5A_C3C3_3C3C;
    }
    let slot = (entropy % SLIDE_SLOTS) + 1;
    let slide = slot * SLIDE_GRANULE;
    SLIDE.store(slide, Ordering::SeqCst);
    serial_println!(
        "[kaslr] kernel module slide {:#x} (base {:#x})",
        slide,
        MODULE_BASE + slide
    );
}

pub fn slide() -> u64 {
    SLIDE.load(Ordering::Relaxed)
}

/// Randomized load address for a kernel module (2 MiB aligned).
pub fn module_load_base() -> u64 {
    MODULE_BASE.wrapping_add(slide())
}

pub fn self_test() -> bool {
    let s = slide();
    if s == 0 || (s & (SLIDE_GRANULE - 1)) != 0 {
        serial_println!("[kaslr] GATE_KASLR FAILED: slide={:#x}", s);
        return false;
    }
    serial_println!("[kaslr] {} ({:#x})", GATE_KASLR_MARKER, s);
    true
}
