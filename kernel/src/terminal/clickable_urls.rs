/// Clickable URLs — detect and handle URLs in terminal output
///
/// Provides:
///   - URL detection via regex-like pattern matching
///   - Ctrl+click to open URLs
///   - Visual underline/highlight for detected URLs
///   - Support for http, https, ftp, file, mailto schemes
use alloc::string::String;
use alloc::vec::Vec;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// URL DETECTION
// ═══════════════════════════════════════════════════════════════════════

/// A detected URL in terminal text
#[derive(Debug, Clone)]
pub struct DetectedUrl {
    /// Start column (character index)
    pub start_col: usize,
    /// End column (exclusive)
    pub end_col: usize,
    /// Row in terminal buffer
    pub row: usize,
    /// The URL text
    pub url: String,
    /// URL scheme
    pub scheme: UrlScheme,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlScheme {
    Http,
    Https,
    Ftp,
    File,
    Mailto,
    Unknown,
}

impl UrlScheme {
    pub fn from_str(s: &str) -> Self {
        match s {
            "http" => UrlScheme::Http,
            "https" => UrlScheme::Https,
            "ftp" => UrlScheme::Ftp,
            "file" => UrlScheme::File,
            "mailto" => UrlScheme::Mailto,
            _ => UrlScheme::Unknown,
        }
    }

    pub fn prefix(&self) -> &'static str {
        match self {
            UrlScheme::Http => "http://",
            UrlScheme::Https => "https://",
            UrlScheme::Ftp => "ftp://",
            UrlScheme::File => "file://",
            UrlScheme::Mailto => "mailto:",
            UrlScheme::Unknown => "",
        }
    }
}

/// URL prefixes to search for
const URL_PREFIXES: &[(&str, UrlScheme)] = &[
    ("https://", UrlScheme::Https),
    ("http://", UrlScheme::Http),
    ("ftp://", UrlScheme::Ftp),
    ("file://", UrlScheme::File),
    ("mailto:", UrlScheme::Mailto),
];

/// Characters that end a URL
fn is_url_terminator(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t' | '\n' | '\r' | '"' | '\'' | '>' | '<' | '|' | '{' | '}'
    )
}

/// Characters that shouldn't end a URL if they're the last character
fn is_trailing_punctuation(c: char) -> bool {
    matches!(c, '.' | ',' | ';' | ':' | ')' | ']' | '!' | '?')
}

/// Detect URLs in a line of text
pub fn detect_urls_in_line(line: &str, row: usize) -> Vec<DetectedUrl> {
    let mut urls = Vec::new();

    for &(prefix, scheme) in URL_PREFIXES {
        let mut search_from = 0;
        while let Some(start) = line[search_from..].find(prefix) {
            let abs_start = search_from + start;
            let url_start = abs_start;

            // Find the end of the URL
            let mut end = url_start + prefix.len();
            let chars: Vec<char> = line.chars().collect();

            while end < chars.len() {
                if is_url_terminator(chars[end]) {
                    break;
                }
                end += 1;
            }

            // Strip trailing punctuation
            while end > url_start + prefix.len() {
                if is_trailing_punctuation(chars[end - 1]) {
                    end -= 1;
                } else {
                    break;
                }
            }

            // Handle matching parentheses (common in markdown/wiki URLs)
            let url_text: String = chars[url_start..end].iter().collect();
            let open_parens = url_text.chars().filter(|&c| c == '(').count();
            let close_parens = url_text.chars().filter(|&c| c == ')').count();
            if close_parens > open_parens && url_text.ends_with(')') {
                // Remove trailing ')' if unmatched
                let url_text_trimmed: String = chars[url_start..end - 1].iter().collect();
                urls.push(DetectedUrl {
                    start_col: url_start,
                    end_col: end - 1,
                    row,
                    url: url_text_trimmed,
                    scheme,
                });
            } else if end > url_start + prefix.len() {
                urls.push(DetectedUrl {
                    start_col: url_start,
                    end_col: end,
                    row,
                    url: url_text,
                    scheme,
                });
            }

            search_from = end;
        }
    }

    urls
}

/// Detect a URL at a specific column position in a line
pub fn url_at_position(line: &str, row: usize, col: usize) -> Option<DetectedUrl> {
    let urls = detect_urls_in_line(line, row);
    urls.into_iter()
        .find(|u| col >= u.start_col && col < u.end_col)
}

/// Handle opening a URL (called on Ctrl+Click)
pub fn open_url(url: &str) {
    serial_println!("[Terminal] Opening URL: {}", url);

    // Determine scheme and dispatch accordingly
    let scheme = if url.starts_with("https://") || url.starts_with("http://") {
        "web"
    } else if url.starts_with("file://") {
        "file"
    } else if url.starts_with("mailto:") {
        "mailto"
    } else {
        "unknown"
    };

    match scheme {
        "web" => {
            // Open in browser window
            crate::gui::window::open_browser_window(url);
            serial_println!("[Terminal] Opened URL in browser: {}", url);
        }
        "file" => {
            // Open file path in file explorer
            let path = &url[7..]; // strip "file://"
            crate::gui::window::open_file_explorer_at(path);
            serial_println!("[Terminal] Opened path in explorer: {}", path);
        }
        _ => {
            serial_println!("[Terminal] Unsupported URL scheme: {}", url);
        }
    }
}

/// Check if a character position is within a URL
pub fn is_url_position(line: &str, row: usize, col: usize) -> bool {
    url_at_position(line, row, col).is_some()
}

/// Initialize clickable URLs
pub fn init() {
    serial_println!("[KnoxOS] Terminal clickable URLs initialized");
}
