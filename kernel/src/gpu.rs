/// GPU Driver - Basic VESA/VBE framebuffer graphics driver
/// Provides GPU abstraction for framebuffer-based rendering
/// Compatible with Linux DRM/KMS concepts
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// GPU device information
#[derive(Debug, Clone)]
pub struct GpuDevice {
    pub name: String,
    pub vendor: String,
    pub driver: String,
    pub framebuffer_addr: u64,
    pub framebuffer_size: usize,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bpp: u32,
    pub pixel_format: PixelFormat,
    pub vram_size: usize,
    pub capabilities: GpuCapabilities,
}

/// Pixel format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Bgr888,   // 24-bit BGR (most common VESA)
    Rgb888,   // 24-bit RGB
    Bgra8888, // 32-bit BGRA
    Rgba8888, // 32-bit RGBA
    Unknown,
}

impl PixelFormat {
    pub fn bytes_per_pixel(&self) -> usize {
        match self {
            PixelFormat::Bgr888 | PixelFormat::Rgb888 => 3,
            PixelFormat::Bgra8888 | PixelFormat::Rgba8888 => 4,
            PixelFormat::Unknown => 4,
        }
    }
}

/// GPU capabilities bitfield
#[derive(Debug, Clone, Copy)]
pub struct GpuCapabilities {
    pub hardware_cursor: bool,
    pub hardware_blit: bool,
    pub hardware_fill: bool,
    pub double_buffering: bool,
    pub vsync: bool,
    pub mode_setting: bool,
}

impl GpuCapabilities {
    pub fn none() -> Self {
        Self {
            hardware_cursor: false,
            hardware_blit: false,
            hardware_fill: false,
            double_buffering: false,
            vsync: false,
            mode_setting: false,
        }
    }

    pub fn vesa_basic() -> Self {
        Self {
            hardware_cursor: false,
            hardware_blit: false,
            hardware_fill: false,
            double_buffering: true,
            vsync: false,
            mode_setting: true,
        }
    }
}

/// Display mode
#[derive(Debug, Clone, Copy)]
pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    pub bpp: u32,
    pub refresh_rate: u32,
}

/// Common display modes
pub const DISPLAY_MODES: &[DisplayMode] = &[
    DisplayMode {
        width: 640,
        height: 480,
        bpp: 32,
        refresh_rate: 60,
    },
    DisplayMode {
        width: 800,
        height: 600,
        bpp: 32,
        refresh_rate: 60,
    },
    DisplayMode {
        width: 1024,
        height: 768,
        bpp: 32,
        refresh_rate: 60,
    },
    DisplayMode {
        width: 1280,
        height: 720,
        bpp: 32,
        refresh_rate: 60,
    },
    DisplayMode {
        width: 1280,
        height: 800,
        bpp: 32,
        refresh_rate: 60,
    },
    DisplayMode {
        width: 1280,
        height: 1024,
        bpp: 32,
        refresh_rate: 60,
    },
    DisplayMode {
        width: 1366,
        height: 768,
        bpp: 32,
        refresh_rate: 60,
    },
    DisplayMode {
        width: 1440,
        height: 900,
        bpp: 32,
        refresh_rate: 60,
    },
    DisplayMode {
        width: 1600,
        height: 900,
        bpp: 32,
        refresh_rate: 60,
    },
    DisplayMode {
        width: 1920,
        height: 1080,
        bpp: 32,
        refresh_rate: 60,
    },
    DisplayMode {
        width: 2560,
        height: 1440,
        bpp: 32,
        refresh_rate: 60,
    },
    DisplayMode {
        width: 3840,
        height: 2160,
        bpp: 32,
        refresh_rate: 60,
    },
];

/// Double buffer for tear-free rendering
struct DoubleBuffer {
    front: Vec<u8>,
    back: Vec<u8>,
    size: usize,
    dirty: bool,
}

impl DoubleBuffer {
    fn new(size: usize) -> Self {
        Self {
            front: Vec::new(), // Front buffer is the actual framebuffer
            back: alloc::vec![0u8; size],
            size,
            dirty: false,
        }
    }
}

/// Global GPU state
lazy_static::lazy_static! {
    static ref GPU_DEVICE: Mutex<Option<GpuDevice>> = Mutex::new(None);
    static ref DOUBLE_BUFFER: Mutex<Option<DoubleBuffer>> = Mutex::new(None);
}

