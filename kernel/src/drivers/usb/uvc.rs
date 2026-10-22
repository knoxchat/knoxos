//! USB Video Class payload header parse + YUY2 frame capture.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// UVC camera stream format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UvcFormat {
    Yuy2,
    Mjpeg,
    H264,
    Nv12,
}

/// Parsed UVC 1.5 payload header (HLE + BFH, optional PTS).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UvcPayloadHeader {
    pub hle: u8,
    pub fid: bool,
    pub eof: bool,
    pub pts: Option<u32>,
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
    last_frame: Vec<u8>,
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
        last_frame: Vec::new(),
    });
    serial_println!("[UVC] Camera '{}' probed (640x480 @ 30fps)", name);
    true
}

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

pub fn uvc_stop_stream(cam_idx: usize) {
    let mut cameras = UVC_CAMERAS.lock();
    if let Some(cam) = cameras.get_mut(cam_idx) {
        cam.streaming = false;
    }
}

/// Parse a UVC isochronous payload: HLE, BFH (FID/EOF/PTS), then sample bytes.
pub fn parse_uvc_payload(packet: &[u8]) -> Option<(UvcPayloadHeader, &[u8])> {
    if packet.is_empty() {
        return None;
    }
    let hle = packet[0] as usize;
    if hle < 2 || hle > packet.len() {
        return None;
    }
    let bfh = packet[1];
    if bfh & 0x40 != 0 {
        return None; // ERR
    }
    let has_pts = bfh & 0x04 != 0;
    if has_pts && hle < 6 {
        return None;
    }
    let pts = if has_pts {
        Some(u32::from_le_bytes([
            packet[2], packet[3], packet[4], packet[5],
        ]))
    } else {
        None
    };
    Some((
        UvcPayloadHeader {
            hle: hle as u8,
            fid: bfh & 0x01 != 0,
            eof: bfh & 0x02 != 0,
            pts,
        },
        &packet[hle..],
    ))
}

pub fn uvc_push_payload(cam_idx: usize, packet: &[u8]) -> bool {
    let Some((hdr, payload)) = parse_uvc_payload(packet) else {
        return false;
    };
    let mut cameras = UVC_CAMERAS.lock();
    let Some(cam) = cameras.get_mut(cam_idx) else {
        return false;
    };
    if !cam.streaming {
        return false;
    }
    cam.last_frame.extend_from_slice(payload);
    if hdr.eof {
        let expected = cam.width as usize * cam.height as usize * 2;
        if cam.last_frame.len() > expected {
            cam.last_frame.truncate(expected);
        }
    }
    true
}

pub fn uvc_read_frame(cam_idx: usize) -> Option<Vec<u8>> {
    let mut cameras = UVC_CAMERAS.lock();
    let cam = cameras.get_mut(cam_idx)?;
    if !cam.streaming || cam.last_frame.is_empty() {
        return None;
    }
    Some(core::mem::take(&mut cam.last_frame))
}

pub const GATE_USB_UVC_MARKER: &str = "GATE_USB uvc";

pub fn uvc_self_test() -> bool {
    let _ = uvc_probe(1, "QEMU Webcam");
    if !uvc_start_stream(0, 2, 2, 30) {
        serial_println!("[UVC] GATE_USB FAILED: start");
        return false;
    }
    // 2×2 YUY2 = 8 bytes. HLE=2, EOF|FID, no PTS.
    let mut packet = alloc::vec![2u8, 0x03];
    packet.extend_from_slice(&[0x10, 0x80, 0x20, 0x80, 0x30, 0x80, 0x40, 0x80]);
    if !uvc_push_payload(0, &packet) {
        serial_println!("[UVC] GATE_USB FAILED: payload");
        return false;
    }
    match uvc_read_frame(0) {
        Some(frame) if frame.len() == 8 && frame[0] == 0x10 && frame[6] == 0x40 => {
            serial_println!(
                "[UVC] {} (YUY2 2x2 EOF payload {} bytes)",
                GATE_USB_UVC_MARKER,
                frame.len()
            );
            true
        }
        other => {
            serial_println!(
                "[UVC] GATE_USB FAILED frame_len={}",
                other.map(|f| f.len()).unwrap_or(0)
            );
            false
        }
    }
}
