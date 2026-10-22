/// Vulkan Driver Framework
///
/// Provides a Vulkan 1.3-compatible driver interface for GPU-accelerated rendering.
/// Implements the core Vulkan object model with software fallback for non-GPU environments.
///
/// Features:
///   - VkInstance, VkPhysicalDevice, VkDevice, VkQueue
///   - VkCommandBuffer with command recording
///   - VkPipeline (Graphics + Compute)
///   - VkRenderPass & VkFramebuffer
///   - VkDescriptorSet for resource binding
///   - VkSwapchain for presentation
///   - VkShaderModule (SPIR-V bytecode)
///   - VkImage / VkBuffer / VkDeviceMemory
///   - VkFence / VkSemaphore synchronization
///   - Software rasterizer fallback
///   - Integration with DRM/KMS
///
/// Split into submodules for maintainability:
///   result    — VkResult codes
///   handle    — type-safe opaque handles
///   enums     — formats, layouts, pipeline and sampler enums
///   command   — recorded command-buffer operations
///   physical  — physical device properties, features, memory types
///   objects   — instance/device/resource data structures
///   state     — global object tables and allocation counters
///   api       — vkCreate* / vkDestroy* / queue submit
///   raster    — software rasterizer fallback
///   util      — API version packing and stats
use core::sync::atomic::Ordering;

use crate::serial_println;

mod api;
mod command;
mod enums;
mod handle;
mod objects;
mod physical;
mod raster;
mod result;
mod state;
mod util;

pub use api::*;
pub use command::*;
pub use enums::*;
pub use handle::*;
pub use objects::*;
pub use physical::*;
pub use raster::*;
pub use result::*;
pub use util::*;

/// Initialize Vulkan subsystem
pub fn init() {
    if state::INITIALIZED.load(Ordering::Relaxed) {
        return;
    }
    state::INITIALIZED.store(true, Ordering::Relaxed);
    serial_println!("[KnoxOS] Vulkan 1.3 driver framework initialized (software rasterizer)");
}
