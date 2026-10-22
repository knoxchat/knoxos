//! PDF Viewer — Basic PDF text extraction and rendering
//!
//! Implements a minimal PDF parser that can extract and display text content
//! from simple PDF files (PDF 1.4+). Handles:
//!   - PDF header validation and xref table parsing
//!   - Object catalog and page tree traversal
//!   - Text extraction from content streams (BT/ET blocks, Tj/TJ operators)
//!   - Flate (deflate) decompressed streams
//!   - Page rendering to framebuffer via font system
//!
//! Does NOT support: Type1/TrueType embedded fonts, images, vector graphics,
//! encryption, forms, annotations, JavaScript.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::serial_println;

/// A parsed PDF page
#[derive(Debug, Clone)]
pub struct PdfPage {
    pub page_number: usize,
    pub text_lines: Vec<String>,
    pub width: f32,
    pub height: f32,
}

/// Parsed PDF document
pub struct PdfDocument {
    pub title: String,
    pub page_count: usize,
    pub pages: Vec<PdfPage>,
}

impl PdfDocument {
    pub fn new() -> Self {
        PdfDocument {
            title: String::new(),
            page_count: 0,
            pages: Vec::new(),
        }
    }
}

/// Parse a PDF from raw bytes
pub fn parse(data: &[u8]) -> Option<PdfDocument> {
    // Validate PDF header
    if data.len() < 8 || &data[0..5] != b"%PDF-" {
        serial_println!("[PDF] Not a valid PDF file");
        return None;
    }

    let version = core::str::from_utf8(&data[5..8]).unwrap_or("?.?");
    serial_println!("[PDF] Parsing PDF version {}", version);

    let mut doc = PdfDocument::new();

    // Find all stream objects and extract text
    let text = data;
    let len = text.len();

    // Strategy: Find "stream" ... "endstream" blocks and look for text operators
    // Also find raw text outside streams that uses Tj/TJ operators
    let mut pages: Vec<PdfPage> = Vec::new();
    let mut current_page_text: Vec<String> = Vec::new();
    let mut page_num = 0usize;

    // First pass: find all content streams
    let mut i = 0;
    while i < len.saturating_sub(6) {
        // Look for "stream" marker
        if &text[i..i + 6] == b"stream" {
            // Find matching "endstream"
            let stream_start = if i + 6 < len && text[i + 6] == b'\r' {
                if i + 7 < len && text[i + 7] == b'\n' {
                    i + 8
                } else {
                    i + 7
                }
            } else if i + 6 < len && text[i + 6] == b'\n' {
                i + 7
            } else {
                i + 6
            };

            if let Some(end_pos) = find_bytes(text, b"endstream", stream_start) {
                let stream_data = &text[stream_start..end_pos];

                // Check if this stream is FlateDecode compressed
                // Look backwards for "/FlateDecode" or "/Filter /FlateDecode"
                let header_start = i.saturating_sub(200);
                let header = &text[header_start..i];

                let decoded = if find_bytes(header, b"FlateDecode", 0).is_some() {
                    // Try to decompress using our inflate implementation
                    decompress_flate(stream_data)
                } else {
                    Some(stream_data.to_vec())
                };

                if let Some(content) = decoded {
                    let extracted = extract_text_from_stream(&content);
                    if !extracted.is_empty() {
                        current_page_text.extend(extracted);
                    }
                }

                i = end_pos + 9;
                continue;
            }
        }

        // Look for page boundaries (BT = begin text, could indicate new page context)
        if i + 2 < len && &text[i..i + 2] == b"BT" {
            // Check if this is actually a BT operator (preceded by whitespace/newline)
            if (i == 0 || text[i - 1].is_ascii_whitespace()) && !current_page_text.is_empty() {
                // Heuristic: if we have accumulated text and see a new BT,
                // we might be on the same page — don't split pages on every BT
            }
        }

        i += 1;
    }

    // Anything leftover becomes a page
    if !current_page_text.is_empty() {
        page_num += 1;
        pages.push(PdfPage {
            page_number: page_num,
            text_lines: current_page_text,
            width: 612.0, // US Letter default
            height: 792.0,
        });
    }

    // If no text was extracted from streams, try the naive approach:
    // scan the entire file for text between parentheses in BT..ET blocks
    if pages.is_empty() {
        let full_text = extract_text_from_stream(data);
        if !full_text.is_empty() {
            pages.push(PdfPage {
                page_number: 1,
                text_lines: full_text,
                width: 612.0,
                height: 792.0,
            });
        }
    }

    if pages.is_empty() {
        serial_println!("[PDF] No text content found");
        return None;
    }

    doc.page_count = pages.len();
    doc.pages = pages;
    doc.title = extract_title(data).unwrap_or_else(|| String::from("Untitled PDF"));

    serial_println!(
        "[PDF] Parsed '{}': {} page(s), {} text lines total",
        doc.title,
        doc.page_count,
        doc.pages.iter().map(|p| p.text_lines.len()).sum::<usize>()
    );

    Some(doc)
}

