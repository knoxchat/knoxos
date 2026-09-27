use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;

use crate::serial_println;

use super::command::VkCommand;
use super::enums::{
    VkColorSpace, VkCommandBufferLevel, VkCullMode, VkFilter, VkFormat, VkFrontFace, VkImageLayout,
    VkImageType, VkImageViewType, VkPipelineBindPoint, VkPolygonMode, VkPresentMode,
    VkPrimitiveTopology, VkSamplerAddressMode,
};
use super::handle::{
    VkBuffer, VkCommandBuffer, VkCommandPool, VkDescriptorSetLayout, VkDevice, VkDeviceMemory,
    VkFence, VkFramebuffer, VkImage, VkImageView, VkInstance, VkPhysicalDevice, VkPipeline,
    VkPipelineLayout, VkQueue, VkRenderPass, VkSampler, VkSemaphore, VkShaderModule, VkSurface,
    VkSwapchain, alloc_handle,
};
use super::objects::{
    AttachmentDescription, BufferData, CommandBufferData, CommandPoolData,
    DescriptorSetLayoutBinding, DescriptorSetLayoutData, DeviceData, FenceData, FramebufferData,
    ImageData, ImageViewData, InstanceData, MemoryData, PipelineData, PipelineLayoutData,
    PushConstantRange, QueueData, RenderPassData, SamplerData, SemaphoreData, ShaderModuleData,
    SubpassDescription, SwapchainData,
};
use super::physical::{
    VK_QUEUE_COMPUTE_BIT, VK_QUEUE_GRAPHICS_BIT, VK_QUEUE_TRANSFER_BIT, VkPhysicalDeviceFeatures,
    VkPhysicalDeviceLimits, VkPhysicalDeviceProperties, VkPhysicalDeviceType,
    VkQueueFamilyProperties,
};
use super::raster::execute_command;
use super::result::VkResult;
use super::state::{
    BUFFERS, COMMAND_BUFFERS, COMMAND_POOLS, DESCRIPTOR_SET_LAYOUTS, DEVICES, FENCES, FRAMEBUFFERS,
    IMAGE_VIEWS, IMAGES, INSTANCES, MEMORY, PIPELINE_LAYOUTS, PIPELINES, RENDER_PASSES, SAMPLERS,
    SEMAPHORES, SHADER_MODULES, SWAPCHAINS, TOTAL_ALLOCATIONS, TOTAL_MEMORY_BYTES,
};
use super::util::vk_make_api_version;

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
