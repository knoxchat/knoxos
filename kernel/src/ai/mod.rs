//! AI Inference Engine
//! Provides on-device AI inference capabilities
//! Supports simple tensor operations and neural network forward pass
//! Accessible via custom syscalls (0x1000+)
//!
//! Split into submodules for maintainability:
//!   math        — no_std float approximations (sqrt, exp, tanh)
//!   tensor      — DType, Shape, Tensor ops
//!   network     — Layer, NeuralNetwork
//!   registry    — load/infer/unload for feedforward models
//!   query       — pattern-match NLP assistant
//!   audio_ai    — Whisper STT / WaveNet TTS
//!   gpu_offload — GPU tensor dispatch
//!   simd        — CPU SIMD tensor kernels
//!   cache       — in-memory model cache
//!   streaming   — token streaming to UI
//!   assistant   — desktop assistant session
//!   voice       — STT/TTS pipeline
//!   image       — text-to-image stub
//!   jobs        — concurrent inference queue
//!   models      — downloadable model registry
use alloc::vec::Vec;

mod assistant;
pub mod audio_ai;
mod cache;
pub mod gpu_offload;
mod image;
mod jobs;
mod math;
mod models;
mod network;
mod query;
mod registry;
pub mod simd;
mod streaming;
mod tensor;
mod voice;

pub(crate) use math::{fast_exp, fast_sqrt, fast_tanh};
pub(crate) use registry::MODELS;

pub use assistant::{
    AssistantMessage, DesktopAssistant, assistant_chat, assistant_close, assistant_open,
};
pub use cache::{CachedModel, cache_model, cache_stats, evict_model};
pub use image::{ImageGenParams, generate_image};
pub use jobs::{
    InferenceJob, InferenceJobStatus, active_inference_jobs, poll_inference_job,
    submit_inference_job,
};
pub use models::{ModelInfo, delete_model, download_model, list_model_registry, register_model};
pub use network::{Layer, NeuralNetwork};
pub use query::query;
pub use registry::{infer, list_models, load_model, unload_model};
pub use streaming::{
    StreamingInference, TokenCallback, poll_token, start_streaming, streaming_buffer,
    streaming_finished,
};
pub use tensor::{DType, Shape, Tensor};
pub use voice::{
    VoicePipeline, VoicePipelineState, voice_pipeline_init, voice_process_audio, voice_speak,
    voice_start_listening,
};

/// Initialize the AI subsystem
pub fn init() {
    crate::serial_println!("[KnoxOS] AI inference engine initialized");
    crate::serial_println!("[KnoxOS]   Supported: tensor ops, feedforward NN, pattern-match NLP");

    // Detect SIMD capabilities
    let level = simd::detect();
    crate::serial_println!("[KnoxOS]   SIMD level: {:?}", level);
    crate::serial_println!(
        "[KnoxOS]   SIMD ops: dot_product, matmul, relu, softmax, layer_norm, rms_norm"
    );

    // Initialize GPU compute offload
    let gpu_ok = gpu_offload::init();
    if gpu_ok {
        crate::serial_println!(
            "[KnoxOS]   GPU offload: enabled (matmul, vec_add, relu, softmax, forward_pass)"
        );
    } else {
        crate::serial_println!("[KnoxOS]   GPU offload: disabled (SIMD-only mode)");
    }

    // Initialize audio AI models (Whisper/WaveNet)
    audio_ai::init();
}

/// GPU-accelerated matrix multiply (via Vulkan compute shaders)
pub fn gpu_matrix_multiply(a: &[f32], b: &[f32], m: usize, n: usize, k: usize) -> Vec<f32> {
    // Check if GPU offload is available, otherwise fall back to SIMD
    if gpu_offload::is_enabled() {
        gpu_offload::matmul_gpu(a, m, k, b, k, n)
            .unwrap_or_else(|_| simd::matmul_simd(a, b, m, k, n))
    } else {
        simd::matmul_simd(a, b, m, k, n)
    }
}
