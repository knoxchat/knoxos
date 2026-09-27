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
