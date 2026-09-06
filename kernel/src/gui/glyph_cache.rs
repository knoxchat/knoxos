/// Glyph Cache — Lazy font glyph caching for performance
///
/// Caches rendered glyph bitmaps to avoid re-rasterizing every frame:
///   - LRU eviction when cache is full
///   - Keyed by (codepoint, font_size, style)
///   - Pre-rendered alpha bitmaps
///   - Thread-safe access
use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// GLYPH CACHE KEY
// ═══════════════════════════════════════════════════════════════════════

/// Cache key: uniquely identifies a rendered glyph
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct GlyphKey {
    pub codepoint: u32,
    pub font_size: u16, // in tenths of pixels (e.g., 200 = 20.0px)
    pub style: GlyphStyle,
    pub subpixel: bool, // subpixel rendering enabled
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GlyphStyle {
    Regular = 0,
    Bold = 1,
    Italic = 2,
    BoldItalic = 3,
}

impl GlyphKey {
    pub fn new(codepoint: char, font_size: f32, style: GlyphStyle, subpixel: bool) -> Self {
        Self {
            codepoint: codepoint as u32,
            font_size: (font_size * 10.0) as u16,
            style,
            subpixel,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// CACHED GLYPH
// ═══════════════════════════════════════════════════════════════════════

/// A cached glyph bitmap
#[derive(Clone)]
pub struct CachedGlyph {
    /// Alpha bitmap (grayscale) or RGB subpixel data
    pub bitmap: Vec<u8>,
    /// Width in pixels
    pub width: u16,
    /// Height in pixels
    pub height: u16,
    /// X bearing (offset from origin)
    pub bearing_x: i16,
    /// Y bearing (offset from baseline)
    pub bearing_y: i16,
    /// Advance width (spacing to next glyph)
    pub advance: u16,
    /// Bytes per pixel (1 for grayscale, 3 for subpixel)
    pub bpp: u8,
    /// Last access time (for LRU eviction)
    access_count: u64,
}

// ═══════════════════════════════════════════════════════════════════════
// CACHE
// ═══════════════════════════════════════════════════════════════════════

/// Maximum number of cached glyphs
const MAX_CACHE_ENTRIES: usize = 4096;
/// Maximum cache memory (bytes)
const MAX_CACHE_BYTES: usize = 4 * 1024 * 1024; // 4 MiB

static ACCESS_COUNTER: AtomicU64 = AtomicU64::new(1);
static CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static CACHE_MISSES: AtomicU64 = AtomicU64::new(0);

pub struct GlyphCache {
    entries: BTreeMap<GlyphKey, CachedGlyph>,
    total_bytes: usize,
}

impl GlyphCache {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            total_bytes: 0,
        }
    }

    /// Look up a cached glyph
    pub fn get(&mut self, key: &GlyphKey) -> Option<&CachedGlyph> {
        if let Some(entry) = self.entries.get_mut(key) {
            entry.access_count = ACCESS_COUNTER.fetch_add(1, Ordering::Relaxed);
            CACHE_HITS.fetch_add(1, Ordering::Relaxed);
            // Need to re-borrow as immutable
            return self.entries.get(key);
        }
        CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Insert a glyph into the cache
    pub fn insert(&mut self, key: GlyphKey, glyph: CachedGlyph) {
        let entry_bytes = glyph.bitmap.len() + 32; // bitmap + overhead

        // Evict if necessary
        while (self.entries.len() >= MAX_CACHE_ENTRIES
            || self.total_bytes + entry_bytes > MAX_CACHE_BYTES)
            && !self.entries.is_empty()
        {
            self.evict_lru();
        }

        self.total_bytes += entry_bytes;
        self.entries.insert(key, glyph);
    }

    /// Evict the least-recently-used entry
    fn evict_lru(&mut self) {
        let lru_key = self
            .entries
            .iter()
            .min_by_key(|(_, v)| v.access_count)
            .map(|(k, _)| *k);

        if let Some(key) = lru_key {
            if let Some(removed) = self.entries.remove(&key) {
                self.total_bytes -= removed.bitmap.len() + 32;
            }
        }
    }

    /// Clear the entire cache
    pub fn clear(&mut self) {
        self.entries.clear();
        self.total_bytes = 0;
    }

    /// Number of cached entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Total memory used
    pub fn memory_usage(&self) -> usize {
        self.total_bytes
    }

    /// Cache hit rate (0.0 - 1.0)
    pub fn hit_rate() -> f32 {
        let hits = CACHE_HITS.load(Ordering::Relaxed) as f32;
        let misses = CACHE_MISSES.load(Ordering::Relaxed) as f32;
        let total = hits + misses;
        if total > 0.0 { hits / total } else { 0.0 }
    }
}

lazy_static::lazy_static! {
    pub static ref GLYPH_CACHE: Mutex<GlyphCache> = Mutex::new(GlyphCache::new());
}

/// Convenience: get or render a glyph
pub fn get_or_render<F>(key: GlyphKey, render_fn: F) -> CachedGlyph
where
    F: FnOnce() -> CachedGlyph,
{
    let mut cache = GLYPH_CACHE.lock();
    if let Some(glyph) = cache.get(&key) {
        return glyph.clone();
    }

    let glyph = render_fn();
    cache.insert(key, glyph.clone());
    glyph
}

/// Pre-cache ASCII characters for a given font size
pub fn precache_ascii(font_size: f32, style: GlyphStyle, subpixel: bool) {
    // This would call the font renderer for each ASCII char
    // Just marks the intent — actual rendering happens on first access
    serial_println!(
        "[GlyphCache] Pre-caching ASCII for size={}, style={:?}, subpixel={}",
        font_size,
        style,
        subpixel
    );
}

/// Reset cache statistics
pub fn reset_stats() {
    CACHE_HITS.store(0, Ordering::Relaxed);
    CACHE_MISSES.store(0, Ordering::Relaxed);
}

/// Initialize glyph cache
pub fn init() {
    serial_println!(
        "[KnoxOS] Glyph cache initialized (max {}KB)",
        MAX_CACHE_BYTES / 1024
    );
}
