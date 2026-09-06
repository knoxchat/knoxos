/// PC Speaker / Sound Driver
/// Provides basic sound output through the PC speaker
/// Also defines the sound subsystem interface for future audio drivers
#[cfg(target_arch = "x86_64")]
use crate::arch_compat::instructions::port::Port;
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::instructions::port::Port;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use spin::Mutex;

/// PIT (Programmable Interval Timer) frequency
const PIT_FREQUENCY: u32 = 1_193_182;

/// PIT I/O ports
const PIT_CHANNEL_2: u16 = 0x42;
const PIT_COMMAND: u16 = 0x43;
const SPEAKER_PORT: u16 = 0x61;

/// Sound state
static SOUND_ENABLED: Mutex<bool> = Mutex::new(true);

/// Note frequencies (Hz) - standard musical notes
pub mod notes {
    pub const C3: u32 = 131;
    pub const D3: u32 = 147;
    pub const E3: u32 = 165;
    pub const F3: u32 = 175;
    pub const G3: u32 = 196;
    pub const A3: u32 = 220;
    pub const B3: u32 = 247;
    pub const C4: u32 = 262; // Middle C
    pub const D4: u32 = 294;
    pub const E4: u32 = 330;
    pub const F4: u32 = 349;
    pub const G4: u32 = 392;
    pub const A4: u32 = 440; // Concert A
    pub const B4: u32 = 494;
    pub const C5: u32 = 523;
    pub const D5: u32 = 587;
    pub const E5: u32 = 659;
    pub const F5: u32 = 698;
    pub const G5: u32 = 784;
    pub const A5: u32 = 880;
    pub const B5: u32 = 988;
    pub const C6: u32 = 1047;
}

/// A note to play (frequency in Hz, duration in ms)
#[derive(Debug, Clone, Copy)]
pub struct Note {
    pub frequency: u32,
    pub duration_ms: u32,
}

/// Sound event types
#[derive(Debug, Clone, Copy)]
pub enum SoundEvent {
    Beep,
    Error,
    Warning,
    Notification,
    Startup,
}

/// Audio sample format
#[derive(Debug, Clone, Copy)]
pub enum SampleFormat {
    U8,
    S16Le,
    S16Be,
    F32Le,
}

/// Audio device information (for future drivers)
#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub name: &'static str,
    pub sample_rate: u32,
    pub channels: u8,
    pub format: SampleFormat,
    pub buffer_size: usize,
}

/// Sound playback queue
lazy_static::lazy_static! {
    static ref PLAY_QUEUE: Mutex<VecDeque<Note>> = Mutex::new(VecDeque::new());
}

/// Enable the PC speaker at a given frequency
pub fn speaker_on(frequency: u32) {
    if frequency == 0 || !*SOUND_ENABLED.lock() {
        return;
    }

    let divisor = PIT_FREQUENCY / frequency;

    unsafe {
        // Set PIT channel 2 to square wave mode
        let mut cmd_port = Port::<u8>::new(PIT_COMMAND);
        cmd_port.write(0xB6); // Channel 2, access mode lobyte/hibyte, square wave

        // Set frequency divisor
        let mut ch2_port = Port::<u8>::new(PIT_CHANNEL_2);
        ch2_port.write((divisor & 0xFF) as u8);
        ch2_port.write(((divisor >> 8) & 0xFF) as u8);

        // Enable speaker
        let mut speaker = Port::<u8>::new(SPEAKER_PORT);
        let val = speaker.read();
        if val & 0x03 != 0x03 {
            speaker.write(val | 0x03);
        }
    }
}

/// Turn off the PC speaker
pub fn speaker_off() {
    unsafe {
        let mut speaker = Port::<u8>::new(SPEAKER_PORT);
        let val = speaker.read();
        speaker.write(val & 0xFC);
    }
}

