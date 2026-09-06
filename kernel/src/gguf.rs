//! GGUF Model Loader — Load and run quantized LLM models
//!
//! Implements the GGUF file format (used by llama.cpp) for loading quantized
//! language models. Supports:
//!   - GGUF v3 file format parsing (header, metadata, tensor info)
//!   - Q4_0 and Q8_0 quantization formats (dequantization)
//!   - BPE tokenizer (encode text → token IDs)
//!   - Autoregressive transformer inference (forward pass)
//!   - Top-k / temperature sampling for text generation
//!
//! This enables KnoxOS to load GGUF model files from disk and generate text
//! using quantized transformer models (e.g., TinyLlama, Phi-2, etc.)

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// GGUF FILE FORMAT PARSER
// ═══════════════════════════════════════════════════════════════════════

/// GGUF magic number: "GGUF" in LE
const GGUF_MAGIC: u32 = 0x46475547; // "GGUF"

/// GGUF metadata value types
#[derive(Debug, Clone)]
pub enum GgufValue {
    U8(u8),
    I8(i8),
    U16(u16),
    I16(i16),
    U32(u32),
    I32(i32),
    F32(f32),
    Bool(bool),
    Str(String),
    Array(Vec<GgufValue>),
    U64(u64),
    I64(i64),
    F64(f64),
}

/// GGUF quantization types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GgmlType {
    F32 = 0,
    F16 = 1,
    Q4_0 = 2,
    Q4_1 = 3,
    Q5_0 = 6,
    Q5_1 = 7,
    Q8_0 = 8,
    Q8_1 = 9,
    Unknown,
}

impl From<u32> for GgmlType {
    fn from(v: u32) -> Self {
        match v {
            0 => GgmlType::F32,
            1 => GgmlType::F16,
            2 => GgmlType::Q4_0,
            3 => GgmlType::Q4_1,
            6 => GgmlType::Q5_0,
            7 => GgmlType::Q5_1,
            8 => GgmlType::Q8_0,
            9 => GgmlType::Q8_1,
            _ => GgmlType::Unknown,
        }
    }
}

/// Information about a tensor in the GGUF file
#[derive(Debug, Clone)]
pub struct TensorInfo {
    pub name: String,
    pub n_dims: u32,
    pub dims: [u64; 4],
    pub dtype: GgmlType,
    pub offset: u64,
}

/// Parsed GGUF file header + metadata
pub struct GgufFile {
    pub version: u32,
    pub n_tensors: u64,
    pub metadata: BTreeMap<String, GgufValue>,
    pub tensor_infos: Vec<TensorInfo>,
    pub data_offset: usize,
    /// Raw file data reference for tensor access
    raw_data: Vec<u8>,
}

/// Reader helper for sequential parsing
struct GgufReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> GgufReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn read_u8(&mut self) -> Option<u8> {
        if self.pos >= self.data.len() {
            return None;
        }
        let v = self.data[self.pos];
        self.pos += 1;
        Some(v)
    }

    fn read_u16(&mut self) -> Option<u16> {
        if self.pos + 2 > self.data.len() {
            return None;
        }
        let v = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Some(v)
    }

    fn read_u32(&mut self) -> Option<u32> {
        if self.pos + 4 > self.data.len() {
            return None;
        }
        let v = u32::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        Some(v)
    }

    fn read_u64(&mut self) -> Option<u64> {
        if self.pos + 8 > self.data.len() {
            return None;
        }
        let v = u64::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
            self.data[self.pos + 4],
            self.data[self.pos + 5],
            self.data[self.pos + 6],
            self.data[self.pos + 7],
        ]);
        self.pos += 8;
        Some(v)
    }

    fn read_i8(&mut self) -> Option<i8> {
        self.read_u8().map(|v| v as i8)
    }
    fn read_i16(&mut self) -> Option<i16> {
        self.read_u16().map(|v| v as i16)
    }
    fn read_i32(&mut self) -> Option<i32> {
        self.read_u32().map(|v| v as i32)
    }
    fn read_i64(&mut self) -> Option<i64> {
        self.read_u64().map(|v| v as i64)
    }

    fn read_f32(&mut self) -> Option<f32> {
        self.read_u32().map(f32::from_bits)
    }

    fn read_f64(&mut self) -> Option<f64> {
        self.read_u64().map(f64::from_bits)
    }

    fn read_string(&mut self) -> Option<String> {
        let len = self.read_u64()? as usize;
        if self.pos + len > self.data.len() {
            return None;
        }
        let s = String::from(core::str::from_utf8(&self.data[self.pos..self.pos + len]).ok()?);
        self.pos += len;
        Some(s)
    }

    fn read_value(&mut self, vtype: u32) -> Option<GgufValue> {
        match vtype {
            0 => self.read_u8().map(GgufValue::U8),
            1 => self.read_i8().map(GgufValue::I8),
            2 => self.read_u16().map(GgufValue::U16),
            3 => self.read_i16().map(GgufValue::I16),
            4 => self.read_u32().map(GgufValue::U32),
            5 => self.read_i32().map(GgufValue::I32),
            6 => self.read_f32().map(GgufValue::F32),
            7 => self.read_u8().map(|v| GgufValue::Bool(v != 0)),
            8 => self.read_string().map(GgufValue::Str),
            9 => {
                // Array: element_type(u32) + count(u64) + elements
                let elem_type = self.read_u32()?;
                let count = self.read_u64()? as usize;
                let mut arr = Vec::with_capacity(count.min(65536));
                for _ in 0..count {
                    arr.push(self.read_value(elem_type)?);
                }
                Some(GgufValue::Array(arr))
            }
            10 => self.read_u64().map(GgufValue::U64),
            11 => self.read_i64().map(GgufValue::I64),
            12 => self.read_f64().map(GgufValue::F64),
            _ => None,
        }
    }
}

