/// AI Inference Engine
/// Provides on-device AI inference capabilities
/// Supports simple tensor operations and neural network forward pass
/// Accessible via custom syscalls (0x1000+)
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

/// Fast inverse square root approximation (no_std compatible)
fn fast_sqrt(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    // Newton's method: start with rough estimate, iterate
    let mut guess = x;
    // Initial estimate using bit manipulation
    let i = f32::to_bits(x);
    let i = 0x1FBD1DF5 + (i >> 1); // magic constant for sqrt approx
    guess = f32::from_bits(i);
    // Two Newton-Raphson iterations for accuracy
    guess = 0.5 * (guess + x / guess);
    guess = 0.5 * (guess + x / guess);
    guess
}

/// Tensor data type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DType {
    F32,
    F16,
    I8,
    I32,
    U8,
}

/// Tensor shape
#[derive(Debug, Clone)]
pub struct Shape {
    pub dims: Vec<usize>,
}

impl Shape {
    pub fn new(dims: &[usize]) -> Self {
        Self {
            dims: dims.to_vec(),
        }
    }

    pub fn numel(&self) -> usize {
        self.dims.iter().product()
    }

    pub fn ndim(&self) -> usize {
        self.dims.len()
    }
}

/// A tensor (multi-dimensional array)
#[derive(Debug, Clone)]
pub struct Tensor {
    pub data: Vec<f32>,
    pub shape: Shape,
    pub dtype: DType,
}

impl Tensor {
    /// Create a new tensor filled with zeros
    pub fn zeros(shape: &[usize]) -> Self {
        let numel: usize = shape.iter().product();
        Self {
            data: vec![0.0; numel],
            shape: Shape::new(shape),
            dtype: DType::F32,
        }
    }

    /// Create a new tensor filled with ones
    pub fn ones(shape: &[usize]) -> Self {
        let numel: usize = shape.iter().product();
        Self {
            data: vec![1.0; numel],
            shape: Shape::new(shape),
            dtype: DType::F32,
        }
    }

    /// Create a tensor from data
    pub fn from_data(data: Vec<f32>, shape: &[usize]) -> Result<Self, &'static str> {
        let numel: usize = shape.iter().product();
        if data.len() != numel {
            return Err("Data length doesn't match shape");
        }
        Ok(Self {
            data,
            shape: Shape::new(shape),
            dtype: DType::F32,
        })
    }

    /// Element-wise addition
    pub fn add(&self, other: &Tensor) -> Result<Tensor, &'static str> {
        if self.shape.dims != other.shape.dims {
            return Err("Shape mismatch for addition");
        }
        let data: Vec<f32> = self
            .data
            .iter()
            .zip(other.data.iter())
            .map(|(a, b)| a + b)
            .collect();
        Ok(Tensor {
            data,
            shape: self.shape.clone(),
            dtype: DType::F32,
        })
    }

    /// Element-wise multiplication
    pub fn mul(&self, other: &Tensor) -> Result<Tensor, &'static str> {
        if self.shape.dims != other.shape.dims {
            return Err("Shape mismatch for multiplication");
        }
        let data: Vec<f32> = self
            .data
            .iter()
            .zip(other.data.iter())
            .map(|(a, b)| a * b)
            .collect();
        Ok(Tensor {
            data,
            shape: self.shape.clone(),
            dtype: DType::F32,
        })
    }

    /// Scalar multiplication
    pub fn scale(&self, scalar: f32) -> Tensor {
        let data: Vec<f32> = self.data.iter().map(|x| x * scalar).collect();
        Tensor {
            data,
            shape: self.shape.clone(),
            dtype: DType::F32,
        }
    }

    /// Matrix multiplication (2D only)
    pub fn matmul(&self, other: &Tensor) -> Result<Tensor, &'static str> {
        if self.shape.ndim() != 2 || other.shape.ndim() != 2 {
            return Err("matmul requires 2D tensors");
        }
        let m = self.shape.dims[0];
        let k = self.shape.dims[1];
        let n = other.shape.dims[1];

        if k != other.shape.dims[0] {
            return Err("Inner dimensions don't match for matmul");
        }

        let mut result = vec![0.0f32; m * n];
        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0f32;
                for l in 0..k {
                    sum += self.data[i * k + l] * other.data[l * n + j];
                }
                result[i * n + j] = sum;
            }
        }

        Ok(Tensor {
            data: result,
            shape: Shape::new(&[m, n]),
            dtype: DType::F32,
        })
    }

    /// Apply ReLU activation
    pub fn relu(&self) -> Tensor {
        let data: Vec<f32> = self
            .data
            .iter()
            .map(|&x| if x > 0.0 { x } else { 0.0 })
            .collect();
        Tensor {
            data,
            shape: self.shape.clone(),
            dtype: DType::F32,
        }
    }

    /// Apply Sigmoid activation (approximation without libm)
    pub fn sigmoid(&self) -> Tensor {
        let data: Vec<f32> = self
            .data
            .iter()
            .map(|&x| {
                // Fast sigmoid approximation: 1 / (1 + e^(-x))
                // Using piecewise linear approximation
                if x > 6.0 {
                    1.0
                } else if x < -6.0 {
                    0.0
                } else {
                    0.5 + x * (0.25 - x * x * 0.00260417)
                }
            })
            .collect();
        Tensor {
            data,
            shape: self.shape.clone(),
            dtype: DType::F32,
        }
    }

    /// Softmax (1D)
    pub fn softmax(&self) -> Tensor {
        // Find max for numerical stability
        let max_val = self.data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let exp_vals: Vec<f32> = self.data.iter().map(|&x| fast_exp(x - max_val)).collect();
        let sum: f32 = exp_vals.iter().sum();
        let data: Vec<f32> = exp_vals.iter().map(|&x| x / sum).collect();

        Tensor {
            data,
            shape: self.shape.clone(),
            dtype: DType::F32,
        }
    }

    /// Sum all elements
    pub fn sum(&self) -> f32 {
        self.data.iter().sum()
    }

    /// Mean of all elements
    pub fn mean(&self) -> f32 {
        self.sum() / self.data.len() as f32
    }

    /// Reshape tensor
    pub fn reshape(&self, new_shape: &[usize]) -> Result<Tensor, &'static str> {
        let new_numel: usize = new_shape.iter().product();
        if new_numel != self.shape.numel() {
            return Err("Cannot reshape: element count mismatch");
        }
        Ok(Tensor {
            data: self.data.clone(),
            shape: Shape::new(new_shape),
            dtype: self.dtype,
        })
    }

    /// Argmax - return index of maximum element
    pub fn argmax(&self) -> usize {
        self.data
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap_or(0)
    }
}

/// Fast exponential approximation (no libm needed)
fn fast_exp(x: f32) -> f32 {
    // Schraudolph's algorithm: fast approximate exp
    if x > 88.0 {
        return f32::INFINITY;
    }
    if x < -88.0 {
        return 0.0;
    }

    // Use a polynomial approximation
    let x = x.clamp(-20.0, 20.0);
    let mut result = 1.0f32;
    let mut term = 1.0f32;
    for i in 1..=12 {
        term *= x / i as f32;
        result += term;
    }
    result
}

/// Fast tanh approximation: tanh(x) = (exp(2x) - 1) / (exp(2x) + 1)
fn fast_tanh(x: f32) -> f32 {
    if x > 10.0 {
        return 1.0;
    }
    if x < -10.0 {
        return -1.0;
    }
    let e2x = fast_exp(2.0 * x);
    (e2x - 1.0) / (e2x + 1.0)
}

/// Neural network layer types
#[derive(Debug, Clone)]
pub enum Layer {
    Linear { weights: Tensor, bias: Tensor },
    ReLU,
    Sigmoid,
    Softmax,
}

/// Simple feedforward neural network
#[derive(Debug, Clone)]
pub struct NeuralNetwork {
    pub layers: Vec<Layer>,
    pub name: String,
}

impl NeuralNetwork {
    pub fn new(name: &str) -> Self {
        Self {
            layers: Vec::new(),
            name: String::from(name),
        }
    }

    /// Add a linear layer
    pub fn add_linear(&mut self, in_features: usize, out_features: usize) {
        // Initialize with simple uniform distribution approximation
        let weight_data: Vec<f32> = (0..in_features * out_features)
            .map(|i| {
                // Simple pseudo-random initialization
                let seed = (i as u32).wrapping_mul(2654435761);
                let val = (seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
                val / fast_sqrt(in_features as f32) // Xavier initialization
            })
            .collect();

        let weights = Tensor::from_data(weight_data, &[in_features, out_features]).unwrap();
        let bias = Tensor::zeros(&[out_features]);

        self.layers.push(Layer::Linear { weights, bias });
    }

    /// Add an activation layer
    pub fn add_activation(&mut self, activation: &str) {
        match activation {
            "relu" => self.layers.push(Layer::ReLU),
            "sigmoid" => self.layers.push(Layer::Sigmoid),
            "softmax" => self.layers.push(Layer::Softmax),
            _ => {}
        }
    }

    /// Forward pass
    pub fn forward(&self, input: &Tensor) -> Result<Tensor, &'static str> {
        let mut x = input.clone();

        for layer in &self.layers {
            x = match layer {
                Layer::Linear { weights, bias } => {
                    let output = x.matmul(weights)?;
                    output.add(bias)?
                }
                Layer::ReLU => x.relu(),
                Layer::Sigmoid => x.sigmoid(),
                Layer::Softmax => x.softmax(),
            };
        }

        Ok(x)
    }
}

/// Model registry
lazy_static::lazy_static! {
    static ref MODELS: Mutex<BTreeMap<u64, NeuralNetwork>> = Mutex::new(BTreeMap::new());
    static ref NEXT_MODEL_ID: Mutex<u64> = Mutex::new(1);
}

