/// Buffer Cache — Block-level caching layer for filesystem I/O
///
/// Provides a write-back cache between filesystems and block devices.
/// All disk reads go through the cache, and writes can be buffered
/// and flushed periodically or on demand.
///
/// Features:
///   - LRU eviction policy
///   - Dirty page tracking for write-back
///   - Configurable cache size
///   - Per-block locking for concurrent access
///   - Sync/flush support
use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

/// Block size for the cache (matches typical sector size)
pub const CACHE_BLOCK_SIZE: usize = 512;

/// Maximum number of cached blocks
const MAX_CACHE_BLOCKS: usize = 2048; // 1 MiB cache

/// Statistics counter
static CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static CACHE_MISSES: AtomicU64 = AtomicU64::new(0);
static CACHE_WRITES: AtomicU64 = AtomicU64::new(0);
static CACHE_EVICTIONS: AtomicU64 = AtomicU64::new(0);

/// A cached block
#[derive(Clone)]
struct CacheBlock {
    /// Block/sector number
    sector: u64,
    /// Device identifier (0 = primary block device)
    device: u32,
    /// Cached data
    data: Vec<u8>,
    /// Whether this block has been modified
    dirty: bool,
    /// Last access timestamp (for LRU)
    last_access: u64,
    /// Reference count
    ref_count: u32,
}

impl CacheBlock {
    fn new(device: u32, sector: u64, data: Vec<u8>) -> Self {
        Self {
            sector,
            device,
            data,
            dirty: false,
            last_access: access_counter(),
            ref_count: 0,
        }
    }

    /// Make a cache key
    fn key(device: u32, sector: u64) -> u64 {
        ((device as u64) << 48) | sector
    }
}

/// Global access counter for LRU
static ACCESS_COUNTER: AtomicU64 = AtomicU64::new(0);

