// SPDX-License-Identifier: MIT
//! F1 help system and interactive tutorial (items 21.7, 21.8)
//!
//! Provides context-sensitive help (F1 key) and a first-run tutorial.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

/// Help topic categories
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpCategory {
    General,
    Desktop,
    FileExplorer,
    Terminal,
    TextEditor,
    Browser,
    Settings,
    Shortcuts,
    Shell,
}

/// A help article
#[derive(Debug, Clone)]
pub struct HelpArticle {
    pub title: String,
    pub category: HelpCategory,
    pub content: String,
    pub keywords: Vec<String>,
}

/// Tutorial step
#[derive(Debug, Clone)]
pub struct TutorialStep {
    pub title: String,
    pub description: String,
    pub action_hint: String,
    /// Screen coordinates for highlight (x, y, w, h)
    pub highlight_rect: Option<(i32, i32, u32, u32)>,
    /// Whether this step requires user action to proceed
    pub wait_for_action: bool,
}

lazy_static::lazy_static! {
    static ref HELP_ARTICLES: Mutex<Vec<HelpArticle>> = Mutex::new(Vec::new());
    static ref TUTORIAL_STEPS: Mutex<Vec<TutorialStep>> = Mutex::new(Vec::new());
}

static HELP_SHOWN: AtomicBool = AtomicBool::new(false);
static TUTORIAL_COMPLETE: AtomicBool = AtomicBool::new(false);
static TUTORIAL_STEP: AtomicU64 = AtomicU64::new(0);
static HELP_QUERIES: AtomicU64 = AtomicU64::new(0);

/// Register the built-in help articles
fn register_builtin_help() {
    let articles = [
        HelpArticle {
            title: String::from("Getting Started with KnoxOS"),
            category: HelpCategory::General,
            content: String::from(
                "Welcome to KnoxOS! This is a desktop operating system with AI-native features.\n\n\
                 • Double-click desktop icons to open applications\n\
                 • Right-click the desktop for a context menu\n\
                 • Click the dock at the bottom to switch applications\n\
                 • Press Ctrl+Alt+T to open a terminal\n\
                 • Press Super+L to lock the screen",
            ),
            keywords: vec![
                String::from("start"),
                String::from("begin"),
                String::from("new"),
            ],
        },
        HelpArticle {
            title: String::from("Keyboard Shortcuts"),
            category: HelpCategory::Shortcuts,
            content: String::from(
                "System Shortcuts:\n\
                 • Alt+F4 — Close window\n\
                 • Alt+Tab — Switch windows\n\
                 • Ctrl+Alt+T — Open terminal\n\
                 • Ctrl+Alt+D — Show desktop\n\
                 • Super+L — Lock screen\n\
                 • Super+1-4 — Switch workspace\n\
                 • Super+Shift+1-4 — Move window to workspace\n\n\
                 Window Management:\n\
                 • Ctrl+Shift+← — Snap window left\n\
                 • Ctrl+Shift+→ — Snap window right\n\
                 • Ctrl+Shift+↑ — Maximize window\n\n\
                 Terminal:\n\
                 • Ctrl+C — Interrupt\n\
                 • Ctrl+D — EOF\n\
                 • Ctrl+R — Reverse search\n\
                 • Ctrl+T — New tab\n\
                 • Ctrl+Shift+C/V — Copy/Paste",
            ),
            keywords: vec![
                String::from("shortcut"),
                String::from("keys"),
                String::from("hotkey"),
            ],
        },
        HelpArticle {
            title: String::from("File Explorer"),
            category: HelpCategory::FileExplorer,
            content: String::from(
                "The File Explorer lets you browse and manage files.\n\n\
                 • Click to select a file\n\
                 • Double-click to open a file or enter a directory\n\
                 • Right-click for context menu (Copy, Cut, Paste, Delete, Rename)\n\
                 • Ctrl+C/X/V — Copy/Cut/Paste\n\
                 • F2 — Rename\n\
                 • Delete — Delete file\n\
                 • Backspace — Go up one directory\n\
                 • Click column headers to sort",
            ),
            keywords: vec![
                String::from("files"),
                String::from("explorer"),
                String::from("browse"),
            ],
        },
        HelpArticle {
            title: String::from("Terminal & Shell"),
            category: HelpCategory::Terminal,
            content: String::from(
                "The KnoxOS terminal supports:\n\n\
                 • Fish-style syntax highlighting and autosuggestions\n\
                 • Tab completion for commands, paths, and variables\n\
                 • Pipe chains: cmd1 | cmd2 | cmd3\n\
                 • I/O redirects: >, >>, <, 2>\n\
                 • Background jobs: command &\n\
                 • Shell scripting: if/for/while/case\n\
                 • 50+ built-in commands\n\n\
                 Type 'help' for a list of commands.",
            ),
            keywords: vec![
                String::from("terminal"),
                String::from("shell"),
                String::from("command"),
            ],
        },
        HelpArticle {
            title: String::from("Text Editor"),
            category: HelpCategory::TextEditor,
            content: String::from(
                "The built-in text editor supports:\n\n\
                 • Syntax highlighting for Rust, C, Python, JS, Shell\n\
                 • Line numbers and active line highlight\n\
                 • Ctrl+S — Save file\n\
                 • Ctrl+Z — Undo\n\
                 • Ctrl+Shift+Z — Redo\n\
                 • Ctrl+F — Find\n\
                 • Ctrl+H — Find and Replace",
            ),
            keywords: vec![
                String::from("editor"),
                String::from("edit"),
                String::from("text"),
            ],
        },
        HelpArticle {
            title: String::from("Settings"),
            category: HelpCategory::Settings,
            content: String::from(
                "Open Settings from the app launcher or right-click desktop.\n\n\
                 • Display — Resolution, scaling\n\
                 • Sound — Volume controls\n\
                 • Network — IP configuration\n\
                 • Personalization — Theme, wallpaper\n\
                 • System — Hostname, timezone, keyboard layout\n\
                 • Users — Account management\n\
                 • About — System information",
            ),
            keywords: vec![
                String::from("settings"),
                String::from("config"),
                String::from("preferences"),
            ],
        },
    ];

    let mut help = HELP_ARTICLES.lock();
    for article in articles {
        help.push(article);
    }
}