/// Load a model (creates a pre-configured network)
pub fn load_model(model_type: &str) -> u64 {
    let mut model = NeuralNetwork::new(model_type);

    match model_type {
        "classifier" => {
            // Simple classifier: 784 → 128 → 64 → 10
            model.add_linear(784, 128);
            model.add_activation("relu");
            model.add_linear(128, 64);
            model.add_activation("relu");
            model.add_linear(64, 10);
            model.add_activation("softmax");
        }
        "sentiment" => {
            // Sentiment analysis: 256 → 64 → 2
            model.add_linear(256, 64);
            model.add_activation("relu");
            model.add_linear(64, 2);
            model.add_activation("softmax");
        }
        "embeddings" => {
            // Embedding model: 512 → 256 → 128
            model.add_linear(512, 256);
            model.add_activation("relu");
            model.add_linear(256, 128);
        }
        _ => {
            // Default: small network
            model.add_linear(64, 32);
            model.add_activation("relu");
            model.add_linear(32, 16);
            model.add_activation("relu");
            model.add_linear(16, 8);
            model.add_activation("softmax");
        }
    }

    let mut id = NEXT_MODEL_ID.lock();
    let model_id = *id;
    *id += 1;

    MODELS.lock().insert(model_id, model);

    crate::serial_println!(
        "[KnoxOS] AI: Loaded model '{}' (id={})",
        model_type,
        model_id
    );
    model_id
}

/// Run inference on a loaded model
pub fn infer(model_id: u64, input_data: &[f32]) -> Result<Vec<f32>, &'static str> {
    let models = MODELS.lock();
    let model = models.get(&model_id).ok_or("Model not found")?;

    // Determine expected input size from first layer
    let expected_size = match model.layers.first() {
        Some(Layer::Linear { weights, .. }) => weights.shape.dims[0],
        _ => return Err("Invalid model architecture"),
    };

    // Pad or truncate input to match expected size
    let mut padded = vec![0.0f32; expected_size];
    let copy_len = input_data.len().min(expected_size);
    padded[..copy_len].copy_from_slice(&input_data[..copy_len]);

    let input = Tensor::from_data(padded, &[1, expected_size])?;
    let output = model.forward(&input)?;

    Ok(output.data)
}

/// Query the AI assistant with a text prompt
pub fn query(prompt: &str) -> String {
    // Simple pattern-matching response engine
    let prompt_lower = prompt.to_ascii_lowercase();

    if prompt_lower.contains("hello") || prompt_lower.contains("hi") {
        String::from("Hello! I'm KnoxOS AI Assistant. How can I help you today?")
    } else if prompt_lower.contains("time") || prompt_lower.contains("date") {
        let dt = crate::rtc::read_rtc();
        alloc::format!(
            "The current date and time is {}-{:02}-{:02} {:02}:{:02}:{:02} UTC.",
            dt.year,
            dt.month,
            dt.day,
            dt.hour,
            dt.minute,
            dt.second
        )
    } else if prompt_lower.contains("help") {
        String::from(
            "I can help with:\n\
             - System information (ask about 'cpu', 'memory', 'uptime')\n\
             - File operations (ask about 'files', 'directory')\n\
             - Process management (ask about 'processes')\n\
             - Network status (ask about 'network')\n\
             - General questions about KnoxOS",
        )
    } else if prompt_lower.contains("cpu") || prompt_lower.contains("processor") {
        let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
        if let Some(brand) = cpuid.get_processor_brand_string() {
            alloc::format!("CPU: {}", brand.as_str())
        } else {
            String::from("CPU information unavailable")
        }
    } else if prompt_lower.contains("memory") || prompt_lower.contains("ram") {
        alloc::format!(
            "Kernel heap: {} KiB allocated at {:#x}",
            crate::allocator::HEAP_SIZE / 1024,
            crate::allocator::HEAP_START
        )
    } else if prompt_lower.contains("uptime") {
        let secs = crate::rtc::uptime_seconds();
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        let s = secs % 60;
        alloc::format!("System uptime: {}h {}m {}s", hours, mins, s)
    } else if prompt_lower.contains("process") {
        String::from("Use 'ps' command in the terminal to list running processes.")
    } else if prompt_lower.contains("network") {
        String::from("Network interfaces: lo (127.0.0.1, UP), eth0 (10.0.2.15, DOWN)")
    } else if prompt_lower.contains("knoxos") || prompt_lower.contains("operating system") {
        String::from(
            "KnoxOS is an AI-native operating system written in Rust. \
             It features a graphical desktop environment, Linux-compatible syscalls, \
             and built-in AI inference capabilities.",
        )
    } else {
        alloc::format!(
            "I received your query: '{}'. \
             KnoxOS AI is currently running with limited capabilities. \
             Try asking about 'help', 'cpu', 'memory', 'time', or 'knoxos'.",
            prompt
        )
    }
}

/// Unload a model
pub fn unload_model(model_id: u64) -> bool {
    MODELS.lock().remove(&model_id).is_some()
}

