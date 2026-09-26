// ═══════════════════════════════════════════════════════════════════════
// RELEASE SIGNING TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::release_sign;

#[test_case]
fn test_semver_parse() {
    let v = release_sign::SemVer::parse("1.2.3").unwrap();
    assert_eq!(v.major, 1);
    assert_eq!(v.minor, 2);
    assert_eq!(v.patch, 3);
}

#[test_case]
fn test_semver_prerelease() {
    let v = release_sign::SemVer::parse("1.0.0-beta.1").unwrap();
    assert_eq!(v.major, 1);
    assert_eq!(v.prerelease, Some(alloc::string::String::from("beta.1")));
}

#[test_case]
fn test_semver_comparison() {
    let v1 = release_sign::SemVer::parse("1.0.0").unwrap();
    let v2 = release_sign::SemVer::parse("1.0.1").unwrap();
    assert_eq!(v1.cmp_precedence(&v2), core::cmp::Ordering::Less);
}

#[test_case]
fn test_semver_prerelease_lower() {
    let v_pre = release_sign::SemVer::parse("1.0.0-alpha").unwrap();
    let v_rel = release_sign::SemVer::parse("1.0.0").unwrap();
    assert_eq!(v_pre.cmp_precedence(&v_rel), core::cmp::Ordering::Less);
}

#[test_case]
fn test_release_sign_verify() {
    let seed = [0xABu8; 32];
    let key = release_sign::generate_key_pair(&seed);
    let data = b"test release data";
    let sig = release_sign::sign_artifact(data, &key);
    assert!(release_sign::verify_signature(data, &sig, &key));
}

#[test_case]
fn test_release_sign_tampered() {
    let seed = [0xCDu8; 32];
    let key = release_sign::generate_key_pair(&seed);
    let data = b"original data";
    let sig = release_sign::sign_artifact(data, &key);
    let tampered = b"tampered data";
    assert!(!release_sign::verify_signature(tampered, &sig, &key));
}

#[test_case]
fn test_semver_bump() {
    let v = release_sign::SemVer::new(1, 2, 3);
    let major = v.bump_major();
    assert_eq!(major.major, 2);
    assert_eq!(major.minor, 0);
    assert_eq!(major.patch, 0);
}
