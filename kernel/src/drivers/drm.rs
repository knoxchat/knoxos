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
        let pages = size.div_ceil(4096).max(1);
        let (phys, virt) = if let Some(phys) = crate::vmm::allocate_contiguous_frames(pages) {
            let virt = crate::vmm::phys_to_virt(phys);
            // SAFETY: freshly allocated buddy frames, mapped through the offset.
            unsafe {
                core::ptr::write_bytes(virt as *mut u8, 0, pages * 4096);
            }
            (phys, virt)
        } else {
            (0, 0)
        };
        let obj = GemObject {
            handle,
            size,
            phys_addr: phys,
            virt_addr: virt,
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

/// Write bytes into a GEM BO (software GL / GBM).
pub fn gem_write(handle: u32, data: &[u8]) -> bool {
    let mut dev = DRM_DEVICE.lock();
    let Some(obj) = dev.gem_objects.get_mut(&handle) else {
        return false;
    };
    if obj.virt_addr != 0 && !data.is_empty() {
        let n = data.len().min(obj.size);
        // SAFETY: `virt_addr` is a buddy mapping owned by this GEM object.
        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr(), obj.virt_addr as *mut u8, n);
        }
    }
    true
}

pub fn is_dri_node(path: &str) -> bool {
    path == "/dev/dri/card0" || path == "/dev/dri/renderD128"
}

/// Linux `DRM_IOCTL_VERSION` (`drmGetVersion`) without a user pointer.
pub fn drm_get_version(name: &mut [u8]) -> bool {
    let src = b"virtio_gpu";
    if name.is_empty() {
        return false;
    }
    let n = src.len().min(name.len().saturating_sub(1));
    name[..n].copy_from_slice(&src[..n]);
    name[n] = 0;
    true
}

/// Export a GEM handle as a memfd (PRIME). Returns a process fd, or the memfd id.
pub fn prime_handle_to_fd(pid: u32, handle: u32) -> Option<i32> {
    let mut pixels = crate::init::gpu_fb_bytes();
    {
        let dev = DRM_DEVICE.lock();
        if let Some(obj) = dev.gem_objects.get(&handle) {
            if obj.virt_addr != 0 && obj.size >= 4 {
                let n = obj.size.min(pixels.len()).min(64 * 64 * 4);
                // SAFETY: GEM mapping is live for this handle.
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        obj.virt_addr as *const u8,
                        pixels.as_mut_ptr(),
                        n,
                    );
                }
            }
        } else {
            return None;
        }
    }
    let id = crate::memfd::sys_memfd_create("drm-prime", 0).ok()?;
    crate::memfd::memfd_write(id, 0, &pixels).ok()?;
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    if let Some(fd_table) = tables.get_mut(&pid) {
        let fd = fd_table
            .open(
                &alloc::format!("memfd:{}", id),
                crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDWR),
                crate::fd::FileType::CharDevice,
            )
            .ok()?;
        return Some(fd);
    }
    Some(id as i32)
}

/// Linux DRM ioctls on `/dev/dri/card0` and `renderD128`.
pub fn handle_ioctl(pid: u32, request: u32, arg: u64) -> Result<u64, i32> {
    const DRM_IOCTL_VERSION: u32 = 0xC040_6400;
    const DRM_IOCTL_GEM_CREATE: u32 = 0xC010_6409;
    const DRM_IOCTL_PRIME_HANDLE_TO_FD: u32 = 0xC00C_642D;
    match request {
        DRM_IOCTL_VERSION => ioctl_version(arg),
        DRM_IOCTL_GEM_CREATE => ioctl_gem_create(pid, arg),
        DRM_IOCTL_PRIME_HANDLE_TO_FD => ioctl_prime(pid, arg),
        _ => Err(-22),
    }
}

#[repr(C)]
struct DrmVersion {
    version_major: i32,
    version_minor: i32,
    version_patchlevel: i32,
    _pad: i32,
    name_len: u64,
    name: u64,
    date_len: u64,
    date: u64,
    desc_len: u64,
    desc: u64,
}

#[repr(C)]
struct DrmGemCreate {
    size: u64,
    handle: u32,
    flags: u32,
}

#[repr(C)]
struct DrmPrimeHandle {
    handle: u32,
    flags: u32,
    fd: i32,
}

fn write_cstr(dst: u64, cap: u64, s: &str) -> u64 {
    let need = s.len() as u64;
    if dst == 0 || cap == 0 {
        return need;
    }
    let n = s.len().min(cap.saturating_sub(1) as usize);
    // SAFETY: `dst` is a mapped user or kernel buffer of at least `cap` bytes from DRM_IOCTL_VERSION.
    unsafe {
        core::ptr::copy_nonoverlapping(s.as_ptr(), dst as *mut u8, n);
        *(dst as *mut u8).add(n) = 0;
    }
    need
}

