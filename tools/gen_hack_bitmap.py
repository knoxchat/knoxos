#!/usr/bin/env python3
"""
Generate crisp 1-bit Hack bitmap font for KnoxOS.

Strategy: Render Hack TTF at HIGH RESOLUTION (6x supersampling),
downsample to target cell size, then threshold to binary (1-bit).
Each glyph row becomes a single byte where bit 7 (MSB) = leftmost pixel.

At 8x16 pixel resolution, 1-bit rendering is superior to antialiased
alpha blending — there simply aren't enough pixels for meaningful
gradient edges. Binary rendering eliminates all blur and fuzz.
"""

from PIL import Image, ImageFont, ImageDraw

TTF_PATH = "hack/build/ttf/Hack-Regular.ttf"
FIRST_CHAR = 0x20
LAST_CHAR  = 0x7E
CHAR_COUNT = LAST_CHAR - FIRST_CHAR + 1
SUPERSAMPLE = 6

# Alpha values >= this threshold become ON (1); below become OFF (0)
BINARY_THRESHOLD = 96


def render_glyph_set(ttf_path, cell_w, cell_h, font_size, baseline_offset):
    hi_w = cell_w * SUPERSAMPLE
    hi_h = cell_h * SUPERSAMPLE
    hi_font_size = font_size * SUPERSAMPLE
    hi_baseline = baseline_offset * SUPERSAMPLE
    font = ImageFont.truetype(ttf_path, hi_font_size)
    glyphs = []
    for code in range(FIRST_CHAR, LAST_CHAR + 1):
        ch = chr(code)
        img = Image.new("L", (hi_w, hi_h), 0)
        draw = ImageDraw.Draw(img)
        bbox = font.getbbox(ch)
        if bbox:
            glyph_w = bbox[2] - bbox[0]
            x_off = (hi_w - glyph_w) // 2 - bbox[0]
        else:
            x_off = 0
        draw.text((x_off, hi_baseline), ch, fill=255, font=font)
        img_small = img.resize((cell_w, cell_h), Image.LANCZOS)
        # Convert to 1-bit bitmap: one byte per row, MSB = leftmost pixel
        bitmap_rows = []
        for y in range(cell_h):
            byte_val = 0
            for x in range(cell_w):
                alpha = img_small.getpixel((x, y))
                if alpha >= BINARY_THRESHOLD:
                    byte_val |= (1 << (7 - x))
            bitmap_rows.append(byte_val)
        glyphs.append((ch, bitmap_rows))
    return glyphs


