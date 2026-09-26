// ═══════════════════════════════════════════════════════════════════════
// DNS TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::dns;

#[test_case]
fn test_dns_build_query() {
    let query = dns::build_query("example.com", 1); // A record
    // DNS header is 12 bytes, then the question section
    assert!(query.len() > 12);
    // Transaction ID is first 2 bytes (non-zero)
    // Flags: standard query = 0x0100
    assert_eq!(query[2], 0x01);
    assert_eq!(query[3], 0x00);
}