/// Parse a GGUF file from raw bytes
pub fn parse_gguf(data: Vec<u8>) -> Option<GgufFile> {
    let mut r = GgufReader::new(&data);

    // Magic
    let magic = r.read_u32()?;
    if magic != GGUF_MAGIC {
        serial_println!(
            "[GGUF] Invalid magic: {:#x} (expected {:#x})",
            magic,
            GGUF_MAGIC
        );
        return None;
    }

    // Version
    let version = r.read_u32()?;
    if !(2..=3).contains(&version) {
        serial_println!("[GGUF] Unsupported version: {} (need 2 or 3)", version);
        return None;
    }

    // Tensor count & metadata KV count
    let n_tensors = r.read_u64()?;
    let n_kv = r.read_u64()?;

    serial_println!(
        "[GGUF] Version {}, {} tensors, {} metadata entries",
        version,
        n_tensors,
        n_kv
    );

    // Parse metadata key-value pairs
    let mut metadata = BTreeMap::new();
    for _ in 0..n_kv {
        let key = r.read_string()?;
        let vtype = r.read_u32()?;
        let value = r.read_value(vtype)?;
        metadata.insert(key, value);
    }

    // Parse tensor infos
    let mut tensor_infos = Vec::with_capacity(n_tensors as usize);
    for _ in 0..n_tensors {
        let name = r.read_string()?;
        let n_dims = r.read_u32()?;
        let mut dims = [0u64; 4];
        for d in 0..n_dims as usize {
            dims[d] = r.read_u64()?;
        }
        let dtype = GgmlType::from(r.read_u32()?);
        let offset = r.read_u64()?;

        tensor_infos.push(TensorInfo {
            name,
            n_dims,
            dims,
            dtype,
            offset,
        });
    }

    // Data starts at alignment boundary after header
    // GGUF aligns tensor data to 32 bytes
    let data_offset = (r.pos + 31) & !31;

    // Log model info from metadata
    if let Some(GgufValue::Str(arch)) = metadata.get("general.architecture") {
        serial_println!("[GGUF] Architecture: {}", arch);
    }
    if let Some(GgufValue::Str(name)) = metadata.get("general.name") {
        serial_println!("[GGUF] Model name: {}", name);
    }

    Some(GgufFile {
        version,
        n_tensors,
        metadata,
        tensor_infos,
        data_offset,
        raw_data: data,
    })
}

// ═══════════════════════════════════════════════════════════════════════
// DEQUANTIZATION — Q4_0 and Q8_0
// ═══════════════════════════════════════════════════════════════════════

/// Q4_0 block: 32 weights packed into 18 bytes (1 f16 scale + 16 bytes of nibbles)
const Q4_0_BLOCK_SIZE: usize = 32;

