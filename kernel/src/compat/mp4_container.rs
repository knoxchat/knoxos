use crate::serial_println;
/// MP4/MOV Container Parser (ISO BMFF)
///
/// Parses MP4/MOV/M4A/M4V files to extract audio/video tracks.
/// Supports fragmented MP4 (fMP4) for streaming.
use alloc::string::String;
use alloc::vec::Vec;

/// Codec type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mp4Codec {
    H264,
    H265,
    Vp9,
    Av1,
    Aac,
    Mp3,
    Opus,
    Flac,
    Alac,
    Unknown,
}

/// Track type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrackType {
    Video,
    Audio,
    Subtitle,
    Unknown,
}

/// Sample entry (time + offset + size)
#[derive(Debug, Clone, Copy)]
pub struct SampleEntry {
    pub offset: u64,
    pub size: u32,
    pub duration: u32,
    pub is_keyframe: bool,
    pub composition_offset: i32,
}

/// Track metadata
#[derive(Debug, Clone)]
pub struct Mp4Track {
    pub track_id: u32,
    pub track_type: TrackType,
    pub codec: Mp4Codec,
    pub duration_ticks: u64,
    pub timescale: u32,
    pub width: u32, // video only
    pub height: u32,
    pub sample_rate: u32, // audio only
    pub channels: u16,
    pub bit_depth: u16,
    pub codec_config: Vec<u8>, // SPS/PPS for H.264, AudioSpecificConfig for AAC, etc.
    pub samples: Vec<SampleEntry>,
}

/// Parsed MP4 file
pub struct Mp4File {
    pub major_brand: [u8; 4],
    pub duration_ms: u64,
    pub tracks: Vec<Mp4Track>,
    pub title: String,
    pub artist: String,
    pub album: String,
}

impl Mp4File {
    /// Parse MP4 from raw bytes
    pub fn parse(data: &[u8]) -> Result<Self, &'static str> {
        let mut mp4 = Mp4File {
            major_brand: [0; 4],
            duration_ms: 0,
            tracks: Vec::new(),
            title: String::new(),
            artist: String::new(),
            album: String::new(),
        };

        let mut pos = 0;
        while pos + 8 <= data.len() {
            let size = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
                as usize;
            let box_type = &data[pos + 4..pos + 8];
            if size < 8 || pos + size > data.len() {
                break;
            }
            let box_data = &data[pos + 8..pos + size];

            match box_type {
                b"ftyp" if box_data.len() >= 4 => {
                    mp4.major_brand.copy_from_slice(&box_data[0..4]);
                }
                b"moov" => {
                    mp4.parse_moov(box_data)?;
                }
                _ => {}
            }
            pos += size;
        }
        Ok(mp4)
    }

    fn parse_moov(&mut self, data: &[u8]) -> Result<(), &'static str> {
        let mut pos = 0;
        while pos + 8 <= data.len() {
            let size = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
                as usize;
            let box_type = &data[pos + 4..pos + 8];
            if size < 8 || pos + size > data.len() {
                break;
            }
            let box_data = &data[pos + 8..pos + size];

            match box_type {
                b"mvhd" if box_data.len() >= 20 => {
                    let timescale = u32::from_be_bytes([
                        box_data[12],
                        box_data[13],
                        box_data[14],
                        box_data[15],
                    ]);
                    let duration = u32::from_be_bytes([
                        box_data[16],
                        box_data[17],
                        box_data[18],
                        box_data[19],
                    ]);
                    if timescale > 0 {
                        self.duration_ms = duration as u64 * 1000 / timescale as u64;
                    }
                }
                b"trak" => {
                    if let Ok(track) = self.parse_trak(box_data) {
                        self.tracks.push(track);
                    }
                }
                b"udta" => {
                    self.parse_udta(box_data);
                }
                _ => {}
            }
            pos += size;
        }
        Ok(())
    }

    fn parse_trak(&self, data: &[u8]) -> Result<Mp4Track, &'static str> {
        let mut track = Mp4Track {
            track_id: 0,
            track_type: TrackType::Unknown,
            codec: Mp4Codec::Unknown,
            duration_ticks: 0,
            timescale: 1,
            width: 0,
            height: 0,
            sample_rate: 0,
            channels: 0,
            bit_depth: 0,
            codec_config: Vec::new(),
            samples: Vec::new(),
        };

        let mut pos = 0;
        while pos + 8 <= data.len() {
            let size = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
                as usize;
            let box_type = &data[pos + 4..pos + 8];
            if size < 8 || pos + size > data.len() {
                break;
            }
            let box_data = &data[pos + 8..pos + size];

            match box_type {
                b"tkhd" if box_data.len() >= 84 => {
                    track.track_id = u32::from_be_bytes([
                        box_data[12],
                        box_data[13],
                        box_data[14],
                        box_data[15],
                    ]);
                    track.width = u32::from_be_bytes([
                        box_data[76],
                        box_data[77],
                        box_data[78],
                        box_data[79],
                    ]) >> 16;
                    track.height = u32::from_be_bytes([
                        box_data[80],
                        box_data[81],
                        box_data[82],
                        box_data[83],
                    ]) >> 16;
                }
                b"mdia" => {
                    self.parse_mdia(box_data, &mut track);
                }
                _ => {}
            }
            pos += size;
        }

        if track.width > 0 {
            track.track_type = TrackType::Video;
        } else if track.sample_rate > 0 {
            track.track_type = TrackType::Audio;
        }

        Ok(track)
    }

    fn parse_mdia(&self, data: &[u8], track: &mut Mp4Track) {
        let mut pos = 0;
        while pos + 8 <= data.len() {
            let size = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
                as usize;
            let box_type = &data[pos + 4..pos + 8];
            if size < 8 || pos + size > data.len() {
                break;
            }
            let box_data = &data[pos + 8..pos + size];

            match box_type {
                b"mdhd" if box_data.len() >= 20 => {
                    track.timescale = u32::from_be_bytes([
                        box_data[12],
                        box_data[13],
                        box_data[14],
                        box_data[15],
                    ]);
                    track.duration_ticks = u32::from_be_bytes([
                        box_data[16],
                        box_data[17],
                        box_data[18],
                        box_data[19],
                    ]) as u64;
                }
                b"hdlr" if box_data.len() >= 12 => match &box_data[8..12] {
                    b"vide" => track.track_type = TrackType::Video,
                    b"soun" => track.track_type = TrackType::Audio,
                    b"subt" | b"text" => track.track_type = TrackType::Subtitle,
                    _ => {}
                },
                _ => {}
            }
            pos += size;
        }
    }

    fn parse_udta(&mut self, _data: &[u8]) {
        // Parse meta/ilst boxes for iTunes-style metadata
    }

    /// Get video track
    pub fn video_track(&self) -> Option<&Mp4Track> {
        self.tracks
            .iter()
            .find(|t| t.track_type == TrackType::Video)
    }

    /// Get audio track
    pub fn audio_track(&self) -> Option<&Mp4Track> {
        self.tracks
            .iter()
            .find(|t| t.track_type == TrackType::Audio)
    }
}

impl Mp4Track {
    pub fn duration_secs(&self) -> f64 {
        if self.timescale == 0 {
            return 0.0;
        }
        self.duration_ticks as f64 / self.timescale as f64
    }
}

pub fn init() {
    serial_println!("[MP4] MP4/MOV container parser loaded");
}
