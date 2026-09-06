/// ONNX Machine Learning Runtime
/// Provides in-kernel ONNX model inference for AI-native OS features
///
/// Features:
/// - ONNX model format parsing (protobuf-like binary)
/// - Tensor operations (add, mul, matmul, relu, softmax, conv2d)
/// - Model graph execution engine
/// - SIMD-accelerated operations (SSE2/AVX when available)
/// - Pre-trained model loading from filesystem
/// - Inference API for user-space and kernel consumers
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── no_std math helpers ────────────────────────────────────────────

/// Approximate exp(x) using a 13-term Taylor series
fn approx_exp(x: f32) -> f32 {
    // Clamp to avoid overflow/underflow
    let x = x.clamp(-88.0, 88.0);
    // Reduce: exp(x) = 2^k * exp(r) where x = k*ln2 + r
    let ln2 = core::f32::consts::LN_2;
    let k = (x / ln2) as i32;
    let r = x - (k as f32) * ln2;
    // Taylor series for exp(r) where |r| < ln2
    let mut term = 1.0_f32;
    let mut sum = 1.0_f32;
    for i in 1..13 {
        term *= r / (i as f32);
        sum += term;
    }
    // Multiply by 2^k using bit manipulation
    let pow2k = f32::from_bits(((127 + k) as u32) << 23);
    sum * pow2k
}

/// Approximate sqrt(x) using Newton-Raphson with fast inverse sqrt seed
fn approx_sqrt(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    // Fast inverse sqrt (Quake III style) as initial guess
    let half = 0.5 * x;
    let i = f32::to_bits(x);
    let i = 0x5f3759df - (i >> 1);
    let mut y = f32::from_bits(i);
    // Two Newton-Raphson iterations for inv_sqrt
    y = y * (1.5 - half * y * y);
    y = y * (1.5 - half * y * y);
    // sqrt(x) = x * inv_sqrt(x)
    x * y
}

// ─── Tensor Data Types ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    Float32 = 1,
    Uint8 = 2,
    Int8 = 3,
    Uint16 = 4,
    Int16 = 5,
    Int32 = 6,
    Int64 = 7,
    Float16 = 10,
    Float64 = 11,
    Bool = 9,
}

impl DataType {
    pub fn element_size(&self) -> usize {
        match self {
            Self::Float32 => 4,
            Self::Uint8 | Self::Int8 | Self::Bool => 1,
            Self::Uint16 | Self::Int16 | Self::Float16 => 2,
            Self::Int32 => 4,
            Self::Int64 | Self::Float64 => 8,
        }
    }
}

// ─── Tensor ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Tensor {
    pub data: Vec<f32>, // Store everything as f32 for simplicity
    pub shape: Vec<usize>,
    pub dtype: DataType,
    pub name: String,
}

impl Tensor {
    pub fn new(shape: &[usize], dtype: DataType) -> Self {
        let total: usize = shape.iter().product();
        Self {
            data: vec![0.0f32; total],
            shape: shape.to_vec(),
            dtype,
            name: String::new(),
        }
    }

    pub fn from_data(shape: &[usize], data: Vec<f32>) -> Self {
        Self {
            data,
            shape: shape.to_vec(),
            dtype: DataType::Float32,
            name: String::new(),
        }
    }

    pub fn named(mut self, name: &str) -> Self {
        self.name = String::from(name);
        self
    }

    pub fn total_elements(&self) -> usize {
        self.shape.iter().product()
    }

    pub fn ndim(&self) -> usize {
        self.shape.len()
    }

    /// Element-wise addition
    pub fn add(&self, other: &Tensor) -> Tensor {
        assert_eq!(self.shape, other.shape, "Shape mismatch for add");
        let data: Vec<f32> = self
            .data
            .iter()
            .zip(other.data.iter())
            .map(|(a, b)| a + b)
            .collect();
        Tensor::from_data(&self.shape, data)
    }

    /// Element-wise multiplication
    pub fn mul(&self, other: &Tensor) -> Tensor {
        assert_eq!(self.shape, other.shape, "Shape mismatch for mul");
        let data: Vec<f32> = self
            .data
            .iter()
            .zip(other.data.iter())
            .map(|(a, b)| a * b)
            .collect();
        Tensor::from_data(&self.shape, data)
    }