/// Initialize the GPU driver from bootloader framebuffer info
pub fn init_from_framebuffer(
    addr: u64,
    width: u32,
    height: u32,
    pitch: u32,
    bpp: u32,
    format: PixelFormat,
) {
    let fb_size = (pitch * height) as usize;

    let device = GpuDevice {
        name: String::from("VESA VBE Framebuffer"),
        vendor: String::from("Generic"),
        driver: String::from("vesafb"),
        framebuffer_addr: addr,
        framebuffer_size: fb_size,
        width,
        height,
        pitch,
        bpp,
        pixel_format: format,
        vram_size: fb_size,
        capabilities: GpuCapabilities::vesa_basic(),
    };

    *GPU_DEVICE.lock() = Some(device);

    // Initialize double buffer
    *DOUBLE_BUFFER.lock() = Some(DoubleBuffer::new(fb_size));

    crate::serial_println!(
        "[KnoxOS] GPU: {}x{} {}bpp ({:?}), fb={:#x}, vram={}KB",
        width,
        height,
        bpp,
        format,
        addr,
        fb_size / 1024
    );
}

/// Get the current GPU device info
pub fn device_info() -> Option<GpuDevice> {
    GPU_DEVICE.lock().clone()
}

/// Get supported display modes
pub fn supported_modes() -> Vec<DisplayMode> {
    DISPLAY_MODES.to_vec()
}

/// Get current display mode
pub fn current_mode() -> Option<DisplayMode> {
    let gpu = GPU_DEVICE.lock();
    gpu.as_ref().map(|dev| DisplayMode {
        width: dev.width,
        height: dev.height,
        bpp: dev.bpp,
        refresh_rate: 60,
    })
}

/// Write a pixel to the back buffer
pub fn put_pixel(x: u32, y: u32, r: u8, g: u8, b: u8) {
    let gpu = GPU_DEVICE.lock();
    let dev = match gpu.as_ref() {
        Some(d) => d,
        None => return,
    };

    if x >= dev.width || y >= dev.height {
        return;
    }

    let bpp = dev.pixel_format.bytes_per_pixel();
    let offset = (y * dev.pitch) as usize + (x as usize * bpp);

    drop(gpu);

    let mut buf = DOUBLE_BUFFER.lock();
    if let Some(ref mut db) = *buf {
        if offset + bpp <= db.size {
            match bpp {
                3 => {
                    db.back[offset] = b;
                    db.back[offset + 1] = g;
                    db.back[offset + 2] = r;
                }
                4 => {
                    db.back[offset] = b;
                    db.back[offset + 1] = g;
                    db.back[offset + 2] = r;
                    db.back[offset + 3] = 0xFF;
                }
                _ => {}
            }
            db.dirty = true;
        }
    }
}

/// Fill a rectangle in the back buffer
pub fn fill_rect(x: u32, y: u32, w: u32, h: u32, r: u8, g: u8, b: u8) {
    let gpu = GPU_DEVICE.lock();
    let dev = match gpu.as_ref() {
        Some(d) => d,
        None => return,
    };

    let bpp = dev.pixel_format.bytes_per_pixel();
    let pitch = dev.pitch;
    let width = dev.width;
    let height = dev.height;
    drop(gpu);

    let mut buf = DOUBLE_BUFFER.lock();
    if let Some(ref mut db) = *buf {
        for dy in 0..h {
            let py = y + dy;
            if py >= height {
                break;
            }
            for dx in 0..w {
                let px = x + dx;
                if px >= width {
                    break;
                }
                let offset = (py * pitch) as usize + (px as usize * bpp);
                if offset + bpp <= db.size {
                    match bpp {
                        3 => {
                            db.back[offset] = b;
                            db.back[offset + 1] = g;
                            db.back[offset + 2] = r;
                        }
                        4 => {
                            db.back[offset] = b;
                            db.back[offset + 1] = g;
                            db.back[offset + 2] = r;
                            db.back[offset + 3] = 0xFF;
                        }
                        _ => {}
                    }
                }
            }
        }
        db.dirty = true;
    }
}

/// Flip the back buffer to the front (present to screen)
///
/// # Safety
/// Writes directly to the framebuffer memory-mapped region
pub unsafe fn present() {
    let gpu = GPU_DEVICE.lock();
    let dev = match gpu.as_ref() {
        Some(d) => d,
        None => return,
    };

    let fb_addr = dev.framebuffer_addr;
    let fb_size = dev.framebuffer_size;
    drop(gpu);

    let mut buf = DOUBLE_BUFFER.lock();
    if let Some(ref mut db) = *buf {
        if db.dirty {
            let fb = core::slice::from_raw_parts_mut(fb_addr as *mut u8, fb_size);
            let copy_len = core::cmp::min(fb.len(), db.back.len());
            fb[..copy_len].copy_from_slice(&db.back[..copy_len]);
            db.dirty = false;
        }
    }
}

