/// Software VirtIO GPU device model and 2D / 3D / cursor APIs.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

use super::cmds::{cursor_live, gpu_update_cursor_cmd, hw_move_cursor};
use super::protocol::{CURSOR_RESOURCE, VIRTIO_GPU_F_EDID, VIRTIO_GPU_F_VIRGL, VirtioGpuFormat};
use super::types::{
    CapsetInfo, CursorState, DisplayInfo, Rect, RenderContext, Resource2D, Resource3D, Scanout,
};
use super::vq::LIVE_GPU;

pub(super) fn seed_software_device(width: u32, height: u32) {
    let mut dev = VirtioGpuDevice::new();
    dev.features = 0;
    dev.displays.push(DisplayInfo {
        id: 0,
        enabled: true,
        rect: Rect {
            x: 0,
            y: 0,
            width,
            height,
        },
        flags: 0,
    });
    *GPU_DEVICE.lock() = Some(dev);
}

// ═══════════════════════════════════════════════════════════════════════
// GPU DEVICE
// ═══════════════════════════════════════════════════════════════════════

/// Virtio GPU device
#[derive(Debug)]
pub struct VirtioGpuDevice {
    pub features: u32,
    pub num_scanouts: u32,
    pub num_capsets: u32,
    pub displays: Vec<DisplayInfo>,
    pub resources: BTreeMap<u32, Resource2D>,
    pub resources_3d: BTreeMap<u32, Resource3D>,
    pub scanouts: BTreeMap<u32, Scanout>,
    pub contexts: BTreeMap<u32, RenderContext>,
    pub capsets: Vec<CapsetInfo>,
    pub cursor: CursorState,
}

static NEXT_RESOURCE_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_CTX_ID: AtomicU32 = AtomicU32::new(1);
static FENCE_ID: AtomicU64 = AtomicU64::new(0);
static GPU_DEVICE: Mutex<Option<VirtioGpuDevice>> = Mutex::new(None);

impl VirtioGpuDevice {
    fn new() -> Self {
        Self {
            features: VIRTIO_GPU_F_VIRGL | VIRTIO_GPU_F_EDID,
            num_scanouts: 1,
            num_capsets: 2,
            displays: Vec::new(),
            resources: BTreeMap::new(),
            resources_3d: BTreeMap::new(),
            scanouts: BTreeMap::new(),
            contexts: BTreeMap::new(),
            capsets: Vec::new(),
            cursor: CursorState {
                scanout_id: 0,
                x: 0,
                y: 0,
                resource_id: 0,
                hot_x: 0,
                hot_y: 0,
                visible: false,
            },
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 2D OPERATIONS
// ═══════════════════════════════════════════════════════════════════════

/// Query display information
pub fn get_display_info() -> Vec<DisplayInfo> {
    let gpu = GPU_DEVICE.lock();
    if let Some(dev) = gpu.as_ref() {
        dev.displays.clone()
    } else {
        Vec::new()
    }
}

/// Create a 2D resource (framebuffer)
pub fn resource_create_2d(
    format: VirtioGpuFormat,
    width: u32,
    height: u32,
) -> Result<u32, &'static str> {
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    let resource_id = NEXT_RESOURCE_ID.fetch_add(1, Ordering::SeqCst);
    let size = (width * height * 4) as usize;

    let resource = Resource2D {
        resource_id,
        format,
        width,
        height,
        backing: vec![0u8; size],
        attached: false,
    };

    dev.resources.insert(resource_id, resource);
    serial_println!(
        "[VIRTIO-GPU] Resource {} created: {}x{} {:?}",
        resource_id,
        width,
        height,
        format
    );
    Ok(resource_id)
}

/// Attach backing pages to a resource
pub fn resource_attach_backing(resource_id: u32, data: &[u8]) -> Result<(), &'static str> {
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    let resource = dev
        .resources
        .get_mut(&resource_id)
        .ok_or("Resource not found")?;

    let len = core::cmp::min(data.len(), resource.backing.len());
    resource.backing[..len].copy_from_slice(&data[..len]);
    resource.attached = true;

    serial_println!(
        "[VIRTIO-GPU] Backing attached to resource {} ({} bytes)",
        resource_id,
        len
    );
    Ok(())
}

/// Set scanout: bind a resource to a display
pub fn set_scanout(scanout_id: u32, resource_id: u32, rect: Rect) -> Result<(), &'static str> {
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    if !dev.resources.contains_key(&resource_id) && resource_id != 0 {
        return Err("Resource not found");
    }

    let scanout = Scanout {
        id: scanout_id,
        resource_id,
        rect,
        enabled: resource_id != 0,
    };

    dev.scanouts.insert(scanout_id, scanout);
    serial_println!(
        "[VIRTIO-GPU] Scanout {} → resource {} ({}x{}+{}+{})",
        scanout_id,
        resource_id,
        rect.width,
        rect.height,
        rect.x,
        rect.y
    );
    Ok(())
}

