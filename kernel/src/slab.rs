/// Slab Allocator — Fast small-object allocation with per-size caches
///
/// Provides O(1) allocation/deallocation for commonly-sized kernel objects
/// by maintaining pre-allocated pools (slabs) for each size class.
///
/// Size classes: 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096 bytes
/// Objects larger than 4096 go through the general heap allocator.
///
/// Each slab is a 4KiB page divided into fixed-size slots with a freelist.
/// This eliminates external fragmentation for small objects and greatly
/// reduces allocation latency compared to the linked-list heap allocator.
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

/// Size classes for slab allocation
const SIZE_CLASSES: &[usize] = &[8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096];

/// Page size for slab backing
const SLAB_PAGE_SIZE: usize = 4096;

/// Maximum number of slabs per size class
const MAX_SLABS_PER_CLASS: usize = 64;

/// A single slab — a page divided into fixed-size slots
struct Slab {
    /// Base address of the slab memory
    base: usize,
    /// Object size in this slab
    obj_size: usize,
    /// Total number of objects in this slab
    capacity: usize,
    /// Number of allocated objects
    allocated: usize,
    /// Freelist: indices of free slots
    freelist: Vec<usize>,
}

impl Slab {
    /// Create a new slab from a page of memory
    fn new(base: usize, obj_size: usize) -> Self {
        let capacity = SLAB_PAGE_SIZE / obj_size;
        let mut freelist = Vec::with_capacity(capacity);
        // All slots start free (reverse order so we pop from end efficiently)
        for i in (0..capacity).rev() {
            freelist.push(i);
        }
        Self {
            base,
            obj_size,
            capacity,
            allocated: 0,
            freelist,
        }
    }

    /// Allocate an object from this slab
    fn alloc(&mut self) -> Option<*mut u8> {
        if let Some(idx) = self.freelist.pop() {
            self.allocated += 1;
            let ptr = (self.base + idx * self.obj_size) as *mut u8;
            // Zero the memory
            unsafe {
                core::ptr::write_bytes(ptr, 0, self.obj_size);
            }
            Some(ptr)
        } else {
            None // Slab is full
        }
    }

    /// Free an object back to this slab
    fn free(&mut self, ptr: *mut u8) -> bool {
        let addr = ptr as usize;
        if addr < self.base || addr >= self.base + self.capacity * self.obj_size {
            return false; // Not from this slab
        }
        let offset = addr - self.base;
        if offset % self.obj_size != 0 {
            return false; // Misaligned
        }
        let idx = offset / self.obj_size;
        self.freelist.push(idx);
        self.allocated -= 1;
        true
    }

    /// Check if this slab contains the given address
    fn contains(&self, ptr: *mut u8) -> bool {
        let addr = ptr as usize;
        addr >= self.base && addr < self.base + self.capacity * self.obj_size
    }

    /// Is this slab completely empty?
    fn is_empty(&self) -> bool {
        self.allocated == 0
    }

    /// Is this slab completely full?
    fn is_full(&self) -> bool {
        self.freelist.is_empty()
    }
}

/// Per-size-class cache
struct SlabCache {
    obj_size: usize,
    slabs: Vec<Slab>,
    total_allocs: u64,
    total_frees: u64,
}

impl SlabCache {
    fn new(obj_size: usize) -> Self {
        Self {
            obj_size,
            slabs: Vec::new(),
            total_allocs: 0,
            total_frees: 0,
        }
    }

    /// Allocate an object
    fn alloc(&mut self) -> Option<*mut u8> {
        // Try existing partial slabs first
        for slab in &mut self.slabs {
            if !slab.is_full() {
                self.total_allocs += 1;
                return slab.alloc();
            }
        }

        // Need a new slab — allocate a page from the heap
        if self.slabs.len() >= MAX_SLABS_PER_CLASS {
            return None; // Too many slabs
        }

        let page = unsafe {
            alloc::alloc::alloc(
                alloc::alloc::Layout::from_size_align(SLAB_PAGE_SIZE, SLAB_PAGE_SIZE).unwrap(),
            )
        };
        if page.is_null() {
            return None;
        }

        let mut new_slab = Slab::new(page as usize, self.obj_size);
        let result = new_slab.alloc();
        self.slabs.push(new_slab);
        self.total_allocs += 1;
        result
    }