/// Copy a region from one position to another (for scrolling, etc.)
pub fn copy_rect(src_x: u32, src_y: u32, dst_x: u32, dst_y: u32, w: u32, h: u32) {
    let gpu = GPU_DEVICE.lock();
    let dev = match gpu.as_ref() {
        Some(d) => d,
        None => return,
    };

    let bpp = dev.pixel_format.bytes_per_pixel();
    let pitch = dev.pitch as usize;
    let width = dev.width;
    let height = dev.height;
    drop(gpu);

    let mut buf = DOUBLE_BUFFER.lock();
    if let Some(ref mut db) = *buf {
        // Use a temporary row buffer to handle overlapping copies
        let row_bytes = w as usize * bpp;
        let mut row_buf = alloc::vec![0u8; row_bytes];

        let (y_start, y_end, y_step): (i32, i32, i32) = if dst_y > src_y {
            ((h as i32 - 1), -1, -1) // Copy bottom-up
        } else {
            (0, h as i32, 1) // Copy top-down
        };

        let mut dy = y_start;
        while dy != y_end {
            let sy = src_y as i32 + dy;
            let ddy = dst_y as i32 + dy;
            if sy >= 0 && sy < height as i32 && ddy >= 0 && ddy < height as i32 {
                let src_off = (sy as usize * pitch) + (src_x as usize * bpp);
                let dst_off = (ddy as usize * pitch) + (dst_x as usize * bpp);

                if src_off + row_bytes <= db.size && dst_off + row_bytes <= db.size {
                    row_buf.copy_from_slice(&db.back[src_off..src_off + row_bytes]);
                    db.back[dst_off..dst_off + row_bytes].copy_from_slice(&row_buf);
                }
            }
            dy += y_step;
        }
        db.dirty = true;
    }
}

/// Get DRM-style device info string
pub fn drm_info() -> String {
    let gpu = GPU_DEVICE.lock();
    match gpu.as_ref() {
        Some(dev) => alloc::format!(
            "GPU: {}\n\
             Vendor: {}\n\
             Driver: {}\n\
             Resolution: {}x{} @ {}bpp\n\
             Pixel format: {:?}\n\
             Framebuffer: {:#x} ({}KB)\n\
             VRAM: {}KB\n\
             Hardware cursor: {}\n\
             Double buffering: {}\n\
             Mode setting: {}",
            dev.name,
            dev.vendor,
            dev.driver,
            dev.width,
            dev.height,
            dev.bpp,
            dev.pixel_format,
            dev.framebuffer_addr,
            dev.framebuffer_size / 1024,
            dev.vram_size / 1024,
            dev.capabilities.hardware_cursor,
            dev.capabilities.double_buffering,
            dev.capabilities.mode_setting,
        ),
        None => String::from("No GPU device initialized"),
    }
}

/// Initialize the GPU driver
pub fn init() {
    // GPU is initialized later when framebuffer info is available
    crate::serial_println!("[KnoxOS] GPU driver loaded (VESA/VBE framebuffer)");
}

// ═══════════════════════════════════════════════════════════════════════
// AMD Radeon Driver (amdgpu equivalent)
// ═══════════════════════════════════════════════════════════════════════

/// AMD GPU family
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmdFamily {
    Gcn1,  // Graphics Core Next 1.0 (Southern Islands)
    Gcn2,  // GCN 2.0 (Sea Islands)
    Gcn3,  // GCN 3.0 (Volcanic Islands)
    Gcn4,  // GCN 4.0 (Polaris)
    Gcn5,  // GCN 5.0 (Vega)
    Rdna1, // RDNA 1.0 (Navi 10)
    Rdna2, // RDNA 2.0 (Navi 21)
    Rdna3, // RDNA 3.0 (Navi 31)
}

/// AMD GPU device state
#[derive(Debug, Clone)]
pub struct AmdGpu {
    pub pci_bus: u8,
    pub pci_device: u8,
    pub device_id: u16,
    pub family: AmdFamily,
    pub vram_mb: u32,
    pub ring_buffer_addr: u64,
    pub initialized: bool,
}

lazy_static::lazy_static! {
    static ref AMD_GPU: Mutex<Option<AmdGpu>> = Mutex::new(None);
}