/// Dequantize a Q4_0 block (18 bytes → 32 f32 values)
fn dequantize_q4_0_block(block: &[u8]) -> [f32; Q4_0_BLOCK_SIZE] {
    let mut result = [0.0f32; Q4_0_BLOCK_SIZE];
    if block.len() < 18 {
        return result;
    }

    // First 2 bytes: f16 scale factor
    let scale_bits = u16::from_le_bytes([block[0], block[1]]);
    let scale = f16_to_f32(scale_bits);

    // Next 16 bytes: 32 4-bit quantized values (packed 2 per byte)
    for i in 0..16 {
        let byte = block[2 + i];
        let lo = (byte & 0x0F) as i8 - 8; // 4-bit value, offset by 8
        let hi = ((byte >> 4) & 0x0F) as i8 - 8;
        result[i * 2] = lo as f32 * scale;
        result[i * 2 + 1] = hi as f32 * scale;
    }

    result
}

/// Dequantize a Q8_0 block (34 bytes → 32 f32 values)
fn dequantize_q8_0_block(block: &[u8]) -> [f32; 32] {
    let mut result = [0.0f32; 32];
    if block.len() < 34 {
        return result;
    }

    let scale_bits = u16::from_le_bytes([block[0], block[1]]);
    let scale = f16_to_f32(scale_bits);

    for i in 0..32 {
        result[i] = block[2 + i] as i8 as f32 * scale;
    }

    result
}

/// Convert IEEE 754 half-precision float to f32
fn f16_to_f32(bits: u16) -> f32 {
    let sign = ((bits >> 15) & 1) as u32;
    let exp = ((bits >> 10) & 0x1F) as u32;
    let mant = (bits & 0x3FF) as u32;

    if exp == 0 {
        if mant == 0 {
            return f32::from_bits(sign << 31);
        }
        // Subnormal
        let mut e = 1u32;
        let mut m = mant;
        while m & 0x400 == 0 {
            m <<= 1;
            e += 1;
        }
        let f32_exp = 127 - 15 - e + 1;
        let f32_mant = (m & 0x3FF) << 13;
        return f32::from_bits((sign << 31) | (f32_exp << 23) | f32_mant);
    }

    if exp == 31 {
        let f32_mant = mant << 13;
        return f32::from_bits((sign << 31) | (0xFF << 23) | f32_mant);
    }

    let f32_exp = (exp as i32 - 15 + 127) as u32;
    let f32_mant = mant << 13;
    f32::from_bits((sign << 31) | (f32_exp << 23) | f32_mant)
}

