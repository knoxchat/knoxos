//! File type detection from magic bytes, shebang, and extension.
use alloc::string::String;

use crate::path;

// ─── File type detection ────────────────────────────────────────────

/// Detected file type from magic bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectedFileType {
    ElfExecutable,
    ElfSharedLib,
    ElfRelocatable,
    ElfCore,
    ShellScript(String), // interpreter path
    PythonScript,
    PerlScript,
    RubyScript,
    GzipCompressed,
    Bzip2Compressed,
    XzCompressed,
    ZstdCompressed,
    ZipArchive,
    TarArchive,
    PngImage,
    JpegImage,
    GifImage,
    BmpImage,
    WebpImage,
    SvgImage,
    PdfDocument,
    RiffMedia, // WAV, AVI
    Mp3Audio,
    FlacAudio,
    OggMedia,
    Mp4Video,
    MkvVideo,
    AsciiText,
    Utf8Text,
    EmptyFile,
    BinaryData,
    RustSource,
    CSource,
    CppSource,
    JavaSource,
    JavaScriptSource,
    JsonData,
    XmlData,
    HtmlDocument,
    CssStylesheet,
    MakefileScript,
    TomlConfig,
    YamlConfig,
    IniConfig,
    Unknown,
}

impl DetectedFileType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::ElfExecutable => "ELF 64-bit LSB executable, x86-64",
            Self::ElfSharedLib => "ELF 64-bit LSB shared object, x86-64",
            Self::ElfRelocatable => "ELF 64-bit LSB relocatable, x86-64",
            Self::ElfCore => "ELF 64-bit LSB core file, x86-64",
            Self::ShellScript(interp) => "POSIX shell script",
            Self::PythonScript => "Python script, ASCII text executable",
            Self::PerlScript => "Perl script, ASCII text executable",
            Self::RubyScript => "Ruby script, ASCII text executable",
            Self::GzipCompressed => "gzip compressed data",
            Self::Bzip2Compressed => "bzip2 compressed data",
            Self::XzCompressed => "XZ compressed data",
            Self::ZstdCompressed => "Zstandard compressed data",
            Self::ZipArchive => "Zip archive data",
            Self::TarArchive => "POSIX tar archive",
            Self::PngImage => "PNG image data",
            Self::JpegImage => "JPEG image data",
            Self::GifImage => "GIF image data",
            Self::BmpImage => "BMP image data",
            Self::WebpImage => "WebP image data",
            Self::SvgImage => "SVG image",
            Self::PdfDocument => "PDF document",
            Self::RiffMedia => "RIFF media data",
            Self::Mp3Audio => "MPEG ADTS audio, layer III",
            Self::FlacAudio => "FLAC audio bitstream data",
            Self::OggMedia => "Ogg data",
            Self::Mp4Video => "ISO Media, MP4",
            Self::MkvVideo => "Matroska video",
            Self::AsciiText => "ASCII text",
            Self::Utf8Text => "UTF-8 Unicode text",
            Self::EmptyFile => "empty",
            Self::BinaryData => "data",
            Self::RustSource => "Rust source, ASCII text",
            Self::CSource => "C source, ASCII text",
            Self::CppSource => "C++ source, ASCII text",
            Self::JavaSource => "Java source, ASCII text",
            Self::JavaScriptSource => "JavaScript source, ASCII text",
            Self::JsonData => "JSON data",
            Self::XmlData => "XML document",
            Self::HtmlDocument => "HTML document",
            Self::CssStylesheet => "CSS stylesheet",
            Self::MakefileScript => "Makefile script",
            Self::TomlConfig => "TOML configuration",
            Self::YamlConfig => "YAML configuration",
            Self::IniConfig => "INI configuration",
            Self::Unknown => "data",
        }
    }

    /// Get MIME type
    pub fn mime_type(&self) -> &str {
        match self {
            Self::ElfExecutable | Self::ElfSharedLib | Self::ElfRelocatable => {
                "application/x-executable"
            }
            Self::ShellScript(_) | Self::PythonScript | Self::PerlScript | Self::RubyScript => {
                "text/x-script"
            }
            Self::GzipCompressed => "application/gzip",
            Self::Bzip2Compressed => "application/x-bzip2",
            Self::XzCompressed => "application/x-xz",
            Self::ZstdCompressed => "application/zstd",
            Self::ZipArchive => "application/zip",
            Self::TarArchive => "application/x-tar",
            Self::PngImage => "image/png",
            Self::JpegImage => "image/jpeg",
            Self::GifImage => "image/gif",
            Self::BmpImage => "image/bmp",
            Self::WebpImage => "image/webp",
            Self::SvgImage => "image/svg+xml",
            Self::PdfDocument => "application/pdf",
            Self::RiffMedia => "audio/wav",
            Self::Mp3Audio => "audio/mpeg",
            Self::FlacAudio => "audio/flac",
            Self::OggMedia => "audio/ogg",
            Self::Mp4Video => "video/mp4",
            Self::MkvVideo => "video/x-matroska",
            Self::AsciiText | Self::Utf8Text => "text/plain",
            Self::RustSource | Self::CSource | Self::CppSource => "text/x-source",
            Self::JsonData => "application/json",
            Self::XmlData => "application/xml",
            Self::HtmlDocument => "text/html",
            Self::CssStylesheet => "text/css",
            Self::JavaScriptSource => "application/javascript",
            Self::TomlConfig | Self::YamlConfig | Self::IniConfig => "text/plain",
            _ => "application/octet-stream",
        }
    }
}