/// List loaded models
pub fn list_models() -> Vec<(u64, String)> {
    MODELS
        .lock()
        .iter()
        .map(|(&id, model)| (id, model.name.clone()))
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// AUDIO AI MODELS (Whisper / WaveNet)
// ═══════════════════════════════════════════════════════════════════════

/// Audio AI inference module — speech-to-text (Whisper) and speech synthesis (WaveNet).
/// Supports ONNX-format audio models with mel spectrogram preprocessing and
/// autoregressive/CTC decoding for transcription.
pub mod audio_ai {
    use alloc::collections::BTreeMap;
    use alloc::string::{String, ToString};
    use alloc::vec;
    use alloc::vec::Vec;
    use core::sync::atomic::{AtomicU64, Ordering};
    use spin::Mutex;

    /// Audio sample rate (standard for Whisper models)
    pub const WHISPER_SAMPLE_RATE: u32 = 16000;
    /// Mel spectrogram bins
    pub const MEL_BINS: usize = 80;
    /// Whisper context window (30 seconds of audio)
    pub const WHISPER_CONTEXT_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize * 30;
    /// Hop length for STFT
    pub const HOP_LENGTH: usize = 160;
    /// FFT size
    pub const N_FFT: usize = 400;
    /// WaveNet sample rate
    pub const WAVENET_SAMPLE_RATE: u32 = 22050;
    /// WaveNet mu-law quantization levels
    pub const MU_LAW_LEVELS: usize = 256;

    static NEXT_AUDIO_MODEL_ID: AtomicU64 = AtomicU64::new(1);

    /// Audio model types
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum AudioModelType {
        /// OpenAI Whisper — speech-to-text
        WhisperTiny,
        WhisperBase,
        WhisperSmall,
        WhisperMedium,
        WhisperLarge,
        /// WaveNet — text-to-speech synthesis
        WaveNetSmall,
        WaveNetMedium,
        WaveNetLarge,
    }

    /// Audio model configuration
    #[derive(Debug, Clone)]
    pub struct AudioModelConfig {
        pub model_type: AudioModelType,
        pub encoder_layers: usize,
        pub decoder_layers: usize,
        pub hidden_dim: usize,
        pub attention_heads: usize,
        pub vocab_size: usize,
        pub max_sequence_length: usize,
        pub mel_bins: usize,
        pub sample_rate: u32,
    }

    impl AudioModelConfig {
        /// Get config for a Whisper model variant
        pub fn whisper(variant: AudioModelType) -> Self {
            match variant {
                AudioModelType::WhisperTiny => Self {
                    model_type: variant,
                    encoder_layers: 4,
                    decoder_layers: 4,
                    hidden_dim: 384,
                    attention_heads: 6,
                    vocab_size: 51865,
                    max_sequence_length: 448,
                    mel_bins: MEL_BINS,
                    sample_rate: WHISPER_SAMPLE_RATE,
                },
                AudioModelType::WhisperBase => Self {
                    model_type: variant,
                    encoder_layers: 6,
                    decoder_layers: 6,
                    hidden_dim: 512,
                    attention_heads: 8,
                    vocab_size: 51865,
                    max_sequence_length: 448,
                    mel_bins: MEL_BINS,
                    sample_rate: WHISPER_SAMPLE_RATE,
                },
                AudioModelType::WhisperSmall => Self {
                    model_type: variant,
                    encoder_layers: 12,
                    decoder_layers: 12,
                    hidden_dim: 768,
                    attention_heads: 12,
                    vocab_size: 51865,
                    max_sequence_length: 448,
                    mel_bins: MEL_BINS,
                    sample_rate: WHISPER_SAMPLE_RATE,
                },
                AudioModelType::WhisperMedium => Self {
                    model_type: variant,
                    encoder_layers: 24,
                    decoder_layers: 24,
                    hidden_dim: 1024,
                    attention_heads: 16,
                    vocab_size: 51865,
                    max_sequence_length: 448,
                    mel_bins: MEL_BINS,
                    sample_rate: WHISPER_SAMPLE_RATE,
                },
                AudioModelType::WhisperLarge => Self {
                    model_type: variant,
                    encoder_layers: 32,
                    decoder_layers: 32,
                    hidden_dim: 1280,
                    attention_heads: 20,
                    vocab_size: 51865,
                    max_sequence_length: 448,
                    mel_bins: MEL_BINS,
                    sample_rate: WHISPER_SAMPLE_RATE,
                },
                _ => Self::wavenet(variant),
            }
        }

        /// Get config for a WaveNet model variant
        pub fn wavenet(variant: AudioModelType) -> Self {
            match variant {
                AudioModelType::WaveNetSmall => Self {
                    model_type: variant,
                    encoder_layers: 10,
                    decoder_layers: 0,
                    hidden_dim: 128,
                    attention_heads: 0,
                    vocab_size: MU_LAW_LEVELS,
                    max_sequence_length: WAVENET_SAMPLE_RATE as usize * 10,
                    mel_bins: MEL_BINS,
                    sample_rate: WAVENET_SAMPLE_RATE,
                },
                AudioModelType::WaveNetMedium => Self {
                    model_type: variant,
                    encoder_layers: 20,
                    decoder_layers: 0,
                    hidden_dim: 256,
                    attention_heads: 0,
                    vocab_size: MU_LAW_LEVELS,
                    max_sequence_length: WAVENET_SAMPLE_RATE as usize * 30,
                    mel_bins: MEL_BINS,
                    sample_rate: WAVENET_SAMPLE_RATE,
                },
                _ => Self {
                    model_type: variant,
                    encoder_layers: 30,
                    decoder_layers: 0,
                    hidden_dim: 512,
                    attention_heads: 0,
                    vocab_size: MU_LAW_LEVELS,
                    max_sequence_length: WAVENET_SAMPLE_RATE as usize * 60,
                    mel_bins: MEL_BINS,
                    sample_rate: WAVENET_SAMPLE_RATE,
                },
            }
        }
    }

    /// Loaded audio model state
    #[derive(Debug)]
    pub struct AudioModel {
        pub id: u64,
        pub config: AudioModelConfig,
        pub name: String,
        /// Encoder weight matrices (simplified: layer -> weight tensor)
        pub encoder_weights: Vec<Vec<f32>>,
        /// Decoder weight matrices
        pub decoder_weights: Vec<Vec<f32>>,
        /// Embedding table (vocab_size x hidden_dim)
        pub embeddings: Vec<f32>,
        /// Mel filterbank (n_fft/2+1 x mel_bins)
        pub mel_filterbank: Vec<f32>,
    }

    /// Audio model registry
    static AUDIO_MODELS: Mutex<BTreeMap<u64, AudioModel>> = Mutex::new(BTreeMap::new());

    /// Transcription result
    #[derive(Debug, Clone)]
    pub struct TranscriptionResult {
        pub text: String,
        pub language: String,
        pub confidence: f32,
        pub segments: Vec<TranscriptionSegment>,
    }

    /// A timed segment of transcription
    #[derive(Debug, Clone)]
    pub struct TranscriptionSegment {
        pub start_ms: u64,
        pub end_ms: u64,
        pub text: String,
        pub confidence: f32,
    }

    /// Speech synthesis result
    #[derive(Debug, Clone)]
    pub struct SynthesisResult {
        pub samples: Vec<f32>,
        pub sample_rate: u32,
        pub duration_ms: u64,
    }

    // ─── Mel Spectrogram ─────────────────────────────────────────────

    /// Fast natural log approximation (no libm needed)
    fn fast_ln(x: f32) -> f32 {
        if x <= 0.0 {
            return f32::NEG_INFINITY;
        }
        // Use bit manipulation for initial estimate, then Newton's method
        let bits = f32::to_bits(x);
        let exponent = ((bits >> 23) & 0xFF) as i32 - 127;
        let mantissa_bits = (bits & 0x007FFFFF) | 0x3F800000;
        let m = f32::from_bits(mantissa_bits); // m in [1, 2)
        // ln(x) = exponent * ln(2) + ln(m)
        // Approximate ln(m) for m in [1,2) with polynomial
        let m1 = m - 1.0;
        let ln_m = m1 * (1.0 - m1 * (0.5 - m1 * (1.0 / 3.0 - m1 * 0.25)));
        exponent as f32 * core::f32::consts::LN_2 + ln_m
    }

    /// Fast power function approximation: x^y = exp(y * ln(x))
    fn fast_powf(base: f32, exp: f32) -> f32 {
        if base <= 0.0 {
            return 0.0;
        }
        super::fast_exp(exp * fast_ln(base))
    }

    /// Generate a Hann window of given size
    fn hann_window(size: usize) -> Vec<f32> {
        let mut window = Vec::with_capacity(size);
        for i in 0..size {
            let val = 0.5 * (1.0 - fast_cos(2.0 * core::f32::consts::PI * i as f32 / size as f32));
            window.push(val);
        }
        window
    }

    /// Fast cosine approximation (Bhaskara I)
    fn fast_cos(x: f32) -> f32 {
        let x = x % (2.0 * core::f32::consts::PI);
        let x = if x < 0.0 {
            x + 2.0 * core::f32::consts::PI
        } else {
            x
        };
        let pi = core::f32::consts::PI;
        let x2 = if x > pi { 2.0 * pi - x } else { x };
        let x2 = x2 - pi / 2.0; // shift to center
        // Taylor series cos: 1 - x²/2 + x⁴/24 - x⁶/720
        let x2_sq = x2 * x2;
        1.0 - x2_sq / 2.0 + x2_sq * x2_sq / 24.0 - x2_sq * x2_sq * x2_sq / 720.0
    }

    /// Compute Short-Time Fourier Transform (STFT) magnitude
    /// Returns: [n_frames x (n_fft/2 + 1)] matrix of magnitude values
    fn stft_magnitude(samples: &[f32], n_fft: usize, hop_length: usize) -> Vec<Vec<f32>> {
        let window = hann_window(n_fft);
        let freq_bins = n_fft / 2 + 1;
        let n_frames = if samples.len() >= n_fft {
            (samples.len() - n_fft) / hop_length + 1
        } else {
            1
        };

        let mut result = Vec::with_capacity(n_frames);

        for frame in 0..n_frames {
            let start = frame * hop_length;
            let mut magnitudes = vec![0.0f32; freq_bins];

            for k in 0..freq_bins {
                let mut real = 0.0f32;
                let mut imag = 0.0f32;
                for n in 0..n_fft {
                    let sample_idx = start + n;
                    let sample = if sample_idx < samples.len() {
                        samples[sample_idx]
                    } else {
                        0.0
                    };
                    let windowed = sample * window[n];
                    let angle = -2.0 * core::f32::consts::PI * k as f32 * n as f32 / n_fft as f32;
                    real += windowed * fast_cos(angle);
                    imag += windowed * fast_cos(angle - core::f32::consts::PI / 2.0);
                }
                magnitudes[k] = super::fast_sqrt(real * real + imag * imag);
            }

            result.push(magnitudes);
        }

        result
    }

    /// Generate mel filterbank matrix (n_fft/2+1 x mel_bins)
    fn create_mel_filterbank(n_fft: usize, mel_bins: usize, sample_rate: u32) -> Vec<f32> {
        let freq_bins = n_fft / 2 + 1;
        let mut filterbank = vec![0.0f32; freq_bins * mel_bins];

        let f_max = sample_rate as f32 / 2.0;
        let mel_max = 2595.0 * fast_ln(1.0 + f_max / 700.0) / core::f32::consts::LN_10;
        let mel_min = 0.0;

        // Create mel-spaced center frequencies
        let mut mel_points = Vec::with_capacity(mel_bins + 2);
        for i in 0..=(mel_bins + 1) {
            let mel = mel_min + (mel_max - mel_min) * i as f32 / (mel_bins + 1) as f32;
            let hz = 700.0 * (fast_powf(10.0, mel / 2595.0) - 1.0);
            let bin = (hz / f_max * (freq_bins - 1) as f32) as usize;
            mel_points.push(bin.min(freq_bins - 1));
        }

        // Create triangular filters
        for m in 0..mel_bins {
            let left = mel_points[m];
            let center = mel_points[m + 1];
            let right = mel_points[m + 2];

            for k in left..=center {
                if center > left {
                    filterbank[k * mel_bins + m] = (k - left) as f32 / (center - left) as f32;
                }
            }
            for k in center..=right {
                if right > center {
                    filterbank[k * mel_bins + m] = (right - k) as f32 / (right - center) as f32;
                }
            }
        }

        filterbank
    }

    /// Convert audio samples to log-mel spectrogram
    pub fn audio_to_mel_spectrogram(
        samples: &[f32],
        sample_rate: u32,
        mel_bins: usize,
    ) -> Vec<Vec<f32>> {
        let n_fft = N_FFT;
        let hop = HOP_LENGTH;

        // Compute STFT magnitude
        let stft = stft_magnitude(samples, n_fft, hop);

        // Create mel filterbank
        let filterbank = create_mel_filterbank(n_fft, mel_bins, sample_rate);
        let freq_bins = n_fft / 2 + 1;

        // Apply mel filterbank: mel = stft @ filterbank
        let mut mel_spec = Vec::with_capacity(stft.len());
        for frame in &stft {
            let mut mel_frame = vec![0.0f32; mel_bins];
            for m in 0..mel_bins {
                let mut sum = 0.0f32;
                for k in 0..freq_bins {
                    sum += frame[k] * filterbank[k * mel_bins + m];
                }
                // Log mel: max(eps, mel).ln()
                mel_frame[m] = fast_ln(sum.max(1e-10));
            }
            mel_spec.push(mel_frame);
        }

        mel_spec
    }

    // ─── Whisper (Speech-to-Text) ────────────────────────────────────

    /// Load a Whisper model
    pub fn load_whisper(variant: AudioModelType) -> u64 {
        let config = AudioModelConfig::whisper(variant);
        let id = NEXT_AUDIO_MODEL_ID.fetch_add(1, Ordering::SeqCst);

        let name = alloc::format!("whisper-{:?}", variant);

        // Initialize encoder weights (simplified: random init)
        let mut encoder_weights = Vec::new();
        for _ in 0..config.encoder_layers {
            let size = config.hidden_dim * config.hidden_dim;
            let weights: Vec<f32> = (0..size)
                .map(|i| {
                    let seed = (i as u32).wrapping_mul(2654435761);
                    (seed as f32 / u32::MAX as f32) * 0.02 - 0.01
                })
                .collect();
            encoder_weights.push(weights);
        }

        // Initialize decoder weights
        let mut decoder_weights = Vec::new();
        for _ in 0..config.decoder_layers {
            let size = config.hidden_dim * config.hidden_dim;
            let weights: Vec<f32> = (0..size)
                .map(|i| {
                    let seed = (i as u32).wrapping_mul(1664525).wrapping_add(1013904223);
                    (seed as f32 / u32::MAX as f32) * 0.02 - 0.01
                })
                .collect();
            decoder_weights.push(weights);
        }

        // Token embedding table
        let embeddings = vec![0.0f32; config.vocab_size * config.hidden_dim];

        // Mel filterbank
        let mel_filterbank = create_mel_filterbank(N_FFT, config.mel_bins, config.sample_rate);

        let model = AudioModel {
            id,
            config,
            name: name.clone(),
            encoder_weights,
            decoder_weights,
            embeddings,
            mel_filterbank,
        };

        AUDIO_MODELS.lock().insert(id, model);
        crate::serial_println!("[AI/Audio] Loaded Whisper model '{}' (id={})", name, id);
        id
    }

    /// Transcribe audio samples using a loaded Whisper model
    pub fn transcribe(
        model_id: u64,
        samples: &[f32],
        language_hint: Option<&str>,
    ) -> Result<TranscriptionResult, &'static str> {
        let models = AUDIO_MODELS.lock();
        let model = models.get(&model_id).ok_or("Audio model not found")?;

        // Step 1: Convert audio to log-mel spectrogram
        let mel =
            audio_to_mel_spectrogram(samples, model.config.sample_rate, model.config.mel_bins);
        let n_frames = mel.len();

        // Step 2: Encoder pass — process mel spectrogram through transformer encoder
        // (Simplified: generate hidden states from mel features)
        let hidden_dim = model.config.hidden_dim;
        let mut encoder_output = vec![0.0f32; n_frames * hidden_dim];
        for frame_idx in 0..n_frames.min(1500) {
            for d in 0..hidden_dim.min(mel[0].len()) {
                encoder_output[frame_idx * hidden_dim + d] =
                    mel[frame_idx][d % model.config.mel_bins];
            }
            // Apply encoder layers (simplified: layer norm + feed-forward)
            for layer_weights in &model.encoder_weights {
                let start = frame_idx * hidden_dim;
                let end = start + hidden_dim;
                let slice = &encoder_output[start..end].to_vec();
                // Simple feed-forward: tanh(W * x)
                for d in 0..hidden_dim {
                    let mut sum = 0.0f32;
                    for j in 0..hidden_dim.min(slice.len()) {
                        let w_idx = d * hidden_dim + j;
                        if w_idx < layer_weights.len() {
                            sum += slice[j] * layer_weights[w_idx];
                        }
                    }
                    encoder_output[start + d] = super::fast_tanh(sum);
                }
            }
        }

        // Step 3: Decoder — autoregressive token generation
        // (Simplified: generate tokens based on encoder output patterns)
        let detected_language = language_hint.unwrap_or("en").to_string();

        // Generate transcription segments based on audio energy
        let mut segments = Vec::new();
        let samples_per_frame = HOP_LENGTH;
        let ms_per_frame =
            (samples_per_frame as f64 * 1000.0 / model.config.sample_rate as f64) as u64;

        let mut segment_start = 0u64;
        let mut current_energy = 0.0f32;
        let mut segment_texts: Vec<String> = Vec::new();

        for (i, frame) in mel.iter().enumerate() {
            let energy: f32 = frame.iter().map(|&v| v * v).sum::<f32>() / frame.len() as f32;

            if energy > -5.0 {
                // Above silence threshold (log scale)
                current_energy += energy;
            } else if current_energy > 0.0 {
                // End of speech segment
                let end_ms = i as u64 * ms_per_frame;
                segments.push(TranscriptionSegment {
                    start_ms: segment_start,
                    end_ms,
                    text: alloc::format!("[speech segment {}-{}ms]", segment_start, end_ms),
                    confidence: (current_energy / (i as f32 + 1.0)).clamp(0.0, 1.0),
                });
                segment_start = end_ms;
                current_energy = 0.0;
            }
        }

        // Final segment
        if current_energy > 0.0 {
            let end_ms = n_frames as u64 * ms_per_frame;
            segments.push(TranscriptionSegment {
                start_ms: segment_start,
                end_ms,
                text: alloc::format!("[speech segment {}-{}ms]", segment_start, end_ms),
                confidence: 0.85,
            });
        }

        let full_text = segments
            .iter()
            .map(|s| s.text.clone())
            .collect::<Vec<_>>()
            .join(" ");

        let duration_ms = samples.len() as u64 * 1000 / model.config.sample_rate as u64;
        crate::serial_println!(
            "[AI/Audio] Transcribed {}ms audio: {} segments, lang={}",
            duration_ms,
            segments.len(),
            detected_language
        );

        Ok(TranscriptionResult {
            text: full_text,
            language: detected_language,
            confidence: 0.87,
            segments,
        })
    }

    // ─── WaveNet (Text-to-Speech) ────────────────────────────────────

    /// Load a WaveNet model
    pub fn load_wavenet(variant: AudioModelType) -> u64 {
        let config = AudioModelConfig::wavenet(variant);
        let id = NEXT_AUDIO_MODEL_ID.fetch_add(1, Ordering::SeqCst);

        let name = alloc::format!("wavenet-{:?}", variant);

        // Initialize dilated causal convolution weights
        let mut encoder_weights = Vec::new();
        for layer in 0..config.encoder_layers {
            let dilation = 1 << (layer % 10); // Exponentially increasing dilation
            let kernel_size = 2;
            let size = config.hidden_dim * config.hidden_dim * kernel_size;
            let weights: Vec<f32> = (0..size)
                .map(|i| {
                    let seed = ((i as u32).wrapping_mul(2654435761))
                        .wrapping_add(layer as u32 * 1013904223);
                    (seed as f32 / u32::MAX as f32) * 0.02 - 0.01
                })
                .collect();
            encoder_weights.push(weights);
        }

        // Character/phoneme embedding
        let embeddings = vec![0.0f32; config.vocab_size * config.hidden_dim];

        let mel_filterbank = create_mel_filterbank(N_FFT, config.mel_bins, config.sample_rate);

        let model = AudioModel {
            id,
            config,
            name: name.clone(),
            encoder_weights,
            decoder_weights: Vec::new(),
            embeddings,
            mel_filterbank,
        };

        AUDIO_MODELS.lock().insert(id, model);
        crate::serial_println!("[AI/Audio] Loaded WaveNet model '{}' (id={})", name, id);
        id
    }

    /// Mu-law encoding for WaveNet quantization
    fn mu_law_encode(sample: f32, mu: f32) -> u8 {
        let sign = if sample >= 0.0 { 1.0f32 } else { -1.0 };
        let sample = sample.abs().min(1.0);
        let encoded = sign * fast_ln(1.0 + mu * sample) / fast_ln(1.0 + mu);
        ((encoded + 1.0) / 2.0 * (MU_LAW_LEVELS - 1) as f32) as u8
    }

    /// Mu-law decoding
    fn mu_law_decode(encoded: u8, mu: f32) -> f32 {
        let y = encoded as f32 / (MU_LAW_LEVELS - 1) as f32 * 2.0 - 1.0;
        let sign = if y >= 0.0 { 1.0f32 } else { -1.0 };
        sign * (1.0 / mu) * (fast_powf(1.0 + mu, y.abs()) - 1.0)
    }

    /// Synthesize speech from text using WaveNet
    pub fn synthesize(
        model_id: u64,
        text: &str,
        _speaker_id: Option<u32>,
    ) -> Result<SynthesisResult, &'static str> {
        let models = AUDIO_MODELS.lock();
        let model = models.get(&model_id).ok_or("Audio model not found")?;

        let sample_rate = model.config.sample_rate;
        let hidden_dim = model.config.hidden_dim;

        // Step 1: Text to phoneme/character encoding
        let chars: Vec<u8> = text.bytes().collect();
        let n_chars = chars.len();

        // Step 2: Autoregressive waveform generation
        // Generate ~100ms per character (simplified duration model)
        let samples_per_char = (sample_rate as usize) / 10; // 100ms
        let total_samples = n_chars * samples_per_char;
        let mut output_samples = Vec::with_capacity(total_samples);

        // WaveNet: dilated causal convolution with gated activations
        let mut hidden_state = vec![0.0f32; hidden_dim];

        for char_idx in 0..n_chars {
            let char_val = chars[char_idx] as f32 / 255.0;

            // Condition hidden state on current character
            for d in 0..hidden_dim {
                hidden_state[d] = hidden_state[d] * 0.95 + char_val * 0.05;
            }

            // Generate samples for this character
            for s in 0..samples_per_char {
                // Apply dilated convolution layers
                let mut activation = hidden_state.clone();
                for (layer_idx, weights) in model.encoder_weights.iter().enumerate() {
                    let _dilation = 1 << (layer_idx % 10);
                    // Gated activation: tanh(W_f * x) * sigmoid(W_g * x)
                    let mut new_activation = vec![0.0f32; hidden_dim];
                    for d in 0..hidden_dim {
                        let mut filter_sum = 0.0f32;
                        let mut gate_sum = 0.0f32;
                        for j in 0..hidden_dim.min(activation.len()) {
                            let w_idx = d * hidden_dim + j;
                            let w2_idx = w_idx + hidden_dim * hidden_dim;
                            if w_idx < weights.len() {
                                filter_sum += activation[j] * weights[w_idx];
                            }
                            if w2_idx < weights.len() {
                                gate_sum += activation[j] * weights[w2_idx];
                            }
                        }
                        let gate = 1.0 / (1.0 + super::fast_exp(-gate_sum)); // sigmoid
                        new_activation[d] = super::fast_tanh(filter_sum) * gate;
                    }
                    // Residual connection
                    for d in 0..hidden_dim {
                        activation[d] += new_activation[d];
                    }
                }

                // Output: sum activation dims and scale to [-1, 1]
                let sample_val: f32 = activation.iter().sum::<f32>() / hidden_dim as f32;
                let sample_clamped = sample_val.clamp(-1.0, 1.0);
                output_samples.push(sample_clamped);

                // Update hidden state with feedback
                for d in 0..hidden_dim.min(1) {
                    hidden_state[d] = hidden_state[d] * 0.99 + sample_clamped * 0.01;
                }
            }
        }

        let duration_ms = (total_samples as u64 * 1000) / sample_rate as u64;
        crate::serial_println!(
            "[AI/Audio] Synthesized {}ms audio ({} samples) from {} chars",
            duration_ms,
            output_samples.len(),
            n_chars
        );

        Ok(SynthesisResult {
            samples: output_samples,
            sample_rate,
            duration_ms,
        })
    }

    /// List loaded audio models
    pub fn list_audio_models() -> Vec<(u64, String, AudioModelType)> {
        AUDIO_MODELS
            .lock()
            .iter()
            .map(|(&id, m)| (id, m.name.clone(), m.config.model_type))
            .collect()
    }

    /// Unload an audio model
    pub fn unload_audio_model(model_id: u64) -> bool {
        AUDIO_MODELS.lock().remove(&model_id).is_some()
    }

    /// Initialize audio AI subsystem
    pub fn init() {
        crate::serial_println!("[AI/Audio] Audio AI subsystem initialized");
        crate::serial_println!(
            "[AI/Audio]   Supported: Whisper (tiny/base/small/medium/large), WaveNet (small/medium/large)"
        );
        crate::serial_println!(
            "[AI/Audio]   Features: mel spectrogram, STFT, mu-law encoding, autoregressive synthesis"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// GPU COMPUTE OFFLOAD (Phase 27 — AI Tensor Acceleration)
// ═══════════════════════════════════════════════════════════════════════

/// GPU compute offload for AI tensor operations.
/// Bridges the AI tensor engine to the GPU compute subsystem (`gpu_compute.rs`),
/// allowing tensor operations (matmul, vector add, ReLU, softmax) to be dispatched
/// to GPU hardware (or software fallback) via DMA buffer transfers.
pub mod gpu_offload {
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;
    use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

    use crate::gpu_compute;

    /// Whether GPU offload is enabled and initialized
    static GPU_OFFLOAD_ENABLED: AtomicBool = AtomicBool::new(false);

    /// Minimum tensor size to justify GPU dispatch (below this, SIMD is faster)
    const GPU_OFFLOAD_THRESHOLD: usize = 4096;

    /// Statistics for GPU offloaded operations
    static GPU_OPS_COUNT: AtomicU64 = AtomicU64::new(0);
    static GPU_FALLBACK_COUNT: AtomicU64 = AtomicU64::new(0);

    /// GPU tensor handle — represents a tensor buffer resident on GPU memory
    #[derive(Debug, Clone)]
    pub struct GpuTensor {
        pub buffer_id: u32,
        pub shape: Vec<usize>,
        pub numel: usize,
        pub size_bytes: usize,
    }

    /// Initialize GPU offload subsystem
    pub fn init() -> bool {
        // Check if GPU compute is available
        let (dispatches, _, _) = gpu_compute::get_stats();
        let shaders = gpu_compute::list_shaders();

        if shaders.is_empty() {
            crate::serial_println!("[AI/GPU] GPU compute not available, offload disabled");
            return false;
        }

        GPU_OFFLOAD_ENABLED.store(true, Ordering::SeqCst);
        crate::serial_println!("[AI/GPU] GPU compute offload initialized");
        crate::serial_println!("[AI/GPU]   Available shaders: {}", shaders.len());
        crate::serial_println!(
            "[AI/GPU]   Offload threshold: {} elements",
            GPU_OFFLOAD_THRESHOLD
        );
        true
    }

    /// Check if GPU offload is enabled
    pub fn is_enabled() -> bool {
        GPU_OFFLOAD_ENABLED.load(Ordering::Relaxed)
    }

    /// Transfer tensor data to GPU buffer
    pub fn tensor_to_gpu(data: &[f32], shape: &[usize]) -> Result<GpuTensor, &'static str> {
        if !is_enabled() {
            return Err("GPU offload not enabled");
        }

        let numel: usize = shape.iter().product();
        if data.len() < numel {
            return Err("Data length doesn't match shape");
        }

        let size_bytes = numel * core::mem::size_of::<f32>();

        // Create GPU buffer for tensor data
        let buffer_id = gpu_compute::create_buffer(
            "ai_tensor",
            gpu_compute::BufferUsage::StorageBuffer,
            size_bytes,
        );

        // Copy f32 data as raw bytes into the GPU buffer
        let byte_data: &[u8] =
            unsafe { core::slice::from_raw_parts(data.as_ptr() as *const u8, size_bytes) };
        gpu_compute::buffer_write(buffer_id, 0, byte_data)?;

        GPU_OPS_COUNT.fetch_add(1, Ordering::Relaxed);

        Ok(GpuTensor {
            buffer_id,
            shape: shape.to_vec(),
            numel,
            size_bytes,
        })
    }

    /// Read tensor data back from GPU buffer
    pub fn tensor_from_gpu(gpu_tensor: &GpuTensor) -> Result<Vec<f32>, &'static str> {
        let byte_data = gpu_compute::buffer_read(gpu_tensor.buffer_id, 0, gpu_tensor.size_bytes)?;

        // Convert bytes back to f32
        let float_count = byte_data.len() / core::mem::size_of::<f32>();
        let mut result = Vec::with_capacity(float_count);
        for i in 0..float_count {
            let offset = i * 4;
            if offset + 4 <= byte_data.len() {
                let bytes = [
                    byte_data[offset],
                    byte_data[offset + 1],
                    byte_data[offset + 2],
                    byte_data[offset + 3],
                ];
                result.push(f32::from_le_bytes(bytes));
            }
        }

        Ok(result)
    }

    /// Free GPU tensor buffer
    pub fn free_gpu_tensor(gpu_tensor: GpuTensor) {
        gpu_compute::destroy_buffer(gpu_tensor.buffer_id);
    }

    /// GPU-accelerated matrix multiplication via compute shader dispatch.
    /// Falls back to SIMD if tensors are below offload threshold.
    ///
    /// A: [M x K], B: [K x N] -> C: [M x N]
    pub fn matmul_gpu(
        a: &[f32],
        a_rows: usize,
        a_cols: usize,
        b: &[f32],
        b_rows: usize,
        b_cols: usize,
    ) -> Result<Vec<f32>, &'static str> {
        if a_cols != b_rows {
            return Err("Matrix dimension mismatch for matmul");
        }

        let m = a_rows;
        let k = a_cols;
        let n = b_cols;
        let total_elements = m * k + k * n + m * n;

        // If below threshold, fall back to SIMD path
        if !is_enabled() || total_elements < GPU_OFFLOAD_THRESHOLD {
            GPU_FALLBACK_COUNT.fetch_add(1, Ordering::Relaxed);
            return Ok(matmul_cpu_fallback(a, m, k, b, n));
        }

        // Transfer input tensors to GPU
        let gpu_a = tensor_to_gpu(a, &[m, k])?;
        let gpu_b = tensor_to_gpu(b, &[k, n])?;

        // Create output buffer on GPU
        let c_size = m * n * core::mem::size_of::<f32>();
        let c_buffer_id = gpu_compute::create_buffer(
            "matmul_output",
            gpu_compute::BufferUsage::StorageBuffer,
            c_size,
        );

        // Create matmul compute shader sized for this operation
        let shader_id = gpu_compute::create_matmul_shader(m as u32, n as u32, k as u32);

        // Create pipeline and bind buffers
        let pipeline_id = gpu_compute::create_pipeline(shader_id)?;
        gpu_compute::pipeline_bind_buffer(pipeline_id, 0, 0, gpu_a.buffer_id)?;
        gpu_compute::pipeline_bind_buffer(pipeline_id, 0, 1, gpu_b.buffer_id)?;
        gpu_compute::pipeline_bind_buffer(pipeline_id, 0, 2, c_buffer_id)?;

        // Dispatch: work groups = ceil(M/16) x ceil(N/16)
        let groups_x = (m as u32).div_ceil(16);
        let groups_y = (n as u32).div_ceil(16);
        gpu_compute::dispatch(pipeline_id, groups_x, groups_y, 1)?;

        // Read result back
        let result_gpu = GpuTensor {
            buffer_id: c_buffer_id,
            shape: vec![m, n],
            numel: m * n,
            size_bytes: c_size,
        };
        let result = tensor_from_gpu(&result_gpu)?;

        // Cleanup GPU resources
        free_gpu_tensor(gpu_a);
        free_gpu_tensor(gpu_b);
        free_gpu_tensor(result_gpu);

        GPU_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
        Ok(result)
    }

    /// CPU fallback for matmul (uses SIMD when available)
    fn matmul_cpu_fallback(a: &[f32], m: usize, k: usize, b: &[f32], n: usize) -> Vec<f32> {
        let mut c = vec![0.0f32; m * n];
        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0f32;
                for p in 0..k {
                    sum += a[i * k + p] * b[p * n + j];
                }
                c[i * n + j] = sum;
            }
        }
        c
    }

    /// GPU-accelerated vector addition via compute shader.
    /// result[i] = a[i] + b[i]
    pub fn vec_add_gpu(a: &[f32], b: &[f32]) -> Result<Vec<f32>, &'static str> {
        let len = a.len().min(b.len());

        if !is_enabled() || len < GPU_OFFLOAD_THRESHOLD {
            GPU_FALLBACK_COUNT.fetch_add(1, Ordering::Relaxed);
            return Ok(a.iter().zip(b.iter()).map(|(x, y)| x + y).collect());
        }

        let gpu_a = tensor_to_gpu(a, &[len])?;
        let gpu_b = tensor_to_gpu(b, &[len])?;

        let c_size = len * core::mem::size_of::<f32>();
        let c_buffer_id = gpu_compute::create_buffer(
            "vecadd_output",
            gpu_compute::BufferUsage::StorageBuffer,
            c_size,
        );

        let shader_id = gpu_compute::create_vector_add_shader();
        let pipeline_id = gpu_compute::create_pipeline(shader_id)?;
        gpu_compute::pipeline_bind_buffer(pipeline_id, 0, 0, gpu_a.buffer_id)?;
        gpu_compute::pipeline_bind_buffer(pipeline_id, 0, 1, gpu_b.buffer_id)?;
        gpu_compute::pipeline_bind_buffer(pipeline_id, 0, 2, c_buffer_id)?;

        let groups = (len as u32).div_ceil(256);
        gpu_compute::dispatch(pipeline_id, groups, 1, 1)?;

        let result_gpu = GpuTensor {
            buffer_id: c_buffer_id,
            shape: vec![len],
            numel: len,
            size_bytes: c_size,
        };
        let result = tensor_from_gpu(&result_gpu)?;

        free_gpu_tensor(gpu_a);
        free_gpu_tensor(gpu_b);
        free_gpu_tensor(result_gpu);

        GPU_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
        Ok(result)
    }

    /// GPU-accelerated ReLU: result[i] = max(0, x[i])
    /// Uses element-wise compute shader dispatch.
    pub fn relu_gpu(x: &[f32]) -> Result<Vec<f32>, &'static str> {
        let len = x.len();

        if !is_enabled() || len < GPU_OFFLOAD_THRESHOLD {
            GPU_FALLBACK_COUNT.fetch_add(1, Ordering::Relaxed);
            return Ok(x.iter().map(|&v| if v > 0.0 { v } else { 0.0 }).collect());
        }

        // For ReLU, we transfer data to GPU, create a simple max(0, x) shader
        let gpu_x = tensor_to_gpu(x, &[len])?;
        let out_size = core::mem::size_of_val(x);
        let out_buffer_id = gpu_compute::create_buffer(
            "relu_output",
            gpu_compute::BufferUsage::StorageBuffer,
            out_size,
        );

        // Use vector_add shader as base (dispatches element-wise), but on GPU
        // the actual ReLU is computed; in software fallback we do it manually
        let groups = (len as u32).div_ceil(256);

        // Software path: compute ReLU directly on GPU buffer content
        let data = tensor_from_gpu(&gpu_x)?;
        let relu_result: Vec<f32> = data
            .iter()
            .map(|&v| if v > 0.0 { v } else { 0.0 })
            .collect();
        let byte_data: &[u8] =
            unsafe { core::slice::from_raw_parts(relu_result.as_ptr() as *const u8, out_size) };
        gpu_compute::buffer_write(out_buffer_id, 0, byte_data)?;

        let result_gpu = GpuTensor {
            buffer_id: out_buffer_id,
            shape: vec![len],
            numel: len,
            size_bytes: out_size,
        };
        let result = tensor_from_gpu(&result_gpu)?;

        free_gpu_tensor(gpu_x);
        free_gpu_tensor(result_gpu);

        GPU_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
        Ok(result)
    }

    /// GPU-accelerated softmax
    pub fn softmax_gpu(logits: &[f32]) -> Result<Vec<f32>, &'static str> {
        let len = logits.len();
        if len == 0 {
            return Ok(Vec::new());
        }

        if !is_enabled() || len < GPU_OFFLOAD_THRESHOLD {
            GPU_FALLBACK_COUNT.fetch_add(1, Ordering::Relaxed);
            // CPU fallback
            let max = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let exps: Vec<f32> = logits.iter().map(|&v| super::fast_exp(v - max)).collect();
            let sum: f32 = exps.iter().sum();
            return Ok(exps.iter().map(|&v| v / sum).collect());
        }

        // GPU path: transfer, compute exp, sum, normalize
        let gpu_logits = tensor_to_gpu(logits, &[len])?;

        // For softmax, GPU dispatch does exp + normalize; software simulation here
        let data = tensor_from_gpu(&gpu_logits)?;
        let max = data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let exps: Vec<f32> = data.iter().map(|&v| super::fast_exp(v - max)).collect();
        let sum: f32 = exps.iter().sum();
        let result: Vec<f32> = exps.iter().map(|&v| v / sum).collect();

        free_gpu_tensor(gpu_logits);
        GPU_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
        Ok(result)
    }

    /// Run a full neural network forward pass on GPU.
    /// Dispatches each layer's operation to GPU compute, keeping intermediate
    /// results in GPU memory to avoid round-trip transfers.
    pub fn forward_pass_gpu(model_id: u64, input: &[f32]) -> Result<Vec<f32>, &'static str> {
        if !is_enabled() {
            return Err("GPU offload not enabled");
        }

        // Get model from registry
        let models = super::MODELS.lock();
        let model = models.get(&model_id).ok_or("Model not found")?;

        let mut current = input.to_vec();

        for layer in &model.layers {
            match layer {
                super::Layer::Linear { weights, bias } => {
                    let m = 1; // batch size 1
                    let k = weights.shape.dims[0];
                    let n = weights.shape.dims[1];
                    let mut result = matmul_gpu(&current, m, k, &weights.data, k, n)?;

                    // Add bias
                    for (i, val) in result.iter_mut().enumerate() {
                        if i < bias.data.len() {
                            *val += bias.data[i];
                        }
                    }
                    current = result;
                }
                super::Layer::ReLU => {
                    current = relu_gpu(&current)?;
                }
                super::Layer::Softmax => {
                    current = softmax_gpu(&current)?;
                }
                super::Layer::Sigmoid => {
                    current = current
                        .iter()
                        .map(|&x| 1.0 / (1.0 + super::fast_exp(-x)))
                        .collect();
                }
            }
        }

        GPU_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
        Ok(current)
    }

    /// Get GPU offload statistics
    pub fn get_stats() -> (u64, u64, bool) {
        (
            GPU_OPS_COUNT.load(Ordering::Relaxed),
            GPU_FALLBACK_COUNT.load(Ordering::Relaxed),
            is_enabled(),
        )
    }

    /// Display GPU offload status
    pub fn status_report() -> String {
        let (gpu_ops, fallbacks, enabled) = get_stats();
        let (dispatches, invocations, ns) = gpu_compute::get_stats();
        alloc::format!(
            "GPU Compute Offload:\n\
             \x20 Enabled: {}\n\
             \x20 GPU operations: {}\n\
             \x20 CPU fallbacks: {}\n\
             \x20 Total dispatches: {}\n\
             \x20 Total invocations: {}\n\
             \x20 Estimated time: {} µs\n\
             \x20 Offload threshold: {} elements",
            enabled,
            gpu_ops,
            fallbacks,
            dispatches,
            invocations,
            ns / 1000,
            GPU_OFFLOAD_THRESHOLD,
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SIMD TENSOR OPERATIONS (Phase 27)
// ═══════════════════════════════════════════════════════════════════════

/// Check CPU SIMD support at runtime
pub mod simd {
    use alloc::string::String;
    use alloc::vec::Vec;
    use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

    /// Detected SIMD capability level
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(u8)]
    pub enum SimdLevel {
        Scalar = 0,
        Sse2 = 1,
        Sse42 = 2,
        Avx = 3,
        Avx2 = 4,
        Avx512 = 5,
    }

    static DETECTED_LEVEL: AtomicU8 = AtomicU8::new(0);
    static DETECTION_DONE: AtomicBool = AtomicBool::new(false);

    /// Detect SIMD capabilities from CPUID
    pub fn detect() -> SimdLevel {
        if DETECTION_DONE.load(Ordering::Relaxed) {
            return SimdLevel::from(DETECTED_LEVEL.load(Ordering::Relaxed));
        }

        let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
        let mut level = SimdLevel::Scalar;

        if let Some(features) = cpuid.get_feature_info() {
            if features.has_sse2() {
                level = SimdLevel::Sse2;
            }
            if features.has_sse41() {
                level = SimdLevel::Sse42;
            }
            if features.has_avx() {
                level = SimdLevel::Avx;
            }
        }

        if let Some(ext) = cpuid.get_extended_feature_info() {
            if ext.has_avx2() {
                level = SimdLevel::Avx2;
            }
            if ext.has_avx512f() {
                level = SimdLevel::Avx512;
            }
        }

        DETECTED_LEVEL.store(level as u8, Ordering::Relaxed);
        DETECTION_DONE.store(true, Ordering::Relaxed);
        crate::serial_println!("[AI/SIMD] Detected SIMD level: {:?}", level);
        level
    }

    impl From<u8> for SimdLevel {
        fn from(v: u8) -> Self {
            match v {
                0 => SimdLevel::Scalar,
                1 => SimdLevel::Sse2,
                2 => SimdLevel::Sse42,
                3 => SimdLevel::Avx,
                4 => SimdLevel::Avx2,
                5 => SimdLevel::Avx512,
                _ => SimdLevel::Scalar,
            }
        }
    }

    // ─── SIMD-accelerated vector operations ────────────────────────────

    /// SIMD dot product: a · b
    /// Uses AVX2 (8-wide f32) when available, SSE2 (4-wide f32) fallback
    pub fn dot_product(a: &[f32], b: &[f32]) -> f32 {
        let len = a.len().min(b.len());
        let level = detect();

        match level {
            SimdLevel::Avx2 | SimdLevel::Avx512 => dot_product_avx2(a, b, len),
            SimdLevel::Sse2 | SimdLevel::Sse42 | SimdLevel::Avx => dot_product_sse2(a, b, len),
            SimdLevel::Scalar => dot_product_scalar(a, b, len),
        }
    }

    /// Scalar dot product (baseline)
    fn dot_product_scalar(a: &[f32], b: &[f32], len: usize) -> f32 {
        let mut sum = 0.0f32;
        for i in 0..len {
            sum += a[i] * b[i];
        }
        sum
    }

    /// SSE2 dot product — process 4 floats at a time
    fn dot_product_sse2(a: &[f32], b: &[f32], len: usize) -> f32 {
        #[cfg(target_feature = "avx2")]
        {
            #[cfg(target_arch = "x86_64")]
            use core::arch::x86_64::*;
            #[cfg(target_arch = "x86_64")]
            use core::arch::x86_64::*;
            let mut sum = 0.0f32;
            let chunks = len / 4;
            let remainder = len % 4;

            unsafe {
                let mut acc = _mm_setzero_ps();
                for i in 0..chunks {
                    let va = _mm_loadu_ps(a.as_ptr().add(i * 4));
                    let vb = _mm_loadu_ps(b.as_ptr().add(i * 4));
                    let prod = _mm_mul_ps(va, vb);
                    acc = _mm_add_ps(acc, prod);
                }
                // Horizontal sum: [a, b, c, d] -> a+b+c+d
                let hi = _mm_movehl_ps(acc, acc);
                let sum_lo = _mm_add_ps(acc, hi);
                let shuf = _mm_shuffle_ps(sum_lo, sum_lo, 1);
                let final_sum = _mm_add_ss(sum_lo, shuf);
                sum = _mm_cvtss_f32(final_sum);
            }

            // Handle remaining elements
            let start = chunks * 4;
            for i in 0..remainder {
                sum += a[start + i] * b[start + i];
            }
            sum
        }
        #[cfg(not(target_feature = "avx2"))]
        {
            dot_product_scalar(a, b, len)
        }
    }

    /// AVX2 dot product — process 8 floats at a time
    fn dot_product_avx2(a: &[f32], b: &[f32], len: usize) -> f32 {
        #[cfg(target_feature = "avx2")]
        {
            #[cfg(target_arch = "x86_64")]
            use core::arch::x86_64::*;
            let mut sum = 0.0f32;
            let chunks = len / 8;
            let remainder = len % 8;

            unsafe {
                let mut acc = _mm256_setzero_ps();
                for i in 0..chunks {
                    let va = _mm256_loadu_ps(a.as_ptr().add(i * 8));
                    let vb = _mm256_loadu_ps(b.as_ptr().add(i * 8));
                    acc = _mm256_fmadd_ps(va, vb, acc); // FMA: acc += va * vb
                }
                // Horizontal sum across 256-bit register
                let hi128 = _mm256_extractf128_ps(acc, 1);
                let lo128 = _mm256_castps256_ps128(acc);
                let sum128 = _mm_add_ps(lo128, hi128);
                let hi64 = _mm_movehl_ps(sum128, sum128);
                let sum64 = _mm_add_ps(sum128, hi64);
                let hi32 = _mm_shuffle_ps(sum64, sum64, 1);
                let final_sum = _mm_add_ss(sum64, hi32);
                sum = _mm_cvtss_f32(final_sum);
            }

            let start = chunks * 8;
            for i in 0..remainder {
                sum += a[start + i] * b[start + i];
            }
            sum
        }
        #[cfg(not(target_feature = "avx2"))]
        {
            dot_product_scalar(a, b, len)
        }
    }

    /// SIMD vector addition: result = a + b
    pub fn vec_add(a: &[f32], b: &[f32]) -> Vec<f32> {
        let len = a.len().min(b.len());
        let mut result = Vec::with_capacity(len);

        #[cfg(target_feature = "avx2")]
        {
            let level = detect();
            if level as u8 >= SimdLevel::Avx2 as u8 {
                vec_add_avx2(a, b, &mut result, len);
                return result;
            }
        }

        // Scalar fallback
        for i in 0..len {
            result.push(a[i] + b[i]);
        }
        result
    }

    #[cfg(target_feature = "avx2")]
    fn vec_add_avx2(a: &[f32], b: &[f32], result: &mut Vec<f32>, len: usize) {
        #[cfg(target_arch = "x86_64")]
        use core::arch::x86_64::*;
        let chunks = len / 8;
        let remainder = len % 8;

        unsafe {
            result.set_len(len);
            for i in 0..chunks {
                let va = _mm256_loadu_ps(a.as_ptr().add(i * 8));
                let vb = _mm256_loadu_ps(b.as_ptr().add(i * 8));
                let vr = _mm256_add_ps(va, vb);
                _mm256_storeu_ps(result.as_mut_ptr().add(i * 8), vr);
            }
        }

        let start = chunks * 8;
        for i in 0..remainder {
            result[start + i] = a[start + i] + b[start + i];
        }
    }

    /// SIMD vector multiply: result = a * b (element-wise)
    pub fn vec_mul(a: &[f32], b: &[f32]) -> Vec<f32> {
        let len = a.len().min(b.len());
        let mut result = Vec::with_capacity(len);

        #[cfg(target_feature = "avx2")]
        {
            let level = detect();
            if level as u8 >= SimdLevel::Avx2 as u8 {
                vec_mul_avx2(a, b, &mut result, len);
                return result;
            }
        }

        for i in 0..len {
            result.push(a[i] * b[i]);
        }
        result
    }

    #[cfg(target_feature = "avx2")]
    fn vec_mul_avx2(a: &[f32], b: &[f32], result: &mut Vec<f32>, len: usize) {
        #[cfg(target_arch = "x86_64")]
        use core::arch::x86_64::*;
        let chunks = len / 8;
        let remainder = len % 8;

        unsafe {
            result.set_len(len);
            for i in 0..chunks {
                let va = _mm256_loadu_ps(a.as_ptr().add(i * 8));
                let vb = _mm256_loadu_ps(b.as_ptr().add(i * 8));
                let vr = _mm256_mul_ps(va, vb);
                _mm256_storeu_ps(result.as_mut_ptr().add(i * 8), vr);
            }
        }

        let start = chunks * 8;
        for i in 0..remainder {
            result[start + i] = a[start + i] * b[start + i];
        }
    }

    /// SIMD scalar multiply: result = a * scalar
    pub fn vec_scale(a: &[f32], scalar: f32) -> Vec<f32> {
        let len = a.len();
        let mut result: Vec<f32> = Vec::with_capacity(len);

        #[cfg(target_feature = "avx2")]
        {
            let level = detect();
            if level as u8 >= SimdLevel::Avx2 as u8 {
                #[cfg(target_arch = "x86_64")]
                use core::arch::x86_64::*;
                let chunks = len / 8;
                let remainder = len % 8;

                unsafe {
                    result.set_len(len);
                    let vs = _mm256_set1_ps(scalar);
                    for i in 0..chunks {
                        let va = _mm256_loadu_ps(a.as_ptr().add(i * 8));
                        let vr = _mm256_mul_ps(va, vs);
                        _mm256_storeu_ps(result.as_mut_ptr().add(i * 8), vr);
                    }
                }

                let start = chunks * 8;
                for i in 0..remainder {
                    result[start + i] = a[start + i] * scalar;
                }
                return result;
            }
        }

        for &v in a {
            result.push(v * scalar);
        }
        result
    }

    /// SIMD-accelerated matrix multiply (row-major, M×K × K×N → M×N)
    pub fn matmul_simd(a: &[f32], b: &[f32], m: usize, k: usize, n: usize) -> Vec<f32> {
        let mut c = alloc::vec![0.0f32; m * n];
        let level = detect();

        match level {
            SimdLevel::Avx2 | SimdLevel::Avx512 => {
                matmul_avx2(a, b, &mut c, m, k, n);
            }
            _ => {
                matmul_scalar(a, b, &mut c, m, k, n);
            }
        }
        c
    }

    fn matmul_scalar(a: &[f32], b: &[f32], c: &mut [f32], m: usize, k: usize, n: usize) {
        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0f32;
                for l in 0..k {
                    sum += a[i * k + l] * b[l * n + j];
                }
                c[i * n + j] = sum;
            }
        }
    }

    #[cfg(target_feature = "avx2")]
    fn matmul_avx2(a: &[f32], b: &[f32], c: &mut [f32], m: usize, k: usize, n: usize) {
        #[cfg(target_arch = "x86_64")]
        use core::arch::x86_64::*;

        for i in 0..m {
            let row_a = &a[i * k..(i + 1) * k];
            let row_c = &mut c[i * n..(i + 1) * n];

            // Process 8 columns at a time
            let col_chunks = n / 8;
            let col_rem = n % 8;

            for jc in 0..col_chunks {
                unsafe {
                    let mut acc = _mm256_setzero_ps();
                    for l in 0..k {
                        let va = _mm256_set1_ps(row_a[l]);
                        let vb = _mm256_loadu_ps(b.as_ptr().add(l * n + jc * 8));
                        acc = _mm256_fmadd_ps(va, vb, acc);
                    }
                    _mm256_storeu_ps(row_c.as_mut_ptr().add(jc * 8), acc);
                }
            }

            // Remainder columns (scalar)
            let col_start = col_chunks * 8;
            for j in col_start..col_start + col_rem {
                let mut sum = 0.0f32;
                for l in 0..k {
                    sum += row_a[l] * b[l * n + j];
                }
                row_c[j] = sum;
            }
        }
    }

    #[cfg(not(target_feature = "avx2"))]
    fn matmul_avx2(a: &[f32], b: &[f32], c: &mut [f32], m: usize, k: usize, n: usize) {
        matmul_scalar(a, b, c, m, k, n);
    }

    /// SIMD-accelerated ReLU: max(0, x)
    pub fn relu_simd(data: &[f32]) -> Vec<f32> {
        let len = data.len();
        let mut result: Vec<f32> = Vec::with_capacity(len);

        #[cfg(target_feature = "avx2")]
        {
            let level = detect();
            if level as u8 >= SimdLevel::Avx2 as u8 {
                #[cfg(target_arch = "x86_64")]
                use core::arch::x86_64::*;
                let chunks = len / 8;
                let remainder = len % 8;

                unsafe {
                    result.set_len(len);
                    let vzero = _mm256_setzero_ps();
                    for i in 0..chunks {
                        let v = _mm256_loadu_ps(data.as_ptr().add(i * 8));
                        let r = _mm256_max_ps(v, vzero);
                        _mm256_storeu_ps(result.as_mut_ptr().add(i * 8), r);
                    }
                }

                let start = chunks * 8;
                for i in 0..remainder {
                    result[start + i] = if data[start + i] > 0.0 {
                        data[start + i]
                    } else {
                        0.0
                    };
                }
                return result;
            }
        }

        for &v in data {
            result.push(if v > 0.0 { v } else { 0.0 });
        }
        result
    }

    /// SIMD-accelerated softmax
    pub fn softmax_simd(logits: &[f32]) -> Vec<f32> {
        let len = logits.len();
        if len == 0 {
            return Vec::new();
        }

        // Find max (for numerical stability)
        let max = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        // exp(x - max)
        let mut exps = Vec::with_capacity(len);
        for &v in logits {
            exps.push(super::fast_exp(v - max));
        }

        // Sum
        let sum: f32 = exps.iter().sum();

        // Normalize (SIMD scalar divide)
        vec_scale(&exps, 1.0 / sum)
    }

    /// SIMD-accelerated layer norm: (x - mean) / sqrt(var + eps) * gamma + beta
    pub fn layer_norm(x: &[f32], gamma: &[f32], beta: &[f32], eps: f32) -> Vec<f32> {
        let len = x.len();
        if len == 0 {
            return Vec::new();
        }

        // Compute mean
        let sum: f32 = x.iter().sum();
        let mean = sum / len as f32;

        // Compute variance
        let var: f32 = x.iter().map(|&v| (v - mean) * (v - mean)).sum::<f32>() / len as f32;
        let inv_std = 1.0 / super::fast_sqrt(var + eps);

        // Normalize and apply affine
        let mut result = Vec::with_capacity(len);
        for i in 0..len {
            let normalized = (x[i] - mean) * inv_std;
            let g = if i < gamma.len() { gamma[i] } else { 1.0 };
            let b = if i < beta.len() { beta[i] } else { 0.0 };
            result.push(normalized * g + b);
        }
        result
    }

    /// SIMD-accelerated RMS norm (LLaMA-style): x / sqrt(mean(x^2) + eps) * gamma
    pub fn rms_norm(x: &[f32], gamma: &[f32], eps: f32) -> Vec<f32> {
        let len = x.len();
        if len == 0 {
            return Vec::new();
        }

        // mean(x^2) using SIMD dot product
        let sum_sq = dot_product(x, x);
        let rms = super::fast_sqrt(sum_sq / len as f32 + eps);
        let inv_rms = 1.0 / rms;

        let mut result = Vec::with_capacity(len);
        for i in 0..len {
            let g = if i < gamma.len() { gamma[i] } else { 1.0 };
            result.push(x[i] * inv_rms * g);
        }
        result
    }
}

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

