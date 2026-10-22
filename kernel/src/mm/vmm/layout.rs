//! User virtual-address layout constants, ASLR, and page alignment.

/// User-space virtual address ranges
pub const USER_SPACE_START: u64 = 0x0000_0000_0010_0000;
pub const USER_SPACE_END: u64 = 0x0000_7FFF_FFFF_FFFF;

/// Program load base
pub const PROGRAM_BASE: u64 = 0x0000_0000_0040_0000;

/// Heap region
pub const HEAP_START: u64 = 0x0000_0000_4000_0000;
pub const HEAP_MAX: u64 = 0x0000_0000_C000_0000; // 2GB max heap

/// mmap region (top-down allocation)
pub const MMAP_REGION_START: u64 = 0x0000_2000_0000_0000;
pub const MMAP_REGION_END: u64 = 0x0000_7000_0000_0000;

/// Stack region
pub const STACK_TOP: u64 = 0x0000_7FFF_FFFF_0000;
pub const STACK_SIZE: u64 = 2 * 1024 * 1024; // 2MB default stack
pub const STACK_GUARD_PAGES: u64 = 1; // Guard page below stack

/// Page size
pub const PAGE_SIZE: u64 = 4096;

/// ASLR settings
pub const ASLR_ENABLED: bool = true;
pub const ASLR_PROGRAM_RANGE: u64 = 0x0000_0000_0100_0000; // 16 MiB range for program base
pub const ASLR_STACK_RANGE: u64 = 0x0000_0000_0200_0000; // 32 MiB range for stack
pub const ASLR_MMAP_RANGE: u64 = 0x0000_0100_0000_0000; // 1 TiB range for mmap
pub const ASLR_HEAP_RANGE: u64 = 0x0000_0000_1000_0000; // 256 MiB range for heap

/// Simple PRNG for ASLR (xorshift64)
static ASLR_SEED: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Initialize ASLR seed from TSC
pub fn init_aslr_seed() {
    let tsc = crate::arch_compat::read_tsc();
    ASLR_SEED.store(tsc, core::sync::atomic::Ordering::Relaxed);
}

/// Mix extra entropy into the ASLR stream (ChaCha20 `getrandom` once it exists).
pub fn reseed_aslr(extra: u64) {
    let mut seed = ASLR_SEED.load(core::sync::atomic::Ordering::Relaxed);
    seed ^= extra;
    if seed == 0 {
        seed = 0xA5A5_A5A5_5A5A_5A5A;
    }
    ASLR_SEED.store(seed, core::sync::atomic::Ordering::Relaxed);
}

/// Get a random page-aligned offset within [0, range)
fn aslr_offset(range: u64) -> u64 {
    if !ASLR_ENABLED || range == 0 {
        return 0;
    }
    let mut seed = ASLR_SEED.load(core::sync::atomic::Ordering::Relaxed);
    if crate::random::is_initialized() {
        seed ^= crate::random::random_u64();
    }
    if seed == 0 {
        seed = 0xDEADBEEF_CAFEBABE;
    }
    // xorshift64
    seed ^= seed << 13;
    seed ^= seed >> 7;
    seed ^= seed << 17;
    ASLR_SEED.store(seed, core::sync::atomic::Ordering::Relaxed);
    // Page-align within range
    let pages = range / PAGE_SIZE;
    if pages == 0 {
        return 0;
    }
    (seed % pages) * PAGE_SIZE
}

/// Get ASLR-randomized program base
pub fn aslr_program_base() -> u64 {
    PROGRAM_BASE + aslr_offset(ASLR_PROGRAM_RANGE)
}

/// Get ASLR-randomized stack top
pub fn aslr_stack_top() -> u64 {
    STACK_TOP - aslr_offset(ASLR_STACK_RANGE)
}

/// Get ASLR-randomized mmap end
pub fn aslr_mmap_end() -> u64 {
    MMAP_REGION_END - aslr_offset(ASLR_MMAP_RANGE)
}

/// Get ASLR-randomized heap start
pub fn aslr_heap_start() -> u64 {
    HEAP_START + aslr_offset(ASLR_HEAP_RANGE)
}

/// Align address down to page boundary
pub fn page_align_down(addr: u64) -> u64 {
    addr & !(PAGE_SIZE - 1)
}

/// Align address up to page boundary
pub fn page_align_up(addr: u64) -> u64 {
    (addr + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}
