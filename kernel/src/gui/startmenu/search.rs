//! Case-insensitive fuzzy subsequence matching for app search.

// ═══════════════════════════════════════════════════════════════════════════
// FUZZY SEARCH
// ═══════════════════════════════════════════════════════════════════════════

/// Case-insensitive fuzzy subsequence match
pub(super) fn fuzzy_match(query: &str, target: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let mut query_chars = query.chars().peekable();
    for ch in target.chars() {
        if let Some(&qc) = query_chars.peek() {
            if ch.eq_ignore_ascii_case(&qc) {
                query_chars.next();
            }
        }
        if query_chars.peek().is_none() {
            return true;
        }
    }
    query_chars.peek().is_none()
}
