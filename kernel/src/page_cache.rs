/// Page Cache — Unified page cache with write-back policy
///
/// Caches file data at page granularity (4 KiB) to reduce disk I/O.
/// Features:
///   - Read-through caching: automatic loading on first access
///   - Write-back policy: dirty pages flushed asynchronously
///   - LRU eviction: least-recently-used pages reclaimed under memory pressure
///   - Readahead: sequential detection triggers prefetching
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// CONSTANTS
// ═══════════════════════════════════════════════════════════════════════

/// Page size (4 KiB)
pub const PAGE_SIZE: usize = 4096;

/// Default max cache size: 64 MiB worth of pages
const DEFAULT_MAX_PAGES: usize = 16384;

/// Readahead window (in pages)
const READAHEAD_SIZE: usize = 32;

/// Dirty page writeback interval (in timer ticks, ~55ms each ≈ 5 s)
const WRITEBACK_INTERVAL: u64 = 90;

// ═══════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════

/// Unique key for a cached page: (inode, page offset index)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PageKey {
    pub inode: u64,
    pub index: u64, // page index (offset / PAGE_SIZE)
}

/// Flags for a cached page
#[derive(Debug, Clone, Copy, Default)]
struct PageFlags {
    dirty: bool,
    uptodate: bool,
    writeback: bool,  // currently being flushed
    referenced: bool, // accessed recently (for LRU)
}

/// A cached page
#[derive(Clone)]
struct CachedPage {
    key: PageKey,
    data: Vec<u8>,
    flags: PageFlags,
    /// LRU generation (higher = more recently used)
    lru_gen: u64,
}

/// Cache statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub reads: u64,
    pub writes: u64,
    pub writebacks: u64,
    pub evictions: u64,
    pub readaheads: u64,
    pub total_pages: usize,
    pub dirty_pages: usize,
}

/// Page cache error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageCacheError {
    NotFound,
    IoError,
    OutOfMemory,
    InvalidOffset,
}

/// Readahead state per-inode
struct ReadaheadState {
    last_index: u64,
    sequential_count: u32,
    window_size: usize,
}

// ═══════════════════════════════════════════════════════════════════════
// STATE
// ═══════════════════════════════════════════════════════════════════════

struct PageCache {
    pages: BTreeMap<PageKey, CachedPage>,
    max_pages: usize,
    lru_counter: u64,
    readahead: BTreeMap<u64, ReadaheadState>, // per-inode
    stats: CacheStats,
    last_writeback_tick: u64,
}

impl PageCache {
    fn new() -> Self {
        PageCache {
            pages: BTreeMap::new(),
            max_pages: DEFAULT_MAX_PAGES,
            lru_counter: 0,
            readahead: BTreeMap::new(),
            stats: CacheStats::default(),
            last_writeback_tick: 0,
        }
    }
}

lazy_static::lazy_static! {
    static ref CACHE: Mutex<PageCache> = Mutex::new(PageCache::new());
}

static TOTAL_HIT: AtomicU64 = AtomicU64::new(0);
static TOTAL_MISS: AtomicU64 = AtomicU64::new(0);

// ═══════════════════════════════════════════════════════════════════════
// LOOKUP / INSERT
// ═══════════════════════════════════════════════════════════════════════

/// Look up a page in the cache. Returns page data if present.
pub fn find_page(inode: u64, index: u64) -> Option<Vec<u8>> {
    let mut cache = CACHE.lock();
    let key = PageKey { inode, index };

    if cache.pages.contains_key(&key) {
        cache.lru_counter += 1;
        let generation = cache.lru_counter;
        let page = cache.pages.get_mut(&key).unwrap();
        page.flags.referenced = true;
        page.lru_gen = generation;
        let data = page.data.clone();
        cache.stats.hits += 1;
        TOTAL_HIT.fetch_add(1, Ordering::Relaxed);
        Some(data)
    } else {
        cache.stats.misses += 1;
        TOTAL_MISS.fetch_add(1, Ordering::Relaxed);
        None
    }
}

