/// Fish-inspired history-based autosuggestion engine
use alloc::string::String;
use alloc::vec::Vec;

use super::highlight::KNOWN_COMMANDS;

// ═══════════════════════════════════════════════════════════════════════════
// AUTOSUGGESTION ENGINE — Fish-inspired history-based suggestions
// ═══════════════════════════════════════════════════════════════════════════

/// Find the best autosuggestion from command history
pub fn find_suggestion(input: &str, history: &[String]) -> Option<String> {
    if input.is_empty() {
        return None;
    }

    // Search history in reverse (most recent first) for prefix match
    for entry in history.iter().rev() {
        if entry.starts_with(input) && entry.len() > input.len() {
            return Some(entry.clone());
        }
    }

    // Fallback: try known commands as suggestions
    for &cmd in KNOWN_COMMANDS {
        if cmd.starts_with(input) && cmd.len() > input.len() {
            return Some(String::from(cmd));
        }
    }

    None
}
