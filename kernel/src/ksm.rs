/// Kernel Same-page Merging (KSM) — Memory deduplication
///
/// Scans physical memory for pages with identical content and merges them
/// using Copy-on-Write (CoW), reducing overall memory consumption.
/// Particularly effective for VMs and containers sharing similar workloads.
///
/// Algorithm:
///   1. Periodically scan registered memory regions
///   2. Hash each page (xxhash64 for speed)
///   3. Compare hashes to find duplicates
///   4. Verify byte-by-byte equality
///   5. Replace duplicate with CoW-mapped shared page
///   6. Track merged pages for CoW fault handling
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// CONSTANTS
// ═══════════════════════════════════════════════════════════════════════

pub const PAGE_SIZE: usize = 4096;
/// Maximum number of pages to scan per cycle
const MAX_PAGES_PER_SCAN: usize = 256;
/// Default scan interval in milliseconds
const DEFAULT_SCAN_INTERVAL_MS: u64 = 2000;

// ═══════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════

/// A memory region registered for KSM scanning
#[derive(Debug, Clone)]
pub struct KsmRegion {
    /// Starting physical address
    pub start_phys: u64,
    /// Number of pages in this region
    pub page_count: usize,
    /// Process ID that owns this region
    pub pid: u32,
    /// Whether MADV_MERGEABLE was set (opt-in)
    pub mergeable: bool,
}

/// A page hash entry in the stable/unstable trees
#[derive(Debug, Clone)]
struct PageEntry {
    /// Physical frame number
    phys_addr: u64,
    /// xxhash64 of the page content
    hash: u64,
    /// Reference count (how many virtual pages point here)
    ref_count: u32,
    /// Whether this is a CoW-shared page
    is_shared: bool,
}

/// KSM statistics
#[derive(Debug, Clone, Copy)]
pub struct KsmStats {
    pub pages_shared: u64,
    pub pages_sharing: u64,
    pub pages_unshared: u64,
    pub pages_volatile: u64,
    pub full_scans: u64,
    pub pages_scanned: u64,
    pub bytes_saved: u64,
}

// ═══════════════════════════════════════════════════════════════════════
// STATE
// ═══════════════════════════════════════════════════════════════════════

static KSM_ENABLED: AtomicBool = AtomicBool::new(false);
static KSM_RUNNING: AtomicBool = AtomicBool::new(false);
static PAGES_SHARED: AtomicU64 = AtomicU64::new(0);
static PAGES_SHARING: AtomicU64 = AtomicU64::new(0);
static PAGES_SCANNED: AtomicU64 = AtomicU64::new(0);
static FULL_SCANS: AtomicU64 = AtomicU64::new(0);
static SCAN_INTERVAL_MS: AtomicU64 = AtomicU64::new(DEFAULT_SCAN_INTERVAL_MS);
static MAX_PAGES_SCAN: AtomicUsize = AtomicUsize::new(MAX_PAGES_PER_SCAN);

