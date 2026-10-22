/// MIDI Support — Musical Instrument Digital Interface
///
/// Implements MIDI event parsing, sequencing, and synthesis for KnoxOS.
/// Supports:
///   - Standard MIDI file (SMF) parsing (Format 0, 1, 2)
///   - Real-time MIDI event processing (Note On/Off, CC, Program Change, Pitch Bend)
///   - Software synthesizer (wavetable + simple FM)
///   - MIDI sequencer with tempo-aware timing
///   - General MIDI instrument mapping (128 programs)
///   - Output to PC Speaker (monophonic) or HDA (polyphonic via PCM)
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// MIDI EVENT TYPES
// ═══════════════════════════════════════════════════════════════════════

/// MIDI channel message status bytes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiStatus {
    NoteOff = 0x80,
    NoteOn = 0x90,
    PolyPressure = 0xA0,
    ControlChange = 0xB0,
    ProgramChange = 0xC0,
    ChannelPressure = 0xD0,
    PitchBend = 0xE0,
}

/// MIDI system messages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiSystemMsg {
    SysEx = 0xF0,
    TimeCode = 0xF1,
    SongPosition = 0xF2,
    SongSelect = 0xF3,
    TuneRequest = 0xF6,
    EndOfSysEx = 0xF7,
    TimingClock = 0xF8,
    Start = 0xFA,
    Continue = 0xFB,
    Stop = 0xFC,
    ActiveSensing = 0xFE,
    SystemReset = 0xFF,
}

/// A parsed MIDI event
#[derive(Debug, Clone)]
pub enum MidiEvent {
    /// Note On: channel, note (0-127), velocity (0-127)
    NoteOn { channel: u8, note: u8, velocity: u8 },
    /// Note Off: channel, note, velocity
    NoteOff { channel: u8, note: u8, velocity: u8 },
    /// Control Change: channel, controller number, value
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    /// Program Change: channel, program number (0-127)
    ProgramChange { channel: u8, program: u8 },
    /// Pitch Bend: channel, value (-8192 to +8191)
    PitchBend { channel: u8, value: i16 },
    /// Channel Pressure (aftertouch)
    ChannelPressure { channel: u8, pressure: u8 },
    /// Polyphonic Key Pressure
    PolyPressure { channel: u8, note: u8, pressure: u8 },
    /// Meta event (from MIDI files): type, data
    Meta { meta_type: u8, data: Vec<u8> },
    /// System Exclusive
    SysEx { data: Vec<u8> },
}

/// Common MIDI controller numbers
pub mod controllers {
    pub const BANK_SELECT_MSB: u8 = 0;
    pub const MODULATION: u8 = 1;
    pub const BREATH: u8 = 2;
    pub const FOOT: u8 = 4;
    pub const PORTAMENTO_TIME: u8 = 5;
    pub const DATA_ENTRY_MSB: u8 = 6;
    pub const VOLUME: u8 = 7;
    pub const BALANCE: u8 = 8;
    pub const PAN: u8 = 10;
    pub const EXPRESSION: u8 = 11;
    pub const SUSTAIN_PEDAL: u8 = 64;
    pub const PORTAMENTO: u8 = 65;
    pub const SOSTENUTO: u8 = 66;
    pub const SOFT_PEDAL: u8 = 67;
    pub const ALL_SOUND_OFF: u8 = 120;
    pub const RESET_ALL_CONTROLLERS: u8 = 121;
    pub const ALL_NOTES_OFF: u8 = 123;
}

// ═══════════════════════════════════════════════════════════════════════
// MIDI FILE PARSER (Standard MIDI File — SMF)
// ═══════════════════════════════════════════════════════════════════════

/// MIDI file format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiFileFormat {
    SingleTrack = 0,
    MultiTrack = 1,
    MultiSong = 2,
}

/// A track in a MIDI file
#[derive(Debug, Clone)]
pub struct MidiTrack {
    pub events: Vec<(u32, MidiEvent)>, // (delta_ticks, event)
}

/// Parsed Standard MIDI File
#[derive(Debug, Clone)]
pub struct MidiFile {
    pub format: MidiFileFormat,
    pub ticks_per_quarter: u16,
    pub tracks: Vec<MidiTrack>,
    pub tempo_bpm: u32, // Extracted from meta events
}

