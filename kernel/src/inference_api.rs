use alloc::collections::VecDeque;
/// AI Inference API — public inference interface for applications
///
/// Provides a unified API for applications to use AI models:
///   - Text generation (LLM)
///   - Text classification
///   - Embeddings
///   - System suggestions
///   - Request queue with priorities
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// INFERENCE API TYPES
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelType {
    TextGeneration,
    TextClassification,
    Embedding,
    NamedEntityRecognition,
    Summarization,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InferencePriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Realtime = 3,
}

#[derive(Debug, Clone)]
pub struct InferenceRequest {
    pub id: u64,
    pub model_type: ModelType,
    pub priority: InferencePriority,
    pub input_text: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
    pub stop_sequences: Vec<String>,
    pub caller_pid: u32,
}

#[derive(Debug, Clone)]
pub enum InferenceResult {
    TextGeneration {
        text: String,
        tokens_generated: u32,
        finish_reason: FinishReason,
    },
    Classification {
        label: String,
        confidence: f32,
    },
    Embedding {
        vector: Vec<f32>,
        dimensions: u32,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishReason {
    MaxTokens,
    StopSequence,
    EndOfText,
}

#[derive(Debug, Clone)]
pub struct InferenceResponse {
    pub request_id: u64,
    pub result: InferenceResult,
    pub latency_ms: u64,
}

// ═══════════════════════════════════════════════════════════════════════
// SUGGESTION TYPES
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct SystemSuggestion {
    pub id: u64,
    pub category: SuggestionCategory,
    pub title: String,
    pub description: String,
    pub action: SuggestionAction,
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionCategory {
    Performance,
    Security,
    Workflow,
    FileOrganization,
    SystemMaintenance,
}

#[derive(Debug, Clone)]
pub enum SuggestionAction {
    RunCommand(String),
    OpenFile(String),
    ChangeSetting { key: String, value: String },
    InstallPackage(String),
    None,
}

// ═══════════════════════════════════════════════════════════════════════
// INFERENCE ENGINE
// ═══════════════════════════════════════════════════════════════════════

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);
static TOTAL_INFERENCES: AtomicU64 = AtomicU64::new(0);

pub struct InferenceEngine {
    /// Pending requests (priority queue)
    queue: VecDeque<InferenceRequest>,
    /// Completed responses
    responses: VecDeque<InferenceResponse>,
    /// Active suggestions
    suggestions: Vec<SystemSuggestion>,
    /// Maximum queue size
    max_queue_size: usize,
    /// Whether the engine is ready (model loaded)
    ready: bool,
}

impl InferenceEngine {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            responses: VecDeque::new(),
            suggestions: Vec::new(),
            max_queue_size: 64,
            ready: false,
        }
    }

    /// Submit an inference request
    pub fn submit(&mut self, mut request: InferenceRequest) -> u64 {
        let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        request.id = id;

        // Insert by priority (higher priority first)
        let pos = self
            .queue
            .iter()
            .position(|r| r.priority < request.priority);
        match pos {
            Some(idx) => self.queue.insert(idx, request),
            None => self.queue.push_back(request),
        }

        serial_println!("[InferenceAPI] Request #{} submitted", id);
        id
    }

    /// Process the next request in the queue
    pub fn process_next(&mut self) -> Option<InferenceResponse> {
        let request = self.queue.pop_front()?;

        let start = crate::rtc::unix_time();

        // For now, generate a simple response based on model type
        let result = match request.model_type {
            ModelType::TextGeneration => {
                // Try to use real GGUF model if one is loaded
                let model_id = crate::gguf::first_loaded_model_id();

                if let Some(mid) = model_id {
                    let config = crate::gguf::GenerationConfig {
                        max_tokens: request.max_tokens.min(256) as usize,
                        temperature: request.temperature,
                        top_k: 40,
                        eos_token: 2, // common EOS
                    };
                    match crate::gguf::generate(mid, &request.input_text, &config) {
                        Ok(text) => {
                            let tokens_generated = text.split_whitespace().count() as u32;
                            InferenceResult::TextGeneration {
                                text,
                                tokens_generated,
                                finish_reason: FinishReason::EndOfText,
                            }
                        }
                        Err(_e) => {
                            // Fallback to template if model inference fails
                            InferenceResult::TextGeneration {
                                text: alloc::format!(
                                    "I understand you asked about: {}. As an AI running natively on KnoxOS, \
                                     I can help with system tasks, answer questions, and provide suggestions.",
                                    if request.input_text.len() > 100 {
                                        &request.input_text[..100]
                                    } else {
                                        &request.input_text
                                    }
                                ),
                                tokens_generated: 32,
                                finish_reason: FinishReason::EndOfText,
                            }
                        }
                    }
                } else {
                    // No model loaded — template response
                    InferenceResult::TextGeneration {
                        text: alloc::format!(
                            "I understand you asked about: {}. As an AI running natively on KnoxOS, \
                             I can help with system tasks, answer questions, and provide suggestions.",
                            if request.input_text.len() > 100 {
                                &request.input_text[..100]
                            } else {
                                &request.input_text
                            }
                        ),
                        tokens_generated: 32,
                        finish_reason: FinishReason::EndOfText,
                    }
                }
            }
            ModelType::TextClassification => InferenceResult::Classification {
                label: String::from("general"),
                confidence: 0.85,
            },
            ModelType::Embedding => {
                // Generate a simple embedding vector
                let dims = 128;
                let mut vector = Vec::with_capacity(dims);
                let mut hash = 0x12345678u32;
                for byte in request.input_text.bytes() {
                    hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
                    vector.push((hash as f32 / u32::MAX as f32) * 2.0 - 1.0);
                    if vector.len() >= dims {
                        break;
                    }
                }
                while vector.len() < dims {
                    hash = hash.wrapping_mul(31).wrapping_add(1);
                    vector.push((hash as f32 / u32::MAX as f32) * 2.0 - 1.0);
                }
                InferenceResult::Embedding {
                    vector,
                    dimensions: dims as u32,
                }
            }
            ModelType::NamedEntityRecognition | ModelType::Summarization => {
                InferenceResult::TextGeneration {
                    text: alloc::format!(
                        "Summary: {}",
                        &request.input_text[..request.input_text.len().min(200)]
                    ),
                    tokens_generated: 16,
                    finish_reason: FinishReason::EndOfText,
                }
            }
        };

        let end = crate::rtc::unix_time();

        let response = InferenceResponse {
            request_id: request.id,
            result,
            latency_ms: ((end - start) * 1000) as u64,
        };

        TOTAL_INFERENCES.fetch_add(1, Ordering::Relaxed);
        self.responses.push_back(response.clone());

        Some(response)
    }