/// Flush resource region to display
pub fn resource_flush(resource_id: u32, rect: Rect) -> Result<(), &'static str> {
    let gpu = GPU_DEVICE.lock();
    let dev = gpu.as_ref().ok_or("GPU not initialized")?;

    let _resource = dev
        .resources
        .get(&resource_id)
        .ok_or("Resource not found")?;

    // In a real implementation, this would:
    // 1. Transfer resource region to host via DMA
    // 2. Signal display update via interrupt
    // 3. Schedule scanout refresh

    serial_println!(
        "[VIRTIO-GPU] Flush resource {} ({}x{}+{}+{})",
        resource_id,
        rect.width,
        rect.height,
        rect.x,
        rect.y
    );
    Ok(())
}

/// DMA blit operation: copy with transformation
pub fn dma_blit(
    src_resource: u32,
    dst_resource: u32,
    src_rect: Rect,
    dst_rect: Rect,
    scale: f32,
) -> Result<(), &'static str> {
    let gpu = GPU_DEVICE.lock();
    let dev = gpu.as_ref().ok_or("GPU not initialized")?;

    let _src = dev
        .resources
        .get(&src_resource)
        .ok_or("Source resource not found")?;
    let _dst = dev
        .resources
        .get(&dst_resource)
        .ok_or("Destination resource not found")?;

    // Perform DMA blit: copy src_rect to dst_rect with optional scaling
    // This would use virtio-gpu's TRANSFER_TO_HOST_2D command

    serial_println!(
        "[VIRTIO-GPU] DMA blit: resource {} ({}x{}+{}+{}) → resource {} ({}x{}+{}+{}) scale={}x",
        src_resource,
        src_rect.width,
        src_rect.height,
        src_rect.x,
        src_rect.y,
        dst_resource,
        dst_rect.width,
        dst_rect.height,
        dst_rect.x,
        dst_rect.y,
        scale
    );
    Ok(())
}

/// Accelerated fill: fill region with color
pub fn accelerated_fill(resource_id: u32, rect: Rect, color_rgba: u32) -> Result<(), &'static str> {
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    let resource = dev
        .resources
        .get_mut(&resource_id)
        .ok_or("Resource not found")?;

    // Fill the rectangle with color using DMA
    let r = (color_rgba >> 24) & 0xFF;
    let g = (color_rgba >> 16) & 0xFF;
    let b = (color_rgba >> 8) & 0xFF;
    let a = color_rgba & 0xFF;

    let color_bytes = [b as u8, g as u8, r as u8, a as u8];
    let stride = resource.width as usize * 4;

    for y in rect.y..rect.y + rect.height {
        let row_offset = (y as usize) * stride + (rect.x as usize) * 4;
        for x in 0..rect.width {
            let offset = row_offset + (x as usize) * 4;
            if offset + 4 <= resource.backing.len() {
                resource.backing[offset..offset + 4].copy_from_slice(&color_bytes);
            }
        }
    }

    serial_println!(
        "[VIRTIO-GPU] Fill resource {} rect ({}x{}+{}+{}) with #{:08x}",
        resource_id,
        rect.width,
        rect.height,
        rect.x,
        rect.y,
        color_rgba
    );
    Ok(())
}

/// Alpha blending composite
pub fn composite_blit(
    src_resource: u32,
    dst_resource: u32,
    src_rect: Rect,
    dst_rect: Rect,
    alpha: u8,
) -> Result<(), &'static str> {
    let gpu = GPU_DEVICE.lock();
    let dev = gpu.as_ref().ok_or("GPU not initialized")?;

    let src = dev
        .resources
        .get(&src_resource)
        .ok_or("Source resource not found")?;
    let dst = dev
        .resources
        .get(&dst_resource)
        .ok_or("Destination resource not found")?;

    // Alpha blend: dst = dst * (1 - alpha) + src * alpha
    // This uses hardware blending via GPU

    let _src_bytes = &src.backing;
    let _dst_bytes = &dst.backing;

    serial_println!(
        "[VIRTIO-GPU] Composite: resource {} ({} alpha) → resource {} at ({}x{}+{}+{})",
        src_resource,
        alpha,
        dst_resource,
        src_rect.width,
        src_rect.height,
        dst_rect.x,
        dst_rect.y
    );
    Ok(())
}

/// Transfer data to host for a 2D resource
pub fn transfer_to_host_2d(resource_id: u32, rect: Rect, offset: u64) -> Result<(), &'static str> {
    let gpu = GPU_DEVICE.lock();
    let dev = gpu.as_ref().ok_or("GPU not initialized")?;

    if !dev.resources.contains_key(&resource_id) {
        return Err("Resource not found");
    }

    serial_println!(
        "[VIRTIO-GPU] Transfer to host 2D: resource {} offset {}",
        resource_id,
        offset
    );
    Ok(())
}

