// ═══════════════════════════════════════════════════════════════════════
// MEDIA PLAYER TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::media_player::{AudioFormat, PlaybackState, parse_wav};

#[test_case]
fn test_cd_quality_format() {
    let fmt = AudioFormat::cd_quality();
    assert_eq!(fmt.sample_rate, 44100);
    assert_eq!(fmt.channels, 2);
    assert_eq!(fmt.bits_per_sample, 16);
}

#[test_case]
fn test_wav_parse_too_small() {
    let data = [0u8; 10];
    assert!(parse_wav(&data).is_err());
}

#[test_case]
fn test_wav_parse_invalid_magic() {
    let mut data = [0u8; 44];
    // Not RIFF
    data[0..4].copy_from_slice(b"NOPE");
    assert!(parse_wav(&data).is_err());
}

#[test_case]
fn test_playback_states() {
    assert_ne!(PlaybackState::Playing, PlaybackState::Paused);
    assert_ne!(PlaybackState::Stopped, PlaybackState::Playing);
}
