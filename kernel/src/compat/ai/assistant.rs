use alloc::string::String;
use alloc::vec::Vec;

/// Assistant message
#[derive(Debug, Clone)]
pub struct AssistantMessage {
    pub role: String, // "user", "assistant", "system"
    pub content: String,
    pub timestamp: u64,
}

/// Desktop assistant state
pub struct DesktopAssistant {
    pub active: bool,
    pub conversation: Vec<AssistantMessage>,
    pub model_name: String,
    pub system_prompt: String,
}

lazy_static::lazy_static! {
    static ref DESKTOP_ASSISTANT: spin::Mutex<DesktopAssistant> = spin::Mutex::new(DesktopAssistant {
        active: false,
        conversation: Vec::new(),
        model_name: String::new(),
        system_prompt: String::new(),
    });
}

/// Open the desktop AI assistant
pub fn assistant_open(model: &str) {
    let mut asst = DESKTOP_ASSISTANT.lock();
    asst.active = true;
    asst.model_name = String::from(model);
    asst.system_prompt =
        String::from("You are KnoxOS Assistant, a helpful AI integrated into the desktop.");
    asst.conversation.clear();
    crate::serial_println!("[AI] Desktop assistant opened (model={})", model);
}

/// Send a message to the assistant
pub fn assistant_chat(message: &str) -> String {
    let mut asst = DESKTOP_ASSISTANT.lock();
    asst.conversation.push(AssistantMessage {
        role: String::from("user"),
        content: String::from(message),
        timestamp: crate::hpet::read_counter(),
    });
    // In real implementation: run inference with conversation history
    let response = String::from(
        "I'm the KnoxOS assistant. I can help with system tasks, file management, and more.",
    );
    asst.conversation.push(AssistantMessage {
        role: String::from("assistant"),
        content: response.clone(),
        timestamp: crate::hpet::read_counter(),
    });
    response
}

/// Close the assistant
pub fn assistant_close() {
    DESKTOP_ASSISTANT.lock().active = false;
}