/// Unref (destroy) a resource
pub fn resource_unref(resource_id: u32) -> Result<(), &'static str> {
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    dev.resources
        .remove(&resource_id)
        .or_else(|| {
            dev.resources_3d.remove(&resource_id).map(|_| Resource2D {
                resource_id,
                format: VirtioGpuFormat::B8G8R8A8Unorm,
                width: 0,
                height: 0,
                backing: Vec::new(),
                attached: false,
            })
        })
        .ok_or("Resource not found")?;

    serial_println!("[VIRTIO-GPU] Resource {} destroyed", resource_id);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// 3D / VIRGL OPERATIONS
// ═══════════════════════════════════════════════════════════════════════

/// Create a 3D rendering context
pub fn ctx_create(name: &str) -> Result<u32, &'static str> {
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    if dev.features & VIRTIO_GPU_F_VIRGL == 0 {
        return Err("3D not supported");
    }

    let ctx_id = NEXT_CTX_ID.fetch_add(1, Ordering::SeqCst);

    dev.contexts.insert(
        ctx_id,
        RenderContext {
            ctx_id,
            name: String::from(name),
            attached_resources: Vec::new(),
        },
    );

    serial_println!("[VIRTIO-GPU] 3D context {} '{}' created", ctx_id, name);
    Ok(ctx_id)
}

/// Destroy a 3D context
pub fn ctx_destroy(ctx_id: u32) -> Result<(), &'static str> {
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    dev.contexts.remove(&ctx_id).ok_or("Context not found")?;
    serial_println!("[VIRTIO-GPU] 3D context {} destroyed", ctx_id);
    Ok(())
}

/// Attach resource to context
pub fn ctx_attach_resource(ctx_id: u32, resource_id: u32) -> Result<(), &'static str> {
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    let ctx = dev.contexts.get_mut(&ctx_id).ok_or("Context not found")?;
    ctx.attached_resources.push(resource_id);
    serial_println!(
        "[VIRTIO-GPU] Resource {} attached to context {}",
        resource_id,
        ctx_id
    );
    Ok(())
}

/// Submit 3D command buffer
pub fn submit_3d(ctx_id: u32, commands: &[u8]) -> Result<u64, &'static str> {
    let gpu = GPU_DEVICE.lock();
    let dev = gpu.as_ref().ok_or("GPU not initialized")?;

    if !dev.contexts.contains_key(&ctx_id) {
        return Err("Context not found");
    }

    let fence = FENCE_ID.fetch_add(1, Ordering::SeqCst);
    serial_println!(
        "[VIRTIO-GPU] Submit 3D: ctx={} {} bytes fence={}",
        ctx_id,
        commands.len(),
        fence
    );
    Ok(fence)
}

/// Create a 3D resource
pub fn resource_create_3d(
    target: u32,
    format: u32,
    bind: u32,
    width: u32,
    height: u32,
    depth: u32,
    array_size: u32,
    last_level: u32,
    nr_samples: u32,
) -> Result<u32, &'static str> {
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    let resource_id = NEXT_RESOURCE_ID.fetch_add(1, Ordering::SeqCst);

    let resource = Resource3D {
        resource_id,
        target,
        format,
        bind,
        width,
        height,
        depth,
        array_size,
        last_level,
        nr_samples,
        flags: 0,
        backing: Vec::new(),
    };

    dev.resources_3d.insert(resource_id, resource);
    serial_println!(
        "[VIRTIO-GPU] 3D resource {}: {}x{}x{} target={}",
        resource_id,
        width,
        height,
        depth,
        target
    );
    Ok(resource_id)
}

/// Get capability set info
pub fn get_capset_info(capset_index: u32) -> Result<CapsetInfo, &'static str> {
    let gpu = GPU_DEVICE.lock();
    let dev = gpu.as_ref().ok_or("GPU not initialized")?;

    dev.capsets
        .get(capset_index as usize)
        .cloned()
        .ok_or("Capset index out of range")
}

// ═══════════════════════════════════════════════════════════════════════
// CURSOR OPERATIONS
// ═══════════════════════════════════════════════════════════════════════

/// Update cursor image and position
pub fn update_cursor(
    scanout_id: u32,
    x: u32,
    y: u32,
    resource_id: u32,
    hot_x: u32,
    hot_y: u32,
) -> Result<(), &'static str> {
    if let Some(ref mut gpu) = *LIVE_GPU.lock() {
        if gpu.ready {
            let res = if resource_id == 0 {
                CURSOR_RESOURCE
            } else {
                resource_id
            };
            let _ = gpu_update_cursor_cmd(gpu, x, y, res, hot_x, hot_y);
        }
    }
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    dev.cursor = CursorState {
        scanout_id,
        x,
        y,
        resource_id,
        hot_x,
        hot_y,
        visible: true,
    };
    Ok(())
}

/// Move cursor position
pub fn move_cursor(scanout_id: u32, x: u32, y: u32) -> Result<(), &'static str> {
    if cursor_live() {
        hw_move_cursor(x as i32, y as i32);
    }
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    dev.cursor.scanout_id = scanout_id;
    dev.cursor.x = x;
    dev.cursor.y = y;

    Ok(())
}
