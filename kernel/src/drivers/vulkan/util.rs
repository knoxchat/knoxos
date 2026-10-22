use alloc::string::String;
use core::sync::atomic::Ordering;

use super::state::{
    BUFFERS, DEVICES, IMAGES, INSTANCES, PIPELINES, TOTAL_ALLOCATIONS, TOTAL_MEMORY_BYTES,
};

pub const fn vk_make_api_version(variant: u32, major: u32, minor: u32, patch: u32) -> u32 {
    (variant << 29) | (major << 22) | (minor << 12) | patch
}

/// Get Vulkan statistics
pub fn vulkan_stats() -> String {
    alloc::format!(
        "Vulkan Stats:\n  Instances: {}\n  Devices: {}\n  Pipelines: {}\n  Buffers: {}\n  Images: {}\n  Total allocations: {}\n  Total memory: {} bytes",
        INSTANCES.lock().len(),
        DEVICES.lock().len(),
        PIPELINES.lock().len(),
        BUFFERS.lock().len(),
        IMAGES.lock().len(),
        TOTAL_ALLOCATIONS.load(Ordering::Relaxed),
        TOTAL_MEMORY_BYTES.load(Ordering::Relaxed),
    )
}