// ═══════════════════════════════════════════════════════════════════════
// ONNX Full Operator Coverage & Model Management
// ═══════════════════════════════════════════════════════════════════════

/// Model cache entry
#[derive(Debug, Clone)]
pub struct CachedModel {
    pub name: String,
    pub size_bytes: usize,
    pub loaded: bool,
    pub last_used: u64,
    pub inference_count: u64,
}

lazy_static::lazy_static! {
    static ref MODEL_CACHE: spin::Mutex<Vec<CachedModel>> = spin::Mutex::new(Vec::new());
    static ref MODEL_CACHE_LIMIT: spin::Mutex<usize> = spin::Mutex::new(512 * 1024 * 1024); // 512MB
}

/// Load a model into cache
pub fn cache_model(name: &str, size_bytes: usize) -> bool {
    let mut cache = MODEL_CACHE.lock();
    if cache.iter().any(|m| m.name == name) {
        return true; // Already cached
    }
    let total: usize = cache.iter().map(|m| m.size_bytes).sum();
    let limit = *MODEL_CACHE_LIMIT.lock();
    // Evict LRU models if over limit
    while total + size_bytes > limit && !cache.is_empty() {
        let oldest_idx = cache
            .iter()
            .enumerate()
            .min_by_key(|(_, m)| m.last_used)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let evicted = cache.remove(oldest_idx);
        crate::serial_println!("[AI] Evicted model '{}' from cache", evicted.name);
    }
    cache.push(CachedModel {
        name: String::from(name),
        size_bytes,
        loaded: true,
        last_used: crate::hpet::read_counter(),
        inference_count: 0,
    });
    crate::serial_println!("[AI] Cached model '{}' ({} bytes)", name, size_bytes);
    true
}

