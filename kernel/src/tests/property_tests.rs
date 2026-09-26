// ═══════════════════════════════════════════════════════════════════════
// PROPERTY / ALLOCATOR TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::path::normalize_path;
use crate::vmm::PhysicalFramePool;
use alloc::vec::Vec;

#[test_case]
fn test_path_normalize_idempotent() {
    let samples = [
        "/home/user/../etc",
        "a/b/./c",
        "//a///b",
        "/a/b/../../c",
        "..",
        "a/../../b",
        "/",
        "/././.",
    ];
    for s in samples {
        let once = normalize_path(s);
        let twice = normalize_path(&once);
        assert_eq!(once, twice, "not idempotent for {s}");
    }
    assert_eq!(normalize_path("/home/user/../etc"), "/etc");
    assert_eq!(normalize_path("a/b/./c"), "a/b/c");
}

#[test_case]
fn test_alloc_free_symmetry() {
    let mut pool = PhysicalFramePool::new();
    for i in 0..32u64 {
        pool.add_frame(0x0040_0000 + i * 4096);
    }
    assert_eq!(pool.available(), 32);
    let mut held = Vec::new();
    for _ in 0..32 {
        held.push(pool.allocate().expect("frame"));
    }
    assert_eq!(pool.available(), 0);
    for f in held {
        pool.free(f);
    }
    assert_eq!(pool.available(), 32);
    let a = pool.allocate().unwrap();
    pool.free(a);
    assert_eq!(pool.available(), 32);
}

#[test_case]
fn test_buddy_add_range_roundtrip() {
    let mut pool = PhysicalFramePool::new();
    pool.add_range(0x0100_0000, 1024);
    assert_eq!(pool.available(), 1024);
    assert_eq!(pool.total(), 1024);
    let mut held = Vec::new();
    for _ in 0..16 {
        held.push(pool.allocate().expect("frame"));
    }
    assert_eq!(pool.available(), 1024 - 16);
    for f in held {
        pool.free(f);
    }
    assert_eq!(pool.available(), 1024);
}
