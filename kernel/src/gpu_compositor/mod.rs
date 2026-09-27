//! GPU-Accelerated Compositing Engine
//!
//! Provides hardware-accelerated 2D compositing using:
//! 1. VirtIO GPU 2D/3D command submission
//! 2. DRM/KMS scanout + plane compositing
//! 3. Software fallback with SIMD optimization
//!
//! The compositor manages render surfaces, performs alpha blending,
//! blur, and damage-tracked composition to achieve 60fps desktop rendering.
//!
//! Module layout:
//!   types      — surface formats, GpuSurface, blend modes, layers, transforms
//!   cmd        — GPU command buffer
//!   compositor — GpuCompositor state, layer management, frame compose
//!   software   — CPU/SIMD fill, composite, and blur
//!   virtio     — VirtIO GPU 2D/3D backends
//!   drm        — DRM/KMS backend
//!   hdr        — HDR metadata, PQ EOTF, Reinhard tonemap
//!   color      — ICC profiles
//!   vrr        — variable refresh rate detection
//!   animation  — easing and animation ticker
//!   vsync      — page-flip / VSync state
//!   effects    — drop shadow and bloom

use core::sync::atomic::{AtomicBool, Ordering};
use lazy_static::lazy_static;
use spin::Mutex;

mod animation;
mod cmd;
mod color;
mod compositor;
mod drm;
mod effects;
mod hdr;
mod software;
mod types;
mod virtio;
mod vrr;
mod vsync;

pub use animation::*;
pub use cmd::*;
pub use color::*;
pub use compositor::*;
pub use effects::*;
pub use hdr::*;
pub use types::*;
pub use vrr::*;
pub use vsync::*;

lazy_static! {
    pub static ref GPU_COMPOSITOR: Mutex<GpuCompositor> =
        Mutex::new(GpuCompositor::new(1920, 1080, CompBackend::Software));
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Select the best available backend
fn detect_backend() -> CompBackend {
    // Check for VirtIO GPU 3D first (Virgl)
    if crate::virtio_gpu::is_virgl_available() {
        return CompBackend::VirtioGpu3D;
    }
    // Check for VirtIO GPU 2D
    if crate::virtio_gpu::is_available() {
        return CompBackend::VirtioGpu2D;
    }
    // Check for DRM-capable GPU on PCI bus
    let gpus = crate::pcie_ecam::find_by_class(0x03, 0x00);
    for dev in &gpus {
        // QEMU QXL (0x1B36:0x0100) or std VGA (0x1234:0x1111) support DRM-like ops
        if (dev.vendor_id == 0x1B36 && dev.device_id == 0x0100)
            || (dev.vendor_id == 0x1234 && dev.device_id == 0x1111)
        {
            return CompBackend::DrmKms;
        }
    }
    // Software fallback
    CompBackend::Software
}

/// Initialize GPU-accelerated compositing
pub fn init() {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }

    let backend = detect_backend();
    let mut comp = GPU_COMPOSITOR.lock();
    comp.backend = backend;

    crate::serial_println!("[gpu_compositor] Initialized with {:?} backend", backend);
    crate::serial_println!(
        "[gpu_compositor] Display: {}×{}",
        comp.display_width,
        comp.display_height
    );
}
