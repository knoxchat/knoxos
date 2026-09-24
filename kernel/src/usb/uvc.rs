//! USB Video Class (UVC) webcam probe and stream placeholders.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// USB Video Class (UVC) Webcam Driver
// ═══════════════════════════════════════════════════════════════════════

/// UVC camera stream format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UvcFormat {
    Yuy2,
    Mjpeg,
    H264,
    Nv12,
}

/// UVC camera device
#[derive(Debug, Clone)]
pub struct UvcCamera {
    pub device_id: u8,
    pub name: String,
    pub width: u16,
    pub height: u16,
    pub fps: u8,
    pub format: UvcFormat,
    pub streaming: bool,
}

lazy_static::lazy_static! {
    static ref UVC_CAMERAS: Mutex<Vec<UvcCamera>> = Mutex::new(Vec::new());
}

/// Probe a USB device for UVC camera interface (class 0x0E)
pub fn uvc_probe(device_id: u8, name: &str) -> bool {
    let mut cameras = UVC_CAMERAS.lock();
    cameras.push(UvcCamera {
        device_id,
        name: String::from(name),
        width: 640,
        height: 480,
        fps: 30,
        format: UvcFormat::Yuy2,
        streaming: false,
    });
    serial_println!("[UVC] Camera '{}' probed (640x480 @ 30fps)", name);
    true
}

/// Start webcam stream
pub fn uvc_start_stream(cam_idx: usize, width: u16, height: u16, fps: u8) -> bool {
    let mut cameras = UVC_CAMERAS.lock();
    if let Some(cam) = cameras.get_mut(cam_idx) {
        cam.width = width;
        cam.height = height;
        cam.fps = fps;
        cam.streaming = true;
        serial_println!("[UVC] Started stream {}x{} @ {}fps", width, height, fps);
        true
    } else {
        false
    }
}

/// Stop webcam stream
pub fn uvc_stop_stream(cam_idx: usize) {
    let mut cameras = UVC_CAMERAS.lock();
    if let Some(cam) = cameras.get_mut(cam_idx) {
        cam.streaming = false;
    }
}

/// Get webcam frame (returns raw pixel data placeholder)
pub fn uvc_read_frame(cam_idx: usize) -> Option<Vec<u8>> {
    let cameras = UVC_CAMERAS.lock();
    if let Some(cam) = cameras.get(cam_idx) {
        if cam.streaming {
            let frame_size = cam.width as usize * cam.height as usize * 2; // YUY2 = 2 bytes/pixel
            return Some(alloc::vec![0u8; frame_size]);
        }
    }
    None
}
