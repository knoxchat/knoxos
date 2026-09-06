#[cfg(target_arch = "x86_64")]
use crate::arch_compat::instructions::port::Port;
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::instructions::port::Port;
/// System Sounds — Audio feedback for GUI events
///
/// Uses the PC speaker (PIT channel 2) on x86_64 for basic beep tones.
/// On aarch64/riscv64, sound functions are no-ops (no PC speaker hardware).
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

// ─── Sound enable/disable ───────────────────────────────────────────

static SOUNDS_ENABLED: AtomicBool = AtomicBool::new(true);
/// Timestamp of last sound (to avoid overlapping beeps)
static LAST_SOUND_TICK: AtomicU64 = AtomicU64::new(0);

/// Enable or disable system sounds
pub fn set_enabled(enabled: bool) {
    SOUNDS_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Check if sounds are enabled
pub fn is_enabled() -> bool {
    SOUNDS_ENABLED.load(Ordering::Relaxed)
}

// ─── PC Speaker Control ─────────────────────────────────────────────

/// PIT base frequency (1.193182 MHz)
const PIT_FREQ: u32 = 1_193_182;

/// Start a tone on the PC speaker at the given frequency (Hz)
fn speaker_on(freq: u32) {
    if freq == 0 {
        return;
    }
    #[cfg(target_arch = "x86_64")]
    {
        let divisor = PIT_FREQ / freq;
        unsafe {
            // Program PIT channel 2 for square wave
            let mut cmd_port: Port<u8> = Port::new(0x43);
            cmd_port.write(0xB6); // Channel 2, lobyte/hibyte, square wave

            let mut ch2_port: Port<u8> = Port::new(0x42);
            ch2_port.write((divisor & 0xFF) as u8);
            ch2_port.write(((divisor >> 8) & 0xFF) as u8);

            // Enable speaker (bits 0,1 of port 0x61)
            let mut ctrl_port: Port<u8> = Port::new(0x61);
            let prev = ctrl_port.read();
            ctrl_port.write(prev | 0x03);
        }
    }
}

/// Turn off the PC speaker
fn speaker_off() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let mut ctrl_port: Port<u8> = Port::new(0x61);
        let prev = ctrl_port.read();
        ctrl_port.write(prev & !0x03);
    }
}

/// Play a short tone (non-blocking — sets up speaker, caller or timer stops it)
fn play_tone(freq: u32, duration_ticks: u64) {
    if !SOUNDS_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let now = crate::interrupts::get_ticks();
    let last = LAST_SOUND_TICK.load(Ordering::Relaxed);
    // Don't overlap — minimum 2 ticks between sounds
    if now.wrapping_sub(last) < 2 {
        return;
    }
    LAST_SOUND_TICK.store(now, Ordering::Relaxed);
    PENDING_OFF_TICK.store(now + duration_ticks, Ordering::Relaxed);
    speaker_on(freq);
}

/// Tick for pending sound-off (should be called from timer interrupt or redraw loop)
static PENDING_OFF_TICK: AtomicU64 = AtomicU64::new(0);

pub fn tick() {
    let off_tick = PENDING_OFF_TICK.load(Ordering::Relaxed);
    if off_tick == 0 {
        return;
    }
    let now = crate::interrupts::get_ticks();
    if now >= off_tick {
        speaker_off();
        PENDING_OFF_TICK.store(0, Ordering::Relaxed);
    }
}

// ─── Musical Notes (Hz) ─────────────────────────────────────────────

const NOTE_C5: u32 = 523;
const NOTE_D5: u32 = 587;
const NOTE_E5: u32 = 659;
const NOTE_F5: u32 = 698;
const NOTE_G5: u32 = 784;
const NOTE_A5: u32 = 880;
const NOTE_C6: u32 = 1047;
const NOTE_E6: u32 = 1319;
const NOTE_G6: u32 = 1568;

// ─── Sound Events ───────────────────────────────────────────────────

/// Soft click sound — short high pip
pub fn click() {
    play_tone(NOTE_E6, 1); // ~55ms
}

/// Button press / confirm
pub fn confirm() {
    play_tone(NOTE_C6, 1);
}

/// Error / alert — lower tone
pub fn error() {
    play_tone(220, 3); // A3, ~165ms
}

/// Window opened — ascending tone
pub fn window_open() {
    play_tone(NOTE_G5, 1);
}

/// Window closed — descending tone
pub fn window_close() {
    play_tone(NOTE_D5, 1);
}

/// Notification received — two-note chime
pub fn notification() {
    play_tone(NOTE_A5, 2);
}

/// Login success — bright ascending
pub fn login() {
    play_tone(NOTE_C6, 2);
}

/// Logout — descending
pub fn logout() {
    play_tone(NOTE_F5, 2);
}

/// Screenshot captured
pub fn screenshot() {
    play_tone(NOTE_G6, 1);
}

/// Warning beep
pub fn warning() {
    play_tone(440, 2); // A4
}

/// Delete / trash
pub fn delete() {
    play_tone(330, 2); // E4
}

/// Generic notification pip
pub fn pip() {
    play_tone(NOTE_E5, 1);
}