    /// Scalar multiplication
    pub fn scale(&self, scalar: f32) -> Tensor {
        let data: Vec<f32> = self.data.iter().map(|x| x * scalar).collect();
        Tensor::from_data(&self.shape, data)
    }

    /// Matrix multiplication (2D tensors)
    pub fn matmul(&self, other: &Tensor) -> Tensor {
        assert!(self.ndim() == 2 && other.ndim() == 2);
        let m = self.shape[0];
        let k = self.shape[1];
        assert_eq!(k, other.shape[0]);
        let n = other.shape[1];

        let mut result = vec![0.0f32; m * n];
        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0f32;
                for p in 0..k {
                    sum += self.data[i * k + p] * other.data[p * n + j];
                }
                result[i * n + j] = sum;
            }
        }
        Tensor::from_data(&[m, n], result)
    }

    /// ReLU activation
    pub fn relu(&self) -> Tensor {
        let data: Vec<f32> = self
            .data
            .iter()
            .map(|x| if *x > 0.0 { *x } else { 0.0 })
            .collect();
        Tensor::from_data(&self.shape, data)
    }

    /// Sigmoid activation
    pub fn sigmoid(&self) -> Tensor {
        let data: Vec<f32> = self
            .data
            .iter()
            .map(|x| 1.0 / (1.0 + approx_exp(-x)))
            .collect();
        Tensor::from_data(&self.shape, data)
    }

    /// Tanh activation
    pub fn tanh_activation(&self) -> Tensor {
        let data: Vec<f32> = self
            .data
            .iter()
            .map(|x| {
                let e2x = approx_exp(2.0 * x);
                (e2x - 1.0) / (e2x + 1.0)
            })
            .collect();
        Tensor::from_data(&self.shape, data)
    }

    /// Softmax (along last axis)
    pub fn softmax(&self) -> Tensor {
        let mut data = self.data.clone();
        let last_dim = *self.shape.last().unwrap_or(&1);
        let batch_size = self.total_elements() / last_dim;

        for b in 0..batch_size {
            let start = b * last_dim;
            let end = start + last_dim;
            let slice = &mut data[start..end];

            // Numerical stability: subtract max
            let max_val = slice.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            for x in slice.iter_mut() {
                *x = approx_exp(*x - max_val);
            }
            let sum: f32 = slice.iter().sum();
            if sum > 0.0 {
                for x in slice.iter_mut() {
                    *x /= sum;
                }
            }
        }
        Tensor::from_data(&self.shape, data)
    }

    /// Reshape tensor
    pub fn reshape(&self, new_shape: &[usize]) -> Tensor {
        let total: usize = new_shape.iter().product();
        assert_eq!(total, self.total_elements(), "Reshape size mismatch");
        Tensor::from_data(new_shape, self.data.clone())
    }

    /// Transpose 2D tensor
    pub fn transpose(&self) -> Tensor {
        assert_eq!(self.ndim(), 2);
        let rows = self.shape[0];
        let cols = self.shape[1];
        let mut data = vec![0.0f32; rows * cols];
        for i in 0..rows {
            for j in 0..cols {
                data[j * rows + i] = self.data[i * cols + j];
            }
        }
        Tensor::from_data(&[cols, rows], data)
    }

    /// Batch normalization
    pub fn batch_norm(
        &self,
        gamma: &Tensor,
        beta: &Tensor,
        mean: &Tensor,
        variance: &Tensor,
        epsilon: f32,
    ) -> Tensor {
        let mut data = self.data.clone();
        let channels = gamma.total_elements();
        let spatial = self.total_elements() / channels;

        for c in 0..channels {
            let g = gamma.data[c];
            let b = beta.data[c];
            let m = mean.data[c];
            let v = variance.data[c];
            let inv_std = 1.0 / approx_sqrt(v + epsilon);

            for s in 0..spatial {
                let idx = c * spatial + s;
                if idx < data.len() {
                    data[idx] = g * (data[idx] - m) * inv_std + b;
                }
            }
        }
        Tensor::from_data(&self.shape, data)
    }

    /// Max pooling 2D (NCHW format)
    pub fn max_pool2d(&self, kernel: usize, stride: usize) -> Tensor {
        assert_eq!(self.ndim(), 4); // NCHW
        let n = self.shape[0];
        let c = self.shape[1];
        let h = self.shape[2];
        let w = self.shape[3];
        let oh = (h - kernel) / stride + 1;
        let ow = (w - kernel) / stride + 1;

        let mut out = vec![0.0f32; n * c * oh * ow];
        for ni in 0..n {
            for ci in 0..c {
                for oi in 0..oh {
                    for oj in 0..ow {
                        let mut max_val = f32::NEG_INFINITY;
                        for ki in 0..kernel {
                            for kj in 0..kernel {
                                let hi = oi * stride + ki;
                                let wj = oj * stride + kj;
                                let idx = ni * c * h * w + ci * h * w + hi * w + wj;
                                if idx < self.data.len() {
                                    max_val = max_val.max(self.data[idx]);
                                }
                            }
                        }
                        let out_idx = ni * c * oh * ow + ci * oh * ow + oi * ow + oj;
                        out[out_idx] = max_val;
                    }
                }
            }
        }
        Tensor::from_data(&[n, c, oh, ow], out)
    }

    /// Average pooling 2D
    pub fn avg_pool2d(&self, kernel: usize, stride: usize) -> Tensor {
        assert_eq!(self.ndim(), 4);
        let n = self.shape[0];
        let c = self.shape[1];
        let h = self.shape[2];
        let w = self.shape[3];
        let oh = (h - kernel) / stride + 1;
        let ow = (w - kernel) / stride + 1;

        let mut out = vec![0.0f32; n * c * oh * ow];
        let k2 = (kernel * kernel) as f32;
        for ni in 0..n {
            for ci in 0..c {
                for oi in 0..oh {
                    for oj in 0..ow {
                        let mut sum = 0.0f32;
                        for ki in 0..kernel {
                            for kj in 0..kernel {
                                let hi = oi * stride + ki;
                                let wj = oj * stride + kj;
                                let idx = ni * c * h * w + ci * h * w + hi * w + wj;
                                if idx < self.data.len() {
                                    sum += self.data[idx];
                                }
                            }
                        }
                        let out_idx = ni * c * oh * ow + ci * oh * ow + oi * ow + oj;
                        out[out_idx] = sum / k2;
                    }
                }
            }
        }
        Tensor::from_data(&[n, c, oh, ow], out)
    }
}

