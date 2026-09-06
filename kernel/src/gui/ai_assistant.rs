use crate::serial_println;
/// AI Assistant Panel
///
/// System-integrated AI assistant panel: chat interface, context-aware
/// suggestions, code completion, document summarization, file search.
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use super::colors;
use super::fonts;
use super::framebuffer::{FrameBuffer, Pixel, Rect};
use super::window::{self, WindowContentType, WindowId};

/// Chat message
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Role {
    User,
    Assistant,
    System,
}

/// Suggestion context
#[derive(Debug, Clone)]
pub enum SuggestionContext {
    TextEditor {
        filename: String,
        language: String,
        cursor_line: u32,
    },
    Terminal {
        recent_commands: Vec<String>,
    },
    FileManager {
        current_dir: String,
    },
    Settings {
        section: String,
    },
    General,
}

/// Per-window AI chat state
struct AiWindowState {
    window_id: WindowId,
    messages: Vec<ChatMessage>,
    input_text: String,
    input_cursor: usize,
    scroll_y: i32,
    is_thinking: bool,
}

lazy_static::lazy_static! {
    static ref AI_STATES: Mutex<Vec<AiWindowState>> = Mutex::new(Vec::new());
}

/// AI assistant state (global singleton for panel/context features)
pub struct AiAssistant {
    pub history: Vec<ChatMessage>,
    pub model_name: String,
    pub context: SuggestionContext,
    pub max_history: usize,
    pub panel_visible: bool,
    pub streaming: bool,
}

lazy_static::lazy_static! {
    static ref ASSISTANT: Mutex<AiAssistant> = Mutex::new(AiAssistant {
        history: Vec::new(),
        model_name: String::new(),
        context: SuggestionContext::General,
        max_history: 100,
        panel_visible: false,
        streaming: false,
    });
}

impl AiAssistant {
    /// Send a user message and get response
    pub fn send_message(&mut self, content: &str) -> ChatMessage {
        let user_msg = ChatMessage {
            role: Role::User,
            content: String::from(content),
            timestamp: 0,
        };
        self.history.push(user_msg);

        serial_println!("[AI_ASSIST] User: {}", content);

        // Generate a contextual response based on the user query
        let response_text = generate_response(content);

        let response = ChatMessage {
            role: Role::Assistant,
            content: response_text,
            timestamp: 0,
        };
        self.history.push(response.clone());

        while self.history.len() > self.max_history {
            self.history.remove(0);
        }

        response
    }

    /// Set context for context-aware suggestions
    pub fn set_context(&mut self, ctx: SuggestionContext) {
        serial_println!("[AI_ASSIST] Context updated");
        self.context = ctx;
    }

    /// Toggle panel visibility
    pub fn toggle_panel(&mut self) {
        self.panel_visible = !self.panel_visible;
        serial_println!(
            "[AI_ASSIST] Panel {}",
            if self.panel_visible {
                "shown"
            } else {
                "hidden"
            }
        );
    }

    /// Clear chat history
    pub fn clear_history(&mut self) {
        self.history.clear();
        serial_println!("[AI_ASSIST] History cleared");
    }

    /// Get code completion suggestion
    pub fn complete_code(&self, prefix: &str, language: &str) -> Option<String> {
        let _ = (prefix, language);
        serial_println!("[AI_ASSIST] Code completion requested for {}", language);
        Some(String::from("// AI suggestion"))
    }

    /// Summarize a document
    pub fn summarize(&self, text: &str, max_sentences: usize) -> String {
        let _ = (text, max_sentences);
        serial_println!("[AI_ASSIST] Summarizing ({} chars)", text.len());
        String::from("[Summary placeholder]")
    }
}

pub fn init() {
    serial_println!("[AI_ASSIST] AI assistant panel initialized");
}

// ── Per-window AI state management ─────────────────────────────────

