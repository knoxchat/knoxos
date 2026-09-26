// ═══════════════════════════════════════════════════════════════════════
// SERVICE MANAGER TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::service_manager::{RestartPolicy, ServiceState, ServiceUnit};

#[test_case]
fn test_service_unit_creation() {
    let svc = ServiceUnit::new("test.service", "Test Service", "/usr/bin/test");
    assert_eq!(svc.name, "test.service");
    assert_eq!(svc.state, ServiceState::Inactive);
    assert!(!svc.enabled);
}

#[test_case]
fn test_restart_policies() {
    assert_ne!(RestartPolicy::No, RestartPolicy::Always);
    assert_ne!(RestartPolicy::OnFailure, RestartPolicy::OnAbnormal);
}
