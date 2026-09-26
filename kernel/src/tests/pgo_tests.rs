// ═══════════════════════════════════════════════════════════════════════
// PGO (PROFILE-GUIDED OPTIMIZATION) TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::pgo::PgoMode;

#[test_case]
fn test_pgo_modes() {
    assert_ne!(PgoMode::None, PgoMode::Instrument);
    assert_ne!(PgoMode::Instrument, PgoMode::Optimize);
}