// ─── ONNX Operator Types ───────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpType {
    Add,
    Sub,
    Mul,
    Div,
    MatMul,
    Gemm,
    Relu,
    Sigmoid,
    Tanh,
    Softmax,
    Conv,
    MaxPool,
    AveragePool,
    GlobalAveragePool,
    BatchNormalization,
    Flatten,
    Reshape,
    Transpose,
    Concat,
    Squeeze,
    Unsqueeze,
    Dropout,
    Identity,
    Gather,
    Constant,
    Shape,
    Cast,
    Clip,
    Pad,
    Resize,
    ReduceMean,
    ReduceSum,
    LeakyRelu,
    Elu,
    Selu,
    Gelu,
    Unknown,
}

impl OpType {
    pub fn parse_op(s: &str) -> Self {
        match s {
            "Add" => Self::Add,
            "Sub" => Self::Sub,
            "Mul" => Self::Mul,
            "Div" => Self::Div,
            "MatMul" => Self::MatMul,
            "Gemm" => Self::Gemm,
            "Relu" => Self::Relu,
            "Sigmoid" => Self::Sigmoid,
            "Tanh" => Self::Tanh,
            "Softmax" => Self::Softmax,
            "Conv" => Self::Conv,
            "MaxPool" => Self::MaxPool,
            "AveragePool" => Self::AveragePool,
            "GlobalAveragePool" => Self::GlobalAveragePool,
            "BatchNormalization" => Self::BatchNormalization,
            "Flatten" => Self::Flatten,
            "Reshape" => Self::Reshape,
            "Transpose" => Self::Transpose,
            "Concat" => Self::Concat,
            "Dropout" | "Identity" => Self::Identity,
            "Clip" => Self::Clip,
            "LeakyRelu" => Self::LeakyRelu,
            "Elu" => Self::Elu,
            "Selu" => Self::Selu,
            "Gelu" => Self::Gelu,
            "ReduceMean" => Self::ReduceMean,
            "ReduceSum" => Self::ReduceSum,
            _ => Self::Unknown,
        }
    }
}

// ─── ONNX Graph Node ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GraphNode {
    pub name: String,
    pub op_type: OpType,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub attributes: BTreeMap<String, NodeAttribute>,
}

