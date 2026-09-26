// ═══════════════════════════════════════════════════════════════════════
// SIXEL GRAPHICS TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::terminal::sixel::SixelImage;

#[test_case]
fn test_sixel_image_creation() {
    let img = SixelImage::new();
    assert_eq!(img.width, 0);
    assert_eq!(img.height, 0);
}

#[test_case]
fn test_sixel_parse_simple() {
    let mut img = SixelImage::new();
    // Simple sixel: ? = all 6 bits off, ~ = all 6 bits on
    img.parse(b"~");
    assert!(img.width > 0 || !img.pixels.is_empty());
}

#[test_case]
fn test_sixel_scale() {
    let mut img = SixelImage::new();
    img.parse(b"\"1;1;100;60~~~$-~~~");
    let scaled = img.scale_to_fit(50, 30);
    // Scaled image should be smaller or equal
    assert!(scaled.width <= 100);
}
