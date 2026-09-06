/// DRM — Direct Rendering Manager (kernel GPU interface)
///
/// Provides Linux DRM/KMS compatible abstractions:
///   - Mode setting (CRTC, connector, encoder, plane)
///   - GEM buffer objects
///   - DRM ioctls
///   - Framebuffer management
///   - VSync / page flip
///
/// This complements the existing gpu.rs module with DRM-specific APIs.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── Object IDs ────────────────────────────────────────────────────────

static NEXT_ID: AtomicU32 = AtomicU32::new(1);
fn alloc_id() -> u32 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

// ─── Display Mode ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DisplayMode {
    pub name: String,
    pub hdisplay: u32,
    pub vdisplay: u32,
    pub hsync_start: u32,
    pub hsync_end: u32,
    pub htotal: u32,
    pub vsync_start: u32,
    pub vsync_end: u32,
    pub vtotal: u32,
    pub clock_khz: u32, // pixel clock
    pub vrefresh: u32,  // Hz
    pub flags: u32,
    pub preferred: bool,
}

impl DisplayMode {
    pub fn default_1024x768() -> Self {
        Self {
            name: String::from("1024x768@60"),
            hdisplay: 1024,
            vdisplay: 768,
            hsync_start: 1048,
            hsync_end: 1184,
            htotal: 1344,
            vsync_start: 771,
            vsync_end: 777,
            vtotal: 806,
            clock_khz: 65000,
            vrefresh: 60,
            flags: 0,
            preferred: false,
        }
    }

    pub fn default_1920x1080() -> Self {
        Self {
            name: String::from("1920x1080@60"),
            hdisplay: 1920,
            vdisplay: 1080,
            hsync_start: 2008,
            hsync_end: 2052,
            htotal: 2200,
            vsync_start: 1084,
            vsync_end: 1089,
            vtotal: 1125,
            clock_khz: 148500,
            vrefresh: 60,
            flags: 0,
            preferred: true,
        }
    }
}

// ─── Connector ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorType {
    VGA,
    DVII,
    DVID,
    HDMIA,
    HDMIB,
    DisplayPort,
    EDP,
    Virtual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorStatus {
    Connected,
    Disconnected,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Connector {
    pub id: u32,
    pub connector_type: ConnectorType,
    pub status: ConnectorStatus,
    pub encoder_id: Option<u32>,
    pub modes: Vec<DisplayMode>,
    pub physical_width_mm: u32,
    pub physical_height_mm: u32,
}

// ─── Encoder ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderType {
    DAC,
    TMDS,
    LVDS,
    DPMST,
    Virtual,
}

#[derive(Debug, Clone)]
pub struct Encoder {
    pub id: u32,
    pub encoder_type: EncoderType,
    pub crtc_id: Option<u32>,
    pub possible_crtcs: u32, // bitmask
}

// ─── CRTC ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Crtc {
    pub id: u32,
    pub x: u32,
    pub y: u32,
    pub mode: Option<DisplayMode>,
    pub fb_id: Option<u32>,
    pub gamma_size: u32,
    pub enabled: bool,
}

// ─── Plane ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneType {
    Primary,
    Overlay,
    Cursor,
}

#[derive(Debug, Clone)]
pub struct Plane {
    pub id: u32,
    pub plane_type: PlaneType,
    pub crtc_id: Option<u32>,
    pub fb_id: Option<u32>,
    pub crtc_x: i32,
    pub crtc_y: i32,
    pub crtc_w: u32,
    pub crtc_h: u32,
    pub src_x: u32,
    pub src_y: u32,
    pub src_w: u32, // 16.16 fixed point
    pub src_h: u32,
    pub possible_crtcs: u32,
    pub formats: Vec<u32>,
}

// ─── GEM Buffer Object ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GemObject {
    pub handle: u32,
    pub size: usize,
    pub phys_addr: u64,
    pub virt_addr: u64,
    pub name: u32, // flink name for sharing
    pub refcount: u32,
    pub owner_pid: u32,
}

