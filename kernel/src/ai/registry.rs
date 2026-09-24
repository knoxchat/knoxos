use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use super::{Layer, NeuralNetwork, Tensor};

/// Model registry
lazy_static::lazy_static! {
    pub(crate) static ref MODELS: Mutex<BTreeMap<u64, NeuralNetwork>> = Mutex::new(BTreeMap::new());
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
