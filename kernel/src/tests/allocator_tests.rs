// ═══════════════════════════════════════════════════════════════════════
// ALLOCATOR TESTS
// ═══════════════════════════════════════════════════════════════════════

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

#[test_case]
fn test_box_alloc_dealloc() {
    let val = Box::new(42u64);
    assert_eq!(*val, 42);
}

#[test_case]
fn test_vec_growth() {
    let mut v: Vec<u32> = Vec::new();
    for i in 0..1000 {
        v.push(i);
    }
    assert_eq!(v.len(), 1000);
    assert_eq!(v[999], 999);
}

#[test_case]
fn test_large_allocation() {
    // Allocate 1 MiB
    let v = vec![0u8; 1024 * 1024];
    assert_eq!(v.len(), 1024 * 1024);
}