/// Create AI state for a new window
pub fn create_for_window(wid: WindowId) {
    let welcome = ChatMessage {
        role: Role::Assistant,
        content: String::from(
            "Hello! I'm the KnoxOS AI Assistant. I can help you with system tasks, \
             answer questions about your system, and manage files. Try asking me something!",
        ),
        timestamp: 0,
    };
    AI_STATES.lock().push(AiWindowState {
        window_id: wid,
        messages: vec![welcome],
        input_text: String::new(),
        input_cursor: 0,
        scroll_y: 0,
        is_thinking: false,
    });
}

/// Destroy AI state when window is closed
pub fn destroy_for_window(wid: WindowId) {
    AI_STATES.lock().retain(|s| s.window_id != wid);
}

/// Check if an AI assistant window is focused
pub fn focused_ai_window_id() -> Option<WindowId> {
    let wm = window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.focused_window {
        if let Some(win) = wm.windows.iter().find(|w| w.id == wid) {
            if win.content_type == WindowContentType::AIAssistant
                && win.state != window::WindowState::Minimized
            {
                return Some(wid);
            }
        }
    }
    None
}

// ── Click handler ───────────────────────────────────────────────────

pub fn handle_click(wid: WindowId, x: i32, y: i32) {
    let wm = window::WINDOW_MANAGER.lock();
    let (content, _scroll_y) = match wm.windows.iter().find(|w| w.id == wid) {
        Some(win) => (win.content_rect(), win.scroll_y),
        None => return,
    };
    drop(wm);

    let header_h = 48i32;
    let input_h = 50i32;
    let input_y = content.y + content.height as i32 - input_h;
    let send_btn_x = content.x + content.width as i32 - 42;
    let send_btn_y = input_y + 10;

    // Check send button click
    if x >= send_btn_x && x <= send_btn_x + 30 && y >= send_btn_y && y <= send_btn_y + 30 {
        send_current_message(wid);
        return;
    }

    // Check input bar click (focus the input field)
    if y >= input_y && y < input_y + input_h {
        // Position cursor based on click x
        let mut states = AI_STATES.lock();
        if let Some(state) = states.iter_mut().find(|s| s.window_id == wid) {
            let text_x = content.x + 20;
            let click_offset = ((x - text_x).max(0) as u32 / 7).min(state.input_text.len() as u32);
            state.input_cursor = click_offset as usize;
        }
        return;
    }

    // Check clear button in header (top-right)
    let clear_btn_x = content.x + content.width as i32 - 80;
    if y >= content.y && y < content.y + header_h && x >= clear_btn_x {
        let mut states = AI_STATES.lock();
        if let Some(state) = states.iter_mut().find(|s| s.window_id == wid) {
            let welcome = ChatMessage {
                role: Role::Assistant,
                content: String::from("Chat cleared. How can I help you?"),
                timestamp: 0,
            };
            state.messages.clear();
            state.messages.push(welcome);
            state.scroll_y = 0;
        }
        super::request_redraw();
        return;
    }
}

// ── Keyboard handler ────────────────────────────────────────────────

pub fn handle_char(wid: WindowId, ch: char) {
    let mut states = AI_STATES.lock();
    if let Some(state) = states.iter_mut().find(|s| s.window_id == wid) {
        state.input_text.insert(state.input_cursor, ch);
        state.input_cursor += ch.len_utf8();
        drop(states);
        super::request_redraw();
    }
}

