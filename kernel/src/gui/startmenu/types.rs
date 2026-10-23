//! Command-center data types: tabs, apps, chat, tasks, and voice.

use alloc::string::String;
use alloc::vec::Vec;

use crate::gui::desktop::IconType;
use crate::gui::framebuffer::Pixel;

/// Which tab/mode the command center is in
#[derive(Clone, Copy, PartialEq)]
pub enum CommandCenterTab {
    /// App search & launch (default)
    Apps,
    /// AI chat & assistant
    AI,
    /// Running tasks & agent workflows
    Tasks,
}

/// App category for filtering
#[derive(Clone, Copy, PartialEq)]
pub enum MenuCategory {
    Pinned,
    App,
    Emulator,
    Game,
    System,
    AI,
    Media,
    Development,
}

/// A start menu application entry
#[derive(Clone)]
pub struct StartMenuItem {
    pub name: String,
    pub description: String,
    pub category: MenuCategory,
    pub icon_type: IconType,
    pub keywords: String,
    pub pinned: bool,
    pub launch_count: u32,
    pub last_launched: u64,
}

/// AI chat message
#[derive(Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    pub timestamp: u64,
    pub tool_calls: Vec<ToolCallResult>,
    pub is_streaming: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ChatRole {
    User,
    Assistant,
    System,
    ToolResult,
}

/// Tool call result displayed in chat
#[derive(Clone)]
pub struct ToolCallResult {
    pub tool_name: String,
    pub description: String,
    pub status: ToolCallStatus,
    pub output: String,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ToolCallStatus {
    Running,
    Success,
    Error,
}

/// Quick action toggle
#[derive(Clone)]
pub struct QuickAction {
    pub name: String,
    pub icon_glyph: char,
    pub enabled: bool,
    pub color: Pixel,
}

/// Background AI agent task
#[derive(Clone)]
pub struct AgentTask {
    pub id: u32,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub progress: u8, // 0-100
    pub steps_total: u8,
    pub steps_done: u8,
}

#[derive(Clone, Copy, PartialEq)]
pub enum TaskStatus {
    Queued,
    Running,
    Complete,
    Failed,
    Paused,
}

/// Voice input state
#[derive(Clone, Copy, PartialEq)]
pub enum VoiceState {
    Idle,
    Listening,
    Processing,
}
