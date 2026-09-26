// ═══════════════════════════════════════════════════════════════════════
// WINDOW TILING TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::gui::window::{TilingMode, Window, WindowManager};

#[test_case]
fn test_tiling_modes_exist() {
    // Verify all tiling modes can be constructed
    let modes = [
        TilingMode::Floating,
        TilingMode::MasterStack,
        TilingMode::Grid,
        TilingMode::Monocle,
        TilingMode::Columns,
    ];
    assert_eq!(modes.len(), 5);
}

#[test_case]
fn test_tiling_mode_default_is_floating() {
    let wm = WindowManager::new();
    assert_eq!(wm.tiling_mode, TilingMode::Floating);
}
