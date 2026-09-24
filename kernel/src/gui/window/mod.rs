/// Window Manager — "Aurora" Design
/// Frosted glass windows with holographic accent edges, pill controls on the left,
/// ambient glow on focused windows, and large corner radius for a futuristic feel.
/// Nothing like Windows, macOS, or Linux — designed for an AI-native experience.
mod apps;
mod browser_content;
mod content;
mod decorations;
mod explorer_content;
mod geometry;
mod grouping;
mod layouts;
mod manager;
mod rules;
mod terminal;
mod types;

pub use decorations::*;
pub use grouping::*;
pub use layouts::*;
pub use manager::*;
pub use rules::{WINDOW_RULES, WindowRule, add_window_rule, clear_window_rules};
pub use types::*;

pub(crate) use rules::apply_window_rules;
