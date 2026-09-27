use alloc::vec::Vec;

// ═══════════════════════════════════════════════════════════════════════
// Audio Codec Decoders (MP3, AAC, OGG Vorbis/Opus, FLAC)
// ═══════════════════════════════════════════════════════════════════════

/// Audio codec type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    Pcm,
    Mp3,
    Aac,
    OggVorbis,
    OggOpus,
    Flac,
    Wav,
}

/// Decoded audio buffer
pub struct AudioBuffer {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
    pub channels: u8,
}

/// MP3 frame header
pub struct Mp3FrameHeader {
    pub version: u8,  // MPEG version (1, 2, 2.5)
    pub layer: u8,    // Layer (1, 2, 3)
    pub bitrate: u32, // kbps
    pub sample_rate: u32,
    pub channels: u8,
    pub frame_size: usize,
}

/// Decode an MP3 frame header from 4 bytes
pub fn mp3_parse_header(header: &[u8; 4]) -> Option<Mp3FrameHeader> {
    // Check sync word: 0xFFE0
    if header[0] != 0xFF || (header[1] & 0xE0) != 0xE0 {
        return None;
    }
    let version = match (header[1] >> 3) & 3 {
        0 => return None, // reserved
        1 => 3,           // MPEG 2.5 (unofficial but common)
        2 => 2,
        3 => 1,
        _ => return None,
    };
    let layer = match (header[1] >> 1) & 3 {
        1 => 3, // Layer III
        2 => 2,
        3 => 1,
        _ => return None,
    };
    let bitrate_idx = ((header[2] >> 4) & 0xF) as usize;
    let sr_idx = ((header[2] >> 2) & 3) as usize;
    let channels = if (header[3] >> 6) == 3 { 1 } else { 2 };

    // Bitrate table for MPEG1, Layer III
    let bitrate_table: [u32; 16] = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
    ];
    let sr_table: [u32; 4] = [44100, 48000, 32000, 0];

    let bitrate = bitrate_table.get(bitrate_idx).copied().unwrap_or(0);
    let sample_rate = sr_table.get(sr_idx).copied().unwrap_or(0);
    if bitrate == 0 || sample_rate == 0 {
        return None;
    }

    let padding = ((header[2] >> 1) & 1) as usize;
    let frame_size = (144 * bitrate as usize * 1000 / sample_rate as usize) + padding;

    Some(Mp3FrameHeader {
        version,
        layer,
        bitrate,
        sample_rate,
        channels,
        frame_size,
    })
}

/// Decode MP3 data to PCM samples (simplified — produces silence with correct frame structure)
pub fn mp3_decode(data: &[u8]) -> Option<AudioBuffer> {
    if data.len() < 4 {
        return None;
    }
    let mut hdr_bytes = [0u8; 4];
    hdr_bytes.copy_from_slice(&data[..4]);
    let hdr = mp3_parse_header(&hdr_bytes)?;
    // In a full implementation: Huffman decode, dequantize, IMDCT, frequency inversion, synthesis filterbank
    let samples_per_frame = 1152; // MP3 Layer III = 1152 samples/frame
    let num_frames = data.len() / hdr.frame_size.max(1);
    let total_samples = num_frames * samples_per_frame * hdr.channels as usize;
    Some(AudioBuffer {
        samples: alloc::vec![0i16; total_samples],
        sample_rate: hdr.sample_rate,
        channels: hdr.channels,
    })
}

/// Parse AAC ADTS header
pub fn aac_parse_adts(data: &[u8]) -> Option<(u32, u8, usize)> {
    if data.len() < 7 {
        return None;
    }
    // ADTS sync: 0xFFF
    if data[0] != 0xFF || (data[1] & 0xF0) != 0xF0 {
        return None;
    }
    let sr_idx = ((data[2] >> 2) & 0xF) as usize;
    let channels = ((data[2] & 0x01) << 2) | ((data[3] >> 6) & 0x03);
    let frame_len = ((data[3] as usize & 0x03) << 11)
        | ((data[4] as usize) << 3)
        | ((data[5] as usize >> 5) & 0x07);
    let sr_table: [u32; 13] = [
        96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
    ];
    let sample_rate = sr_table.get(sr_idx).copied().unwrap_or(44100);
    Some((sample_rate, channels, frame_len))
}