/// Play a beep at a frequency for a duration (blocking)
pub fn beep(frequency: u32, duration_ms: u32) {
    speaker_on(frequency);

    // Simple busy-wait delay (crude but works in kernel)
    // Each iteration ≈ a few ns, need ~duration_ms * 1000 μs
    let iterations = duration_ms as u64 * 10_000; // Approximate
    for _ in 0..iterations {
        core::hint::spin_loop();
    }

    speaker_off();
}

/// Play a sequence of notes
pub fn play_melody(melody: &[Note]) {
    for note in melody {
        if note.frequency == 0 {
            // Rest
            let iterations = note.duration_ms as u64 * 10_000;
            for _ in 0..iterations {
                core::hint::spin_loop();
            }
        } else {
            beep(note.frequency, note.duration_ms);
        }
    }
}

/// Play a system sound event
pub fn play_event(event: SoundEvent) {
    match event {
        SoundEvent::Beep => {
            beep(notes::A4, 100);
        }
        SoundEvent::Error => {
            let melody = [
                Note {
                    frequency: notes::E4,
                    duration_ms: 100,
                },
                Note {
                    frequency: notes::C4,
                    duration_ms: 200,
                },
            ];
            play_melody(&melody);
        }
        SoundEvent::Warning => {
            let melody = [
                Note {
                    frequency: notes::E5,
                    duration_ms: 80,
                },
                Note {
                    frequency: 0,
                    duration_ms: 50,
                },
                Note {
                    frequency: notes::E5,
                    duration_ms: 80,
                },
            ];
            play_melody(&melody);
        }
        SoundEvent::Notification => {
            let melody = [
                Note {
                    frequency: notes::C5,
                    duration_ms: 60,
                },
                Note {
                    frequency: notes::E5,
                    duration_ms: 60,
                },
            ];
            play_melody(&melody);
        }
        SoundEvent::Startup => {
            let melody = [
                Note {
                    frequency: notes::C4,
                    duration_ms: 80,
                },
                Note {
                    frequency: notes::E4,
                    duration_ms: 80,
                },
                Note {
                    frequency: notes::G4,
                    duration_ms: 80,
                },
                Note {
                    frequency: notes::C5,
                    duration_ms: 150,
                },
            ];
            play_melody(&melody);
        }
    }
}

/// Enable or disable sound
pub fn set_enabled(enabled: bool) {
    *SOUND_ENABLED.lock() = enabled;
    if !enabled {
        speaker_off();
    }
}

/// Check if sound is enabled
pub fn is_enabled() -> bool {
    *SOUND_ENABLED.lock()
}

/// Get the default audio device info
pub fn default_device() -> AudioDevice {
    AudioDevice {
        name: "PC Speaker",
        sample_rate: 0, // Not applicable for PC speaker
        channels: 1,
        format: SampleFormat::U8,
        buffer_size: 0,
    }
}

/// ALSA-compatible ioctl constants (for future sound card drivers)
pub mod alsa {
    // Sound card ioctl numbers (subset of Linux ALSA)
    pub const SNDRV_PCM_IOCTL_HW_PARAMS: u32 = 0x4101;
    pub const SNDRV_PCM_IOCTL_SW_PARAMS: u32 = 0x4102;
    pub const SNDRV_PCM_IOCTL_PREPARE: u32 = 0x4140;
    pub const SNDRV_PCM_IOCTL_START: u32 = 0x4142;
    pub const SNDRV_PCM_IOCTL_DROP: u32 = 0x4143;

    /// PCM stream direction
    #[derive(Debug, Clone, Copy)]
    pub enum StreamDirection {
        Playback,
        Capture,
    }

    /// PCM state
    #[derive(Debug, Clone, Copy)]
    pub enum PcmState {
        Open,
        Setup,
        Prepared,
        Running,
        Paused,
        Draining,
        Disconnected,
    }
}

/// Initialize the sound subsystem
pub fn init() {
    // Ensure speaker is off initially
    speaker_off();
    crate::serial_println!("[KnoxOS] Sound subsystem initialized (PC Speaker)");
}