/// Dequantize tensor data from a GGUF file
fn dequantize_tensor(gguf: &GgufFile, info: &TensorInfo) -> Option<Vec<f32>> {
    let data_start = gguf.data_offset + info.offset as usize;
    let n_elements: u64 = info.dims[..info.n_dims as usize].iter().product();
    let n = n_elements as usize;

    match info.dtype {
        GgmlType::F32 => {
            let byte_len = n * 4;
            if data_start + byte_len > gguf.raw_data.len() {
                return None;
            }
            let mut out = Vec::with_capacity(n);
            for i in 0..n {
                let off = data_start + i * 4;
                let bits = u32::from_le_bytes([
                    gguf.raw_data[off],
                    gguf.raw_data[off + 1],
                    gguf.raw_data[off + 2],
                    gguf.raw_data[off + 3],
                ]);
                out.push(f32::from_bits(bits));
            }
            Some(out)
        }
        GgmlType::Q4_0 => {
            let n_blocks = n.div_ceil(Q4_0_BLOCK_SIZE);
            let byte_len = n_blocks * 18;
            if data_start + byte_len > gguf.raw_data.len() {
                return None;
            }
            let mut out = Vec::with_capacity(n);
            for b in 0..n_blocks {
                let block = &gguf.raw_data[data_start + b * 18..data_start + (b + 1) * 18];
                let vals = dequantize_q4_0_block(block);
                out.extend_from_slice(&vals);
            }
            out.truncate(n);
            Some(out)
        }
        GgmlType::Q8_0 => {
            let n_blocks = n.div_ceil(32);
            let byte_len = n_blocks * 34;
            if data_start + byte_len > gguf.raw_data.len() {
                return None;
            }
            let mut out = Vec::with_capacity(n);
            for b in 0..n_blocks {
                let block = &gguf.raw_data[data_start + b * 34..data_start + (b + 1) * 34];
                let vals = dequantize_q8_0_block(block);
                out.extend_from_slice(&vals);
            }
            out.truncate(n);
            Some(out)
        }
        _ => {
            serial_println!("[GGUF] Unsupported quantization type: {:?}", info.dtype);
            None
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// BPE TOKENIZER
// ═══════════════════════════════════════════════════════════════════════

/// A simple BPE tokenizer
pub struct Tokenizer {
    /// Token ID → string
    pub vocab: Vec<String>,
    /// Token scores (for merge priority)
    pub scores: Vec<f32>,
    /// String → Token ID lookup
    pub token_to_id: BTreeMap<String, u32>,
}

impl Tokenizer {
    /// Build tokenizer from GGUF metadata
    pub fn from_gguf(gguf: &GgufFile) -> Option<Self> {
        let tokens = match gguf.metadata.get("tokenizer.ggml.tokens") {
            Some(GgufValue::Array(arr)) => arr,
            _ => {
                serial_println!("[GGUF] No tokenizer.ggml.tokens found");
                return None;
            }
        };

        let scores = match gguf.metadata.get("tokenizer.ggml.scores") {
            Some(GgufValue::Array(arr)) => arr,
            _ => &Vec::new(),
        };

        let mut vocab = Vec::with_capacity(tokens.len());
        let mut score_vec = Vec::with_capacity(tokens.len());
        let mut token_to_id = BTreeMap::new();

        for (i, tok) in tokens.iter().enumerate() {
            let s = match tok {
                GgufValue::Str(s) => s.clone(),
                _ => format!("<{}>", i),
            };
            token_to_id.insert(s.clone(), i as u32);
            vocab.push(s);
            let sc = scores
                .get(i)
                .and_then(|v| match v {
                    GgufValue::F32(f) => Some(*f),
                    _ => None,
                })
                .unwrap_or(0.0);
            score_vec.push(sc);
        }

        serial_println!("[GGUF] Tokenizer: {} tokens loaded", vocab.len());

        Some(Self {
            vocab,
            scores: score_vec,
            token_to_id,
        })
    }

    /// Encode text into token IDs using simple BPE
    pub fn encode(&self, text: &str) -> Vec<u32> {
        if text.is_empty() {
            return Vec::new();
        }

        // Start with individual characters
        let mut tokens: Vec<String> = text
            .chars()
            .map(|c| {
                let mut s = String::new();
                s.push(c);
                s
            })
            .collect();

        // Iteratively merge the highest-scoring pair
        loop {
            let mut best_score = f32::NEG_INFINITY;
            let mut best_idx = usize::MAX;
            let mut best_merged = String::new();

            for i in 0..tokens.len().saturating_sub(1) {
                let merged = format!("{}{}", tokens[i], tokens[i + 1]);
                if let Some(&tid) = self.token_to_id.get(&merged) {
                    let score = self.scores.get(tid as usize).copied().unwrap_or(0.0);
                    if score > best_score {
                        best_score = score;
                        best_idx = i;
                        best_merged = merged;
                    }
                }
            }

            if best_idx == usize::MAX {
                break;
            }

            // Apply merge
            tokens[best_idx] = best_merged;
            tokens.remove(best_idx + 1);
        }

        // Convert to IDs
        tokens
            .iter()
            .map(|t| {
                self.token_to_id.get(t).copied().unwrap_or(0) // 0 = unknown token
            })
            .collect()
    }

    /// Decode token IDs back to text
    pub fn decode(&self, ids: &[u32]) -> String {
        let mut result = String::new();
        for &id in ids {
            if let Some(tok) = self.vocab.get(id as usize) {
                result.push_str(tok);
            }
        }
        result
    }
}

// ═══════════════════════════════════════════════════════════════════════
// TRANSFORMER FORWARD PASS
// ═══════════════════════════════════════════════════════════════════════

/// Extracted model weights for transformer inference
pub struct TransformerWeights {
    /// Token embedding table [vocab_size × dim]
    pub embed: Vec<f32>,
    /// Per-layer weights
    pub layers: Vec<TransformerLayerWeights>,
    /// Final RMSNorm weight
    pub norm_weight: Vec<f32>,
    /// Output projection (often == embed transposed)
    pub output: Vec<f32>,
    /// Model dimension
    pub dim: usize,
    /// Number of attention heads
    pub n_heads: usize,
    /// Number of KV heads (for GQA)
    pub n_kv_heads: usize,
    /// Number of layers
    pub n_layers: usize,
    /// Vocabulary size
    pub vocab_size: usize,
    /// Head dimension
    pub head_dim: usize,
}

pub struct TransformerLayerWeights {
    pub attn_norm: Vec<f32>, // RMSNorm for attention
    pub wq: Vec<f32>,        // Query projection [dim × dim]
    pub wk: Vec<f32>,        // Key projection [dim × kv_dim]
    pub wv: Vec<f32>,        // Value projection [dim × kv_dim]
    pub wo: Vec<f32>,        // Output projection [dim × dim]
    pub ffn_norm: Vec<f32>,  // RMSNorm for FFN
    pub w1: Vec<f32>,        // FFN gate [dim × ff_dim]
    pub w2: Vec<f32>,        // FFN down [ff_dim × dim]
    pub w3: Vec<f32>,        // FFN up [dim × ff_dim]
}

/// RMS normalization
fn rms_norm(out: &mut [f32], x: &[f32], weight: &[f32]) {
    let n = x.len();
    let mut ss: f32 = 0.0;
    for &v in x {
        ss += v * v;
    }
    ss = 1.0 / libm::sqrtf(ss / n as f32 + 1e-5);
    for i in 0..n {
        out[i] = x[i] * ss * weight[i];
    }
}

/// Matrix-vector multiply: out = mat × x
/// mat is [rows × cols], stored row-major
fn matmul(out: &mut [f32], mat: &[f32], x: &[f32], rows: usize, cols: usize) {
    for r in 0..rows {
        let mut sum = 0.0f32;
        let base = r * cols;
        for c in 0..cols {
            sum += mat[base + c] * x[c];
        }
        out[r] = sum;
    }
}

/// Softmax in-place
fn softmax(x: &mut [f32]) {
    let max_val = x.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut sum = 0.0f32;
    for v in x.iter_mut() {
        *v = libm::expf(*v - max_val);
        sum += *v;
    }
    let inv = 1.0 / sum;
    for v in x.iter_mut() {
        *v *= inv;
    }
}

/// SiLU activation: x * sigmoid(x)
fn silu(x: f32) -> f32 {
    x / (1.0 + libm::expf(-x))
}

/// Run one forward pass of the transformer for a single token
/// Returns logits [vocab_size]
fn forward(
    weights: &TransformerWeights,
    token: u32,
    pos: usize,
    kv_cache_k: &mut [Vec<f32>], // [layer][pos * kv_dim .. (pos+1) * kv_dim]
    kv_cache_v: &mut [Vec<f32>],
) -> Vec<f32> {
    let dim = weights.dim;
    let n_heads = weights.n_heads;
    let n_kv_heads = weights.n_kv_heads;
    let head_dim = weights.head_dim;
    let kv_dim = n_kv_heads * head_dim;
    let kv_mul = n_heads / n_kv_heads;

    // Token embedding
    let mut x = vec![0.0f32; dim];
    let emb_offset = token as usize * dim;
    if emb_offset + dim <= weights.embed.len() {
        x.copy_from_slice(&weights.embed[emb_offset..emb_offset + dim]);
    }

    // Transformer layers
    let mut xb = vec![0.0f32; dim];
    let mut q = vec![0.0f32; dim];
    let mut k = vec![0.0f32; kv_dim];
    let mut v = vec![0.0f32; kv_dim];
    let mut xb2 = vec![0.0f32; dim];
    let mut hb = vec![0.0f32; dim * 4]; // FFN hidden (approximate)
    let mut hb2 = vec![0.0f32; dim * 4];

    for (layer_idx, lw) in weights.layers.iter().enumerate() {
        // Attention norm
        rms_norm(&mut xb, &x, &lw.attn_norm);

        // QKV projections
        matmul(&mut q, &lw.wq, &xb, dim, dim);
        matmul(&mut k, &lw.wk, &xb, kv_dim, dim);
        matmul(&mut v, &lw.wv, &xb, kv_dim, dim);

        // RoPE positional encoding
        for h in 0..n_heads {
            for i in (0..head_dim).step_by(2) {
                let freq = 1.0 / libm::powf(10000.0, i as f32 / head_dim as f32);
                let val = pos as f32 * freq;
                let cos_val = libm::cosf(val);
                let sin_val = libm::sinf(val);

                let qi = h * head_dim + i;
                if qi + 1 < q.len() {
                    let q0 = q[qi];
                    let q1 = q[qi + 1];
                    q[qi] = q0 * cos_val - q1 * sin_val;
                    q[qi + 1] = q0 * sin_val + q1 * cos_val;
                }
            }
        }
        for h in 0..n_kv_heads {
            for i in (0..head_dim).step_by(2) {
                let freq = 1.0 / libm::powf(10000.0, i as f32 / head_dim as f32);
                let val = pos as f32 * freq;
                let cos_val = libm::cosf(val);
                let sin_val = libm::sinf(val);

                let ki = h * head_dim + i;
                if ki + 1 < k.len() {
                    let k0 = k[ki];
                    let k1 = k[ki + 1];
                    k[ki] = k0 * cos_val - k1 * sin_val;
                    k[ki + 1] = k0 * sin_val + k1 * cos_val;
                }
            }
        }

        // Update KV cache
        let cache_offset = pos * kv_dim;
        if cache_offset + kv_dim <= kv_cache_k[layer_idx].len() {
            kv_cache_k[layer_idx][cache_offset..cache_offset + kv_dim]
                .copy_from_slice(&k[..kv_dim]);
            kv_cache_v[layer_idx][cache_offset..cache_offset + kv_dim]
                .copy_from_slice(&v[..kv_dim]);
        }

        // Multi-head attention
        let mut attn_out = vec![0.0f32; dim];
        for h in 0..n_heads {
            let kv_h = h / kv_mul;
            // Compute attention scores for this head
            let mut scores = vec![0.0f32; pos + 1];
            for t in 0..=pos {
                let mut dot = 0.0f32;
                for d in 0..head_dim {
                    let qi = h * head_dim + d;
                    let ki = t * kv_dim + kv_h * head_dim + d;
                    if qi < q.len() && ki < kv_cache_k[layer_idx].len() {
                        dot += q[qi] * kv_cache_k[layer_idx][ki];
                    }
                }
                scores[t] = dot / libm::sqrtf(head_dim as f32);
            }

            softmax(&mut scores);

            // Weighted sum of values
            for d in 0..head_dim {
                let mut val = 0.0f32;
                for t in 0..=pos {
                    let vi = t * kv_dim + kv_h * head_dim + d;
                    if vi < kv_cache_v[layer_idx].len() {
                        val += scores[t] * kv_cache_v[layer_idx][vi];
                    }
                }
                attn_out[h * head_dim + d] = val;
            }
        }

        // Output projection + residual
        matmul(&mut xb2, &lw.wo, &attn_out, dim, dim);
        for i in 0..dim {
            x[i] += xb2[i];
        }

        // FFN
        rms_norm(&mut xb, &x, &lw.ffn_norm);

        let ff_dim = lw.w1.len() / dim;
        if ff_dim > 0 {
            hb.resize(ff_dim, 0.0);
            hb2.resize(ff_dim, 0.0);

            matmul(&mut hb[..ff_dim], &lw.w1, &xb, ff_dim, dim);
            matmul(&mut hb2[..ff_dim], &lw.w3, &xb, ff_dim, dim);

            // SiLU gate
            for i in 0..ff_dim {
                hb[i] = silu(hb[i]) * hb2[i];
            }

            // Down projection + residual
            matmul(&mut xb2, &lw.w2, &hb[..ff_dim], dim, ff_dim);
            for i in 0..dim {
                x[i] += xb2[i];
            }
        }
    }

    // Final norm
    rms_norm(&mut xb, &x, &weights.norm_weight);

    // Output logits
    let mut logits = vec![0.0f32; weights.vocab_size];
    matmul(&mut logits, &weights.output, &xb, weights.vocab_size, dim);

    logits
}

// ═══════════════════════════════════════════════════════════════════════
// TEXT GENERATION
// ═══════════════════════════════════════════════════════════════════════

/// Generation parameters
pub struct GenerationConfig {
    pub max_tokens: usize,
    pub temperature: f32,
    pub top_k: usize,
    /// Stop token ID (usually 2 for EOS)
    pub eos_token: u32,
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            max_tokens: 128,
            temperature: 0.8,
            top_k: 40,
            eos_token: 2,
        }
    }
}

/// Sample from logits with temperature and top-k
fn sample_token(logits: &mut [f32], config: &GenerationConfig, rng_state: &mut u64) -> u32 {
    let temp = config.temperature;
    if temp <= 0.0 {
        // Greedy
        return logits
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(core::cmp::Ordering::Equal))
            .map(|(i, _)| i as u32)
            .unwrap_or(0);
    }

    // Apply temperature
    let inv_temp = 1.0 / temp;
    for v in logits.iter_mut() {
        *v *= inv_temp;
    }

    // Top-k filtering
    let k = config.top_k.min(logits.len());
    if k < logits.len() {
        // Find k-th largest value
        let mut sorted_indices: Vec<usize> = (0..logits.len()).collect();
        sorted_indices.sort_by(|&a, &b| {
            logits[b]
                .partial_cmp(&logits[a])
                .unwrap_or(core::cmp::Ordering::Equal)
        });
        let threshold = logits[sorted_indices[k - 1]];
        for v in logits.iter_mut() {
            if *v < threshold {
                *v = f32::NEG_INFINITY;
            }
        }
    }

    // Softmax
    softmax(logits);

    // Sample from distribution
    let r = xorshift64(rng_state) as f32 / u64::MAX as f32;
    let mut cumsum = 0.0f32;
    for (i, &p) in logits.iter().enumerate() {
        cumsum += p;
        if cumsum >= r {
            return i as u32;
        }
    }

    logits.len() as u32 - 1
}

