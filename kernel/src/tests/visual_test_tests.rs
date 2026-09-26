// ═══════════════════════════════════════════════════════════════════════
// VISUAL TEST FRAMEWORK TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::visual_test::{Screenshot, compare};

#[test_case]
fn test_compare_identical() {
    let a = Screenshot {
        name: alloc::string::String::from("test"),
        width: 2,
        height: 2,
        pixels: alloc::vec![0xFF000000, 0xFF000000, 0xFF000000, 0xFF000000],
    };
    let b = a.clone();
    let result = compare(&a, &b, 0.1);
    assert!(result.passed);
    assert_eq!(result.diff_pixels, 0);
}

#[test_case]
fn test_compare_size_mismatch() {
    let a = Screenshot {
        name: alloc::string::String::from("a"),
        width: 2,
        height: 2,
        pixels: alloc::vec![0; 4],
    };
    let b = Screenshot {
        name: alloc::string::String::from("b"),
        width: 3,
        height: 3,
        pixels: alloc::vec![0; 9],
    };
    let result = compare(&a, &b, 0.1);
    assert!(!result.passed);
    assert_eq!(result.diff_percent, 100.0);
}
