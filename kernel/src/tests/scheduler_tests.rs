// ═══════════════════════════════════════════════════════════════════════
// SCHEDULER TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::scheduler;

#[test_case]
fn test_scheduler_init() {
    scheduler::init();
    let _ = scheduler::current_pid();
}
