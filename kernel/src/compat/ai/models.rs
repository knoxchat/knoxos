use alloc::string::String;
use alloc::vec::Vec;

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