    /// Free an object
    fn free(&mut self, ptr: *mut u8) -> bool {
        for slab in &mut self.slabs {
            if slab.contains(ptr) {
                slab.free(ptr);
                self.total_frees += 1;
                return true;
            }
        }
        false
    }

    /// Get statistics
    fn stats(&self) -> (usize, usize, usize) {
        let total_capacity: usize = self.slabs.iter().map(|s| s.capacity).sum();
        let total_used: usize = self.slabs.iter().map(|s| s.allocated).sum();
        (self.slabs.len(), total_capacity, total_used)
    }
}

/// Global slab allocator
struct SlabAllocator {
    caches: Vec<SlabCache>,
    initialized: bool,
}

impl SlabAllocator {
    const fn new() -> Self {
        Self {
            caches: Vec::new(),
            initialized: false,
        }
    }

    fn init(&mut self) {
        for &size in SIZE_CLASSES {
            self.caches.push(SlabCache::new(size));
        }
        self.initialized = true;
    }

    /// Find the size class for a given allocation size
    fn find_class(&self, size: usize) -> Option<usize> {
        for (i, &class_size) in SIZE_CLASSES.iter().enumerate() {
            if size <= class_size {
                return Some(i);
            }
        }
        None // Too large for slab
    }

    fn alloc(&mut self, size: usize) -> Option<*mut u8> {
        if !self.initialized {
            return None;
        }
        let idx = self.find_class(size)?;
        self.caches[idx].alloc()
    }

    fn free(&mut self, ptr: *mut u8, size: usize) -> bool {
        if !self.initialized {
            return false;
        }
        if let Some(idx) = self.find_class(size) {
            self.caches[idx].free(ptr)
        } else {
            false
        }
    }
}

lazy_static::lazy_static! {
    static ref SLAB: Mutex<SlabAllocator> = Mutex::new(SlabAllocator::new());
}

// Statistics
static SLAB_ALLOCS: AtomicU64 = AtomicU64::new(0);
static SLAB_FREES: AtomicU64 = AtomicU64::new(0);

/// Allocate memory from the slab allocator
pub fn slab_alloc(size: usize) -> Option<*mut u8> {
    // Use try_lock to avoid deadlock: the global allocator calls slab_alloc,
    // but slab init itself allocates (Vec::push) which re-enters the allocator.
    let result = SLAB.try_lock()?.alloc(size);
    if result.is_some() {
        SLAB_ALLOCS.fetch_add(1, Ordering::Relaxed);
    }
    result
}

/// Free memory back to the slab allocator
pub fn slab_free(ptr: *mut u8, size: usize) -> bool {
    // Use try_lock to avoid deadlock when the slab is being modified.
    if let Some(mut slab) = SLAB.try_lock() {
        let result = slab.free(ptr, size);
        if result {
            SLAB_FREES.fetch_add(1, Ordering::Relaxed);
        }
        result
    } else {
        false
    }
}

/// Get slab allocator statistics
pub fn stats() -> SlabStats {
    if let Some(slab) = SLAB.try_lock() {
        let mut classes = Vec::new();
        for cache in &slab.caches {
            let (num_slabs, capacity, used) = cache.stats();
            classes.push(SlabClassStats {
                obj_size: cache.obj_size,
                num_slabs,
                total_capacity: capacity,
                total_used: used,
                total_allocs: cache.total_allocs,
                total_frees: cache.total_frees,
            });
        }
        SlabStats {
            total_allocs: SLAB_ALLOCS.load(Ordering::Relaxed),
            total_frees: SLAB_FREES.load(Ordering::Relaxed),
            classes,
        }
    } else {
        SlabStats {
            total_allocs: SLAB_ALLOCS.load(Ordering::Relaxed),
            total_frees: SLAB_FREES.load(Ordering::Relaxed),
            classes: Vec::new(),
        }
    }
}

/// Slab allocator statistics
pub struct SlabStats {
    pub total_allocs: u64,
    pub total_frees: u64,
    pub classes: Vec<SlabClassStats>,
}

pub struct SlabClassStats {
    pub obj_size: usize,
    pub num_slabs: usize,
    pub total_capacity: usize,
    pub total_used: usize,
    pub total_allocs: u64,
    pub total_frees: u64,
}

/// Initialize the slab allocator
pub fn init() {
    SLAB.lock().init();
    serial_println!(
        "[KnoxOS] Slab allocator initialized ({} size classes: {:?})",
        SIZE_CLASSES.len(),
        SIZE_CLASSES
    );
}
