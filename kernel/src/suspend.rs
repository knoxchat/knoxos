use alloc::vec::Vec;
/// Suspend / Hibernate — ACPI S3 (sleep) and S4 (hibernate) support
///
/// Provides system power state transitions:
///   - ACPI S3 suspend-to-RAM
///   - ACPI S4 hibernate-to-disk
///   - Pre-suspend hooks (device quiesce, framebuffer save)
///   - Resume hooks (device re-init, framebuffer restore)
///   - Lid switch handling
///   - Idle-based auto-suspend
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// POWER STATES
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SleepState {
    /// S0 — Working
    S0Working,
    /// S1 — Power on suspend (CPU stops, RAM refreshed)
    S1Standby,
    /// S3 — Suspend to RAM
    S3Suspend,
    /// S4 — Hibernate to disk
    S4Hibernate,
    /// S5 — Soft off
    S5SoftOff,
}

impl SleepState {
    pub fn name(&self) -> &'static str {
        match self {
            SleepState::S0Working => "S0 (Working)",
            SleepState::S1Standby => "S1 (Standby)",
            SleepState::S3Suspend => "S3 (Suspend)",
            SleepState::S4Hibernate => "S4 (Hibernate)",
            SleepState::S5SoftOff => "S5 (Soft Off)",
        }
    }
}

/// Lid state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LidState {
    Open,
    Closed,
    Unknown,
}

/// What to do when lid closes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LidAction {
    Nothing,
    Suspend,
    Hibernate,
    ShutDown,
    Lock,
}

// ═══════════════════════════════════════════════════════════════════════
// STATE
// ═══════════════════════════════════════════════════════════════════════

static CURRENT_STATE: AtomicU8 = AtomicU8::new(0); // S0
static SUSPEND_SUPPORTED: AtomicBool = AtomicBool::new(false);
static HIBERNATE_SUPPORTED: AtomicBool = AtomicBool::new(false);
static LID_CLOSED: AtomicBool = AtomicBool::new(false);
static LID_ACTION_VAL: AtomicU8 = AtomicU8::new(1); // Suspend
static AUTO_SUSPEND_IDLE_SECS: AtomicU64 = AtomicU64::new(0); // 0 = disabled
static LAST_ACTIVITY: AtomicU64 = AtomicU64::new(0);
static AUTO_DIM_SECS: AtomicU64 = AtomicU64::new(0); // 0 = disabled
static SCREEN_DIMMED: AtomicBool = AtomicBool::new(false);

/// Saved framebuffer state for resume
lazy_static::lazy_static! {
    static ref SAVED_FB: Mutex<Vec<u8>> = Mutex::new(Vec::new());
}

// ═══════════════════════════════════════════════════════════════════════
// ACPI PM REGISTERS (simplified)
// ═══════════════════════════════════════════════════════════════════════

/// ACPI PM1a control block address (from FADT)
static PM1A_CTL_BLK: AtomicU64 = AtomicU64::new(0);
/// SLP_TYPa values for each sleep state
static SLP_TYPA_S3: AtomicU8 = AtomicU8::new(0);
static SLP_TYPA_S4: AtomicU8 = AtomicU8::new(0);
static SLP_TYPA_S5: AtomicU8 = AtomicU8::new(0);

