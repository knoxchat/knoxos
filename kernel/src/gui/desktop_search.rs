/// Desktop Search
///
/// Full-text indexed search across files, applications, contacts, and settings.
/// Provides instant results as the user types.
use alloc::string::String;
use alloc::vec::Vec;

/// Search result categories
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SearchCategory {
    Application,
    File,
    Setting,
    Contact,
    RecentDocument,
    WebBookmark,
    Calculator,
}

/// A single search result
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub category: SearchCategory,
    pub title: String,
    pub subtitle: String,
    pub icon: String,
    pub relevance: f32,
    pub action_path: String,
}

/// Search index entry
struct IndexEntry {
    category: SearchCategory,
    title: String,
    keywords: Vec<String>,
    path: String,
    icon: String,
}

/// Desktop search engine
pub struct DesktopSearch {
    index: Vec<IndexEntry>,
}

impl DesktopSearch {
    pub fn new() -> Self {
        Self { index: Vec::new() }
    }

    /// Add an item to the search index
    pub fn add_entry(
        &mut self,
        category: SearchCategory,
        title: &str,
        keywords: &[&str],
        path: &str,
        icon: &str,
    ) {
        self.index.push(IndexEntry {
            category,
            title: String::from(title),
            keywords: keywords.iter().map(|k| String::from(*k)).collect(),
            path: String::from(path),
            icon: String::from(icon),
        });
    }

    /// Search with a query string
    pub fn search(&self, query: &str) -> Vec<SearchResult> {
        if query.is_empty() {
            return Vec::new();
        }
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        // Check if it's a math expression
        if query
            .chars()
            .all(|c| c.is_ascii_digit() || "+-*/().% ".contains(c))
        {
            // Attempt calculator evaluation
            results.push(SearchResult {
                category: SearchCategory::Calculator,
                title: String::from(query),
                subtitle: String::from("Calculator"),
                icon: String::from("calculator"),
                relevance: 1.0,
                action_path: String::new(),
            });
        }

        for entry in &self.index {
            let mut score = 0.0f32;
            let title_lower = entry.title.to_lowercase();

            if title_lower == query_lower {
                score = 1.0;
            } else if title_lower.starts_with(&query_lower) {
                score = 0.9;
            } else if title_lower.contains(&query_lower) {
                score = 0.7;
            } else {
                for kw in &entry.keywords {
                    if kw.to_lowercase().contains(&query_lower) {
                        score = 0.5;
                        break;
                    }
                }
            }

            if score > 0.0 {
                // Boost apps
                if entry.category == SearchCategory::Application {
                    score += 0.1;
                }
                results.push(SearchResult {
                    category: entry.category,
                    title: entry.title.clone(),
                    subtitle: entry.path.clone(),
                    icon: entry.icon.clone(),
                    relevance: score,
                    action_path: entry.path.clone(),
                });
            }
        }

        results.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(core::cmp::Ordering::Equal)
        });
        results.truncate(20);
        results
    }

    /// Rebuild index (scan applications, files, settings)
    pub fn rebuild_index(&mut self) {
        // Scan /usr/share/applications for .desktop files
        // Scan user documents
        // Index system settings
    }
}

pub fn init() {
    crate::serial_println!("[SEARCH] Desktop search engine loaded");
}
