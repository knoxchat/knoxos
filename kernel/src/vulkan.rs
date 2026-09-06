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
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// VULKAN RESULT CODES
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum VkResult {
    Success = 0,
    NotReady = 1,
    Timeout = 2,
    Incomplete = 5,
    ErrorOutOfHostMemory = -1,
    ErrorOutOfDeviceMemory = -2,
    ErrorInitializationFailed = -3,
    ErrorDeviceLost = -4,
    ErrorMemoryMapFailed = -5,
    ErrorLayerNotPresent = -6,
    ErrorExtensionNotPresent = -7,
    ErrorFeatureNotPresent = -8,
    ErrorIncompatibleDriver = -9,
    ErrorTooManyObjects = -10,
    ErrorFormatNotSupported = -11,
    ErrorSurfaceLost = -1000000000,
    ErrorOutOfDate = -1000001004,
    SuboptimalKhr = 1000001003,
}

// ═══════════════════════════════════════════════════════════════════════
// VULKAN HANDLES (type-safe opaque handles)
// ═══════════════════════════════════════════════════════════════════════

macro_rules! vk_handle {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u64);
        impl $name {
            pub const NULL: Self = Self(0);
            pub fn is_null(&self) -> bool {
                self.0 == 0
            }
        }
    };
}

vk_handle!(VkInstance);
vk_handle!(VkPhysicalDevice);
vk_handle!(VkDevice);
vk_handle!(VkQueue);
vk_handle!(VkCommandPool);
vk_handle!(VkCommandBuffer);
vk_handle!(VkRenderPass);
vk_handle!(VkFramebuffer);
vk_handle!(VkPipeline);
vk_handle!(VkPipelineLayout);
vk_handle!(VkShaderModule);
vk_handle!(VkDescriptorSetLayout);
vk_handle!(VkDescriptorPool);
vk_handle!(VkDescriptorSet);
vk_handle!(VkBuffer);
vk_handle!(VkImage);
vk_handle!(VkImageView);
vk_handle!(VkSampler);
vk_handle!(VkDeviceMemory);
vk_handle!(VkFence);
vk_handle!(VkSemaphore);
vk_handle!(VkSwapchain);
vk_handle!(VkSurface);

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);
fn alloc_handle() -> u64 {
    NEXT_HANDLE.fetch_add(1, Ordering::Relaxed)
}

// ═══════════════════════════════════════════════════════════════════════
// VULKAN ENUMS
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkFormat {
    Undefined,
    R8Unorm,
    R8G8Unorm,
    R8G8B8Unorm,
    R8G8B8A8Unorm,
    B8G8R8A8Unorm,
    R8G8B8A8Srgb,
    B8G8R8A8Srgb,
    R16Sfloat,
    R32Sfloat,
    R32G32Sfloat,
    R32G32B32Sfloat,
    R32G32B32A32Sfloat,
    D16Unorm,
    D32Sfloat,
    D24UnormS8Uint,
    D32SfloatS8Uint,
}

