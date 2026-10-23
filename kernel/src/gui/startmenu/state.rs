//! Command-center session state, app catalog, and the global `START_MENU`.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::gui::desktop::IconType;
use crate::gui::framebuffer::Pixel;

use super::ai::generate_ai_response;
use super::search::fuzzy_match;
use super::types::{
    AgentTask, ChatMessage, ChatRole, CommandCenterTab, MenuCategory, QuickAction, StartMenuItem,
    TaskStatus, VoiceState,
};

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
                icon_type: IconType::Terminal,
                keywords: String::from("shell bash zsh command prompt cli console"),
                pinned: true,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Files"),
                description: String::from("Browse and manage files"),
                category: MenuCategory::Pinned,
                icon_type: IconType::Folder,
                keywords: String::from("file manager explorer nautilus finder folder directory"),
                pinned: true,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Browser"),
                description: String::from("Web browser"),
                category: MenuCategory::Pinned,
                icon_type: IconType::Globe,
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
                icon_type: IconType::Document,
                keywords: String::from("code text editor vscode ide programming"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("AI Assistant"),
                description: String::from("Conversational AI with tool use"),
                category: MenuCategory::AI,
                icon_type: IconType::AIBrain,
                keywords: String::from("ai assistant chat llm gpt claude copilot neural"),
                pinned: true,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Vim"),
                description: String::from("Modal text editor"),
                category: MenuCategory::Development,
                icon_type: IconType::Terminal,
                keywords: String::from("vim vi editor modal text nvim neovim"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Settings"),
                description: String::from("System configuration"),
                category: MenuCategory::System,
                icon_type: IconType::Settings,
                keywords: String::from("settings preferences config system display sound network"),
                pinned: true,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Task Manager"),
                description: String::from("Monitor processes and system resources"),
                category: MenuCategory::System,
                icon_type: IconType::Settings,
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
                icon_type: IconType::Document,
                keywords: String::from("calculator calc math arithmetic compute add subtract"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Image Viewer"),
                description: String::from("View images and photos"),
                category: MenuCategory::Media,
                icon_type: IconType::Document,
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
                icon_type: IconType::Document,
                keywords: String::from("log viewer dmesg kernel messages syslog journal"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Calendar"),
                description: String::from("Calendar and date management"),
                category: MenuCategory::App,
                icon_type: IconType::Document,
                keywords: String::from("calendar date schedule event planner time"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Disk Utility"),
                description: String::from("Manage disks and partitions"),
                category: MenuCategory::System,
                icon_type: IconType::MyPC,
                keywords: String::from("disk utility partition format mount storage drive"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Bluetooth"),
                description: String::from("Bluetooth device manager"),
                category: MenuCategory::System,
                icon_type: IconType::Settings,
                keywords: String::from("bluetooth wireless pair device audio headset speaker"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Software Updater"),
                description: String::from("Check for and install system updates"),
                category: MenuCategory::System,
                icon_type: IconType::Settings,
                keywords: String::from("software updater update upgrade patch security"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Software Center"),
                description: String::from("Browse and install applications"),
                category: MenuCategory::App,
                icon_type: IconType::Settings,
                keywords: String::from("software center app store install package download"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Archive Manager"),
                description: String::from("Open and extract archive files"),
                category: MenuCategory::App,
                icon_type: IconType::Document,
                keywords: String::from("archive manager zip tar gz extract compress"),
                pinned: false,
                launch_count: 0,
                last_launched: 0,
            },
            StartMenuItem {
                name: String::from("Kit Gallery"),
                description: String::from("Knox UI component catalog"),
                category: MenuCategory::Development,
                icon_type: IconType::Game,
                keywords: String::from("gpui kit gallery ui widgets components catalog"),
                pinned: true,
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