/// Insert or update a page in the cache
pub fn insert_page(inode: u64, index: u64, data: &[u8], dirty: bool) {
    let mut cache = CACHE.lock();

    // Evict if full
    while cache.pages.len() >= cache.max_pages {
        evict_lru(&mut cache);
    }

    cache.lru_counter += 1;
    let generation = cache.lru_counter;
    let key = PageKey { inode, index };

    let mut page_data = alloc::vec![0u8; PAGE_SIZE];
    let copy_len = data.len().min(PAGE_SIZE);
    page_data[..copy_len].copy_from_slice(&data[..copy_len]);

    let page = CachedPage {
        key,
        data: page_data,
        flags: PageFlags {
            dirty,
            uptodate: true,
            writeback: false,
            referenced: true,
        },
        lru_gen: generation,
    };

    cache.pages.insert(key, page);

    if dirty {
        cache.stats.dirty_pages += 1;
    }
    cache.stats.total_pages = cache.pages.len();
}

// ═══════════════════════════════════════════════════════════════════════
// READ / WRITE API
// ═══════════════════════════════════════════════════════════════════════

/// Read data from the page cache, loading from backing store on miss
pub fn read(inode: u64, offset: u64, buf: &mut [u8]) -> Result<usize, PageCacheError> {
    let page_index = offset / PAGE_SIZE as u64;
    let page_offset = (offset % PAGE_SIZE as u64) as usize;

    let mut total = 0;
    let mut current_index = page_index;
    let mut buf_offset = 0;
    let mut first_page_skip = page_offset;

    while buf_offset < buf.len() {
        let page_data = if let Some(data) = find_page(inode, current_index) {
            data
        } else {
            // Cache miss — load from backing store
            let data = load_from_backing(inode, current_index)?;
            insert_page(inode, current_index, &data, false);

            // Check readahead
            update_readahead(inode, current_index);

            data
        };

        let available = PAGE_SIZE - first_page_skip;
        let copy_len = available.min(buf.len() - buf_offset);
        buf[buf_offset..buf_offset + copy_len]
            .copy_from_slice(&page_data[first_page_skip..first_page_skip + copy_len]);

        buf_offset += copy_len;
        total += copy_len;
        current_index += 1;
        first_page_skip = 0; // Only the first page has an offset
    }

    CACHE.lock().stats.reads += 1;
    Ok(total)
}

/// Write data through the page cache (write-back: marks dirty, flushes later)
pub fn write(inode: u64, offset: u64, data: &[u8]) -> Result<usize, PageCacheError> {
    let page_index = offset / PAGE_SIZE as u64;
    let page_offset = (offset % PAGE_SIZE as u64) as usize;

    let mut total = 0;
    let mut current_index = page_index;
    let mut data_offset = 0;
    let mut first_page_skip = page_offset;

    while data_offset < data.len() {
        // Read-modify-write for partial pages
        let mut page_data = if let Some(existing) = find_page(inode, current_index) {
            existing
        } else {
            // Try to load existing page, or start with zeros
            load_from_backing(inode, current_index).unwrap_or_else(|_| alloc::vec![0u8; PAGE_SIZE])
        };

        let available = PAGE_SIZE - first_page_skip;
        let copy_len = available.min(data.len() - data_offset);
        page_data[first_page_skip..first_page_skip + copy_len]
            .copy_from_slice(&data[data_offset..data_offset + copy_len]);

        insert_page(inode, current_index, &page_data, true);

        data_offset += copy_len;
        total += copy_len;
        current_index += 1;
        first_page_skip = 0;
    }

    CACHE.lock().stats.writes += 1;
    Ok(total)
}

// ═══════════════════════════════════════════════════════════════════════
// WRITE-BACK & EVICTION
// ═══════════════════════════════════════════════════════════════════════

/// Flush all dirty pages for a given inode
pub fn flush_inode(inode: u64) -> Result<usize, PageCacheError> {
    let mut cache = CACHE.lock();
    let mut flushed = 0;

    let keys: Vec<PageKey> = cache
        .pages
        .keys()
        .filter(|k| k.inode == inode)
        .copied()
        .collect();

    for key in keys {
        if let Some(page) = cache.pages.get_mut(&key) {
            if page.flags.dirty && !page.flags.writeback {
                page.flags.writeback = true;
                // Write to backing store
                let _ = write_to_backing(key.inode, key.index, &page.data);
                page.flags.dirty = false;
                page.flags.writeback = false;
                flushed += 1;
            }
        }
    }

    cache.stats.writebacks += flushed as u64;
    if flushed > 0 {
        cache.stats.dirty_pages = cache.stats.dirty_pages.saturating_sub(flushed);
    }

    Ok(flushed)
}