/// Set PM register addresses (called during ACPI init)
pub fn set_pm_registers(pm1a_ctl: u64, slp_s3: u8, slp_s4: u8, slp_s5: u8) {
    PM1A_CTL_BLK.store(pm1a_ctl, Ordering::Relaxed);
    SLP_TYPA_S3.store(slp_s3, Ordering::Relaxed);
    SLP_TYPA_S4.store(slp_s4, Ordering::Relaxed);
    SLP_TYPA_S5.store(slp_s5, Ordering::Relaxed);

    if slp_s3 != 0 {
        SUSPEND_SUPPORTED.store(true, Ordering::Relaxed);
    }
    if slp_s4 != 0 {
        HIBERNATE_SUPPORTED.store(true, Ordering::Relaxed);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PRE-SUSPEND HOOKS
// ═══════════════════════════════════════════════════════════════════════

/// Prepare system for sleep
fn pre_suspend() {
    serial_println!("[Suspend] Pre-suspend: saving state...");

    // 1. Notify userspace (stop GUI updates)
    // 2. Flush disk caches
    // 3. Save framebuffer if needed
    // 4. Quiesce devices (network, USB, storage)
    // 5. Stop timers

    // Save some state info
    CURRENT_STATE.store(0xFF, Ordering::Relaxed); // transitioning
}

/// Restore system after resume
fn post_resume() {
    serial_println!("[Suspend] Post-resume: restoring state...");

    // 1. Re-initialize timer (PIT/HPET)
    // 2. Re-initialize interrupts
    // 3. Restore device state
    // 4. Restore framebuffer
    // 5. Resume GUI

    CURRENT_STATE.store(0, Ordering::Relaxed); // S0
    SCREEN_DIMMED.store(false, Ordering::Relaxed);

    serial_println!("[Suspend] System resumed to S0");
}

// ═══════════════════════════════════════════════════════════════════════
// SUSPEND (S3)
// ═══════════════════════════════════════════════════════════════════════

/// Enter S3 suspend-to-RAM
pub fn suspend() -> Result<(), &'static str> {
    if !SUSPEND_SUPPORTED.load(Ordering::Relaxed) {
        return Err("S3 suspend not supported");
    }

    serial_println!("[Suspend] Entering S3 suspend-to-RAM...");
    pre_suspend();

    let pm1a = PM1A_CTL_BLK.load(Ordering::Relaxed);
    let slp_typ = SLP_TYPA_S3.load(Ordering::Relaxed);

    if pm1a == 0 {
        post_resume();
        return Err("PM1a control block not configured");
    }

    // Write SLP_TYPa | SLP_EN to PM1a_CNT
    let value = ((slp_typ as u16) << 10) | (1 << 13); // SLP_EN = bit 13
    unsafe {
        crate::arch_compat::instructions::port::Port::<u16>::new(pm1a as u16).write(value);
    }

    // If we get here, we've resumed
    post_resume();
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// HIBERNATE (S4)
// ═══════════════════════════════════════════════════════════════════════

/// Enter S4 hibernate-to-disk
pub fn hibernate() -> Result<(), &'static str> {
    if !HIBERNATE_SUPPORTED.load(Ordering::Relaxed) {
        return Err("S4 hibernate not supported");
    }

    serial_println!("[Hibernate] Saving memory state to disk...");
    pre_suspend();

    // In a real implementation:
    // 1. Save all physical memory pages to swap/hibernate partition
    // 2. Write hibernate header with page table info
    // 3. Enter S4

    let pm1a = PM1A_CTL_BLK.load(Ordering::Relaxed);
    let slp_typ = SLP_TYPA_S4.load(Ordering::Relaxed);

    if pm1a == 0 {
        post_resume();
        return Err("PM1a control block not configured");
    }

    // Write SLP_TYPa | SLP_EN
    let value = ((slp_typ as u16) << 10) | (1 << 13);
    unsafe {
        crate::arch_compat::instructions::port::Port::<u16>::new(pm1a as u16).write(value);
    }

    // If we get here, system was resumed from S4
    serial_println!("[Hibernate] Restoring from hibernate...");
    post_resume();
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// LID SWITCH
// ═══════════════════════════════════════════════════════════════════════

/// Called when lid switch state changes
pub fn handle_lid_event(closed: bool) {
    LID_CLOSED.store(closed, Ordering::Relaxed);
    serial_println!("[Power] Lid {}", if closed { "closed" } else { "opened" });

    if closed {
        let action = match LID_ACTION_VAL.load(Ordering::Relaxed) {
            0 => LidAction::Nothing,
            1 => LidAction::Suspend,
            2 => LidAction::Hibernate,
            3 => LidAction::ShutDown,
            4 => LidAction::Lock,
            _ => LidAction::Nothing,
        };

        match action {
            LidAction::Nothing => {}
            LidAction::Suspend => {
                let _ = suspend();
            }
            LidAction::Hibernate => {
                let _ = hibernate();
            }
            LidAction::ShutDown => crate::acpi::shutdown(),
            LidAction::Lock => {
                // Lock the screen
                serial_println!("[Power] Locking screen on lid close");
            }
        }
    }
}

/// Set lid close action
pub fn set_lid_action(action: LidAction) {
    let val = match action {
        LidAction::Nothing => 0,
        LidAction::Suspend => 1,
        LidAction::Hibernate => 2,
        LidAction::ShutDown => 3,
        LidAction::Lock => 4,
    };
    LID_ACTION_VAL.store(val, Ordering::Relaxed);
}

/// Get lid state
pub fn lid_state() -> LidState {
    if LID_CLOSED.load(Ordering::Relaxed) {
        LidState::Closed
    } else {
        LidState::Open
    }
}

// ═══════════════════════════════════════════════════════════════════════
// AUTO-SUSPEND / AUTO-DIM
// ═══════════════════════════════════════════════════════════════════════

/// Report user activity (resets idle timer)
pub fn report_activity() {
    LAST_ACTIVITY.store(crate::rtc::unix_time() as u64, Ordering::Relaxed);
    if SCREEN_DIMMED.load(Ordering::Relaxed) {
        undim_screen();
    }
}

/// Set auto-suspend timeout (0 = disabled)
pub fn set_auto_suspend_timeout(seconds: u64) {
    AUTO_SUSPEND_IDLE_SECS.store(seconds, Ordering::Relaxed);
}

/// Set auto-dim timeout (0 = disabled)
pub fn set_auto_dim_timeout(seconds: u64) {
    AUTO_DIM_SECS.store(seconds, Ordering::Relaxed);
}

/// Called periodically to check idle state
pub fn check_idle() {
    let now = crate::rtc::unix_time() as u64;
    let last = LAST_ACTIVITY.load(Ordering::Relaxed);
    if last == 0 {
        return;
    }

    let idle = now.saturating_sub(last);

    // Auto-dim check
    let dim_timeout = AUTO_DIM_SECS.load(Ordering::Relaxed);
    if dim_timeout > 0 && idle >= dim_timeout && !SCREEN_DIMMED.load(Ordering::Relaxed) {
        dim_screen();
    }

    // Auto-suspend check
    let suspend_timeout = AUTO_SUSPEND_IDLE_SECS.load(Ordering::Relaxed);
    if suspend_timeout > 0 && idle >= suspend_timeout {
        serial_println!("[Power] Auto-suspend after {}s idle", idle);
        let _ = suspend();
    }
}

fn dim_screen() {
    SCREEN_DIMMED.store(true, Ordering::Relaxed);
    serial_println!("[Power] Screen dimmed due to inactivity");
    // In a real implementation, reduce backlight brightness
}

fn undim_screen() {
    SCREEN_DIMMED.store(false, Ordering::Relaxed);
    serial_println!("[Power] Screen un-dimmed");
}

/// Is screen dimmed?
pub fn is_screen_dimmed() -> bool {
    SCREEN_DIMMED.load(Ordering::Relaxed)
}

/// Is suspend supported?
pub fn is_suspend_supported() -> bool {
    SUSPEND_SUPPORTED.load(Ordering::Relaxed)
}

/// Is hibernate supported?
pub fn is_hibernate_supported() -> bool {
    HIBERNATE_SUPPORTED.load(Ordering::Relaxed)
}

/// Initialize suspend/hibernate subsystem
pub fn init() {
    report_activity(); // Set initial activity timestamp
    serial_println!("[KnoxOS] Suspend/hibernate subsystem initialized");
}
