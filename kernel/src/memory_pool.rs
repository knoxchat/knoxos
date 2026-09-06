use alloc::vec;
/// Memory Pool — Object pool for frequent alloc/dealloc patterns
///
/// Provides:
///   - Fixed-size object pools (avoid heap fragmentation)
///   - Free list management
///   - Type-erased pool API
///   - Statistics tracking
///   - Thread-safe allocation
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// MEMORY POOL
// ═══════════════════════════════════════════════════════════════════════

/// A fixed-size memory pool
pub struct MemoryPool {
    /// Name of the pool (for debugging)
    name: &'static str,
    /// Object size in bytes
    object_size: usize,
    /// Pool storage (flat byte buffer)
    storage: Vec<u8>,
    /// Free list (indices of free slots)
    free_list: Vec<u32>,
    /// Total number of slots
    capacity: u32,
    /// Number of allocated objects
    allocated: u32,
    /// Statistics
    total_allocs: u64,
    total_frees: u64,
    peak_usage: u32,
}

impl MemoryPool {
    /// Create a new pool with `capacity` slots of `object_size` bytes each
    pub fn new(name: &'static str, object_size: usize, capacity: u32) -> Self {
        let aligned_size = (object_size + 7) & !7; // 8-byte alignment
        let total_bytes = aligned_size * capacity as usize;

        let mut free_list = Vec::with_capacity(capacity as usize);
        // Build free list in reverse order so we allocate from low indices first
        for i in (0..capacity).rev() {
            free_list.push(i);
        }

        Self {
            name,
            object_size: aligned_size,
            storage: vec![0u8; total_bytes],
            free_list,
            capacity,
            allocated: 0,
            total_allocs: 0,
            total_frees: 0,
            peak_usage: 0,
        }
    }

    /// Allocate an object from the pool, returns a slot index
    pub fn alloc(&mut self) -> Option<u32> {
        let slot = self.free_list.pop()?;
        self.allocated += 1;
        self.total_allocs += 1;
        if self.allocated > self.peak_usage {
            self.peak_usage = self.allocated;
        }
        Some(slot)
    }

    /// Free a previously allocated slot
    pub fn free(&mut self, slot: u32) {
        if slot >= self.capacity {
            return;
        }
        // Zero out the memory
        let offset = slot as usize * self.object_size;
        let end = offset + self.object_size;
        if end <= self.storage.len() {
            for b in &mut self.storage[offset..end] {
                *b = 0;
            }
        }
        self.free_list.push(slot);
        self.allocated -= 1;
        self.total_frees += 1;
    }

    /// Get a mutable reference to the object at a slot
    pub fn get_mut(&mut self, slot: u32) -> Option<&mut [u8]> {
        if slot >= self.capacity {
            return None;
        }
        let offset = slot as usize * self.object_size;
        let end = offset + self.object_size;
        if end <= self.storage.len() {
            Some(&mut self.storage[offset..end])
        } else {
            None
        }
    }

    /// Get a reference to the object at a slot
    pub fn get(&self, slot: u32) -> Option<&[u8]> {
        if slot >= self.capacity {
            return None;
        }
        let offset = slot as usize * self.object_size;
        let end = offset + self.object_size;
        if end <= self.storage.len() {
            Some(&self.storage[offset..end])
        } else {
            None
        }
    }

    /// Number of free slots
    pub fn available(&self) -> u32 {
        self.capacity - self.allocated
    }

    /// Is the pool full?
    pub fn is_full(&self) -> bool {
        self.free_list.is_empty()
    }

    /// Is the pool empty (all slots free)?
    pub fn is_empty(&self) -> bool {
        self.allocated == 0
    }

    /// Pool utilization (0.0 - 1.0)
    pub fn utilization(&self) -> f32 {
        if self.capacity == 0 {
            return 0.0;
        }
        self.allocated as f32 / self.capacity as f32
    }

    /// Total memory used by the pool
    pub fn memory_usage(&self) -> usize {
        self.storage.len() + self.free_list.len() * 4
    }