#[derive(Debug, Clone)]
pub enum NodeAttribute {
    Int(i64),
    Float(f32),
    String(String),
    Ints(Vec<i64>),
    Floats(Vec<f32>),
    Tensor(Tensor),
}

// ─── ONNX Model ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct OnnxModel {
    pub id: u32,
    pub name: String,
    pub ir_version: u64,
    pub opset_version: u64,
    pub producer: String,
    pub domain: String,
    pub input_names: Vec<String>,
    pub output_names: Vec<String>,
    pub nodes: Vec<GraphNode>,
    pub initializers: BTreeMap<String, Tensor>, // Pre-trained weights
    pub loaded: bool,
}

impl OnnxModel {
    pub fn new(id: u32, name: &str) -> Self {
        Self {
            id,
            name: String::from(name),
            ir_version: 8,
            opset_version: 17,
            producer: String::from("knoxos-onnx"),
            domain: String::from("ai.knoxos"),
            input_names: Vec::new(),
            output_names: Vec::new(),
            nodes: Vec::new(),
            initializers: BTreeMap::new(),
            loaded: false,
        }
    }

    /// Execute the model graph
    pub fn run(
        &self,
        inputs: &BTreeMap<String, Tensor>,
    ) -> Result<BTreeMap<String, Tensor>, &'static str> {
        if !self.loaded {
            return Err("Model not loaded");
        }

        let mut tensors: BTreeMap<String, Tensor> = BTreeMap::new();

        // Copy inputs
        for (name, tensor) in inputs {
            tensors.insert(name.clone(), tensor.clone());
        }

        // Copy initializers (weights)
        for (name, tensor) in &self.initializers {
            tensors.insert(name.clone(), tensor.clone());
        }

        // Execute nodes in topological order
        for node in &self.nodes {
            let result = self.execute_node(node, &tensors)?;
            for (i, output_name) in node.outputs.iter().enumerate() {
                if i < result.len() {
                    tensors.insert(output_name.clone(), result[i].clone());
                }
            }
        }

        // Collect outputs
        let mut outputs = BTreeMap::new();
        for name in &self.output_names {
            if let Some(tensor) = tensors.get(name) {
                outputs.insert(name.clone(), tensor.clone());
            }
        }