fn access_counter() -> u64 {
    ACCESS_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// The buffer cache
pub struct BufferCache {
    /// Cached blocks indexed by (device << 48 | sector)
    blocks: BTreeMap<u64, CacheBlock>,
    /// Maximum number of blocks
    max_blocks: usize,
}

impl BufferCache {
    pub fn new(max_blocks: usize) -> Self {
        Self {
            blocks: BTreeMap::new(),
            max_blocks,
        }
    }

    /// Read a block from cache, or fetch from disk
    pub fn read(&mut self, device: u32, sector: u64) -> Option<Vec<u8>> {
        let key = CacheBlock::key(device, sector);

        // Check cache first
        if let Some(block) = self.blocks.get_mut(&key) {
            block.last_access = access_counter();
            block.ref_count += 1;
            CACHE_HITS.fetch_add(1, Ordering::Relaxed);
            return Some(block.data.clone());
        }

        // Cache miss — read from disk
        CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
        let data = read_from_device(device, sector)?;

        // Insert into cache (evict if necessary)
        if self.blocks.len() >= self.max_blocks {
            self.evict_lru();
        }

        let block = CacheBlock::new(device, sector, data.clone());
        self.blocks.insert(key, block);

        Some(data)
    }

    /// Read multiple contiguous sectors
    pub fn read_sectors(&mut self, device: u32, start_sector: u64, count: u64) -> Option<Vec<u8>> {
        let mut result = Vec::with_capacity((count as usize) * CACHE_BLOCK_SIZE);
        for i in 0..count {
            let data = self.read(device, start_sector + i)?;
            result.extend_from_slice(&data);
        }
        Some(result)
    }

    /// Write a block (write-back: marks dirty, actual write on flush)
    pub fn write(&mut self, device: u32, sector: u64, data: &[u8]) -> bool {
        let key = CacheBlock::key(device, sector);
        CACHE_WRITES.fetch_add(1, Ordering::Relaxed);

        if let Some(block) = self.blocks.get_mut(&key) {
            // Update existing cache entry
            let len = data.len().min(CACHE_BLOCK_SIZE);
            block.data[..len].copy_from_slice(&data[..len]);
            block.dirty = true;
            block.last_access = access_counter();
            return true;
        }

        // Not in cache — create new entry
        if self.blocks.len() >= self.max_blocks {
            self.evict_lru();
        }

        let mut block_data = vec![0u8; CACHE_BLOCK_SIZE];
        let len = data.len().min(CACHE_BLOCK_SIZE);
        block_data[..len].copy_from_slice(&data[..len]);

        let mut block = CacheBlock::new(device, sector, block_data);
        block.dirty = true;
        self.blocks.insert(key, block);

        true
    }

    /// Write-through: write to cache and immediately to disk
    pub fn write_through(&mut self, device: u32, sector: u64, data: &[u8]) -> bool {
        // Write to cache
        self.write(device, sector, data);

        // Write to disk immediately
        let success = write_to_device(device, sector, data);

        if success {
            // Mark as clean since we wrote to disk
            let key = CacheBlock::key(device, sector);
            if let Some(block) = self.blocks.get_mut(&key) {
                block.dirty = false;
            }
        }

        success
    }

    /// Flush all dirty blocks to disk
    pub fn flush(&mut self) -> usize {
        let mut flushed = 0;
        let dirty_keys: Vec<u64> = self
            .blocks
            .iter()
            .filter(|(_, b)| b.dirty)
            .map(|(&k, _)| k)
            .collect();

        for key in dirty_keys {
            if let Some(block) = self.blocks.get_mut(&key) {
                if write_to_device(block.device, block.sector, &block.data) {
                    block.dirty = false;
                    flushed += 1;
                }
            }
        }

        if flushed > 0 {
            serial_println!("[BCACHE] Flushed {} dirty blocks", flushed);
        }
        flushed
    }

    /// Flush dirty blocks for a specific device
    pub fn flush_device(&mut self, device: u32) -> usize {
        let mut flushed = 0;
        let dirty_keys: Vec<u64> = self
            .blocks
            .iter()
            .filter(|(_, b)| b.dirty && b.device == device)
            .map(|(&k, _)| k)
            .collect();

        for key in dirty_keys {
            if let Some(block) = self.blocks.get_mut(&key) {
                if write_to_device(block.device, block.sector, &block.data) {
                    block.dirty = false;
                    flushed += 1;
                }
            }
        }
        flushed
    }

    /// Evict the least recently used (non-dirty) block
    fn evict_lru(&mut self) {
        // Find LRU block that isn't dirty (prefer evicting clean blocks)
        let evict_key = self
            .blocks
            .iter()
            .filter(|(_, b)| !b.dirty && b.ref_count == 0)
            .min_by_key(|(_, b)| b.last_access)
            .map(|(&k, _)| k);

        if let Some(key) = evict_key {
            self.blocks.remove(&key);
            CACHE_EVICTIONS.fetch_add(1, Ordering::Relaxed);
            return;
        }

        // All clean blocks are referenced — flush and evict a dirty block
        let evict_key = self
            .blocks
            .iter()
            .min_by_key(|(_, b)| b.last_access)
            .map(|(&k, _)| k);

        if let Some(key) = evict_key {
            // Flush dirty block before eviction
            if let Some(block) = self.blocks.get(&key) {
                if block.dirty {
                    write_to_device(block.device, block.sector, &block.data);
                }
            }
            self.blocks.remove(&key);
            CACHE_EVICTIONS.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Invalidate a cache entry (e.g., after a device reset)
    pub fn invalidate(&mut self, device: u32, sector: u64) {
        let key = CacheBlock::key(device, sector);
        if let Some(block) = self.blocks.get(&key) {
            if block.dirty {
                // Flush before invalidating
                write_to_device(block.device, block.sector, &block.data);
            }
        }
        self.blocks.remove(&key);
    }

    /// Invalidate all cache entries for a device
    pub fn invalidate_device(&mut self, device: u32) {
        self.flush_device(device);
        let keys: Vec<u64> = self
            .blocks
            .iter()
            .filter(|(_, b)| b.device == device)
            .map(|(&k, _)| k)
            .collect();
        for key in keys {
            self.blocks.remove(&key);
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let dirty = self.blocks.values().filter(|b| b.dirty).count();
        CacheStats {
            total_blocks: self.blocks.len(),
            max_blocks: self.max_blocks,
            dirty_blocks: dirty,
            hits: CACHE_HITS.load(Ordering::Relaxed),
            misses: CACHE_MISSES.load(Ordering::Relaxed),
            writes: CACHE_WRITES.load(Ordering::Relaxed),
            evictions: CACHE_EVICTIONS.load(Ordering::Relaxed),
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_blocks: usize,
    pub max_blocks: usize,
    pub dirty_blocks: usize,
    pub hits: u64,
    pub misses: u64,
    pub writes: u64,
    pub evictions: u64,
}

impl CacheStats {
    /// Hit rate as percentage
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 0.0;
        }
        (self.hits as f64 / total as f64) * 100.0
    }
}

// ─── Device I/O Backend ─────────────────────────────────────────────────

/// Read a sector from a block device
fn read_from_device(device: u32, sector: u64) -> Option<Vec<u8>> {
    match device {
        0 => {
            // Primary block device: try virtio-blk first, then ramdisk
            let mut buf = vec![0u8; CACHE_BLOCK_SIZE];
            if crate::virtio_blk::is_available() && crate::virtio_blk::read(sector, 1, &mut buf) {
                return Some(buf);
            }
            // Fallback to ramdisk
            if crate::block::read_blocks(0, sector, 1, &mut buf).is_ok() {
                return Some(buf);
            }
            None
        }
        _ => None,
    }
}

/// Write a sector to a block device
fn write_to_device(device: u32, sector: u64, data: &[u8]) -> bool {
    match device {
        0 => {
            if crate::virtio_blk::is_available() {
                crate::virtio_blk::write(sector, 1, data)
            } else {
                crate::block::write_blocks(0, sector, 1, data).is_ok()
            }
        }
        _ => false,
    }
}

// ─── Global Cache Instance ──────────────────────────────────────────────

lazy_static::lazy_static! {
    pub static ref BUFFER_CACHE: Mutex<BufferCache> = Mutex::new(BufferCache::new(MAX_CACHE_BLOCKS));
}

// ─── Public API ─────────────────────────────────────────────────────────

/// Read a sector through the cache
pub fn cached_read(device: u32, sector: u64) -> Option<Vec<u8>> {
    BUFFER_CACHE.lock().read(device, sector)
}

/// Read multiple sectors through the cache
pub fn cached_read_sectors(device: u32, start: u64, count: u64) -> Option<Vec<u8>> {
    BUFFER_CACHE.lock().read_sectors(device, start, count)
}

/// Write a sector through the cache (write-back)
pub fn cached_write(device: u32, sector: u64, data: &[u8]) -> bool {
    BUFFER_CACHE.lock().write(device, sector, data)
}

/// Write a sector through the cache (write-through)
pub fn cached_write_sync(device: u32, sector: u64, data: &[u8]) -> bool {
    BUFFER_CACHE.lock().write_through(device, sector, data)
}

/// Flush all dirty blocks to disk
pub fn sync() -> usize {
    BUFFER_CACHE.lock().flush()
}

/// Get cache statistics
pub fn stats() -> CacheStats {
    BUFFER_CACHE.lock().stats()
}

/// Initialize the buffer cache
pub fn init() {
    serial_println!(
        "[KnoxOS] Buffer cache initialized ({} blocks, {} KiB max)",
        MAX_CACHE_BLOCKS,
        MAX_CACHE_BLOCKS * CACHE_BLOCK_SIZE / 1024
    );
}
