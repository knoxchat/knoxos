/// Virtio GPU Driver
/// Implements the VirtIO GPU device for hardware-accelerated rendering
///
/// Features:
/// - VirtIO GPU device discovery and initialization
/// - 2D framebuffer scanout with RESOURCE_CREATE_2D
/// - 3D Virgl rendering context (OpenGL ES 2.0)
/// - Multiple display heads / scanouts
/// - Cursor plane with hotspot
/// - DMA-BUF resource sharing
/// - Display info and EDID queries
/// - Transfer to/from host resources
/// - Fence-based synchronization
/// - Capset negotiation (virgl, venus/vulkan)
///
/// Module layout:
///   protocol  — command opcodes, pixel formats, feature bits, headers
///   types     — Rect, resources, scanouts, contexts, cursor state
///   pci       — PCI discovery and VirtIO capability MMIO
///   vq        — virtqueues and live modern/legacy transport
///   cmds      — live controlq/cursorq commands and desktop present
///   device    — software device model and 2D/3D/cursor APIs
///   api       — compositor convenience wrappers
///   gates     — GATE_F8 / GATE_GPU serial markers
mod api;
mod cmds;
mod device;
mod gates;
mod pci;
mod protocol;
mod types;
mod vq;

pub use api::{
    create_2d_resource, create_3d_resource, flush_resource, is_available, is_virgl_available,
    set_scanout_rect, transfer_to_host_2d_rect,
};
pub use cmds::{
    cursor_live, hw_move_cursor, pointer_plane_live, present_desktop_fb, scanout_live, scanout_size,
};
pub use device::{
    VirtioGpuDevice, accelerated_fill, composite_blit, ctx_attach_resource, ctx_create,
    ctx_destroy, dma_blit, get_capset_info, get_display_info, move_cursor, resource_attach_backing,
    resource_create_2d, resource_create_3d, resource_flush, resource_unref, set_scanout, submit_3d,
    transfer_to_host_2d, update_cursor,
};
pub use gates::{
    GATE_F8_CURSOR, GATE_F8_SCANOUT, GATE_GPU_MARKER, GATE_GPU_POINTER, GATE_GPU_STACK,
    GATE_GPUI_GPU_SCANOUT,
};
pub use pci::is_primary_display;
pub use protocol::{
    VIRTIO_GPU_F_CONTEXT_INIT, VIRTIO_GPU_F_EDID, VIRTIO_GPU_F_RESOURCE_BLOB,
    VIRTIO_GPU_F_RESOURCE_UUID, VIRTIO_GPU_F_VIRGL, VIRTIO_GPU_MAX_SCANOUTS, VirtioGpuCmd,
    VirtioGpuFormat,
};
pub use types::{
    CapsetInfo, CursorState, DisplayInfo, EdidInfo, Rect, RenderContext, Resource2D, Resource3D,
    Scanout,
};

use crate::serial_println;

use cmds::scanout_self_test;
use device::seed_software_device;
use pci::{enable_pci_bus_master, find_virtio_gpu};
use vq::{LIVE_GPU, init_legacy, init_modern};

/// Initialize the VirtIO GPU device
pub fn init() {
    serial_println!("[VIRTIO-GPU] Scanning PCI for a GPU virtqueue");

    let Some((bus, dev, func, bar0)) = find_virtio_gpu() else {
        serial_println!("[VIRTIO-GPU] No virtio-gpu device on PCI (add -vga virtio)");
        return;
    };

    enable_pci_bus_master(bus, dev, func);

    let live = if bar0 & 1 == 0 {
        init_modern(bus, dev, func).or_else(|| {
            serial_println!("[VIRTIO-GPU] modern transport failed; trying legacy I/O");
            None
        })
    } else {
        init_legacy((bar0 & 0xFFFC) as u16)
    }
    .or_else(|| {
        if bar0 & 1 != 0 {
            None
        } else {
            init_legacy((bar0 & 0xFFFC) as u16)
        }
    });

    let Some(mut gpu) = live else {
        serial_println!("[VIRTIO-GPU] virtqueue setup failed");
        return;
    };

    serial_println!(
        "[VIRTIO-GPU] {} transport ready (ctrl qsz={} cursor qsz={})",
        if gpu.modern { "modern" } else { "legacy" },
        gpu.ctrl.size,
        gpu.cursor.size
    );

    let _ = scanout_self_test(&mut gpu);
    seed_software_device(gpu.scanout_w, gpu.scanout_h);
    *LIVE_GPU.lock() = Some(gpu);
}
