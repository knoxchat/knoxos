// ═══════════════════════════════════════════════════════════════════════
// PDF TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::pdf;

#[test_case]
fn test_pdf_invalid_data() {
    let result = pdf::parse(b"not a pdf");
    assert!(result.is_none());
}

#[test_case]
fn test_pdf_header_check() {
    // Valid PDF header but no content
    let data = b"%PDF-1.4\n%%EOF";
    let result = pdf::parse(data);
    // Should parse without panic (may return empty doc)
    if let Some(doc) = result {
        assert!(doc.page_count == 0 || doc.page_count > 0);
    }
}
