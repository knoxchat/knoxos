use alloc::string::String;
use alloc::vec::Vec;

use super::command::VkCommand;
use super::enums::{
    VkAttachmentLoadOp, VkAttachmentStoreOp, VkColorSpace, VkCommandBufferLevel, VkCullMode,
    VkDescriptorType, VkFilter, VkFormat, VkFrontFace, VkImageLayout, VkImageType, VkImageViewType,
    VkPipelineBindPoint, VkPolygonMode, VkPresentMode, VkPrimitiveTopology, VkSamplerAddressMode,
};
use super::handle::{
    VkBuffer, VkCommandBuffer, VkCommandPool, VkDescriptorSetLayout, VkDevice, VkDeviceMemory,
    VkFence, VkFramebuffer, VkImage, VkImageView, VkInstance, VkPhysicalDevice, VkPipeline,
    VkPipelineLayout, VkQueue, VkRenderPass, VkSampler, VkSemaphore, VkShaderModule, VkSurface,
    VkSwapchain,
};

#[derive(Debug, Clone)]
pub struct AttachmentDescription {
    pub format: VkFormat,
    pub samples: u32,
    pub load_op: VkAttachmentLoadOp,
    pub store_op: VkAttachmentStoreOp,
    pub stencil_load_op: VkAttachmentLoadOp,
    pub stencil_store_op: VkAttachmentStoreOp,
    pub initial_layout: VkImageLayout,
    pub final_layout: VkImageLayout,
}

#[derive(Debug, Clone)]
pub struct SubpassDescription {
    pub color_attachments: Vec<u32>,
    pub depth_attachment: Option<u32>,
    pub input_attachments: Vec<u32>,
    pub resolve_attachments: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct InstanceData {
    pub handle: VkInstance,
    pub app_name: String,
    pub engine_name: String,
    pub api_version: u32,
    pub physical_devices: Vec<VkPhysicalDevice>,
}

#[derive(Debug, Clone)]
pub struct DeviceData {
    pub handle: VkDevice,
    pub physical_device: VkPhysicalDevice,
    pub queues: Vec<QueueData>,
}

#[derive(Debug, Clone)]
pub struct QueueData {
    pub handle: VkQueue,
    pub family_index: u32,
    pub queue_index: u32,
}

#[derive(Debug, Clone)]
pub struct CommandPoolData {
    pub handle: VkCommandPool,
    pub device: VkDevice,
    pub queue_family_index: u32,
    pub command_buffers: Vec<VkCommandBuffer>,
}

#[derive(Debug, Clone)]
pub struct CommandBufferData {
    pub handle: VkCommandBuffer,
    pub pool: VkCommandPool,
    pub level: VkCommandBufferLevel,
    pub recording: bool,
    pub commands: Vec<VkCommand>,
}

#[derive(Debug, Clone)]
pub struct RenderPassData {
    pub handle: VkRenderPass,
    pub attachments: Vec<AttachmentDescription>,
    pub subpasses: Vec<SubpassDescription>,
}

#[derive(Debug, Clone)]
pub struct FramebufferData {
    pub handle: VkFramebuffer,
    pub render_pass: VkRenderPass,
    pub attachments: Vec<VkImageView>,
    pub width: u32,
    pub height: u32,
    pub layers: u32,
}

#[derive(Debug, Clone)]
pub struct ShaderModuleData {
    pub handle: VkShaderModule,
    pub code: Vec<u32>, // SPIR-V
    pub entry_point: String,
}

#[derive(Debug, Clone)]
pub struct PipelineData {
    pub handle: VkPipeline,
    pub bind_point: VkPipelineBindPoint,
    pub layout: VkPipelineLayout,
    pub shaders: Vec<VkShaderModule>,
    pub topology: VkPrimitiveTopology,
    pub polygon_mode: VkPolygonMode,
    pub cull_mode: VkCullMode,
    pub front_face: VkFrontFace,
    pub depth_test: bool,
    pub depth_write: bool,
    pub blend_enable: bool,
}

#[derive(Debug, Clone)]
pub struct BufferData {
    pub handle: VkBuffer,
    pub size: u64,
    pub usage: u32,
    pub memory: Option<VkDeviceMemory>,
    pub data: Vec<u8>,
}

pub const VK_BUFFER_USAGE_TRANSFER_SRC_BIT: u32 = 0x01;
pub const VK_BUFFER_USAGE_TRANSFER_DST_BIT: u32 = 0x02;
pub const VK_BUFFER_USAGE_UNIFORM_BUFFER_BIT: u32 = 0x10;
pub const VK_BUFFER_USAGE_STORAGE_BUFFER_BIT: u32 = 0x20;
pub const VK_BUFFER_USAGE_INDEX_BUFFER_BIT: u32 = 0x40;
pub const VK_BUFFER_USAGE_VERTEX_BUFFER_BIT: u32 = 0x80;

#[derive(Debug, Clone)]
pub struct ImageData {
    pub handle: VkImage,
    pub image_type: VkImageType,
    pub format: VkFormat,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub mip_levels: u32,
    pub array_layers: u32,
    pub samples: u32,
    pub layout: VkImageLayout,
    pub memory: Option<VkDeviceMemory>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ImageViewData {
    pub handle: VkImageView,
    pub image: VkImage,
    pub view_type: VkImageViewType,
    pub format: VkFormat,
}

#[derive(Debug, Clone)]
pub struct SamplerData {
    pub handle: VkSampler,
    pub mag_filter: VkFilter,
    pub min_filter: VkFilter,
    pub address_mode_u: VkSamplerAddressMode,
    pub address_mode_v: VkSamplerAddressMode,
    pub address_mode_w: VkSamplerAddressMode,
    pub anisotropy_enable: bool,
    pub max_anisotropy: f32,
    pub mip_lod_bias: f32,
    pub min_lod: f32,
    pub max_lod: f32,
}

#[derive(Debug, Clone)]
pub struct MemoryData {
    pub handle: VkDeviceMemory,
    pub size: u64,
    pub memory_type_index: u32,
    pub data: Vec<u8>,
    pub mapped: bool,
}

#[derive(Debug, Clone)]
pub struct FenceData {
    pub handle: VkFence,
    pub signaled: bool,
}

#[derive(Debug, Clone)]
pub struct SemaphoreData {
    pub handle: VkSemaphore,
    pub signaled: bool,
}

#[derive(Debug, Clone)]
pub struct SwapchainData {
    pub handle: VkSwapchain,
    pub surface: VkSurface,
    pub format: VkFormat,
    pub color_space: VkColorSpace,
    pub present_mode: VkPresentMode,
    pub width: u32,
    pub height: u32,
    pub image_count: u32,
    pub images: Vec<VkImage>,
    pub current_index: u32,
}

#[derive(Debug, Clone)]
pub struct DescriptorSetLayoutData {
    pub handle: VkDescriptorSetLayout,
    pub bindings: Vec<DescriptorSetLayoutBinding>,
}

#[derive(Debug, Clone)]
pub struct DescriptorSetLayoutBinding {
    pub binding: u32,
    pub descriptor_type: VkDescriptorType,
    pub descriptor_count: u32,
    pub stage_flags: u32,
}

#[derive(Debug, Clone)]
pub struct PipelineLayoutData {
    pub handle: VkPipelineLayout,
    pub set_layouts: Vec<VkDescriptorSetLayout>,
    pub push_constant_ranges: Vec<PushConstantRange>,
}

#[derive(Debug, Clone)]
pub struct PushConstantRange {
    pub stage_flags: u32,
    pub offset: u32,
    pub size: u32,
}
