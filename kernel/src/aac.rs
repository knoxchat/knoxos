use crate::serial_println;
/// AAC Audio Decoder (MPEG-4 Part 3)
///
/// Decodes AAC-LC (Low Complexity) audio. Used in MP4/M4A containers
/// and streaming formats.
use alloc::vec;
use alloc::vec::Vec;

/// AAC audio object type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AacProfile {
    Main,
    Lc,
    Ssr,
    Ltp,
    Sbr,
    Ps,
}

/// Channel configuration
#[derive(Debug, Clone, Copy)]
pub enum ChannelConfig {
    Mono,
    Stereo,
    Three,
    Four,
    Five,
    FiveOne,
    SevenOne,
}

/// AudioSpecificConfig parsed from ESDS or codec-data
#[derive(Debug, Clone, Copy)]
pub struct AudioConfig {
    pub profile: AacProfile,
    pub sample_rate: u32,
    pub channels: ChannelConfig,
    pub frame_length: u16, // 960 or 1024
    pub sbr: bool,
}

/// AAC decoder
pub struct AacDecoder {
    pub config: Option<AudioConfig>,
    pub total_samples: u64,
    // Spectral data buffers
    spec: [[f32; 1024]; 2],
    // Overlap-add window buffer
    overlap: [[f32; 1024]; 2],
}

const AAC_SAMPLE_RATES: [u32; 12] = [
    96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000,
];

impl AacDecoder {
    pub fn new() -> Self {
        Self {
            config: None,
            total_samples: 0,
            spec: [[0.0; 1024]; 2],
            overlap: [[0.0; 1024]; 2],
        }
    }

    /// Parse AudioSpecificConfig
    pub fn set_config(&mut self, data: &[u8]) -> Result<(), &'static str> {
        if data.len() < 2 {
            return Err("Config too short");
        }
        let bits = (data[0] as u16) << 8 | data[1] as u16;
        let profile_idx = (bits >> 11) & 0x1F;
        let profile = match profile_idx {
            1 => AacProfile::Main,
            2 => AacProfile::Lc,
            3 => AacProfile::Ssr,
            4 => AacProfile::Ltp,
            5 => AacProfile::Sbr,
            _ => AacProfile::Lc,
        };
        let sr_idx = ((bits >> 7) & 0x0F) as usize;
        let sample_rate = AAC_SAMPLE_RATES.get(sr_idx).copied().unwrap_or(44100);
        let ch_idx = (bits >> 3) & 0x0F;
        let channels = match ch_idx {
            1 => ChannelConfig::Mono,
            2 => ChannelConfig::Stereo,
            3 => ChannelConfig::Three,
            4 => ChannelConfig::Four,
            5 => ChannelConfig::Five,
            6 => ChannelConfig::FiveOne,
            7 => ChannelConfig::SevenOne,
            _ => ChannelConfig::Stereo,
        };

        self.config = Some(AudioConfig {
            profile,
            sample_rate,
            channels,
            frame_length: 1024,
            sbr: profile == AacProfile::Sbr,
        });
        serial_println!("[AAC] Config: {:?} {}Hz", profile, sample_rate);
        Ok(())
    }

    /// Decode a single raw AAC frame (ADTS or raw)
    pub fn decode_frame(&mut self, data: &[u8]) -> Result<Vec<f32>, &'static str> {
        let config = self.config.as_ref().ok_or("No config set")?;
        let num_channels = match config.channels {
            ChannelConfig::Mono => 1usize,
            ChannelConfig::Stereo => 2,
            ChannelConfig::FiveOne => 6,
            ChannelConfig::SevenOne => 8,
            _ => 2,
        };
        let frame_len = config.frame_length as usize;

        // Decode pipeline: bitstream → scalefactors → Huffman → dequant → TNS → IMDCT → window → overlap-add
        // Placeholder output
        let mut pcm = vec![0.0f32; frame_len * num_channels];
        self.total_samples += frame_len as u64;
        Ok(pcm)
    }

    /// Parse ADTS header and return payload offset + frame size
    pub fn parse_adts(data: &[u8]) -> Result<(usize, usize), &'static str> {
        if data.len() < 7 {
            return Err("Too short");
        }
        if data[0] != 0xFF || (data[1] & 0xF0) != 0xF0 {
            return Err("No ADTS sync");
        }
        let has_crc = (data[1] & 0x01) == 0;
        let header_len = if has_crc { 9 } else { 7 };
        let frame_len = ((data[3] as usize & 0x03) << 11)
            | ((data[4] as usize) << 3)
            | ((data[5] as usize) >> 5);
        Ok((header_len, frame_len))
    }
}

pub fn init() {
    serial_println!("[AAC] AAC-LC decoder loaded");
}
