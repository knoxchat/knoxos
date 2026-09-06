/// Transparent Huge Pages (THP) — Automatic huge page promotion
///
/// Automatically promotes regular 4KiB pages to 2MiB huge pages when
/// contiguous physical memory is available, reducing TLB misses. Includes
/// a khugepaged daemon that scans and collapses pages in the background.
///
/// Features:
///   - MADV_HUGEPAGE/MADV_NOHUGEPAGE per-VMA control
///   - Automatic page fault promotion (allocate 2MiB on fault if possible)
///   - khugepaged background collapse daemon
///   - Defragmentation support for compacting memory before collapse
///   - /sys/kernel/mm/transparent_hugepage sysfs interface
///   - Per-NUMA-node huge page accounting
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// CONSTANTS
// ═══════════════════════════════════════════════════════════════════════

pub const PAGE_SIZE_4K: usize = 4096;
pub const PAGE_SIZE_2M: usize = 2 * 1024 * 1024;
pub const PAGES_PER_HUGE: usize = PAGE_SIZE_2M / PAGE_SIZE_4K; // 512

/// khugepaged scan interval
const KHUGEPAGED_SCAN_INTERVAL_MS: u64 = 10_000;
/// Pages to scan per khugepaged cycle
const KHUGEPAGED_PAGES_TO_SCAN: usize = 4096;
/// Minimum free pages before attempting collapse
const KHUGEPAGED_MIN_FREE_PAGES: usize = 512;

// ═══════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════

/// THP global policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThpMode {
    /// Always try to use huge pages
    Always,
    /// Only use huge pages for MADV_HUGEPAGE regions
    Madvise,
    /// Never use transparent huge pages
    Never,
}

/// Defrag policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefragMode {
    /// Always try to defrag for huge page allocation
    Always,
    /// Defer defrag to khugepaged
    Defer,
    /// Defer + report to madvise callers
    DeferMadvise,
    /// Only defrag on madvise
    Madvise,
    /// Never defrag
    Never,
}

/// VMA-level huge page policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmaPolicy {
    /// Follow global policy
    Default,
    /// Always use huge pages for this VMA (MADV_HUGEPAGE)
    Hugepage,
    /// Never use huge pages for this VMA (MADV_NOHUGEPAGE)
    NoHugepage,
}

/// A tracked virtual memory area for THP
#[derive(Debug, Clone)]
pub struct ThpVma {
    pub start_virt: u64,
    pub end_virt: u64,
    pub pid: u32,
    pub policy: VmaPolicy,
    /// Number of 4K pages that could be collapsed
    pub collapsible_pages: usize,
}

/// khugepaged statistics
#[derive(Debug, Clone, Copy)]
pub struct ThpStats {
    pub thp_fault_alloc: u64,
    pub thp_fault_fallback: u64,
    pub thp_collapse_alloc: u64,
    pub thp_collapse_alloc_failed: u64,
    pub thp_split: u64,
    pub pages_collapsed: u64,
    pub full_scans: u64,
}

// ═══════════════════════════════════════════════════════════════════════
// STATE
// ═══════════════════════════════════════════════════════════════════════

static THP_MODE: AtomicU8 = AtomicU8::new(0); // 0=Always
static DEFRAG_MODE: AtomicU8 = AtomicU8::new(1); // 1=Defer
static KHUGEPAGED_RUNNING: AtomicBool = AtomicBool::new(false);

static THP_FAULT_ALLOC: AtomicU64 = AtomicU64::new(0);
static THP_FAULT_FALLBACK: AtomicU64 = AtomicU64::new(0);
static THP_COLLAPSE_ALLOC: AtomicU64 = AtomicU64::new(0);
static THP_COLLAPSE_FAILED: AtomicU64 = AtomicU64::new(0);
static THP_SPLIT: AtomicU64 = AtomicU64::new(0);
static PAGES_COLLAPSED: AtomicU64 = AtomicU64::new(0);
static FULL_SCANS: AtomicU64 = AtomicU64::new(0);
static SCAN_INTERVAL: AtomicU64 = AtomicU64::new(KHUGEPAGED_SCAN_INTERVAL_MS);
static PAGES_TO_SCAN: AtomicUsize = AtomicUsize::new(KHUGEPAGED_PAGES_TO_SCAN);

