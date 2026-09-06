// SPDX-License-Identifier: MIT
//! Media player application (item 15.6)
//!
//! Provides a simple media player that can play PCM audio via
//! the ALSA compatibility layer and display media controls in the GUI.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

/// Playback state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

/// Supported audio format
#[derive(Debug, Clone, Copy)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u8,
    pub bits_per_sample: u16,
}

impl AudioFormat {
    pub fn cd_quality() -> Self {
        Self {
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: 16,
        }
    }
}

/// A media track
#[derive(Debug, Clone)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    pub format: AudioFormat,
    pub file_path: String,
}

/// Playlist
#[derive(Debug, Clone)]
pub struct Playlist {
    pub name: String,
    pub tracks: Vec<Track>,
    pub current_index: usize,
    pub repeat_mode: RepeatMode,
    pub shuffle: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    Off,
    RepeatOne,
    RepeatAll,
}

/// Media player state
pub struct MediaPlayer {
    pub state: PlaybackState,
    pub playlist: Playlist,
    pub volume: u8, // 0-100
    pub position_ms: u64,
    pub muted: bool,
    pub equalizer: [i8; 10], // 10-band EQ, -12 to +12 dB
}

lazy_static::lazy_static! {
    static ref PLAYER: Mutex<MediaPlayer> = Mutex::new(MediaPlayer::new());
}

static TRACKS_PLAYED: AtomicU64 = AtomicU64::new(0);

impl MediaPlayer {
    pub fn new() -> Self {
        Self {
            state: PlaybackState::Stopped,
            playlist: Playlist {
                name: String::from("Default"),
                tracks: Vec::new(),
                current_index: 0,
                repeat_mode: RepeatMode::Off,
                shuffle: false,
            },
            volume: 80,
            position_ms: 0,
            muted: false,
            equalizer: [0; 10],
        }
    }
}

/// WAV file header parser
#[repr(C, packed)]
struct WavHeader {
    riff_magic: [u8; 4], // "RIFF"
    file_size: u32,
    wave_magic: [u8; 4],   // "WAVE"
    fmt_chunk_id: [u8; 4], // "fmt "
    fmt_chunk_size: u32,
    audio_format: u16, // 1 = PCM
    num_channels: u16,
    sample_rate: u32,
    byte_rate: u32,
    block_align: u16,
    bits_per_sample: u16,
}

/// Parse a WAV file and extract audio data
pub fn parse_wav(data: &[u8]) -> Result<(AudioFormat, &[u8]), &'static str> {
    if data.len() < 44 {
        return Err("file too small for WAV header");
    }

    // Check RIFF header
    if &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err("not a valid WAV file");
    }

    // Parse format chunk
    let audio_format = u16::from_le_bytes([data[20], data[21]]);
    if audio_format != 1 {
        return Err("only PCM WAV supported");
    }

    let channels = u16::from_le_bytes([data[22], data[23]]);
    let sample_rate = u32::from_le_bytes([data[24], data[25], data[26], data[27]]);
    let bits_per_sample = u16::from_le_bytes([data[34], data[35]]);

    // Find data chunk
    let mut offset = 36;
    while offset + 8 <= data.len() {
        let chunk_id = &data[offset..offset + 4];
        let chunk_size = u32::from_le_bytes([
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]) as usize;

        if chunk_id == b"data" {
            let audio_data = &data[offset + 8..(offset + 8 + chunk_size).min(data.len())];
            return Ok((
                AudioFormat {
                    sample_rate,
                    channels: channels as u8,
                    bits_per_sample,
                },
                audio_data,
            ));
        }

        offset += 8 + chunk_size;
    }

    Err("no data chunk found in WAV")
}