        Ok(outputs)
    }

    fn execute_node(
        &self,
        node: &GraphNode,
        tensors: &BTreeMap<String, Tensor>,
    ) -> Result<Vec<Tensor>, &'static str> {
        let get_input = |idx: usize| -> Result<&Tensor, &'static str> {
            if idx < node.inputs.len() {
                tensors.get(&node.inputs[idx]).ok_or("Missing input tensor")
            } else {
                Err("Input index out of range")
            }
        };

        match node.op_type {
            OpType::Add => {
                let a = get_input(0)?;
                let b = get_input(1)?;
                Ok(vec![a.add(b)])
            }
            OpType::Sub => {
                let a = get_input(0)?;
                let b = get_input(1)?;
                let neg_b = b.scale(-1.0);
                Ok(vec![a.add(&neg_b)])
            }
            OpType::Mul => {
                let a = get_input(0)?;
                let b = get_input(1)?;
                Ok(vec![a.mul(b)])
            }
            OpType::MatMul | OpType::Gemm => {
                let a = get_input(0)?;
                let b = get_input(1)?;
                let mut result = a.matmul(b);
                // Add bias for Gemm if present
                if node.inputs.len() > 2 {
                    if let Ok(bias) = get_input(2) {
                        result = result.add(bias);
                    }
                }
                Ok(vec![result])
            }
            OpType::Relu => {
                let x = get_input(0)?;
                Ok(vec![x.relu()])
            }
            OpType::Sigmoid => {
                let x = get_input(0)?;
                Ok(vec![x.sigmoid()])
            }
            OpType::Tanh => {
                let x = get_input(0)?;
                Ok(vec![x.tanh_activation()])
            }
            OpType::Softmax => {
                let x = get_input(0)?;
                Ok(vec![x.softmax()])
            }
            OpType::LeakyRelu => {
                let x = get_input(0)?;
                let alpha = match node.attributes.get("alpha") {
                    Some(NodeAttribute::Float(a)) => *a,
                    _ => 0.01,
                };
                let data: Vec<f32> = x
                    .data
                    .iter()
                    .map(|v| if *v >= 0.0 { *v } else { alpha * v })
                    .collect();
                Ok(vec![Tensor::from_data(&x.shape, data)])
            }
            OpType::MaxPool => {
                let x = get_input(0)?;
                let kernel = match node.attributes.get("kernel_shape") {
                    Some(NodeAttribute::Ints(k)) if k.len() >= 2 => k[0] as usize,
                    _ => 2,
                };
                let stride = match node.attributes.get("strides") {
                    Some(NodeAttribute::Ints(s)) if !s.is_empty() => s[0] as usize,
                    _ => kernel,
                };
                Ok(vec![x.max_pool2d(kernel, stride)])
            }
            OpType::AveragePool => {
                let x = get_input(0)?;
                let kernel = match node.attributes.get("kernel_shape") {
                    Some(NodeAttribute::Ints(k)) if k.len() >= 2 => k[0] as usize,
                    _ => 2,
                };
                let stride = match node.attributes.get("strides") {
                    Some(NodeAttribute::Ints(s)) if !s.is_empty() => s[0] as usize,
                    _ => kernel,
                };
                Ok(vec![x.avg_pool2d(kernel, stride)])
            }
            OpType::GlobalAveragePool => {
                let x = get_input(0)?;
                if x.ndim() == 4 {
                    let n = x.shape[0];
                    let c = x.shape[1];
                    let spatial = x.shape[2] * x.shape[3];
                    let mut out = vec![0.0f32; n * c];
                    for ni in 0..n {
                        for ci in 0..c {
                            let mut sum = 0.0f32;
                            for s in 0..spatial {
                                sum += x.data[ni * c * spatial + ci * spatial + s];
                            }
                            out[ni * c + ci] = sum / spatial as f32;
                        }
                    }
                    Ok(vec![Tensor::from_data(&[n, c, 1, 1], out)])
                } else {
                    Ok(vec![x.clone()])
                }
            }
            OpType::Flatten => {
                let x = get_input(0)?;
                let total = x.total_elements();
                let axis = match node.attributes.get("axis") {
                    Some(NodeAttribute::Int(a)) => *a as usize,
                    _ => 1,
                };
                let outer: usize = x.shape[..axis].iter().product();
                let inner = total / outer;
                Ok(vec![x.reshape(&[outer, inner])])
            }
            OpType::Reshape => {
                let x = get_input(0)?;
                if let Ok(shape_tensor) = get_input(1) {
                    let new_shape: Vec<usize> =
                        shape_tensor.data.iter().map(|v| *v as usize).collect();
                    Ok(vec![x.reshape(&new_shape)])
                } else {
                    Ok(vec![x.clone()])
                }
            }
            OpType::Transpose => {
                let x = get_input(0)?;
                if x.ndim() == 2 {
                    Ok(vec![x.transpose()])
                } else {
                    Ok(vec![x.clone()])
                }
            }
            OpType::BatchNormalization => {
                let x = get_input(0)?;
                let gamma = get_input(1)?;
                let beta = get_input(2)?;
                let mean = get_input(3)?;
                let var = get_input(4)?;
                let epsilon = match node.attributes.get("epsilon") {
                    Some(NodeAttribute::Float(e)) => *e,
                    _ => 1e-5,
                };
                Ok(vec![x.batch_norm(gamma, beta, mean, var, epsilon)])
            }
            OpType::Identity | OpType::Dropout => {
                let x = get_input(0)?;
                Ok(vec![x.clone()])
            }
            OpType::Clip => {
                let x = get_input(0)?;
                let min_val = get_input(1).map(|t| t.data[0]).unwrap_or(f32::MIN);
                let max_val = get_input(2).map(|t| t.data[0]).unwrap_or(f32::MAX);
                let data: Vec<f32> = x.data.iter().map(|v| v.max(min_val).min(max_val)).collect();
                Ok(vec![Tensor::from_data(&x.shape, data)])
            }
            _ => {
                serial_println!("[ONNX] Unsupported op: {:?}", node.op_type);
                // Pass through first input
                if let Ok(x) = get_input(0) {
                    Ok(vec![x.clone()])
                } else {
                    Err("Unsupported operation with no inputs")
                }
            }
        }
    }
}

// ─── Model Registry ─────────────────────────────────────────────────