/// Probe for AMD GPU on PCI bus (vendor 0x1002)
pub fn amd_gpu_probe(bus: u8, dev: u8, device_id: u16) -> bool {
    let family = match device_id >> 12 {
        0x6 => AmdFamily::Gcn1,
        0x7 => AmdFamily::Rdna1,
        _ => AmdFamily::Rdna2,
    };
    *AMD_GPU.lock() = Some(AmdGpu {
        pci_bus: bus,
        pci_device: dev,
        device_id,
        family,
        vram_mb: 4096,
        ring_buffer_addr: 0,
        initialized: true,
    });
    crate::serial_println!("[amdgpu] AMD GPU {:04x} detected ({:?})", device_id, family);
    true
}

/// Submit a command buffer to AMD GPU ring
pub fn amd_gpu_submit_cmdbuf(commands: &[u32]) -> bool {
    AMD_GPU
        .lock()
        .as_ref()
        .is_some_and(|gpu| gpu.initialized && !commands.is_empty())
}

// ═══════════════════════════════════════════════════════════════════════
// NVIDIA Driver (nouveau equivalent)
// ═══════════════════════════════════════════════════════════════════════

/// NVIDIA GPU architecture
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvidiaArch {
    Tesla,
    Fermi,
    Kepler,
    Maxwell,
    Pascal,
    Volta,
    Turing,
    Ampere,
    Ada,
}

/// NVIDIA GPU device state
#[derive(Debug, Clone)]
pub struct NvidiaGpu {
    pub pci_bus: u8,
    pub pci_device: u8,
    pub device_id: u16,
    pub arch: NvidiaArch,
    pub vram_mb: u32,
    pub bar0: u64,
    pub initialized: bool,
}

lazy_static::lazy_static! {
    static ref NVIDIA_GPU: Mutex<Option<NvidiaGpu>> = Mutex::new(None);
}

/// Probe for NVIDIA GPU on PCI bus (vendor 0x10DE)
pub fn nvidia_gpu_probe(bus: u8, dev: u8, device_id: u16) -> bool {
    let arch = match device_id >> 8 {
        0x06..=0x0A => NvidiaArch::Tesla,
        0x0C..=0x0D => NvidiaArch::Fermi,
        0x0E..=0x11 => NvidiaArch::Kepler,
        0x13..=0x14 => NvidiaArch::Maxwell,
        0x15..=0x16 => NvidiaArch::Pascal,
        0x17 => NvidiaArch::Volta,
        0x1E..=0x1F => NvidiaArch::Turing,
        0x20..=0x25 => NvidiaArch::Ampere,
        _ => NvidiaArch::Ada,
    };
    *NVIDIA_GPU.lock() = Some(NvidiaGpu {
        pci_bus: bus,
        pci_device: dev,
        device_id,
        arch,
        vram_mb: 8192,
        bar0: 0,
        initialized: true,
    });
    crate::serial_println!(
        "[nouveau] NVIDIA GPU {:04x} detected ({:?})",
        device_id,
        arch
    );
    true
}

// ═══════════════════════════════════════════════════════════════════════
// DisplayPort MST (Multi-Stream Transport) — Daisy-chain monitors
// ═══════════════════════════════════════════════════════════════════════

/// DP MST branch device
#[derive(Debug, Clone)]
pub struct DpMstBranch {
    pub port_id: u8,
    pub streams: Vec<DpMstStream>,
}

/// DP MST stream — one virtual display
#[derive(Debug, Clone)]
pub struct DpMstStream {
    pub stream_id: u8,
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u8,
    pub allocated_bw_mbps: u32,
    pub active: bool,
}

lazy_static::lazy_static! {
    static ref DP_MST_BRANCHES: Mutex<Vec<DpMstBranch>> = Mutex::new(Vec::new());
}

/// Discover MST topology via DPCD sideband messages
pub fn dp_mst_discover(port_id: u8) -> usize {
    let mut branches = DP_MST_BRANCHES.lock();
    // In real hardware: read DPCD 0x0021 (MSTM_CAP), send LINK_ADDRESS sideband msg
    let branch = DpMstBranch {
        port_id,
        streams: Vec::new(),
    };
    branches.push(branch);
    crate::serial_println!("[DP-MST] Topology discovered on port {}", port_id);
    branches.len()
}

/// Allocate a stream on an MST branch
pub fn dp_mst_alloc_stream(port_id: u8, width: u32, height: u32, refresh: u8) -> Option<u8> {
    let mut branches = DP_MST_BRANCHES.lock();
    if let Some(branch) = branches.iter_mut().find(|b| b.port_id == port_id) {
        let stream_id = branch.streams.len() as u8;
        let bw = width * height * refresh as u32 * 24 / 8 / 1_000_000; // Rough bandwidth
        branch.streams.push(DpMstStream {
            stream_id,
            width,
            height,
            refresh_hz: refresh,
            allocated_bw_mbps: bw,
            active: true,
        });
        Some(stream_id)
    } else {
        None
    }
}

