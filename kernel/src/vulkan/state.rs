use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicBool, AtomicU64};
use spin::Mutex;

use super::objects::{
    BufferData, CommandBufferData, CommandPoolData, DescriptorSetLayoutData, DeviceData, FenceData,
    FramebufferData, ImageData, ImageViewData, InstanceData, MemoryData, PipelineData,
    PipelineLayoutData, RenderPassData, SamplerData, SemaphoreData, ShaderModuleData,
    SwapchainData,
};

lazy_static::lazy_static! {
    pub(crate) static ref INSTANCES: Mutex<BTreeMap<u64, InstanceData>> = Mutex::new(BTreeMap::new());
    pub(crate) static ref DEVICES: Mutex<BTreeMap<u64, DeviceData>> = Mutex::new(BTreeMap::new());
    pub(crate) static ref COMMAND_POOLS: Mutex<BTreeMap<u64, CommandPoolData>> = Mutex::new(BTreeMap::new());
    pub(crate) static ref COMMAND_BUFFERS: Mutex<BTreeMap<u64, CommandBufferData>> = Mutex::new(BTreeMap::new());
    pub(crate) static ref RENDER_PASSES: Mutex<BTreeMap<u64, RenderPassData>> = Mutex::new(BTreeMap::new());
    pub(crate) static ref FRAMEBUFFERS: Mutex<BTreeMap<u64, FramebufferData>> = Mutex::new(BTreeMap::new());
    pub(crate) static ref SHADER_MODULES: Mutex<BTreeMap<u64, ShaderModuleData>> = Mutex::new(BTreeMap::new());
    pub(crate) static ref PIPELINES: Mutex<BTreeMap<u64, PipelineData>> = Mutex::new(BTreeMap::new());
    pub(crate) static ref BUFFERS: Mutex<BTreeMap<u64, BufferData>> = Mutex::new(BTreeMap::new());
    pub(crate) static ref IMAGES: Mutex<BTreeMap<u64, ImageData>> = Mutex::new(BTreeMap::new());
    pub(crate) static ref IMAGE_VIEWS: Mutex<BTreeMap<u64, ImageViewData>> = Mutex::new(BTreeMap::new());
    pub(crate) static ref SAMPLERS: Mutex<BTreeMap<u64, SamplerData>> = Mutex::new(BTreeMap::new());
    pub(crate) static ref MEMORY: Mutex<BTreeMap<u64, MemoryData>> = Mutex::new(BTreeMap::new());
    pub(crate) static ref FENCES: Mutex<BTreeMap<u64, FenceData>> = Mutex::new(BTreeMap::new());
    pub(crate) static ref SEMAPHORES: Mutex<BTreeMap<u64, SemaphoreData>> = Mutex::new(BTreeMap::new());
    pub(crate) static ref SWAPCHAINS: Mutex<BTreeMap<u64, SwapchainData>> = Mutex::new(BTreeMap::new());
    pub(crate) static ref PIPELINE_LAYOUTS: Mutex<BTreeMap<u64, PipelineLayoutData>> = Mutex::new(BTreeMap::new());
    pub(crate) static ref DESCRIPTOR_SET_LAYOUTS: Mutex<BTreeMap<u64, DescriptorSetLayoutData>> = Mutex::new(BTreeMap::new());
}

pub(crate) static INITIALIZED: AtomicBool = AtomicBool::new(false);
pub(crate) static TOTAL_ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
pub(crate) static TOTAL_MEMORY_BYTES: AtomicU64 = AtomicU64::new(0);
