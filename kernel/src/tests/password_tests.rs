// ═══════════════════════════════════════════════════════════════════════
// PASSWORD HASHING TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::users;

#[test_case]
fn test_hash_and_verify() {
    let hash = users::hash_password("secret123", "testsalt");
    assert!(hash.starts_with("$5$"));
    assert!(users::verify_password("secret123", &hash));
    assert!(!users::verify_password("wrong_password", &hash));
}

#[test_case]
fn test_empty_password_hash() {
    let hash = users::hash_password("", "knoxos");
    assert!(users::verify_password("", &hash));
    assert!(!users::verify_password("notempty", &hash));
}

#[test_case]
fn test_wildcard_password() {
    assert!(users::verify_password("anything", "*"));
}