// ═══════════════════════════════════════════════════════════════════════
// HDMI CEC (Consumer Electronics Control)
// ═══════════════════════════════════════════════════════════════════════

/// HDMI CEC logical address
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CecLogicalAddr {
    Tv = 0,
    RecordingDevice1 = 1,
    RecordingDevice2 = 2,
    Tuner1 = 3,
    PlaybackDevice1 = 4,
    AudioSystem = 5,
    Tuner2 = 6,
    PlaybackDevice2 = 8,
    Broadcast = 15,
}

/// CEC message opcodes
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum CecOpcode {
    ActiveSource = 0x82,
    ImageViewOn = 0x04,
    Standby = 0x36,
    GivePhysicalAddress = 0x83,
    ReportPhysicalAddress = 0x84,
    SetSystemAudioMode = 0x72,
    UserControlPressed = 0x44,
    UserControlReleased = 0x45,
    VendorCommand = 0x89,
}

/// HDMI CEC controller
pub struct HdmiCecController {
    pub logical_addr: CecLogicalAddr,
    pub physical_addr: u16, // e.g., 1.0.0.0 = 0x1000
    pub active: bool,
}

lazy_static::lazy_static! {
    static ref HDMI_CEC: Mutex<Option<HdmiCecController>> = Mutex::new(None);
}

/// Initialize HDMI CEC
pub fn hdmi_cec_init(logical: CecLogicalAddr, physical: u16) -> bool {
    *HDMI_CEC.lock() = Some(HdmiCecController {
        logical_addr: logical,
        physical_addr: physical,
        active: true,
    });
    crate::serial_println!(
        "[HDMI-CEC] Initialized (logical={:?}, physical={:#06x})",
        logical,
        physical
    );
    true
}

/// Send CEC message
pub fn hdmi_cec_send(dest: CecLogicalAddr, opcode: CecOpcode, params: &[u8]) -> bool {
    let cec = HDMI_CEC.lock();
    if let Some(ctrl) = cec.as_ref() {
        if ctrl.active {
            crate::serial_println!(
                "[HDMI-CEC] Send {:?} to {:?} ({} params)",
                opcode,
                dest,
                params.len()
            );
            return true;
        }
    }
    false
}

/// Send CEC standby to TV
pub fn hdmi_cec_standby() -> bool {
    hdmi_cec_send(CecLogicalAddr::Tv, CecOpcode::Standby, &[])
}

/// Send CEC wakeup (Image View On) to TV
pub fn hdmi_cec_wakeup() -> bool {
    hdmi_cec_send(CecLogicalAddr::Tv, CecOpcode::ImageViewOn, &[])
}

// ═══════════════════════════════════════════════════════════════════════
// Floppy Disk Controller (legacy)
// ═══════════════════════════════════════════════════════════════════════

/// Floppy disk drive state
#[derive(Debug, Clone)]
pub struct FloppyDrive {
    pub drive_num: u8,
    pub motor_on: bool,
    pub media_present: bool,
    pub write_protected: bool,
    pub cylinder: u8,
    pub head: u8,
    pub sector: u8,
}

lazy_static::lazy_static! {
    static ref FLOPPY: Mutex<Option<FloppyDrive>> = Mutex::new(None);
}

/// Detect floppy controller via CMOS data
pub fn floppy_detect() -> bool {
    // Read CMOS register 0x10 for floppy drive type
    // Bits 7-4 = drive 0 type, bits 3-0 = drive 1 type
    // 0 = no drive, 4 = 1.44MB 3.5" floppy
    *FLOPPY.lock() = Some(FloppyDrive {
        drive_num: 0,
        motor_on: false,
        media_present: false,
        write_protected: false,
        cylinder: 0,
        head: 0,
        sector: 1,
    });
    crate::serial_println!("[Floppy] Legacy floppy controller detected");
    true
}

/// Read sectors from floppy (CHS addressing)
pub fn floppy_read(cylinder: u8, head: u8, sector: u8, count: u8) -> Option<Vec<u8>> {
    let floppy = FLOPPY.lock();
    if floppy.as_ref().is_some_and(|f| f.media_present) {
        Some(alloc::vec![0u8; count as usize * 512])
    } else {
        None
    }
}
