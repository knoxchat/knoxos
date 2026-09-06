// SPDX-License-Identifier: MIT
//! Lid switch, power button, display dimming (items 16.7, 16.8)
//!
//! Handles ACPI power events: lid open/close, power button press,
//! display auto-dimming, and sleep timers.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

/// Power event types from ACPI
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerEvent {
    /// Power button pressed briefly
    PowerButtonPress,
    /// Power button held (force shutdown)
    PowerButtonHold,
    /// Lid closed
    LidClose,
    /// Lid opened
    LidOpen,
    /// AC adapter connected
    AcConnect,
    /// AC adapter disconnected
    AcDisconnect,
    /// Thermal threshold exceeded
    ThermalAlarm,
}

/// Lid switch state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LidState {
    Open,
    Closed,
    Unknown,
}

/// Power button action configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerButtonAction {
    DoNothing,
    Suspend,
    Hibernate,
    Shutdown,
    ShowDialog,
}

/// Lid close action configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LidCloseAction {
    DoNothing,
    Suspend,
    Hibernate,
    Shutdown,
    LockScreen,
}

/// Display dimming settings
#[derive(Debug, Clone)]
pub struct DimmingConfig {
    /// Seconds of inactivity before dimming (0 = disabled)
    pub dim_after_secs: u32,
    /// Seconds of inactivity before turning off display (0 = disabled)
    pub off_after_secs: u32,
    /// Dimming brightness level (0-100, percent of current)
    pub dim_brightness: u8,
    /// Whether to dim on battery only
    pub battery_only: bool,
}

impl Default for DimmingConfig {
    fn default() -> Self {
        Self {
            dim_after_secs: 120, // 2 minutes
            off_after_secs: 300, // 5 minutes
            dim_brightness: 30,
            battery_only: false,
        }
    }
}

/// Display power state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayPowerState {
    On,
    Dimmed,
    Off,
    /// Display in low-power standby
    Standby,
}

lazy_static::lazy_static! {
    static ref POWER_EVENTS_STATE: Mutex<PowerEventsState> = Mutex::new(PowerEventsState::new());
}

static EVENTS_HANDLED: AtomicU64 = AtomicU64::new(0);
static DISPLAY_STATE: Mutex<DisplayPowerState> = Mutex::new(DisplayPowerState::On);

struct PowerEventsState {
    lid_state: LidState,
    power_button_action: PowerButtonAction,
    lid_close_action: LidCloseAction,
    dimming_config: DimmingConfig,
    last_activity_tick: u64,
    current_brightness: u8, // 0-100
    ac_connected: bool,
    display_power: DisplayPowerState,
}

impl PowerEventsState {
    fn new() -> Self {
        Self {
            lid_state: LidState::Unknown,
            power_button_action: PowerButtonAction::ShowDialog,
            lid_close_action: LidCloseAction::Suspend,
            dimming_config: DimmingConfig::default(),
            last_activity_tick: 0,
            current_brightness: 100,
            ac_connected: true,
            display_power: DisplayPowerState::On,
        }
    }
}

// ── ACPI Fixed Event Registers ───────────────────────────────────────

/// ACPI PM1a Event Register bit: power button
const ACPI_PM1_PWRBTN: u16 = 1 << 8;
/// ACPI PM1a Event Register bit: sleep button
const ACPI_PM1_SLPBTN: u16 = 1 << 9;

/// ACPI General Purpose Event for lid switch (GPE _Lxx)
const GPE_LID_BIT: u8 = 0x20;

/// Handle a power event from ACPI
pub fn handle_power_event(event: PowerEvent) {
    EVENTS_HANDLED.fetch_add(1, Ordering::Relaxed);

    let mut state = POWER_EVENTS_STATE.lock();

    match event {
        PowerEvent::PowerButtonPress => {
            crate::serial_println!("[power_events] power button pressed");
            match state.power_button_action {
                PowerButtonAction::DoNothing => {}
                PowerButtonAction::Suspend => {
                    drop(state);
                    // crate::suspend::suspend_to_ram();
                }
                PowerButtonAction::Hibernate => {
                    drop(state);
                    // crate::suspend::hibernate();
                }
                PowerButtonAction::Shutdown => {
                    drop(state);
                    crate::acpi::shutdown();
                }
                PowerButtonAction::ShowDialog => {
                    // Signal GUI to show power dialog
                    crate::serial_println!("[power_events] showing power dialog");
                }
            }
        }
        PowerEvent::PowerButtonHold => {
            crate::serial_println!("[power_events] power button held — forcing shutdown");
            drop(state);
            crate::acpi::shutdown();
        }
        PowerEvent::LidClose => {
            state.lid_state = LidState::Closed;
            crate::serial_println!("[power_events] lid closed");
            let action = state.lid_close_action;
            drop(state);
            match action {
                LidCloseAction::DoNothing => {}
                LidCloseAction::Suspend => {
                    // crate::suspend::suspend_to_ram();
                }
                LidCloseAction::Hibernate => {
                    // crate::suspend::hibernate();
                }
                LidCloseAction::Shutdown => {
                    crate::acpi::shutdown();
                }
                LidCloseAction::LockScreen => {
                    // crate::gui::lock_screen::lock();
                }
            }
        }
        PowerEvent::LidOpen => {
            state.lid_state = LidState::Open;
            crate::serial_println!("[power_events] lid opened");
            state.display_power = DisplayPowerState::On;
            state.current_brightness = 100;
        }
        PowerEvent::AcConnect => {
            state.ac_connected = true;
            crate::serial_println!("[power_events] AC connected");
        }
        PowerEvent::AcDisconnect => {
            state.ac_connected = false;
            crate::serial_println!("[power_events] AC disconnected");
        }
        PowerEvent::ThermalAlarm => {
            crate::serial_println!("[power_events] THERMAL ALARM — throttling");
            // crate::cpufreq::set_governor(Governor::Powersave);
        }
    }
}

