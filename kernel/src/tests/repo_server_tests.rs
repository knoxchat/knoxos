// ═══════════════════════════════════════════════════════════════════════
// REPOSITORY SERVER TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::repo_server;

#[test_case]
fn test_repo_add_package() {
    repo_server::add_package(
        "test-pkg",
        "1.0.0",
        "A test package",
        b"KPKG\x01\x00\x00\x00test data",
        &[],
    );
    let list = repo_server::list_packages();
    assert!(list.iter().any(|(name, _, _)| name == "test-pkg"));
}

#[test_case]
fn test_repo_packages_index() {
    let index = repo_server::generate_packages_index();
    // Should contain Debian-compatible fields
    assert!(index.contains("Package:") || index.is_empty());
}

#[test_case]
fn test_repo_handle_404() {
    let resp = repo_server::handle_request("/nonexistent");
    assert_eq!(resp.status, 404);
}

#[test_case]
fn test_repo_handle_index() {
    let resp = repo_server::handle_request("/repo/");
    assert_eq!(resp.status, 200);
}