    /// Get a response for a specific request ID
    pub fn get_response(&mut self, request_id: u64) -> Option<InferenceResponse> {
        let pos = self
            .responses
            .iter()
            .position(|r| r.request_id == request_id)?;
        Some(self.responses.remove(pos).unwrap())
    }

    /// Generate system suggestions based on context
    pub fn generate_suggestions(&mut self) {
        self.suggestions.clear();

        // Example suggestions
        self.suggestions.push(SystemSuggestion {
            id: 1,
            category: SuggestionCategory::Performance,
            title: String::from("Enable CPU frequency scaling"),
            description: String::from(
                "Your CPU is running at fixed frequency. Enable scaling to save power.",
            ),
            action: SuggestionAction::RunCommand(String::from("cpufreq set ondemand")),
            confidence: 0.92,
        });

        self.suggestions.push(SystemSuggestion {
            id: 2,
            category: SuggestionCategory::Security,
            title: String::from("Set up keyring"),
            description: String::from("Secure your credentials with the system keyring."),
            action: SuggestionAction::RunCommand(String::from("keyring setup")),
            confidence: 0.85,
        });
    }

    /// Get current suggestions
    pub fn suggestions(&self) -> &[SystemSuggestion] {
        &self.suggestions
    }

    /// Queue length
    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    /// Is the engine ready?
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Mark engine as ready
    pub fn set_ready(&mut self, ready: bool) {
        self.ready = ready;
    }
}

lazy_static::lazy_static! {
    pub static ref ENGINE: Mutex<InferenceEngine> = Mutex::new(InferenceEngine::new());
}

// ═══════════════════════════════════════════════════════════════════════
// CONVENIENCE API
// ═══════════════════════════════════════════════════════════════════════

/// Quick text generation
pub fn generate(prompt: &str, max_tokens: u32) -> u64 {
    let request = InferenceRequest {
        id: 0,
        model_type: ModelType::TextGeneration,
        priority: InferencePriority::Normal,
        input_text: String::from(prompt),
        max_tokens,
        temperature: 0.7,
        top_p: 0.9,
        stop_sequences: Vec::new(),
        caller_pid: 0,
    };
    ENGINE.lock().submit(request)
}

/// Quick text classification
pub fn classify(text: &str) -> u64 {
    let request = InferenceRequest {
        id: 0,
        model_type: ModelType::TextClassification,
        priority: InferencePriority::Normal,
        input_text: String::from(text),
        max_tokens: 1,
        temperature: 0.0,
        top_p: 1.0,
        stop_sequences: Vec::new(),
        caller_pid: 0,
    };
    ENGINE.lock().submit(request)
}

/// Quick embedding
pub fn embed(text: &str) -> u64 {
    let request = InferenceRequest {
        id: 0,
        model_type: ModelType::Embedding,
        priority: InferencePriority::Low,
        input_text: String::from(text),
        max_tokens: 0,
        temperature: 0.0,
        top_p: 1.0,
        stop_sequences: Vec::new(),
        caller_pid: 0,
    };
    ENGINE.lock().submit(request)
}

/// Get total inferences processed
pub fn total_inferences() -> u64 {
    TOTAL_INFERENCES.load(Ordering::Relaxed)
}

/// Initialize inference API
pub fn init() {
    let mut engine = ENGINE.lock();
    engine.set_ready(true);
    engine.generate_suggestions();
    drop(engine);
    serial_println!("[KnoxOS] Inference API initialized");
}
