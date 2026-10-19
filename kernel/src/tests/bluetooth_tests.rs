// ═══════════════════════════════════════════════════════════════════════
// BLUETOOTH TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::bluetooth;

#[test_case]
fn test_bluetooth_init() {
    bluetooth::init();
    // Should not panic
}

#[test_case]
fn test_bluetooth_hci_reset() {
    bluetooth::init();
    assert!(
        bluetooth::hci_self_test(),
        "HCI Reset must produce Command Complete status 0"
    );
}