def generate_rust(glyphs_std, glyphs_compact, cell_w, cell_h_std, cell_h_compact):
    lines = []
    lines.append("/// Hack Bitmap Font — Crisp 1-bit glyph data for KnoxOS")
    lines.append("///")
    lines.append("/// Each glyph row is stored as a single byte where bit 7 (MSB) is the")
    lines.append("/// leftmost pixel and bit 0 is the rightmost. A set bit means the pixel")
    lines.append("/// is fully ON (rendered in the text color); a clear bit is fully OFF")
    lines.append("/// (transparent). This produces razor-sharp text with zero blur or fuzz.")
    lines.append("///")
    lines.append("/// Generated from Hack-Regular.ttf using 6x supersampled rendering,")
    lines.append("/// downsampled and thresholded to binary for maximum crispness.")
    lines.append("")
    lines.append("/// Glyph width in pixels")
    lines.append(f"pub const GLYPH_WIDTH: usize = {cell_w};")
    lines.append("/// Glyph height in pixels (standard)")
    lines.append(f"pub const GLYPH_HEIGHT: usize = {cell_h_std};")
    lines.append("/// Glyph height in pixels (compact)")
    lines.append(f"pub const GLYPH_HEIGHT_COMPACT: usize = {cell_h_compact};")
    lines.append("")
    lines.append(f"const GLYPH_COUNT: usize = {CHAR_COUNT};")
    lines.append("")
    lines.append("/// Look up the standard 8×16 bitmap glyph for a character.")
    lines.append("/// Returns 16 bytes, one per row. Bit 7 = leftmost pixel.")
    lines.append(f"pub fn get_glyph(ch: char) -> Option<&'static [u8; GLYPH_HEIGHT]>" + " {")
    lines.append("    let idx = ch as usize;")
    lines.append(f"    if idx >= 0x{FIRST_CHAR:02X} && idx <= 0x{LAST_CHAR:02X}" + " {")
    lines.append(f"        Some(&HACK_STANDARD[idx - 0x{FIRST_CHAR:02X}])")
    lines.append("    } else {")
    lines.append("        None")
    lines.append("    }")
    lines.append("}")
    lines.append("")
    lines.append("/// Look up the compact 8×12 bitmap glyph for a character.")
    lines.append("/// Returns 12 bytes, one per row. Bit 7 = leftmost pixel.")
    lines.append(f"pub fn get_glyph_compact(ch: char) -> Option<&'static [u8; GLYPH_HEIGHT_COMPACT]>" + " {")
    lines.append("    let idx = ch as usize;")
    lines.append(f"    if idx >= 0x{FIRST_CHAR:02X} && idx <= 0x{LAST_CHAR:02X}" + " {")
    lines.append(f"        Some(&HACK_COMPACT[idx - 0x{FIRST_CHAR:02X}])")
    lines.append("    } else {")
    lines.append("        None")
    lines.append("    }")
    lines.append("}")
    lines.append("")

    # Standard bitmaps
    lines.append("#[rustfmt::skip]")
    lines.append(f"static HACK_STANDARD: [[u8; GLYPH_HEIGHT]; GLYPH_COUNT] = [")
    for ch, bitmap_rows in glyphs_std:
        hex_str = ", ".join(f"0x{b:02X}" for b in bitmap_rows)
        lines.append(f"    // 0x{ord(ch):02X} {repr(ch)}")
        lines.append(f"    [{hex_str}],")
    lines.append("];")
    lines.append("")

    # Compact bitmaps
    lines.append("#[rustfmt::skip]")
    lines.append(f"static HACK_COMPACT: [[u8; GLYPH_HEIGHT_COMPACT]; GLYPH_COUNT] = [")
    for ch, bitmap_rows in glyphs_compact:
        hex_str = ", ".join(f"0x{b:02X}" for b in bitmap_rows)
        lines.append(f"    // 0x{ord(ch):02X} {repr(ch)}")
        lines.append(f"    [{hex_str}],")
    lines.append("];")
    lines.append("")
    return "\n".join(lines)


def main():
    ttf = TTF_PATH
    print("Generating 8x16 standard glyphs (6x supersampled, binary threshold)")
    glyphs_16 = render_glyph_set(ttf, cell_w=8, cell_h=16, font_size=12, baseline_offset=1)
    print("Generating 8x12 compact glyphs (6x supersampled, binary threshold)")
    glyphs_12 = render_glyph_set(ttf, cell_w=8, cell_h=12, font_size=9, baseline_offset=0)
    rust_src = generate_rust(glyphs_16, glyphs_12, 8, 16, 12)
    out_path = "kernel/src/gui/hack_font.rs"
    with open(out_path, "w") as f:
        f.write(rust_src)
    print(f"Written {out_path} ({len(rust_src)} bytes)")
    
    # Verify distinct characters
    g0 = dict(glyphs_16)
    if g0.get("0", []) != g0.get("O", []):
        print("OK: 0 != O (slashed zero)")
    gl, gI, g1 = g0.get("l", []), g0.get("I", []), g0.get("1", [])
    if gl != gI and gl != g1 and gI != g1:
        print("OK: l != I != 1 (all distinct)")

    # Show sample
    for ch, rows in glyphs_16:
        if ch in 'AMe0lI1':
            print(f"  {ch}:")
            for r in rows:
                bits = ''.join('##' if r & (1 << (7-b)) else '..' for b in range(8))
                print(f"    {bits}")


if __name__ == "__main__":
    main()
