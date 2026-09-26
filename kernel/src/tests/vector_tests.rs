// ═══════════════════════════════════════════════════════════════════════
// VECTOR GRAPHICS TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::gui::vector::Point;

#[test_case]
fn test_point_creation() {
    let p = Point::new(10.5, 20.3);
    assert!(p.x > 10.0 && p.x < 11.0);
    assert!(p.y > 20.0 && p.y < 21.0);
}
