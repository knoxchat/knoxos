// ═══════════════════════════════════════════════════════════════════════
// HWTEST (HARDWARE TESTING) TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::hwtest::{TestCategory, TestSeverity};

#[test_case]
fn test_categories() {
    let categories = [
        TestCategory::Cpu,
        TestCategory::Memory,
        TestCategory::Pci,
        TestCategory::Interrupt,
        TestCategory::Timer,
        TestCategory::Storage,
        TestCategory::Network,
        TestCategory::Serial,
        TestCategory::Acpi,
    ];
    assert_eq!(categories.len(), 9);
}

#[test_case]
fn test_severities() {
    assert_ne!(TestSeverity::Info, TestSeverity::Critical);
    assert_ne!(TestSeverity::Warning, TestSeverity::Error);
}