fn ioctl_version(arg: u64) -> Result<u64, i32> {
    if arg == 0 {
        return Err(-14);
    }
    // SAFETY: Ring 3 `drmGetVersion` passes a mapped `struct drm_version`.
    let ver = unsafe { &mut *(arg as *mut DrmVersion) };
    ver.version_major = 1;
    ver.version_minor = 0;
    ver.version_patchlevel = 0;
    ver.name_len = write_cstr(ver.name, ver.name_len, "virtio_gpu");
    ver.date_len = write_cstr(ver.date, ver.date_len, "20261024");
    ver.desc_len = write_cstr(ver.desc, ver.desc_len, "KnoxOS DRM");
    serial_println!(
        "[chrome] {} (drmGetVersion virtio_gpu on /dev/dri/*)",
        "GATE_CHR drm"
    );
    Ok(0)
}

fn ioctl_gem_create(pid: u32, arg: u64) -> Result<u64, i32> {
    if arg == 0 {
        return Err(-14);
    }
    // SAFETY: mapped `struct drm_gem_create` from DRM_IOCTL_GEM_CREATE.
    let gem = unsafe { &mut *(arg as *mut DrmGemCreate) };
    let size = gem.size.max(4096) as usize;
    let handle = create_gem(size, pid);
    if handle == 0 {
        return Err(-12);
    }
    gem.handle = handle;
    crate::init::fill_gem(handle);
    Ok(0)
}

fn ioctl_prime(pid: u32, arg: u64) -> Result<u64, i32> {
    if arg == 0 {
        return Err(-14);
    }
    // SAFETY: mapped `struct drm_prime_handle` from DRM_IOCTL_PRIME_HANDLE_TO_FD.
    let prime = unsafe { &mut *(arg as *mut DrmPrimeHandle) };
    let fd = prime_handle_to_fd(pid, prime.handle).ok_or(-22)?;
    prime.fd = fd;
    Ok(0)
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

    // Cursor overlay plane — bound to the live VirtIO-GPU cursor resource
    // when `-vga virtio` is the primary scanout.
    let cursor_plane_id = alloc_id();
    let (scan_w, scan_h) = crate::virtio_gpu::scanout_size();
    let plane_w = if scan_w > 0 { scan_w } else { 1920 };
    let plane_h = if scan_h > 0 { scan_h } else { 1080 };
    if let Some(plane) = dev.planes.get_mut(&plane_id) {
        plane.crtc_id = Some(crtc_id);
        plane.crtc_w = plane_w;
        plane.crtc_h = plane_h;
        plane.src_w = plane_w << 16;
        plane.src_h = plane_h << 16;
    }
    dev.planes.insert(
        cursor_plane_id,
        Plane {
            id: cursor_plane_id,
            plane_type: PlaneType::Cursor,
            crtc_id: Some(crtc_id),
            fb_id: None,
            crtc_x: 0,
            crtc_y: 0,
            crtc_w: 64,
            crtc_h: 64,
            src_x: 0,
            src_y: 0,
            src_w: 64 << 16,
            src_h: 64 << 16,
            possible_crtcs: 1,
            formats: alloc::vec![0x34325241], // AR24
        },
    );
    if crate::virtio_gpu::scanout_live() && scan_w > 0 && scan_h > 0 {
        let mut mode = DisplayMode::default_1920x1080();
        mode.name = alloc::format!("{}x{}@60", scan_w, scan_h);
        mode.hdisplay = scan_w;
        mode.vdisplay = scan_h;
        let _ = dev.set_crtc(crtc_id, 0, 0, 0, &[conn_id], &mode);
        serial_println!(
            "[DRM] KMS bound to VirtIO-GPU scanout {}x{} (cursor plane {})",
            scan_w,
            scan_h,
            cursor_plane_id
        );
    }

    drop(dev);
    DRM_AVAILABLE.store(true, Ordering::Relaxed);

    serial_println!("[DRM] Direct Rendering Manager initialized");
    serial_println!("[DRM]   1 CRTC, 1 encoder, 1 connector, 2 planes");
    let _ = gem_crtc_self_test();
}

pub const GATE_DRM_MARKER: &str = "GATE_DRM gem";

fn gem_crtc_self_test() -> bool {
    let gem = create_gem(4096, 1);
    let fb = create_fb(64, 16, 64 * 4, 32, gem);
    let (crtc_id, conn_id, phys) = {
        let mut dev = DRM_DEVICE.lock();
        let crtc_id = *dev.crtcs.keys().next().unwrap_or(&0);
        let conn_id = *dev.connectors.keys().next().unwrap_or(&0);
        let mode = DisplayMode::default_1024x768();
        let ok = dev.set_crtc(crtc_id, fb, 0, 0, &[conn_id], &mode);
        let enabled = dev.crtcs.get(&crtc_id).map(|c| c.enabled).unwrap_or(false);
        let fb_match = dev.crtcs.get(&crtc_id).and_then(|c| c.fb_id) == Some(fb);
        let phys = dev.gem_objects.get(&gem).map(|g| g.phys_addr).unwrap_or(0);
        (ok && enabled && fb_match, conn_id, phys)
    };
    if crtc_id && phys != 0 {
        serial_println!("[DRM] {} (buddy GEM {:#x} + CRTC)", GATE_DRM_MARKER, phys);
        true
    } else {
        serial_println!(
            "[DRM] GATE_DRM FAILED crtc={} gem_phys={:#x} conn={}",
            crtc_id,
            phys,
            conn_id
        );
        false
    }
}
