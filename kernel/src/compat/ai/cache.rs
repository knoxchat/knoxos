use alloc::string::String;
use alloc::vec::Vec;

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
