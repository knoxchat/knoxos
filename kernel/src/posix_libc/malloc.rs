// Allocation size tracking (stdlib.h) — for proper realloc/free
use alloc::collections::BTreeMap;
use core::ffi::c_void;
use spin::Mutex;

lazy_static::lazy_static! {
    /// Track allocation sizes for proper free/realloc
    static ref ALLOC_SIZES: Mutex<BTreeMap<u64, usize>> = Mutex::new(BTreeMap::new());
}

/// Allocate memory with size tracking
pub unsafe fn tracked_malloc(size: usize) -> *mut c_void {
    if size == 0 {
        return core::ptr::null_mut();
    }
    let layout = alloc::alloc::Layout::from_size_align_unchecked(size, 16);
    let ptr = alloc::alloc::alloc(layout);
    if !ptr.is_null() {
        ALLOC_SIZES.lock().insert(ptr as u64, size);
    }
    ptr as *mut c_void
}

/// Free memory with size tracking
pub unsafe fn tracked_free(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    let size = ALLOC_SIZES.lock().remove(&(ptr as u64)).unwrap_or(64);
    let layout = alloc::alloc::Layout::from_size_align_unchecked(size, 16);
    alloc::alloc::dealloc(ptr as *mut u8, layout);
}

/// Reallocate with proper size tracking
pub unsafe fn tracked_realloc(ptr: *mut c_void, new_size: usize) -> *mut c_void {
    if ptr.is_null() {
        return tracked_malloc(new_size);
    }
    if new_size == 0 {
        tracked_free(ptr);
        return core::ptr::null_mut();
    }
    let old_size = ALLOC_SIZES.lock().get(&(ptr as u64)).copied().unwrap_or(0);
    let new_ptr = tracked_malloc(new_size);
    if !new_ptr.is_null() {
        let copy_size = if old_size < new_size {
            old_size
        } else {
            new_size
        };
        core::ptr::copy_nonoverlapping(ptr as *const u8, new_ptr as *mut u8, copy_size);
        tracked_free(ptr);
    }
    new_ptr
}
