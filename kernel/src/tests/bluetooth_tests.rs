// ═══════════════════════════════════════════════════════════════════════
// BLUETOOTH TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::bluetooth;

#[test_case]
fn test_bluetooth_init() {
    bluetooth::init();
    // Should not panic
}
