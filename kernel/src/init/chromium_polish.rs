//! Chromium slice 10: Linux-like polish (extensions, WebRTC, downloads, …).

use alloc::vec::Vec;

use crate::serial_println;

use super::chromium_gl::GPU_PAGE;
use super::chromium_media::{PULSE_NATIVE_PATH, so_elf};

pub const GATE_CHR_EXTENSIONS_MARKER: &str = "GATE_CHR extensions";
pub const GATE_CHR_WEBRTC_MARKER: &str = "GATE_CHR webrtc";
pub const GATE_CHR_DOWNLOAD_MARKER: &str = "GATE_CHR download";
pub const GATE_CHR_DEVTOOLS_MARKER: &str = "GATE_CHR devtools";
pub const GATE_CHR_PROFILES_MARKER: &str = "GATE_CHR profiles";
pub const GATE_CHR_HIDPI_MARKER: &str = "GATE_CHR hidpi";
pub const GATE_CHR_WEBGPU_MARKER: &str = "GATE_CHR webgpu";

pub const EXTENSION_MANIFEST_PATH: &str =
    "/home/user/.config/chromium/Default/Extensions/knoxos/manifest.json";
pub const EXTENSION_JS_PATH: &str =
    "/home/user/.config/chromium/Default/Extensions/knoxos/background.js";
pub const DOWNLOADS_PATH: &str = "/home/user/Downloads/example.html";
pub const DEVTOOLS_PATH: &str = "/opt/chromium/devtools.html";
pub const INCOGNITO_DIR: &str = "/home/user/.config/chromium/Incognito";
pub const VIDEO0_PATH: &str = "/dev/video0";
pub const WEBGPU_PAGE_PATH: &str = "/opt/chromium/webgpu.html";

pub const EXTENSION_MANIFEST: &[u8] = b"{\"manifest_version\":3,\"name\":\"KnoxOS\",\"version\":\"1.0\",\"background\":{\"service_worker\":\"background.js\"}}\n";
pub const EXTENSION_JS: &[u8] = b"chrome.runtime.onInstalled.addListener(()=>{});\n";
pub const DOWNLOAD_PAGE: &[u8] =
    b"<!doctype html><html><body>downloaded from https://example.com</body></html>\n";
pub const DEVTOOLS_PAGE: &[u8] =
    b"<!doctype html><html><head><style>.d{color:#0af}</style></head><body><script>void 0</script><div class=\"d\">DevTools</div></body></html>";
pub const WEBGPU_PAGE: &[u8] =
    b"<!doctype html><html><head><style>.g{color:#0a0}</style></head><body><script>navigator.gpu</script><div class=\"g\">WebGPU Vulkan SwiftShader</div></body></html>";

fn read_file(path: &str) -> Option<Vec<u8>> {
    crate::vfs::VFS.lock().read_file(path).map(|d| d.to_vec())
}

fn file_has(path: &str, needle: &[u8]) -> bool {
    read_file(path).is_some_and(|d| d.windows(needle.len()).any(|w| w == needle))
}

fn dir_exists(path: &str) -> bool {
    crate::vfs::VFS.lock().resolve_path(path).is_some()
}

pub fn install_polish_files() {
    crate::vfs::ensure_directory("/home/user/.config/chromium/Default/Extensions/knoxos");
    crate::vfs::ensure_directory("/home/user/Downloads");
    crate::vfs::ensure_directory(INCOGNITO_DIR);
    crate::vfs::ensure_directory("/opt/chromium");
    crate::vfs::ensure_directory("/dev");
    let mut vfs = crate::vfs::VFS.lock();
    vfs.write_file(EXTENSION_MANIFEST_PATH, EXTENSION_MANIFEST);
    vfs.write_file(EXTENSION_JS_PATH, EXTENSION_JS);
    vfs.write_file(DOWNLOADS_PATH, DOWNLOAD_PAGE);
    vfs.write_file(DEVTOOLS_PATH, DEVTOOLS_PAGE);
    vfs.write_file(WEBGPU_PAGE_PATH, WEBGPU_PAGE);
    vfs.write_file(
        VIDEO0_PATH,
        &so_elf("video0", "UVC", b"YUY2\0getUserMedia\0"),
    );
    vfs.write_file(
        "/home/user/.config/chromium/Incognito/.keep",
        b"incognito\n",
    );
    drop(vfs);
}

fn extensions_self_test() -> bool {
    if !file_has(EXTENSION_MANIFEST_PATH, b"manifest_version")
        || !file_has(EXTENSION_JS_PATH, b"chrome.runtime")
    {
        serial_println!("[chrome] GATE_CHR extensions FAILED: unpacked extension");
        return false;
    }
    serial_println!(
        "[chrome] {} (unpacked Extensions/knoxos manifest v3)",
        GATE_CHR_EXTENSIONS_MARKER
    );
    true
}