/// Register the getting-started tutorial steps
fn register_tutorial() {
    let steps = [
        TutorialStep {
            title: String::from("Welcome to KnoxOS!"),
            description: String::from(
                "This tutorial will guide you through the basics of your new operating system.",
            ),
            action_hint: String::from("Click 'Next' to continue"),
            highlight_rect: None,
            wait_for_action: true,
        },
        TutorialStep {
            title: String::from("The Desktop"),
            description: String::from(
                "This is your desktop. You can double-click icons to open applications, and right-click for more options.",
            ),
            action_hint: String::from("Try double-clicking the 'Files' icon"),
            highlight_rect: Some((100, 100, 200, 200)),
            wait_for_action: true,
        },
        TutorialStep {
            title: String::from("The Dock"),
            description: String::from(
                "The dock at the bottom shows running apps. Click an icon to switch to that app.",
            ),
            action_hint: String::from("Click 'Next' to continue"),
            highlight_rect: Some((0, 1040, 1920, 40)),
            wait_for_action: true,
        },
        TutorialStep {
            title: String::from("Opening the Terminal"),
            description: String::from(
                "Press Ctrl+Alt+T to open a terminal. The terminal is your command-line interface.",
            ),
            action_hint: String::from("Press Ctrl+Alt+T now"),
            highlight_rect: None,
            wait_for_action: true,
        },
        TutorialStep {
            title: String::from("App Launcher"),
            description: String::from(
                "Click the grid icon on the dock to open the Command Center. You can search for and launch any application.",
            ),
            action_hint: String::from("Click the grid icon"),
            highlight_rect: None,
            wait_for_action: true,
        },
        TutorialStep {
            title: String::from("You're Ready!"),
            description: String::from(
                "You now know the basics! Press F1 at any time for help. Explore and enjoy KnoxOS!",
            ),
            action_hint: String::from("Click 'Finish' to close"),
            highlight_rect: None,
            wait_for_action: true,
        },
    ];

    let mut tutorial = TUTORIAL_STEPS.lock();
    for step in steps {
        tutorial.push(step);
    }
}

/// Show help for a specific category
pub fn show_help(category: HelpCategory) -> Vec<HelpArticle> {
    HELP_QUERIES.fetch_add(1, Ordering::Relaxed);
    HELP_SHOWN.store(true, Ordering::Relaxed);
    HELP_ARTICLES
        .lock()
        .iter()
        .filter(|a| a.category == category)
        .cloned()
        .collect()
}

/// Search help articles
pub fn search_help(query: &str) -> Vec<HelpArticle> {
    HELP_QUERIES.fetch_add(1, Ordering::Relaxed);
    let q = query.to_lowercase();
    HELP_ARTICLES
        .lock()
        .iter()
        .filter(|a| {
            a.title.to_lowercase().contains(&q)
                || a.content.to_lowercase().contains(&q)
                || a.keywords.iter().any(|k| k.to_lowercase().contains(&q))
        })
        .cloned()
        .collect()
}

/// Handle F1 key press — show context-sensitive help
pub fn handle_f1() {
    // Determine current context (active window type)
    // Show relevant help articles
    let articles = show_help(HelpCategory::General);
    if !articles.is_empty() {
        crate::serial_println!("[help] showing: {}", articles[0].title);
    }
}

/// Start the getting-started tutorial
pub fn start_tutorial() {
    TUTORIAL_STEP.store(0, Ordering::Relaxed);
    TUTORIAL_COMPLETE.store(false, Ordering::Relaxed);
    crate::serial_println!("[help] tutorial started");
}

/// Get current tutorial step
pub fn current_tutorial_step() -> Option<TutorialStep> {
    let step = TUTORIAL_STEP.load(Ordering::Relaxed) as usize;
    TUTORIAL_STEPS.lock().get(step).cloned()
}

/// Advance to next tutorial step
pub fn next_tutorial_step() -> bool {
    let step = TUTORIAL_STEP.fetch_add(1, Ordering::Relaxed) as usize + 1;
    let total = TUTORIAL_STEPS.lock().len();
    if step >= total {
        TUTORIAL_COMPLETE.store(true, Ordering::Relaxed);
        crate::serial_println!("[help] tutorial complete!");
        false
    } else {
        true
    }
}

/// Check if tutorial has been completed
pub fn is_tutorial_complete() -> bool {
    TUTORIAL_COMPLETE.load(Ordering::Relaxed)
}

pub fn stats() -> u64 {
    HELP_QUERIES.load(Ordering::Relaxed)
}

/// Initialize the help system
pub fn init() {
    register_builtin_help();
    register_tutorial();
    crate::serial_println!(
        "[help] initialized with {} articles, {} tutorial steps",
        HELP_ARTICLES.lock().len(),
        TUTORIAL_STEPS.lock().len()
    );
}
