/// Hot Corners — Screen edge triggers for quick actions
///
/// When the mouse dwells in a screen corner for a short duration, an action fires.
/// Each corner is independently configurable.
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

/// Action to trigger when a hot corner is activated
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HotCornerAction {
    None = 0,
    ShowAllWindows = 1, // Exposé / mission control
    ShowDesktop = 2,    // Minimize all windows
    NotificationCenter = 3,
    StartMenu = 4,
    LockScreen = 5,
}

impl HotCornerAction {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::ShowAllWindows,
            2 => Self::ShowDesktop,
            3 => Self::NotificationCenter,
            4 => Self::StartMenu,
            5 => Self::LockScreen,
            _ => Self::None,
        }
    }
}

/// Whether hot corners are enabled
static ENABLED: AtomicBool = AtomicBool::new(true);

/// Action for each corner: [TopLeft, TopRight, BottomLeft, BottomRight]
static CORNER_ACTIONS: [AtomicU8; 4] = [
    AtomicU8::new(1), // TopLeft = ShowAllWindows
    AtomicU8::new(3), // TopRight = NotificationCenter
    AtomicU8::new(4), // BottomLeft = StartMenu
    AtomicU8::new(2), // BottomRight = ShowDesktop
];

/// TSC timestamp when the mouse entered a corner (0 = not in corner)
static CORNER_ENTER_TSC: AtomicU64 = AtomicU64::new(0);
/// Which corner the mouse is in (0-3, or 255 = none)
static ACTIVE_CORNER: AtomicU8 = AtomicU8::new(255);
/// Cooldown — prevents re-triggering immediately after activation
static COOLDOWN_TSC: AtomicU64 = AtomicU64::new(0);

/// Corner detection size in pixels
const CORNER_SIZE: i32 = 3;
/// Dwell time before activation (~300ms in TSC ticks, calibrated at ~2GHz)
const DWELL_TICKS: u64 = 600_000_000;
/// Cooldown period after activation (~1s)
const COOLDOWN_TICKS: u64 = 2_000_000_000;

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn set_action(corner: usize, action: HotCornerAction) {
    if corner < 4 {
        CORNER_ACTIONS[corner].store(action as u8, Ordering::Relaxed);
    }
}

pub fn get_action(corner: usize) -> HotCornerAction {
    if corner < 4 {
        HotCornerAction::from_u8(CORNER_ACTIONS[corner].load(Ordering::Relaxed))
    } else {
        HotCornerAction::None
    }
}

/// Check mouse position and trigger hot corner actions.
/// Called from the input handler on mouse move.
pub fn update(mx: i32, my: i32, screen_w: i32, screen_h: i32) {
    if !is_enabled() {
        return;
    }

    let tsc = super::read_tsc_public();

    // Check cooldown
    let cooldown = COOLDOWN_TSC.load(Ordering::Relaxed);
    if cooldown > 0 && tsc.wrapping_sub(cooldown) < COOLDOWN_TICKS {
        // Still in cooldown — but clear corner if mouse left
        let corner = detect_corner(mx, my, screen_w, screen_h);
        if corner == 255 {
            COOLDOWN_TSC.store(0, Ordering::Relaxed);
        }
        return;
    }

    let corner = detect_corner(mx, my, screen_w, screen_h);
    let prev_corner = ACTIVE_CORNER.load(Ordering::Relaxed);

    if corner == 255 {
        // Not in any corner
        ACTIVE_CORNER.store(255, Ordering::Relaxed);
        CORNER_ENTER_TSC.store(0, Ordering::Relaxed);
        return;
    }

    if corner != prev_corner {
        // Entered a new corner — start dwell timer
        ACTIVE_CORNER.store(corner, Ordering::Relaxed);
        CORNER_ENTER_TSC.store(tsc, Ordering::Relaxed);
        return;
    }

    // Same corner — check dwell time
    let enter_tsc = CORNER_ENTER_TSC.load(Ordering::Relaxed);
    if enter_tsc > 0 && tsc.wrapping_sub(enter_tsc) >= DWELL_TICKS {
        // Trigger!
        let action = get_action(corner as usize);
        execute_action(action);
        // Set cooldown and reset
        COOLDOWN_TSC.store(tsc, Ordering::Relaxed);
        CORNER_ENTER_TSC.store(0, Ordering::Relaxed);
    }
}

/// Detect which corner the mouse is in (0-3) or 255 for none
fn detect_corner(mx: i32, my: i32, screen_w: i32, screen_h: i32) -> u8 {
    let in_left = mx < CORNER_SIZE;
    let in_right = mx >= screen_w - CORNER_SIZE;
    let in_top = my < CORNER_SIZE;
    let in_bottom = my >= screen_h - CORNER_SIZE;

    if in_top && in_left {
        0
    }
    // TopLeft
    else if in_top && in_right {
        1
    }
    // TopRight
    else if in_bottom && in_left {
        2
    }
    // BottomLeft
    else if in_bottom && in_right {
        3
    }
    // BottomRight
    else {
        255
    }
}

/// Execute the action for a hot corner
fn execute_action(action: HotCornerAction) {
    match action {
        HotCornerAction::None => {}
        HotCornerAction::ShowAllWindows => {
            // Toggle alt-tab overlay (exposé-like)
            if super::alt_tab::is_visible() {
                super::alt_tab::dismiss();
            } else {
                super::alt_tab::show(false);
            }
            super::request_redraw();
        }
        HotCornerAction::ShowDesktop => {
            // Minimize all visible windows
            let mut wm = super::window::WINDOW_MANAGER.lock();
            let ids: alloc::vec::Vec<_> = wm
                .windows
                .iter()
                .filter(|w| w.is_visible())
                .map(|w| w.id)
                .collect();
            let (sw, sh) = super::screen_size();
            for id in ids {
                wm.minimize_window(id);
            }
            drop(wm);
            super::request_redraw();
        }
        HotCornerAction::NotificationCenter => {
            super::notifications::toggle_panel();
            super::request_redraw();
        }
        HotCornerAction::StartMenu => {
            super::startmenu::toggle();
            super::request_redraw();
        }
        HotCornerAction::LockScreen => {
            super::lock_screen::lock();
            super::request_redraw();
        }
    }
}