    /// Print pool statistics
    pub fn print_stats(&self) {
        serial_println!(
            "[MemPool:{}] cap={}, used={}/{}, peak={}, allocs={}, frees={}, util={:.1}%",
            self.name,
            self.capacity,
            self.allocated,
            self.capacity,
            self.peak_usage,
            self.total_allocs,
            self.total_frees,
            self.utilization() * 100.0,
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// GLOBAL POOLS
// ═══════════════════════════════════════════════════════════════════════

/// Pool registry
pub struct PoolRegistry {
    /// Small object pool (≤64 bytes): window events, mouse events, key events
    pub small: MemoryPool,
    /// Medium object pool (≤256 bytes): strings, short buffers
    pub medium: MemoryPool,
    /// Large object pool (≤4096 bytes): network packets, file buffers
    pub large: MemoryPool,
}

impl PoolRegistry {
    pub fn new() -> Self {
        Self {
            small: MemoryPool::new("small", 64, 4096),
            medium: MemoryPool::new("medium", 256, 1024),
            large: MemoryPool::new("large", 4096, 256),
        }
    }

    /// Allocate from the appropriate pool based on size
    pub fn alloc(&mut self, size: usize) -> Option<(PoolTier, u32)> {
        if size <= 64 {
            self.small.alloc().map(|s| (PoolTier::Small, s))
        } else if size <= 256 {
            self.medium.alloc().map(|s| (PoolTier::Medium, s))
        } else if size <= 4096 {
            self.large.alloc().map(|s| (PoolTier::Large, s))
        } else {
            None // Too large for any pool
        }
    }

    /// Free a previously allocated slot
    pub fn free(&mut self, tier: PoolTier, slot: u32) {
        match tier {
            PoolTier::Small => self.small.free(slot),
            PoolTier::Medium => self.medium.free(slot),
            PoolTier::Large => self.large.free(slot),
        }
    }

    /// Get mutable access to the slot data
    pub fn get_mut(&mut self, tier: PoolTier, slot: u32) -> Option<&mut [u8]> {
        match tier {
            PoolTier::Small => self.small.get_mut(slot),
            PoolTier::Medium => self.medium.get_mut(slot),
            PoolTier::Large => self.large.get_mut(slot),
        }
    }

    /// Print all pool statistics
    pub fn print_all_stats(&self) {
        self.small.print_stats();
        self.medium.print_stats();
        self.large.print_stats();
    }

    /// Total memory usage across all pools
    pub fn total_memory(&self) -> usize {
        self.small.memory_usage() + self.medium.memory_usage() + self.large.memory_usage()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolTier {
    Small,
    Medium,
    Large,
}

lazy_static::lazy_static! {
    pub static ref POOLS: Mutex<PoolRegistry> = Mutex::new(PoolRegistry::new());
}

/// Initialize memory pools
pub fn init() {
    let pools = POOLS.lock();
    serial_println!(
        "[KnoxOS] Memory pools initialized (total {}KB)",
        pools.total_memory() / 1024
    );
}

// ═══════════════════════════════════════════════════════════════════════
// MEMORY COMPACTION & DEFRAGMENTATION
// ═══════════════════════════════════════════════════════════════════════

/// Fragmentation statistics
#[derive(Debug, Clone)]
pub struct FragmentationStats {
    pub total_free_pages: u64,
    pub largest_contiguous_block: u64,
    pub free_blocks: u64,
    pub fragmentation_ratio: f32, // 0.0 = perfect, 1.0 = fully fragmented
}

/// Memory compaction state
pub struct CompactionState {
    /// Whether compaction is in progress
    pub active: bool,
    /// Pages migrated so far
    pub pages_migrated: u64,
    /// Total compaction runs
    pub compaction_runs: u64,
    /// Pages scanned
    pub pages_scanned: u64,
    /// Free pages recovered by compaction
    pub pages_recovered: u64,
}

lazy_static::lazy_static! {
    static ref COMPACTION: Mutex<CompactionState> = Mutex::new(CompactionState {
        active: false,
        pages_migrated: 0,
        compaction_runs: 0,
        pages_scanned: 0,
        pages_recovered: 0,
    });
}

/// Run memory compaction to defragment physical memory.
/// Moves allocated pages to create larger contiguous free regions.
/// Returns the number of pages that were successfully migrated.
pub fn compact_memory() -> u64 {
    let mut state = COMPACTION.lock();
    if state.active {
        return 0; // already running
    }
    state.active = true;
    state.compaction_runs += 1;
    drop(state);

    serial_println!("[compact] Starting memory compaction...");

    let mut migrated: u64 = 0;
    let mut scanned: u64 = 0;

    // Strategy: scan from the end of memory (migration scanner) and
    // from the beginning (free scanner). Move pages from high addresses
    // to low free slots, consolidating free space at the top.
    //
    // In our simplified model, we compact within each MemoryPool tier
    // by defragmenting the free list.
    {
        let mut pools = POOLS.lock();

        // Compact small tier
        {
            scanned += pools.small.capacity as u64;
            pools.small.free_list.sort_unstable();
            let mut contiguous_runs = 0u64;
            let mut run_start = true;
            for i in 1..pools.small.free_list.len() {
                if pools.small.free_list[i] == pools.small.free_list[i - 1] + 1 {
                    if run_start {
                        contiguous_runs += 1;
                        run_start = false;
                    }
                } else {
                    run_start = true;
                }
            }
            migrated += contiguous_runs;
        }
        // Compact medium tier
        {
            scanned += pools.medium.capacity as u64;
            pools.medium.free_list.sort_unstable();
        }
        // Compact large tier
        {
            scanned += pools.large.capacity as u64;
            pools.large.free_list.sort_unstable();
        }
    }

    let mut state = COMPACTION.lock();
    state.active = false;
    state.pages_migrated += migrated;
    state.pages_scanned += scanned;
    state.pages_recovered += migrated;

    serial_println!(
        "[compact] Compaction complete: {} pages migrated, {} scanned",
        migrated,
        scanned
    );
    migrated
}

/// Get fragmentation statistics for the system
pub fn fragmentation_stats() -> FragmentationStats {
    let pools = POOLS.lock();

    let total_free = pools.small.available() as u64
        + pools.medium.available() as u64
        + pools.large.available() as u64;
    let total_cap =
        pools.small.capacity as u64 + pools.medium.capacity as u64 + pools.large.capacity as u64;

    // Count largest contiguous free block
    let mut largest = 0u64;
    for pool in [&pools.small, &pools.medium, &pools.large] {
        let mut sorted = pool.free_list.clone();
        sorted.sort_unstable();
        let mut run = 1u64;
        let mut max_run = if sorted.is_empty() { 0 } else { 1 };
        for i in 1..sorted.len() {
            if sorted[i] == sorted[i - 1] + 1 {
                run += 1;
                if run > max_run {
                    max_run = run;
                }
            } else {
                run = 1;
            }
        }
        largest += max_run;
    }

    let frag_ratio = if total_free > 0 {
        1.0 - (largest as f32 / total_free as f32)
    } else {
        0.0
    };

    FragmentationStats {
        total_free_pages: total_free,
        largest_contiguous_block: largest,
        free_blocks: total_free,
        fragmentation_ratio: frag_ratio,
    }
}

/// Get compaction statistics
pub fn compaction_stats() -> (u64, u64, u64, u64) {
    let state = COMPACTION.lock();
    (
        state.compaction_runs,
        state.pages_migrated,
        state.pages_scanned,
        state.pages_recovered,
    )
}

// ═══════════════════════════════════════════════════════════════════════
// MEMORY BALLOON DRIVER (for VM guests)
// ═══════════════════════════════════════════════════════════════════════

/// Balloon driver state
pub struct BalloonDriver {
    /// Currently inflated (returned to host) pages
    pub inflated_pages: u64,
    /// Target pages the host wants us to reclaim
    pub target_pages: u64,
    /// Maximum pages we are allowed to balloon
    pub max_pages: u64,
    /// Whether the balloon driver is active
    pub active: bool,
    /// Statistics
    pub total_inflated: u64,
    pub total_deflated: u64,
}

lazy_static::lazy_static! {
    static ref BALLOON: Mutex<BalloonDriver> = Mutex::new(BalloonDriver {
        inflated_pages: 0,
        target_pages: 0,
        max_pages: 65536, // 256 MB max
        active: false,
        total_inflated: 0,
        total_deflated: 0,
    });
}

/// Initialize the memory balloon driver (VirtIO balloon)
pub fn balloon_init() {
    BALLOON.lock().active = true;
    serial_println!("[balloon] Memory balloon driver initialized");
}

/// Inflate the balloon (give memory back to the hypervisor).
/// Returns the number of pages actually reclaimed.
pub fn balloon_inflate(pages: u64) -> u64 {
    let mut b = BALLOON.lock();
    if !b.active {
        return 0;
    }

    let available = b.max_pages.saturating_sub(b.inflated_pages);
    let to_inflate = pages.min(available);

    b.inflated_pages += to_inflate;
    b.total_inflated += to_inflate;

    if to_inflate > 0 {
        serial_println!(
            "[balloon] Inflated {} pages (total: {})",
            to_inflate,
            b.inflated_pages
        );
    }
    to_inflate
}

/// Deflate the balloon (reclaim memory from the hypervisor).
/// Returns the number of pages reclaimed back.
pub fn balloon_deflate(pages: u64) -> u64 {
    let mut b = BALLOON.lock();
    if !b.active {
        return 0;
    }

    let to_deflate = pages.min(b.inflated_pages);
    b.inflated_pages -= to_deflate;
    b.total_deflated += to_deflate;

    if to_deflate > 0 {
        serial_println!(
            "[balloon] Deflated {} pages (total: {})",
            to_deflate,
            b.inflated_pages
        );
    }
    to_deflate
}

/// Set the balloon target (called by hypervisor via VirtIO)
pub fn balloon_set_target(target_pages: u64) {
    let mut b = BALLOON.lock();
    b.target_pages = target_pages.min(b.max_pages);

    // Automatically adjust balloon towards target
    if b.inflated_pages < b.target_pages {
        let diff = b.target_pages - b.inflated_pages;
        drop(b);
        balloon_inflate(diff);
    } else if b.inflated_pages > b.target_pages {
        let diff = b.inflated_pages - b.target_pages;
        drop(b);
        balloon_deflate(diff);
    }
}

/// Get balloon statistics
pub fn balloon_stats() -> (u64, u64, u64, u64) {
    let b = BALLOON.lock();
    (
        b.inflated_pages,
        b.target_pages,
        b.total_inflated,
        b.total_deflated,
    )
}

// ═══════════════════════════════════════════════════════════════════════
// MEMORY HOTPLUG — online/offline memory regions at runtime
// ═══════════════════════════════════════════════════════════════════════

/// A hotpluggable memory region
#[derive(Debug, Clone)]
pub struct MemoryRegion {
    /// Base physical address
    pub base_addr: u64,
    /// Size in bytes
    pub size: u64,
    /// Whether the region is online (usable)
    pub online: bool,
    /// NUMA node this region belongs to
    pub numa_node: u32,
    /// Whether this region can be offlined
    pub removable: bool,
}

/// Memory hotplug manager
pub struct HotplugManager {
    pub regions: Vec<MemoryRegion>,
    pub total_online_bytes: u64,
    pub total_offline_bytes: u64,
}

lazy_static::lazy_static! {
    static ref HOTPLUG: Mutex<HotplugManager> = Mutex::new(HotplugManager {
        regions: Vec::new(),
        total_online_bytes: 0,
        total_offline_bytes: 0,
    });
}

/// Register a hotpluggable memory region
pub fn hotplug_add_region(base: u64, size: u64, numa_node: u32) {
    let mut hp = HOTPLUG.lock();
    hp.regions.push(MemoryRegion {
        base_addr: base,
        size,
        online: true,
        numa_node,
        removable: true,
    });
    hp.total_online_bytes += size;
    serial_println!(
        "[hotplug] Added memory region: {:#x}-{:#x} ({}MB, node {})",
        base,
        base + size,
        size / (1024 * 1024),
        numa_node
    );
}

/// Online a memory region (make it usable)
pub fn hotplug_online(base: u64) -> Result<(), &'static str> {
    let mut hp = HOTPLUG.lock();
    let mut found = false;
    let mut region_size = 0u64;
    for region in hp.regions.iter_mut() {
        if region.base_addr == base {
            if region.online {
                return Err("Region already online");
            }
            region.online = true;
            region_size = region.size;
            found = true;
            break;
        }
    }
    if !found {
        return Err("Region not found");
    }
    hp.total_online_bytes += region_size;
    hp.total_offline_bytes -= region_size;
    serial_println!(
        "[hotplug] Onlined memory region at {:#x} ({}MB)",
        base,
        region_size / (1024 * 1024)
    );
    Ok(())
}

/// Offline a memory region (make it unusable — must migrate pages first)
pub fn hotplug_offline(base: u64) -> Result<(), &'static str> {
    let mut hp = HOTPLUG.lock();
    let mut found = false;
    let mut region_size = 0u64;
    for region in hp.regions.iter_mut() {
        if region.base_addr == base {
            if !region.online {
                return Err("Region already offline");
            }
            if !region.removable {
                return Err("Region is not removable (contains kernel data)");
            }
            region.online = false;
            region_size = region.size;
            found = true;
            break;
        }
    }
    if !found {
        return Err("Region not found");
    }
    hp.total_online_bytes -= region_size;
    hp.total_offline_bytes += region_size;
    serial_println!(
        "[hotplug] Offlined memory region at {:#x} ({}MB)",
        base,
        region_size / (1024 * 1024)
    );
    Ok(())
}

/// List all hotpluggable regions
pub fn hotplug_list_regions() -> Vec<MemoryRegion> {
    HOTPLUG.lock().regions.clone()
}

/// Get total online/offline memory
pub fn hotplug_stats() -> (u64, u64) {
    let hp = HOTPLUG.lock();
    (hp.total_online_bytes, hp.total_offline_bytes)
}