/// Simple xorshift64 PRNG
fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

/// Extract transformer weights from a parsed GGUF file
pub fn extract_weights(gguf: &GgufFile) -> Option<TransformerWeights> {
    // Get model dimensions from metadata
    let dim = get_u32_meta(gguf, "llama.embedding_length")
        .or_else(|| get_u32_meta(gguf, "phi2.embedding_length"))
        .unwrap_or(512) as usize;
    let n_heads = get_u32_meta(gguf, "llama.attention.head_count")
        .or_else(|| get_u32_meta(gguf, "phi2.attention.head_count"))
        .unwrap_or(8) as usize;
    let n_kv_heads = get_u32_meta(gguf, "llama.attention.head_count_kv")
        .or_else(|| get_u32_meta(gguf, "phi2.attention.head_count_kv"))
        .unwrap_or(n_heads as u32) as usize;
    let n_layers = get_u32_meta(gguf, "llama.block_count")
        .or_else(|| get_u32_meta(gguf, "phi2.block_count"))
        .unwrap_or(4) as usize;
    let vocab_size = get_u32_meta(gguf, "llama.vocab_size")
        .or_else(|| get_u32_meta(gguf, "phi2.vocab_size"))
        .unwrap_or(32000) as usize;
    let head_dim = dim / n_heads;

    serial_println!(
        "[GGUF] Model: dim={}, heads={}, kv_heads={}, layers={}, vocab={}",
        dim,
        n_heads,
        n_kv_heads,
        n_layers,
        vocab_size
    );

    // Helper to find and dequantize a tensor by name
    let find_tensor = |name: &str| -> Option<Vec<f32>> {
        for info in &gguf.tensor_infos {
            if info.name == name {
                return dequantize_tensor(gguf, info);
            }
        }
        None
    };

    // Extract weights
    let embed = find_tensor("token_embd.weight")?;
    let norm_weight = find_tensor("output_norm.weight")?;
    let output = find_tensor("output.weight").unwrap_or_else(|| embed.clone()); // Tied embeddings

    let mut layers = Vec::with_capacity(n_layers);
    for l in 0..n_layers {
        let lw = TransformerLayerWeights {
            attn_norm: find_tensor(&format!("blk.{}.attn_norm.weight", l))
                .unwrap_or_else(|| vec![1.0; dim]),
            wq: find_tensor(&format!("blk.{}.attn_q.weight", l))
                .unwrap_or_else(|| vec![0.0; dim * dim]),
            wk: find_tensor(&format!("blk.{}.attn_k.weight", l))
                .unwrap_or_else(|| vec![0.0; n_kv_heads * head_dim * dim]),
            wv: find_tensor(&format!("blk.{}.attn_v.weight", l))
                .unwrap_or_else(|| vec![0.0; n_kv_heads * head_dim * dim]),
            wo: find_tensor(&format!("blk.{}.attn_output.weight", l))
                .unwrap_or_else(|| vec![0.0; dim * dim]),
            ffn_norm: find_tensor(&format!("blk.{}.ffn_norm.weight", l))
                .unwrap_or_else(|| vec![1.0; dim]),
            w1: find_tensor(&format!("blk.{}.ffn_gate.weight", l)).unwrap_or_else(Vec::new),
            w2: find_tensor(&format!("blk.{}.ffn_down.weight", l)).unwrap_or_else(Vec::new),
            w3: find_tensor(&format!("blk.{}.ffn_up.weight", l)).unwrap_or_else(Vec::new),
        };
        layers.push(lw);
    }

    Some(TransformerWeights {
        embed,
        layers,
        norm_weight,
        output,
        dim,
        n_heads,
        n_kv_heads,
        n_layers,
        vocab_size,
        head_dim,
    })
}

