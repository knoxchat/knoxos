// ═══════════════════════════════════════════════════════════════════════
// IMAGE DECODER TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::gui::image;

#[test_case]
fn test_bmp_decode() {
    // Minimal 2x2 24-bit BMP (no compression)
    #[rustfmt::skip]
    let bmp: &[u8] = &[
        // BM header (14 bytes)
        0x42, 0x4D,             // "BM"
        0x46, 0x00, 0x00, 0x00, // file size = 70
        0x00, 0x00, 0x00, 0x00, // reserved
        0x36, 0x00, 0x00, 0x00, // pixel data offset = 54
        // DIB header (40 bytes)
        0x28, 0x00, 0x00, 0x00, // header size = 40
        0x02, 0x00, 0x00, 0x00, // width = 2
        0x02, 0x00, 0x00, 0x00, // height = 2
        0x01, 0x00,             // color planes = 1
        0x18, 0x00,             // bits per pixel = 24
        0x00, 0x00, 0x00, 0x00, // compression = 0 (none)
        0x10, 0x00, 0x00, 0x00, // image size = 16
        0x13, 0x0B, 0x00, 0x00, // h-res
        0x13, 0x0B, 0x00, 0x00, // v-res
        0x00, 0x00, 0x00, 0x00, // colors
        0x00, 0x00, 0x00, 0x00, // important colors
        // Pixel data (bottom-up, padded to 4-byte rows)
        // Row 0 (bottom): 2 pixels × 3 bytes = 6 bytes + 2 padding
        0x00, 0x00, 0xFF, // pixel (0,1) = red (BGR)
        0x00, 0xFF, 0x00, // pixel (1,1) = green
        0x00, 0x00,       // padding
        // Row 1 (top): 2 pixels × 3 bytes = 6 bytes + 2 padding
        0xFF, 0x00, 0x00, // pixel (0,0) = blue (BGR)
        0xFF, 0xFF, 0xFF, // pixel (1,0) = white
        0x00, 0x00,       // padding
    ];

    let img = image::decode(bmp).expect("BMP decode failed");
    assert_eq!(img.width, 2);
    assert_eq!(img.height, 2);
    assert_eq!(img.pixels.len(), 4);
}

#[test_case]
fn test_format_detection_png() {
    let png_sig: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    let fmt = image::detect_format(&png_sig);
    assert_eq!(fmt, Some(image::ImageFormat::Png));
}

#[test_case]
fn test_format_detection_jpeg() {
    let jpg_sig: [u8; 8] = [0xFF, 0xD8, 0xFF, 0xE0, 0, 0, 0, 0];
    let fmt = image::detect_format(&jpg_sig);
    assert_eq!(fmt, Some(image::ImageFormat::Jpeg));
}

#[test_case]
fn test_scale_image() {
    // Create a tiny 2x2 image and scale to 4x4
    use crate::gui::framebuffer::Pixel;
    let img = image::DecodedImage {
        width: 2,
        height: 2,
        pixels: alloc::vec![
            Pixel::rgb(255, 0, 0),
            Pixel::rgb(0, 255, 0),
            Pixel::rgb(0, 0, 255),
            Pixel::rgb(255, 255, 255),
        ],
        format: image::ImageFormat::Raw,
    };
    let scaled = image::scale_image(&img, 4, 4);
    assert_eq!(scaled.width, 4);
    assert_eq!(scaled.height, 4);
    assert_eq!(scaled.pixels.len(), 16);
}
