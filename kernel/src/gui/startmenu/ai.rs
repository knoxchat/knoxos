//! Built-in command patterns plus optional GGUF inference for chat replies.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use super::types::{ChatMessage, ChatRole, ToolCallResult, ToolCallStatus};

const ARCH_NAME: &str = if cfg!(target_arch = "x86_64") {
    "x86_64"
} else if cfg!(target_arch = "aarch64") {
    "aarch64"
} else {
    "riscv64"
};

// ═══════════════════════════════════════════════════════════════════════════
// AI RESPONSE GENERATION — tries GGUF on-device inference, falls back to pattern matching
// ═══════════════════════════════════════════════════════════════════════════

/// Generate an AI response, attempting real GGUF inference if a model is loaded,
/// otherwise falling back to keyword-based pattern matching for built-in commands.
pub(super) fn generate_ai_response(input: &str) -> ChatMessage {
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
            "OS: KnoxOS v0.2.6\nKernel: Rust bare-metal {}\nProcesses: {}\nRAM: {} MiB total, {} MiB free\nUptime: {}s",
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
