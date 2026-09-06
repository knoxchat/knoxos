/// Clock / Alarm / Timer Application
///
/// Provides world clock, alarm, stopwatch, and countdown timer functionality
/// integrated with the kernel's timekeeping subsystem.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// TIME REPRESENTATION
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimeOfDay {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl TimeOfDay {
    pub fn new(hour: u8, minute: u8, second: u8) -> Self {
        Self {
            hour: hour % 24,
            minute: minute % 60,
            second: second % 60,
        }
    }

    pub fn total_seconds(&self) -> u32 {
        self.hour as u32 * 3600 + self.minute as u32 * 60 + self.second as u32
    }

    pub fn from_total_seconds(secs: u32) -> Self {
        let s = secs % 86400;
        Self {
            hour: (s / 3600) as u8,
            minute: ((s % 3600) / 60) as u8,
            second: (s % 60) as u8,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// TIMEZONE / WORLD CLOCK
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct TimezoneEntry {
    pub name: String,
    pub city: String,
    /// Offset from UTC in minutes (supports half-hour timezones)
    pub utc_offset_minutes: i16,
}

lazy_static::lazy_static! {
    static ref WORLD_CLOCKS: Mutex<Vec<TimezoneEntry>> = Mutex::new(Vec::new());
}

/// Add a world clock entry
pub fn add_world_clock(name: &str, city: &str, utc_offset_minutes: i16) {
    WORLD_CLOCKS.lock().push(TimezoneEntry {
        name: String::from(name),
        city: String::from(city),
        utc_offset_minutes,
    });
}

/// Get the local time in a given timezone offset
pub fn time_in_timezone(utc: TimeOfDay, offset_minutes: i16) -> TimeOfDay {
    let total = utc.total_seconds() as i32 + offset_minutes as i32 * 60;
    let normalized = ((total % 86400) + 86400) % 86400;
    TimeOfDay::from_total_seconds(normalized as u32)
}

/// List all configured world clocks with their current times
pub fn list_world_clocks(utc_now: TimeOfDay) -> Vec<(String, String, TimeOfDay)> {
    WORLD_CLOCKS
        .lock()
        .iter()
        .map(|tz| {
            let local = time_in_timezone(utc_now, tz.utc_offset_minutes);
            (tz.name.clone(), tz.city.clone(), local)
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// ALARMS
// ═══════════════════════════════════════════════════════════════════════

static NEXT_ALARM_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlarmRepeat {
    Once,
    Daily,
    Weekdays,
    Weekends,
    Custom(u8), // bitmask: bit 0 = Monday .. bit 6 = Sunday
}

#[derive(Debug, Clone)]
pub struct Alarm {
    pub id: u64,
    pub time: TimeOfDay,
    pub label: String,
    pub enabled: bool,
    pub repeat: AlarmRepeat,
    pub snooze_minutes: u8,
    pub snoozed_until: Option<TimeOfDay>,
}

impl Alarm {
    pub fn new(time: TimeOfDay, label: &str, repeat: AlarmRepeat) -> Self {
        Self {
            id: NEXT_ALARM_ID.fetch_add(1, Ordering::Relaxed),
            time,
            label: String::from(label),
            enabled: true,
            repeat,
            snooze_minutes: 9,
            snoozed_until: None,
        }
    }

    /// Check if this alarm should fire at the given time
    pub fn should_fire(&self, now: TimeOfDay, day_of_week: u8) -> bool {
        if !self.enabled {
            return false;
        }

        // Check snooze
        if let Some(snooze_time) = self.snoozed_until {
            return now.total_seconds() == snooze_time.total_seconds();
        }

        if now.total_seconds() != self.time.total_seconds() {
            return false;
        }

        match self.repeat {
            AlarmRepeat::Once | AlarmRepeat::Daily => true,
            AlarmRepeat::Weekdays => day_of_week < 5,
            AlarmRepeat::Weekends => day_of_week >= 5,
            AlarmRepeat::Custom(mask) => (mask >> day_of_week) & 1 != 0,
        }
    }

    /// Snooze this alarm
    pub fn snooze(&mut self) {
        let snooze_secs = self.time.total_seconds() + self.snooze_minutes as u32 * 60;
        self.snoozed_until = Some(TimeOfDay::from_total_seconds(snooze_secs));
    }

    /// Dismiss the alarm
    pub fn dismiss(&mut self) {
        self.snoozed_until = None;
        if self.repeat == AlarmRepeat::Once {
            self.enabled = false;
        }
    }
}

lazy_static::lazy_static! {
    static ref ALARMS: Mutex<Vec<Alarm>> = Mutex::new(Vec::new());
}

/// Create a new alarm
pub fn create_alarm(time: TimeOfDay, label: &str, repeat: AlarmRepeat) -> u64 {
    let alarm = Alarm::new(time, label, repeat);
    let id = alarm.id;
    serial_println!(
        "[clock] Alarm {} created: {:02}:{:02} - {}",
        id,
        time.hour,
        time.minute,
        label
    );
    ALARMS.lock().push(alarm);
    id
}

/// Delete an alarm
pub fn delete_alarm(id: u64) -> bool {
    let mut alarms = ALARMS.lock();
    if let Some(pos) = alarms.iter().position(|a| a.id == id) {
        alarms.remove(pos);
        true
    } else {
        false
    }
}

/// Toggle an alarm on/off
pub fn toggle_alarm(id: u64) -> Option<bool> {
    let mut alarms = ALARMS.lock();
    if let Some(alarm) = alarms.iter_mut().find(|a| a.id == id) {
        alarm.enabled = !alarm.enabled;
        Some(alarm.enabled)
    } else {
        None
    }
}

/// Check all alarms, returns list of firing alarm IDs
pub fn check_alarms(now: TimeOfDay, day_of_week: u8) -> Vec<u64> {
    let alarms = ALARMS.lock();
    alarms
        .iter()
        .filter(|a| a.should_fire(now, day_of_week))
        .map(|a| a.id)
        .collect()
}

/// Get all alarms
pub fn list_alarms() -> Vec<(u64, TimeOfDay, String, bool)> {
    ALARMS
        .lock()
        .iter()
        .map(|a| (a.id, a.time, a.label.clone(), a.enabled))
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// STOPWATCH
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopwatchState {
    Stopped,
    Running,
    Paused,
}

pub struct Stopwatch {
    state: StopwatchState,
    start_ticks: u64,
    elapsed_ms: u64,
    pause_elapsed: u64,
    laps: Vec<u64>,
}

lazy_static::lazy_static! {
    static ref STOPWATCH: Mutex<Stopwatch> = Mutex::new(Stopwatch {
        state: StopwatchState::Stopped,
        start_ticks: 0,
        elapsed_ms: 0,
        pause_elapsed: 0,
        laps: Vec::new(),
    });
}

static SYSTEM_TICKS_MS: AtomicU64 = AtomicU64::new(0);

/// Update system tick counter (called from timer interrupt)
pub fn tick(ms: u64) {
    SYSTEM_TICKS_MS.store(ms, Ordering::Relaxed);
}

fn now_ms() -> u64 {
    SYSTEM_TICKS_MS.load(Ordering::Relaxed)
}

/// Start the stopwatch
pub fn stopwatch_start() {
    let mut sw = STOPWATCH.lock();
    match sw.state {
        StopwatchState::Stopped => {
            sw.start_ticks = now_ms();
            sw.elapsed_ms = 0;
            sw.pause_elapsed = 0;
            sw.laps.clear();
            sw.state = StopwatchState::Running;
        }
        StopwatchState::Paused => {
            sw.start_ticks = now_ms();
            sw.state = StopwatchState::Running;
        }
        _ => {}
    }
}

/// Pause the stopwatch
pub fn stopwatch_pause() {
    let mut sw = STOPWATCH.lock();
    if sw.state == StopwatchState::Running {
        sw.pause_elapsed += now_ms() - sw.start_ticks;
        sw.state = StopwatchState::Paused;
    }
}

/// Reset the stopwatch
pub fn stopwatch_reset() {
    let mut sw = STOPWATCH.lock();
    sw.state = StopwatchState::Stopped;
    sw.elapsed_ms = 0;
    sw.pause_elapsed = 0;
    sw.laps.clear();
}

/// Record a lap
pub fn stopwatch_lap() {
    let mut sw = STOPWATCH.lock();
    if sw.state == StopwatchState::Running {
        let elapsed = sw.pause_elapsed + (now_ms() - sw.start_ticks);
        sw.laps.push(elapsed);
    }
}

/// Get current stopwatch elapsed time in ms
pub fn stopwatch_elapsed() -> u64 {
    let sw = STOPWATCH.lock();
    match sw.state {
        StopwatchState::Stopped => 0,
        StopwatchState::Paused => sw.pause_elapsed,
        StopwatchState::Running => sw.pause_elapsed + (now_ms() - sw.start_ticks),
    }
}

/// Get lap times
pub fn stopwatch_laps() -> Vec<u64> {
    STOPWATCH.lock().laps.clone()
}

// ═══════════════════════════════════════════════════════════════════════
// COUNTDOWN TIMER
// ═══════════════════════════════════════════════════════════════════════

static NEXT_TIMER_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct CountdownTimer {
    pub id: u64,
    pub label: String,
    pub duration_ms: u64,
    pub remaining_ms: u64,
    pub running: bool,
    pub last_tick: u64,
}

lazy_static::lazy_static! {
    static ref TIMERS: Mutex<Vec<CountdownTimer>> = Mutex::new(Vec::new());
}

/// Create a new countdown timer (duration in seconds)
pub fn timer_create(label: &str, duration_secs: u64) -> u64 {
    let id = NEXT_TIMER_ID.fetch_add(1, Ordering::Relaxed);
    let dur_ms = duration_secs * 1000;
    TIMERS.lock().push(CountdownTimer {
        id,
        label: String::from(label),
        duration_ms: dur_ms,
        remaining_ms: dur_ms,
        running: false,
        last_tick: 0,
    });
    serial_println!(
        "[clock] Timer {} created: {}s - {}",
        id,
        duration_secs,
        label
    );
    id
}

/// Start a countdown timer
pub fn timer_start(id: u64) -> bool {
    let mut timers = TIMERS.lock();
    if let Some(t) = timers.iter_mut().find(|t| t.id == id) {
        t.running = true;
        t.last_tick = now_ms();
        true
    } else {
        false
    }
}

/// Pause a countdown timer
pub fn timer_pause(id: u64) -> bool {
    let mut timers = TIMERS.lock();
    if let Some(t) = timers.iter_mut().find(|t| t.id == id) {
        if t.running {
            let elapsed = now_ms() - t.last_tick;
            t.remaining_ms = t.remaining_ms.saturating_sub(elapsed);
            t.running = false;
        }
        true
    } else {
        false
    }
}

/// Reset a timer to its original duration
pub fn timer_reset(id: u64) -> bool {
    let mut timers = TIMERS.lock();
    if let Some(t) = timers.iter_mut().find(|t| t.id == id) {
        t.remaining_ms = t.duration_ms;
        t.running = false;
        true
    } else {
        false
    }
}

/// Update all running timers, returns IDs of expired timers
pub fn timers_tick() -> Vec<u64> {
    let mut timers = TIMERS.lock();
    let current = now_ms();
    let mut expired = Vec::new();

    for t in timers.iter_mut() {
        if t.running {
            let elapsed = current - t.last_tick;
            t.last_tick = current;
            if elapsed >= t.remaining_ms {
                t.remaining_ms = 0;
                t.running = false;
                expired.push(t.id);
            } else {
                t.remaining_ms -= elapsed;
            }
        }
    }

    expired
}

/// Delete a timer
pub fn timer_delete(id: u64) -> bool {
    let mut timers = TIMERS.lock();
    if let Some(pos) = timers.iter().position(|t| t.id == id) {
        timers.remove(pos);
        true
    } else {
        false
    }
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Initialize the clock application
pub fn init() {
    // Add default world clocks
    add_world_clock("UTC", "London", 0);
    add_world_clock("EST", "New York", -300);
    add_world_clock("PST", "Los Angeles", -480);
    add_world_clock("CET", "Berlin", 60);
    add_world_clock("JST", "Tokyo", 540);
    add_world_clock("IST", "Mumbai", 330);
    add_world_clock("AEST", "Sydney", 600);

    serial_println!("[clock] Clock / Alarm / Timer application initialized");
}