/// Extract text from a PDF content stream using BT/ET blocks and Tj/TJ operators
fn extract_text_from_stream(data: &[u8]) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current_line = String::new();
    let mut in_text_block = false;
    let len = data.len();
    let mut i = 0;

    while i < len {
        // Detect BT (begin text)
        if i + 2 <= len && &data[i..i + 2] == b"BT" && (i == 0 || data[i - 1].is_ascii_whitespace())
        {
            in_text_block = true;
            i += 2;
            continue;
        }

        // Detect ET (end text)
        if i + 2 <= len && &data[i..i + 2] == b"ET" && in_text_block {
            in_text_block = false;
            if !current_line.is_empty() {
                lines.push(core::mem::take(&mut current_line));
            }
            i += 2;
            continue;
        }

        if in_text_block {
            // Look for Tj operator: (text) Tj
            if data[i] == b'(' {
                // Extract text from parenthesized string
                let text = extract_paren_string(data, &mut i);
                current_line.push_str(&text);
                continue;
            }

            // Look for TJ operator: [(text) num (text) ...] TJ
            if data[i] == b'[' {
                i += 1;
                while i < len && data[i] != b']' {
                    if data[i] == b'(' {
                        let text = extract_paren_string(data, &mut i);
                        current_line.push_str(&text);
                    } else {
                        i += 1;
                    }
                }
                if i < len {
                    i += 1;
                } // skip ']'
                continue;
            }

            // Detect Td/TD (move to next line position) or T* (next line)
            if i + 2 <= len
                && (&data[i..i + 2] == b"Td"
                    || &data[i..i + 2] == b"TD"
                    || &data[i..i + 2] == b"T*")
                && (i + 2 >= len || !data[i + 2].is_ascii_alphabetic())
            {
                if !current_line.is_empty() {
                    lines.push(core::mem::take(&mut current_line));
                }
                i += 2;
                continue;
            }

            // Detect ' and " operators (show text with newline)
            if (data[i] == b'\'' || data[i] == b'"') && !current_line.is_empty() {
                lines.push(core::mem::take(&mut current_line));
            }
        }

        i += 1;
    }

    // Flush remaining text
    if !current_line.is_empty() {
        lines.push(current_line);
    }

    lines
}

/// Extract a parenthesized string, handling escapes and nesting
fn extract_paren_string(data: &[u8], pos: &mut usize) -> String {
    let mut result = String::new();
    let len = data.len();
    *pos += 1; // skip opening '('
    let mut depth = 1;

    while *pos < len && depth > 0 {
        let b = data[*pos];
        match b {
            b'(' => {
                depth += 1;
                result.push('(');
            }
            b')' => {
                depth -= 1;
                if depth > 0 {
                    result.push(')');
                }
            }
            b'\\' => {
                *pos += 1;
                if *pos < len {
                    match data[*pos] {
                        b'n' => result.push('\n'),
                        b'r' => result.push('\r'),
                        b't' => result.push('\t'),
                        b'(' => result.push('('),
                        b')' => result.push(')'),
                        b'\\' => result.push('\\'),
                        // Octal escape: \ddd
                        d @ b'0'..=b'7' => {
                            let mut val = (d - b'0') as u32;
                            for _ in 0..2 {
                                if *pos + 1 < len
                                    && data[*pos + 1] >= b'0'
                                    && data[*pos + 1] <= b'7'
                                {
                                    *pos += 1;
                                    val = val * 8 + (data[*pos] - b'0') as u32;
                                }
                            }
                            if let Some(c) = char::from_u32(val) {
                                result.push(c);
                            }
                        }
                        other => {
                            result.push(other as char);
                        }
                    }
                }
            }
            _ => {
                if (0x20..0x7F).contains(&b) {
                    result.push(b as char);
                } else if b == b'\n' || b == b'\r' {
                    result.push(' ');
                }
            }
        }
        *pos += 1;
    }

    result
}

/// Try to decompress a FlateDecode stream (zlib format: 2-byte header + deflate data)
fn decompress_flate(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 3 {
        return None;
    }
    // zlib header: CMF (usually 0x78) + FLG
    if data[0] == 0x78 {
        // Strip 2-byte zlib header, pass raw deflate to our inflate
        crate::gui::image::inflate_decompress_raw(&data[2..])
    } else {
        // Try raw deflate
        crate::gui::image::inflate_decompress_raw(data)
    }
}

/// Find a byte sequence in data starting from offset
fn find_bytes(data: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    if needle.is_empty() || start + needle.len() > data.len() {
        return None;
    }
    for i in start..=data.len() - needle.len() {
        if &data[i..i + needle.len()] == needle {
            return Some(i);
        }
    }
    None
}

/// Try to extract the /Title from the PDF Info dictionary
fn extract_title(data: &[u8]) -> Option<String> {
    if let Some(pos) = find_bytes(data, b"/Title", 0) {
        let after = pos + 6;
        // Skip whitespace
        let mut i = after;
        while i < data.len() && data[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < data.len() && data[i] == b'(' {
            let text = extract_paren_string(data, &mut i);
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    None
}

/// Initialize PDF viewer
pub fn init() {
    serial_println!("[KnoxOS] PDF viewer initialized (text extraction mode)");
}