impl VkFormat {
    pub fn bytes_per_pixel(&self) -> usize {
        match self {
            VkFormat::Undefined => 0,
            VkFormat::R8Unorm => 1,
            VkFormat::R8G8Unorm => 2,
            VkFormat::R8G8B8Unorm => 3,
            VkFormat::R8G8B8A8Unorm
            | VkFormat::B8G8R8A8Unorm
            | VkFormat::R8G8B8A8Srgb
            | VkFormat::B8G8R8A8Srgb
            | VkFormat::R32Sfloat
            | VkFormat::D32Sfloat
            | VkFormat::D24UnormS8Uint => 4,
            VkFormat::R16Sfloat => 2,
            VkFormat::R32G32Sfloat | VkFormat::D32SfloatS8Uint => 8,
            VkFormat::R32G32B32Sfloat => 12,
            VkFormat::R32G32B32A32Sfloat => 16,
            VkFormat::D16Unorm => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkImageLayout {
    Undefined,
    General,
    ColorAttachmentOptimal,
    DepthStencilAttachmentOptimal,
    ShaderReadOnlyOptimal,
    TransferSrcOptimal,
    TransferDstOptimal,
    PresentSrc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkPrimitiveTopology {
    PointList,
    LineList,
    LineStrip,
    TriangleList,
    TriangleStrip,
    TriangleFan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkPolygonMode {
    Fill,
    Line,
    Point,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkCullMode {
    None,
    Front,
    Back,
    FrontAndBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkFrontFace {
    CounterClockwise,
    Clockwise,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkCompareOp {
    Never,
    Less,
    Equal,
    LessOrEqual,
    Greater,
    NotEqual,
    GreaterOrEqual,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkBlendFactor {
    Zero,
    One,
    SrcColor,
    OneMinusSrcColor,
    DstColor,
    OneMinusDstColor,
    SrcAlpha,
    OneMinusSrcAlpha,
    DstAlpha,
    OneMinusDstAlpha,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkBlendOp {
    Add,
    Subtract,
    ReverseSubtract,
    Min,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkPipelineBindPoint {
    Graphics,
    Compute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkDescriptorType {
    Sampler,
    CombinedImageSampler,
    SampledImage,
    StorageImage,
    UniformTexelBuffer,
    StorageTexelBuffer,
    UniformBuffer,
    StorageBuffer,
    UniformBufferDynamic,
    StorageBufferDynamic,
    InputAttachment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkShaderStageFlagBits {
    Vertex = 0x01,
    TessControl = 0x02,
    TessEval = 0x04,
    Geometry = 0x08,
    Fragment = 0x10,
    Compute = 0x20,
    AllGraphics = 0x1F,
    All = 0x7FFFFFFF,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkCommandBufferLevel {
    Primary,
    Secondary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkPipelineStageFlagBits {
    TopOfPipe = 0x0001,
    DrawIndirect = 0x0002,
    VertexInput = 0x0004,
    VertexShader = 0x0008,
    FragmentShader = 0x0080,
    EarlyFragmentTests = 0x0100,
    LateFragmentTests = 0x0200,
    ColorAttachmentOutput = 0x0400,
    ComputeShader = 0x0800,
    Transfer = 0x1000,
    BottomOfPipe = 0x2000,
    Host = 0x4000,
    AllGraphics = 0x8000,
    AllCommands = 0x10000,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkAttachmentLoadOp {
    Load,
    Clear,
    DontCare,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkAttachmentStoreOp {
    Store,
    DontCare,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkPresentMode {
    Immediate,
    Mailbox,
    Fifo,
    FifoRelaxed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkColorSpace {
    SrgbNonlinear,
    DisplayP3NonlinearExt,
    ExtendedSrgbLinearExt,
    Bt2020LinearExt,
    HdrMetadataExt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkImageType {
    Type1D,
    Type2D,
    Type3D,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkImageViewType {
    Type1D,
    Type2D,
    Type3D,
    Cube,
    Array1D,
    Array2D,
    CubeArray,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkFilter {
    Nearest,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkSamplerAddressMode {
    Repeat,
    MirroredRepeat,
    ClampToEdge,
    ClampToBorder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VkIndexType {
    Uint16,
    Uint32,
}

// ═══════════════════════════════════════════════════════════════════════
// COMMAND RECORDING
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub enum VkCommand {
    BeginRenderPass {
        render_pass: VkRenderPass,
        framebuffer: VkFramebuffer,
        clear_values: Vec<[f32; 4]>,
    },
    EndRenderPass,
    BindPipeline {
        bind_point: VkPipelineBindPoint,
        pipeline: VkPipeline,
    },
    BindDescriptorSets {
        bind_point: VkPipelineBindPoint,
        layout: VkPipelineLayout,
        sets: Vec<VkDescriptorSet>,
    },
    BindVertexBuffers {
        first_binding: u32,
        buffers: Vec<VkBuffer>,
        offsets: Vec<u64>,
    },
    BindIndexBuffer {
        buffer: VkBuffer,
        offset: u64,
        index_type: VkIndexType,
    },
    Draw {
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    },
    DrawIndexed {
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    },
    Dispatch {
        group_count_x: u32,
        group_count_y: u32,
        group_count_z: u32,
    },
    CopyBuffer {
        src: VkBuffer,
        dst: VkBuffer,
        size: u64,
        src_offset: u64,
        dst_offset: u64,
    },
    CopyBufferToImage {
        src_buffer: VkBuffer,
        dst_image: VkImage,
        layout: VkImageLayout,
    },
    CopyImageToBuffer {
        src_image: VkImage,
        dst_buffer: VkBuffer,
        layout: VkImageLayout,
    },
    PipelineBarrier {
        src_stage: u32,
        dst_stage: u32,
    },
    PushConstants {
        layout: VkPipelineLayout,
        stage_flags: u32,
        offset: u32,
        data: Vec<u8>,
    },
    SetViewport {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        min_depth: f32,
        max_depth: f32,
    },
    SetScissor {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
    BlitImage {
        src: VkImage,
        dst: VkImage,
        filter: VkFilter,
    },
    ClearColorImage {
        image: VkImage,
        color: [f32; 4],
    },
    ClearDepthStencilImage {
        image: VkImage,
        depth: f32,
        stencil: u32,
    },
}

// ═══════════════════════════════════════════════════════════════════════
// PHYSICAL DEVICE
// ═══════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════
// CORE STRUCTURES
// ═══════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════
// GLOBAL STATE
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    static ref INSTANCES: Mutex<BTreeMap<u64, InstanceData>> = Mutex::new(BTreeMap::new());
    static ref DEVICES: Mutex<BTreeMap<u64, DeviceData>> = Mutex::new(BTreeMap::new());
    static ref COMMAND_POOLS: Mutex<BTreeMap<u64, CommandPoolData>> = Mutex::new(BTreeMap::new());
    static ref COMMAND_BUFFERS: Mutex<BTreeMap<u64, CommandBufferData>> = Mutex::new(BTreeMap::new());
    static ref RENDER_PASSES: Mutex<BTreeMap<u64, RenderPassData>> = Mutex::new(BTreeMap::new());
    static ref FRAMEBUFFERS: Mutex<BTreeMap<u64, FramebufferData>> = Mutex::new(BTreeMap::new());
    static ref SHADER_MODULES: Mutex<BTreeMap<u64, ShaderModuleData>> = Mutex::new(BTreeMap::new());
    static ref PIPELINES: Mutex<BTreeMap<u64, PipelineData>> = Mutex::new(BTreeMap::new());
    static ref BUFFERS: Mutex<BTreeMap<u64, BufferData>> = Mutex::new(BTreeMap::new());
    static ref IMAGES: Mutex<BTreeMap<u64, ImageData>> = Mutex::new(BTreeMap::new());
    static ref IMAGE_VIEWS: Mutex<BTreeMap<u64, ImageViewData>> = Mutex::new(BTreeMap::new());
    static ref SAMPLERS: Mutex<BTreeMap<u64, SamplerData>> = Mutex::new(BTreeMap::new());
    static ref MEMORY: Mutex<BTreeMap<u64, MemoryData>> = Mutex::new(BTreeMap::new());
    static ref FENCES: Mutex<BTreeMap<u64, FenceData>> = Mutex::new(BTreeMap::new());
    static ref SEMAPHORES: Mutex<BTreeMap<u64, SemaphoreData>> = Mutex::new(BTreeMap::new());
    static ref SWAPCHAINS: Mutex<BTreeMap<u64, SwapchainData>> = Mutex::new(BTreeMap::new());
    static ref PIPELINE_LAYOUTS: Mutex<BTreeMap<u64, PipelineLayoutData>> = Mutex::new(BTreeMap::new());
    static ref DESCRIPTOR_SET_LAYOUTS: Mutex<BTreeMap<u64, DescriptorSetLayoutData>> = Mutex::new(BTreeMap::new());
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static TOTAL_ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static TOTAL_MEMORY_BYTES: AtomicU64 = AtomicU64::new(0);

// ═══════════════════════════════════════════════════════════════════════
// API FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════

/// Create a Vulkan instance
pub fn vk_create_instance(
    app_name: &str,
    engine_name: &str,
    api_version: u32,
) -> Result<VkInstance, VkResult> {
    let handle = VkInstance(alloc_handle());

    // Create software physical device
    let phys_dev = VkPhysicalDevice(alloc_handle());

    let instance = InstanceData {
        handle,
        app_name: String::from(app_name),
        engine_name: String::from(engine_name),
        api_version,
        physical_devices: vec![phys_dev],
    };

    INSTANCES.lock().insert(handle.0, instance);
    serial_println!("[Vulkan] Instance created: {}", app_name);
    Ok(handle)
}

/// Enumerate physical devices
pub fn vk_enumerate_physical_devices(
    instance: VkInstance,
) -> Result<Vec<VkPhysicalDevice>, VkResult> {
    let instances = INSTANCES.lock();
    match instances.get(&instance.0) {
        Some(inst) => Ok(inst.physical_devices.clone()),
        None => Err(VkResult::ErrorInitializationFailed),
    }
}

/// Get physical device properties
pub fn vk_get_physical_device_properties(
    _phys_dev: VkPhysicalDevice,
) -> VkPhysicalDeviceProperties {
    VkPhysicalDeviceProperties {
        api_version: vk_make_api_version(0, 1, 3, 0),
        driver_version: vk_make_api_version(0, 0, 14, 0),
        vendor_id: 0x4B4E, // "KN" for KnoxOS
        device_id: 0x0001,
        device_type: VkPhysicalDeviceType::Cpu, // Software renderer
        device_name: String::from("KnoxOS Software Rasterizer"),
        limits: VkPhysicalDeviceLimits::software_defaults(),
    }
}

/// Get physical device features
pub fn vk_get_physical_device_features(_phys_dev: VkPhysicalDevice) -> VkPhysicalDeviceFeatures {
    VkPhysicalDeviceFeatures::software_defaults()
}

/// Get queue family properties
pub fn vk_get_queue_family_properties(_phys_dev: VkPhysicalDevice) -> Vec<VkQueueFamilyProperties> {
    vec![
        // Universal queue family (graphics + compute + transfer)
        VkQueueFamilyProperties {
            queue_flags: VK_QUEUE_GRAPHICS_BIT | VK_QUEUE_COMPUTE_BIT | VK_QUEUE_TRANSFER_BIT,
            queue_count: 4,
            timestamp_valid_bits: 64,
            min_image_transfer_granularity: [1, 1, 1],
        },
        // Compute-only queue family
        VkQueueFamilyProperties {
            queue_flags: VK_QUEUE_COMPUTE_BIT | VK_QUEUE_TRANSFER_BIT,
            queue_count: 2,
            timestamp_valid_bits: 64,
            min_image_transfer_granularity: [1, 1, 1],
        },
        // Transfer-only queue family
        VkQueueFamilyProperties {
            queue_flags: VK_QUEUE_TRANSFER_BIT,
            queue_count: 1,
            timestamp_valid_bits: 64,
            min_image_transfer_granularity: [1, 1, 1],
        },
    ]
}

/// Create logical device
pub fn vk_create_device(
    phys_dev: VkPhysicalDevice,
    queue_create_infos: &[(u32, u32)], // (family_index, count)
) -> Result<VkDevice, VkResult> {
    let dev_handle = VkDevice(alloc_handle());

    let mut queues = Vec::new();
    for &(family_index, count) in queue_create_infos {
        for i in 0..count {
            queues.push(QueueData {
                handle: VkQueue(alloc_handle()),
                family_index,
                queue_index: i,
            });
        }
    }

    let device = DeviceData {
        handle: dev_handle,
        physical_device: phys_dev,
        queues,
    };

    DEVICES.lock().insert(dev_handle.0, device);
    serial_println!("[Vulkan] Logical device created");
    Ok(dev_handle)
}

/// Get device queue
pub fn vk_get_device_queue(
    device: VkDevice,
    family_index: u32,
    queue_index: u32,
) -> Result<VkQueue, VkResult> {
    let devices = DEVICES.lock();
    match devices.get(&device.0) {
        Some(dev) => {
            for q in &dev.queues {
                if q.family_index == family_index && q.queue_index == queue_index {
                    return Ok(q.handle);
                }
            }
            Err(VkResult::ErrorInitializationFailed)
        }
        None => Err(VkResult::ErrorDeviceLost),
    }
}

/// Create command pool
pub fn vk_create_command_pool(
    device: VkDevice,
    queue_family_index: u32,
) -> Result<VkCommandPool, VkResult> {
    let handle = VkCommandPool(alloc_handle());
    let pool = CommandPoolData {
        handle,
        device,
        queue_family_index,
        command_buffers: Vec::new(),
    };
    COMMAND_POOLS.lock().insert(handle.0, pool);
    Ok(handle)
}

/// Allocate command buffers
pub fn vk_allocate_command_buffers(
    pool: VkCommandPool,
    level: VkCommandBufferLevel,
    count: u32,
) -> Result<Vec<VkCommandBuffer>, VkResult> {
    let mut result = Vec::new();
    let mut pools = COMMAND_POOLS.lock();
    let mut bufs = COMMAND_BUFFERS.lock();

    for _ in 0..count {
        let handle = VkCommandBuffer(alloc_handle());
        let cb = CommandBufferData {
            handle,
            pool,
            level,
            recording: false,
            commands: Vec::new(),
        };
        bufs.insert(handle.0, cb);
        result.push(handle);
    }

    if let Some(pool_data) = pools.get_mut(&pool.0) {
        pool_data.command_buffers.extend_from_slice(&result);
    }

    Ok(result)
}

/// Begin command buffer recording
pub fn vk_begin_command_buffer(cmd: VkCommandBuffer) -> VkResult {
    let mut bufs = COMMAND_BUFFERS.lock();
    if let Some(cb) = bufs.get_mut(&cmd.0) {
        cb.recording = true;
        cb.commands.clear();
        VkResult::Success
    } else {
        VkResult::ErrorDeviceLost
    }
}

/// End command buffer recording
pub fn vk_end_command_buffer(cmd: VkCommandBuffer) -> VkResult {
    let mut bufs = COMMAND_BUFFERS.lock();
    if let Some(cb) = bufs.get_mut(&cmd.0) {
        cb.recording = false;
        VkResult::Success
    } else {
        VkResult::ErrorDeviceLost
    }
}

/// Record a command
pub fn vk_cmd_record(cmd: VkCommandBuffer, command: VkCommand) -> VkResult {
    let mut bufs = COMMAND_BUFFERS.lock();
    if let Some(cb) = bufs.get_mut(&cmd.0) {
        if !cb.recording {
            return VkResult::ErrorDeviceLost;
        }
        cb.commands.push(command);
        VkResult::Success
    } else {
        VkResult::ErrorDeviceLost
    }
}

/// Create render pass
pub fn vk_create_render_pass(
    attachments: Vec<AttachmentDescription>,
    subpasses: Vec<SubpassDescription>,
) -> Result<VkRenderPass, VkResult> {
    let handle = VkRenderPass(alloc_handle());
    let rp = RenderPassData {
        handle,
        attachments,
        subpasses,
    };
    RENDER_PASSES.lock().insert(handle.0, rp);
    Ok(handle)
}

/// Create framebuffer
pub fn vk_create_framebuffer(
    render_pass: VkRenderPass,
    attachments: Vec<VkImageView>,
    width: u32,
    height: u32,
) -> Result<VkFramebuffer, VkResult> {
    let handle = VkFramebuffer(alloc_handle());
    let fb = FramebufferData {
        handle,
        render_pass,
        attachments,
        width,
        height,
        layers: 1,
    };
    FRAMEBUFFERS.lock().insert(handle.0, fb);
    Ok(handle)
}

/// Create shader module from SPIR-V bytecode
pub fn vk_create_shader_module(
    code: Vec<u32>,
    entry_point: &str,
) -> Result<VkShaderModule, VkResult> {
    let handle = VkShaderModule(alloc_handle());
    let module = ShaderModuleData {
        handle,
        code,
        entry_point: String::from(entry_point),
    };
    SHADER_MODULES.lock().insert(handle.0, module);
    Ok(handle)
}

/// Create descriptor set layout
pub fn vk_create_descriptor_set_layout(
    bindings: Vec<DescriptorSetLayoutBinding>,
) -> Result<VkDescriptorSetLayout, VkResult> {
    let handle = VkDescriptorSetLayout(alloc_handle());
    let layout = DescriptorSetLayoutData { handle, bindings };
    DESCRIPTOR_SET_LAYOUTS.lock().insert(handle.0, layout);
    Ok(handle)
}

/// Create pipeline layout
pub fn vk_create_pipeline_layout(
    set_layouts: Vec<VkDescriptorSetLayout>,
    push_constant_ranges: Vec<PushConstantRange>,
) -> Result<VkPipelineLayout, VkResult> {
    let handle = VkPipelineLayout(alloc_handle());
    let layout = PipelineLayoutData {
        handle,
        set_layouts,
        push_constant_ranges,
    };
    PIPELINE_LAYOUTS.lock().insert(handle.0, layout);
    Ok(handle)
}

/// Create graphics pipeline
pub fn vk_create_graphics_pipeline(
    layout: VkPipelineLayout,
    render_pass: VkRenderPass,
    shaders: Vec<VkShaderModule>,
    topology: VkPrimitiveTopology,
    polygon_mode: VkPolygonMode,
    cull_mode: VkCullMode,
    front_face: VkFrontFace,
    depth_test: bool,
    blend_enable: bool,
) -> Result<VkPipeline, VkResult> {
    let handle = VkPipeline(alloc_handle());
    let pipeline = PipelineData {
        handle,
        bind_point: VkPipelineBindPoint::Graphics,
        layout,
        shaders,
        topology,
        polygon_mode,
        cull_mode,
        front_face,
        depth_test,
        depth_write: depth_test,
        blend_enable,
    };
    PIPELINES.lock().insert(handle.0, pipeline);
    serial_println!("[Vulkan] Graphics pipeline created");
    Ok(handle)
}

/// Create compute pipeline
pub fn vk_create_compute_pipeline(
    layout: VkPipelineLayout,
    shader: VkShaderModule,
) -> Result<VkPipeline, VkResult> {
    let handle = VkPipeline(alloc_handle());
    let pipeline = PipelineData {
        handle,
        bind_point: VkPipelineBindPoint::Compute,
        layout,
        shaders: vec![shader],
        topology: VkPrimitiveTopology::PointList,
        polygon_mode: VkPolygonMode::Fill,
        cull_mode: VkCullMode::None,
        front_face: VkFrontFace::CounterClockwise,
        depth_test: false,
        depth_write: false,
        blend_enable: false,
    };
    PIPELINES.lock().insert(handle.0, pipeline);
    serial_println!("[Vulkan] Compute pipeline created");
    Ok(handle)
}

/// Create buffer
pub fn vk_create_buffer(size: u64, usage: u32) -> Result<VkBuffer, VkResult> {
    let handle = VkBuffer(alloc_handle());
    let buffer = BufferData {
        handle,
        size,
        usage,
        memory: None,
        data: vec![0u8; size as usize],
    };
    BUFFERS.lock().insert(handle.0, buffer);
    TOTAL_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    Ok(handle)
}

/// Create image
pub fn vk_create_image(
    image_type: VkImageType,
    format: VkFormat,
    width: u32,
    height: u32,
    depth: u32,
    mip_levels: u32,
    array_layers: u32,
) -> Result<VkImage, VkResult> {
    let handle = VkImage(alloc_handle());
    let bpp = format.bytes_per_pixel();
    let data_size = (width as usize) * (height as usize) * (depth as usize) * bpp;
    let image = ImageData {
        handle,
        image_type,
        format,
        width,
        height,
        depth,
        mip_levels,
        array_layers,
        samples: 1,
        layout: VkImageLayout::Undefined,
        memory: None,
        data: vec![0u8; data_size],
    };
    IMAGES.lock().insert(handle.0, image);
    TOTAL_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    Ok(handle)
}

/// Create image view
pub fn vk_create_image_view(
    image: VkImage,
    view_type: VkImageViewType,
    format: VkFormat,
) -> Result<VkImageView, VkResult> {
    let handle = VkImageView(alloc_handle());
    let view = ImageViewData {
        handle,
        image,
        view_type,
        format,
    };
    IMAGE_VIEWS.lock().insert(handle.0, view);
    Ok(handle)
}

/// Create sampler
pub fn vk_create_sampler(
    mag_filter: VkFilter,
    min_filter: VkFilter,
    address_mode: VkSamplerAddressMode,
) -> Result<VkSampler, VkResult> {
    let handle = VkSampler(alloc_handle());
    let sampler = SamplerData {
        handle,
        mag_filter,
        min_filter,
        address_mode_u: address_mode,
        address_mode_v: address_mode,
        address_mode_w: address_mode,
        anisotropy_enable: false,
        max_anisotropy: 1.0,
        mip_lod_bias: 0.0,
        min_lod: 0.0,
        max_lod: 1000.0,
    };
    SAMPLERS.lock().insert(handle.0, sampler);
    Ok(handle)
}

/// Allocate device memory
pub fn vk_allocate_memory(size: u64, memory_type_index: u32) -> Result<VkDeviceMemory, VkResult> {
    let handle = VkDeviceMemory(alloc_handle());
    let mem = MemoryData {
        handle,
        size,
        memory_type_index,
        data: vec![0u8; size as usize],
        mapped: false,
    };
    MEMORY.lock().insert(handle.0, mem);
    TOTAL_MEMORY_BYTES.fetch_add(size, Ordering::Relaxed);
    Ok(handle)
}

/// Bind buffer memory
pub fn vk_bind_buffer_memory(buffer: VkBuffer, memory: VkDeviceMemory, _offset: u64) -> VkResult {
    let mut bufs = BUFFERS.lock();
    if let Some(buf) = bufs.get_mut(&buffer.0) {
        buf.memory = Some(memory);
        VkResult::Success
    } else {
        VkResult::ErrorDeviceLost
    }
}

/// Bind image memory
pub fn vk_bind_image_memory(image: VkImage, memory: VkDeviceMemory, _offset: u64) -> VkResult {
    let mut imgs = IMAGES.lock();
    if let Some(img) = imgs.get_mut(&image.0) {
        img.memory = Some(memory);
        VkResult::Success
    } else {
        VkResult::ErrorDeviceLost
    }
}

/// Map memory for CPU access
pub fn vk_map_memory(memory: VkDeviceMemory, offset: u64, size: u64) -> Result<*mut u8, VkResult> {
    let mut mems = MEMORY.lock();
    if let Some(mem) = mems.get_mut(&memory.0) {
        if offset + size > mem.size {
            return Err(VkResult::ErrorMemoryMapFailed);
        }
        mem.mapped = true;
        Ok(mem.data.as_mut_ptr().wrapping_add(offset as usize))
    } else {
        Err(VkResult::ErrorMemoryMapFailed)
    }
}

/// Unmap memory
pub fn vk_unmap_memory(memory: VkDeviceMemory) {
    let mut mems = MEMORY.lock();
    if let Some(mem) = mems.get_mut(&memory.0) {
        mem.mapped = false;
    }
}

/// Create fence
pub fn vk_create_fence(signaled: bool) -> Result<VkFence, VkResult> {
    let handle = VkFence(alloc_handle());
    FENCES
        .lock()
        .insert(handle.0, FenceData { handle, signaled });
    Ok(handle)
}

/// Wait for fence
pub fn vk_wait_for_fences(fences: &[VkFence], wait_all: bool, _timeout: u64) -> VkResult {
    let fence_map = FENCES.lock();
    if wait_all {
        for f in fences {
            if let Some(fd) = fence_map.get(&f.0) {
                if !fd.signaled {
                    return VkResult::Timeout;
                }
            }
        }
    } else {
        for f in fences {
            if let Some(fd) = fence_map.get(&f.0) {
                if fd.signaled {
                    return VkResult::Success;
                }
            }
        }
        return VkResult::Timeout;
    }
    VkResult::Success
}

/// Reset fences
pub fn vk_reset_fences(fences: &[VkFence]) -> VkResult {
    let mut fence_map = FENCES.lock();
    for f in fences {
        if let Some(fd) = fence_map.get_mut(&f.0) {
            fd.signaled = false;
        }
    }
    VkResult::Success
}

/// Create semaphore
pub fn vk_create_semaphore() -> Result<VkSemaphore, VkResult> {
    let handle = VkSemaphore(alloc_handle());
    SEMAPHORES.lock().insert(
        handle.0,
        SemaphoreData {
            handle,
            signaled: false,
        },
    );
    Ok(handle)
}

/// Submit command buffers to queue
pub fn vk_queue_submit(
    queue: VkQueue,
    command_buffers: &[VkCommandBuffer],
    wait_semaphores: &[VkSemaphore],
    signal_semaphores: &[VkSemaphore],
    fence: Option<VkFence>,
) -> VkResult {
    // Execute commands via software renderer
    let bufs = COMMAND_BUFFERS.lock();
    for cb_handle in command_buffers {
        if let Some(cb) = bufs.get(&cb_handle.0) {
            for cmd in &cb.commands {
                execute_command(cmd);
            }
        }
    }

    // Signal semaphores
    let mut sems = SEMAPHORES.lock();
    for s in signal_semaphores {
        if let Some(sem) = sems.get_mut(&s.0) {
            sem.signaled = true;
        }
    }

    // Signal fence
    if let Some(f) = fence {
        let mut fences = FENCES.lock();
        if let Some(fd) = fences.get_mut(&f.0) {
            fd.signaled = true;
        }
    }

    VkResult::Success
}

/// Wait for queue idle
pub fn vk_queue_wait_idle(_queue: VkQueue) -> VkResult {
    // Software renderer completes synchronously
    VkResult::Success
}

/// Wait for device idle
pub fn vk_device_wait_idle(_device: VkDevice) -> VkResult {
    VkResult::Success
}

/// Create swapchain
pub fn vk_create_swapchain(
    surface: VkSurface,
    format: VkFormat,
    color_space: VkColorSpace,
    present_mode: VkPresentMode,
    width: u32,
    height: u32,
    image_count: u32,
) -> Result<VkSwapchain, VkResult> {
    let handle = VkSwapchain(alloc_handle());

    let mut images = Vec::new();
    for _ in 0..image_count {
        let img = vk_create_image(VkImageType::Type2D, format, width, height, 1, 1, 1)?;
        images.push(img);
    }

    let swapchain = SwapchainData {
        handle,
        surface,
        format,
        color_space,
        present_mode,
        width,
        height,
        image_count,
        images,
        current_index: 0,
    };

    SWAPCHAINS.lock().insert(handle.0, swapchain);
    serial_println!(
        "[Vulkan] Swapchain created: {}x{} {} images",
        width,
        height,
        image_count
    );
    Ok(handle)
}

/// Acquire next swapchain image
pub fn vk_acquire_next_image(
    swapchain: VkSwapchain,
    _timeout: u64,
    _semaphore: Option<VkSemaphore>,
    _fence: Option<VkFence>,
) -> Result<u32, VkResult> {
    let mut swapchains = SWAPCHAINS.lock();
    if let Some(sc) = swapchains.get_mut(&swapchain.0) {
        let index = sc.current_index;
        sc.current_index = (sc.current_index + 1) % sc.image_count;
        Ok(index)
    } else {
        Err(VkResult::ErrorDeviceLost)
    }
}

/// Present swapchain image
pub fn vk_queue_present(
    _queue: VkQueue,
    _swapchain: VkSwapchain,
    _image_index: u32,
    _wait_semaphores: &[VkSemaphore],
) -> VkResult {
    // In a real implementation, this would blit the image to the framebuffer
    VkResult::Success
}

/// Destroy instance
pub fn vk_destroy_instance(instance: VkInstance) {
    INSTANCES.lock().remove(&instance.0);
}

/// Destroy device
pub fn vk_destroy_device(device: VkDevice) {
    DEVICES.lock().remove(&device.0);
}

/// Destroy buffer
pub fn vk_destroy_buffer(buffer: VkBuffer) {
    BUFFERS.lock().remove(&buffer.0);
}

/// Destroy image
pub fn vk_destroy_image(image: VkImage) {
    IMAGES.lock().remove(&image.0);
}

/// Destroy pipeline
pub fn vk_destroy_pipeline(pipeline: VkPipeline) {
    PIPELINES.lock().remove(&pipeline.0);
}

/// Destroy render pass
pub fn vk_destroy_render_pass(render_pass: VkRenderPass) {
    RENDER_PASSES.lock().remove(&render_pass.0);
}

/// Destroy framebuffer
pub fn vk_destroy_framebuffer(framebuffer: VkFramebuffer) {
    FRAMEBUFFERS.lock().remove(&framebuffer.0);
}

/// Destroy shader module
pub fn vk_destroy_shader_module(shader_module: VkShaderModule) {
    SHADER_MODULES.lock().remove(&shader_module.0);
}

/// Free memory
pub fn vk_free_memory(memory: VkDeviceMemory) {
    let mut mems = MEMORY.lock();
    if let Some(m) = mems.remove(&memory.0) {
        TOTAL_MEMORY_BYTES.fetch_sub(m.size, Ordering::Relaxed);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SOFTWARE RASTERIZER
// ═══════════════════════════════════════════════════════════════════════

/// Execute a single command (software fallback)
fn execute_command(cmd: &VkCommand) {
    match cmd {
        VkCommand::Draw {
            vertex_count,
            instance_count,
            first_vertex,
            first_instance,
        } => {
            // Software vertex processing: assemble triangles from vertex buffer
            // For a triangle list, every 3 vertices form one triangle
            let total = (*vertex_count) * (*instance_count);
            serial_println!(
                "[vulkan-sw] Draw: {} vertices, {} instances (first_vtx={}, first_inst={})",
                vertex_count,
                instance_count,
                first_vertex,
                first_instance
            );
            // In a full software renderer, we would:
            // 1. Fetch vertices from the bound vertex buffer
            // 2. Run the SPIR-V vertex shader on each vertex
            // 3. Assemble primitives (triangles/lines/points)
            // 4. Clip and rasterize using rasterize_triangle()
            // 5. Run the SPIR-V fragment shader per pixel
            // 6. Write to the bound framebuffer
            let _ = total;
        }
        VkCommand::DrawIndexed {
            index_count,
            instance_count,
            first_index,
            vertex_offset,
            first_instance,
        } => {
            let total = (*index_count) * (*instance_count);
            serial_println!(
                "[vulkan-sw] DrawIndexed: {} indices, {} instances",
                index_count,
                instance_count
            );
            let _ = (total, first_index, vertex_offset, first_instance);
        }
        VkCommand::Dispatch {
            group_count_x,
            group_count_y,
            group_count_z,
        } => {
            let total_groups = (*group_count_x) * (*group_count_y) * (*group_count_z);
            serial_println!(
                "[vulkan-sw] Dispatch: {}x{}x{} = {} groups",
                group_count_x,
                group_count_y,
                group_count_z,
                total_groups
            );
        }
        VkCommand::CopyBuffer {
            src,
            dst,
            size,
            src_offset,
            dst_offset,
        } => {
            let mut bufs = BUFFERS.lock();
            // Extract source data first
            let src_data = bufs.get(&src.0).map(|sb| {
                let start = *src_offset as usize;
                let end = (start + *size as usize).min(sb.data.len());
                sb.data[start..end].to_vec()
            });
            // Write to destination
            if let Some(data) = src_data {
                if let Some(dst_buf) = bufs.get_mut(&dst.0) {
                    let d_start = *dst_offset as usize;
                    let copy_len = data.len().min(dst_buf.data.len().saturating_sub(d_start));
                    dst_buf.data[d_start..d_start + copy_len].copy_from_slice(&data[..copy_len]);
                }
            }
        }
        VkCommand::CopyBufferToImage {
            src_buffer,
            dst_image,
            layout: _,
        } => {
            let bufs = BUFFERS.lock();
            let mut imgs = IMAGES.lock();
            if let (Some(src), Some(dst)) = (bufs.get(&src_buffer.0), imgs.get_mut(&dst_image.0)) {
                let copy_len = src.data.len().min(dst.data.len());
                dst.data[..copy_len].copy_from_slice(&src.data[..copy_len]);
            }
        }
        VkCommand::CopyImageToBuffer {
            src_image,
            dst_buffer,
            layout: _,
        } => {
            let imgs = IMAGES.lock();
            let mut bufs = BUFFERS.lock();
            if let (Some(src), Some(dst)) = (imgs.get(&src_image.0), bufs.get_mut(&dst_buffer.0)) {
                let copy_len = src.data.len().min(dst.data.len());
                dst.data[..copy_len].copy_from_slice(&src.data[..copy_len]);
            }
        }
        VkCommand::ClearColorImage { image, color } => {
            let mut imgs = IMAGES.lock();
            if let Some(img) = imgs.get_mut(&image.0) {
                let bpp = img.format.bytes_per_pixel();
                for chunk in img.data.chunks_mut(bpp) {
                    if bpp >= 4 {
                        chunk[0] = (color[2] * 255.0) as u8; // B
                        chunk[1] = (color[1] * 255.0) as u8; // G
                        chunk[2] = (color[0] * 255.0) as u8; // R
                        if bpp == 4 {
                            chunk[3] = (color[3] * 255.0) as u8; // A
                        }
                    }
                }
            }
        }
        VkCommand::ClearDepthStencilImage {
            image,
            depth,
            stencil,
        } => {
            let mut imgs = IMAGES.lock();
            if let Some(img) = imgs.get_mut(&image.0) {
                // D32_SFLOAT format: write f32 depth to each texel
                for chunk in img.data.chunks_mut(4) {
                    if chunk.len() == 4 {
                        let bytes = depth.to_le_bytes();
                        chunk.copy_from_slice(&bytes);
                    }
                }
                let _ = stencil; // Stencil stored in separate plane if D24S8
            }
        }
        VkCommand::BlitImage { src, dst, filter } => {
            let mut imgs = IMAGES.lock();
            // Simple copy for nearest filter; would do bilinear for linear
            let src_data = imgs.get(&src.0).map(|i| i.data.clone());
            if let (Some(data), Some(dst_img)) = (src_data, imgs.get_mut(&dst.0)) {
                let copy_len = data.len().min(dst_img.data.len());
                dst_img.data[..copy_len].copy_from_slice(&data[..copy_len]);
            }
            let _ = filter;
        }
        VkCommand::BeginRenderPass {
            render_pass: _,
            framebuffer,
            clear_values,
        } => {
            // Clear framebuffer attachments with clear values
            let fbs = FRAMEBUFFERS.lock();
            if let Some(fb) = fbs.get(&framebuffer.0) {
                let mut imgs = IMAGES.lock();
                for (i, &img_handle) in fb.attachments.iter().enumerate() {
                    if let Some(img) = imgs.get_mut(&img_handle.0) {
                        if let Some(clear) = clear_values.get(i) {
                            let bpp = img.format.bytes_per_pixel();
                            for chunk in img.data.chunks_mut(bpp) {
                                if bpp >= 4 {
                                    chunk[0] = (clear[2] * 255.0) as u8;
                                    chunk[1] = (clear[1] * 255.0) as u8;
                                    chunk[2] = (clear[0] * 255.0) as u8;
                                    if bpp == 4 {
                                        chunk[3] = (clear[3] * 255.0) as u8;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        VkCommand::EndRenderPass => { /* State transition only */ }
        VkCommand::BindPipeline { .. } => { /* Records current pipeline for subsequent draws */ }
        VkCommand::BindDescriptorSets { .. } => { /* Records bound descriptors */ }
        VkCommand::BindVertexBuffers { .. } => { /* Records vertex buffer bindings */ }
        VkCommand::BindIndexBuffer { .. } => { /* Records index buffer binding */ }
        VkCommand::PipelineBarrier { .. } => { /* Memory/execution barrier — no-op in SW renderer */
        }
        VkCommand::PushConstants { .. } => { /* Would update push constant memory block */ }
        VkCommand::SetViewport { .. } => { /* Records viewport transform parameters */ }
        VkCommand::SetScissor { .. } => { /* Records scissor rect for clipping */ }
    }
}

/// Software triangle rasterizer (barycentric coordinates)
pub fn rasterize_triangle(
    framebuffer: &mut [u8],
    width: u32,
    height: u32,
    v0: [f32; 4], // x, y, z, w
    v1: [f32; 4],
    v2: [f32; 4],
    color: [u8; 4], // BGRA
) {
    // Compute bounding box
    let min_x = (v0[0].min(v1[0]).min(v2[0]).max(0.0)) as u32;
    let max_x = (v0[0].max(v1[0]).max(v2[0]).min(width as f32 - 1.0)) as u32;
    let min_y = (v0[1].min(v1[1]).min(v2[1]).max(0.0)) as u32;
    let max_y = (v0[1].max(v1[1]).max(v2[1]).min(height as f32 - 1.0)) as u32;

    let area = edge_function(v0, v1, v2);
    if area <= 0.0 {
        return;
    }

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let p = [x as f32 + 0.5, y as f32 + 0.5, 0.0, 1.0];
            let w0 = edge_function(v1, v2, p);
            let w1 = edge_function(v2, v0, p);
            let w2 = edge_function(v0, v1, p);

            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                let offset = ((y * width + x) * 4) as usize;
                if offset + 3 < framebuffer.len() {
                    framebuffer[offset] = color[0];
                    framebuffer[offset + 1] = color[1];
                    framebuffer[offset + 2] = color[2];
                    framebuffer[offset + 3] = color[3];
                }
            }
        }
    }
}

fn edge_function(a: [f32; 4], b: [f32; 4], c: [f32; 4]) -> f32 {
    (c[0] - a[0]) * (b[1] - a[1]) - (c[1] - a[1]) * (b[0] - a[0])
}

/// Software line rasterizer (Bresenham's algorithm)
pub fn rasterize_line(
    framebuffer: &mut [u8],
    width: u32,
    _height: u32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: [u8; 4],
) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx: i32 = if x0 < x1 { 1 } else { -1 };
    let sy: i32 = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut cx = x0;
    let mut cy = y0;

    loop {
        let offset = ((cy as u32 * width + cx as u32) * 4) as usize;
        if offset + 3 < framebuffer.len() {
            framebuffer[offset] = color[0];
            framebuffer[offset + 1] = color[1];
            framebuffer[offset + 2] = color[2];
            framebuffer[offset + 3] = color[3];
        }

        if cx == x1 && cy == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            cx += sx;
        }
        if e2 <= dx {
            err += dx;
            cy += sy;
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// UTILITY
// ═══════════════════════════════════════════════════════════════════════

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

/// Initialize Vulkan subsystem
pub fn init() {
    if INITIALIZED.load(Ordering::Relaxed) {
        return;
    }
    INITIALIZED.store(true, Ordering::Relaxed);
    serial_println!("[KnoxOS] Vulkan 1.3 driver framework initialized (software rasterizer)");
}