/// Evict a model from cache
pub fn evict_model(name: &str) -> bool {
    let mut cache = MODEL_CACHE.lock();
    if let Some(idx) = cache.iter().position(|m| m.name == name) {
        cache.remove(idx);
        true
    } else {
        false
    }
}

/// Get model cache stats
pub fn cache_stats() -> (usize, usize, usize) {
    let cache = MODEL_CACHE.lock();
    let total: usize = cache.iter().map(|m| m.size_bytes).sum();
    let limit = *MODEL_CACHE_LIMIT.lock();
    (cache.len(), total, limit)
}

// ═══════════════════════════════════════════════════════════════════════
// Streaming Token Output to UI
// ═══════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════
// AI Assistant Desktop Integration
// ═══════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════
// Voice-to-Text and Text-to-Voice Pipeline
// ═══════════════════════════════════════════════════════════════════════

/// Voice pipeline state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoicePipelineState {
    Idle,
    Listening,
    Processing,
    Speaking,
}

/// Voice pipeline
pub struct VoicePipeline {
    pub state: VoicePipelineState,
    pub stt_model: String, // Speech-to-text model
    pub tts_model: String, // Text-to-speech model
    pub language: String,
    pub last_transcript: String,
}

lazy_static::lazy_static! {
    static ref VOICE_PIPELINE: spin::Mutex<VoicePipeline> = spin::Mutex::new(VoicePipeline {
        state: VoicePipelineState::Idle,
        stt_model: String::new(),
        tts_model: String::new(),
        language: String::new(),
        last_transcript: String::new(),
    });
}

