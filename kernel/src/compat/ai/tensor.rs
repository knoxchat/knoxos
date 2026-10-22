use alloc::vec;
use alloc::vec::Vec;

use super::fast_exp;

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
