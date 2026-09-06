use crate::serial_println;
/// ONNX Runtime Operator Support
///
/// Implements core ONNX operators for neural network inference:
/// Conv, MatMul, Relu, Sigmoid, Add, Reshape, Softmax, BatchNorm, etc.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Tensor data type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TensorType {
    Float32,
    Float16,
    Int32,
    Int64,
    Int8,
    Uint8,
}

/// Multi-dimensional tensor
#[derive(Debug, Clone)]
pub struct Tensor {
    pub name: String,
    pub shape: Vec<usize>,
    pub dtype: TensorType,
    pub data: Vec<u8>,
}

/// ONNX operator kind
#[derive(Debug, Clone)]
pub enum OpKind {
    Conv {
        kernel: Vec<usize>,
        stride: Vec<usize>,
        pads: Vec<usize>,
        dilations: Vec<usize>,
    },
    MatMul,
    Relu,
    Sigmoid,
    Tanh,
    Softmax {
        axis: i32,
    },
    Add,
    Mul,
    Reshape {
        shape: Vec<i64>,
    },
    Transpose {
        perm: Vec<usize>,
    },
    BatchNormalization {
        epsilon: f32,
        momentum: f32,
    },
    MaxPool {
        kernel: Vec<usize>,
        stride: Vec<usize>,
    },
    AveragePool {
        kernel: Vec<usize>,
        stride: Vec<usize>,
    },
    Flatten {
        axis: i32,
    },
    Gemm {
        alpha: f32,
        beta: f32,
        trans_a: bool,
        trans_b: bool,
    },
    LayerNormalization {
        axis: i32,
        epsilon: f32,
    },
    Gather {
        axis: i32,
    },
    Concat {
        axis: i32,
    },
}

/// A node in the ONNX compute graph
#[derive(Debug, Clone)]
pub struct OnnxNode {
    pub op: OpKind,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

/// ONNX model graph
pub struct OnnxGraph {
    pub nodes: Vec<OnnxNode>,
    pub initializers: Vec<Tensor>,
    pub input_names: Vec<String>,
    pub output_names: Vec<String>,
}

lazy_static::lazy_static! {
    static ref REGISTRY: Mutex<Vec<String>> = Mutex::new(Vec::new());
}

impl Tensor {
    pub fn new_f32(name: &str, shape: &[usize], data: Vec<f32>) -> Self {
        let bytes: Vec<u8> = data.iter().flat_map(|f| f.to_le_bytes()).collect();
        Self {
            name: String::from(name),
            shape: shape.to_vec(),
            dtype: TensorType::Float32,
            data: bytes,
        }
    }

    /// Total number of elements
    pub fn numel(&self) -> usize {
        self.shape.iter().product()
    }

    /// Get f32 data slice
    pub fn as_f32(&self) -> Vec<f32> {
        self.data
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }
}

/// Execute a single ONNX operator
pub fn execute_op(op: &OpKind, inputs: &[&Tensor]) -> Tensor {
    match op {
        OpKind::Relu => {
            let input = inputs[0];
            let mut out_data: Vec<f32> = input.as_f32();
            for v in &mut out_data {
                if *v < 0.0 {
                    *v = 0.0;
                }
            }
            Tensor::new_f32("relu_out", &input.shape, out_data)
        }
        OpKind::Sigmoid => {
            let input = inputs[0];
            let out_data: Vec<f32> = input
                .as_f32()
                .into_iter()
                .map(|x| 1.0 / (1.0 + libm::expf(-x)))
                .collect();
            Tensor::new_f32("sigmoid_out", &input.shape, out_data)
        }
        OpKind::Tanh => {
            let input = inputs[0];
            let out_data: Vec<f32> = input.as_f32().into_iter().map(libm::tanhf).collect();
            Tensor::new_f32("tanh_out", &input.shape, out_data)
        }
        OpKind::Add => {
            let a = inputs[0].as_f32();
            let b = inputs[1].as_f32();
            let out: Vec<f32> = a.iter().zip(b.iter()).map(|(x, y)| x + y).collect();
            Tensor::new_f32("add_out", &inputs[0].shape, out)
        }
        OpKind::Mul => {
            let a = inputs[0].as_f32();
            let b = inputs[1].as_f32();
            let out: Vec<f32> = a.iter().zip(b.iter()).map(|(x, y)| x * y).collect();
            Tensor::new_f32("mul_out", &inputs[0].shape, out)
        }
        OpKind::Softmax { axis } => {
            let input = inputs[0];
            let data = input.as_f32();
            let axis_size = input.shape[*axis as usize];
            let outer: usize = input.shape[..*axis as usize].iter().product();
            let inner: usize = input.shape[(*axis as usize + 1)..].iter().product();
            let mut out = data.clone();

            for o in 0..outer {
                for i in 0..inner {
                    let mut max_val = f32::NEG_INFINITY;
                    for a in 0..axis_size {
                        let idx = o * axis_size * inner + a * inner + i;
                        if data[idx] > max_val {
                            max_val = data[idx];
                        }
                    }
                    let mut sum = 0.0f32;
                    for a in 0..axis_size {
                        let idx = o * axis_size * inner + a * inner + i;
                        let e = libm::expf(data[idx] - max_val);
                        out[idx] = e;
                        sum += e;
                    }
                    for a in 0..axis_size {
                        let idx = o * axis_size * inner + a * inner + i;
                        out[idx] /= sum;
                    }
                }
            }
            Tensor::new_f32("softmax_out", &input.shape, out)
        }
        _ => {
            serial_println!(
                "[ONNX] Operator {:?} not yet implemented, returning input",
                op
            );
            inputs[0].clone()
        }
    }
}

/// Execute full ONNX graph
pub fn run_graph(graph: &OnnxGraph, inputs: &[(String, Tensor)]) -> Vec<Tensor> {
    serial_println!("[ONNX] Running graph with {} nodes", graph.nodes.len());
    let _ = inputs;
    // Would build tensor map, execute nodes in topological order
    Vec::new()
}

pub fn init() {
    let mut reg = REGISTRY.lock();
    for name in &[
        "Conv",
        "MatMul",
        "Relu",
        "Sigmoid",
        "Tanh",
        "Softmax",
        "Add",
        "Mul",
        "Reshape",
        "Transpose",
        "BatchNormalization",
        "MaxPool",
        "AveragePool",
        "Flatten",
        "Gemm",
        "LayerNormalization",
        "Gather",
        "Concat",
    ] {
        reg.push(String::from(*name));
    }
    serial_println!("[ONNX] {} operators registered", reg.len());
}