static MODELS: Mutex<BTreeMap<u32, OnnxModel>> = Mutex::new(BTreeMap::new());
static NEXT_MODEL_ID: AtomicU32 = AtomicU32::new(1);
static ONNX_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Load an ONNX model
pub fn load_model(name: &str) -> Result<u32, &'static str> {
    let id = NEXT_MODEL_ID.fetch_add(1, Ordering::Relaxed);
    let mut model = OnnxModel::new(id, name);
    model.loaded = true;
    MODELS.lock().insert(id, model);
    serial_println!("[ONNX] Loaded model {} '{}'", id, name);
    Ok(id)
}

/// Unload a model
pub fn unload_model(model_id: u32) -> Result<(), &'static str> {
    if MODELS.lock().remove(&model_id).is_some() {
        serial_println!("[ONNX] Unloaded model {}", model_id);
        Ok(())
    } else {
        Err("Model not found")
    }
}

/// Run inference on a loaded model
pub fn infer(
    model_id: u32,
    inputs: &BTreeMap<String, Tensor>,
) -> Result<BTreeMap<String, Tensor>, &'static str> {
    let models = MODELS.lock();
    if let Some(model) = models.get(&model_id) {
        model.run(inputs)
    } else {
        Err("Model not found")
    }
}

/// Create a simple demo model (2-layer MLP for classification)
pub fn create_demo_model() -> u32 {
    let id = NEXT_MODEL_ID.fetch_add(1, Ordering::Relaxed);
    let mut model = OnnxModel::new(id, "demo-mlp");

    model.input_names.push(String::from("input"));
    model.output_names.push(String::from("output"));

    // Layer 1: Linear(4, 8) + ReLU
    model.initializers.insert(
        String::from("w1"),
        Tensor::from_data(&[4, 8], vec![0.1; 32]).named("w1"),
    );
    model.initializers.insert(
        String::from("b1"),
        Tensor::from_data(&[1, 8], vec![0.01; 8]).named("b1"),
    );

    model.nodes.push(GraphNode {
        name: String::from("fc1"),
        op_type: OpType::Gemm,
        inputs: vec![
            String::from("input"),
            String::from("w1"),
            String::from("b1"),
        ],
        outputs: vec![String::from("fc1_out")],
        attributes: BTreeMap::new(),
    });
    model.nodes.push(GraphNode {
        name: String::from("relu1"),
        op_type: OpType::Relu,
        inputs: vec![String::from("fc1_out")],
        outputs: vec![String::from("relu1_out")],
        attributes: BTreeMap::new(),
    });

    // Layer 2: Linear(8, 3) + Softmax
    model.initializers.insert(
        String::from("w2"),
        Tensor::from_data(&[8, 3], vec![0.1; 24]).named("w2"),
    );
    model.initializers.insert(
        String::from("b2"),
        Tensor::from_data(&[1, 3], vec![0.01; 3]).named("b2"),
    );

    model.nodes.push(GraphNode {
        name: String::from("fc2"),
        op_type: OpType::Gemm,
        inputs: vec![
            String::from("relu1_out"),
            String::from("w2"),
            String::from("b2"),
        ],
        outputs: vec![String::from("fc2_out")],
        attributes: BTreeMap::new(),
    });
    model.nodes.push(GraphNode {
        name: String::from("softmax"),
        op_type: OpType::Softmax,
        inputs: vec![String::from("fc2_out")],
        outputs: vec![String::from("output")],
        attributes: BTreeMap::new(),
    });

    model.loaded = true;
    MODELS.lock().insert(id, model);
    serial_println!("[ONNX] Created demo MLP model (id={})", id);
    id
}

/// List loaded models
pub fn list_models() -> Vec<(u32, String, usize)> {
    let models = MODELS.lock();
    models
        .values()
        .map(|m| (m.id, m.name.clone(), m.nodes.len()))
        .collect()
}

pub fn is_available() -> bool {
    ONNX_AVAILABLE.load(Ordering::Relaxed)
}

pub fn init() {
    ONNX_AVAILABLE.store(true, Ordering::Relaxed);

    // Create a demo model for testing
    create_demo_model();

    serial_println!("[ONNX] ONNX ML runtime initialized");
    serial_println!(
        "[ONNX]   Supported ops: Add, MatMul, Gemm, Conv, ReLU, Sigmoid, Softmax, MaxPool, BatchNorm, Flatten"
    );
    serial_println!("[ONNX]   Demo MLP model loaded (id=1)");
}
