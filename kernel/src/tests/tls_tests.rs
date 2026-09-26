// ═══════════════════════════════════════════════════════════════════════
// TLS TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::tls;

#[test_case]
fn test_certificate_store_init() {
    // Creating a certificate store should not panic
    let store = tls::CertificateStore::new();
    assert!(store.trusted_roots.is_empty());
}

#[test_case]
fn test_x509_parse_invalid() {
    // Invalid DER data should return None
    let result = tls::X509Certificate::from_der(&[0x00, 0x01, 0x02]);
    assert!(result.is_none());
}