pub fn handle_backspace(wid: WindowId) {
    let mut states = AI_STATES.lock();
    if let Some(state) = states.iter_mut().find(|s| s.window_id == wid) {
        if state.input_cursor > 0 {
            // Find the previous character boundary
            let prev = state.input_text[..state.input_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            state.input_text.remove(prev);
            state.input_cursor = prev;
        }
        drop(states);
        super::request_redraw();
    }
}

pub fn handle_enter(wid: WindowId) {
    send_current_message(wid);
}

pub fn handle_escape(wid: WindowId) {
    let mut states = AI_STATES.lock();
    if let Some(state) = states.iter_mut().find(|s| s.window_id == wid) {
        state.input_text.clear();
        state.input_cursor = 0;
    }
    drop(states);
    super::request_redraw();
}

pub fn handle_arrow_left(wid: WindowId) {
    let mut states = AI_STATES.lock();
    if let Some(state) = states.iter_mut().find(|s| s.window_id == wid) {
        if state.input_cursor > 0 {
            let prev = state.input_text[..state.input_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            state.input_cursor = prev;
            drop(states);
            super::request_redraw();
        }
    }
}

pub fn handle_arrow_right(wid: WindowId) {
    let mut states = AI_STATES.lock();
    if let Some(state) = states.iter_mut().find(|s| s.window_id == wid) {
        if state.input_cursor < state.input_text.len() {
            let next = state.input_text[state.input_cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| state.input_cursor + i)
                .unwrap_or(state.input_text.len());
            state.input_cursor = next;
            drop(states);
            super::request_redraw();
        }
    }
}

pub fn handle_home(wid: WindowId) {
    let mut states = AI_STATES.lock();
    if let Some(state) = states.iter_mut().find(|s| s.window_id == wid) {
        state.input_cursor = 0;
        drop(states);
        super::request_redraw();
    }
}

pub fn handle_end(wid: WindowId) {
    let mut states = AI_STATES.lock();
    if let Some(state) = states.iter_mut().find(|s| s.window_id == wid) {
        state.input_cursor = state.input_text.len();
        drop(states);
        super::request_redraw();
    }
}

pub fn handle_ctrl_key(wid: WindowId, ch: char) {
    match ch {
        'l' | '\x0C' => {
            // Ctrl+L — clear chat
            let mut states = AI_STATES.lock();
            if let Some(state) = states.iter_mut().find(|s| s.window_id == wid) {
                let welcome = ChatMessage {
                    role: Role::Assistant,
                    content: String::from("Chat cleared. How can I help you?"),
                    timestamp: 0,
                };
                state.messages.clear();
                state.messages.push(welcome);
                state.scroll_y = 0;
            }
            drop(states);
            super::request_redraw();
        }
        'u' | '\x15' => {
            // Ctrl+U — clear input
            let mut states = AI_STATES.lock();
            if let Some(state) = states.iter_mut().find(|s| s.window_id == wid) {
                state.input_text.clear();
                state.input_cursor = 0;
            }
            drop(states);
            super::request_redraw();
        }
        'a' | '\x01' => {
            // Ctrl+A — select all in input (move cursor to start)
            let mut states = AI_STATES.lock();
            if let Some(state) = states.iter_mut().find(|s| s.window_id == wid) {
                state.input_cursor = 0;
            }
            drop(states);
            super::request_redraw();
        }
        'e' | '\x05' => {
            // Ctrl+E — move cursor to end
            let mut states = AI_STATES.lock();
            if let Some(state) = states.iter_mut().find(|s| s.window_id == wid) {
                state.input_cursor = state.input_text.len();
            }
            drop(states);
            super::request_redraw();
        }
        _ => {}
    }
}

// ── Send message ────────────────────────────────────────────────────

fn send_current_message(wid: WindowId) {
    let mut states = AI_STATES.lock();
    let state = match states.iter_mut().find(|s| s.window_id == wid) {
        Some(s) => s,
        None => return,
    };

    let text = String::from(state.input_text.trim());
    if text.is_empty() {
        return;
    }

    // Add user message
    state.messages.push(ChatMessage {
        role: Role::User,
        content: text.clone(),
        timestamp: 0,
    });

    // Clear input
    state.input_text.clear();
    state.input_cursor = 0;

    // Generate AI response
    let response_text = generate_response(&text);

    state.messages.push(ChatMessage {
        role: Role::Assistant,
        content: response_text,
        timestamp: 0,
    });

    // Also log to global history
    drop(states);
    {
        let mut assistant = ASSISTANT.lock();
        assistant.history.push(ChatMessage {
            role: Role::User,
            content: text.clone(),
            timestamp: 0,
        });
    }

    super::request_redraw();
}

/// Generate a contextual response based on user input
fn generate_response(query: &str) -> String {
    let lower = query.to_lowercase();

    // System information queries
    if lower.contains("system") || lower.contains("info") || lower.contains("about") {
        return alloc::format!(
            "Here's your system information:\n\
             \u{2022} OS: KnoxOS v0.1.0\n\
             \u{2022} Architecture: {}\n\
             \u{2022} Kernel: Rust bare-metal\n\
             \u{2022} Uptime: {}s\n\
             \u{2022} Status: Running",
            core::env!("CARGO_PKG_NAME"),
            crate::clock::uptime_seconds(),
        );
    }

    // File-related queries
    if lower.contains("file") || lower.contains("directory") || lower.contains("folder") {
        let vfs = crate::vfs::VFS.lock();
        let entries = vfs.list_dir("/home/user").unwrap_or_default();
        drop(vfs);
        let listing: Vec<String> = entries
            .iter()
            .take(10)
            .map(|e| alloc::format!("  \u{2022} {}", e))
            .collect();
        if listing.is_empty() {
            return String::from(
                "Your home directory is empty. You can create files using the File Explorer or Terminal.",
            );
        }
        return alloc::format!("Here are the files in /home/user:\n{}", listing.join("\n"));
    }

    // Process queries
    if lower.contains("process") || lower.contains("running") || lower.contains("task") {
        let count = 1u32; // kernel process
        return alloc::format!(
            "Currently {} processes are running. You can view detailed process \
             information in the Task Manager (Ctrl+Shift+Escape).",
            count
        );
    }

    // Help queries
    if lower.contains("help") || lower.contains("what can") || lower.contains("how to") {
        return String::from(
            "I can help you with:\n\
             \u{2022} System information and status\n\
             \u{2022} File and directory listing\n\
             \u{2022} Process information\n\
             \u{2022} Keyboard shortcuts\n\
             \u{2022} Opening applications\n\
             \u{2022} General questions about KnoxOS\n\n\
             Try asking: \"Show system info\", \"List my files\", or \"What shortcuts are available?\"",
        );
    }

    // Keyboard shortcuts
    if lower.contains("shortcut") || lower.contains("hotkey") || lower.contains("keyboard") {
        return String::from(
            "Useful keyboard shortcuts:\n\
             \u{2022} Ctrl+Alt+T: Open Terminal\n\
             \u{2022} Ctrl+Alt+D: Show Desktop\n\
             \u{2022} Alt+Tab: Switch Windows\n\
             \u{2022} Super+E: Mission Control\n\
             \u{2022} Super+L: Lock Screen\n\
             \u{2022} Ctrl+Shift+Esc: Task Manager\n\
             \u{2022} Ctrl+W: Close Window\n\
             \u{2022} Ctrl+Shift+Left/Right: Snap Window",
        );
    }

    // Open app commands
    if lower.contains("open terminal") {
        super::desktop::open_application("Terminal", super::desktop::IconType::Terminal);
        return String::from("Opening Terminal for you!");
    }
    if lower.contains("open file") || lower.contains("open explorer") {
        super::desktop::open_application("Files", super::desktop::IconType::Folder);
        return String::from("Opening File Explorer for you!");
    }
    if lower.contains("open browser") {
        super::desktop::open_application("Browser", super::desktop::IconType::Globe);
        return String::from("Opening Browser for you!");
    }
    if lower.contains("open settings") {
        super::desktop::open_application("Settings", super::desktop::IconType::Settings);
        return String::from("Opening Settings for you!");
    }
    if lower.contains("open calculator") {
        super::desktop::open_application("Calculator", super::desktop::IconType::Document);
        return String::from("Opening Calculator for you!");
    }

    // Time/date queries
    if lower.contains("time") || lower.contains("date") || lower.contains("clock") {
        let up = crate::clock::uptime_seconds();
        let hours = up / 3600;
        let minutes = (up % 3600) / 60;
        let seconds = up % 60;
        return alloc::format!(
            "System time: {:02}:{:02}:{:02} uptime (hh:mm:ss).",
            hours,
            minutes,
            seconds
        );
    }

    // Uptime queries
    if lower.contains("uptime") || lower.contains("how long") {
        let secs = crate::clock::uptime_seconds();
        let mins = secs / 60;
        let hrs = mins / 60;
        return alloc::format!("System uptime: {}h {}m {}s", hrs, mins % 60, secs % 60);
    }

    // Memory queries
    if lower.contains("memory") || lower.contains("ram") || lower.contains("heap") {
        let up = crate::clock::uptime_seconds();
        return alloc::format!(
            "Memory info:\n\
             \u{2022} Uptime: {}s\n\
             \u{2022} Kernel heap is managed by the linked-list allocator",
            up,
        );
    }

    // Default response
    alloc::format!(
        "I understand you're asking about \"{}\". I'm a local AI assistant running \
         directly on KnoxOS. While I can help with system tasks and information, my \
         responses are generated locally. Try asking about system info, files, \
         processes, shortcuts, or ask me to open an application!",
        if query.len() > 60 {
            &query[..60]
        } else {
            query
        }
    )
}

// ── Drawing ─────────────────────────────────────────────────────────

/// Draw AI assistant content (called from window.rs draw_content dispatch)
pub fn draw_content(fb: &mut FrameBuffer, wid: WindowId, content: Rect, scroll_y: i32) {
    let states = AI_STATES.lock();
    let state = match states.iter().find(|s| s.window_id == wid) {
        Some(s) => s,
        None => {
            drop(states);
            // No state yet — draw placeholder
            fb.fill_rect(content, colors::WINDOW_BG);
            fonts::draw_string_compact(
                fb,
                content.x + 20,
                content.y + 40,
                "AI Assistant loading...",
                Pixel::rgb(150, 150, 150),
                1,
            );
            return;
        }
    };

    // Header bar
    let header_h: i32 = 48;
    fb.fill_rect(
        Rect::new(content.x, content.y, content.width, header_h as u32),
        Pixel::rgb(25, 28, 40),
    );
    fonts::draw_string_bold(
        fb,
        content.x + 16,
        content.y + 14,
        "KnoxOS AI Assistant",
        Pixel::rgb(100, 200, 255),
        2,
    );
    // Clear button
    let clear_x = content.x + content.width as i32 - 75;
    fb.fill_rounded_rect_aa(
        Rect::new(clear_x, content.y + 10, 60, 28),
        Pixel::rgb(50, 50, 70),
        6,
    );
    fonts::draw_string_compact(
        fb,
        clear_x + 10,
        content.y + 18,
        "Clear",
        Pixel::rgb(180, 180, 200),
        1,
    );
    fb.draw_hline(
        content.x,
        content.y + header_h - 1,
        content.width,
        Pixel::rgb(60, 60, 80),
    );

    // Chat area
    let input_h: i32 = 50;
    let chat_top = content.y + header_h;
    let chat_h = content.height as i32 - header_h - input_h;

    // Chat background
    fb.fill_rect(
        Rect::new(content.x, chat_top, content.width, chat_h as u32),
        colors::WINDOW_BG,
    );

    // Render messages
    let pad = 16i32;
    let mut msg_y = chat_top + pad - scroll_y;
    let max_bubble_w = (content.width as i32 - 48).min(440) as u32;

    for msg in &state.messages {
        let is_user = msg.role == Role::User;

        // Estimate text height (rough: 14px per line, ~50 chars per line at scale=1)
        let chars_per_line = (max_bubble_w as usize - 24).max(10) / 7;
        let lines = (msg.content.len() / chars_per_line).max(1) + msg.content.matches('\n').count();
        let bubble_h = (lines as i32 * 16 + 16).max(32);

        if msg_y + bubble_h > chat_top && msg_y < chat_top + chat_h {
            if is_user {
                // User message — right-aligned, blue bubble
                let bubble_x = content.x + content.width as i32 - max_bubble_w as i32 - pad;
                fb.fill_rounded_rect_aa(
                    Rect::new(bubble_x, msg_y, max_bubble_w, bubble_h as u32),
                    Pixel::rgb(0, 106, 230),
                    8,
                );
                fonts::draw_text_wrapped(
                    fb,
                    bubble_x + 12,
                    msg_y + 8,
                    max_bubble_w - 24,
                    &msg.content,
                    colors::WHITE,
                    1,
                );
            } else {
                // Assistant message — left-aligned, dark bubble
                let bubble_x = content.x + pad;
                fb.fill_rounded_rect_aa(
                    Rect::new(bubble_x, msg_y, max_bubble_w, bubble_h as u32),
                    Pixel::rgb(30, 35, 50),
                    8,
                );
                // AI indicator dot
                fb.fill_circle_aa(bubble_x - 6, msg_y + 8, 4, Pixel::rgb(100, 200, 255));
                fonts::draw_text_wrapped(
                    fb,
                    bubble_x + 12,
                    msg_y + 8,
                    max_bubble_w - 24,
                    &msg.content,
                    Pixel::rgb(200, 210, 230),
                    1,
                );
            }
        }

        msg_y += bubble_h + 8;
    }

    // Input bar (bottom)
    let input_y = content.y + content.height as i32 - input_h;
    fb.fill_rect(
        Rect::new(content.x, input_y, content.width, input_h as u32),
        Pixel::rgb(25, 28, 40),
    );
    fb.draw_hline(content.x, input_y, content.width, Pixel::rgb(60, 60, 80));

    // Input field background
    let input_field_w = content.width - 60;
    fb.fill_rounded_rect_aa(
        Rect::new(content.x + 12, input_y + 10, input_field_w, 30),
        Pixel::rgb(30, 30, 40),
        6,
    );
    fb.draw_rounded_rect(
        Rect::new(content.x + 12, input_y + 10, input_field_w, 30),
        Pixel::rgb(80, 120, 200),
        6,
        1,
    );

    // Input text or placeholder
    if state.input_text.is_empty() {
        fonts::draw_string_compact(
            fb,
            content.x + 20,
            input_y + 19,
            "Type a message and press Enter...",
            Pixel::rgb(100, 100, 120),
            1,
        );
    } else {
        // Draw text with cursor
        let display_text = &state.input_text;
        fonts::draw_string_compact(
            fb,
            content.x + 20,
            input_y + 19,
            display_text,
            Pixel::rgb(220, 225, 240),
            1,
        );

        // Draw cursor
        let cursor_x = content.x
            + 20
            + fonts::measure_string_width_compact(&state.input_text[..state.input_cursor], 1)
                as i32;
        fb.fill_rect(
            Rect::new(cursor_x, input_y + 14, 2, 18),
            Pixel::rgb(100, 200, 255),
        );
    }

    // Send button
    let send_x = content.x + content.width as i32 - 42;
    let send_color = if state.input_text.is_empty() {
        Pixel::rgb(50, 60, 80)
    } else {
        Pixel::rgb(0, 140, 255)
    };
    fb.fill_rounded_rect_aa(Rect::new(send_x, input_y + 10, 30, 30), send_color, 6);
    fonts::draw_string_centered_bold_compact(
        fb,
        send_x,
        input_y + 10,
        30,
        30,
        ">",
        colors::WHITE,
        1,
    );
}
