use alloc::string::String;
use alloc::vec::Vec;

/// Voice pipeline state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoicePipelineState {
    Idle,
    Listening,
    Processing,
    Speaking,
}

/// Voice pipeline
pub struct VoicePipeline {
    pub state: VoicePipelineState,
    pub stt_model: String, // Speech-to-text model
    pub tts_model: String, // Text-to-speech model
    pub language: String,
    pub last_transcript: String,
}

lazy_static::lazy_static! {
    static ref VOICE_PIPELINE: spin::Mutex<VoicePipeline> = spin::Mutex::new(VoicePipeline {
        state: VoicePipelineState::Idle,
        stt_model: String::new(),
        tts_model: String::new(),
        language: String::new(),
        last_transcript: String::new(),
    });
}

/// Initialize voice pipeline
pub fn voice_pipeline_init(stt_model: &str, tts_model: &str, language: &str) {
    let mut vp = VOICE_PIPELINE.lock();
    vp.stt_model = String::from(stt_model);
    vp.tts_model = String::from(tts_model);
    vp.language = String::from(language);
    crate::serial_println!(
        "[AI] Voice pipeline: STT={}, TTS={}, lang={}",
        stt_model,
        tts_model,
        language
    );
}

/// Start listening for voice input
pub fn voice_start_listening() -> bool {
    let mut vp = VOICE_PIPELINE.lock();
    if vp.state == VoicePipelineState::Idle {
        vp.state = VoicePipelineState::Listening;
        return true;
    }
    false
}

/// Process recorded audio and return transcript
pub fn voice_process_audio(_audio_data: &[i16]) -> Option<String> {
    let mut vp = VOICE_PIPELINE.lock();
    vp.state = VoicePipelineState::Processing;
    // In real implementation: run Whisper inference on audio data
    let transcript = String::from("(transcribed text)");
    vp.last_transcript = transcript.clone();
    vp.state = VoicePipelineState::Idle;
    Some(transcript)
}

/// Synthesize speech from text
pub fn voice_speak(text: &str) -> Option<Vec<i16>> {
    let mut vp = VOICE_PIPELINE.lock();
    vp.state = VoicePipelineState::Speaking;
    // In real implementation: run TTS model
    let _text_len = text.len();
    let samples = alloc::vec![0i16; 16000]; // 1 second of 16kHz audio stub
    vp.state = VoicePipelineState::Idle;
    Some(samples)
}