/// Load a track from a file
pub fn load_track(path: &str) -> Result<Track, &'static str> {
    let data = crate::file_manager::read_file(path).map_err(|_| "failed to read file")?;

    let (format, audio_data) = parse_wav(&data)?;

    // Calculate duration
    let bytes_per_sample = format.bits_per_sample as u64 / 8;
    let total_samples = audio_data.len() as u64 / (bytes_per_sample * format.channels as u64);
    let duration_ms = total_samples * 1000 / format.sample_rate as u64;

    let filename = path.rsplit('/').next().unwrap_or(path);

    Ok(Track {
        title: String::from(filename),
        artist: String::from("Unknown"),
        album: String::from("Unknown"),
        duration_ms,
        format,
        file_path: String::from(path),
    })
}

/// Play the current track
pub fn play() {
    let mut player = PLAYER.lock();
    if player.playlist.tracks.is_empty() {
        return;
    }
    player.state = PlaybackState::Playing;
    TRACKS_PLAYED.fetch_add(1, Ordering::Relaxed);

    let track = &player.playlist.tracks[player.playlist.current_index];
    crate::serial_println!("[media] playing: {} ({}ms)", track.title, track.duration_ms);

    // In a real implementation:
    // 1. Read PCM data from the track file
    // 2. Feed it to crate::alsa or crate::sound mixer
    // 3. Start the playback timer
}

/// Pause playback
pub fn pause() {
    let mut player = PLAYER.lock();
    if player.state == PlaybackState::Playing {
        player.state = PlaybackState::Paused;
        crate::serial_println!("[media] paused at {}ms", player.position_ms);
    }
}

/// Stop playback
pub fn stop() {
    let mut player = PLAYER.lock();
    player.state = PlaybackState::Stopped;
    player.position_ms = 0;
    crate::serial_println!("[media] stopped");
}

/// Skip to next track
pub fn next_track() {
    let mut player = PLAYER.lock();
    let count = player.playlist.tracks.len();
    if count == 0 {
        return;
    }

    match player.playlist.repeat_mode {
        RepeatMode::RepeatOne => {
            // Stay on same track, restart
            player.position_ms = 0;
        }
        _ => {
            player.playlist.current_index = (player.playlist.current_index + 1) % count;
            player.position_ms = 0;
        }
    }
}

/// Skip to previous track
pub fn prev_track() {
    let mut player = PLAYER.lock();
    let count = player.playlist.tracks.len();
    if count == 0 {
        return;
    }

    // If more than 3 seconds in, restart current track
    if player.position_ms > 3000 {
        player.position_ms = 0;
    } else {
        player.playlist.current_index = if player.playlist.current_index == 0 {
            count - 1
        } else {
            player.playlist.current_index - 1
        };
        player.position_ms = 0;
    }
}

/// Set volume (0-100)
pub fn set_volume(vol: u8) {
    PLAYER.lock().volume = vol.min(100);
}

/// Toggle mute
pub fn toggle_mute() {
    let mut player = PLAYER.lock();
    player.muted = !player.muted;
}

/// Seek to position in milliseconds
pub fn seek(position_ms: u64) {
    let mut player = PLAYER.lock();
    if let Some(track) = player.playlist.tracks.get(player.playlist.current_index) {
        player.position_ms = position_ms.min(track.duration_ms);
    }
}

/// Add a track to the playlist
pub fn add_to_playlist(track: Track) {
    PLAYER.lock().playlist.tracks.push(track);
}

/// Get current playback state
pub fn get_state() -> PlaybackState {
    PLAYER.lock().state
}

/// Get current position in milliseconds
pub fn get_position() -> u64 {
    PLAYER.lock().position_ms
}

/// Get current track info
pub fn current_track() -> Option<Track> {
    let player = PLAYER.lock();
    player
        .playlist
        .tracks
        .get(player.playlist.current_index)
        .cloned()
}

pub fn stats() -> u64 {
    TRACKS_PLAYED.load(Ordering::Relaxed)
}

/// Initialize the media player
pub fn init() {
    crate::serial_println!("[media] media player initialized");
}
