/// Compositor-facing convenience wrappers around the software GPU model.
use crate::serial_println;

use super::device::{
    resource_create_2d, resource_create_3d, resource_flush, set_scanout, transfer_to_host_2d,
};
use super::protocol::VirtioGpuFormat;
use super::types::Rect;
use super::vq::LIVE_GPU;

/// Check if VirtIO GPU device is initialized and available
pub fn is_available() -> bool {
    LIVE_GPU.lock().as_ref().is_some_and(|g| g.ready)
}

/// Check if Virgl (3D) is available
pub fn is_virgl_available() -> bool {
    false
}

/// Create a 2D resource (convenience wrapper for compositor)
pub fn create_2d_resource(id: u32, width: u32, height: u32, format_id: u32) {
    let format = match format_id {
        67 => VirtioGpuFormat::R8G8B8A8Unorm,
        _ => VirtioGpuFormat::B8G8R8A8Unorm,
    };
    let _ = resource_create_2d(format, width, height);
    serial_println!(
        "[VIRTIO-GPU] create_2d_resource: id={} {}x{} fmt={}",
        id,
        width,
        height,
        format_id
    );
}

/// Create a 3D resource (convenience wrapper for compositor)
pub fn create_3d_resource(id: u32, width: u32, height: u32, depth: u32, format: u32, bind: u32) {
    let _ = resource_create_3d(
        2, // PIPE_TEXTURE_2D
        format, bind, width, height, depth, 1, // array_size
        0, // last_level
        0, // nr_samples
    );
    serial_println!(
        "[VIRTIO-GPU] create_3d_resource: id={} {}x{}x{}",
        id,
        width,
        height,
        depth
    );
}

/// Transfer region to host (convenience wrapper with x, y, w, h)
pub fn transfer_to_host_2d_rect(resource_id: u32, x: u32, y: u32, w: u32, h: u32) {
    let rect = Rect {
        x,
        y,
        width: w,
        height: h,
    };
    let offset = (y * w + x) as u64 * 4;
    let _ = transfer_to_host_2d(resource_id, rect, offset);
}

/// Set scanout with individual parameters (convenience wrapper for compositor)
pub fn set_scanout_rect(scanout_id: u32, resource_id: u32, x: u32, y: u32, w: u32, h: u32) {
    let rect = Rect {
        x,
        y,
        width: w,
        height: h,
    };
    let _ = set_scanout(scanout_id, resource_id, rect);
}

/// Flush a resource region (convenience wrapper for compositor)
pub fn flush_resource(resource_id: u32, x: u32, y: u32, w: u32, h: u32) {
    let rect = Rect {
        x,
        y,
        width: w,
        height: h,
    };
    let _ = resource_flush(resource_id, rect);
}