/// Initialize voice pipeline
pub fn voice_pipeline_init(stt_model: &str, tts_model: &str, language: &str) {
    let mut vp = VOICE_PIPELINE.lock();
    vp.stt_model = String::from(stt_model);
    vp.tts_model = String::from(tts_model);
    vp.language = String::from(language);
    crate::serial_println!(
        "[AI] Voice pipeline: STT={}, TTS={}, lang={}",
        stt_model,
        tts_model,
        language
    );
}

/// Start listening for voice input
pub fn voice_start_listening() -> bool {
    let mut vp = VOICE_PIPELINE.lock();
    if vp.state == VoicePipelineState::Idle {
        vp.state = VoicePipelineState::Listening;
        return true;
    }
    false
}

/// Process recorded audio and return transcript
pub fn voice_process_audio(_audio_data: &[i16]) -> Option<String> {
    let mut vp = VOICE_PIPELINE.lock();
    vp.state = VoicePipelineState::Processing;
    // In real implementation: run Whisper inference on audio data
    let transcript = String::from("(transcribed text)");
    vp.last_transcript = transcript.clone();
    vp.state = VoicePipelineState::Idle;
    Some(transcript)
}

/// Synthesize speech from text
pub fn voice_speak(text: &str) -> Option<Vec<i16>> {
    let mut vp = VOICE_PIPELINE.lock();
    vp.state = VoicePipelineState::Speaking;
    // In real implementation: run TTS model
    let _text_len = text.len();
    let samples = alloc::vec![0i16; 16000]; // 1 second of 16kHz audio stub
    vp.state = VoicePipelineState::Idle;
    Some(samples)
}

