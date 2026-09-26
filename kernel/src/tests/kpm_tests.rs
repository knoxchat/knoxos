// ═══════════════════════════════════════════════════════════════════════
// KPM (PACKAGE MANAGER) TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::kpm;
use alloc::string::String;

#[test_case]
fn test_kpm_init_has_packages() {
    kpm::init();
    let packages = kpm::PACKAGES.lock();
    assert!(
        packages.len() > 0,
        "KPM should have built-in packages after init"
    );
}

#[test_case]
fn test_kpm_search() {
    let results = kpm::search("kernel");
    assert!(
        results.len() > 0,
        "Searching 'kernel' should return results"
    );
}

#[test_case]
fn test_kpm_resolve_dependencies() {
    // Should resolve a known package without error
    let result = kpm::resolve_dependencies("knoxos-kernel");
    assert!(
        result.is_ok(),
        "Resolving knoxos-kernel deps should succeed"
    );
}