// ─── Framebuffer ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DrmFramebuffer {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bpp: u32,
    pub depth: u32,
    pub gem_handle: u32,
    pub offset: u32,
    pub pixel_format: u32, // fourcc
}

// ─── DRM Device ────────────────────────────────────────────────────────

pub struct DrmDevice {
    pub connectors: BTreeMap<u32, Connector>,
    pub encoders: BTreeMap<u32, Encoder>,
    pub crtcs: BTreeMap<u32, Crtc>,
    pub planes: BTreeMap<u32, Plane>,
    pub framebuffers: BTreeMap<u32, DrmFramebuffer>,
    pub gem_objects: BTreeMap<u32, GemObject>,
    pub vblank_count: u64,
    pub page_flip_pending: bool,
    pub driver_name: String,
}

impl Default for DrmDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl DrmDevice {
    pub fn new() -> Self {
        Self {
            connectors: BTreeMap::new(),
            encoders: BTreeMap::new(),
            crtcs: BTreeMap::new(),
            planes: BTreeMap::new(),
            framebuffers: BTreeMap::new(),
            gem_objects: BTreeMap::new(),
            vblank_count: 0,
            page_flip_pending: false,
            driver_name: String::from("knoxos-drm"),
        }
    }

    // ── Mode setting ──────────────────────────────────────────────

    pub fn get_resources(&self) -> DrmResources {
        DrmResources {
            crtc_ids: self.crtcs.keys().copied().collect(),
            connector_ids: self.connectors.keys().copied().collect(),
            encoder_ids: self.encoders.keys().copied().collect(),
            min_width: 0,
            max_width: 8192,
            min_height: 0,
            max_height: 8192,
        }
    }

    pub fn set_crtc(
        &mut self,
        crtc_id: u32,
        fb_id: u32,
        x: u32,
        y: u32,
        connector_ids: &[u32],
        mode: &DisplayMode,
    ) -> bool {
        if let Some(crtc) = self.crtcs.get_mut(&crtc_id) {
            crtc.fb_id = Some(fb_id);
            crtc.x = x;
            crtc.y = y;
            crtc.mode = Some(mode.clone());
            crtc.enabled = true;

            // Connect encoder to CRTC
            for &conn_id in connector_ids {
                if let Some(conn) = self.connectors.get(&conn_id) {
                    if let Some(enc_id) = conn.encoder_id {
                        if let Some(enc) = self.encoders.get_mut(&enc_id) {
                            enc.crtc_id = Some(crtc_id);
                        }
                    }
                }
            }
            serial_println!(
                "[DRM] Set CRTC {}: {}x{} @ {}Hz",
                crtc_id,
                mode.hdisplay,
                mode.vdisplay,
                mode.vrefresh
            );
            true
        } else {
            false
        }
    }

    pub fn page_flip(&mut self, crtc_id: u32, fb_id: u32) -> bool {
        if self.page_flip_pending {
            return false;
        }
        if let Some(crtc) = self.crtcs.get_mut(&crtc_id) {
            crtc.fb_id = Some(fb_id);
            self.page_flip_pending = true;
            true
        } else {
            false
        }
    }

    pub fn vblank(&mut self) {
        self.vblank_count += 1;
        self.page_flip_pending = false;
    }

    // ── GEM buffer management ─────────────────────────────────────

    pub fn gem_create(&mut self, size: usize, owner_pid: u32) -> u32 {
        let handle = alloc_id();
        let obj = GemObject {
            handle,
            size,
            phys_addr: 0, // would allocate from frame allocator
            virt_addr: 0,
            name: 0,
            refcount: 1,
            owner_pid,
        };
        self.gem_objects.insert(handle, obj);
        handle
    }

    pub fn gem_close(&mut self, handle: u32) {
        if let Some(obj) = self.gem_objects.get_mut(&handle) {
            obj.refcount -= 1;
            if obj.refcount == 0 {
                self.gem_objects.remove(&handle);
            }
        }
    }

    // ── Framebuffer ───────────────────────────────────────────────