/// Flush ALL dirty pages to disk (called periodically by the writeback timer)
pub fn sync_all() -> usize {
    let mut cache = CACHE.lock();
    let mut flushed = 0;

    let dirty_keys: Vec<PageKey> = cache
        .pages
        .iter()
        .filter(|(_, p)| p.flags.dirty && !p.flags.writeback)
        .map(|(k, _)| *k)
        .collect();

    for key in dirty_keys {
        if let Some(page) = cache.pages.get_mut(&key) {
            page.flags.writeback = true;
            let _ = write_to_backing(key.inode, key.index, &page.data);
            page.flags.dirty = false;
            page.flags.writeback = false;
            flushed += 1;
        }
    }

    cache.stats.writebacks += flushed as u64;
    cache.stats.dirty_pages = cache.pages.values().filter(|p| p.flags.dirty).count();
    cache.last_writeback_tick = crate::interrupts::get_ticks();

    if flushed > 0 {
        serial_println!("[page_cache] Synced {} dirty pages", flushed);
    }
    flushed
}

/// Periodic tick — called from timer interrupt handler
pub fn writeback_tick() {
    let now = crate::interrupts::get_ticks();
    let should_flush = {
        let cache = CACHE.lock();
        now.saturating_sub(cache.last_writeback_tick) >= WRITEBACK_INTERVAL
            && cache.stats.dirty_pages > 0
    };
    if should_flush {
        sync_all();
    }
}

