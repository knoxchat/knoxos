// ═══════════════════════════════════════════════════════════════════════
// FIREWALL TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::firewall;
use crate::net::Ipv4Address;

#[test_case]
fn test_firewall_default_accept() {
    firewall::init();
    let action = firewall::filter_packet(
        firewall::Chain::Input,
        Ipv4Address([10, 0, 0, 1]),
        Ipv4Address([10, 0, 0, 2]),
        6, // TCP
        12345,
        80,
        100,
    );
    // Default policy should be Accept when no rules match
    assert!(matches!(action, firewall::Target::Accept));
}
