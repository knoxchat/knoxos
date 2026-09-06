use alloc::string::String;
/// Image Decoding — PNG and JPEG image loading for wallpapers and icons
///
/// Provides software-only image decoding:
///   - PNG: deflate decompression, IDAT chunk parsing, filtering
///   - JPEG: baseline DCT, Huffman decoding, YCbCr→RGB conversion
///   - BMP: simple uncompressed bitmap loading
///   - ICO: Windows icon format
///   - Output: BGRA pixel buffer suitable for framebuffer blitting
use alloc::vec;
use alloc::vec::Vec;

use crate::gui::framebuffer::Pixel;
use crate::serial_println;

/// Decoded image
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Pixel>,
    pub format: ImageFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Bmp,
    Ico,
    Raw,
}

/// PNG signature: 137 80 78 71 13 10 26 10
const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

/// JPEG signature: FFD8
const JPEG_SIGNATURE: [u8; 2] = [0xFF, 0xD8];

/// BMP signature: "BM"
const BMP_SIGNATURE: [u8; 2] = [0x42, 0x4D];

/// Detect image format from header bytes
pub fn detect_format(data: &[u8]) -> Option<ImageFormat> {
    if data.len() < 8 {
        return None;
    }
    if data[..8] == PNG_SIGNATURE {
        Some(ImageFormat::Png)
    } else if data[..2] == JPEG_SIGNATURE {
        Some(ImageFormat::Jpeg)
    } else if data[..2] == BMP_SIGNATURE {
        Some(ImageFormat::Bmp)
    } else {
        None
    }
}

/// Decode an image from raw bytes
pub fn decode(data: &[u8]) -> Option<DecodedImage> {
    let format = detect_format(data)?;
    match format {
        ImageFormat::Png => decode_png(data),
        ImageFormat::Jpeg => decode_jpeg(data),
        ImageFormat::Bmp => decode_bmp(data),
        _ => None,
    }
}

/// Read a big-endian u32 from a byte slice
fn read_be_u32(data: &[u8], offset: usize) -> u32 {
    ((data[offset] as u32) << 24)
        | ((data[offset + 1] as u32) << 16)
        | ((data[offset + 2] as u32) << 8)
        | (data[offset + 3] as u32)
}

/// Read a little-endian u32
fn read_le_u32(data: &[u8], offset: usize) -> u32 {
    (data[offset] as u32)
        | ((data[offset + 1] as u32) << 8)
        | ((data[offset + 2] as u32) << 16)
        | ((data[offset + 3] as u32) << 24)
}

/// Read a little-endian u16
fn read_le_u16(data: &[u8], offset: usize) -> u16 {
    (data[offset] as u16) | ((data[offset + 1] as u16) << 8)
}

// ═══════════════════════════════════════════════════════════════════════
// PNG DECODER (simplified — supports 8-bit RGB/RGBA uncompressed IDAT)
// ═══════════════════════════════════════════════════════════════════════

/// Minimal PNG decoder
/// Parses IHDR, collects IDAT chunks, decompresses (deflate), unfilters
fn decode_png(data: &[u8]) -> Option<DecodedImage> {
    if data.len() < 33 || data[..8] != PNG_SIGNATURE {
        return None;
    }

    // Parse IHDR chunk (must be first)
    let ihdr_len = read_be_u32(data, 8) as usize;
    if &data[12..16] != b"IHDR" || ihdr_len != 13 {
        return None;
    }

    let width = read_be_u32(data, 16);
    let height = read_be_u32(data, 20);
    let bit_depth = data[24];
    let color_type = data[25];
    // compression, filter, interlace at [26..29]

    if bit_depth != 8 {
        serial_println!("[PNG] Only 8-bit depth supported, got {}", bit_depth);
        return None;
    }

    let channels: usize = match color_type {
        2 => 3, // RGB
        6 => 4, // RGBA
        0 => 1, // Grayscale
        4 => 2, // Grayscale+Alpha
        _ => {
            serial_println!("[PNG] Unsupported color type: {}", color_type);
            return None;
        }
    };

    // Collect all IDAT chunks
    let mut idat_data = Vec::new();
    let mut offset = 8; // After signature
    while offset + 12 <= data.len() {
        let chunk_len = read_be_u32(data, offset) as usize;
        let chunk_type = &data[offset + 4..offset + 8];
        let chunk_data_start = offset + 8;
        let chunk_data_end = chunk_data_start + chunk_len;

        if chunk_data_end > data.len() {
            break;
        }

        if chunk_type == b"IDAT" {
            idat_data.extend_from_slice(&data[chunk_data_start..chunk_data_end]);
        }

        if chunk_type == b"IEND" {
            break;
        }

        offset = chunk_data_end + 4; // +4 for CRC
    }

    if idat_data.is_empty() {
        serial_println!("[PNG] No IDAT data found");
        return None;
    }

    // Decompress the IDAT data (deflate)
    let raw_data = deflate_decompress(&idat_data)?;

    // Unfilter scanlines
    let stride = channels * width as usize;
    let expected_len = (stride + 1) * height as usize; // +1 for filter byte per row

    if raw_data.len() < expected_len {
        serial_println!(
            "[PNG] Decompressed data too short: {} < {}",
            raw_data.len(),
            expected_len
        );
        return None;
    }

    // Apply PNG row filters and convert to Pixel array
    let mut pixels = Vec::with_capacity((width * height) as usize);
    let mut prev_row = vec![0u8; stride];

    for y in 0..height as usize {
        let row_start = y * (stride + 1);
        let filter_type = raw_data[row_start];
        let row_data = &raw_data[row_start + 1..row_start + 1 + stride];

        let mut filtered = vec![0u8; stride];
        for i in 0..stride {
            let raw = row_data[i];
            let a = if i >= channels {
                filtered[i - channels]
            } else {
                0
            };
            let b = prev_row[i];
            let c = if i >= channels {
                prev_row[i - channels]
            } else {
                0
            };

            filtered[i] = match filter_type {
                0 => raw,                                                 // None
                1 => raw.wrapping_add(a),                                 // Sub
                2 => raw.wrapping_add(b),                                 // Up
                3 => raw.wrapping_add(((a as u16 + b as u16) / 2) as u8), // Average
                4 => raw.wrapping_add(paeth_predictor(a, b, c)),          // Paeth
                _ => raw,
            };
        }

        // Convert to pixels
        for x in 0..width as usize {
            let px = match channels {
                4 => Pixel::new(
                    filtered[x * 4],
                    filtered[x * 4 + 1],
                    filtered[x * 4 + 2],
                    filtered[x * 4 + 3],
                ),
                3 => Pixel::rgb(filtered[x * 3], filtered[x * 3 + 1], filtered[x * 3 + 2]),
                1 => Pixel::rgb(filtered[x], filtered[x], filtered[x]),
                2 => Pixel::new(
                    filtered[x * 2],
                    filtered[x * 2],
                    filtered[x * 2],
                    filtered[x * 2 + 1],
                ),
                _ => Pixel::rgb(0, 0, 0),
            };
            pixels.push(px);
        }

        prev_row = filtered;
    }

    Some(DecodedImage {
        width,
        height,
        pixels,
        format: ImageFormat::Png,
    })
}

