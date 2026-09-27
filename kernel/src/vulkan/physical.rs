use alloc::string::String;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkPhysicalDeviceType {
    Other,
    IntegratedGpu,
    DiscreteGpu,
    VirtualGpu,
    Cpu,
}

#[derive(Debug, Clone)]
pub struct VkPhysicalDeviceProperties {
    pub api_version: u32,
    pub driver_version: u32,
    pub vendor_id: u32,
    pub device_id: u32,
    pub device_type: VkPhysicalDeviceType,
    pub device_name: String,
    pub limits: VkPhysicalDeviceLimits,
}

#[derive(Debug, Clone, Copy)]
pub struct VkPhysicalDeviceLimits {
    pub max_image_dimension_2d: u32,
    pub max_image_dimension_3d: u32,
    pub max_viewports: u32,
    pub max_framebuffer_width: u32,
    pub max_framebuffer_height: u32,
    pub max_color_attachments: u32,
    pub max_compute_work_group_count: [u32; 3],
    pub max_compute_work_group_size: [u32; 3],
    pub max_compute_work_group_invocations: u32,
    pub max_push_constants_size: u32,
    pub max_memory_allocation_count: u32,
    pub max_bound_descriptor_sets: u32,
    pub max_descriptor_set_samplers: u32,
    pub max_descriptor_set_uniform_buffers: u32,
    pub max_descriptor_set_storage_buffers: u32,
    pub max_vertex_input_attributes: u32,
    pub max_vertex_input_bindings: u32,
    pub framebuffer_color_sample_counts: u32,
    pub framebuffer_depth_sample_counts: u32,
    pub min_uniform_buffer_offset_alignment: u64,
    pub min_storage_buffer_offset_alignment: u64,
}

impl VkPhysicalDeviceLimits {
    pub fn software_defaults() -> Self {
        Self {
            max_image_dimension_2d: 8192,
            max_image_dimension_3d: 2048,
            max_viewports: 16,
            max_framebuffer_width: 8192,
            max_framebuffer_height: 8192,
            max_color_attachments: 8,
            max_compute_work_group_count: [65535, 65535, 65535],
            max_compute_work_group_size: [1024, 1024, 64],
            max_compute_work_group_invocations: 1024,
            max_push_constants_size: 256,
            max_memory_allocation_count: 4096,
            max_bound_descriptor_sets: 8,
            max_descriptor_set_samplers: 1024,
            max_descriptor_set_uniform_buffers: 256,
            max_descriptor_set_storage_buffers: 256,
            max_vertex_input_attributes: 32,
            max_vertex_input_bindings: 32,
            framebuffer_color_sample_counts: 0x0F, // 1, 2, 4, 8
            framebuffer_depth_sample_counts: 0x0F,
            min_uniform_buffer_offset_alignment: 256,
            min_storage_buffer_offset_alignment: 32,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VkPhysicalDeviceFeatures {
    pub geometry_shader: bool,
    pub tessellation_shader: bool,
    pub multi_draw_indirect: bool,
    pub sampler_anisotropy: bool,
    pub texture_compression_bc: bool,
    pub shader_float64: bool,
    pub shader_int64: bool,
    pub shader_int16: bool,
    pub depth_clamp: bool,
    pub depth_bias_clamp: bool,
    pub fill_mode_non_solid: bool,
    pub wide_lines: bool,
    pub large_points: bool,
    pub multi_viewport: bool,
    pub robust_buffer_access: bool,
}

impl VkPhysicalDeviceFeatures {
    pub fn software_defaults() -> Self {
        Self {
            geometry_shader: true,
            tessellation_shader: true,
            multi_draw_indirect: true,
            sampler_anisotropy: true,
            texture_compression_bc: false,
            shader_float64: true,
            shader_int64: true,
            shader_int16: true,
            depth_clamp: true,
            depth_bias_clamp: true,
            fill_mode_non_solid: true,
            wide_lines: true,
            large_points: true,
            multi_viewport: true,
            robust_buffer_access: true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VkQueueFamilyProperties {
    pub queue_flags: u32,
    pub queue_count: u32,
    pub timestamp_valid_bits: u32,
    pub min_image_transfer_granularity: [u32; 3],
}

// Queue family flag bits
pub const VK_QUEUE_GRAPHICS_BIT: u32 = 0x01;
pub const VK_QUEUE_COMPUTE_BIT: u32 = 0x02;
pub const VK_QUEUE_TRANSFER_BIT: u32 = 0x04;
pub const VK_QUEUE_SPARSE_BINDING_BIT: u32 = 0x08;

#[derive(Debug, Clone, Copy)]
pub struct VkMemoryType {
    pub property_flags: u32,
    pub heap_index: u32,
}

pub const VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT: u32 = 0x01;
pub const VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT: u32 = 0x02;
pub const VK_MEMORY_PROPERTY_HOST_COHERENT_BIT: u32 = 0x04;
pub const VK_MEMORY_PROPERTY_HOST_CACHED_BIT: u32 = 0x08;

#[derive(Debug, Clone, Copy)]
pub struct VkMemoryHeap {
    pub size: u64,
    pub flags: u32,
}

pub const VK_MEMORY_HEAP_DEVICE_LOCAL_BIT: u32 = 0x01;