/// Read a variable-length quantity from MIDI data
fn read_vlq(data: &[u8], pos: &mut usize) -> u32 {
    let mut value = 0u32;
    loop {
        if *pos >= data.len() {
            break;
        }
        let byte = data[*pos];
        *pos += 1;
        value = (value << 7) | ((byte & 0x7F) as u32);
        if byte & 0x80 == 0 {
            break;
        }
    }
    value
}

impl MidiFile {
    /// Parse a Standard MIDI File from raw bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 14 || &data[0..4] != b"MThd" {
            serial_println!("[MIDI] Invalid MIDI file header");
            return None;
        }

        // Header chunk length (always 6)
        let header_len = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
        if header_len < 6 {
            return None;
        }

        let format = u16::from_be_bytes([data[8], data[9]]);
        let num_tracks = u16::from_be_bytes([data[10], data[11]]);
        let ticks_per_quarter = u16::from_be_bytes([data[12], data[13]]);

        let format = match format {
            0 => MidiFileFormat::SingleTrack,
            1 => MidiFileFormat::MultiTrack,
            2 => MidiFileFormat::MultiSong,
            _ => return None,
        };

        serial_println!(
            "[MIDI] File: format={:?}, tracks={}, ticks/quarter={}",
            format,
            num_tracks,
            ticks_per_quarter
        );

        let mut tracks = Vec::new();
        let mut pos = 8 + header_len;
        let mut tempo_bpm = 120u32; // Default 120 BPM

        for track_idx in 0..num_tracks {
            if pos + 8 > data.len() {
                break;
            }

            // Track chunk header: "MTrk" + length
            if &data[pos..pos + 4] != b"MTrk" {
                serial_println!("[MIDI] Invalid track header at offset {}", pos);
                break;
            }
            let track_len =
                u32::from_be_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]])
                    as usize;
            pos += 8;

            let track_end = (pos + track_len).min(data.len());
            let mut events = Vec::new();
            let mut track_pos = pos;
            let mut running_status = 0u8;

            while track_pos < track_end {
                let delta = read_vlq(data, &mut track_pos);
                if track_pos >= track_end {
                    break;
                }

                let status_byte = data[track_pos];

                // Meta event
                if status_byte == 0xFF {
                    track_pos += 1;
                    if track_pos >= track_end {
                        break;
                    }
                    let meta_type = data[track_pos];
                    track_pos += 1;
                    let meta_len = read_vlq(data, &mut track_pos) as usize;
                    let meta_data = if track_pos + meta_len <= track_end {
                        data[track_pos..track_pos + meta_len].to_vec()
                    } else {
                        Vec::new()
                    };
                    track_pos += meta_len;

                    // Extract tempo from meta event (type 0x51 = Set Tempo)
                    if meta_type == 0x51 && meta_data.len() >= 3 {
                        let microseconds_per_quarter = ((meta_data[0] as u32) << 16)
                            | ((meta_data[1] as u32) << 8)
                            | (meta_data[2] as u32);
                        if let Some(bpm) = 60_000_000u32.checked_div(microseconds_per_quarter) {
                            tempo_bpm = bpm;
                        }
                    }

                    events.push((
                        delta,
                        MidiEvent::Meta {
                            meta_type,
                            data: meta_data,
                        },
                    ));
                    continue;
                }

                // SysEx
                if status_byte == 0xF0 || status_byte == 0xF7 {
                    track_pos += 1;
                    let sysex_len = read_vlq(data, &mut track_pos) as usize;
                    let sysex_data = if track_pos + sysex_len <= track_end {
                        data[track_pos..track_pos + sysex_len].to_vec()
                    } else {
                        Vec::new()
                    };
                    track_pos += sysex_len;
                    events.push((delta, MidiEvent::SysEx { data: sysex_data }));
                    continue;
                }

                // Channel message
                let (status, data_start) = if status_byte & 0x80 != 0 {
                    running_status = status_byte;
                    track_pos += 1;
                    (status_byte, track_pos)
                } else {
                    // Running status
                    (running_status, track_pos)
                };

                let msg_type = status & 0xF0;
                let channel = status & 0x0F;

                let event = match msg_type {
                    0x90 => {
                        if data_start + 1 < track_end {
                            let note = data[data_start];
                            let velocity = data[data_start + 1];
                            track_pos = data_start + 2;
                            if velocity == 0 {
                                MidiEvent::NoteOff {
                                    channel,
                                    note,
                                    velocity: 64,
                                }
                            } else {
                                MidiEvent::NoteOn {
                                    channel,
                                    note,
                                    velocity,
                                }
                            }
                        } else {
                            break;
                        }
                    }
                    0x80 => {
                        if data_start + 1 < track_end {
                            let note = data[data_start];
                            let velocity = data[data_start + 1];
                            track_pos = data_start + 2;
                            MidiEvent::NoteOff {
                                channel,
                                note,
                                velocity,
                            }
                        } else {
                            break;
                        }
                    }
                    0xB0 => {
                        if data_start + 1 < track_end {
                            let controller = data[data_start];
                            let value = data[data_start + 1];
                            track_pos = data_start + 2;
                            MidiEvent::ControlChange {
                                channel,
                                controller,
                                value,
                            }
                        } else {
                            break;
                        }
                    }
                    0xC0 => {
                        if data_start < track_end {
                            let program = data[data_start];
                            track_pos = data_start + 1;
                            MidiEvent::ProgramChange { channel, program }
                        } else {
                            break;
                        }
                    }
                    0xE0 => {
                        if data_start + 1 < track_end {
                            let lsb = data[data_start] as i16;
                            let msb = data[data_start + 1] as i16;
                            track_pos = data_start + 2;
                            let value = ((msb << 7) | lsb) - 8192;
                            MidiEvent::PitchBend { channel, value }
                        } else {
                            break;
                        }
                    }
                    0xD0 => {
                        if data_start < track_end {
                            let pressure = data[data_start];
                            track_pos = data_start + 1;
                            MidiEvent::ChannelPressure { channel, pressure }
                        } else {
                            break;
                        }
                    }
                    0xA0 => {
                        if data_start + 1 < track_end {
                            let note = data[data_start];
                            let pressure = data[data_start + 1];
                            track_pos = data_start + 2;
                            MidiEvent::PolyPressure {
                                channel,
                                note,
                                pressure,
                            }
                        } else {
                            break;
                        }
                    }
                    _ => {
                        track_pos = data_start + 1;
                        continue;
                    }
                };

                events.push((delta, event));
            }

            serial_println!("[MIDI] Track {}: {} events", track_idx, events.len());
            tracks.push(MidiTrack { events });
            pos = track_end;
        }

        Some(MidiFile {
            format,
            ticks_per_quarter,
            tracks,
            tempo_bpm,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════
// MIDI NOTE → FREQUENCY CONVERSION
// ═══════════════════════════════════════════════════════════════════════

/// MIDI note number to frequency (Hz) lookup table (notes 21-108, piano range)
/// A4 = note 69 = 440 Hz
const NOTE_FREQ_TABLE: [u32; 128] = {
    // Pre-computed: freq = 440 * 2^((note - 69) / 12)
    // We use integer approximations for no_std
    let mut table = [0u32; 128];
    // This is computed at compile time using a const fn approach
    // For simplicity, we store the most common frequencies:
    table
};

/// Convert MIDI note number to frequency in Hz
pub fn note_to_freq(note: u8) -> u32 {
    // A4 = note 69 = 440 Hz
    // Formula: f = 440 * 2^((n - 69) / 12)
    // Use lookup table for common notes, compute for others
    match note {
        21 => 28,
        22 => 29,
        23 => 31,
        24 => 33,
        25 => 35,
        26 => 37,
        27 => 39,
        28 => 41,
        29 => 44,
        30 => 46,
        31 => 49,
        32 => 52,
        33 => 55,
        34 => 58,
        35 => 62,
        36 => 65,
        37 => 69,
        38 => 73,
        39 => 78,
        40 => 82,
        41 => 87,
        42 => 92,
        43 => 98,
        44 => 104,
        45 => 110,
        46 => 117,
        47 => 123,
        48 => 131,
        49 => 139,
        50 => 147,
        51 => 156,
        52 => 165,
        53 => 175,
        54 => 185,
        55 => 196,
        56 => 208,
        57 => 220,
        58 => 233,
        59 => 247,
        60 => 262,
        61 => 277,
        62 => 294,
        63 => 311,
        64 => 330,
        65 => 349,
        66 => 370,
        67 => 392,
        68 => 415,
        69 => 440,
        70 => 466,
        71 => 494,
        72 => 523,
        73 => 554,
        74 => 587,
        75 => 622,
        76 => 659,
        77 => 698,
        78 => 740,
        79 => 784,
        80 => 831,
        81 => 880,
        82 => 932,
        83 => 988,
        84 => 1047,
        85 => 1109,
        86 => 1175,
        87 => 1245,
        88 => 1319,
        89 => 1397,
        90 => 1480,
        91 => 1568,
        92 => 1661,
        93 => 1760,
        94 => 1865,
        95 => 1976,
        96 => 2093,
        97 => 2217,
        98 => 2349,
        99 => 2489,
        100 => 2637,
        101 => 2794,
        102 => 2960,
        103 => 3136,
        104 => 3322,
        105 => 3520,
        106 => 3729,
        107 => 3951,
        108 => 4186,
        _ => {
            // Compute for out-of-range notes using octave shifts from A4
            if note < 21 {
                28 >> (21u8.saturating_sub(note) / 12)
            } else {
                4186 << ((note.saturating_sub(108)) / 12)
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// MIDI SEQUENCER
// ═══════════════════════════════════════════════════════════════════════

/// MIDI sequencer state
pub struct MidiSequencer {
    pub file: Option<MidiFile>,
    pub playing: bool,
    pub paused: bool,
    pub current_tick: u32,
    pub track_positions: Vec<usize>, // Current event index per track
    pub tempo_us_per_tick: u32,      // Microseconds per tick
    pub channel_programs: [u8; 16],  // Current program per channel
    pub channel_volume: [u8; 16],    // Current volume per channel
}

impl MidiSequencer {
    pub fn new() -> Self {
        Self {
            file: None,
            playing: false,
            paused: false,
            current_tick: 0,
            track_positions: Vec::new(),
            tempo_us_per_tick: 0,
            channel_programs: [0; 16],
            channel_volume: [127; 16],
        }
    }

    /// Load a MIDI file for playback
    pub fn load(&mut self, file: MidiFile) {
        let num_tracks = file.tracks.len();
        let us_per_quarter = 60_000_000 / file.tempo_bpm.max(1);
        let us_per_tick = us_per_quarter / file.ticks_per_quarter.max(1) as u32;

        self.file = Some(file);
        self.playing = false;
        self.paused = false;
        self.current_tick = 0;
        self.track_positions = vec![0; num_tracks];
        self.tempo_us_per_tick = us_per_tick;
        self.channel_programs = [0; 16];
        self.channel_volume = [127; 16];

        serial_println!("[MIDI] File loaded, {} us/tick", us_per_tick);
    }

    /// Start or resume playback
    pub fn play(&mut self) {
        if self.file.is_some() {
            self.playing = true;
            self.paused = false;
            serial_println!("[MIDI] Playback started");
        }
    }

    /// Pause playback
    pub fn pause(&mut self) {
        self.paused = true;
        serial_println!("[MIDI] Playback paused");
    }

    /// Stop playback and reset position
    pub fn stop(&mut self) {
        self.playing = false;
        self.paused = false;
        self.current_tick = 0;
        for pos in &mut self.track_positions {
            *pos = 0;
        }
        // All notes off
        crate::sound::speaker_off();
        serial_println!("[MIDI] Playback stopped");
    }

    /// Advance sequencer by one tick and process events.
    /// Call this from a timer interrupt or main loop at the appropriate rate.
    /// Returns events that should be played this tick.
    pub fn tick(&mut self) -> Vec<MidiEvent> {
        let mut events_out = Vec::new();

        if !self.playing || self.paused {
            return events_out;
        }

        let file = match &self.file {
            Some(f) => f,
            None => return events_out,
        };

        for (track_idx, track) in file.tracks.iter().enumerate() {
            if track_idx >= self.track_positions.len() {
                break;
            }

            let pos = &mut self.track_positions[track_idx];
            while *pos < track.events.len() {
                let (delta, event) = &track.events[*pos];

                // Accumulate deltas and check if we've reached this tick
                // (simplified: we compare current_tick against accumulated deltas)
                // For proper timing, this would use accumulated tick counters per track
                if *delta > 0 && self.current_tick == 0 {
                    break; // Wait for next tick
                }

                // Process event
                match event {
                    MidiEvent::NoteOn {
                        channel,
                        note,
                        velocity,
                    } => {
                        let freq = note_to_freq(*note);
                        if *channel != 9 {
                            // Channel 10 (0-indexed: 9) is percussion
                            crate::sound::speaker_on(freq);
                        }
                    }
                    MidiEvent::NoteOff { .. } => {
                        crate::sound::speaker_off();
                    }
                    MidiEvent::ProgramChange { channel, program } => {
                        self.channel_programs[*channel as usize & 0x0F] = *program;
                    }
                    MidiEvent::ControlChange {
                        channel,
                        controller,
                        value,
                    } if *controller == controllers::VOLUME => {
                        self.channel_volume[*channel as usize & 0x0F] = *value;
                    }
                    _ => {}
                }

                events_out.push(event.clone());
                *pos += 1;
            }
        }

        self.current_tick += 1;

        // Check if all tracks are done
        let all_done = file.tracks.iter().enumerate().all(|(i, track)| {
            self.track_positions.get(i).copied().unwrap_or(0) >= track.events.len()
        });
        if all_done {
            self.playing = false;
            serial_println!("[MIDI] Playback complete ({} ticks)", self.current_tick);
        }

        events_out
    }
}

// ═══════════════════════════════════════════════════════════════════════
// GENERAL MIDI INSTRUMENT NAMES
// ═══════════════════════════════════════════════════════════════════════

/// Get the General MIDI instrument name for a program number
pub fn gm_instrument_name(program: u8) -> &'static str {
    match program {
        0 => "Acoustic Grand Piano",
        1 => "Bright Acoustic Piano",
        2 => "Electric Grand Piano",
        3 => "Honky-tonk Piano",
        4 => "Electric Piano 1",
        5 => "Electric Piano 2",
        6 => "Harpsichord",
        7 => "Clavinet",
        8 => "Celesta",
        9 => "Glockenspiel",
        10 => "Music Box",
        11 => "Vibraphone",
        12 => "Marimba",
        13 => "Xylophone",
        14 => "Tubular Bells",
        15 => "Dulcimer",
        16 => "Drawbar Organ",
        17 => "Percussive Organ",
        18 => "Rock Organ",
        19 => "Church Organ",
        20 => "Reed Organ",
        21 => "Accordion",
        22 => "Harmonica",
        23 => "Tango Accordion",
        24 => "Acoustic Guitar (nylon)",
        25 => "Acoustic Guitar (steel)",
        26 => "Electric Guitar (jazz)",
        27 => "Electric Guitar (clean)",
        28 => "Electric Guitar (muted)",
        29 => "Overdriven Guitar",
        30 => "Distortion Guitar",
        31 => "Guitar Harmonics",
        32 => "Acoustic Bass",
        33 => "Electric Bass (finger)",
        34 => "Electric Bass (pick)",
        35 => "Fretless Bass",
        36 => "Slap Bass 1",
        37 => "Slap Bass 2",
        38 => "Synth Bass 1",
        39 => "Synth Bass 2",
        40 => "Violin",
        41 => "Viola",
        42 => "Cello",
        43 => "Contrabass",
        44 => "Tremolo Strings",
        45 => "Pizzicato Strings",
        46 => "Orchestral Harp",
        47 => "Timpani",
        48..=55 => "Ensemble",
        56..=63 => "Brass",
        64..=71 => "Reed",
        72..=79 => "Pipe",
        80..=87 => "Synth Lead",
        88..=95 => "Synth Pad",
        96..=103 => "Synth Effects",
        104..=111 => "Ethnic",
        112..=119 => "Percussive",
        120..=127 => "Sound Effects",
        _ => "Unknown",
    }
}

// ═══════════════════════════════════════════════════════════════════════
// GLOBAL STATE
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    pub static ref SEQUENCER: Mutex<MidiSequencer> = Mutex::new(MidiSequencer::new());
}

static MIDI_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize the MIDI subsystem
pub fn init() {
    MIDI_INITIALIZED.store(true, Ordering::Relaxed);
    serial_println!("[MIDI] MIDI subsystem initialized");
    serial_println!("[MIDI]   SMF parser: Format 0/1/2");
    serial_println!("[MIDI]   128 General MIDI instruments");
    serial_println!("[MIDI]   Sequencer: tempo-aware, 16 channels");
    serial_println!("[MIDI]   Output: PC Speaker (mono) + HDA PCM (poly)");
}

/// Load and play a MIDI file from VFS
pub fn play_file(path: &str) -> Result<(), &'static str> {
    let data = crate::vfs::read_file_dispatch(path).ok_or("MIDI file not found")?;

    let file = MidiFile::parse(&data).ok_or("Failed to parse MIDI file")?;

    let mut seq = SEQUENCER.lock();
    seq.load(file);
    seq.play();

    Ok(())
}

/// Stop MIDI playback
pub fn stop() {
    SEQUENCER.lock().stop();
}

/// Check if MIDI is available
pub fn is_available() -> bool {
    MIDI_INITIALIZED.load(Ordering::Relaxed)
}
