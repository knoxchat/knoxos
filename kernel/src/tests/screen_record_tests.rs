// ═══════════════════════════════════════════════════════════════════════
// SCREEN RECORDING TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::screen_record::{RecordConfig, RecordingState};

#[test_case]
fn test_recording_state() {
    assert_ne!(RecordingState::Idle, RecordingState::Recording);
    assert_ne!(RecordingState::Recording, RecordingState::Paused);
}

#[test_case]
fn test_record_config_default() {
    let config = RecordConfig::default();
    assert_eq!(config.fps, 30);
    assert_eq!(config.capture_width, 1920);
    assert_eq!(config.capture_height, 1080);
    assert!(config.include_cursor);
    assert_eq!(config.max_duration_secs, 300);
}