/// Notify the power event system of user activity (keyboard, mouse)
pub fn notify_activity() {
    let mut state = POWER_EVENTS_STATE.lock();
    state.last_activity_tick = crate::clock::get_ticks();

    // If display was dimmed or off, wake it up
    if state.display_power != DisplayPowerState::On {
        state.display_power = DisplayPowerState::On;
        state.current_brightness = 100;
        crate::serial_println!("[power_events] display woke from activity");
    }
}

/// Tick function called periodically to check inactivity timers
pub fn tick(current_time_secs: u64) {
    let mut state = POWER_EVENTS_STATE.lock();

    let idle_secs = current_time_secs.saturating_sub(state.last_activity_tick);

    // Copy config values to avoid borrow conflict
    let battery_only = state.dimming_config.battery_only;
    let off_after_secs = state.dimming_config.off_after_secs;
    let dim_after_secs = state.dimming_config.dim_after_secs;
    let dim_brightness = state.dimming_config.dim_brightness;

    // Skip if battery-only dimming and on AC
    if battery_only && state.ac_connected {
        return;
    }

    // Check display off timer
    if off_after_secs > 0 && idle_secs >= off_after_secs as u64 {
        if state.display_power != DisplayPowerState::Off {
            state.display_power = DisplayPowerState::Off;
            state.current_brightness = 0;
            crate::serial_println!("[power_events] display off after {}s idle", idle_secs);
        }
    }
    // Check dim timer
    else if dim_after_secs > 0
        && idle_secs >= dim_after_secs as u64
        && state.display_power != DisplayPowerState::Dimmed
    {
        state.display_power = DisplayPowerState::Dimmed;
        state.current_brightness = dim_brightness;
        crate::serial_println!(
            "[power_events] display dimmed to {}% after {}s idle",
            dim_brightness,
            idle_secs
        );
    }
}

/// Set the power button action
pub fn set_power_button_action(action: PowerButtonAction) {
    POWER_EVENTS_STATE.lock().power_button_action = action;
}

/// Set the lid close action
pub fn set_lid_close_action(action: LidCloseAction) {
    POWER_EVENTS_STATE.lock().lid_close_action = action;
}

/// Set display dimming configuration
pub fn set_dimming_config(config: DimmingConfig) {
    POWER_EVENTS_STATE.lock().dimming_config = config;
}

/// Get current display brightness (0-100)
pub fn display_brightness() -> u8 {
    POWER_EVENTS_STATE.lock().current_brightness
}

/// Set display brightness
pub fn set_brightness(brightness: u8) {
    let mut state = POWER_EVENTS_STATE.lock();
    state.current_brightness = brightness.min(100);
    if brightness > 0 {
        state.display_power = DisplayPowerState::On;
    }
}

/// Get current lid state
pub fn lid_state() -> LidState {
    POWER_EVENTS_STATE.lock().lid_state
}

/// Get current display power state
pub fn display_power_state() -> DisplayPowerState {
    POWER_EVENTS_STATE.lock().display_power
}

/// Poll ACPI for power events (called from interrupt or timer)
pub fn poll_acpi_events() {
    // Read ACPI PM1a status register
    // In a real implementation:
    // let pm1a_sts = inw(FADT.pm1a_evt_blk);
    // if pm1a_sts & ACPI_PM1_PWRBTN != 0 {
    //     handle_power_event(PowerEvent::PowerButtonPress);
    //     outw(FADT.pm1a_evt_blk, ACPI_PM1_PWRBTN); // clear bit
    // }
}

pub fn stats() -> u64 {
    EVENTS_HANDLED.load(Ordering::Relaxed)
}

/// Initialize the power events subsystem
pub fn init() {
    let mut state = POWER_EVENTS_STATE.lock();
    state.last_activity_tick = crate::clock::get_ticks();
    crate::serial_println!(
        "[power_events] initialized, dim_after={}s, off_after={}s",
        state.dimming_config.dim_after_secs,
        state.dimming_config.off_after_secs
    );
}