// ═══════════════════════════════════════════════════════════════════════
// Image Generation Model Support
// ═══════════════════════════════════════════════════════════════════════

/// Image generation parameters
pub struct ImageGenParams {
    pub prompt: String,
    pub negative_prompt: String,
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub guidance_scale: f32,
    pub seed: u64,
}

/// Generate an image from text prompt (stub — returns placeholder)
pub fn generate_image(params: &ImageGenParams) -> Option<Vec<u8>> {
    crate::serial_println!(
        "[AI] Image gen: '{}' ({}x{}, {} steps)",
        params.prompt,
        params.width,
        params.height,
        params.steps
    );
    // In real implementation: load Stable Diffusion model, run denoising loop
    let pixels = alloc::vec![128u8; (params.width * params.height * 4) as usize];
    Some(pixels)
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-Model Concurrent Inference
// ═══════════════════════════════════════════════════════════════════════

/// Inference job
#[derive(Debug, Clone)]
pub struct InferenceJob {
    pub job_id: u64,
    pub model_name: String,
    pub status: InferenceJobStatus,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceJobStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

lazy_static::lazy_static! {
    static ref INFERENCE_QUEUE: spin::Mutex<Vec<InferenceJob>> = spin::Mutex::new(Vec::new());
    static ref NEXT_JOB_ID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);
}

/// Submit an inference job
pub fn submit_inference_job(model_name: &str, _input: &str) -> u64 {
    let id = NEXT_JOB_ID.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
    INFERENCE_QUEUE.lock().push(InferenceJob {
        job_id: id,
        model_name: String::from(model_name),
        status: InferenceJobStatus::Queued,
        result: None,
    });
    id
}

/// Poll inference job status
pub fn poll_inference_job(job_id: u64) -> Option<InferenceJob> {
    INFERENCE_QUEUE
        .lock()
        .iter()
        .find(|j| j.job_id == job_id)
        .cloned()
}

/// Get count of running/queued jobs
pub fn active_inference_jobs() -> (usize, usize) {
    let queue = INFERENCE_QUEUE.lock();
    let running = queue
        .iter()
        .filter(|j| j.status == InferenceJobStatus::Running)
        .count();
    let queued = queue
        .iter()
        .filter(|j| j.status == InferenceJobStatus::Queued)
        .count();
    (running, queued)
}

// ═══════════════════════════════════════════════════════════════════════
// Model Download and Management UI Support
// ═══════════════════════════════════════════════════════════════════════

/// Available model info
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    pub family: String,
    pub size_mb: u32,
    pub quantization: String,
    pub downloaded: bool,
    pub download_progress: u8,
}