    pub fn create_framebuffer(
        &mut self,
        width: u32,
        height: u32,
        pitch: u32,
        bpp: u32,
        gem_handle: u32,
    ) -> u32 {
        let id = alloc_id();
        let fb = DrmFramebuffer {
            id,
            width,
            height,
            pitch,
            bpp,
            depth: bpp,
            gem_handle,
            offset: 0,
            pixel_format: 0x34325258, // XR24 (XRGB8888)
        };
        self.framebuffers.insert(id, fb);
        id
    }

    pub fn destroy_framebuffer(&mut self, id: u32) {
        self.framebuffers.remove(&id);
    }
}

#[derive(Debug, Clone)]
pub struct DrmResources {
    pub crtc_ids: Vec<u32>,
    pub connector_ids: Vec<u32>,
    pub encoder_ids: Vec<u32>,
    pub min_width: u32,
    pub max_width: u32,
    pub min_height: u32,
    pub max_height: u32,
}

// ─── Global state ──────────────────────────────────────────────────────

lazy_static::lazy_static! {
    pub static ref DRM_DEVICE: Mutex<DrmDevice> = Mutex::new(DrmDevice::new());
}

static DRM_AVAILABLE: AtomicBool = AtomicBool::new(false);

pub fn is_available() -> bool {
    DRM_AVAILABLE.load(Ordering::Relaxed)
}

// ─── Public API ────────────────────────────────────────────────────────

pub fn get_resources() -> DrmResources {
    DRM_DEVICE.lock().get_resources()
}

pub fn create_gem(size: usize, pid: u32) -> u32 {
    DRM_DEVICE.lock().gem_create(size, pid)
}

pub fn create_fb(width: u32, height: u32, pitch: u32, bpp: u32, gem: u32) -> u32 {
    DRM_DEVICE
        .lock()
        .create_framebuffer(width, height, pitch, bpp, gem)
}

pub fn page_flip(crtc: u32, fb: u32) -> bool {
    DRM_DEVICE.lock().page_flip(crtc, fb)
}

/// Initialize DRM subsystem
pub fn init() {
    let mut dev = DRM_DEVICE.lock();

    // Create virtual CRTC
    let crtc_id = alloc_id();
    dev.crtcs.insert(
        crtc_id,
        Crtc {
            id: crtc_id,
            x: 0,
            y: 0,
            mode: None,
            fb_id: None,
            gamma_size: 256,
            enabled: false,
        },
    );

    // Create virtual encoder
    let enc_id = alloc_id();
    dev.encoders.insert(
        enc_id,
        Encoder {
            id: enc_id,
            encoder_type: EncoderType::Virtual,
            crtc_id: None,
            possible_crtcs: 1,
        },
    );

    // Create virtual connector (represents the framebuffer display)
    let conn_id = alloc_id();
    dev.connectors.insert(
        conn_id,
        Connector {
            id: conn_id,
            connector_type: ConnectorType::Virtual,
            status: ConnectorStatus::Connected,
            encoder_id: Some(enc_id),
            modes: alloc::vec![
                DisplayMode::default_1920x1080(),
                DisplayMode::default_1024x768(),
            ],
            physical_width_mm: 0,
            physical_height_mm: 0,
        },
    );

    // Create primary plane
    let plane_id = alloc_id();
    dev.planes.insert(
        plane_id,
        Plane {
            id: plane_id,
            plane_type: PlaneType::Primary,
            crtc_id: None,
            fb_id: None,
            crtc_x: 0,
            crtc_y: 0,
            crtc_w: 1920,
            crtc_h: 1080,
            src_x: 0,
            src_y: 0,
            src_w: 1920 << 16,
            src_h: 1080 << 16,
            possible_crtcs: 1,
            formats: alloc::vec![0x34325258], // XR24
        },
    );

    drop(dev);
    DRM_AVAILABLE.store(true, Ordering::Relaxed);

    serial_println!("[DRM] Direct Rendering Manager initialized");
    serial_println!("[DRM]   1 CRTC, 1 encoder, 1 connector, 1 plane");
}