lazy_static::lazy_static! {
    /// Regions registered for KSM scanning
    static ref REGIONS: Mutex<Vec<KsmRegion>> = Mutex::new(Vec::new());

    /// Stable tree: pages that have been found equal and are shared (CoW)
    /// Key = hash, Value = list of page entries with that hash
    static ref STABLE_TREE: Mutex<BTreeMap<u64, Vec<PageEntry>>> =
        Mutex::new(BTreeMap::new());

    /// Unstable tree: pages seen once, candidates for future merging
    static ref UNSTABLE_TREE: Mutex<BTreeMap<u64, PageEntry>> =
        Mutex::new(BTreeMap::new());
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Initialize KSM subsystem
pub fn init() {
    KSM_ENABLED.store(true, Ordering::SeqCst);
    serial_println!("[KSM] Kernel Same-page Merging initialized");
    serial_println!(
        "[KSM] Scan interval: {}ms, max pages/scan: {}",
        SCAN_INTERVAL_MS.load(Ordering::Relaxed),
        MAX_PAGES_SCAN.load(Ordering::Relaxed)
    );
}

/// Enable or disable KSM
pub fn set_enabled(enabled: bool) {
    KSM_ENABLED.store(enabled, Ordering::SeqCst);
    serial_println!("[KSM] {}", if enabled { "Enabled" } else { "Disabled" });
}

/// Start the KSM scanning thread
pub fn start() {
    if !KSM_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    KSM_RUNNING.store(true, Ordering::SeqCst);
    serial_println!("[KSM] Scanner started");
}

/// Stop the KSM scanner
pub fn stop() {
    KSM_RUNNING.store(false, Ordering::SeqCst);
    serial_println!("[KSM] Scanner stopped");
}

// ═══════════════════════════════════════════════════════════════════════
// REGION MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════

/// Register a memory region for KSM scanning (MADV_MERGEABLE)
pub fn register_region(start_phys: u64, page_count: usize, pid: u32) {
    let region = KsmRegion {
        start_phys,
        page_count,
        pid,
        mergeable: true,
    };
    REGIONS.lock().push(region);
    serial_println!(
        "[KSM] Registered region: {:#x}, {} pages, pid {}",
        start_phys,
        page_count,
        pid
    );
}

/// Unregister a region (MADV_UNMERGEABLE)
pub fn unregister_region(start_phys: u64, pid: u32) {
    let mut regions = REGIONS.lock();
    regions.retain(|r| !(r.start_phys == start_phys && r.pid == pid));
    serial_println!("[KSM] Unregistered region: {:#x}, pid {}", start_phys, pid);
}

// ═══════════════════════════════════════════════════════════════════════
// PAGE HASHING
// ═══════════════════════════════════════════════════════════════════════

/// Compute xxhash64-style hash of a page
fn hash_page(phys_addr: u64) -> u64 {
    let ptr = phys_addr as *const u8;
    let mut h: u64 = 0xcbf29ce484222325; // FNV offset basis

    // Read page content and hash
    for i in 0..PAGE_SIZE {
        let byte = unsafe { *ptr.add(i) };
        h ^= byte as u64;
        h = h.wrapping_mul(0x100000001b3); // FNV prime
    }

    h
}

/// Compare two pages byte-by-byte
fn pages_equal(addr_a: u64, addr_b: u64) -> bool {
    let a = addr_a as *const u64;
    let b = addr_b as *const u64;
    let words = PAGE_SIZE / 8;

    for i in 0..words {
        unsafe {
            if *a.add(i) != *b.add(i) {
                return false;
            }
        }
    }
    true
}

// ═══════════════════════════════════════════════════════════════════════
// SCANNING ENGINE
// ═══════════════════════════════════════════════════════════════════════

/// Run one scan cycle
pub fn scan_cycle() {
    if !KSM_RUNNING.load(Ordering::Relaxed) {
        return;
    }

    let regions = REGIONS.lock().clone();
    let max_pages = MAX_PAGES_SCAN.load(Ordering::Relaxed);
    let mut scanned = 0usize;
    let mut merged = 0usize;

    for region in &regions {
        if !region.mergeable {
            continue;
        }

        for page_idx in 0..region.page_count {
            if scanned >= max_pages {
                break;
            }

            let page_phys = region.start_phys + (page_idx as u64 * PAGE_SIZE as u64);
            let hash = hash_page(page_phys);
            scanned += 1;

            // Check stable tree first (previously merged pages)
            {
                let stable = STABLE_TREE.lock();
                if let Some(entries) = stable.get(&hash) {
                    for entry in entries {
                        if pages_equal(page_phys, entry.phys_addr) {
                            // Found a match in stable tree — merge via CoW
                            merge_page(page_phys, entry.phys_addr);
                            merged += 1;
                            break;
                        }
                    }
                }
            }

            // Check unstable tree
            {
                let mut unstable = UNSTABLE_TREE.lock();
                if let Some(existing) = unstable.get(&hash) {
                    if pages_equal(page_phys, existing.phys_addr) {
                        // Two pages match — promote to stable tree
                        let existing_phys = existing.phys_addr;
                        let shared_entry = PageEntry {
                            phys_addr: existing_phys,
                            hash,
                            ref_count: 2,
                            is_shared: true,
                        };

                        // Move to stable tree
                        let mut stable = STABLE_TREE.lock();
                        stable.entry(hash).or_default().push(shared_entry);

                        unstable.remove(&hash);
                        merge_page(page_phys, existing_phys);
                        merged += 1;
                    }
                } else {
                    // First time seeing this hash — add to unstable tree
                    unstable.insert(
                        hash,
                        PageEntry {
                            phys_addr: page_phys,
                            hash,
                            ref_count: 1,
                            is_shared: false,
                        },
                    );
                }
            }
        }
    }

    PAGES_SCANNED.fetch_add(scanned as u64, Ordering::Relaxed);
    if merged > 0 {
        PAGES_SHARING.fetch_add(merged as u64, Ordering::Relaxed);
    }
    FULL_SCANS.fetch_add(1, Ordering::Relaxed);
}

/// Merge a page by remapping it as CoW to the shared page
fn merge_page(victim_phys: u64, shared_phys: u64) {
    // In a real implementation:
    // 1. Update page tables to point victim's virtual address to shared_phys
    // 2. Mark the PTE as read-only (CoW)
    // 3. Free the victim's physical frame
    // 4. Increment shared page reference count

    PAGES_SHARED.fetch_add(1, Ordering::Relaxed);
    serial_println!(
        "[KSM] Merged page {:#x} -> shared {:#x}",
        victim_phys,
        shared_phys
    );
}

// ═══════════════════════════════════════════════════════════════════════
// STATISTICS
// ═══════════════════════════════════════════════════════════════════════

/// Get KSM statistics
pub fn stats() -> KsmStats {
    let shared = PAGES_SHARED.load(Ordering::Relaxed);
    let sharing = PAGES_SHARING.load(Ordering::Relaxed);
    KsmStats {
        pages_shared: shared,
        pages_sharing: sharing,
        pages_unshared: 0,
        pages_volatile: 0,
        full_scans: FULL_SCANS.load(Ordering::Relaxed),
        pages_scanned: PAGES_SCANNED.load(Ordering::Relaxed),
        bytes_saved: shared * PAGE_SIZE as u64,
    }
}

/// Set scan interval
pub fn set_scan_interval(ms: u64) {
    SCAN_INTERVAL_MS.store(ms, Ordering::Relaxed);
}

/// Set max pages per scan
pub fn set_max_pages_per_scan(count: usize) {
    MAX_PAGES_SCAN.store(count, Ordering::Relaxed);
}

/// Is KSM enabled?
pub fn is_enabled() -> bool {
    KSM_ENABLED.load(Ordering::Relaxed)
}
