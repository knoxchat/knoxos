// ═══════════════════════════════════════════════════════════════════════
// BENCHMARK TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::benchmark;

#[test_case]
fn test_benchmark_init() {
    benchmark::init();
    // Should not panic
}

#[test_case]
fn test_benchmark_run_all() {
    benchmark::run_all();
    // Should complete without panic
}
