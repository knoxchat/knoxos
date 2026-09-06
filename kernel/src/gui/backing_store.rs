/// Window Backing Store — Cache window content to avoid full re-renders
///
/// Provides:
///   - Per-window off-screen pixel buffer
///   - Dirty flag tracking (only re-render when content changes)
///   - Fast blit from cache to framebuffer
///   - Memory management with eviction
use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::gui::framebuffer::Pixel;
use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// BACKING STORE
// ═══════════════════════════════════════════════════════════════════════

/// A backing store for a single window
pub struct BackingStore {
    /// Pixel buffer
    pub pixels: Vec<Pixel>,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Whether the content needs re-rendering
    pub dirty: bool,
    /// Last time the store was used (for LRU eviction)
    pub last_used: u64,
}

impl BackingStore {
    pub fn new(width: u32, height: u32) -> Self {
        let size = (width * height) as usize;
        Self {
            pixels: vec![Pixel::new(0, 0, 0, 0); size],
            width,
            height,
            dirty: true,
            last_used: 0,
        }
    }

    /// Resize the backing store (marks as dirty)
    pub fn resize(&mut self, new_width: u32, new_height: u32) {
        if new_width == self.width && new_height == self.height {
            return;
        }
        let size = (new_width * new_height) as usize;
        self.pixels = vec![Pixel::new(0, 0, 0, 0); size];
        self.width = new_width;
        self.height = new_height;
        self.dirty = true;
    }

    /// Get a pixel from the store
    pub fn get_pixel(&self, x: u32, y: u32) -> Pixel {
        if x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize]
        } else {
            Pixel::new(0, 0, 0, 0)
        }
    }

    /// Set a pixel in the store
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Pixel) {
        if x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize] = color;
        }
    }

    /// Fill a rectangle in the store
    pub fn fill_rect(&mut self, x: i32, y: i32, w: u32, h: u32, color: Pixel) {
        let x0 = x.max(0) as u32;
        let y0 = y.max(0) as u32;
        let x1 = ((x as u32).wrapping_add(w)).min(self.width);
        let y1 = ((y as u32).wrapping_add(h)).min(self.height);

        for py in y0..y1 {
            let row_start = (py * self.width + x0) as usize;
            let row_end = (py * self.width + x1) as usize;
            if row_end <= self.pixels.len() {
                for px in &mut self.pixels[row_start..row_end] {
                    *px = color;
                }
            }
        }
    }

    /// Memory usage in bytes
    pub fn memory_usage(&self) -> usize {
        self.pixels.len() * 4 // 4 bytes per pixel (RGBA)
    }

    /// Mark as dirty (needs re-render)
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// Mark as clean (just rendered)
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }
}

// ═══════════════════════════════════════════════════════════════════════
// BACKING STORE MANAGER
// ═══════════════════════════════════════════════════════════════════════

/// Maximum total memory for all backing stores
const MAX_TOTAL_MEMORY: usize = 64 * 1024 * 1024; // 64 MiB

static USAGE_COUNTER: AtomicU64 = AtomicU64::new(1);

pub struct BackingStoreManager {
    stores: BTreeMap<u32, BackingStore>, // window_id → store
    total_memory: usize,
}

impl BackingStoreManager {
    pub fn new() -> Self {
        Self {
            stores: BTreeMap::new(),
            total_memory: 0,
        }
    }

    /// Create or get a backing store for a window
    pub fn get_or_create(&mut self, window_id: u32, width: u32, height: u32) -> &mut BackingStore {
        let entry_size = (width * height) as usize * 4;

        // Evict if necessary
        while self.total_memory + entry_size > MAX_TOTAL_MEMORY && !self.stores.is_empty() {
            self.evict_lru();
        }

        if !self.stores.contains_key(&window_id) {
            let store = BackingStore::new(width, height);
            self.total_memory += store.memory_usage();
            self.stores.insert(window_id, store);
        }

        let store = self.stores.get_mut(&window_id).unwrap();

        // Resize if needed
        if store.width != width || store.height != height {
            self.total_memory -= store.memory_usage();
            store.resize(width, height);
            self.total_memory += store.memory_usage();
        }

        store.last_used = USAGE_COUNTER.fetch_add(1, Ordering::Relaxed);
        store
    }

    /// Remove a backing store (window closed)
    pub fn remove(&mut self, window_id: u32) {
        if let Some(store) = self.stores.remove(&window_id) {
            self.total_memory -= store.memory_usage();
        }
    }

    /// Invalidate a specific window (mark dirty)
    pub fn invalidate(&mut self, window_id: u32) {
        if let Some(store) = self.stores.get_mut(&window_id) {
            store.invalidate();
        }
    }

    /// Invalidate all backing stores
    pub fn invalidate_all(&mut self) {
        for store in self.stores.values_mut() {
            store.invalidate();
        }
    }

    /// Is a window's backing store clean (doesn't need re-render)?
    pub fn is_clean(&self, window_id: u32) -> bool {
        self.stores
            .get(&window_id)
            .map(|s| !s.dirty)
            .unwrap_or(false)
    }

    /// Evict the least-recently-used backing store
    fn evict_lru(&mut self) {
        let lru_id = self
            .stores
            .iter()
            .min_by_key(|(_, v)| v.last_used)
            .map(|(k, _)| *k);

        if let Some(id) = lru_id {
            if let Some(store) = self.stores.remove(&id) {
                self.total_memory -= store.memory_usage();
                serial_println!("[BackingStore] Evicted window {}", id);
            }
        }
    }

    /// Total memory usage
    pub fn total_memory_usage(&self) -> usize {
        self.total_memory
    }

    /// Number of cached stores
    pub fn count(&self) -> usize {
        self.stores.len()
    }
}

lazy_static::lazy_static! {
    pub static ref STORE_MANAGER: Mutex<BackingStoreManager> =
        Mutex::new(BackingStoreManager::new());
}

/// Initialize backing store manager
pub fn init() {
    serial_println!(
        "[KnoxOS] Window backing store initialized (max {}MB)",
        MAX_TOTAL_MEMORY / (1024 * 1024)
    );
}