fn get_u32_meta(gguf: &GgufFile, key: &str) -> Option<u32> {
    match gguf.metadata.get(key) {
        Some(GgufValue::U32(v)) => Some(*v),
        Some(GgufValue::I32(v)) => Some(*v as u32),
        _ => None,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// HIGH-LEVEL API
// ═══════════════════════════════════════════════════════════════════════

/// A loaded and ready-to-use LLM
pub struct LoadedLlm {
    pub weights: TransformerWeights,
    pub tokenizer: Tokenizer,
    pub name: String,
}

lazy_static::lazy_static! {
    static ref LOADED_LLMS: Mutex<BTreeMap<u64, LoadedLlm>> = Mutex::new(BTreeMap::new());
}
static NEXT_LLM_ID: AtomicU64 = AtomicU64::new(1);

/// Return the ID of the first loaded model, if any
pub fn first_loaded_model_id() -> Option<u64> {
    LOADED_LLMS.lock().keys().next().copied()
}

/// Load a GGUF model from raw file data
pub fn load_model(data: Vec<u8>) -> Result<u64, &'static str> {
    let gguf = parse_gguf(data).ok_or("Failed to parse GGUF file")?;
    let tokenizer = Tokenizer::from_gguf(&gguf).ok_or("Failed to build tokenizer")?;
    let weights = extract_weights(&gguf).ok_or("Failed to extract model weights")?;

    let name = match gguf.metadata.get("general.name") {
        Some(GgufValue::Str(s)) => s.clone(),
        _ => String::from("unknown"),
    };

    let id = NEXT_LLM_ID.fetch_add(1, Ordering::Relaxed);

    serial_println!("[GGUF] Model '{}' loaded (id={})", name, id);

    LOADED_LLMS.lock().insert(
        id,
        LoadedLlm {
            weights,
            tokenizer,
            name,
        },
    );

    Ok(id)
}

/// Generate text using a loaded model
pub fn generate(
    model_id: u64,
    prompt: &str,
    config: &GenerationConfig,
) -> Result<String, &'static str> {
    let models = LOADED_LLMS.lock();
    let model = models.get(&model_id).ok_or("Model not loaded")?;

    let prompt_tokens = model.tokenizer.encode(prompt);
    if prompt_tokens.is_empty() {
        return Err("Empty prompt after tokenization");
    }

    let dim = model.weights.dim;
    let n_layers = model.weights.n_layers;
    let n_kv_heads = model.weights.n_kv_heads;
    let head_dim = model.weights.head_dim;
    let kv_dim = n_kv_heads * head_dim;
    let max_seq = prompt_tokens.len() + config.max_tokens;

    // Initialize KV cache
    let mut kv_cache_k = Vec::with_capacity(n_layers);
    let mut kv_cache_v = Vec::with_capacity(n_layers);
    for _ in 0..n_layers {
        kv_cache_k.push(vec![0.0f32; max_seq * kv_dim]);
        kv_cache_v.push(vec![0.0f32; max_seq * kv_dim]);
    }

    let mut rng_state = 42u64;
    let mut generated_tokens: Vec<u32> = Vec::new();

    // Process prompt tokens
    for (pos, &tok) in prompt_tokens.iter().enumerate() {
        let _ = forward(&model.weights, tok, pos, &mut kv_cache_k, &mut kv_cache_v);
    }

    // Generate new tokens
    let mut prev_token = *prompt_tokens.last().unwrap_or(&0);

    for (pos, _) in (prompt_tokens.len()..).zip(0..config.max_tokens) {
        let mut logits = forward(
            &model.weights,
            prev_token,
            pos,
            &mut kv_cache_k,
            &mut kv_cache_v,
        );
        let next_token = sample_token(&mut logits, config, &mut rng_state);

        if next_token == config.eos_token {
            break;
        }

        generated_tokens.push(next_token);
        prev_token = next_token;
    }

    let output = model.tokenizer.decode(&generated_tokens);

    serial_println!(
        "[GGUF] Generated {} tokens for model {}",
        generated_tokens.len(),
        model_id
    );

    Ok(output)
}

/// Load a GGUF model from the VFS
pub fn load_model_from_path(path: &str) -> Result<u64, &'static str> {
    let data = crate::vfs::read_file_dispatch(path).ok_or("Failed to read model file")?;
    load_model(data)
}

/// Initialize the GGUF subsystem
pub fn init() {
    serial_println!(
        "[KnoxOS] GGUF model loader initialized (Q4_0/Q8_0, BPE tokenizer, transformer inference)"
    );

    // Try loading a model from default path
    let default_paths = ["/usr/share/models/model.gguf", "/opt/models/tinyllama.gguf"];
    for path in &default_paths {
        if let Ok(id) = load_model_from_path(path) {
            serial_println!("[GGUF] Auto-loaded model from {} (id={})", path, id);
            break;
        }
    }
}
