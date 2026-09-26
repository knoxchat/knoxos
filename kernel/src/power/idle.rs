use spin::Mutex;

use crate::serial_println;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Idle Power Saving   (31.9)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Screen dimming configuration
pub struct ScreenDimConfig {
    /// Seconds of inactivity before dimming (0 = disable)
    pub dim_after_secs: u64,
    /// Brightness percentage when dimmed (0–100)
    pub dim_brightness: u8,
    /// Seconds of inactivity before screen off (0 = disable)
    pub off_after_secs: u64,
}

static SCREEN_DIM_CONFIG: Mutex<ScreenDimConfig> = Mutex::new(ScreenDimConfig {
    dim_after_secs: 300,
    dim_brightness: 30,
    off_after_secs: 600,
});

/// Last user input timestamp (TSC ticks)
static LAST_INPUT_TICK: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Screen state: 0 = normal, 1 = dimmed, 2 = off
static SCREEN_STATE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

/// Record user input activity (call from input handler)
pub fn record_user_activity() {
    LAST_INPUT_TICK.store(
        crate::hpet::read_counter(),
        core::sync::atomic::Ordering::Relaxed,
    );
    let prev = SCREEN_STATE.swap(0, core::sync::atomic::Ordering::Relaxed);
    if prev != 0 {
        serial_println!("[PM] Screen wakeup — user activity detected");
    }
}

/// Tick the idle power manager (call periodically from timer)
pub fn idle_power_tick(current_tick: u64, ticks_per_sec: u64) {
    let last = LAST_INPUT_TICK.load(core::sync::atomic::Ordering::Relaxed);
    if last == 0 || ticks_per_sec == 0 {
        return;
    }
    let elapsed_secs = (current_tick.saturating_sub(last)) / ticks_per_sec;
    let cfg = SCREEN_DIM_CONFIG.lock();
    let state = SCREEN_STATE.load(core::sync::atomic::Ordering::Relaxed);

    if cfg.off_after_secs > 0 && elapsed_secs >= cfg.off_after_secs && state < 2 {
        SCREEN_STATE.store(2, core::sync::atomic::Ordering::Relaxed);
        serial_println!("[PM] Screen off — {} seconds idle", elapsed_secs);
    } else if cfg.dim_after_secs > 0 && elapsed_secs >= cfg.dim_after_secs && state < 1 {
        SCREEN_STATE.store(1, core::sync::atomic::Ordering::Relaxed);
        serial_println!(
            "[PM] Screen dimmed to {}% — {} seconds idle",
            cfg.dim_brightness,
            elapsed_secs
        );
    }
}

/// Get current screen state (0=normal, 1=dimmed, 2=off)
pub fn screen_idle_state() -> u8 {
    SCREEN_STATE.load(core::sync::atomic::Ordering::Relaxed)
}

/// Set screen dim configuration
pub fn set_screen_dim_config(dim_secs: u64, brightness: u8, off_secs: u64) {
    let mut cfg = SCREEN_DIM_CONFIG.lock();
    cfg.dim_after_secs = dim_secs;
    cfg.dim_brightness = brightness.min(100);
    cfg.off_after_secs = off_secs;
}
