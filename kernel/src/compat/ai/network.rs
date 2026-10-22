use alloc::string::String;
use alloc::vec::Vec;

use super::{Tensor, fast_sqrt};

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
