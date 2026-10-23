//! Aurora Command Center — SI-native App Launcher & Command Hub
//!
//! This is the primary interaction surface for KnoxOS. It replaces the traditional
//! "Start Menu" with a fully-featured command center that combines:
//!
//! • **App Search & Launch** — Fuzzy search across all installed apps, pinned favorites,
//!   categorized browsing, and recent apps with keyboard-driven navigation
//! • **AI Chat & Assistant** — Inline conversational AI with streaming responses,
//!   context-aware suggestions, and natural language computer/app control
//! • **Tool Calling** — The AI can invoke system tools (open apps, run commands,
//!   manage files, control settings) directly from the chat interface
//! • **Voice Input** — Push-to-talk microphone input for hands-free AI interaction
//! • **Quick Actions** — One-tap system controls (Wi-Fi, Bluetooth, DND, brightness)
//! • **Tasks & Agent** — Background AI agent tasks with status tracking, chained
//!   tool-use pipelines, and autonomous multi-step workflows
//! • **Recent Files & Activities** — Quick access to recently opened files and actions
//!
//! Design: Warm frosted-glass "Aurora" aesthetic with soft borders, smooth animations,
//! and a cyberpunk-inspired color palette matching the KnoxOS desktop theme.

mod ai;
mod apps;
mod chat;
mod draw;
mod input;
mod layout;
mod search;
mod state;
mod tasks;
mod types;

pub use draw::draw_start_menu;
pub use input::{close, handle_click, handle_key, handle_scroll, is_visible, toggle, update_hover};
pub use layout::{
    MENU_ITEM_HEIGHT, PANEL_MAX_HEIGHT, PANEL_WIDTH, SIDEBAR_WIDTH, START_MENU_MAX_HEIGHT,
    START_MENU_WIDTH,
};
pub use state::{START_MENU, StartMenu};
pub use types::{
    AgentTask, ChatMessage, ChatRole, CommandCenterTab, MenuCategory, QuickAction, StartMenuItem,
    TaskStatus, ToolCallResult, ToolCallStatus, VoiceState,
};
