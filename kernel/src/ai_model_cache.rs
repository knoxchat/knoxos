use crate::serial_println;
/// AI Model Cache & Management
///
/// Caching of downloaded AI models, streaming token generation,
/// model management UI data, voice-to-text pipeline, multi-model inference.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Cached model entry
#[derive(Debug, Clone)]
pub struct CachedModel {
    pub name: String,
    pub version: String,
    pub size_bytes: u64,
    pub format: ModelFormat,
    pub hash: [u8; 32], // SHA-256 of model file
    pub last_used: u64,
    pub loaded: bool,
}

/// Model serialization format
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModelFormat {
    Onnx,
    SafeTensors,
    Gguf,
    Custom,
}

/// Streaming token output
#[derive(Debug, Clone)]
pub struct TokenStream {
    pub model_name: String,
    pub tokens: Vec<String>,
    pub finished: bool,
    pub total_time_ms: u64,
    pub tokens_per_sec: f32,
}

/// Voice-to-text pipeline state
#[derive(Debug)]
pub struct VoiceToText {
    pub model_name: String,
    pub sample_rate: u32,
    pub buffer: Vec<i16>,
    pub language: String,
    pub active: bool,
}

/// Multi-model inference request
#[derive(Debug)]
pub struct MultiModelRequest {
    pub models: Vec<String>,
    pub input: String,
    pub strategy: MergeStrategy,
}

#[derive(Debug, Clone, Copy)]
pub enum MergeStrategy {
    First,
    Best,
    Ensemble,
}

/// Model cache manager
pub struct ModelCache {
    pub models: Vec<CachedModel>,
    pub cache_dir: String,
    pub max_cache_bytes: u64,
    pub used_bytes: u64,
    pub voice: Option<VoiceToText>,
}

lazy_static::lazy_static! {
    static ref CACHE: Mutex<ModelCache> = Mutex::new(ModelCache {
        models: Vec::new(),
        cache_dir: String::new(),
        max_cache_bytes: 4 * 1024 * 1024 * 1024, // 4 GB
        used_bytes: 0,
        voice: None,
    });
}

impl ModelCache {
    /// Register a model in the cache
    pub fn register_model(&mut self, model: CachedModel) {
        serial_println!(
            "[AI_CACHE] Model registered: {} v{} ({} bytes)",
            model.name,
            model.version,
            model.size_bytes
        );
        self.used_bytes += model.size_bytes;
        self.models.push(model);
    }

    /// Evict least recently used models to free space
    pub fn evict_lru(&mut self, needed_bytes: u64) {
        while self.used_bytes + needed_bytes > self.max_cache_bytes {
            if let Some(idx) = self
                .models
                .iter()
                .enumerate()
                .filter(|(_, m)| !m.loaded)
                .min_by_key(|(_, m)| m.last_used)
                .map(|(i, _)| i)
            {
                let removed = self.models.remove(idx);
                self.used_bytes -= removed.size_bytes;
                serial_println!("[AI_CACHE] Evicted: {}", removed.name);
            } else {
                break;
            }
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> (usize, u64, u64) {
        (self.models.len(), self.used_bytes, self.max_cache_bytes)
    }

    /// Start streaming token generation
    pub fn start_stream(&self, model_name: &str) -> TokenStream {
        serial_println!("[AI_CACHE] Starting token stream from {}", model_name);
        TokenStream {
            model_name: String::from(model_name),
            tokens: Vec::new(),
            finished: false,
            total_time_ms: 0,
            tokens_per_sec: 0.0,
        }
    }

    /// Initialize voice-to-text
    pub fn init_voice(&mut self, model: &str, sample_rate: u32, lang: &str) {
        self.voice = Some(VoiceToText {
            model_name: String::from(model),
            sample_rate,
            buffer: Vec::new(),
            language: String::from(lang),
            active: false,
        });
        serial_println!(
            "[AI_CACHE] Voice-to-text initialized: model={} sr={}",
            model,
            sample_rate
        );
    }

    /// Feed audio samples to voice-to-text
    pub fn voice_feed(&mut self, samples: &[i16]) -> Option<String> {
        if let Some(vtt) = &mut self.voice {
            vtt.buffer.extend_from_slice(samples);
            // When enough samples accumulated, run inference
            if vtt.buffer.len() >= vtt.sample_rate as usize * 2 {
                let text = String::from("[transcribed text]");
                vtt.buffer.clear();
                return Some(text);
            }
        }
        None
    }

    /// List all cached models
    pub fn list_models(&self) -> Vec<&CachedModel> {
        self.models.iter().collect()
    }

    /// Delete a model from cache
    pub fn delete_model(&mut self, name: &str) -> bool {
        if let Some(idx) = self.models.iter().position(|m| m.name == name) {
            let m = self.models.remove(idx);
            self.used_bytes -= m.size_bytes;
            serial_println!("[AI_CACHE] Deleted: {}", name);
            true
        } else {
            false
        }
    }
}

pub fn init() {
    serial_println!("[AI_CACHE] Model cache initialized (max 4GB)");
}
