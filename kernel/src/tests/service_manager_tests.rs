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

#[test_case]
fn test_unit_cgroup_memory() {
    assert!(
        crate::service_manager::cgroup_self_test(),
        "unit MemoryMax=4KiB must ENOMEM on an 8KiB charge"
    );
}