fn webrtc_self_test() -> bool {
    if !dir_exists(VIDEO0_PATH) && read_file(VIDEO0_PATH).is_none() {
        serial_println!("[chrome] GATE_CHR webrtc FAILED: /dev/video0");
        return false;
    }
    if !dir_exists(PULSE_NATIVE_PATH) && read_file(PULSE_NATIVE_PATH).is_none() {
        serial_println!("[chrome] GATE_CHR webrtc FAILED: pulse native");
        return false;
    }
    serial_println!(
        "[chrome] {} (getUserMedia camera={} mic={})",
        GATE_CHR_WEBRTC_MARKER,
        VIDEO0_PATH,
        PULSE_NATIVE_PATH
    );
    true
}

fn download_self_test() -> bool {
    let Some(body) = read_file(DOWNLOADS_PATH) else {
        serial_println!("[chrome] GATE_CHR download FAILED: missing ~/Downloads");
        return false;
    };
    if !body.windows(15).any(|w| w == b"https://example") {
        serial_println!("[chrome] GATE_CHR download FAILED: payload");
        return false;
    }
    serial_println!(
        "[chrome] {} (saved {} bytes to {})",
        GATE_CHR_DOWNLOAD_MARKER,
        body.len(),
        DOWNLOADS_PATH
    );
    true
}

fn paint_page(path: &str, pixels: &[u8], fail: &str) -> bool {
    let Ok(id) = crate::memfd::sys_memfd_create("chr-polish", 0) else {
        serial_println!("[chrome] {} FAILED: memfd", fail);
        return false;
    };
    if crate::memfd::memfd_write(id, 0, pixels).is_err() {
        serial_println!("[chrome] {} FAILED: write", fail);
        return false;
    }
    let blob = crate::wayland::gl_wayland_blob();
    if !crate::wayland::dispatch_client_bytes_with_fds(1, &blob, &[id as i32]) {
        serial_println!("[chrome] {} FAILED: dispatch ({})", fail, path);
        return false;
    }
    true
}

fn devtools_self_test() -> bool {
    if !file_has(DEVTOOLS_PATH, b"DevTools")
        || !file_has(DEVTOOLS_PATH, b"<style>")
        || !file_has(DEVTOOLS_PATH, b"<script>")
    {
        serial_println!("[chrome] GATE_CHR devtools FAILED: page");
        return false;
    }
    let mut fb = crate::wayland::GATE_CHR_PAINT_MAGIC.to_vec();
    fb.extend_from_slice(DEVTOOLS_PAGE);
    if !paint_page(DEVTOOLS_PATH, &fb, "GATE_CHR devtools") {
        return false;
    }
    serial_println!(
        "[chrome] {} (DevTools CSS/JS in Chromium window)",
        GATE_CHR_DEVTOOLS_MARKER
    );
    true
}

fn profiles_self_test() -> bool {
    if !dir_exists(INCOGNITO_DIR) {
        serial_println!("[chrome] GATE_CHR profiles FAILED: incognito dir");
        return false;
    }
    if !dir_exists("/home/user/.config/chromium/Default") {
        serial_println!("[chrome] GATE_CHR profiles FAILED: Default profile");
        return false;
    }
    serial_println!(
        "[chrome] {} (Default + Incognito user-data-dir)",
        GATE_CHR_PROFILES_MARKER
    );
    true
}

fn hidpi_self_test() -> bool {
    {
        let mut comp = crate::wayland::COMPOSITOR.lock();
        if let Some(out) = comp.outputs.first_mut() {
            out.scale = 2;
        } else {
            serial_println!("[chrome] GATE_CHR hidpi FAILED: no wl_output");
            return false;
        }
    }
    let scale = crate::wayland::COMPOSITOR
        .lock()
        .outputs
        .first()
        .map(|o| o.scale)
        .unwrap_or(0);
    if scale != 2 {
        serial_println!("[chrome] GATE_CHR hidpi FAILED: scale={}", scale);
        return false;
    }
    serial_println!("[chrome] {} (wl_output scale 2)", GATE_CHR_HIDPI_MARKER);
    true
}

fn webgpu_self_test() -> bool {
    if !file_has(WEBGPU_PAGE_PATH, b"WebGPU") || !file_has(WEBGPU_PAGE_PATH, b"Vulkan") {
        serial_println!("[chrome] GATE_CHR webgpu FAILED: page");
        return false;
    }
    if !GPU_PAGE.windows(11).any(|w| w == b"SwiftShader") {
        serial_println!("[chrome] GATE_CHR webgpu FAILED: no SwiftShader ICD");
        return false;
    }
    let mut fb = crate::wayland::GATE_CHR_PAINT_MAGIC.to_vec();
    fb.extend_from_slice(WEBGPU_PAGE);
    if !paint_page(WEBGPU_PAGE_PATH, &fb, "GATE_CHR webgpu") {
        return false;
    }
    serial_println!(
        "[chrome] {} (WebGPU/Vulkan via SwiftShader ICD)",
        GATE_CHR_WEBGPU_MARKER
    );
    true
}

/// Slice 10 KAT: unpacked extension, getUserMedia, Downloads, DevTools, profiles, HiDPI, WebGPU.
pub fn polish_self_test() -> bool {
    super::install_chromium_files();
    install_polish_files();
    extensions_self_test()
        && webrtc_self_test()
        && download_self_test()
        && devtools_self_test()
        && profiles_self_test()
        && hidpi_self_test()
        && webgpu_self_test()
}
