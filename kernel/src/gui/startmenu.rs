/// Aurora Command Center — AI-Native App Launcher & Command Hub
///
/// This is the primary interaction surface for KnoxOS. It replaces the traditional
/// "Start Menu" with a fully-featured command center that combines:
///
/// • **App Search & Launch** — Fuzzy search across all installed apps, pinned favorites,
///   categorized browsing, and recent apps with keyboard-driven navigation
/// • **AI Chat & Assistant** — Inline conversational AI with streaming responses,
///   context-aware suggestions, and natural language computer/app control
/// • **Tool Calling** — The AI can invoke system tools (open apps, run commands,
///   manage files, control settings) directly from the chat interface
/// • **Voice Input** — Push-to-talk microphone input for hands-free AI interaction
/// • **Quick Actions** — One-tap system controls (Wi-Fi, Bluetooth, DND, brightness)
/// • **Tasks & Agent** — Background AI agent tasks with status tracking, chained
///   tool-use pipelines, and autonomous multi-step workflows
/// • **Recent Files & Activities** — Quick access to recently opened files and actions
///
/// Design: Warm frosted-glass "Aurora" aesthetic with soft borders, smooth animations,
/// and a cyberpunk-inspired color palette matching the KnoxOS desktop theme.
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write;
use spin::Mutex;

use super::colors;
use super::font_engine;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::icon_theme;
use super::icon_theme::IconCategory;
use super::icons;

const ARCH_NAME: &str = if cfg!(target_arch = "x86_64") {
    "x86_64"
} else if cfg!(target_arch = "aarch64") {
    "aarch64"
} else {
    "riscv64"
};

// ═══════════════════════════════════════════════════════════════════════════
// LAYOUT CONSTANTS — Aurora Command Center Geometry
// ═══════════════════════════════════════════════════════════════════════════

/// Total panel width
pub const PANEL_WIDTH: u32 = 480;
/// Maximum panel height
pub const PANEL_MAX_HEIGHT: u32 = 660;
/// Corner radius for main panel
const PANEL_RADIUS: u32 = 18;
/// Horizontal padding inside panel
const PAD_H: i32 = 20;
/// Gap between panel bottom and dock top
const DOCK_GAP: i32 = 12;

/// Search bar height
const SEARCH_BAR_H: u32 = 40;
/// Search bar corner radius
const SEARCH_BAR_RADIUS: u32 = 14;

/// Tab bar height
const TAB_BAR_H: u32 = 32;

/// Menu item row height
pub const MENU_ITEM_HEIGHT: u32 = 44;
/// Menu item horizontal inset
const ITEM_INSET: i32 = 10;
/// Icon size in app list
const ITEM_ICON_SIZE: i32 = 24;
/// Gap between icon and text
const ICON_TEXT_GAP: i32 = 14;

/// Bottom bar height (user + power)
const BOTTOM_BAR_H: u32 = 48;

/// AI chat input bar height
const AI_INPUT_H: u32 = 44;
/// AI message bubble max width ratio
const AI_BUBBLE_MAX_W_PCT: u32 = 85; // % of panel content width

/// Quick action button size
const QUICK_ACTION_SIZE: u32 = 56;
/// Quick action grid columns
const QUICK_ACTION_COLS: u32 = 4;
/// Quick action grid gap
const QUICK_ACTION_GAP: u32 = 12;

/// Agent task item height
const TASK_ITEM_H: u32 = 52;

// ═══════════════════════════════════════════════════════════════════════════
// LEGACY ALIASES (used by other modules that reference old constants)
// ═══════════════════════════════════════════════════════════════════════════

/// Start menu width (legacy alias)
pub const START_MENU_WIDTH: u32 = PANEL_WIDTH;
/// Start menu max height (legacy alias)
pub const START_MENU_MAX_HEIGHT: u32 = PANEL_MAX_HEIGHT;
/// Sidebar width (legacy, unused)
pub const SIDEBAR_WIDTH: u32 = 52;

// ═══════════════════════════════════════════════════════════════════════════
// DATA STRUCTURES
// ═══════════════════════════════════════════════════════════════════════════

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
    pub icon_type: super::desktop::IconType,
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

/// The main command center state
pub struct StartMenu {
    // ── Visibility & Navigation ──
    pub visible: bool,
    pub active_tab: CommandCenterTab,

    // ── Power Menu ──
    pub power_menu_visible: bool,

    // ── Apps Tab ──
    pub items: Vec<StartMenuItem>,
    pub hovered_index: Option<usize>,
    pub scroll_offset: usize,
    pub search_text: String,
    pub search_cursor: usize,
    pub search_active: bool,
    pub filtered_indices: Vec<usize>,

    // ── AI Tab ──
    pub chat_messages: Vec<ChatMessage>,
    pub chat_input: String,
    pub chat_cursor: usize,
    pub chat_scroll_y: i32,
    pub ai_is_thinking: bool,
    pub voice_state: VoiceState,
    pub quick_actions: Vec<QuickAction>,
    pub suggested_prompts: Vec<String>,

    // ── Tasks Tab ──
    pub agent_tasks: Vec<AgentTask>,
    pub tasks_scroll_y: i32,

    // ── Animation state ──
    pub animation_progress: u8, // 0-255 for open/close animation
    pub hover_glow_phase: u16,  // For animated glow effects
}

lazy_static::lazy_static! {
    pub static ref START_MENU: Mutex<StartMenu> = Mutex::new(StartMenu::new());
}

