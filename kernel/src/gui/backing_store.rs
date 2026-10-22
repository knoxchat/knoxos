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

use crate::gui::framebuffer::{FrameBuffer, Pixel};
use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// BACKING STORE
// ═══════════════════════════════════════════════════════════════════════

/// A backing store for a single window (packed BGRA, matching Knox GPUI).
pub struct BackingStore {
    /// Packed BGRA8888 (`b, g, r, a`), row-major, no padding.
    pub bytes: Vec<u8>,
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
        let size = (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4);
        Self {
            bytes: vec![0u8; size],
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
        let size = (new_width as usize)
            .saturating_mul(new_height as usize)
            .saturating_mul(4);
        self.bytes = vec![0u8; size];
        self.width = new_width;
        self.height = new_height;
        self.dirty = true;
    }

    /// Replace contents from a packed BGRA raster (Knox GPUI store).
    pub fn copy_packed_bgra(&mut self, width: u32, height: u32, bytes: &[u8]) {
        self.resize(width, height);
        let n = self.bytes.len().min(bytes.len());
        self.bytes[..n].copy_from_slice(&bytes[..n]);
        if n < self.bytes.len() {
            self.bytes[n..].fill(0);
        }
        self.dirty = false;
    }

    pub fn blit_to(&self, fb: &mut FrameBuffer, x: i32, y: i32) {
        fb.blit_bgra(x, y, self.width, self.height, &self.bytes);
    }

    /// Get a pixel from the store
    pub fn get_pixel(&self, x: u32, y: u32) -> Pixel {
        if x < self.width && y < self.height {
            let i = ((y * self.width + x) as usize) * 4;
            if i + 3 < self.bytes.len() {
                return Pixel::new(
                    self.bytes[i + 2],
                    self.bytes[i + 1],
                    self.bytes[i],
                    self.bytes[i + 3],
                );
            }
        }
        Pixel::new(0, 0, 0, 0)
    }

    /// Set a pixel in the store
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Pixel) {
        if x < self.width && y < self.height {
            let i = ((y * self.width + x) as usize) * 4;
            if i + 3 < self.bytes.len() {
                self.bytes[i] = color.b;
                self.bytes[i + 1] = color.g;
                self.bytes[i + 2] = color.r;
                self.bytes[i + 3] = color.a;
            }
        }
    }

    /// Fill a rectangle in the store
    pub fn fill_rect(&mut self, x: i32, y: i32, w: u32, h: u32, color: Pixel) {
        let x0 = x.max(0) as u32;
        let y0 = y.max(0) as u32;
        let x1 = ((x as u32).wrapping_add(w)).min(self.width);
        let y1 = ((y as u32).wrapping_add(h)).min(self.height);
        let packed = [color.b, color.g, color.r, color.a];

        for py in y0..y1 {
            for px in x0..x1 {
                let i = ((py * self.width + px) as usize) * 4;
                if i + 3 < self.bytes.len() {
                    self.bytes[i..i + 4].copy_from_slice(&packed);
                }
            }
        }
    }

    /// Memory usage in bytes
    pub fn memory_usage(&self) -> usize {
        self.bytes.len()
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

    /// Copy a Knox GPUI raster into the store for `window_id` (D2.6).
    pub fn sync_packed(&mut self, window_id: u32, width: u32, height: u32, bytes: &[u8]) {
        let store = self.get_or_create(window_id, width, height);
        store.copy_packed_bgra(width, height, bytes);
    }

    /// Blit a stored window. Returns false if that id has no store.
    pub fn blit(&self, window_id: u32, fb: &mut FrameBuffer, x: i32, y: i32) -> bool {
        if let Some(store) = self.stores.get(&window_id) {
            store.blit_to(fb, x, y);
            true
        } else {
            false
        }
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