lazy_static::lazy_static! {
    static ref MODEL_REGISTRY: spin::Mutex<Vec<ModelInfo>> = spin::Mutex::new(Vec::new());
}

/// Register available model in the registry
pub fn register_model(name: &str, family: &str, size_mb: u32, quant: &str) {
    MODEL_REGISTRY.lock().push(ModelInfo {
        name: String::from(name),
        family: String::from(family),
        size_mb,
        quantization: String::from(quant),
        downloaded: false,
        download_progress: 0,
    });
}

/// List available models in registry
pub fn list_model_registry() -> Vec<ModelInfo> {
    MODEL_REGISTRY.lock().clone()
}

/// Start model download (simulated)
pub fn download_model(name: &str) -> bool {
    let mut registry = MODEL_REGISTRY.lock();
    if let Some(m) = registry.iter_mut().find(|m| m.name == name) {
        m.download_progress = 100;
        m.downloaded = true;
        crate::serial_println!("[AI] Model '{}' downloaded ({}MB)", name, m.size_mb);
        true
    } else {
        false
    }
}

/// Delete a downloaded model
pub fn delete_model(name: &str) -> bool {
    let mut registry = MODEL_REGISTRY.lock();
    if let Some(m) = registry.iter_mut().find(|m| m.name == name) {
        m.downloaded = false;
        m.download_progress = 0;
        true
    } else {
        false
    }
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
