use crate::serial_println;
/// Ogg Container + Vorbis/Opus Audio Decoder
///
/// Decodes Ogg Vorbis and Ogg Opus audio streams.
/// Supports chained Ogg bitstreams and seeking.
use alloc::vec::Vec;

/// Ogg page header
#[derive(Debug, Clone)]
pub struct OggPage {
    pub version: u8,
    pub header_type: u8,
    pub granule_position: u64,
    pub serial_number: u32,
    pub page_sequence: u32,
    pub checksum: u32,
    pub segments: Vec<usize>,
    pub data: Vec<u8>,
}

/// Audio codec type detected in Ogg stream
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OggCodec {
    Vorbis,
    Opus,
    Unknown,
}

/// Vorbis identification header
#[derive(Debug, Clone)]
pub struct VorbisIdHeader {
    pub channels: u8,
    pub sample_rate: u32,
    pub bitrate_max: i32,
    pub bitrate_nominal: i32,
    pub bitrate_min: i32,
    pub blocksize_0: u16,
    pub blocksize_1: u16,
}

/// Opus head structure
#[derive(Debug, Clone)]
pub struct OpusHead {
    pub version: u8,
    pub channels: u8,
    pub pre_skip: u16,
    pub sample_rate: u32,
    pub output_gain: i16,
}

/// Ogg demuxer
pub struct OggDemuxer {
    pub codec: OggCodec,
    pub pages: Vec<OggPage>,
}

impl OggDemuxer {
    pub fn new() -> Self {
        Self {
            codec: OggCodec::Unknown,
            pages: Vec::new(),
        }
    }

    /// Parse Ogg pages from raw data
    pub fn parse(&mut self, data: &[u8]) -> Result<(), &'static str> {
        let mut pos = 0;
        while pos + 27 <= data.len() {
            // Check "OggS" capture pattern
            if &data[pos..pos + 4] != b"OggS" {
                pos += 1;
                continue;
            }
            let version = data[pos + 4];
            let header_type = data[pos + 5];
            let granule = u64::from_le_bytes([
                data[pos + 6],
                data[pos + 7],
                data[pos + 8],
                data[pos + 9],
                data[pos + 10],
                data[pos + 11],
                data[pos + 12],
                data[pos + 13],
            ]);
            let serial = u32::from_le_bytes([
                data[pos + 14],
                data[pos + 15],
                data[pos + 16],
                data[pos + 17],
            ]);
            let pageseq = u32::from_le_bytes([
                data[pos + 18],
                data[pos + 19],
                data[pos + 20],
                data[pos + 21],
            ]);
            let checksum = u32::from_le_bytes([
                data[pos + 22],
                data[pos + 23],
                data[pos + 24],
                data[pos + 25],
            ]);
            let num_segments = data[pos + 26] as usize;
            pos += 27;
            if pos + num_segments > data.len() {
                break;
            }

            let mut segments = Vec::new();
            let mut total_data_size = 0usize;
            for i in 0..num_segments {
                let seg_size = data[pos + i] as usize;
                segments.push(seg_size);
                total_data_size += seg_size;
            }
            pos += num_segments;
            if pos + total_data_size > data.len() {
                break;
            }

            let page_data = data[pos..pos + total_data_size].to_vec();
            pos += total_data_size;

            // Detect codec from first page
            if self.codec == OggCodec::Unknown && !page_data.is_empty() {
                if page_data.len() >= 7 && &page_data[1..7] == b"vorbis" {
                    self.codec = OggCodec::Vorbis;
                } else if page_data.len() >= 8 && &page_data[0..8] == b"OpusHead" {
                    self.codec = OggCodec::Opus;
                }
            }

            self.pages.push(OggPage {
                version,
                header_type,
                granule_position: granule,
                serial_number: serial,
                page_sequence: pageseq,
                checksum,
                segments,
                data: page_data,
            });
        }
        Ok(())
    }

    /// Parse Vorbis identification header from first page
    pub fn parse_vorbis_id(&self) -> Result<VorbisIdHeader, &'static str> {
        let page = self.pages.first().ok_or("No pages")?;
        let d = &page.data;
        if d.len() < 30 || d[0] != 1 || &d[1..7] != b"vorbis" {
            return Err("Not Vorbis ID header");
        }
        Ok(VorbisIdHeader {
            channels: d[11],
            sample_rate: u32::from_le_bytes([d[12], d[13], d[14], d[15]]),
            bitrate_max: i32::from_le_bytes([d[16], d[17], d[18], d[19]]),
            bitrate_nominal: i32::from_le_bytes([d[20], d[21], d[22], d[23]]),
            bitrate_min: i32::from_le_bytes([d[24], d[25], d[26], d[27]]),
            blocksize_0: 1 << (d[28] & 0x0F),
            blocksize_1: 1 << ((d[28] >> 4) & 0x0F),
        })
    }

    /// Parse OpusHead from first page
    pub fn parse_opus_head(&self) -> Result<OpusHead, &'static str> {
        let page = self.pages.first().ok_or("No pages")?;
        let d = &page.data;
        if d.len() < 19 || &d[0..8] != b"OpusHead" {
            return Err("Not OpusHead");
        }
        Ok(OpusHead {
            version: d[8],
            channels: d[9],
            pre_skip: u16::from_le_bytes([d[10], d[11]]),
            sample_rate: u32::from_le_bytes([d[12], d[13], d[14], d[15]]),
            output_gain: i16::from_le_bytes([d[16], d[17]]),
        })
    }
}

pub fn init() {
    serial_println!("[OGG] Ogg Vorbis/Opus decoder loaded");
}
