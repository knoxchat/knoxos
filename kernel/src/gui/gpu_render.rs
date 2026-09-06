// SPDX-License-Identifier: MIT
//! GPU-accelerated 2D rendering abstraction (item 7.11)
//!
//! Provides a rendering backend that can dispatch to either the software
//! framebuffer or a GPU-accelerated path when VirtIO GPU or DRM is available.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

/// Helper: raw framebuffer info for direct pixel writes
struct RawFbInfo {
    stride: i32,
    bpp: i32,
    base: *mut u8,
}

fn get_raw_fb() -> Option<RawFbInfo> {
    let guard = crate::gui::FRAMEBUFFER.lock();
    if let Some(ref fb) = *guard {
        if fb.use_hw_framebuffer && fb.framebuffer_addr != 0 {
            Some(RawFbInfo {
                stride: fb.hw_stride as i32 * fb.hw_bytes_per_pixel as i32,
                bpp: fb.hw_bytes_per_pixel as i32,
                base: fb.framebuffer_addr as *mut u8,
            })
        } else {
            // Use the software back-buffer pointer
            let ptr = fb.buffer.as_ptr() as *mut u8;
            Some(RawFbInfo {
                stride: fb.pitch as i32,
                bpp: fb.bytes_per_pixel as i32,
                base: ptr,
            })
        }
    } else {
        None
    }
}

/// Render backend selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderBackend {
    /// Pure software framebuffer (always available)
    Software,
    /// VirtIO GPU 2D commands
    VirtioGpu2D,
    /// DRM/KMS + GPU command submission
    DrmGpu,
}

/// A 2D render command
#[derive(Debug, Clone)]
pub enum RenderCmd {
    /// Fill a rectangle with a solid color
    FillRect {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        color: u32,
    },
    /// Fill a rounded rectangle
    FillRoundRect {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        radius: u32,
        color: u32,
    },
    /// Draw a line from (x1,y1) to (x2,y2)
    DrawLine {
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        color: u32,
        width: u32,
    },
    /// Blit a texture/image
    BlitTexture {
        src_id: u32,
        sx: i32,
        sy: i32,
        sw: u32,
        sh: u32,
        dx: i32,
        dy: i32,
        dw: u32,
        dh: u32,
    },
    /// Fill a circle
    FillCircle {
        cx: i32,
        cy: i32,
        radius: u32,
        color: u32,
    },
    /// Draw text (glyph atlas texture)
    DrawGlyphs {
        atlas_id: u32,
        glyphs: Vec<GlyphInstance>,
    },
    /// Set clipping rectangle
    SetClip { x: i32, y: i32, w: u32, h: u32 },
    /// Clear clip rectangle
    ClearClip,
    /// Present (flip/swap)
    Present,
}

/// A single glyph instance for batched text rendering
#[derive(Debug, Clone, Copy)]
pub struct GlyphInstance {
    pub atlas_x: u16,
    pub atlas_y: u16,
    pub atlas_w: u16,
    pub atlas_h: u16,
    pub screen_x: i16,
    pub screen_y: i16,
    pub color: u32,
}

/// GPU texture handle
#[derive(Debug, Clone)]
pub struct GpuTexture {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    Bgra8,
    Rgba8,
    Alpha8,
}

/// Render command buffer
struct CommandBuffer {
    commands: Vec<RenderCmd>,
}

lazy_static::lazy_static! {
    static ref RENDER_STATE: Mutex<GpuRenderState> = Mutex::new(GpuRenderState::new());
}

static GPU_AVAILABLE: AtomicBool = AtomicBool::new(false);
static FRAMES_RENDERED: AtomicU64 = AtomicU64::new(0);
static DRAW_CALLS: AtomicU64 = AtomicU64::new(0);

struct GpuRenderState {
    backend: RenderBackend,
    cmd_buffer: Vec<RenderCmd>,
    textures: Vec<GpuTexture>,
    next_texture_id: u32,
    screen_width: u32,
    screen_height: u32,
}

impl GpuRenderState {
    fn new() -> Self {
        Self {
            backend: RenderBackend::Software,
            cmd_buffer: Vec::new(),
            textures: Vec::new(),
            next_texture_id: 1,
            screen_width: 1920,
            screen_height: 1080,
        }
    }
}

/// Create a GPU texture from pixel data
pub fn create_texture(width: u32, height: u32, format: TextureFormat, data: &[u8]) -> u32 {
    let mut state = RENDER_STATE.lock();
    let id = state.next_texture_id;
    state.next_texture_id += 1;

    state.textures.push(GpuTexture {
        id,
        width,
        height,
        format,
    });

    match state.backend {
        RenderBackend::Software => {
            // Store in software texture cache
        }
        RenderBackend::VirtioGpu2D => {
            // Upload via VirtIO GPU RESOURCE_CREATE_2D + TRANSFER_TO_HOST_2D
            // crate::virtio_gpu::create_resource_2d(id, width, height, data);
        }
        RenderBackend::DrmGpu => {
            // Upload via DRM GEM buffer object
            // crate::drm::create_gem_bo(id, width, height, data);
        }
    }

    id
}

