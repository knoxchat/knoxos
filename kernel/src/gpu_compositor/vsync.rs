use spin::Mutex;

/// VSync mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VsyncMode {
    /// No sync — render as fast as possible
    Off,
    /// Wait for vertical blanking interval
    On,
    /// Adaptive vsync (disable on frame drops)
    Adaptive,
    /// Triple buffering
    TripleBuffer,
}

/// Page flip state for double/triple buffering
pub struct PageFlipState {
    pub mode: VsyncMode,
    /// Current front buffer index
    pub front: u8,
    /// Back buffer index (being rendered to)
    pub back: u8,
    /// Total number of buffers
    pub buffer_count: u8,
    /// Frame counter
    pub frame_count: u64,
    /// Whether a flip is pending
    pub flip_pending: bool,
    /// VBlank counter from hardware
    pub vblank_count: u64,
    /// Target frame time in microseconds (16666 for 60Hz)
    pub target_frame_us: u64,
    /// Last frame timestamp (TSC)
    pub last_frame_tsc: u64,
}

impl PageFlipState {
    pub fn new(mode: VsyncMode) -> Self {
        let buffers = match mode {
            VsyncMode::TripleBuffer => 3,
            _ => 2,
        };
        Self {
            mode,
            front: 0,
            back: 1,
            buffer_count: buffers,
            frame_count: 0,
            flip_pending: false,
            vblank_count: 0,
            target_frame_us: 16666, // 60 Hz
            last_frame_tsc: 0,
        }
    }

    /// Request a page flip (swap front and back buffers)
    pub fn request_flip(&mut self) {
        if self.mode == VsyncMode::Off {
            // Immediate flip
            self.swap_buffers();
        } else {
            self.flip_pending = true;
        }
    }

    /// Called on VBlank interrupt — perform pending flip
    pub fn on_vblank(&mut self) {
        self.vblank_count += 1;
        if self.flip_pending {
            self.swap_buffers();
            self.flip_pending = false;
        }
    }

    fn swap_buffers(&mut self) {
        let old_front = self.front;
        self.front = self.back;
        if self.buffer_count == 3 {
            // Triple buffer: cycle through 0,1,2
            self.back = (self.back + 1) % 3;
            if self.back == self.front {
                self.back = (self.back + 1) % 3;
            }
        } else {
            self.back = old_front;
        }
        self.frame_count += 1;
    }
}

lazy_static::lazy_static! {
    pub static ref VSYNC: Mutex<PageFlipState> = Mutex::new(PageFlipState::new(VsyncMode::On));
}

/// Set VSync mode
pub fn set_vsync(mode: VsyncMode) {
    let mut state = VSYNC.lock();
    state.mode = mode;
    crate::serial_println!("[vsync] Mode set to {:?}", mode);
}