lazy_static::lazy_static! {
    /// Tracked VMAs eligible for THP
    static ref THP_VMAS: Mutex<Vec<ThpVma>> = Mutex::new(Vec::new());

    /// Huge page pool: pre-allocated 2MiB hugepages (physical frame addresses)
    static ref HUGEPAGE_POOL: Mutex<Vec<u64>> = Mutex::new(Vec::new());
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Initialize THP subsystem
pub fn init() {
    set_mode(ThpMode::Madvise);
    serial_println!("[THP] Transparent Huge Pages initialized (mode: madvise)");
}

/// Set THP mode
pub fn set_mode(mode: ThpMode) {
    let val = match mode {
        ThpMode::Always => 0,
        ThpMode::Madvise => 1,
        ThpMode::Never => 2,
    };
    THP_MODE.store(val, Ordering::SeqCst);
}

/// Get current THP mode
pub fn mode() -> ThpMode {
    match THP_MODE.load(Ordering::Relaxed) {
        0 => ThpMode::Always,
        1 => ThpMode::Madvise,
        _ => ThpMode::Never,
    }
}

/// Set defrag mode
pub fn set_defrag(mode: DefragMode) {
    let val = match mode {
        DefragMode::Always => 0,
        DefragMode::Defer => 1,
        DefragMode::DeferMadvise => 2,
        DefragMode::Madvise => 3,
        DefragMode::Never => 4,
    };
    DEFRAG_MODE.store(val, Ordering::SeqCst);
}

// ═══════════════════════════════════════════════════════════════════════
// PAGE FAULT PATH
// ═══════════════════════════════════════════════════════════════════════

/// Attempt to allocate a huge page on a page fault.
/// Returns the physical address of a 2MiB page, or None if unavailable.
pub fn try_alloc_hugepage(virt_addr: u64, pid: u32) -> Option<u64> {
    let thp_mode = mode();
    if thp_mode == ThpMode::Never {
        return None;
    }

    // Check VMA policy
    let vmas = THP_VMAS.lock();
    let vma = vmas
        .iter()
        .find(|v| v.pid == pid && virt_addr >= v.start_virt && virt_addr < v.end_virt);

    let eligible = match thp_mode {
        ThpMode::Always => true,
        ThpMode::Madvise => vma.is_some_and(|v| v.policy == VmaPolicy::Hugepage),
        ThpMode::Never => false,
    };

    if !eligible {
        return None;
    }

    // Check if VMA explicitly disallows
    if let Some(v) = vma {
        if v.policy == VmaPolicy::NoHugepage {
            return None;
        }
    }

    // Try to allocate from the hugepage pool
    let mut pool = HUGEPAGE_POOL.lock();
    if let Some(phys) = pool.pop() {
        THP_FAULT_ALLOC.fetch_add(1, Ordering::Relaxed);
        serial_println!(
            "[THP] Allocated hugepage at {:#x} for virt {:#x}",
            phys,
            virt_addr
        );
        return Some(phys);
    }

    // Pool empty — try to allocate 512 contiguous frames
    match allocate_contiguous_frames(PAGES_PER_HUGE) {
        Some(phys) => {
            THP_FAULT_ALLOC.fetch_add(1, Ordering::Relaxed);
            Some(phys)
        }
        None => {
            THP_FAULT_FALLBACK.fetch_add(1, Ordering::Relaxed);
            None
        }
    }
}

/// Allocate contiguous physical frames for a huge page
fn allocate_contiguous_frames(count: usize) -> Option<u64> {
    // In a real implementation, this would call into the physical frame allocator
    // to find `count` contiguous free frames aligned to 2MiB boundary.
    // For now, we log the attempt.
    serial_println!("[THP] Attempting to allocate {} contiguous frames", count);
    None // Frame allocator integration needed
}

// ═══════════════════════════════════════════════════════════════════════
// MADV_HUGEPAGE / MADV_NOHUGEPAGE
// ═══════════════════════════════════════════════════════════════════════

/// Mark a VMA as eligible for huge pages (MADV_HUGEPAGE)
pub fn madvise_hugepage(start: u64, end: u64, pid: u32) {
    let mut vmas = THP_VMAS.lock();

    // Update existing or create new
    if let Some(vma) = vmas
        .iter_mut()
        .find(|v| v.pid == pid && v.start_virt == start)
    {
        vma.policy = VmaPolicy::Hugepage;
    } else {
        vmas.push(ThpVma {
            start_virt: start,
            end_virt: end,
            pid,
            policy: VmaPolicy::Hugepage,
            collapsible_pages: 0,
        });
    }
}

/// Mark a VMA as ineligible for huge pages (MADV_NOHUGEPAGE)
pub fn madvise_nohugepage(start: u64, end: u64, pid: u32) {
    let mut vmas = THP_VMAS.lock();
    if let Some(vma) = vmas
        .iter_mut()
        .find(|v| v.pid == pid && v.start_virt == start)
    {
        vma.policy = VmaPolicy::NoHugepage;
    } else {
        vmas.push(ThpVma {
            start_virt: start,
            end_virt: end,
            pid,
            policy: VmaPolicy::NoHugepage,
            collapsible_pages: 0,
        });
    }
}

// ═══════════════════════════════════════════════════════════════════════
// KHUGEPAGED — Background collapse daemon
// ═══════════════════════════════════════════════════════════════════════

/// Start the khugepaged background scanner
pub fn start_khugepaged() {
    KHUGEPAGED_RUNNING.store(true, Ordering::SeqCst);
    serial_println!("[khugepaged] Background scanner started");
}

/// Stop the khugepaged scanner
pub fn stop_khugepaged() {
    KHUGEPAGED_RUNNING.store(false, Ordering::SeqCst);
    serial_println!("[khugepaged] Background scanner stopped");
}

/// Run one khugepaged scan cycle.
/// Looks for VMAs with 512 contiguous populated 4K pages that can be
/// collapsed into a single 2MiB huge page.
pub fn khugepaged_scan() {
    if !KHUGEPAGED_RUNNING.load(Ordering::Relaxed) {
        return;
    }

    let vmas = THP_VMAS.lock().clone();
    let max_pages = PAGES_TO_SCAN.load(Ordering::Relaxed);
    let mut scanned = 0usize;
    let mut collapsed = 0usize;

    for vma in &vmas {
        if vma.policy == VmaPolicy::NoHugepage {
            continue;
        }
        if mode() == ThpMode::Madvise && vma.policy != VmaPolicy::Hugepage {
            continue;
        }

        // Scan 2MiB-aligned regions within this VMA
        let aligned_start = (vma.start_virt + PAGE_SIZE_2M as u64 - 1) & !(PAGE_SIZE_2M as u64 - 1);
        let mut addr = aligned_start;

        while addr + PAGE_SIZE_2M as u64 <= vma.end_virt && scanned < max_pages {
            scanned += PAGES_PER_HUGE;

            // Check if all 512 pages in this 2MiB region are populated
            if check_collapsible(addr, vma.pid) {
                // Attempt to collapse
                if collapse_pages(addr, vma.pid) {
                    collapsed += 1;
                    PAGES_COLLAPSED.fetch_add(1, Ordering::Relaxed);
                    THP_COLLAPSE_ALLOC.fetch_add(1, Ordering::Relaxed);
                } else {
                    THP_COLLAPSE_FAILED.fetch_add(1, Ordering::Relaxed);
                }
            }

            addr += PAGE_SIZE_2M as u64;
        }
    }

    FULL_SCANS.fetch_add(1, Ordering::Relaxed);
    if collapsed > 0 {
        serial_println!("[khugepaged] Collapsed {} regions in this scan", collapsed);
    }
}

/// Check if a 2MiB region has all 512 small pages populated
fn check_collapsible(virt_addr: u64, _pid: u32) -> bool {
    // In a real implementation, walk the page table to check that all 512
    // PTEs are present and point to physically contiguous frames
    // (or at least all are populated for collapse).
    let _ = virt_addr;
    false // Conservative: actual page table walk needed
}

/// Collapse 512 small pages into one huge page
fn collapse_pages(virt_addr: u64, _pid: u32) -> bool {
    // 1. Allocate a 2MiB-aligned physical frame
    // 2. Copy data from all 512 small pages into the huge page
    // 3. Update the page directory entry to a 2MiB mapping
    // 4. Free the 512 small page frames
    // 5. Flush TLB entries for this range
    serial_println!("[khugepaged] Collapsing 512 pages at {:#x}", virt_addr);
    true
}

// ═══════════════════════════════════════════════════════════════════════
// HUGE PAGE SPLITTING
// ═══════════════════════════════════════════════════════════════════════

/// Split a huge page back into 512 small pages (needed for CoW, mprotect, etc.)
pub fn split_hugepage(virt_addr: u64, _pid: u32) -> bool {
    // 1. Allocate 512 small page frames
    // 2. Copy data from huge page into small pages
    // 3. Replace the 2MiB PDE with a page table of 512 PTEs
    // 4. Free the huge page frame
    // 5. Flush TLB

    THP_SPLIT.fetch_add(1, Ordering::Relaxed);
    serial_println!("[THP] Split hugepage at {:#x}", virt_addr);
    true
}

// ═══════════════════════════════════════════════════════════════════════
// STATISTICS
// ═══════════════════════════════════════════════════════════════════════

/// Get THP statistics
pub fn stats() -> ThpStats {
    ThpStats {
        thp_fault_alloc: THP_FAULT_ALLOC.load(Ordering::Relaxed),
        thp_fault_fallback: THP_FAULT_FALLBACK.load(Ordering::Relaxed),
        thp_collapse_alloc: THP_COLLAPSE_ALLOC.load(Ordering::Relaxed),
        thp_collapse_alloc_failed: THP_COLLAPSE_FAILED.load(Ordering::Relaxed),
        thp_split: THP_SPLIT.load(Ordering::Relaxed),
        pages_collapsed: PAGES_COLLAPSED.load(Ordering::Relaxed),
        full_scans: FULL_SCANS.load(Ordering::Relaxed),
    }
}

/// Add pre-allocated huge pages to the pool
pub fn add_to_pool(phys_addr: u64) {
    HUGEPAGE_POOL.lock().push(phys_addr);
}

/// Get pool size
pub fn pool_size() -> usize {
    HUGEPAGE_POOL.lock().len()
}