/// Evict the least-recently-used clean page
fn evict_lru(cache: &mut PageCache) {
    // Find clean page with lowest LRU generation
    let victim = cache
        .pages
        .iter()
        .filter(|(_, p)| !p.flags.dirty && !p.flags.writeback)
        .min_by_key(|(_, p)| p.lru_gen)
        .map(|(k, _)| *k);

    if let Some(key) = victim {
        cache.pages.remove(&key);
        cache.stats.evictions += 1;
        cache.stats.total_pages = cache.pages.len();
    } else {
        // All pages dirty — force writeback of oldest dirty page
        let oldest_dirty = cache
            .pages
            .iter()
            .filter(|(_, p)| p.flags.dirty && !p.flags.writeback)
            .min_by_key(|(_, p)| p.lru_gen)
            .map(|(k, _)| *k);

        if let Some(key) = oldest_dirty {
            if let Some(page) = cache.pages.get(&key) {
                let _ = write_to_backing(key.inode, key.index, &page.data);
            }
            cache.pages.remove(&key);
            cache.stats.evictions += 1;
            cache.stats.writebacks += 1;
            cache.stats.total_pages = cache.pages.len();
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// READAHEAD
// ═══════════════════════════════════════════════════════════════════════

fn update_readahead(inode: u64, current_index: u64) {
    let mut cache = CACHE.lock();

    let ra = cache.readahead.entry(inode).or_insert(ReadaheadState {
        last_index: current_index,
        sequential_count: 0,
        window_size: 4,
    });

    if current_index == ra.last_index + 1 {
        ra.sequential_count += 1;
        // After 2 sequential accesses, start readahead
        if ra.sequential_count >= 2 {
            ra.window_size = (ra.window_size * 2).min(READAHEAD_SIZE);
            let window = ra.window_size;
            let start = current_index + 1;

            // Don't hold cache lock while doing I/O — collect what we need
            let pages_to_fetch: Vec<u64> = (start..start + window as u64)
                .filter(|idx| !cache.pages.contains_key(&PageKey { inode, index: *idx }))
                .collect();

            // Drop lock before I/O
            drop(cache);

            for idx in pages_to_fetch {
                if let Ok(data) = load_from_backing(inode, idx) {
                    insert_page(inode, idx, &data, false);
                }
            }

            CACHE.lock().stats.readaheads += 1;
            return;
        }
    } else {
        ra.sequential_count = 0;
        ra.window_size = 4;
    }

    ra.last_index = current_index;
}

// ═══════════════════════════════════════════════════════════════════════
// BACKING STORE INTERFACE
// ═══════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════
// INODE PATH TABLE  — maps inode numbers to VFS paths for backing I/O
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    static ref INODE_PATHS: Mutex<BTreeMap<u64, String>> = Mutex::new(BTreeMap::new());
}

/// Register an inode → path mapping so the page cache can do backing I/O
pub fn register_inode(inode: u64, path: &str) {
    INODE_PATHS.lock().insert(inode, String::from(path));
}

/// Unregister an inode mapping
pub fn unregister_inode(inode: u64) {
    INODE_PATHS.lock().remove(&inode);
}

/// Load a page from the underlying filesystem via VFS
fn load_from_backing(inode: u64, index: u64) -> Result<Vec<u8>, PageCacheError> {
    let path = { INODE_PATHS.lock().get(&inode).cloned() };
    let path = path.ok_or(PageCacheError::NotFound)?;

    // Read the whole file via VFS and extract the page we need
    let file_data = crate::vfs::read_file_dispatch(&path).ok_or(PageCacheError::IoError)?;

    let offset = (index as usize) * PAGE_SIZE;
    let mut buf = alloc::vec![0u8; PAGE_SIZE];
    if offset < file_data.len() {
        let avail = (file_data.len() - offset).min(PAGE_SIZE);
        buf[..avail].copy_from_slice(&file_data[offset..offset + avail]);
    }
    Ok(buf)
}

/// Write a page to the underlying filesystem via VFS
fn write_to_backing(inode: u64, _index: u64, data: &[u8]) -> Result<(), PageCacheError> {
    let path = { INODE_PATHS.lock().get(&inode).cloned() };
    let path = path.ok_or(PageCacheError::NotFound)?;

    // Write the page data back (simplified — full impl would do partial writes)
    if crate::vfs::write_file_dispatch(&path, data) {
        Ok(())
    } else {
        Err(PageCacheError::IoError)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// INVALIDATION
// ═══════════════════════════════════════════════════════════════════════

/// Invalidate all cached pages for an inode (e.g. on file deletion)
pub fn invalidate_inode(inode: u64) {
    let mut cache = CACHE.lock();
    let keys: Vec<PageKey> = cache
        .pages
        .keys()
        .filter(|k| k.inode == inode)
        .copied()
        .collect();
    for key in keys {
        cache.pages.remove(&key);
    }
    cache.readahead.remove(&inode);
    cache.stats.total_pages = cache.pages.len();
}

/// Invalidate a single page
pub fn invalidate_page(inode: u64, index: u64) {
    let mut cache = CACHE.lock();
    cache.pages.remove(&PageKey { inode, index });
    cache.stats.total_pages = cache.pages.len();
}

/// Drop all clean pages to free memory (under pressure)
pub fn shrink(target_free: usize) -> usize {
    let mut cache = CACHE.lock();
    let mut freed = 0;

    while freed < target_free {
        let victim = cache
            .pages
            .iter()
            .filter(|(_, p)| !p.flags.dirty && !p.flags.writeback)
            .min_by_key(|(_, p)| p.lru_gen)
            .map(|(k, _)| *k);

        if let Some(key) = victim {
            cache.pages.remove(&key);
            freed += 1;
        } else {
            break; // Only dirty pages remain
        }
    }

    cache.stats.evictions += freed as u64;
    cache.stats.total_pages = cache.pages.len();
    freed
}

// ═══════════════════════════════════════════════════════════════════════
// STATS / INIT
// ═══════════════════════════════════════════════════════════════════════

/// Get page cache statistics
pub fn stats() -> CacheStats {
    let cache = CACHE.lock();
    CacheStats {
        total_pages: cache.pages.len(),
        dirty_pages: cache.pages.values().filter(|p| p.flags.dirty).count(),
        ..cache.stats
    }
}

/// Set maximum number of cached pages
pub fn set_max_pages(max: usize) {
    CACHE.lock().max_pages = max;
}

/// Initialize the page cache
pub fn init() {
    serial_println!(
        "[KnoxOS] Page cache initialized (max {} pages, {} MiB)",
        DEFAULT_MAX_PAGES,
        DEFAULT_MAX_PAGES * PAGE_SIZE / (1024 * 1024)
    );
}