/// Detect file type from content (magic bytes) and filename.
pub fn detect_file_type(path: &str, data: &[u8]) -> DetectedFileType {
    if data.is_empty() {
        return DetectedFileType::EmptyFile;
    }

    // Check magic bytes first
    if data.len() >= 4 && data[0] == 0x7f && data[1] == b'E' && data[2] == b'L' && data[3] == b'F' {
        // ELF — check type at offset 16
        if data.len() >= 18 {
            match data[16] {
                1 => return DetectedFileType::ElfRelocatable,
                2 => return DetectedFileType::ElfExecutable,
                3 => return DetectedFileType::ElfSharedLib,
                4 => return DetectedFileType::ElfCore,
                _ => return DetectedFileType::ElfExecutable,
            }
        }
        return DetectedFileType::ElfExecutable;
    }

    if data.len() >= 2 && data[0] == b'#' && data[1] == b'!' {
        // Shebang
        let line_end = data
            .iter()
            .position(|&b| b == b'\n')
            .unwrap_or(data.len().min(256));
        if let Ok(shebang) = core::str::from_utf8(&data[2..line_end]) {
            let interp = shebang.trim();
            if interp.contains("python") {
                return DetectedFileType::PythonScript;
            }
            if interp.contains("perl") {
                return DetectedFileType::PerlScript;
            }
            if interp.contains("ruby") {
                return DetectedFileType::RubyScript;
            }
            return DetectedFileType::ShellScript(String::from(interp));
        }
    }

    // Compressed formats
    if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
        return DetectedFileType::GzipCompressed;
    }
    if data.len() >= 3 && data[0] == b'B' && data[1] == b'Z' && data[2] == b'h' {
        return DetectedFileType::Bzip2Compressed;
    }
    if data.len() >= 6
        && data[0] == 0xfd
        && data[1] == 0x37
        && data[2] == 0x7a
        && data[3] == 0x58
        && data[4] == 0x5a
        && data[5] == 0x00
    {
        return DetectedFileType::XzCompressed;
    }
    if data.len() >= 4 && data[0] == 0x28 && data[1] == 0xb5 && data[2] == 0x2f && data[3] == 0xfd {
        return DetectedFileType::ZstdCompressed;
    }

    // Archives
    if data.len() >= 4 && data[0] == 0x50 && data[1] == 0x4b && data[2] == 0x03 && data[3] == 0x04 {
        return DetectedFileType::ZipArchive;
    }
    if data.len() >= 263 && &data[257..262] == b"ustar" {
        return DetectedFileType::TarArchive;
    }

    // Images
    if data.len() >= 8
        && data[0] == 0x89
        && data[1] == b'P'
        && data[2] == b'N'
        && data[3] == b'G'
        && data[4] == 0x0d
        && data[5] == 0x0a
        && data[6] == 0x1a
        && data[7] == 0x0a
    {
        return DetectedFileType::PngImage;
    }
    if data.len() >= 3 && data[0] == 0xff && data[1] == 0xd8 && data[2] == 0xff {
        return DetectedFileType::JpegImage;
    }
    if data.len() >= 6 && (data[..6] == *b"GIF87a" || data[..6] == *b"GIF89a") {
        return DetectedFileType::GifImage;
    }
    if data.len() >= 2 && data[0] == b'B' && data[1] == b'M' {
        return DetectedFileType::BmpImage;
    }
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return DetectedFileType::WebpImage;
    }

    // Audio/Video
    if data.len() >= 12 && &data[0..4] == b"RIFF" {
        return DetectedFileType::RiffMedia;
    }
    if data.len() >= 3 && (data[0] == 0xff && (data[1] & 0xe0) == 0xe0) {
        return DetectedFileType::Mp3Audio;
    }
    if data.len() >= 4 && &data[0..4] == b"fLaC" {
        return DetectedFileType::FlacAudio;
    }
    if data.len() >= 4 && &data[0..4] == b"OggS" {
        return DetectedFileType::OggMedia;
    }
    if data.len() >= 8 && &data[4..8] == b"ftyp" {
        return DetectedFileType::Mp4Video;
    }
    if data.len() >= 4 && data[0] == 0x1a && data[1] == 0x45 && data[2] == 0xdf && data[3] == 0xa3 {
        return DetectedFileType::MkvVideo;
    }

    // PDF
    if data.len() >= 5 && &data[0..5] == b"%PDF-" {
        return DetectedFileType::PdfDocument;
    }

    // Try text-based detection
    let sample_len = data.len().min(8192);
    let sample = &data[..sample_len];

    let is_ascii = sample
        .iter()
        .all(|&b| b.is_ascii() || b == b'\n' || b == b'\r' || b == b'\t');
    let is_utf8 = core::str::from_utf8(sample).is_ok();

    if is_ascii || is_utf8 {
        // Use file extension to determine type
        if let Some(ext) = path::Path::new(path).extension() {
            match ext {
                "rs" => return DetectedFileType::RustSource,
                "c" | "h" => return DetectedFileType::CSource,
                "cpp" | "cc" | "cxx" | "hpp" | "hh" => return DetectedFileType::CppSource,
                "java" => return DetectedFileType::JavaSource,
                "js" | "mjs" => return DetectedFileType::JavaScriptSource,
                "json" => return DetectedFileType::JsonData,
                "xml" => return DetectedFileType::XmlData,
                "html" | "htm" => return DetectedFileType::HtmlDocument,
                "css" => return DetectedFileType::CssStylesheet,
                "svg" => return DetectedFileType::SvgImage,
                "toml" => return DetectedFileType::TomlConfig,
                "yaml" | "yml" => return DetectedFileType::YamlConfig,
                "ini" | "cfg" | "conf" => return DetectedFileType::IniConfig,
                _ => {}
            }
        }

        // Check content patterns
        if let Ok(text) = core::str::from_utf8(sample) {
            if text.starts_with('{') || text.starts_with('[') {
                // Might be JSON
                if text.contains("\":") || text.contains("\": ") {
                    return DetectedFileType::JsonData;
                }
            }
            if text.starts_with("<?xml") || text.starts_with("<svg") {
                return if text.contains("<svg") {
                    DetectedFileType::SvgImage
                } else {
                    DetectedFileType::XmlData
                };
            }
            if text.starts_with("<!DOCTYPE") || text.starts_with("<html") {
                return DetectedFileType::HtmlDocument;
            }
        }

        if is_ascii {
            return DetectedFileType::AsciiText;
        }
        return DetectedFileType::Utf8Text;
    }

    DetectedFileType::BinaryData
}
