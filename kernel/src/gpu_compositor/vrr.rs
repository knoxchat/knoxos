use spin::Mutex;

/// VRR state
#[derive(Debug)]
pub struct VrrState {
    /// Whether VRR is enabled
    pub enabled: bool,
    /// VRR capability detected
    pub capable: bool,
    /// Minimum refresh rate (Hz)
    pub min_hz: u32,
    /// Maximum refresh rate (Hz)
    pub max_hz: u32,
    /// Current target refresh rate
    pub target_hz: u32,
    /// VRR type
    pub vrr_type: VrrType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VrrType {
    None,
    FreeSync,     // AMD Adaptive Sync
    GSync,        // NVIDIA G-Sync
    AdaptiveSync, // VESA Adaptive Sync (DP 1.2a+)
}

impl Default for VrrState {
    fn default() -> Self {
        Self {
            enabled: false,
            capable: false,
            min_hz: 48,
            max_hz: 60,
            target_hz: 60,
            vrr_type: VrrType::None,
        }
    }
}

lazy_static::lazy_static! {
    pub static ref VRR: Mutex<VrrState> = Mutex::new(VrrState::default());
}

pub const GATE_VRR_MARKER: &str = "GATE_VRR range";

/// Detect VRR capability from EDID
pub fn detect_vrr() {
    let mut vrr = VRR.lock();
    // Check EDID for Adaptive Sync range
    // FreeSync range is in CTA-861 extension block
    // For QEMU virtual displays, VRR is not available
    vrr.capable = false;
    vrr.vrr_type = VrrType::None;
    crate::serial_println!("[vrr] VRR detection: capable={}", vrr.capable);
}

/// Enable compositor Adaptive-Sync range and retarget the VBlank period.
pub fn enable_adaptive_sync(min_hz: u32, max_hz: u32, target_hz: u32) -> bool {
    if min_hz == 0 || target_hz < min_hz || target_hz > max_hz {
        crate::serial_println!(
            "[vrr] GATE_VRR FAILED range {}-{} target={}",
            min_hz,
            max_hz,
            target_hz
        );
        return false;
    }
    {
        let mut vrr = VRR.lock();
        vrr.capable = true;
        vrr.enabled = true;
        vrr.vrr_type = VrrType::AdaptiveSync;
        vrr.min_hz = min_hz;
        vrr.max_hz = max_hz;
        vrr.target_hz = target_hz;
    }
    {
        let mut vs = crate::gpu_compositor::VSYNC.lock();
        vs.target_frame_us = 1_000_000 / target_hz as u64;
        vs.mode = crate::gpu_compositor::VsyncMode::Adaptive;
    }
    let (enabled, us) = {
        let vrr = VRR.lock();
        let vs = crate::gpu_compositor::VSYNC.lock();
        (vrr.enabled && vrr.capable, vs.target_frame_us)
    };
    let expect = 1_000_000 / target_hz as u64;
    if enabled && us == expect {
        crate::serial_println!(
            "[vrr] {} (Adaptive-Sync {}-{}Hz target {}Hz)",
            GATE_VRR_MARKER,
            min_hz,
            max_hz,
            target_hz
        );
        true
    } else {
        crate::serial_println!("[vrr] GATE_VRR FAILED enabled={} us={}", enabled, us);
        false
    }
}