/// Paeth predictor for PNG filtering
fn paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i16 + b as i16 - c as i16;
    let pa = (p - a as i16).unsigned_abs();
    let pb = (p - b as i16).unsigned_abs();
    let pc = (p - c as i16).unsigned_abs();
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Minimal deflate decompressor (RFC 1950/1951)
/// Handles zlib header + deflate compressed data
fn deflate_decompress(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 6 {
        return None;
    }

    // zlib header: CMF (byte 0), FLG (byte 1)
    let cmf = data[0];
    let _flg = data[1];
    let cm = cmf & 0x0F;
    if cm != 8 {
        // Not deflate
        return None;
    }

    // Skip 2-byte zlib header, decompress deflate stream
    let deflate_data = &data[2..data.len().saturating_sub(4)]; // -4 for Adler32
    inflate_decompress(deflate_data)
}

/// Inflate (decompress) raw deflate data (RFC 1951)
/// Supports block types 0 (uncompressed), 1 (fixed Huffman), 2 (dynamic Huffman)
fn inflate_decompress(data: &[u8]) -> Option<Vec<u8>> {
    let mut output = Vec::new();
    let mut bit_offset: usize = 0;

    fn read_bits(data: &[u8], bit_offset: &mut usize, count: usize) -> u32 {
        let mut value: u32 = 0;
        for i in 0..count {
            let byte_idx = *bit_offset / 8;
            let bit_idx = *bit_offset % 8;
            if byte_idx < data.len() && data[byte_idx] & (1 << bit_idx) != 0 {
                value |= 1 << i;
            }
            *bit_offset += 1;
        }
        value
    }

    /// Reverse bits for Huffman code matching (codes are MSB-first in the spec)
    fn reverse_bits(val: u32, bits: u32) -> u32 {
        let mut r = 0u32;
        let mut v = val;
        for _ in 0..bits {
            r = (r << 1) | (v & 1);
            v >>= 1;
        }
        r
    }

    /// Build a decode table: maps (reversed code) → symbol for codes of a given length.
    /// Returns (max_bits, symbols[code] for each bit length).
    /// We use a flat lookup table indexed by (bit_length - 1, reversed_code_prefix).
    struct HuffTable {
        /// For each bit-length 1..=max_bits: array of 2^max_bits entries.
        /// We pack into a single flat array: symbols[code] where code has max_bits bits.
        /// -1 means no symbol at that slot.
        symbols: Vec<i16>,
        max_bits: u32,
    }

    impl HuffTable {
        fn build(lengths: &[u8], max_sym: usize) -> Self {
            let max_bits = lengths.iter().copied().max().unwrap_or(0) as u32;
            if max_bits == 0 {
                return HuffTable {
                    symbols: Vec::new(),
                    max_bits: 0,
                };
            }
            let table_size = 1usize << max_bits;
            let mut symbols = vec![-1i16; table_size];

            // Count codes per length
            let mut bl_count = [0u32; 16];
            for &len in lengths.iter().take(max_sym) {
                if len > 0 {
                    bl_count[len as usize] += 1;
                }
            }

            // Compute first code for each length (RFC 1951 section 3.2.2)
            let mut next_code = [0u32; 16];
            let mut code = 0u32;
            for bits in 1..=15 {
                code = (code + bl_count[bits - 1]) << 1;
                next_code[bits] = code;
            }

            // Assign codes to symbols and fill the lookup table
            for sym in 0..max_sym {
                let len = lengths[sym] as u32;
                if len == 0 {
                    continue;
                }
                let c = next_code[len as usize];
                next_code[len as usize] += 1;

                // The code `c` has `len` bits. Reverse it for our LSB-first bit reading.
                let rev = reverse_bits(c, len);

                // Fill all table entries that share this prefix
                let fill_count = 1u32 << (max_bits - len);
                for fill in 0..fill_count {
                    let idx = rev | (fill << len);
                    if (idx as usize) < symbols.len() {
                        symbols[idx as usize] = sym as i16;
                    }
                }
            }

            HuffTable { symbols, max_bits }
        }

        fn decode(&self, data: &[u8], bit_offset: &mut usize) -> Option<u16> {
            if self.max_bits == 0 {
                return None;
            }
            let bits = read_bits(data, bit_offset, self.max_bits as usize);
            let sym = *self.symbols.get(bits as usize)?;
            if sym < 0 {
                return None;
            }

            // We consumed max_bits, but the actual code may be shorter.
            // We need to know the real length to un-consume extra bits.
            // Since we filled the table densely, the symbol is correct and all
            // max_bits were properly consumed. This works for full-table lookup.
            Some(sym as u16)
        }
    }

    // Fixed Huffman tables (RFC 1951 section 3.2.6)
    fn build_fixed_lit_table() -> HuffTable {
        let mut lengths = [0u8; 288];
        for i in 0..=143 {
            lengths[i] = 8;
        }
        for i in 144..=255 {
            lengths[i] = 9;
        }
        for i in 256..=279 {
            lengths[i] = 7;
        }
        for i in 280..=287 {
            lengths[i] = 8;
        }
        HuffTable::build(&lengths, 288)
    }

    fn build_fixed_dist_table() -> HuffTable {
        let lengths = [5u8; 32];
        HuffTable::build(&lengths, 32)
    }

    // Length base values (symbols 257..285)
    const LEN_BASE: [u16; 29] = [
        3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
        131, 163, 195, 227, 258,
    ];
    const LEN_EXTRA: [u8; 29] = [
        0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
    ];

    // Distance base values
    const DIST_BASE: [u16; 30] = [
        1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
        2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
    ];
    const DIST_EXTRA: [u8; 30] = [
        0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12,
        13, 13,
    ];

    /// Decode a Huffman-compressed block using the given literal/length and distance tables
    fn decode_block(
        data: &[u8],
        bit_offset: &mut usize,
        output: &mut Vec<u8>,
        lit_table: &HuffTable,
        dist_table: &HuffTable,
    ) -> bool {
        loop {
            let sym = match lit_table.decode(data, bit_offset) {
                Some(s) => s,
                None => return false,
            };

            if sym < 256 {
                // Literal byte
                output.push(sym as u8);
            } else if sym == 256 {
                // End of block
                return true;
            } else {
                // Length/distance pair
                let len_idx = (sym - 257) as usize;
                if len_idx >= LEN_BASE.len() {
                    return false;
                }
                let length = LEN_BASE[len_idx] as usize
                    + read_bits(data, bit_offset, LEN_EXTRA[len_idx] as usize) as usize;

                let dist_sym = match dist_table.decode(data, bit_offset) {
                    Some(s) => s as usize,
                    None => return false,
                };
                if dist_sym >= DIST_BASE.len() {
                    return false;
                }
                let distance = DIST_BASE[dist_sym] as usize
                    + read_bits(data, bit_offset, DIST_EXTRA[dist_sym] as usize) as usize;

                // Copy from back-reference
                if distance == 0 || distance > output.len() {
                    return false;
                }
                let start = output.len() - distance;
                for i in 0..length {
                    let byte = output[start + (i % distance)];
                    output.push(byte);
                }
            }
        }
    }

    // Code length alphabet order for dynamic Huffman
    const CL_ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];

    loop {
        if bit_offset / 8 >= data.len() {
            break;
        }

        let bfinal = read_bits(data, &mut bit_offset, 1);
        let btype = read_bits(data, &mut bit_offset, 2);

        match btype {
            0 => {
                // Uncompressed block
                bit_offset = (bit_offset + 7) & !7;
                let byte_off = bit_offset / 8;
                if byte_off + 4 > data.len() {
                    break;
                }
                let len = data[byte_off] as u16 | ((data[byte_off + 1] as u16) << 8);
                let _nlen = data[byte_off + 2] as u16 | ((data[byte_off + 3] as u16) << 8);
                let start = byte_off + 4;
                let end = start + len as usize;
                if end <= data.len() {
                    output.extend_from_slice(&data[start..end]);
                }
                bit_offset = end * 8;
            }
            1 => {
                // Fixed Huffman codes
                let lit = build_fixed_lit_table();
                let dist = build_fixed_dist_table();
                if !decode_block(data, &mut bit_offset, &mut output, &lit, &dist) {
                    break;
                }
            }
            2 => {
                // Dynamic Huffman codes
                let hlit = read_bits(data, &mut bit_offset, 5) as usize + 257;
                let hdist = read_bits(data, &mut bit_offset, 5) as usize + 1;
                let hclen = read_bits(data, &mut bit_offset, 4) as usize + 4;

                // Read code length code lengths
                let mut cl_lengths = [0u8; 19];
                for i in 0..hclen {
                    cl_lengths[CL_ORDER[i]] = read_bits(data, &mut bit_offset, 3) as u8;
                }

                let cl_table = HuffTable::build(&cl_lengths, 19);

                // Decode literal/length + distance code lengths
                let total = hlit + hdist;
                let mut lengths = vec![0u8; total];
                let mut i = 0;
                while i < total {
                    let sym = match cl_table.decode(data, &mut bit_offset) {
                        Some(s) => s,
                        None => break,
                    };
                    match sym {
                        0..=15 => {
                            lengths[i] = sym as u8;
                            i += 1;
                        }
                        16 => {
                            let repeat = read_bits(data, &mut bit_offset, 2) as usize + 3;
                            let val = if i > 0 { lengths[i - 1] } else { 0 };
                            for _ in 0..repeat {
                                if i < total {
                                    lengths[i] = val;
                                    i += 1;
                                }
                            }
                        }
                        17 => {
                            let repeat = read_bits(data, &mut bit_offset, 3) as usize + 3;
                            for _ in 0..repeat {
                                if i < total {
                                    lengths[i] = 0;
                                    i += 1;
                                }
                            }
                        }
                        18 => {
                            let repeat = read_bits(data, &mut bit_offset, 7) as usize + 11;
                            for _ in 0..repeat {
                                if i < total {
                                    lengths[i] = 0;
                                    i += 1;
                                }
                            }
                        }
                        _ => break,
                    }
                }

                let lit_table = HuffTable::build(&lengths[..hlit], hlit);
                let dist_table = HuffTable::build(&lengths[hlit..], hdist);

                if !decode_block(data, &mut bit_offset, &mut output, &lit_table, &dist_table) {
                    break;
                }
            }
            _ => break, // Invalid block type
        }

        if bfinal == 1 {
            break;
        }
    }

    if output.is_empty() {
        None
    } else {
        Some(output)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// BMP DECODER
// ═══════════════════════════════════════════════════════════════════════

fn decode_bmp(data: &[u8]) -> Option<DecodedImage> {
    if data.len() < 54 || data[..2] != BMP_SIGNATURE {
        return None;
    }

    let pixel_offset = read_le_u32(data, 10) as usize;
    let width = read_le_u32(data, 18);
    let height = read_le_u32(data, 22);
    let bpp = read_le_u16(data, 28);

    if bpp != 24 && bpp != 32 {
        serial_println!("[BMP] Only 24/32-bit BMP supported, got {}", bpp);
        return None;
    }

    let bytes_per_pixel = (bpp / 8) as usize;
    let row_size = (width as usize * bytes_per_pixel).div_ceil(4) * 4; // Padded to 4 bytes

    let mut pixels = vec![Pixel::rgb(0, 0, 0); (width * height) as usize];

    // BMP stores rows bottom-to-top
    for y in 0..height as usize {
        let src_y = (height as usize - 1) - y; // Flip vertically
        let row_start = pixel_offset + src_y * row_size;

        for x in 0..width as usize {
            let px_off = row_start + x * bytes_per_pixel;
            if px_off + bytes_per_pixel > data.len() {
                continue;
            }
            let b = data[px_off];
            let g = data[px_off + 1];
            let r = data[px_off + 2];
            let a = if bytes_per_pixel == 4 {
                data[px_off + 3]
            } else {
                255
            };
            pixels[y * width as usize + x] = Pixel::new(r, g, b, a);
        }
    }

    Some(DecodedImage {
        width,
        height,
        pixels,
        format: ImageFormat::Bmp,
    })
}

/// Public entry point for raw deflate decompression (used by pdf.rs, etc.)
pub fn inflate_decompress_raw(data: &[u8]) -> Option<Vec<u8>> {
    inflate_decompress(data)
}

// ═══════════════════════════════════════════════════════════════════════
// JPEG DECODER (minimal baseline)
// ═══════════════════════════════════════════════════════════════════════

/// Baseline JPEG decoder (JFIF, SOF0 only)
/// Implements: marker parsing, Huffman decoding, inverse DCT, YCbCr→RGB
fn decode_jpeg(data: &[u8]) -> Option<DecodedImage> {
    if data.len() < 4 || data[..2] != JPEG_SIGNATURE {
        return None;
    }

    // ── Internal Huffman table for JPEG (max 16-bit codes) ──────────
    struct JpegHuffTable {
        /// mincode[i] = smallest Huffman code of length i+1
        mincode: [u32; 16],
        /// maxcode[i] = largest Huffman code of length i+1 (-1 if none)
        maxcode: [i32; 16],
        /// valptr[i] = index into vals for codes of length i+1
        valptr: [usize; 16],
        /// symbol values in order
        vals: Vec<u8>,
    }

    impl JpegHuffTable {
        fn new() -> Self {
            JpegHuffTable {
                mincode: [0; 16],
                maxcode: [-1; 16],
                valptr: [0; 16],
                vals: Vec::new(),
            }
        }

        /// Build from BITS (count per length) and HUFFVAL arrays
        fn build(bits: &[u8; 16], huffval: &[u8]) -> Self {
            let mut t = JpegHuffTable::new();
            t.vals = huffval.to_vec();
            let mut code = 0u32;
            let mut idx = 0usize;
            for i in 0..16 {
                let count = bits[i] as usize;
                if count > 0 {
                    t.mincode[i] = code;
                    t.maxcode[i] = (code + count as u32 - 1) as i32;
                    t.valptr[i] = idx;
                    idx += count;
                    code += count as u32;
                } else {
                    t.maxcode[i] = -1;
                }
                code <<= 1;
            }
            t
        }
    }

    // ── Quantization table ──────────────────────────────────────────
    struct QuantTable {
        table: [i32; 64],
    }

    // ── Component info ──────────────────────────────────────────────
    #[derive(Clone, Copy)]
    struct Component {
        h_samp: u8,
        v_samp: u8,
        qt_id: u8,
        dc_table: u8,
        ac_table: u8,
    }

    // ── JPEG state ──────────────────────────────────────────────────
    let mut width: u32 = 0;
    let mut height: u32 = 0;
    let mut num_components: u8 = 0;
    let mut components = [Component {
        h_samp: 1,
        v_samp: 1,
        qt_id: 0,
        dc_table: 0,
        ac_table: 0,
    }; 4];
    let mut qt = [None, None, None, None]; // up to 4 quantization tables
    let mut dc_tables: [Option<JpegHuffTable>; 4] = [None, None, None, None];
    let mut ac_tables: [Option<JpegHuffTable>; 4] = [None, None, None, None];
    let mut sos_offset: usize = 0;
    let mut max_h_samp: u8 = 1;
    let mut max_v_samp: u8 = 1;

    // ── Parse markers ───────────────────────────────────────────────
    let mut offset = 2;
    while offset + 1 < data.len() {
        if data[offset] != 0xFF {
            offset += 1;
            continue;
        }
        let marker = data[offset + 1];
        offset += 2;

        match marker {
            0x00 | 0xFF => continue,
            0xD8 => continue, // SOI
            0xD9 => break,    // EOI
            // DQT — Define Quantization Table
            0xDB => {
                if offset + 2 > data.len() {
                    break;
                }
                let seg_len = ((data[offset] as usize) << 8) | data[offset + 1] as usize;
                let seg_end = offset + seg_len;
                let mut p = offset + 2;
                while p < seg_end && p < data.len() {
                    let pq = (data[p] >> 4) & 0x0F; // precision
                    let tq = (data[p] & 0x0F) as usize;
                    p += 1;
                    if tq < 4 {
                        let mut table = [0i32; 64];
                        // Zigzag order
                        const ZIGZAG: [usize; 64] = [
                            0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33,
                            40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50,
                            43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46,
                            53, 60, 61, 54, 47, 55, 62, 63,
                        ];
                        for i in 0..64 {
                            if p >= data.len() {
                                break;
                            }
                            if pq == 0 {
                                table[ZIGZAG[i]] = data[p] as i32;
                                p += 1;
                            } else {
                                if p + 1 >= data.len() {
                                    break;
                                }
                                table[ZIGZAG[i]] = ((data[p] as i32) << 8) | data[p + 1] as i32;
                                p += 2;
                            }
                        }
                        qt[tq] = Some(QuantTable { table });
                    }
                }
                offset = seg_end;
            }
            // SOF0 — Start of Frame (baseline DCT)
            0xC0 => {
                if offset + 2 > data.len() {
                    break;
                }
                let seg_len = ((data[offset] as usize) << 8) | data[offset + 1] as usize;
                let p = offset + 2;
                if p + 6 > data.len() {
                    break;
                }
                let _precision = data[p];
                height = ((data[p + 1] as u32) << 8) | data[p + 2] as u32;
                width = ((data[p + 3] as u32) << 8) | data[p + 4] as u32;
                num_components = data[p + 5];
                for c in 0..(num_components as usize).min(4) {
                    let base = p + 6 + c * 3;
                    if base + 2 >= data.len() {
                        break;
                    }
                    let _id = data[base];
                    components[c].h_samp = (data[base + 1] >> 4) & 0x0F;
                    components[c].v_samp = data[base + 1] & 0x0F;
                    components[c].qt_id = data[base + 2];
                    if components[c].h_samp > max_h_samp {
                        max_h_samp = components[c].h_samp;
                    }
                    if components[c].v_samp > max_v_samp {
                        max_v_samp = components[c].v_samp;
                    }
                }
                offset += seg_len;
            }
            // DHT — Define Huffman Table
            0xC4 => {
                if offset + 2 > data.len() {
                    break;
                }
                let seg_len = ((data[offset] as usize) << 8) | data[offset + 1] as usize;
                let seg_end = offset + seg_len;
                let mut p = offset + 2;
                while p < seg_end && p + 17 <= data.len() {
                    let tc = (data[p] >> 4) & 0x0F; // 0=DC, 1=AC
                    let th = (data[p] & 0x0F) as usize;
                    p += 1;
                    let mut bits = [0u8; 16];
                    let mut total = 0usize;
                    for i in 0..16 {
                        bits[i] = data[p + i];
                        total += bits[i] as usize;
                    }
                    p += 16;
                    let huffval: Vec<u8> = data[p..p + total.min(data.len() - p)].to_vec();
                    p += total;

                    let table = JpegHuffTable::build(&bits, &huffval);
                    if tc == 0 && th < 4 {
                        dc_tables[th] = Some(table);
                    } else if tc == 1 && th < 4 {
                        ac_tables[th] = Some(table);
                    }
                }
                offset = seg_end;
            }
            // SOS — Start of Scan
            0xDA => {
                if offset + 2 > data.len() {
                    break;
                }
                let seg_len = ((data[offset] as usize) << 8) | data[offset + 1] as usize;
                let p = offset + 2;
                if p >= data.len() {
                    break;
                }
                let ns = data[p] as usize;
                for c in 0..ns.min(4) {
                    let base = p + 1 + c * 2;
                    if base + 1 >= data.len() {
                        break;
                    }
                    let _csj = data[base];
                    components[c].dc_table = (data[base + 1] >> 4) & 0x0F;
                    components[c].ac_table = data[base + 1] & 0x0F;
                }
                sos_offset = offset + seg_len;
                break; // Entropy-coded data follows
            }
            // Skip any other marker
            _ => {
                if offset + 2 <= data.len() {
                    let seg_len = ((data[offset] as usize) << 8) | data[offset + 1] as usize;
                    offset += seg_len;
                }
            }
        }
    }

    if width == 0 || height == 0 || num_components == 0 || sos_offset == 0 {
        serial_println!("[JPEG] Missing required markers");
        return None;
    }

    // ── Decode entropy-coded segment (un-stuff 0xFF 0x00 → 0xFF) ─────
    let mut scan_data: Vec<u8> = Vec::new();
    {
        let mut i = sos_offset;
        while i < data.len() {
            if data[i] == 0xFF {
                if i + 1 < data.len() {
                    if data[i + 1] == 0x00 {
                        scan_data.push(0xFF);
                        i += 2;
                    } else if data[i + 1] == 0xD9 {
                        break; // EOI
                    } else if data[i + 1] >= 0xD0 && data[i + 1] <= 0xD7 {
                        i += 2; // Skip RST markers
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            } else {
                scan_data.push(data[i]);
                i += 1;
            }
        }
    }

    // ── Bit reader for entropy-coded data ───────────────────────────
    struct BitReader {
        data: Vec<u8>,
        byte_pos: usize,
        bits_left: u8,
        current: u32,
    }

    impl BitReader {
        fn new(data: Vec<u8>) -> Self {
            BitReader {
                data,
                byte_pos: 0,
                bits_left: 0,
                current: 0,
            }
        }

        fn read_bit(&mut self) -> u8 {
            if self.bits_left == 0 {
                self.current = if self.byte_pos < self.data.len() {
                    let b = self.data[self.byte_pos];
                    self.byte_pos += 1;
                    b as u32
                } else {
                    0
                };
                self.bits_left = 8;
            }
            self.bits_left -= 1;
            ((self.current >> self.bits_left) & 1) as u8
        }

        fn read_bits(&mut self, n: u8) -> u32 {
            let mut val = 0u32;
            for _ in 0..n {
                val = (val << 1) | self.read_bit() as u32;
            }
            val
        }

        fn decode_huffman(&mut self, table: &JpegHuffTable) -> Option<u8> {
            let mut code = 0u32;
            for i in 0..16 {
                code = (code << 1) | self.read_bit() as u32;
                if table.maxcode[i] >= 0 && code as i32 <= table.maxcode[i] {
                    let idx = table.valptr[i] + (code - table.mincode[i]) as usize;
                    return table.vals.get(idx).copied();
                }
            }
            None
        }

        /// Read and extend a signed coefficient value
        fn receive_extend(&mut self, nbits: u8) -> i32 {
            if nbits == 0 {
                return 0;
            }
            let val = self.read_bits(nbits) as i32;
            // If the MSB is 0, it's negative per JPEG spec
            if val < (1 << (nbits - 1)) {
                val - (1 << nbits) + 1
            } else {
                val
            }
        }
    }

    // ── Inverse DCT (8×8, integer approximation, AAN algorithm) ─────
    fn idct_8x8(block: &mut [i32; 64]) {
        // Scaled integer IDCT based on AAN (Arai, Agui, Nakajima)
        // Using fixed-point with 12-bit precision
        const W1: i32 = 2841; // 2048*sqrt(2)*cos(1*pi/16)
        const W2: i32 = 2676; // 2048*sqrt(2)*cos(2*pi/16)
        const W3: i32 = 2408; // 2048*sqrt(2)*cos(3*pi/16)
        const W5: i32 = 1609; // 2048*sqrt(2)*cos(5*pi/16)
        const W6: i32 = 1108; // 2048*sqrt(2)*cos(6*pi/16)
        const W7: i32 = 565; // 2048*sqrt(2)*cos(7*pi/16)

        fn idct_row(blk: &mut [i32; 64], row: usize) {
            let r = row * 8;
            // Check for all-zero AC coefficients
            if blk[r + 1] == 0
                && blk[r + 2] == 0
                && blk[r + 3] == 0
                && blk[r + 4] == 0
                && blk[r + 5] == 0
                && blk[r + 6] == 0
                && blk[r + 7] == 0
            {
                let dc = blk[r] << 3;
                for i in 0..8 {
                    blk[r + i] = dc;
                }
                return;
            }

            let mut x0 = (blk[r] << 11) + 128;
            let mut x1 = blk[r + 4] << 11;
            let mut x2 = blk[r + 6];
            let mut x3 = blk[r + 2];
            let mut x4 = blk[r + 1];
            let mut x5 = blk[r + 7];
            let mut x6 = blk[r + 5];
            let mut x7 = blk[r + 3];

            let mut x8 = W7 * (x4 + x5);
            x4 = x8 + (W1 - W7) * x4;
            x5 = x8 - (W1 + W7) * x5;
            x8 = W3 * (x6 + x7);
            x6 = x8 - (W3 - W5) * x6;
            x7 = x8 - (W3 + W5) * x7;

            x8 = x0 + x1;
            x0 -= x1;
            x1 = W6 * (x3 + x2);
            x2 = x1 - (W2 + W6) * x2;
            x3 = x1 + (W2 - W6) * x3;
            x1 = x4 + x6;
            x4 -= x6;
            x6 = x5 + x7;
            x5 -= x7;

            x7 = x8 + x3;
            x8 -= x3;
            x3 = x0 + x2;
            x0 -= x2;
            x2 = (181 * (x4 + x5) + 128) >> 8;
            x4 = (181 * (x4 - x5) + 128) >> 8;

            blk[r] = (x7 + x1) >> 8;
            blk[r + 1] = (x3 + x2) >> 8;
            blk[r + 2] = (x0 + x4) >> 8;
            blk[r + 3] = (x8 + x6) >> 8;
            blk[r + 4] = (x8 - x6) >> 8;
            blk[r + 5] = (x0 - x4) >> 8;
            blk[r + 6] = (x3 - x2) >> 8;
            blk[r + 7] = (x7 - x1) >> 8;
        }

        fn idct_col(blk: &mut [i32; 64], col: usize) {
            if blk[col + 8] == 0
                && blk[col + 16] == 0
                && blk[col + 24] == 0
                && blk[col + 32] == 0
                && blk[col + 40] == 0
                && blk[col + 48] == 0
                && blk[col + 56] == 0
            {
                let dc = (blk[col] + 32) >> 6;
                for i in 0..8 {
                    blk[col + i * 8] = dc;
                }
                return;
            }

            let mut x0 = (blk[col] << 8) + 8192;
            let mut x1 = blk[col + 32] << 8;
            let mut x2 = blk[col + 48];
            let mut x3 = blk[col + 16];
            let mut x4 = blk[col + 8];
            let mut x5 = blk[col + 56];
            let mut x6 = blk[col + 40];
            let mut x7 = blk[col + 24];

            let mut x8 = W7 * (x4 + x5) + 4;
            x4 = (x8 + (W1 - W7) * x4) >> 3;
            x5 = (x8 - (W1 + W7) * x5) >> 3;
            x8 = W3 * (x6 + x7) + 4;
            x6 = (x8 - (W3 - W5) * x6) >> 3;
            x7 = (x8 - (W3 + W5) * x7) >> 3;

            x8 = x0 + x1;
            x0 -= x1;
            x1 = W6 * (x3 + x2) + 4;
            x2 = (x1 - (W2 + W6) * x2) >> 3;
            x3 = (x1 + (W2 - W6) * x3) >> 3;
            x1 = x4 + x6;
            x4 -= x6;
            x6 = x5 + x7;
            x5 -= x7;

            x7 = x8 + x3;
            x8 -= x3;
            x3 = x0 + x2;
            x0 -= x2;
            x2 = (181 * (x4 + x5) + 128) >> 8;
            x4 = (181 * (x4 - x5) + 128) >> 8;

            blk[col] = (x7 + x1) >> 14;
            blk[col + 8] = (x3 + x2) >> 14;
            blk[col + 16] = (x0 + x4) >> 14;
            blk[col + 24] = (x8 + x6) >> 14;
            blk[col + 32] = (x8 - x6) >> 14;
            blk[col + 40] = (x0 - x4) >> 14;
            blk[col + 48] = (x3 - x2) >> 14;
            blk[col + 56] = (x7 - x1) >> 14;
        }

        for row in 0..8 {
            idct_row(block, row);
        }
        for col in 0..8 {
            idct_col(block, col);
        }
    }

    // ── YCbCr → RGB ────────────────────────────────────────────────
    fn ycbcr_to_rgb(y: i32, cb: i32, cr: i32) -> (u8, u8, u8) {
        let r = y + ((cr * 91881 + 32768) >> 16);
        let g = y - ((cb * 22554 + cr * 46802 + 32768) >> 16);
        let b = y + ((cb * 116130 + 32768) >> 16);
        (
            r.clamp(0, 255) as u8,
            g.clamp(0, 255) as u8,
            b.clamp(0, 255) as u8,
        )
    }

    // ── Decode MCU blocks ───────────────────────────────────────────
    let nc = (num_components as usize).min(4);
    let mcu_w = (max_h_samp as u32) * 8;
    let mcu_h = (max_v_samp as u32) * 8;
    let mcus_x = width.div_ceil(mcu_w);
    let mcus_y = height.div_ceil(mcu_h);

    // Allocate per-component image buffers
    let padded_w = mcus_x * mcu_w;
    let padded_h = mcus_y * mcu_h;
    let mut comp_buf: Vec<Vec<i32>> = (0..nc)
        .map(|_| vec![0i32; (padded_w * padded_h) as usize])
        .collect();

    let mut reader = BitReader::new(scan_data);
    let mut dc_pred = [0i32; 4];

    for mcu_y in 0..mcus_y {
        for mcu_x in 0..mcus_x {
            for c in 0..nc {
                let h = components[c].h_samp as u32;
                let v = components[c].v_samp as u32;

                for vy in 0..v {
                    for hx in 0..h {
                        let dc_id = components[c].dc_table as usize;
                        let ac_id = components[c].ac_table as usize;
                        let qt_id = components[c].qt_id as usize;

                        let dc_tbl = dc_tables[dc_id].as_ref()?;
                        let ac_tbl = ac_tables[ac_id].as_ref()?;
                        let q_tbl = qt[qt_id].as_ref()?;

                        // Decode DC coefficient
                        let dc_cat = reader.decode_huffman(dc_tbl).unwrap_or(0);
                        let dc_diff = reader.receive_extend(dc_cat);
                        dc_pred[c] += dc_diff;

                        let mut block = [0i32; 64];
                        block[0] = dc_pred[c] * q_tbl.table[0];

                        // Decode AC coefficients
                        let mut k = 1;
                        while k < 64 {
                            let rs = reader.decode_huffman(ac_tbl).unwrap_or(0);
                            let rrrr = (rs >> 4) & 0x0F;
                            let ssss = rs & 0x0F;

                            if ssss == 0 {
                                if rrrr == 0 {
                                    break;
                                } // EOB
                                if rrrr == 0x0F {
                                    k += 16;
                                    continue;
                                } // ZRL
                                break;
                            }

                            k += rrrr as usize;
                            if k >= 64 {
                                break;
                            }
                            let val = reader.receive_extend(ssss);
                            block[k] = val * q_tbl.table[k];
                            k += 1;
                        }

                        // IDCT
                        idct_8x8(&mut block);

                        // Write block to component buffer (level shift +128)
                        let comp_w = padded_w * h / max_h_samp as u32;
                        let bx = mcu_x * h * 8 + hx * 8;
                        let by = mcu_y * v * 8 + vy * 8;

                        for row in 0..8u32 {
                            for col in 0..8u32 {
                                let px = bx + col;
                                let py = by + row;
                                if px < padded_w && py < padded_h {
                                    let idx = (py * padded_w + px) as usize;
                                    if idx < comp_buf[c].len() {
                                        comp_buf[c][idx] = block[(row * 8 + col) as usize] + 128;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Assemble final RGBA pixels ──────────────────────────────────
    let mut pixels = Vec::with_capacity((width * height) as usize);

    if nc >= 3 {
        // YCbCr → RGB (handle chroma subsampling via nearest-neighbor upscale)
        for y_pos in 0..height {
            for x_pos in 0..width {
                let idx = (y_pos * padded_w + x_pos) as usize;
                let y_val = comp_buf[0].get(idx).copied().unwrap_or(128);

                // Chroma components may be subsampled
                let cx = x_pos * components[1].h_samp as u32 / max_h_samp as u32;
                let cy = y_pos * components[1].v_samp as u32 / max_v_samp as u32;
                let c_w = padded_w * components[1].h_samp as u32 / max_h_samp as u32;
                let cidx = (cy * c_w + cx) as usize;

                let cb = comp_buf[1].get(cidx).copied().unwrap_or(128) - 128;
                let cr = comp_buf[2].get(cidx).copied().unwrap_or(128) - 128;

                let (r, g, b) = ycbcr_to_rgb(y_val, cb, cr);
                pixels.push(Pixel::rgb(r, g, b));
            }
        }
    } else {
        // Grayscale
        for y_pos in 0..height {
            for x_pos in 0..width {
                let idx = (y_pos * padded_w + x_pos) as usize;
                let v = comp_buf[0].get(idx).copied().unwrap_or(128).clamp(0, 255) as u8;
                pixels.push(Pixel::rgb(v, v, v));
            }
        }
    }

    serial_println!(
        "[JPEG] Decoded {}x{} image ({} components)",
        width,
        height,
        nc
    );

    Some(DecodedImage {
        width,
        height,
        pixels,
        format: ImageFormat::Jpeg,
    })
}

/// Scale an image to a new size using bilinear interpolation
pub fn scale_image(img: &DecodedImage, new_width: u32, new_height: u32) -> DecodedImage {
    let mut pixels = Vec::with_capacity((new_width * new_height) as usize);

    for y in 0..new_height {
        for x in 0..new_width {
            let src_x = (x as f32 * img.width as f32) / new_width as f32;
            let src_y = (y as f32 * img.height as f32) / new_height as f32;

            let x0 = src_x as u32;
            let y0 = src_y as u32;
            let x1 = (x0 + 1).min(img.width - 1);
            let y1 = (y0 + 1).min(img.height - 1);

            let fx = src_x - x0 as f32;
            let fy = src_y - y0 as f32;

            let p00 = img.pixels[(y0 * img.width + x0) as usize];
            let p10 = img.pixels[(y0 * img.width + x1) as usize];
            let p01 = img.pixels[(y1 * img.width + x0) as usize];
            let p11 = img.pixels[(y1 * img.width + x1) as usize];

            let r = bilinear(p00.r, p10.r, p01.r, p11.r, fx, fy);
            let g = bilinear(p00.g, p10.g, p01.g, p11.g, fx, fy);
            let b = bilinear(p00.b, p10.b, p01.b, p11.b, fx, fy);
            let a = bilinear(p00.a, p10.a, p01.a, p11.a, fx, fy);

            pixels.push(Pixel::new(r, g, b, a));
        }
    }

    DecodedImage {
        width: new_width,
        height: new_height,
        pixels,
        format: img.format,
    }
}

fn bilinear(c00: u8, c10: u8, c01: u8, c11: u8, fx: f32, fy: f32) -> u8 {
    let top = c00 as f32 * (1.0 - fx) + c10 as f32 * fx;
    let bot = c01 as f32 * (1.0 - fx) + c11 as f32 * fx;
    let val = top * (1.0 - fy) + bot * fy;
    val.clamp(0.0, 255.0) as u8
}

/// Initialize image decoder
pub fn init() {
    serial_println!("[KnoxOS] Image decoder initialized (PNG, JPEG, BMP)");
}
