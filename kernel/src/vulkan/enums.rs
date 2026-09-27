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