/// Decode AAC to PCM (stub — correct frame structure, outputs silence)
pub fn aac_decode(data: &[u8]) -> Option<AudioBuffer> {
    let (sr, ch, _) = aac_parse_adts(data)?;
    let samples = 1024; // AAC frame = 1024 samples
    Some(AudioBuffer {
        samples: alloc::vec![0i16; samples * ch as usize],
        sample_rate: sr,
        channels: ch,
    })
}

/// Decode OGG Vorbis/Opus container header
pub fn ogg_parse_header(data: &[u8]) -> Option<(AudioCodec, u32, u8)> {
    if data.len() < 35 {
        return None;
    }
    if &data[0..4] != b"OggS" {
        return None;
    }
    // Check for Vorbis or Opus identification header in first page payload
    let payload_start = 27 + data.get(26).copied().unwrap_or(0) as usize;
    if data.len() <= payload_start + 8 {
        return None;
    }
    let payload = &data[payload_start..];
    if payload.len() >= 7 && &payload[1..7] == b"vorbis" {
        let ch = payload.get(11).copied().unwrap_or(2);
        let sr = if payload.len() >= 16 {
            u32::from_le_bytes([payload[12], payload[13], payload[14], payload[15]])
        } else {
            44100
        };
        Some((AudioCodec::OggVorbis, sr, ch))
    } else if payload.len() >= 8 && &payload[0..8] == b"OpusHead" {
        let ch = payload.get(9).copied().unwrap_or(2);
        Some((AudioCodec::OggOpus, 48000, ch)) // Opus always 48kHz internally
    } else {
        None
    }
}

/// Decode OGG to PCM (stub)
pub fn ogg_decode(data: &[u8]) -> Option<AudioBuffer> {
    let (_, sr, ch) = ogg_parse_header(data)?;
    Some(AudioBuffer {
        samples: alloc::vec![0i16; 4096],
        sample_rate: sr,
        channels: ch,
    })
}

/// FLAC stream info
pub struct FlacStreamInfo {
    pub min_block_size: u16,
    pub max_block_size: u16,
    pub sample_rate: u32,
    pub channels: u8,
    pub bits_per_sample: u8,
    pub total_samples: u64,
}

/// Parse FLAC stream header
pub fn flac_parse_header(data: &[u8]) -> Option<FlacStreamInfo> {
    if data.len() < 42 {
        return None;
    }
    if &data[0..4] != b"fLaC" {
        return None;
    }
    // METADATA_BLOCK_HEADER + STREAMINFO
    let min_bs = u16::from_be_bytes([data[8], data[9]]);
    let max_bs = u16::from_be_bytes([data[10], data[11]]);
    let sr = ((data[18] as u32) << 12) | ((data[19] as u32) << 4) | ((data[20] as u32) >> 4);
    let channels = ((data[20] >> 1) & 0x07) + 1;
    let bps = (((data[20] & 0x01) << 4) | ((data[21] >> 4) & 0x0F)) + 1;
    let total = ((data[21] as u64 & 0x0F) << 32)
        | ((data[22] as u64) << 24)
        | ((data[23] as u64) << 16)
        | ((data[24] as u64) << 8)
        | (data[25] as u64);
    Some(FlacStreamInfo {
        min_block_size: min_bs,
        max_block_size: max_bs,
        sample_rate: sr,
        channels,
        bits_per_sample: bps,
        total_samples: total,
    })
}

/// Decode FLAC to PCM (stub)
pub fn flac_decode(data: &[u8]) -> Option<AudioBuffer> {
    let info = flac_parse_header(data)?;
    Some(AudioBuffer {
        samples: alloc::vec![0i16; info.total_samples.min(1_000_000) as usize * info.channels as usize],
        sample_rate: info.sample_rate,
        channels: info.channels,
    })
}