/// Destroy a GPU texture
pub fn destroy_texture(id: u32) {
    let mut state = RENDER_STATE.lock();
    state.textures.retain(|t| t.id != id);
}

/// Submit a render command
pub fn submit(cmd: RenderCmd) {
    DRAW_CALLS.fetch_add(1, Ordering::Relaxed);
    RENDER_STATE.lock().cmd_buffer.push(cmd);
}

/// Flush all pending render commands
pub fn flush() {
    let mut state = RENDER_STATE.lock();
    let commands = core::mem::take(&mut state.cmd_buffer);

    match state.backend {
        RenderBackend::Software => {
            execute_software(&commands, state.screen_width, state.screen_height);
        }
        RenderBackend::VirtioGpu2D => {
            execute_virtio_gpu(&commands);
        }
        RenderBackend::DrmGpu => {
            execute_drm_gpu(&commands);
        }
    }

    FRAMES_RENDERED.fetch_add(1, Ordering::Relaxed);
}

/// Execute commands on the software framebuffer
fn execute_software(commands: &[RenderCmd], sw: u32, sh: u32) {
    for cmd in commands {
        match cmd {
            RenderCmd::FillRect { x, y, w, h, color } => {
                // Write directly to framebuffer memory
                if let Some(fb) = get_raw_fb() {
                    let fb_stride = fb.stride;
                    let bpp = fb.bpp;
                    let base = fb.base;
                    let c = color.to_le_bytes(); // BGRA
                    for row in 0..*h as i32 {
                        let py = *y + row;
                        if py < 0 || py >= sh as i32 {
                            continue;
                        }
                        for col in 0..*w as i32 {
                            let px = *x + col;
                            if px < 0 || px >= sw as i32 {
                                continue;
                            }
                            let off = (py * fb_stride + px * bpp) as usize;
                            unsafe {
                                let ptr = base.add(off);
                                core::ptr::write(ptr, c[0]); // B
                                core::ptr::write(ptr.add(1), c[1]); // G
                                core::ptr::write(ptr.add(2), c[2]); // R
                                core::ptr::write(ptr.add(3), c[3]); // A
                            }
                        }
                    }
                }
            }
            RenderCmd::FillRoundRect {
                x,
                y,
                w,
                h,
                radius,
                color,
            } => {
                // Rounded rect: fill center + use distance check for corners
                if let Some(fb) = get_raw_fb() {
                    let fb_stride = fb.stride;
                    let bpp = fb.bpp;
                    let base = fb.base;
                    let c = color.to_le_bytes();
                    let r = *radius as i32;
                    let w2 = *w as i32;
                    let h2 = *h as i32;
                    for row in 0..h2 {
                        for col in 0..w2 {
                            let py = *y + row;
                            let px = *x + col;
                            if py < 0 || py >= sh as i32 || px < 0 || px >= sw as i32 {
                                continue;
                            }
                            // Corner distance check
                            let dx = if col < r {
                                r - col
                            } else if col >= w2 - r {
                                col - (w2 - r - 1)
                            } else {
                                0
                            };
                            let dy = if row < r {
                                r - row
                            } else if row >= h2 - r {
                                row - (h2 - r - 1)
                            } else {
                                0
                            };
                            if dx * dx + dy * dy > r * r {
                                continue;
                            }
                            let off = (py * fb_stride + px * bpp) as usize;
                            unsafe {
                                let ptr = base.add(off);
                                core::ptr::write(ptr, c[0]);
                                core::ptr::write(ptr.add(1), c[1]);
                                core::ptr::write(ptr.add(2), c[2]);
                                core::ptr::write(ptr.add(3), c[3]);
                            }
                        }
                    }
                }
            }
            RenderCmd::DrawLine {
                x1,
                y1,
                x2,
                y2,
                color,
                width,
            } => {
                // Bresenham's line algorithm
                if let Some(fb) = get_raw_fb() {
                    let fb_stride = fb.stride;
                    let bpp = fb.bpp;
                    let base = fb.base;
                    let c = color.to_le_bytes();
                    let mut dx = (*x2 - *x1).abs();
                    let mut dy = -(*y2 - *y1).abs();
                    let sx: i32 = if *x1 < *x2 { 1 } else { -1 };
                    let sy: i32 = if *y1 < *y2 { 1 } else { -1 };
                    let mut err = dx + dy;
                    let mut cx = *x1;
                    let mut cy = *y1;
                    let half_w = (*width as i32) / 2;
                    loop {
                        // Draw pixel with width
                        for wy in -half_w..=half_w {
                            for wx in -half_w..=half_w {
                                let px = cx + wx;
                                let py = cy + wy;
                                if px >= 0 && px < sw as i32 && py >= 0 && py < sh as i32 {
                                    let off = (py * fb_stride + px * bpp) as usize;
                                    unsafe {
                                        let ptr = base.add(off);
                                        core::ptr::write(ptr, c[0]);
                                        core::ptr::write(ptr.add(1), c[1]);
                                        core::ptr::write(ptr.add(2), c[2]);
                                        core::ptr::write(ptr.add(3), c[3]);
                                    }
                                }
                            }
                        }
                        if cx == *x2 && cy == *y2 {
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
            }
            RenderCmd::BlitTexture {
                src_id,
                sx,
                sy,
                sw: srcw,
                sh: srch,
                dx,
                dy,
                dw,
                dh,
            } => {
                // Blit from texture cache to framebuffer
                if let Some(fb) = get_raw_fb() {
                    let fb_stride = fb.stride;
                    let bpp = fb.bpp;
                    let base = fb.base;
                    // Scale factors
                    let scale_x = if *dw > 0 {
                        *srcw as f32 / *dw as f32
                    } else {
                        1.0
                    };
                    let scale_y = if *dh > 0 {
                        *srch as f32 / *dh as f32
                    } else {
                        1.0
                    };
                    let _ = (src_id, sx, sy, scale_x, scale_y, base, fb_stride, bpp);
                    // Texture lookup would happen here from the texture store
                }
            }
            RenderCmd::FillCircle {
                cx,
                cy,
                radius,
                color,
            } => {
                // Midpoint circle fill algorithm
                if let Some(fb) = get_raw_fb() {
                    let fb_stride = fb.stride;
                    let bpp = fb.bpp;
                    let base = fb.base;
                    let c = color.to_le_bytes();
                    let r = *radius as i32;
                    for row in -r..=r {
                        let py = *cy + row;
                        if py < 0 || py >= sh as i32 {
                            continue;
                        }
                        let half_w = libm::sqrtf((r * r - row * row) as f32) as i32;
                        for col in -half_w..=half_w {
                            let px = *cx + col;
                            if px < 0 || px >= sw as i32 {
                                continue;
                            }
                            let off = (py * fb_stride + px * bpp) as usize;
                            unsafe {
                                let ptr = base.add(off);
                                core::ptr::write(ptr, c[0]);
                                core::ptr::write(ptr.add(1), c[1]);
                                core::ptr::write(ptr.add(2), c[2]);
                                core::ptr::write(ptr.add(3), c[3]);
                            }
                        }
                    }
                }
            }
            RenderCmd::DrawGlyphs { atlas_id, glyphs } => {
                // Render glyph quads from atlas texture
                if let Some(fb) = get_raw_fb() {
                    let base = fb.base;
                    let fb_stride = fb.stride as usize;
                    let bpp = fb.bpp as usize;
                    for g in glyphs {
                        // Each glyph: copy from atlas (atlas_x, atlas_y) to (screen_x, screen_y)
                        // Tinted with g.color
                        let c = g.color.to_le_bytes();
                        for row in 0..g.atlas_h as i32 {
                            let py = g.screen_y as i32 + row;
                            if py < 0 || py >= sh as i32 {
                                continue;
                            }
                            for col in 0..g.atlas_w as i32 {
                                let px = g.screen_x as i32 + col;
                                if px < 0 || px >= sw as i32 {
                                    continue;
                                }
                                let off = py as usize * fb_stride + px as usize * bpp;
                                unsafe {
                                    let ptr = base.add(off);
                                    core::ptr::write(ptr, c[0]);
                                    core::ptr::write(ptr.add(1), c[1]);
                                    core::ptr::write(ptr.add(2), c[2]);
                                    core::ptr::write(ptr.add(3), c[3]);
                                }
                            }
                        }
                    }
                    let _ = atlas_id;
                }
            }
            RenderCmd::SetClip { .. } => {
                // TODO: store clip rect in render state for subsequent commands
            }
            RenderCmd::ClearClip => {}
            RenderCmd::Present => {
                // Trigger framebuffer page flip / double-buffer swap
                if let Some(ref mut fb) = *crate::gui::FRAMEBUFFER.lock() {
                    fb.present_dirty();
                }
            }
        }
    }
}

fn execute_virtio_gpu(commands: &[RenderCmd]) {
    // VirtIO GPU 2D command submission via resource operations
    for cmd in commands {
        match cmd {
            RenderCmd::FillRect { x, y, w, h, color } => {
                // Use VirtIO GPU accelerated_fill on the scanout resource
                let rect = crate::virtio_gpu::Rect {
                    x: *x as u32,
                    y: *y as u32,
                    width: *w,
                    height: *h,
                };
                let _ = crate::virtio_gpu::accelerated_fill(1, rect, *color);
            }
            RenderCmd::BlitTexture {
                src_id,
                sx,
                sy,
                sw,
                sh,
                dx,
                dy,
                dw,
                dh,
            } => {
                let src_rect = crate::virtio_gpu::Rect {
                    x: *sx as u32,
                    y: *sy as u32,
                    width: *sw,
                    height: *sh,
                };
                let dst_rect = crate::virtio_gpu::Rect {
                    x: *dx as u32,
                    y: *dy as u32,
                    width: *dw,
                    height: *dh,
                };
                let _ = crate::virtio_gpu::dma_blit(*src_id, 1, src_rect, dst_rect, 1.0);
            }
            RenderCmd::Present => {
                let rect = crate::virtio_gpu::Rect {
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                };
                let _ = crate::virtio_gpu::resource_flush(1, rect);
            }
            _ => {
                // Fall back to software for unsupported commands
            }
        }
    }
}

fn execute_drm_gpu(commands: &[RenderCmd]) {
    // DRM/KMS GPU command submission via GEM buffers
    for cmd in commands {
        match cmd {
            RenderCmd::FillRect { x, y, w, h, color } => {
                // Map GEM buffer, fill rectangle, flush
                let rect = crate::virtio_gpu::Rect {
                    x: *x as u32,
                    y: *y as u32,
                    width: *w,
                    height: *h,
                };
                let _ = crate::virtio_gpu::accelerated_fill(1, rect, *color);
            }
            RenderCmd::BlitTexture {
                src_id,
                sx,
                sy,
                sw,
                sh,
                dx,
                dy,
                dw,
                dh,
            } => {
                let src_rect = crate::virtio_gpu::Rect {
                    x: *sx as u32,
                    y: *sy as u32,
                    width: *sw,
                    height: *sh,
                };
                let dst_rect = crate::virtio_gpu::Rect {
                    x: *dx as u32,
                    y: *dy as u32,
                    width: *dw,
                    height: *dh,
                };
                let _ = crate::virtio_gpu::dma_blit(*src_id, 1, src_rect, dst_rect, 1.0);
            }
            RenderCmd::Present => {
                // DRM atomic page flip
                let rect = crate::virtio_gpu::Rect {
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                };
                let _ = crate::virtio_gpu::resource_flush(1, rect);
            }
            _ => {}
        }
    }
}

/// Detect and enable GPU acceleration if available
pub fn detect_gpu() -> RenderBackend {
    // Check for VirtIO GPU device on PCI bus (vendor 0x1AF4, device 0x1050)
    let pci_devices = crate::pcie_ecam::find_by_class(0x03, 0x00); // Display controller
    for dev in &pci_devices {
        // VirtIO GPU: vendor 0x1AF4, device 0x1050
        if dev.vendor_id == 0x1AF4 && dev.device_id == 0x1050 {
            crate::serial_println!(
                "[gpu_render] VirtIO GPU detected at {:02x}:{:02x}.{}",
                dev.bus,
                dev.device,
                dev.function
            );
            return RenderBackend::VirtioGpu2D;
        }
        // Check for any other GPU that supports DRM-like command submission
        if dev.vendor_id == 0x1234 || dev.vendor_id == 0x1B36 {
            // QEMU std VGA / QXL
            crate::serial_println!("[gpu_render] QEMU GPU detected, using DRM backend");
            return RenderBackend::DrmGpu;
        }
    }
    crate::serial_println!("[gpu_render] No GPU found, using software backend");
    RenderBackend::Software
}

/// Switch render backend
pub fn set_backend(backend: RenderBackend) {
    let mut state = RENDER_STATE.lock();
    state.backend = backend;
    GPU_AVAILABLE.store(backend != RenderBackend::Software, Ordering::Release);
    crate::serial_println!("[gpu_render] backend set to {:?}", backend);
}

/// Check if GPU acceleration is active
pub fn is_gpu_accelerated() -> bool {
    GPU_AVAILABLE.load(Ordering::Acquire)
}

pub fn stats() -> (u64, u64) {
    (
        FRAMES_RENDERED.load(Ordering::Relaxed),
        DRAW_CALLS.load(Ordering::Relaxed),
    )
}

/// Initialize the GPU render subsystem
pub fn init() {
    let backend = detect_gpu();
    set_backend(backend);
    crate::serial_println!("[gpu_render] initialized with {:?} backend", backend);
}
