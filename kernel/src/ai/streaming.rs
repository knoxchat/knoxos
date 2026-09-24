use alloc::string::String;

/// Token stream callback type
pub type TokenCallback = fn(&str);

/// Streaming inference state
pub struct StreamingInference {
    pub model_name: String,
    pub tokens_generated: u64,
    pub total_tokens: u64,
    pub buffer: String,
    pub finished: bool,
}

lazy_static::lazy_static! {
    static ref STREAMING_STATE: spin::Mutex<Option<StreamingInference>> = spin::Mutex::new(None);
}

/// Start streaming token generation
pub fn start_streaming(model_name: &str, prompt: &str, max_tokens: u64) -> bool {
    *STREAMING_STATE.lock() = Some(StreamingInference {
        model_name: String::from(model_name),
        tokens_generated: 0,
        total_tokens: max_tokens,
        buffer: String::from(prompt),
        finished: false,
    });
    crate::serial_println!(
        "[AI] Streaming started: model={}, max_tokens={}",
        model_name,
        max_tokens
    );
    true
}

/// Get next token from streaming inference
pub fn poll_token() -> Option<String> {
    let mut state = STREAMING_STATE.lock();
    if let Some(ref mut s) = *state {
        if !s.finished && s.tokens_generated < s.total_tokens {
            s.tokens_generated += 1;
            // In real implementation: run one forward pass, sample next token
            let token = String::from(" ");
            s.buffer.push_str(&token);
            if s.tokens_generated >= s.total_tokens {
                s.finished = true;
            }
            return Some(token);
        }
    }
    None
}

/// Check if streaming is complete
pub fn streaming_finished() -> bool {
    STREAMING_STATE.lock().as_ref().is_none_or(|s| s.finished)
}

/// Get the full generated text so far
pub fn streaming_buffer() -> String {
    STREAMING_STATE
        .lock()
        .as_ref()
        .map_or(String::new(), |s| s.buffer.clone())
}