impl Default for StartMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl StartMenu {
    pub fn new() -> Self {
        let items = vec![
            // ── Pinned Apps ──
            StartMenuItem {
                name: String::from("Terminal"),
                description: String::from("System terminal emulator"),
                category: MenuCategory::Pinned,
                icon_type: super::desktop::IconType::Terminal,
                keywords: String::from("shell bash zsh command prompt cli console"),
                pinned: true,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Files"),
                description: String::from("Browse and manage files"),
                category: MenuCategory::Pinned,
                icon_type: super::desktop::IconType::Folder,
                keywords: String::from("file manager explorer nautilus finder folder directory"),
                pinned: true,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Browser"),
                description: String::from("Web browser"),
                category: MenuCategory::Pinned,
                icon_type: super::desktop::IconType::Globe,
                keywords: String::from("web internet chrome firefox vivaldi http surf"),
                pinned: true,
                launch_count: 0,
                last_launched: 0,
            },
            // ── Apps ──
            StartMenuItem {
                name: String::from("Monaco Editor"),
                description: String::from("Code editor with syntax highlighting"),
                category: MenuCategory::Development,
                icon_type: super::desktop::IconType::Document,
                keywords: String::from("code text editor vscode ide programming"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Paint"),
                description: String::from("Image editor and drawing tool"),
                category: MenuCategory::App,
                icon_type: super::desktop::IconType::Document,
                keywords: String::from("draw paint image art canvas brush pixel"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Video Player"),
                description: String::from("Media playback"),
                category: MenuCategory::Media,
                icon_type: super::desktop::IconType::MediaPlayer,
                keywords: String::from("video movie media player vlc mpv stream"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("AI Assistant"),
                description: String::from("Conversational AI with tool use"),
                category: MenuCategory::AI,
                icon_type: super::desktop::IconType::AIBrain,
                keywords: String::from("ai assistant chat llm gpt claude copilot neural"),
                pinned: true,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Vim"),
                description: String::from("Modal text editor"),
                category: MenuCategory::Development,
                icon_type: super::desktop::IconType::Terminal,
                keywords: String::from("vim vi editor modal text nvim neovim"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Webamp"),
                description: String::from("Winamp-style music player"),
                category: MenuCategory::Media,
                icon_type: super::desktop::IconType::MediaPlayer,
                keywords: String::from("music audio player winamp mp3 playlist"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Doom"),
                description: String::from("Classic FPS game"),
                category: MenuCategory::Game,
                icon_type: super::desktop::IconType::Game,
                keywords: String::from("doom fps shooter game classic retro id"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("ClassiCube"),
                description: String::from("Voxel sandbox game"),
                category: MenuCategory::Game,
                icon_type: super::desktop::IconType::Game,
                keywords: String::from("minecraft voxel sandbox cube craft build"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Quake III"),
                description: String::from("Arena FPS game"),
                category: MenuCategory::Game,
                icon_type: super::desktop::IconType::Game,
                keywords: String::from("quake arena fps shooter multiplayer"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Settings"),
                description: String::from("System configuration"),
                category: MenuCategory::System,
                icon_type: super::desktop::IconType::Settings,
                keywords: String::from("settings preferences config system display sound network"),
                pinned: true,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Task Manager"),
                description: String::from("Monitor processes and system resources"),
                category: MenuCategory::System,
                icon_type: super::desktop::IconType::Settings,
                keywords: String::from(
                    "task manager process monitor system resources cpu memory kill",
                ),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Calculator"),
                description: String::from("Basic and scientific calculator"),
                category: MenuCategory::App,
                icon_type: super::desktop::IconType::Document,
                keywords: String::from("calculator calc math arithmetic compute add subtract"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Image Viewer"),
                description: String::from("View images and photos"),
                category: MenuCategory::Media,
                icon_type: super::desktop::IconType::Document,
                keywords: String::from("image viewer photo picture gallery png jpg bmp"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            // ── Utility Apps ──
            StartMenuItem {
                name: String::from("Log Viewer"),
                description: String::from("View kernel logs and system messages"),
                category: MenuCategory::System,
                icon_type: super::desktop::IconType::Document,
                keywords: String::from("log viewer dmesg kernel messages syslog journal"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Calendar"),
                description: String::from("Calendar and date management"),
                category: MenuCategory::App,
                icon_type: super::desktop::IconType::Document,
                keywords: String::from("calendar date schedule event planner time"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Disk Utility"),
                description: String::from("Manage disks and partitions"),
                category: MenuCategory::System,
                icon_type: super::desktop::IconType::MyPC,
                keywords: String::from("disk utility partition format mount storage drive"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Bluetooth"),
                description: String::from("Bluetooth device manager"),
                category: MenuCategory::System,
                icon_type: super::desktop::IconType::Settings,
                keywords: String::from("bluetooth wireless pair device audio headset speaker"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Software Updater"),
                description: String::from("Check for and install system updates"),
                category: MenuCategory::System,
                icon_type: super::desktop::IconType::Settings,
                keywords: String::from("software updater update upgrade patch security"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Software Center"),
                description: String::from("Browse and install applications"),
                category: MenuCategory::App,
                icon_type: super::desktop::IconType::Settings,
                keywords: String::from("software center app store install package download"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Archive Manager"),
                description: String::from("Open and extract archive files"),
                category: MenuCategory::App,
                icon_type: super::desktop::IconType::Document,
                keywords: String::from("archive manager zip tar gz extract compress"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
        ];

        let quick_actions = vec![
            QuickAction {
                name: String::from("Wi-Fi"),
                icon_glyph: 'W',
                enabled: true,
                color: Pixel::new(140, 180, 200, 255),
            },
            QuickAction {
                name: String::from("Bluetooth"),
                icon_glyph: 'B',
                enabled: false,
                color: Pixel::new(140, 170, 210, 255),
            },
            QuickAction {
                name: String::from("DND"),
                icon_glyph: 'D',
                enabled: false,
                color: Pixel::new(255, 120, 80, 255),
            },
            QuickAction {
                name: String::from("Night"),
                icon_glyph: 'N',
                enabled: false,
                color: Pixel::new(255, 200, 80, 255),
            },
        ];

        let suggested_prompts = vec![
            String::from("Open a terminal and show system info"),
            String::from("What files are in my Documents?"),
            String::from("Take a screenshot"),
            String::from("Set display to dark mode"),
        ];

        let chat_messages = vec![ChatMessage {
            role: ChatRole::Assistant,
            content: String::from(
                "Hello! I'm the KnoxOS AI. I can open apps, manage files, \
                 run commands, and answer questions. Try asking me anything \
                 or use the suggested prompts below.",
            ),
            timestamp: 0,
            tool_calls: Vec::new(),
            is_streaming: false,
        }];

        let agent_tasks = vec![
            AgentTask {
                id: 1,
                title: String::from("System Health Monitor"),
                description: String::from("Monitoring CPU, memory, and disk status"),
                status: TaskStatus::Running,
                progress: 100,
                steps_total: 1,
                steps_done: 1,
            },
            AgentTask {
                id: 2,
                title: String::from("Security Scan"),
                description: String::from("Scanning system for vulnerabilities"),
                status: TaskStatus::Complete,
                progress: 100,
                steps_total: 4,
                steps_done: 4,
            },
        ];

        // Pre-build filtered indices (all items initially)
        let filtered_indices: Vec<usize> = (0..items.len()).collect();

        Self {
            visible: false,
            active_tab: CommandCenterTab::Apps,
            power_menu_visible: false,
            items,
            hovered_index: None,
            scroll_offset: 0,
            search_text: String::new(),
            search_cursor: 0,
            search_active: false,
            filtered_indices,
            chat_messages,
            chat_input: String::new(),
            chat_cursor: 0,
            chat_scroll_y: 0,
            ai_is_thinking: false,
            voice_state: VoiceState::Idle,
            quick_actions,
            suggested_prompts,
            agent_tasks,
            tasks_scroll_y: 0,
            animation_progress: 0,
            hover_glow_phase: 0,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.active_tab = CommandCenterTab::Apps;
            self.search_text.clear();
            self.search_cursor = 0;
            self.search_active = true;
            self.hovered_index = None;
            self.scroll_offset = 0;
            self.rebuild_filtered();
            self.animation_progress = 255;
        } else {
            self.animation_progress = 0;
        }
    }

    /// Rebuild filtered indices based on current search text
    pub fn rebuild_filtered(&mut self) {
        self.filtered_indices.clear();
        if self.search_text.is_empty() {
            let mut pinned: Vec<usize> = Vec::new();
            let mut rest: Vec<usize> = Vec::new();
            for (i, item) in self.items.iter().enumerate() {
                if item.pinned {
                    pinned.push(i);
                } else {
                    rest.push(i);
                }
            }
            self.filtered_indices.extend_from_slice(&pinned);
            self.filtered_indices.extend_from_slice(&rest);
        } else {
            let query = self.search_text.as_str();
            for (i, item) in self.items.iter().enumerate() {
                if fuzzy_match(query, &item.name)
                    || fuzzy_match(query, &item.keywords)
                    || fuzzy_match(query, &item.description)
                {
                    self.filtered_indices.push(i);
                }
            }
        }
        self.scroll_offset = 0;
        self.hovered_index = None;
    }

    /// Handle a character typed into the search bar
    pub fn type_char(&mut self, ch: char) {
        if self.active_tab == CommandCenterTab::Apps && self.search_active {
            self.search_text.push(ch);
            self.search_cursor = self.search_text.len();
            self.rebuild_filtered();
        } else if self.active_tab == CommandCenterTab::AI {
            self.chat_input.push(ch);
            self.chat_cursor = self.chat_input.len();
        }
    }

    /// Handle backspace in search or chat
    pub fn backspace(&mut self) {
        if self.active_tab == CommandCenterTab::Apps && self.search_active {
            self.search_text.pop();
            self.search_cursor = self.search_text.len();
            self.rebuild_filtered();
        } else if self.active_tab == CommandCenterTab::AI {
            self.chat_input.pop();
            self.chat_cursor = self.chat_input.len();
        }
    }

    /// Submit the current AI chat input
    pub fn submit_chat(&mut self) {
        if self.chat_input.is_empty() {
            return;
        }
        let user_msg = ChatMessage {
            role: ChatRole::User,
            content: self.chat_input.clone(),
            timestamp: 0,
            tool_calls: Vec::new(),
            is_streaming: false,
        };
        self.chat_messages.push(user_msg);

        let response = generate_ai_response(&self.chat_input);
        self.chat_messages.push(response);

        self.chat_input.clear();
        self.chat_cursor = 0;
        self.chat_scroll_y = 0;
    }

    /// Switch active tab
    pub fn set_tab(&mut self, tab: CommandCenterTab) {
        self.active_tab = tab;
        self.hovered_index = None;
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// FUZZY SEARCH
// ═══════════════════════════════════════════════════════════════════════════

/// Case-insensitive fuzzy subsequence match
fn fuzzy_match(query: &str, target: &str) -> bool {
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

// ═══════════════════════════════════════════════════════════════════════════
// AI RESPONSE GENERATION — tries GGUF on-device inference, falls back to pattern matching
// ═══════════════════════════════════════════════════════════════════════════

/// Generate an AI response, attempting real GGUF inference if a model is loaded,
/// otherwise falling back to keyword-based pattern matching for built-in commands.
fn generate_ai_response(input: &str) -> ChatMessage {
    let lower = input.to_ascii_lowercase();

    // ── Tool-call patterns (handled locally, no LLM needed) ──────────────

    if lower.contains("open") && lower.contains("terminal") {
        return ChatMessage {
            role: ChatRole::Assistant,
            content: String::from("Opening a new terminal window for you."),
            timestamp: 0,
            tool_calls: vec![ToolCallResult {
                tool_name: String::from("open_app"),
                description: String::from("Opening Terminal"),
                status: ToolCallStatus::Success,
                output: String::from("Terminal window opened successfully"),
            }],
            is_streaming: false,
        };
    }

    if lower.contains("system") && (lower.contains("info") || lower.contains("status")) {
        let sys = crate::sysinfo::get_sysinfo();
        let info_str = alloc::format!(
            "OS: KnoxOS v0.2.1\nKernel: Rust bare-metal {}\nProcesses: {}\nRAM: {} MiB total, {} MiB free\nUptime: {}s",
            ARCH_NAME,
            sys.procs,
            sys.totalram / (1024 * 1024),
            sys.freeram / (1024 * 1024),
            sys.uptime
        );
        return ChatMessage {
            role: ChatRole::Assistant,
            content: String::from("Here's your current system information:"),
            timestamp: 0,
            tool_calls: vec![ToolCallResult {
                tool_name: String::from("system_info"),
                description: String::from("Querying system status"),
                status: ToolCallStatus::Success,
                output: info_str,
            }],
            is_streaming: false,
        };
    }

    if lower.contains("file")
        && (lower.contains("list") || lower.contains("show") || lower.contains("what"))
    {
        // List real files from VFS
        let entries = crate::vfs::list_dir_dispatch("/home/user/Documents");
        let file_list = match entries {
            Some(names) if !names.is_empty() => names.join("\n"),
            _ => String::from("(empty directory)"),
        };
        return ChatMessage {
            role: ChatRole::Assistant,
            content: String::from("Here are the files I found:"),
            timestamp: 0,
            tool_calls: vec![ToolCallResult {
                tool_name: String::from("list_files"),
                description: String::from("Listing /home/user/Documents"),
                status: ToolCallStatus::Success,
                output: file_list,
            }],
            is_streaming: false,
        };
    }

    if lower.contains("screenshot") || lower.contains("screen capture") {
        return ChatMessage {
            role: ChatRole::Assistant,
            content: String::from("Taking a screenshot now."),
            timestamp: 0,
            tool_calls: vec![ToolCallResult {
                tool_name: String::from("screenshot"),
                description: String::from("Capturing screen"),
                status: ToolCallStatus::Success,
                output: String::from("Screenshot saved to /home/user/Pictures/screenshot.png"),
            }],
            is_streaming: false,
        };
    }

    if lower.contains("open") && lower.contains("browser") {
        return ChatMessage {
            role: ChatRole::Assistant,
            content: String::from("Launching the web browser."),
            timestamp: 0,
            tool_calls: vec![ToolCallResult {
                tool_name: String::from("open_app"),
                description: String::from("Opening Browser"),
                status: ToolCallStatus::Success,
                output: String::from("Browser window opened"),
            }],
            is_streaming: false,
        };
    }

    if lower.contains("open") && lower.contains("setting") {
        return ChatMessage {
            role: ChatRole::Assistant,
            content: String::from("Opening Settings for you."),
            timestamp: 0,
            tool_calls: vec![ToolCallResult {
                tool_name: String::from("open_app"),
                description: String::from("Opening Settings"),
                status: ToolCallStatus::Success,
                output: String::from("Settings window opened"),
            }],
            is_streaming: false,
        };
    }

    if lower.contains("hello") || lower.contains("hi ") || lower == "hi" {
        return ChatMessage {
            role: ChatRole::Assistant,
            content: String::from(
                "Hello! I'm your KnoxOS AI assistant. I can help you with:\n\
                 \n- Opening apps (\"open terminal\")\n\
                 - Managing files (\"list my documents\")\n\
                 - System info (\"show system status\")\n\
                 - Running commands (\"run uname -a\")\n\
                 - And much more! Just ask.",
            ),
            timestamp: 0,
            tool_calls: Vec::new(),
            is_streaming: false,
        };
    }

    if lower.contains("help") {
        return ChatMessage {
            role: ChatRole::Assistant,
            content: String::from(
                "Here's what I can do:\n\n\
                 Open apps: \"open terminal\", \"launch browser\"\n\
                 Files: \"list files in Documents\", \"find *.rs\"\n\
                 System: \"show system info\", \"check memory\"\n\
                 Capture: \"take a screenshot\"\n\
                 Settings: \"open settings\", \"change resolution\"\n\
                 Tasks: \"monitor CPU usage\", \"scan for updates\"\n\n\
                 You can also use voice input with the microphone button!",
            ),
            timestamp: 0,
            tool_calls: Vec::new(),
            is_streaming: false,
        };
    }

    // ── Try real GGUF inference if a model is loaded ───────────────────
    if let Some(model_id) = crate::gguf::first_loaded_model_id() {
        let config = crate::gguf::GenerationConfig {
            max_tokens: 128,
            temperature: 0.7,
            top_k: 40,
            eos_token: 2,
        };
        if let Ok(response) = crate::gguf::generate(model_id, input, &config) {
            return ChatMessage {
                role: ChatRole::Assistant,
                content: response,
                timestamp: 0,
                tool_calls: Vec::new(),
                is_streaming: false,
            };
        }
    }

    // Default response when no model loaded and no pattern matched
    ChatMessage {
        role: ChatRole::Assistant,
        content: String::from(
            "I understand your request. As KnoxOS AI, I can help with \
             system tasks, app management, and answering questions. \
             Try asking me to open an app, list files, or check system status!",
        ),
        timestamp: 0,
        tool_calls: Vec::new(),
        is_streaming: false,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// RENDERING — Draw the Aurora Command Center
// ═══════════════════════════════════════════════════════════════════════════

/// Draw the start menu on the framebuffer — Aurora style floating panel
/// Appears centered above the floating dock.
pub fn draw_start_menu(fb: &mut FrameBuffer) {
    let menu = START_MENU.lock();
    if !menu.visible {
        return;
    }

    let screen_w = fb.width as i32;
    let screen_h = fb.height as i32;
    let dock_bottom_y = screen_h - super::scale::taskbar_height() as i32;
    let panel_h = PANEL_MAX_HEIGHT;
    let pw = PANEL_WIDTH as i32;

    // ── POSITION: Centered horizontally, floating above dock ──
    let panel_x = (screen_w - pw) / 2;
    let panel_y = dock_bottom_y - panel_h as i32 - DOCK_GAP;

    // ══════════════════════════════════════════════════════════════════
    // SHADOW — Multi-layer depth shadow
    // ══════════════════════════════════════════════════════════════════
    for i in (0i32..20).step_by(5) {
        let alpha = (18 - i).clamp(0, 16) as u8;
        fb.fill_rounded_rect_aa(
            Rect::new(
                panel_x + 4 - i / 3,
                panel_y + 6 + i / 2,
                PANEL_WIDTH + (i as u32) * 2 / 3,
                panel_h + i as u32 / 2,
            ),
            Pixel::new(0, 0, 0, alpha),
            PANEL_RADIUS + i as u32 / 3,
        );
    }
    // Colored glow shadow
    fb.fill_rounded_rect_aa(
        Rect::new(panel_x - 2, panel_y + 4, PANEL_WIDTH + 4, panel_h + 2),
        Pixel::new(80, 40, 30, 12),
        PANEL_RADIUS + 2,
    );

    // ══════════════════════════════════════════════════════════════════
    // FROSTED GLASS PANEL — Gradient for 3D depth
    // ══════════════════════════════════════════════════════════════════
    let panel_rect = Rect::new(panel_x, panel_y, PANEL_WIDTH, panel_h);
    let panel_top = Pixel::new(28, 26, 34, 235);
    let panel_bottom = Pixel::new(18, 16, 22, 240);
    fb.fill_rounded_rect_gradient_aa(panel_rect, panel_top, panel_bottom, PANEL_RADIUS);

    // ── Multi-layer warm border ──
    fb.draw_rounded_rect(panel_rect, Pixel::new(180, 120, 100, 40), PANEL_RADIUS, 1);
    let inner = Rect::new(panel_x + 1, panel_y + 1, PANEL_WIDTH - 2, panel_h - 2);
    fb.draw_rounded_rect(inner, Pixel::new(200, 160, 140, 16), PANEL_RADIUS - 1, 1);
    // Top shimmer
    fb.draw_hline(
        panel_x + PANEL_RADIUS as i32 + 2,
        panel_y + 1,
        PANEL_WIDTH.saturating_sub(PANEL_RADIUS * 2 + 4),
        Pixel::new(230, 200, 180, 24),
    );

    // Clip content to panel interior
    fb.push_clip(Rect::new(
        panel_x + 1,
        panel_y + 1,
        PANEL_WIDTH - 2,
        panel_h - 2,
    ));

    // ══════════════════════════════════════════════════════════════════
    // TAB BAR — Apps | AI | Tasks
    // ══════════════════════════════════════════════════════════════════
    let tab_y = panel_y + 14;
    draw_tab_bar(fb, panel_x, tab_y, PANEL_WIDTH, &menu);

    // ══════════════════════════════════════════════════════════════════
    // SEARCH BAR (universal)
    // ══════════════════════════════════════════════════════════════════
    let search_y = tab_y + TAB_BAR_H as i32 + 10;
    draw_search_bar(fb, panel_x, search_y, PANEL_WIDTH, &menu);

    // ══════════════════════════════════════════════════════════════════
    // CONTENT AREA — Tab-specific
    // ══════════════════════════════════════════════════════════════════
    let content_y = search_y + SEARCH_BAR_H as i32 + 8;
    let content_h = panel_h as i32 - (content_y - panel_y) - BOTTOM_BAR_H as i32;

    match menu.active_tab {
        CommandCenterTab::Apps => {
            draw_apps_content(fb, panel_x, content_y, PANEL_WIDTH, content_h as u32, &menu);
        }
        CommandCenterTab::AI => {
            draw_ai_content(fb, panel_x, content_y, PANEL_WIDTH, content_h as u32, &menu);
        }
        CommandCenterTab::Tasks => {
            draw_tasks_content(fb, panel_x, content_y, PANEL_WIDTH, content_h as u32, &menu);
        }
    }

    // ══════════════════════════════════════════════════════════════════
    // BOTTOM BAR — User info + power
    // ══════════════════════════════════════════════════════════════════
    let bottom_y = panel_y + panel_h as i32 - BOTTOM_BAR_H as i32;
    draw_bottom_bar(fb, panel_x, bottom_y, PANEL_WIDTH, &menu);

    // ══════════════════════════════════════════════════════════════════
    // POWER MENU POPUP — Shutdown / Restart / Log Out
    // ══════════════════════════════════════════════════════════════════
    if menu.power_menu_visible {
        let pm_w = 120i32;
        let pm_item_h = 30i32;
        let pm_h = pm_item_h * 3 + 8;
        let pm_x = panel_x + PANEL_WIDTH as i32 - PAD_H - pm_w - 10;
        let pm_y = bottom_y - pm_h - 4;

        // Popup background (dark glassmorphism)
        fb.fill_rounded_rect(
            Rect::new(pm_x, pm_y, pm_w as u32, pm_h as u32),
            Pixel::new(28, 26, 34, 240),
            10,
        );
        fb.draw_rounded_rect(
            Rect::new(pm_x, pm_y, pm_w as u32, pm_h as u32),
            Pixel::new(180, 130, 110, 50),
            10,
            1,
        );

        let items = [
            ("\u{23FB} Shut Down", Pixel::new(220, 85, 85, 220)), // ⏻ power
            ("\u{21BB} Restart", Pixel::new(230, 180, 90, 220)),  // ↻ restart
            ("\u{2192} Log Out", Pixel::new(140, 170, 200, 220)), // → logout
        ];

        for (i, (label, color)) in items.iter().enumerate() {
            let iy = pm_y + 4 + i as i32 * pm_item_h;
            // Hover highlight (approximate — actual hover would need mouse tracking)
            fb.fill_rounded_rect(
                Rect::new(pm_x + 4, iy, (pm_w - 8) as u32, pm_item_h as u32 - 2),
                Pixel::new(50, 45, 55, 60),
                6,
            );
            font_engine::draw_ui_text(fb, pm_x + 12, iy + 8, label, 12, *color);
        }
    }

    fb.pop_clip();
}

// ═══════════════════════════════════════════════════════════════════════════
// TAB BAR — Segmented control for Apps / AI / Tasks
// ═══════════════════════════════════════════════════════════════════════════

fn draw_tab_bar(fb: &mut FrameBuffer, panel_x: i32, tab_y: i32, panel_w: u32, menu: &StartMenu) {
    let tab_labels = ["Apps", "AI", "Tasks"];
    let tab_values = [
        CommandCenterTab::Apps,
        CommandCenterTab::AI,
        CommandCenterTab::Tasks,
    ];
    let tab_count = tab_labels.len() as i32;
    let tab_total_w = (panel_w as i32 - PAD_H * 2) as u32;
    let tab_w = tab_total_w / tab_count as u32;
    let tab_h = TAB_BAR_H - 4;

    // Tab bar background
    let bar_rect = Rect::new(panel_x + PAD_H, tab_y, tab_total_w, TAB_BAR_H);
    fb.fill_rounded_rect_aa(bar_rect, Pixel::new(22, 20, 28, 180), 10);

    for (i, &label) in tab_labels.iter().enumerate() {
        let tx = panel_x + PAD_H + (i as u32 * tab_w) as i32 + 2;
        let ty = tab_y + 2;
        let tw = tab_w - 4;
        let is_active = menu.active_tab == tab_values[i];

        if is_active {
            fb.fill_rounded_rect_aa(
                Rect::new(tx, ty, tw, tab_h),
                Pixel::new(232, 121, 100, 45),
                8,
            );
            fb.draw_rounded_rect(
                Rect::new(tx, ty, tw, tab_h),
                Pixel::new(232, 140, 120, 60),
                8,
                1,
            );
        }

        let icon_x = tx + 10;
        let text_x = icon_x + 16;
        let text_y = ty + (tab_h as i32 - 12) / 2;

        let icon_color = if is_active {
            Pixel::new(232, 140, 120, 255)
        } else {
            Pixel::new(140, 120, 110, 160)
        };

        match tab_values[i] {
            CommandCenterTab::Apps => {
                let ic = icon_x + 2;
                let iy = ty + (tab_h as i32 - 10) / 2;
                fb.fill_rounded_rect_aa(Rect::new(ic, iy, 4, 4), icon_color, 1);
                fb.fill_rounded_rect_aa(Rect::new(ic + 6, iy, 4, 4), icon_color, 1);
                fb.fill_rounded_rect_aa(Rect::new(ic, iy + 6, 4, 4), icon_color, 1);
                fb.fill_rounded_rect_aa(Rect::new(ic + 6, iy + 6, 4, 4), icon_color, 1);
            }
            CommandCenterTab::AI => {
                let ic = icon_x + 6;
                let iy = ty + tab_h as i32 / 2;
                fb.fill_circle_aa(ic, iy, 4, icon_color);
                fb.draw_circle_aa(
                    ic,
                    iy,
                    6,
                    Pixel::new(icon_color.r, icon_color.g, icon_color.b, icon_color.a / 2),
                );
            }
            CommandCenterTab::Tasks => {
                let ic = icon_x + 2;
                let iy = ty + (tab_h as i32 - 10) / 2;
                fb.fill_rounded_rect_aa(Rect::new(ic, iy, 3, 3), icon_color, 1);
                fb.draw_hline(ic + 5, iy + 1, 6, icon_color);
                fb.fill_rounded_rect_aa(Rect::new(ic, iy + 5, 3, 3), icon_color, 1);
                fb.draw_hline(ic + 5, iy + 6, 6, icon_color);
            }
        }

        let text_color = if is_active {
            Pixel::new(245, 238, 230, 255)
        } else {
            Pixel::new(140, 125, 115, 180)
        };
        font_engine::draw_ui_bold(fb, text_x, text_y, label, 12, text_color);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SEARCH BAR — Universal search with AI mode indicator & voice input
// ═══════════════════════════════════════════════════════════════════════════

fn draw_search_bar(
    fb: &mut FrameBuffer,
    panel_x: i32,
    search_y: i32,
    panel_w: u32,
    menu: &StartMenu,
) {
    let sx = panel_x + PAD_H;
    let sw = (panel_w as i32 - PAD_H * 2) as u32;

    fb.fill_rounded_rect_aa(
        Rect::new(sx, search_y, sw, SEARCH_BAR_H),
        Pixel::new(26, 24, 32, 210),
        SEARCH_BAR_RADIUS,
    );

    let border_color = if menu.search_active && menu.active_tab == CommandCenterTab::Apps {
        Pixel::new(232, 121, 100, 60)
    } else if menu.active_tab == CommandCenterTab::AI {
        Pixel::new(180, 140, 200, 50)
    } else {
        Pixel::new(80, 70, 65, 35)
    };
    fb.draw_rounded_rect(
        Rect::new(sx, search_y, sw, SEARCH_BAR_H),
        border_color,
        SEARCH_BAR_RADIUS,
        1,
    );

    // Magnifying glass icon
    let mag_cx = sx + 18;
    let mag_cy = search_y + SEARCH_BAR_H as i32 / 2;
    let mag_color = if menu.active_tab == CommandCenterTab::AI {
        Pixel::new(180, 140, 200, 180)
    } else {
        Pixel::new(180, 140, 120, 180)
    };
    fb.draw_circle_aa(mag_cx, mag_cy - 1, 6, mag_color);
    fb.draw_line_aa(mag_cx + 4, mag_cy + 3, mag_cx + 8, mag_cy + 7, mag_color);

    // Text or placeholder
    let text_x = sx + 34;
    let text_y = search_y + (SEARCH_BAR_H as i32 - 12) / 2;

    let display_text = match menu.active_tab {
        CommandCenterTab::Apps => {
            if menu.search_text.is_empty() {
                None
            } else {
                Some(menu.search_text.as_str())
            }
        }
        CommandCenterTab::AI => {
            if menu.chat_input.is_empty() {
                None
            } else {
                Some(menu.chat_input.as_str())
            }
        }
        CommandCenterTab::Tasks => None,
    };

    if let Some(text) = display_text {
        font_engine::draw_ui_text(fb, text_x, text_y, text, 12, Pixel::new(220, 230, 250, 240));
        let cursor_x = text_x + font_engine::measure_ui_text(text, 12) as i32 + 1;
        fb.fill_rect(
            Rect::new(cursor_x, text_y, 2, 12),
            Pixel::new(0, 200, 255, 200),
        );
    } else {
        let placeholder = match menu.active_tab {
            CommandCenterTab::Apps => "Search apps...",
            CommandCenterTab::AI => "Ask KnoxOS AI anything...",
            CommandCenterTab::Tasks => "Filter tasks...",
        };
        font_engine::draw_ui_text(
            fb,
            text_x,
            text_y,
            placeholder,
            12,
            Pixel::new(110, 100, 90, 120),
        );
    }

    // Voice input button (microphone)
    let mic_cx = sx + sw as i32 - 28;
    let mic_cy = search_y + SEARCH_BAR_H as i32 / 2;
    let mic_color = match menu.voice_state {
        VoiceState::Idle => Pixel::new(80, 130, 200, 150),
        VoiceState::Listening => Pixel::new(255, 80, 80, 255),
        VoiceState::Processing => Pixel::new(255, 200, 0, 200),
    };
    fb.fill_rounded_rect_aa(Rect::new(mic_cx - 3, mic_cy - 6, 6, 9), mic_color, 3);
    fb.draw_circle_aa(
        mic_cx,
        mic_cy + 2,
        5,
        Pixel::new(mic_color.r, mic_color.g, mic_color.b, mic_color.a / 2),
    );
    fb.draw_vline(mic_cx, mic_cy + 6, 3, mic_color);
    fb.draw_hline(mic_cx - 2, mic_cy + 9, 5, mic_color);

    if menu.voice_state == VoiceState::Listening {
        fb.draw_circle_aa(mic_cx, mic_cy, 10, Pixel::new(255, 60, 60, 80));
        fb.draw_circle_aa(mic_cx, mic_cy, 12, Pixel::new(255, 60, 60, 40));
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// APPS CONTENT — Pinned apps, all apps list with fuzzy search & categories
// ═══════════════════════════════════════════════════════════════════════════

fn draw_apps_content(
    fb: &mut FrameBuffer,
    panel_x: i32,
    content_y: i32,
    panel_w: u32,
    content_h: u32,
    menu: &StartMenu,
) {
    let mut cur_y = content_y;

    // Section header
    if !menu.search_text.is_empty() && !menu.filtered_indices.is_empty() {
        let result_text = alloc::format!(
            "{} result{}",
            menu.filtered_indices.len(),
            if menu.filtered_indices.len() == 1 {
                ""
            } else {
                "s"
            }
        );
        font_engine::draw_ui_text(
            fb,
            panel_x + PAD_H + 4,
            cur_y + 2,
            &result_text,
            12,
            Pixel::new(80, 140, 220, 140),
        );
        cur_y += 18;
    } else {
        font_engine::draw_ui_text(
            fb,
            panel_x + PAD_H + 4,
            cur_y + 2,
            "All Apps",
            12,
            Pixel::new(80, 140, 220, 120),
        );
        cur_y += 18;
    }

    // Scrollable app list
    let list_start_y = cur_y;
    let list_h = content_h as i32 - (cur_y - content_y);
    let max_visible = (list_h / MENU_ITEM_HEIGHT as i32).max(0) as usize;

    for (vi, &real_idx) in menu
        .filtered_indices
        .iter()
        .enumerate()
        .skip(menu.scroll_offset)
        .take(max_visible)
    {
        let item = &menu.items[real_idx];
        let row = (vi - menu.scroll_offset) as i32;
        let item_y = list_start_y + row * MENU_ITEM_HEIGHT as i32;

        let item_rect = Rect::new(
            panel_x + ITEM_INSET,
            item_y,
            panel_w - ITEM_INSET as u32 * 2,
            MENU_ITEM_HEIGHT,
        );

        let is_hovered = menu.hovered_index == Some(vi);
        if is_hovered {
            fb.fill_rounded_rect_aa(item_rect, Pixel::new(232, 121, 100, 35), 10);
            fb.fill_rounded_rect_aa(
                Rect::new(panel_x + ITEM_INSET, item_y + 8, 3, MENU_ITEM_HEIGHT - 16),
                Pixel::new(232, 140, 120, 200),
                1,
            );
        }

        // Icon (24×24)
        let icon_x = panel_x + ITEM_INSET + 14;
        let icon_y = item_y + (MENU_ITEM_HEIGHT as i32 - ITEM_ICON_SIZE) / 2;
        let (cat, iname) = super::desktop::icon_type_theme(item.icon_type);
        icon_theme::draw_small_icon(fb, icon_x, icon_y, cat, iname);

        // App name + description
        let text_x = icon_x + ITEM_ICON_SIZE + ICON_TEXT_GAP;
        let name_y = item_y + 7;
        let desc_y = item_y + 22;

        let name_color = if is_hovered {
            Pixel::new(252, 244, 236, 255)
        } else {
            Pixel::new(230, 220, 210, 230)
        };
        font_engine::draw_ui_text(fb, text_x, name_y, &item.name, 12, name_color);

        if !item.description.is_empty() {
            font_engine::draw_ui_text(
                fb,
                text_x,
                desc_y,
                &item.description,
                12,
                Pixel::new(120, 110, 100, 130),
            );
        }

        // Category badge
        let badge_text = match item.category {
            MenuCategory::Pinned => "PIN",
            MenuCategory::Game => "GAME",
            MenuCategory::AI => "AI",
            MenuCategory::Media => "MEDIA",
            MenuCategory::Development => "DEV",
            MenuCategory::System => "SYS",
            _ => "",
        };
        if !badge_text.is_empty() {
            let badge_w = font_engine::measure_ui_text(badge_text, 12) + 8;
            let badge_x = panel_x + panel_w as i32 - ITEM_INSET - badge_w as i32 - 8;
            let badge_y = item_y + (MENU_ITEM_HEIGHT as i32 - 16) / 2;
            let badge_color = match item.category {
                MenuCategory::Pinned => Pixel::new(232, 121, 100, 45),
                MenuCategory::Game => Pixel::new(230, 150, 60, 45),
                MenuCategory::AI => Pixel::new(180, 140, 200, 45),
                MenuCategory::Media => Pixel::new(130, 180, 150, 42),
                MenuCategory::Development => Pixel::new(200, 190, 100, 40),
                MenuCategory::System => Pixel::new(140, 160, 180, 40),
                _ => Pixel::new(90, 82, 76, 30),
            };
            fb.fill_rounded_rect_aa(Rect::new(badge_x, badge_y, badge_w, 16), badge_color, 6);
            let badge_text_color = match item.category {
                MenuCategory::Pinned => Pixel::new(240, 160, 140, 200),
                MenuCategory::Game => Pixel::new(240, 190, 100, 190),
                MenuCategory::AI => Pixel::new(200, 170, 220, 200),
                MenuCategory::Media => Pixel::new(150, 200, 170, 190),
                MenuCategory::Development => Pixel::new(220, 210, 140, 180),
                MenuCategory::System => Pixel::new(170, 185, 205, 180),
                _ => Pixel::new(140, 130, 120, 150),
            };
            font_engine::draw_ui_text(
                fb,
                badge_x + 4,
                badge_y + 2,
                badge_text,
                12,
                badge_text_color,
            );
        }
    }

    // Scroll indicator
    if menu.filtered_indices.len() > max_visible {
        let track_x = panel_x + panel_w as i32 - 6;
        let track_h = list_h.max(1);
        let thumb_ratio = max_visible as f32 / menu.filtered_indices.len() as f32;
        let thumb_h = ((track_h as f32 * thumb_ratio) as i32).max(20);
        let scroll_ratio = if menu.filtered_indices.len() > max_visible {
            menu.scroll_offset as f32 / (menu.filtered_indices.len() - max_visible) as f32
        } else {
            0.0
        };
        let thumb_y = list_start_y + ((track_h - thumb_h) as f32 * scroll_ratio) as i32;

        fb.fill_rounded_rect_aa(
            Rect::new(track_x, list_start_y, 4, track_h as u32),
            Pixel::new(30, 40, 60, 60),
            2,
        );
        fb.fill_rounded_rect_aa(
            Rect::new(track_x, thumb_y, 4, thumb_h as u32),
            Pixel::new(60, 140, 220, 120),
            2,
        );
    }

    // Empty state
    if menu.filtered_indices.is_empty() {
        let empty_y = content_y + content_h as i32 / 2 - 20;
        let cx = panel_x + panel_w as i32 / 2;
        {
            let _tw = font_engine::measure_ui_text("No apps found", 12) as i32;
            font_engine::draw_ui_text(
                fb,
                cx - _tw / 2,
                empty_y,
                "No apps found",
                12,
                Pixel::new(100, 120, 160, 160),
            );
        }
        {
            let _tw = font_engine::measure_ui_text("Try a different search query", 12) as i32;
            font_engine::draw_ui_text(
                fb,
                cx - _tw / 2,
                empty_y + 20,
                "Try a different search query",
                12,
                Pixel::new(80, 100, 140, 120),
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// AI CONTENT — Chat interface with tool calls, quick actions, voice
// ═══════════════════════════════════════════════════════════════════════════

fn draw_ai_content(
    fb: &mut FrameBuffer,
    panel_x: i32,
    content_y: i32,
    panel_w: u32,
    content_h: u32,
    menu: &StartMenu,
) {
    let content_w = panel_w as i32 - PAD_H * 2;

    // ── Quick Actions Row ──
    let qa_y = content_y + 4;
    let qa_total_w =
        QUICK_ACTION_COLS * QUICK_ACTION_SIZE + (QUICK_ACTION_COLS - 1) * QUICK_ACTION_GAP;
    let qa_start_x = panel_x + (panel_w as i32 - qa_total_w as i32) / 2;

    for (i, action) in menu.quick_actions.iter().enumerate() {
        let col = i as u32;
        let ax = qa_start_x + (col * (QUICK_ACTION_SIZE + QUICK_ACTION_GAP)) as i32;
        let ay = qa_y;

        let bg_color = if action.enabled {
            Pixel::new(
                action.color.r / 4,
                action.color.g / 4,
                action.color.b / 4,
                120,
            )
        } else {
            Pixel::new(16, 20, 32, 100)
        };
        let border_color = if action.enabled {
            Pixel::new(action.color.r, action.color.g, action.color.b, 80)
        } else {
            Pixel::new(40, 60, 100, 40)
        };

        fb.fill_rounded_rect_aa(
            Rect::new(ax, ay, QUICK_ACTION_SIZE, QUICK_ACTION_SIZE),
            bg_color,
            12,
        );
        fb.draw_rounded_rect(
            Rect::new(ax, ay, QUICK_ACTION_SIZE, QUICK_ACTION_SIZE),
            border_color,
            12,
            1,
        );

        let glyph_color = if action.enabled {
            action.color
        } else {
            Pixel::new(80, 100, 140, 140)
        };
        let glyph_str: String = alloc::format!("{}", action.icon_glyph);
        {
            let _tw = font_engine::measure_ui_text(&glyph_str, 14) as i32;
            font_engine::draw_ui_bold(
                fb,
                ax + QUICK_ACTION_SIZE as i32 / 2 - _tw / 2,
                ay + 4,
                &glyph_str,
                14,
                glyph_color,
            );
        }

        let label_color = if action.enabled {
            Pixel::new(180, 200, 230, 200)
        } else {
            Pixel::new(70, 90, 130, 130)
        };
        {
            let _tw = font_engine::measure_ui_text(&action.name, 11) as i32;
            font_engine::draw_ui_text(
                fb,
                ax + QUICK_ACTION_SIZE as i32 / 2 - _tw / 2,
                ay + QUICK_ACTION_SIZE as i32 - 14,
                &action.name,
                11,
                label_color,
            );
        }
    }

    // Divider
    let div_y = qa_y + QUICK_ACTION_SIZE as i32 + 10;
    fb.draw_hline(
        panel_x + PAD_H,
        div_y,
        (panel_w as i32 - PAD_H * 2).max(0) as u32,
        Pixel::new(50, 80, 140, 25),
    );

    // ── Chat Messages Area ──
    let chat_y = div_y + 8;
    let chat_h = content_h as i32 - (chat_y - content_y) - AI_INPUT_H as i32 - 8;

    fb.push_clip(Rect::new(
        panel_x + PAD_H,
        chat_y,
        content_w as u32,
        chat_h.max(0) as u32,
    ));

    let mut msg_y = chat_y + 4;
    for msg in &menu.chat_messages {
        msg_y = draw_chat_message(
            fb,
            panel_x + PAD_H,
            msg_y,
            content_w,
            msg,
            chat_y,
            chat_y + chat_h,
        );
        msg_y += 8;
    }

    // Thinking indicator
    if menu.ai_is_thinking {
        let thinking_y = msg_y + 4;
        if thinking_y < chat_y + chat_h {
            fb.fill_circle_aa(
                panel_x + PAD_H + 8,
                thinking_y + 8,
                5,
                colors::AI_BALANCED_1,
            );
            for dot in 0..3 {
                let dx = panel_x + PAD_H + 22 + dot * 10;
                let alpha = 120u8 + ((dot as u8 * 40) % 135);
                fb.fill_circle_aa(dx, thinking_y + 8, 3, Pixel::new(232, 140, 120, alpha));
            }
        }
    }

    fb.pop_clip();

    // Suggested Prompts
    if menu.chat_messages.len() <= 2 && !menu.ai_is_thinking {
        let suggest_y = chat_y + chat_h - (menu.suggested_prompts.len() as i32 * 24 + 8);
        if suggest_y > chat_y {
            for (i, prompt) in menu.suggested_prompts.iter().enumerate() {
                let sy = suggest_y + i as i32 * 24;
                let prompt_w = font_engine::measure_ui_text(prompt, 12) + 16;
                let sx = panel_x + PAD_H + 4;

                fb.fill_rounded_rect_aa(
                    Rect::new(sx, sy, prompt_w, 20),
                    Pixel::new(36, 32, 40, 120),
                    8,
                );
                fb.draw_rounded_rect(
                    Rect::new(sx, sy, prompt_w, 20),
                    Pixel::new(140, 110, 100, 50),
                    8,
                    1,
                );
                font_engine::draw_ui_text(
                    fb,
                    sx + 8,
                    sy + 4,
                    prompt,
                    12,
                    Pixel::new(190, 160, 140, 180),
                );
            }
        }
    }

    // ── AI Chat Input Bar ──
    let input_y = chat_y + chat_h + 4;
    let input_x = panel_x + PAD_H;
    let input_w = content_w - 40;

    fb.fill_rounded_rect_aa(
        Rect::new(input_x, input_y, input_w as u32, AI_INPUT_H),
        Pixel::new(26, 24, 32, 200),
        10,
    );
    fb.draw_rounded_rect(
        Rect::new(input_x, input_y, input_w as u32, AI_INPUT_H),
        Pixel::new(130, 100, 90, 40),
        10,
        1,
    );

    let chat_text_x = input_x + 12;
    let chat_text_y = input_y + (AI_INPUT_H as i32 - 12) / 2;
    if !menu.chat_input.is_empty() {
        font_engine::draw_ui_text(
            fb,
            chat_text_x,
            chat_text_y,
            &menu.chat_input,
            12,
            Pixel::new(235, 225, 215, 240),
        );
        let cursor_x = chat_text_x + font_engine::measure_ui_text(&menu.chat_input, 12) as i32 + 1;
        fb.fill_rect(
            Rect::new(cursor_x, chat_text_y, 2, 12),
            Pixel::new(180, 140, 200, 200),
        );
    } else {
        font_engine::draw_ui_text(
            fb,
            chat_text_x,
            chat_text_y,
            "Type a message or command...",
            12,
            Pixel::new(100, 90, 80, 110),
        );
    }

    // Send button
    let send_x = input_x + input_w + 6;
    let send_size = AI_INPUT_H - 8;
    fb.fill_rounded_rect_aa(
        Rect::new(send_x, input_y + 4, send_size, send_size),
        Pixel::new(200, 130, 110, 180),
        8,
    );
    let arrow_cx = send_x + send_size as i32 / 2;
    let arrow_cy = input_y + AI_INPUT_H as i32 / 2;
    fb.draw_line_aa(
        arrow_cx - 4,
        arrow_cy,
        arrow_cx + 4,
        arrow_cy,
        colors::WHITE,
    );
    fb.draw_line_aa(
        arrow_cx + 1,
        arrow_cy - 4,
        arrow_cx + 4,
        arrow_cy,
        colors::WHITE,
    );
    fb.draw_line_aa(
        arrow_cx + 1,
        arrow_cy + 4,
        arrow_cx + 4,
        arrow_cy,
        colors::WHITE,
    );
}

/// Draw a single chat message bubble, returns the Y coordinate after this message
fn draw_chat_message(
    fb: &mut FrameBuffer,
    area_x: i32,
    msg_y: i32,
    area_w: i32,
    msg: &ChatMessage,
    clip_top: i32,
    clip_bottom: i32,
) -> i32 {
    let max_bubble_w = ((area_w as u32 * AI_BUBBLE_MAX_W_PCT) / 100) as i32;
    let is_user = msg.role == ChatRole::User;

    let chars_per_line = (max_bubble_w - 24) / 8;
    let text_lines = if chars_per_line > 0 {
        ((msg.content.len() as i32 + chars_per_line - 1) / chars_per_line).max(1)
    } else {
        1
    };
    let text_h = text_lines * 14 + 4;

    let tool_h: i32 = msg
        .tool_calls
        .iter()
        .map(|tc| {
            let output_lines = tc.output.lines().count() as i32;
            36 + (output_lines.max(1) * 14)
        })
        .sum();

    let bubble_h = (text_h + tool_h + 16).max(28) as u32;
    let bubble_w = max_bubble_w.min(area_w - 16) as u32;

    if msg_y + bubble_h as i32 + 4 < clip_top || msg_y > clip_bottom {
        return msg_y + bubble_h as i32;
    }

    let bubble_x = if is_user {
        area_x + area_w - bubble_w as i32 - 4
    } else {
        area_x + 4
    };

    // Avatar dot
    if !is_user && msg_y + 8 >= clip_top {
        let avatar_color = match msg.role {
            ChatRole::Assistant => colors::AI_BALANCED_1,
            ChatRole::System => Pixel::new(255, 200, 0, 200),
            ChatRole::ToolResult => Pixel::new(0, 200, 140, 200),
            _ => Pixel::new(100, 100, 100, 200),
        };
        fb.fill_circle_aa(area_x, msg_y + 10, 4, avatar_color);
    }

    // Bubble background
    let bubble_bg = if is_user {
        Pixel::new(0, 100, 220, 160)
    } else {
        Pixel::new(24, 30, 48, 200)
    };
    fb.fill_rounded_rect_aa(
        Rect::new(bubble_x, msg_y, bubble_w, bubble_h),
        bubble_bg,
        10,
    );

    if !is_user {
        fb.draw_rounded_rect(
            Rect::new(bubble_x, msg_y, bubble_w, bubble_h),
            Pixel::new(50, 80, 140, 35),
            10,
            1,
        );
    }

    // Message text
    let text_color = if is_user {
        colors::WHITE
    } else {
        Pixel::new(200, 215, 240, 240)
    };
    fonts::draw_text_wrapped_compact(
        fb,
        bubble_x + 12,
        msg_y + 8,
        bubble_w - 24,
        &msg.content,
        text_color,
        1,
    );

    // Tool call results
    let mut tc_y = msg_y + text_h + 12;
    for tc in &msg.tool_calls {
        if tc_y > clip_bottom {
            break;
        }
        tc_y = draw_tool_call(fb, bubble_x + 8, tc_y, bubble_w - 16, tc);
        tc_y += 4;
    }

    msg_y + bubble_h as i32
}

/// Draw a tool call result card inside a chat bubble
fn draw_tool_call(fb: &mut FrameBuffer, x: i32, y: i32, w: u32, tc: &ToolCallResult) -> i32 {
    let output_lines = tc.output.lines().count() as i32;
    let card_h = (28 + output_lines.max(1) * 14) as u32;

    fb.fill_rounded_rect_aa(Rect::new(x, y, w, card_h), Pixel::new(10, 14, 26, 180), 6);

    let status_color = match tc.status {
        ToolCallStatus::Running => Pixel::new(255, 200, 0, 220),
        ToolCallStatus::Success => Pixel::new(0, 220, 140, 220),
        ToolCallStatus::Error => Pixel::new(255, 80, 80, 220),
    };
    fb.fill_circle_aa(x + 10, y + 10, 3, status_color);

    let name_str = alloc::format!("{}: {}", tc.tool_name, tc.description);
    font_engine::draw_ui_text(
        fb,
        x + 20,
        y + 4,
        &name_str,
        12,
        Pixel::new(100, 160, 230, 200),
    );

    let mut out_y = y + 22;
    for line in tc.output.lines() {
        font_engine::draw_ui_text(fb, x + 12, out_y, line, 12, Pixel::new(160, 180, 210, 190));
        out_y += 14;
    }

    y + card_h as i32
}

// ═══════════════════════════════════════════════════════════════════════════
// TASKS CONTENT — Background AI agent tasks & workflows
// ═══════════════════════════════════════════════════════════════════════════

fn draw_tasks_content(
    fb: &mut FrameBuffer,
    panel_x: i32,
    content_y: i32,
    panel_w: u32,
    content_h: u32,
    menu: &StartMenu,
) {
    let header_y = content_y + 4;
    let running_count = menu
        .agent_tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Running)
        .count();
    let header_text = alloc::format!(
        "{} active task{}",
        running_count,
        if running_count == 1 { "" } else { "s" }
    );
    font_engine::draw_ui_text(
        fb,
        panel_x + PAD_H + 4,
        header_y,
        &header_text,
        12,
        Pixel::new(80, 140, 220, 150),
    );

    let list_y = header_y + 22;
    let list_h = content_h as i32 - (list_y - content_y);
    let max_visible = (list_h / TASK_ITEM_H as i32).max(0) as usize;

    for (i, task) in menu.agent_tasks.iter().enumerate().take(max_visible) {
        let ty = list_y + i as i32 * TASK_ITEM_H as i32;
        draw_agent_task(
            fb,
            panel_x + ITEM_INSET,
            ty,
            panel_w - ITEM_INSET as u32 * 2,
            task,
        );
    }

    if menu.agent_tasks.is_empty() {
        let empty_y = content_y + content_h as i32 / 2 - 30;
        let cx = panel_x + panel_w as i32 / 2;
        {
            let _tw = font_engine::measure_ui_text("No active tasks", 12) as i32;
            font_engine::draw_ui_text(
                fb,
                cx - _tw / 2,
                empty_y,
                "No active tasks",
                12,
                Pixel::new(100, 120, 160, 160),
            );
        }
        {
            let _tw = font_engine::measure_ui_text("AI agent tasks will appear here", 12) as i32;
            font_engine::draw_ui_text(
                fb,
                cx - _tw / 2,
                empty_y + 20,
                "AI agent tasks will appear here",
                12,
                Pixel::new(80, 100, 140, 120),
            );
        }
    }
}

/// Draw a single agent task card
fn draw_agent_task(fb: &mut FrameBuffer, x: i32, y: i32, w: u32, task: &AgentTask) {
    fb.fill_rounded_rect_aa(
        Rect::new(x, y, w, TASK_ITEM_H - 4),
        Pixel::new(16, 20, 34, 160),
        10,
    );
    fb.draw_rounded_rect(
        Rect::new(x, y, w, TASK_ITEM_H - 4),
        Pixel::new(60, 54, 50, 30),
        10,
        1,
    );

    let status_color = match task.status {
        TaskStatus::Queued => Pixel::new(140, 132, 126, 200),
        TaskStatus::Running => Pixel::new(232, 140, 120, 240),
        TaskStatus::Complete => Pixel::new(130, 190, 150, 230),
        TaskStatus::Failed => Pixel::new(220, 85, 85, 230),
        TaskStatus::Paused => Pixel::new(230, 180, 70, 200),
    };
    fb.fill_circle_aa(x + 16, y + 14, 5, status_color);

    if task.status == TaskStatus::Running {
        fb.draw_circle_aa(x + 16, y + 14, 7, Pixel::new(232, 140, 120, 70));
    }

    font_engine::draw_ui_text(
        fb,
        x + 30,
        y + 6,
        &task.title,
        12,
        Pixel::new(230, 220, 210, 230),
    );
    font_engine::draw_ui_text(
        fb,
        x + 30,
        y + 20,
        &task.description,
        12,
        Pixel::new(120, 110, 100, 140),
    );

    // Progress bar
    let bar_x = x + 30;
    let bar_y = y + TASK_ITEM_H as i32 - 14;
    let bar_w = w as i32 - 46;

    fb.fill_rounded_rect_aa(
        Rect::new(bar_x, bar_y, bar_w.max(0) as u32, 4),
        Pixel::new(36, 32, 40, 120),
        2,
    );

    let fill_w = ((bar_w as u64 * task.progress as u64) / 100) as u32;
    if fill_w > 0 {
        let fill_color = match task.status {
            TaskStatus::Running => Pixel::new(232, 140, 120, 200),
            TaskStatus::Complete => Pixel::new(130, 190, 150, 200),
            TaskStatus::Failed => Pixel::new(220, 85, 85, 200),
            _ => Pixel::new(140, 132, 126, 160),
        };
        fb.fill_rounded_rect_aa(Rect::new(bar_x, bar_y, fill_w, 4), fill_color, 2);
    }

    let steps_text = alloc::format!("{}/{}", task.steps_done, task.steps_total);
    let steps_w = font_engine::measure_ui_text(&steps_text, 12);
    font_engine::draw_ui_text(
        fb,
        x + w as i32 - steps_w as i32 - 12,
        y + 10,
        &steps_text,
        12,
        Pixel::new(130, 120, 110, 150),
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// BOTTOM BAR — User profile, power, settings shortcuts
// ═══════════════════════════════════════════════════════════════════════════

fn draw_bottom_bar(
    fb: &mut FrameBuffer,
    panel_x: i32,
    bottom_y: i32,
    panel_w: u32,
    _menu: &StartMenu,
) {
    let pw = panel_w as i32;

    fb.draw_hline(
        panel_x + PAD_H,
        bottom_y,
        (pw - PAD_H * 2).max(0) as u32,
        Pixel::new(50, 80, 140, 20),
    );

    let bar_cy = bottom_y + BOTTOM_BAR_H as i32 / 2;

    // User avatar
    let user_cx = panel_x + PAD_H + 16;
    fb.fill_circle_aa(user_cx, bar_cy, 12, Pixel::new(25, 50, 90, 180));
    fb.draw_circle_aa(user_cx, bar_cy, 12, Pixel::new(60, 120, 200, 60));
    fb.fill_circle_aa(user_cx, bar_cy - 4, 4, Pixel::new(100, 160, 220, 160));
    fb.fill_circle_aa(user_cx, bar_cy + 5, 7, Pixel::new(80, 140, 200, 80));
    font_engine::draw_ui_text(
        fb,
        user_cx + 18,
        bar_cy - 6,
        "User",
        12,
        Pixel::new(160, 190, 230, 200),
    );

    // Settings gear
    let settings_cx = panel_x + pw - PAD_H - 50;
    fb.draw_circle_aa(settings_cx, bar_cy, 7, Pixel::new(80, 120, 180, 140));
    fb.fill_circle_aa(settings_cx, bar_cy, 3, Pixel::new(80, 120, 180, 100));

    // Power button
    let power_cx = panel_x + pw - PAD_H - 18;
    let power_col = Pixel::new(200, 70, 70, 190);
    fb.draw_circle_aa(power_cx, bar_cy, 9, power_col);
    fb.draw_vline(power_cx, bar_cy - 12, 7, power_col);
}

// ═══════════════════════════════════════════════════════════════════════════
// GEOMETRY & HIT TESTING
// ═══════════════════════════════════════════════════════════════════════════

/// Compute panel geometry for hit testing.
fn panel_geometry(screen_w: i32, screen_h: i32) -> (i32, i32, u32, i32, i32, usize) {
    let dock_bottom_y = screen_h - super::scale::taskbar_height() as i32;
    let panel_h = PANEL_MAX_HEIGHT;
    let pw = PANEL_WIDTH as i32;
    let panel_x = (screen_w - pw) / 2;
    let panel_y = dock_bottom_y - panel_h as i32 - DOCK_GAP;

    let tab_y = panel_y + 14;
    let search_y = tab_y + TAB_BAR_H as i32 + 10;
    let content_y = search_y + SEARCH_BAR_H as i32 + 8;
    let list_start_y = content_y + 18;
    let list_h = panel_h as i32 - (list_start_y - panel_y) - BOTTOM_BAR_H as i32;
    let max_visible = (list_h / MENU_ITEM_HEIGHT as i32).max(0) as usize;

    (
        panel_x,
        panel_y,
        panel_h,
        content_y,
        list_start_y,
        max_visible,
    )
}

/// Compute tab bar hit regions.
fn tab_hit_test(
    panel_x: i32,
    panel_y: i32,
    panel_w: u32,
    x: i32,
    y: i32,
) -> Option<CommandCenterTab> {
    let tab_y = panel_y + 14;
    let tab_total_w = (panel_w as i32 - PAD_H * 2) as u32;
    let tab_w = tab_total_w / 3;

    if y < tab_y || y >= tab_y + TAB_BAR_H as i32 {
        return None;
    }
    let rel_x = x - (panel_x + PAD_H);
    if rel_x < 0 || rel_x >= tab_total_w as i32 {
        return None;
    }
    match rel_x / tab_w as i32 {
        0 => Some(CommandCenterTab::Apps),
        1 => Some(CommandCenterTab::AI),
        2 => Some(CommandCenterTab::Tasks),
        _ => None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PUBLIC API — Click, Hover, Scroll, Keyboard, Toggle
// ═══════════════════════════════════════════════════════════════════════════

/// Handle a click on the start menu, returns true if consumed
pub fn handle_click(x: i32, y: i32) -> Option<StartMenuItem> {
    let mut menu = START_MENU.lock();
    if !menu.visible {
        return None;
    }

    let (screen_w, screen_h) = super::cached_screen_size();
    let (panel_x, panel_y, panel_h, content_y, list_start_y, max_visible) =
        panel_geometry(screen_w, screen_h);

    let panel_rect = Rect::new(panel_x, panel_y, PANEL_WIDTH, panel_h);

    if !panel_rect.contains(x, y) {
        menu.visible = false;
        return None;
    }

    // Tab bar clicks
    if let Some(tab) = tab_hit_test(panel_x, panel_y, PANEL_WIDTH, x, y) {
        menu.set_tab(tab);
        return None;
    }

    // Voice button click
    let search_y = panel_y + 14 + TAB_BAR_H as i32 + 10;
    let mic_cx = panel_x + PANEL_WIDTH as i32 - PAD_H - 28;
    let mic_cy = search_y + SEARCH_BAR_H as i32 / 2;
    if (x - mic_cx).abs() < 14 && (y - mic_cy).abs() < 14 {
        menu.voice_state = match menu.voice_state {
            VoiceState::Idle => VoiceState::Listening,
            VoiceState::Listening => VoiceState::Idle,
            VoiceState::Processing => VoiceState::Idle,
        };
        return None;
    }

    // Apps tab: item clicks
    if menu.active_tab == CommandCenterTab::Apps
        && x > panel_x + ITEM_INSET
        && x < panel_x + PANEL_WIDTH as i32 - ITEM_INSET
    {
        for (vi, &real_idx) in menu
            .filtered_indices
            .iter()
            .enumerate()
            .skip(menu.scroll_offset)
            .take(max_visible)
        {
            let item_y =
                list_start_y + ((vi - menu.scroll_offset) as i32 * MENU_ITEM_HEIGHT as i32);
            if y >= item_y && y < item_y + MENU_ITEM_HEIGHT as i32 {
                let result = menu.items[real_idx].clone();
                menu.items[real_idx].launch_count += 1;
                menu.visible = false;
                return Some(result);
            }
        }
    }

    // AI tab: quick action clicks
    if menu.active_tab == CommandCenterTab::AI {
        let qa_y = content_y + 4;
        let qa_total_w =
            QUICK_ACTION_COLS * QUICK_ACTION_SIZE + (QUICK_ACTION_COLS - 1) * QUICK_ACTION_GAP;
        let qa_start_x = panel_x + (PANEL_WIDTH as i32 - qa_total_w as i32) / 2;

        for (i, action) in menu.quick_actions.iter_mut().enumerate() {
            let col = i as u32;
            let ax = qa_start_x + (col * (QUICK_ACTION_SIZE + QUICK_ACTION_GAP)) as i32;
            let ay = qa_y;
            let action_rect = Rect::new(ax, ay, QUICK_ACTION_SIZE, QUICK_ACTION_SIZE);
            if action_rect.contains(x, y) {
                action.enabled = !action.enabled;
                return None;
            }
        }

        // Send button click
        let div_y = qa_y + QUICK_ACTION_SIZE as i32 + 10;
        let chat_y = div_y + 8;
        let ai_content_h = panel_h as i32 - (content_y - panel_y) - BOTTOM_BAR_H as i32;
        let chat_h = ai_content_h - (chat_y - content_y) - AI_INPUT_H as i32 - 8;
        let input_y = chat_y + chat_h + 4;
        let input_x = panel_x + PAD_H;
        let content_w = PANEL_WIDTH as i32 - PAD_H * 2;
        let input_w = content_w - 40;
        let send_x = input_x + input_w + 6;
        let send_size = AI_INPUT_H - 8;

        let send_rect = Rect::new(send_x, input_y + 4, send_size, send_size);
        if send_rect.contains(x, y) {
            menu.submit_chat();
            return None;
        }

        // Suggested prompt clicks
        if menu.chat_messages.len() <= 2 && !menu.ai_is_thinking {
            let suggest_y = chat_y + chat_h - (menu.suggested_prompts.len() as i32 * 24 + 8);
            for (i, prompt) in menu.suggested_prompts.iter().enumerate() {
                let sy = suggest_y + i as i32 * 24;
                let prompt_w = font_engine::measure_ui_text(prompt, 12) + 16;
                let sx = panel_x + PAD_H + 4;
                let prompt_rect = Rect::new(sx, sy, prompt_w, 20);
                if prompt_rect.contains(x, y) {
                    menu.chat_input = prompt.clone();
                    menu.submit_chat();
                    return None;
                }
            }
        }
    }

    // Bottom bar: power button — shows power menu (shutdown/restart/logout)
    let bottom_y = panel_y + panel_h as i32 - BOTTOM_BAR_H as i32;
    let bar_cy = bottom_y + BOTTOM_BAR_H as i32 / 2;
    let power_cx = panel_x + PANEL_WIDTH as i32 - PAD_H - 18;
    if (x - power_cx).abs() < 12 && (y - bar_cy).abs() < 12 {
        // Toggle the power menu popup
        menu.power_menu_visible = !menu.power_menu_visible;
        return None;
    }

    // Power menu option clicks (when power menu popup is visible)
    if menu.power_menu_visible {
        let pm_x = panel_x + PANEL_WIDTH as i32 - PAD_H - 130;
        let pm_y = bottom_y - 110;
        let pm_w = 120;
        let pm_item_h = 30;
        // Shutdown
        if x >= pm_x && x < pm_x + pm_w && y >= pm_y && y < pm_y + pm_item_h {
            menu.visible = false;
            menu.power_menu_visible = false;
            crate::serial_println!("[KnoxOS] Power: Shutdown requested via GUI");
            crate::acpi::shutdown();
        }
        // Restart
        if x >= pm_x && x < pm_x + pm_w && y >= pm_y + pm_item_h && y < pm_y + 2 * pm_item_h {
            menu.visible = false;
            menu.power_menu_visible = false;
            crate::serial_println!("[KnoxOS] Power: Reboot requested via GUI");
            crate::acpi::reboot();
        }
        // Log Out (lock screen)
        if x >= pm_x && x < pm_x + pm_w && y >= pm_y + 2 * pm_item_h && y < pm_y + 3 * pm_item_h {
            menu.visible = false;
            menu.power_menu_visible = false;
            crate::serial_println!("[KnoxOS] Power: Lock screen requested via GUI");
            crate::gui::lock_screen::lock();
            return None;
        }
        // Click anywhere else dismisses the power menu
        menu.power_menu_visible = false;
        return None;
    }

    // Bottom bar: settings shortcut
    let settings_cx = panel_x + PANEL_WIDTH as i32 - PAD_H - 50;
    if (x - settings_cx).abs() < 12 && (y - bar_cy).abs() < 12 {
        menu.visible = false;
        return Some(StartMenuItem {
            name: String::from("Settings"),
            description: String::from("System configuration"),
            category: MenuCategory::System,
            icon_type: super::desktop::IconType::Settings,
            keywords: String::new(),
            pinned: true,
            launch_count: 0,
            last_launched: 0,
        });
    }

    None
}

/// Toggle the start menu visibility
pub fn toggle() {
    START_MENU.lock().toggle();
    super::request_redraw();
}

/// Check if start menu is currently visible
pub fn is_visible() -> bool {
    START_MENU.lock().visible
}

/// Close the start menu
pub fn close() {
    let mut menu = START_MENU.lock();
    if menu.visible {
        menu.visible = false;
        drop(menu);
        super::request_redraw();
    }
}

/// Handle scroll wheel in start menu
pub fn handle_scroll(scroll_z: i8) {
    let mut menu = START_MENU.lock();
    if !menu.visible {
        return;
    }

    match menu.active_tab {
        CommandCenterTab::Apps => {
            if scroll_z > 0 {
                menu.scroll_offset = menu.scroll_offset.saturating_sub(2);
            } else {
                let max = menu.filtered_indices.len().saturating_sub(6);
                menu.scroll_offset = (menu.scroll_offset + 2).min(max);
            }
        }
        CommandCenterTab::AI => {
            if scroll_z > 0 {
                menu.chat_scroll_y = menu.chat_scroll_y.saturating_sub(40);
            } else {
                menu.chat_scroll_y += 40;
            }
        }
        CommandCenterTab::Tasks => {
            if scroll_z > 0 {
                menu.tasks_scroll_y = menu.tasks_scroll_y.saturating_sub(40);
            } else {
                menu.tasks_scroll_y += 40;
            }
        }
    }
}

/// Update hover state based on mouse position (called during redraw)
pub fn update_hover(mouse_x: i32, mouse_y: i32) {
    let mut menu = START_MENU.lock();
    if !menu.visible {
        return;
    }

    if menu.active_tab != CommandCenterTab::Apps {
        menu.hovered_index = None;
        return;
    }

    let (screen_w, screen_h) = super::cached_screen_size();
    let (panel_x, _panel_y, _panel_h, _content_y, list_start_y, max_visible) =
        panel_geometry(screen_w, screen_h);

    let mut hovered = None;
    if mouse_x > panel_x + ITEM_INSET && mouse_x < panel_x + PANEL_WIDTH as i32 - ITEM_INSET {
        for (vi, _) in menu
            .filtered_indices
            .iter()
            .enumerate()
            .skip(menu.scroll_offset)
            .take(max_visible)
        {
            let item_y =
                list_start_y + ((vi - menu.scroll_offset) as i32 * MENU_ITEM_HEIGHT as i32);
            if mouse_y >= item_y && mouse_y < item_y + MENU_ITEM_HEIGHT as i32 {
                hovered = Some(vi);
                break;
            }
        }
    }
    menu.hovered_index = hovered;
}

/// Handle keyboard input when command center is visible.
/// Returns true if the key was consumed.
pub fn handle_key(scancode: u8, ch: Option<char>) -> bool {
    let mut menu = START_MENU.lock();
    if !menu.visible {
        return false;
    }

    // Escape — close
    if scancode == 0x01 {
        menu.visible = false;
        drop(menu);
        super::request_redraw();
        return true;
    }

    // Tab key — cycle tabs
    if scancode == 0x0F {
        let next = match menu.active_tab {
            CommandCenterTab::Apps => CommandCenterTab::AI,
            CommandCenterTab::AI => CommandCenterTab::Tasks,
            CommandCenterTab::Tasks => CommandCenterTab::Apps,
        };
        menu.set_tab(next);
        return true;
    }

    // Enter — submit
    if scancode == 0x1C {
        match menu.active_tab {
            CommandCenterTab::Apps => {
                let target_idx = if let Some(vi) = menu.hovered_index {
                    menu.filtered_indices.get(vi).copied()
                } else if !menu.filtered_indices.is_empty() {
                    Some(menu.filtered_indices[0])
                } else {
                    None
                };

                if let Some(real_idx) = target_idx {
                    let result = menu.items[real_idx].clone();
                    menu.items[real_idx].launch_count += 1;
                    menu.visible = false;
                    drop(menu);
                    super::desktop::open_application(&result.name, result.icon_type);
                    super::request_redraw();
                    return true;
                }
            }
            CommandCenterTab::AI => {
                menu.submit_chat();
            }
            CommandCenterTab::Tasks => {}
        }
        return true;
    }

    // Backspace
    if scancode == 0x0E {
        menu.backspace();
        return true;
    }

    // Arrow Up
    if scancode == 0x48 {
        if menu.active_tab == CommandCenterTab::Apps && !menu.filtered_indices.is_empty() {
            let current = menu.hovered_index.unwrap_or(0);
            if current > 0 {
                menu.hovered_index = Some(current - 1);
                if current - 1 < menu.scroll_offset {
                    menu.scroll_offset = current - 1;
                }
            }
        }
        return true;
    }

    // Arrow Down
    if scancode == 0x50 {
        if menu.active_tab == CommandCenterTab::Apps && !menu.filtered_indices.is_empty() {
            let current = menu.hovered_index.unwrap_or(0);
            let max_idx = menu.filtered_indices.len().saturating_sub(1);
            if current < max_idx {
                menu.hovered_index = Some(current + 1);
            }
        }
        return true;
    }

    // Printable character — type into search/chat
    if let Some(ch) = ch {
        if ch.is_ascii_graphic() || ch == ' ' {
            menu.type_char(ch);
            return true;
        }
    }

    false
}
