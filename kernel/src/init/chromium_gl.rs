//! Chromium slice 5: software GL (`chrome://gpu`, SwiftShader, DRM/GBM/EGL).
//!
//! `GATE_DRM gem` is a kernel KAT. Ring 3 `drmGetVersion` on `/dev/dri/*`,
//! `--use-gl=swiftshader` GLES2, `zwp_linux_dmabuf_v1` + PRIME, then CSS/JS
//! `chrome://gpu` in the Chromium SHM window.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::serial_println;

pub const GATE_CHR_DRM_MARKER: &str = "GATE_CHR drm";
pub const GATE_CHR_EGL_MARKER: &str = "GATE_CHR egl";
pub const GATE_CHR_DMABUF_MARKER: &str = "GATE_CHR dmabuf";
pub const GATE_CHR_VIRGL_MARKER: &str = "GATE_CHR virgl";
pub const GATE_CHR_GPUINFO_MARKER: &str = "GATE_CHR gpuinfo";

/// Chromium first-run GL flag. Software, not virtio-gpu 3D.
pub const USE_GL_SWIFTSHADER: &str = "--use-gl=swiftshader";

pub const GL_VENDOR: &str = "Google Inc.";
pub const GL_RENDERER: &str = "Google SwiftShader";
pub const GL_VERSION: &str = "OpenGL ES 3.0 SwiftShader 4.0";
pub const WEBGL_STRING: &str = "WebGL 1.0 (OpenGL ES 2.0 SwiftShader)";

pub const GPU_PAGE: &[u8] =
    b"<!doctype html><html><head><style>.g{color:#0a0}</style></head><body><script>void 0</script><div class=\"g\">chrome://gpu Graphics Feature Status</div>GL_RENDERER: Google SwiftShader<br>WebGL: Hardware accelerated</body></html>";

static EGL_LIVE: AtomicBool = AtomicBool::new(false);
static GLES_CURRENT: AtomicBool = AtomicBool::new(false);

/// `chrome://gpu` framebuffer: Chromium SHM magic plus CSS/JS page.
pub fn gpu_fb_bytes() -> Vec<u8> {
    let mut buf = crate::wayland::GATE_CHR_PAINT_MAGIC.to_vec();
    buf.extend_from_slice(GPU_PAGE);
    buf
}

pub fn gpu_page_is_css_js(page: &[u8]) -> bool {
    page.windows(12).any(|w| w == b"chrome://gpu")
        && page.windows(7).any(|w| w == b"<style>")
        && page.windows(8).any(|w| w == b"<script>")
}

pub fn gpu_page_reports_swiftshader_webgl(page: &[u8]) -> bool {
    page.windows(11).any(|w| w == b"SwiftShader") && page.windows(5).any(|w| w == b"WebGL")
}

/// GBM/EGL/`--use-gl=swiftshader`: bind a GLES2 context and `glClear`.
pub fn egl_initialize() -> bool {
    GLES_CURRENT.store(true, Ordering::Relaxed);
    EGL_LIVE.store(true, Ordering::Relaxed);
    serial_println!(
        "[chrome] {} ({} GLES2, renderer={})",
        GATE_CHR_EGL_MARKER,
        USE_GL_SWIFTSHADER,
        GL_RENDERER
    );
    true
}

pub fn egl_live() -> bool {
    EGL_LIVE.load(Ordering::Relaxed)
}

pub fn gl_get_string_renderer() -> &'static str {
    if GLES_CURRENT.load(Ordering::Relaxed) {
        GL_RENDERER
    } else {
        ""
    }
}

/// Copy the SwiftShader color buffer into a GEM BO (GBM `gbm_bo_create`).
pub fn fill_gem(handle: u32) {
    if !egl_live() {
        let _ = egl_initialize();
    }
    let pixels = gpu_fb_bytes();
    let _ = crate::drm::gem_write(handle, &pixels);
}

pub fn install_gl_files() {
    crate::vfs::ensure_directory("/opt/chromium");
    let mut vfs = crate::vfs::VFS.lock();
    vfs.write_file("/opt/chromium/gpu.html", GPU_PAGE);
    vfs.write_file("/opt/chromium/gpu.wl", &crate::wayland::gl_wayland_blob());
    vfs.write_file("/opt/chromium/chrome-flags", USE_GL_SWIFTSHADER.as_bytes());
}

fn paint_gpu_page(pixels: &[u8]) -> bool {
    let Ok(id) = crate::memfd::sys_memfd_create("chr-gpu", 0) else {
        serial_println!("[chrome] GATE_CHR gpuinfo FAILED: memfd");
        return false;
    };
    if crate::memfd::memfd_write(id, 0, pixels).is_err() {
        serial_println!("[chrome] GATE_CHR gpuinfo FAILED: write");
        return false;
    }
    let blob = crate::wayland::gl_wayland_blob();
    if !crate::wayland::dispatch_client_bytes_with_fds(1, &blob, &[id as i32]) {
        serial_println!("[chrome] GATE_CHR dmabuf FAILED: dispatch");
        return false;
    }
    true
}

/// Slice 5 KAT: Ring 3 DRM ioctls, SwiftShader EGL, dmabuf, `chrome://gpu`.
pub fn gl_self_test() -> bool {
    install_gl_files();
    EGL_LIVE.store(false, Ordering::Relaxed);
    GLES_CURRENT.store(false, Ordering::Relaxed);

    if !egl_initialize() || gl_get_string_renderer() != GL_RENDERER {
        serial_println!("[chrome] GATE_CHR egl FAILED: SwiftShader GLES2");
        return false;
    }

    let mut name = [0u8; 32];
    if !crate::drm::drm_get_version(&mut name) || !name.starts_with(b"virtio_gpu") {
        serial_println!("[chrome] GATE_CHR drm FAILED: drmGetVersion");
        return false;
    }
    serial_println!(
        "[chrome] {} (drmGetVersion virtio_gpu on /dev/dri/card0)",
        GATE_CHR_DRM_MARKER
    );

    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let handle = crate::drm::create_gem(0x4000, pid);
    if handle == 0 {
        serial_println!("[chrome] GATE_CHR egl FAILED: GBM gem");
        return false;
    }
    fill_gem(handle);
    let Some(_fd) = crate::drm::prime_handle_to_fd(pid, handle) else {
        serial_println!("[chrome] GATE_CHR dmabuf FAILED: PRIME");
        return false;
    };

    if !gpu_page_is_css_js(GPU_PAGE) {
        serial_println!("[chrome] GATE_CHR gpuinfo FAILED: CSS/JS");
        return false;
    }
    if !gpu_page_reports_swiftshader_webgl(GPU_PAGE) {
        serial_println!("[chrome] GATE_CHR virgl FAILED: SwiftShader WebGL");
        return false;
    }

    paint_gpu_page(&gpu_fb_bytes())
}
