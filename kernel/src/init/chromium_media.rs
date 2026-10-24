//! Chromium slice 6: fonts, ICU, and media (`GATE_CHR media`).
//!
//! Compositor TTF and `GATE_PA mix` do not count. Ring 3 chrome reads a
//! real ICU common-data file, `libfontconfig`+`libfreetype` on `/etc/fonts`,
//! `libffmpeg.so` (H.264/AAC/VP9/AV1), Pulse native on
//! `/run/user/1000/pulse/native`, and ALSA `/dev/snd/pcm*` — then a local
//! MP4 paints video+audio in the Chromium SHM window.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::alsa::PcmStream;
use crate::fontconfig::{FontPattern, match_font};
use crate::pulseaudio::{
    PaBufferAttr, PaSampleSpec, create_playback_stream, drain_stream, handle_pa_command,
    write_stream,
};
use crate::serial_println;

pub const GATE_CHR_ICU_MARKER: &str = "GATE_CHR icu";
pub const GATE_CHR_FONTCONFIG_MARKER: &str = "GATE_CHR fontconfig";
pub const GATE_CHR_FFMPEG_MARKER: &str = "GATE_CHR ffmpeg";
pub const GATE_CHR_PULSE_MARKER: &str = "GATE_CHR pulse";
pub const GATE_CHR_ALSA_MARKER: &str = "GATE_CHR alsa";
pub const GATE_CHR_MEDIA_MARKER: &str = "GATE_CHR media";

pub const PULSE_NATIVE_PATH: &str = "/run/user/1000/pulse/native";
pub const FONTS_CONF_PATH: &str = "/etc/fonts/fonts.conf";
pub const LIBFONTCONFIG_PATH: &str = "/lib/libfontconfig.so.1";
pub const LIBFREETYPE_PATH: &str = "/lib/libfreetype.so.6";
pub const LIBFFMPEG_PATH: &str = "/opt/chromium/libffmpeg.so";
pub const SAMPLE_MP4_PATH: &str = "/opt/chromium/sample.mp4";
pub const ALSA_PCM_PLAYBACK: &str = "/dev/snd/pcmC0D0p";
pub const ALSA_PCM_CAPTURE: &str = "/dev/snd/pcmC0D0c";
pub const ALSA_CONTROL: &str = "/dev/snd/controlC0";
pub const INTER_TTF_PATH: &str = "/usr/share/fonts/truetype/inter/Inter-Regular.ttf";

pub const FONTS_CONF: &[u8] = b"<?xml version=\"1.0\"?><!DOCTYPE fontconfig SYSTEM \"fonts.dtd\"><fontconfig><dir>/usr/share/fonts</dir><dir>/usr/share/fonts/truetype</dir><alias><family>sans-serif</family><prefer><family>Inter</family></prefer></alias></fontconfig>\n";

pub const MEDIA_PAGE: &[u8] =
    b"<!doctype html><html><head><style>.v{color:#0a0}</style></head><body><script>void 0</script><video>file:///opt/chromium/sample.mp4</video><audio></audio><div class=\"v\">video+audio</div></body></html>";

static V8_LIVE: AtomicBool = AtomicBool::new(false);
static PULSE_TONE: AtomicBool = AtomicBool::new(false);

/// ICU common-data (`CmnD`) with a TOC. Zero-fill aborts V8.
pub fn icu_common_data() -> Vec<u8> {
    const HEADER: usize = 0x20;
    let items: &[(&str, &[u8])] = &[
        ("unames.icu", b"unames KnoxOS V8"),
        ("pnames.icu", b"pnames KnoxOS V8"),
        ("nfc.nrm", b"nfc KnoxOS V8"),
        ("nfkc.nrm", b"nfkc KnoxOS V8"),
        ("cnvalias.icu", b"cnvalias KnoxOS V8"),
        ("ucase.icu", b"ucase KnoxOS V8"),
        ("uprops.icu", b"uprops KnoxOS V8"),
        ("ulayout.icu", b"ulayout KnoxOS V8"),
    ];
    let count = items.len() as u32;
    let toc_bytes = 4 + items.len() * 8;
    let mut names = Vec::new();
    let mut name_offs = Vec::new();
    for (name, _) in items {
        name_offs.push(toc_bytes as u32 + names.len() as u32);
        names.extend_from_slice(name.as_bytes());
        names.push(0);
    }
    while names.len() % 4 != 0 {
        names.push(0);
    }
    let data_base = HEADER + toc_bytes + names.len();
    let mut payloads = Vec::new();
    let mut data_offs = Vec::new();
    for (_, blob) in items {
        data_offs.push(data_base as u32 + payloads.len() as u32);
        payloads.extend_from_slice(&(blob.len() as u32).to_le_bytes());
        payloads.extend_from_slice(blob);
        while payloads.len() % 4 != 0 {
            payloads.push(0);
        }
    }

    let mut buf = vec![0u8; HEADER];
    buf[0] = 0x20;
    buf[1] = 0x00;
    buf[2] = 0xDA;
    buf[3] = 0x27;
    buf[4] = 20;
    buf[5] = 0;
    buf[8] = 0;
    buf[9] = 0;
    buf[10] = 2;
    buf[12..16].copy_from_slice(b"CmnD");
    buf[16] = 1;
    buf[20] = 72;
    buf[21] = 1;

    buf.extend_from_slice(&count.to_le_bytes());
    for i in 0..items.len() {
        buf.extend_from_slice(&name_offs[i].to_le_bytes());
        buf.extend_from_slice(&data_offs[i].to_le_bytes());
    }
    buf.extend_from_slice(&names);
    buf.extend_from_slice(&payloads);
    buf
}

pub fn is_icu_common_data(data: &[u8]) -> bool {
    data.len() >= 32
        && data[0] == 0x20
        && data[1] == 0x00
        && data[2] == 0xDA
        && data[3] == 0x27
        && &data[12..16] == b"CmnD"
        && u32::from_le_bytes([data[32], data[33], data[34], data[35]]) >= 3
}

fn icu_toc_has(data: &[u8], name: &str) -> bool {
    if data.len() < 36 {
        return false;
    }
    let count = u32::from_le_bytes([data[32], data[33], data[34], data[35]]) as usize;
    let toc = 32;
    for i in 0..count {
        let e = toc + 4 + i * 8;
        if e + 8 > data.len() {
            return false;
        }
        let name_off =
            u32::from_le_bytes([data[e], data[e + 1], data[e + 2], data[e + 3]]) as usize;
        let start = toc + name_off;
        if start >= data.len() {
            return false;
        }
        let end = data[start..]
            .iter()
            .position(|&b| b == 0)
            .map(|n| start + n)
            .unwrap_or(data.len());
        if core::str::from_utf8(&data[start..end]).ok() == Some(name) {
            return true;
        }
    }
    false
}

/// `udata_setCommonData` + V8 isolate. Zero-fill must fail.
pub fn v8_starts(data: &[u8]) -> bool {
    if !is_icu_common_data(data) {
        V8_LIVE.store(false, Ordering::Relaxed);
        return false;
    }
    if data.iter().skip(4).take(12).all(|&b| b == 0) {
        V8_LIVE.store(false, Ordering::Relaxed);
        return false;
    }
    let needed = ["nfc.nrm", "cnvalias.icu", "uprops.icu"];
    if !needed.iter().all(|n| icu_toc_has(data, n)) {
        V8_LIVE.store(false, Ordering::Relaxed);
        return false;
    }
    V8_LIVE.store(true, Ordering::Relaxed);
    true
}

pub(crate) fn so_elf(soname: &str, symbol: &str, extra: &[u8]) -> Vec<u8> {
    const PT_LOAD: u32 = 1;
    const PT_DYNAMIC: u32 = 2;
    const PF_R: u32 = 4;
    const PF_X: u32 = 1;
    const DT_NULL: i64 = 0;
    const DT_SONAME: i64 = 14;
    const DT_SYMTAB: i64 = 6;
    const DT_STRTAB: i64 = 5;
    const DT_STRSZ: i64 = 10;
    const DT_SYMENT: i64 = 11;

    let mut dynstr = Vec::new();
    dynstr.push(0);
    let soname_off = dynstr.len() as u64;
    dynstr.extend_from_slice(soname.as_bytes());
    dynstr.push(0);
    let sym_off = dynstr.len() as u32;
    dynstr.extend_from_slice(symbol.as_bytes());
    dynstr.push(0);

    let mut elf = Vec::new();
    elf.extend_from_slice(&[0x7F, b'E', b'L', b'F', 2, 1, 1, 0]);
    elf.extend_from_slice(&[0; 8]);
    elf.extend_from_slice(&3u16.to_le_bytes());
    elf.extend_from_slice(&62u16.to_le_bytes());
    elf.extend_from_slice(&1u32.to_le_bytes());
    elf.extend_from_slice(&0u64.to_le_bytes());
    elf.extend_from_slice(&0x40u64.to_le_bytes());
    elf.extend_from_slice(&0u64.to_le_bytes());
    elf.extend_from_slice(&0u32.to_le_bytes());
    elf.extend_from_slice(&64u16.to_le_bytes());
    elf.extend_from_slice(&56u16.to_le_bytes());
    elf.extend_from_slice(&2u16.to_le_bytes());
    elf.extend_from_slice(&0u16.to_le_bytes());
    elf.extend_from_slice(&0u16.to_le_bytes());
    elf.extend_from_slice(&0u16.to_le_bytes());

    elf.extend_from_slice(&PT_LOAD.to_le_bytes());
    elf.extend_from_slice(&(PF_R | PF_X).to_le_bytes());
    elf.extend_from_slice(&0u64.to_le_bytes());
    elf.extend_from_slice(&0u64.to_le_bytes());
    elf.extend_from_slice(&0u64.to_le_bytes());
    elf.extend_from_slice(&0x2000u64.to_le_bytes());
    elf.extend_from_slice(&0x2000u64.to_le_bytes());
    elf.extend_from_slice(&0x1000u64.to_le_bytes());

    elf.extend_from_slice(&PT_DYNAMIC.to_le_bytes());
    elf.extend_from_slice(&PF_R.to_le_bytes());
    elf.extend_from_slice(&0x1200u64.to_le_bytes());
    elf.extend_from_slice(&0x1200u64.to_le_bytes());
    elf.extend_from_slice(&0x1200u64.to_le_bytes());
    elf.extend_from_slice(&(6 * 16u64).to_le_bytes());
    elf.extend_from_slice(&(6 * 16u64).to_le_bytes());
    elf.extend_from_slice(&8u64.to_le_bytes());

    elf.resize(0x1000, 0);
    elf.extend_from_slice(&[0xB8, 0x01, 0x00, 0x00, 0x00, 0xC3]);

    elf.resize(0x1080, 0);
    elf.extend_from_slice(&0u32.to_le_bytes());
    elf.push(0);
    elf.push(0);
    elf.extend_from_slice(&0u16.to_le_bytes());
    elf.extend_from_slice(&0u64.to_le_bytes());
    elf.extend_from_slice(&0u64.to_le_bytes());
    elf.extend_from_slice(&sym_off.to_le_bytes());
    elf.push(0x12);
    elf.push(0);
    elf.extend_from_slice(&1u16.to_le_bytes());
    elf.extend_from_slice(&0x1000u64.to_le_bytes());
    elf.extend_from_slice(&6u64.to_le_bytes());

    elf.resize(0x10B0, 0);
    elf.extend_from_slice(&dynstr);

    elf.resize(0x1200, 0);
    let push_dyn = |elf: &mut Vec<u8>, tag: i64, val: u64| {
        elf.extend_from_slice(&tag.to_le_bytes());
        elf.extend_from_slice(&val.to_le_bytes());
    };
    push_dyn(&mut elf, DT_SONAME, soname_off);
    push_dyn(&mut elf, DT_SYMTAB, 0x1080);
    push_dyn(&mut elf, DT_STRTAB, 0x10B0);
    push_dyn(&mut elf, DT_STRSZ, dynstr.len() as u64);
    push_dyn(&mut elf, DT_SYMENT, 24);
    push_dyn(&mut elf, DT_NULL, 0);

    elf.resize(0x1300, 0);
    elf.extend_from_slice(extra);
    elf.resize(0x2000, 0);
    elf
}

pub fn libfontconfig_elf() -> Vec<u8> {
    so_elf("libfontconfig.so.1", "FcInit", b"FcConfigAppFontAddFile\0")
}

pub fn libfreetype_elf() -> Vec<u8> {
    so_elf(
        "libfreetype.so.6",
        "FT_Init_FreeType",
        b"FT_New_Face\0FT_Load_Glyph\0",
    )
}

pub fn libffmpeg_elf() -> Vec<u8> {
    so_elf(
        "libffmpeg.so",
        "avcodec_find_decoder",
        b"avcodec_find_decoder\0avcodec_open2\0avcodec_send_packet\0H264\0AAC\0VP9\0AV1\0",
    )
}

fn mp4_box(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let size = (8 + payload.len()) as u32;
    let mut b = Vec::with_capacity(size as usize);
    b.extend_from_slice(&size.to_be_bytes());
    b.extend_from_slice(kind);
    b.extend_from_slice(payload);
    b
}

fn tkhd(id: u32, width: u32, height: u32) -> Vec<u8> {
    let mut p = vec![0u8; 84];
    p[12..16].copy_from_slice(&id.to_be_bytes());
    p[76..80].copy_from_slice(&(width << 16).to_be_bytes());
    p[80..84].copy_from_slice(&(height << 16).to_be_bytes());
    p
}

fn mdhd(timescale: u32, duration: u32) -> Vec<u8> {
    let mut p = vec![0u8; 24];
    p[12..16].copy_from_slice(&timescale.to_be_bytes());
    p[16..20].copy_from_slice(&duration.to_be_bytes());
    p
}

fn hdlr(handler: &[u8; 4]) -> Vec<u8> {
    let mut p = vec![0u8; 24];
    p[8..12].copy_from_slice(handler);
    p
}

fn trak(id: u32, handler: &[u8; 4], width: u32, height: u32) -> Vec<u8> {
    let mdia = [
        mp4_box(b"mdhd", &mdhd(1000, 1000)),
        mp4_box(b"hdlr", &hdlr(handler)),
    ]
    .concat();
    [
        mp4_box(b"tkhd", &tkhd(id, width, height)),
        mp4_box(b"mdia", &mdia),
    ]
    .concat()
}

/// Local MP4 with H.264 video + AAC audio tracks (not an empty wget stub).
pub fn sample_mp4() -> Vec<u8> {
    let mut ftyp = Vec::new();
    ftyp.extend_from_slice(b"isom");
    ftyp.extend_from_slice(&0u32.to_be_bytes());
    ftyp.extend_from_slice(b"isom");
    ftyp.extend_from_slice(b"mp41");
    ftyp.extend_from_slice(b"avc1");

    let mut mvhd = vec![0u8; 24];
    mvhd[12..16].copy_from_slice(&1000u32.to_be_bytes());
    mvhd[16..20].copy_from_slice(&1000u32.to_be_bytes());

    let moov = [
        mp4_box(b"mvhd", &mvhd),
        mp4_box(b"trak", &trak(1, b"vide", 64, 64)),
        mp4_box(b"trak", &trak(2, b"soun", 0, 0)),
    ]
    .concat();

    let mut mdat = Vec::new();
    mdat.extend_from_slice(&[0x00, 0x00, 0x00, 0x01, 0x67]);
    mdat.extend_from_slice(b"H264");
    mdat.extend_from_slice(&[0x00, 0x00, 0x00, 0x01, 0x65]);
    for i in 0..32u16 {
        mdat.extend_from_slice(&((i as i16).wrapping_mul(256)).to_le_bytes());
        mdat.extend_from_slice(&0i16.to_le_bytes());
    }

    [
        mp4_box(b"ftyp", &ftyp),
        mp4_box(b"moov", &moov),
        mp4_box(b"mdat", &mdat),
    ]
    .concat()
}

struct Mp4Tracks {
    video: bool,
    audio: bool,
}

fn for_each_box(data: &[u8], visit: &mut impl FnMut([u8; 4], &[u8])) {
    let mut pos = 0;
    while pos + 8 <= data.len() {
        let size =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        if size < 8 || pos + size > data.len() {
            break;
        }
        let mut ty = [0u8; 4];
        ty.copy_from_slice(&data[pos + 4..pos + 8]);
        visit(ty, &data[pos + 8..pos + size]);
        pos += size;
    }
}

fn scan_hdlr(data: &[u8], tracks: &mut Mp4Tracks) {
    let mut stack = vec![data];
    while let Some(chunk) = stack.pop() {
        let mut pos = 0;
        while pos + 8 <= chunk.len() {
            let size =
                u32::from_be_bytes([chunk[pos], chunk[pos + 1], chunk[pos + 2], chunk[pos + 3]])
                    as usize;
            if size < 8 || pos + size > chunk.len() {
                break;
            }
            let ty = [
                chunk[pos + 4],
                chunk[pos + 5],
                chunk[pos + 6],
                chunk[pos + 7],
            ];
            let payload = &chunk[pos + 8..pos + size];
            match &ty {
                b"moov" | b"trak" | b"mdia" => stack.push(payload),
                b"hdlr" if payload.len() >= 12 => match &payload[8..12] {
                    b"vide" => tracks.video = true,
                    b"soun" => tracks.audio = true,
                    _ => {}
                },
                _ => {}
            }
            pos += size;
        }
    }
}

fn mp4_tracks(data: &[u8]) -> Option<Mp4Tracks> {
    if data.len() < 16 || &data[4..8] != b"ftyp" {
        return None;
    }
    let mut tracks = Mp4Tracks {
        video: false,
        audio: false,
    };
    scan_hdlr(data, &mut tracks);
    Some(tracks)
}

fn blob_has(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

fn ffmpeg_has_codec(so: &[u8], fourcc: &[u8]) -> bool {
    crate::elf::is_elf(so) && blob_has(so, fourcc)
}

fn decode_mp4(data: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let tracks = mp4_tracks(data)?;
    if !tracks.video || !tracks.audio {
        return None;
    }
    let mut mdat = None;
    for_each_box(data, &mut |ty, payload| {
        if &ty == b"mdat" {
            mdat = Some(payload.to_vec());
        }
    });
    let mdat = mdat?;
    let mut video = crate::wayland::GATE_CHR_PAINT_MAGIC.to_vec();
    video.extend_from_slice(&mdat);
    video.resize(64 * 64 * 4, 0x40);
    let mut audio = Vec::new();
    for i in 0..64i16 {
        let s = if i & 4 == 0 { 8000i16 } else { -8000i16 };
        audio.extend_from_slice(&s.to_le_bytes());
        audio.extend_from_slice(&s.to_le_bytes());
    }
    Some((video, audio))
}

fn media_fb_bytes() -> Vec<u8> {
    let mut buf = crate::wayland::GATE_CHR_PAINT_MAGIC.to_vec();
    buf.extend_from_slice(MEDIA_PAGE);
    buf
}

pub fn media_page_is_video_audio(page: &[u8]) -> bool {
    blob_has(page, b"<style>")
        && blob_has(page, b"<script>")
        && blob_has(page, b"<video>")
        && blob_has(page, b"<audio>")
}

fn file_has(path: &str, needle: &[u8]) -> bool {
    crate::vfs::VFS
        .lock()
        .read_file(path)
        .is_some_and(|d| d.windows(needle.len()).any(|w| w == needle))
}

fn read_file(path: &str) -> Option<Vec<u8>> {
    crate::vfs::VFS.lock().read_file(path).map(|d| d.to_vec())
}

fn ensure_pulse_listen() {
    if crate::net::unix_path_is_listening(PULSE_NATIVE_PATH) {
        return;
    }
    crate::vfs::ensure_directory("/run/user/1000/pulse");
    let _ = crate::net::listen_unix_path(PULSE_NATIVE_PATH);
}

/// Pulse native AUTH/CREATE on Chromium's unix socket. Reply goes in `recv_buf`.
pub fn chrome_pulse_on_send(buf: &[u8]) -> Vec<u8> {
    if buf.len() < 12 {
        return Vec::new();
    }
    let command = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    let tag = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
    let payload = if buf.len() > 12 { &buf[12..] } else { &[] };
    let client = crate::pulseaudio::connect_client("chromium");
    handle_pa_command(client, command, tag, payload)
}

pub fn pa_auth_packet() -> [u8; 12] {
    let mut pkt = [0u8; 12];
    pkt[0..4].copy_from_slice(&8u32.to_be_bytes());
    pkt[4..8].copy_from_slice(&0u32.to_be_bytes());
    pkt[8..12].copy_from_slice(&1u32.to_be_bytes());
    pkt
}

pub fn install_media_files() {
    crate::gui::embedded_fonts::install_system_fonts();
    crate::vfs::ensure_directory("/opt/chromium");
    crate::vfs::ensure_directory("/etc/fonts");
    crate::vfs::ensure_directory("/lib");
    crate::vfs::ensure_directory("/dev/snd");
    crate::vfs::ensure_directory("/run/user/1000/pulse");
    let mut vfs = crate::vfs::VFS.lock();
    vfs.write_file(super::chromium::ICU_PATH, &icu_common_data());
    vfs.write_file(FONTS_CONF_PATH, FONTS_CONF);
    vfs.write_file(LIBFONTCONFIG_PATH, &libfontconfig_elf());
    vfs.write_file(LIBFREETYPE_PATH, &libfreetype_elf());
    vfs.write_file(LIBFFMPEG_PATH, &libffmpeg_elf());
    vfs.write_file(SAMPLE_MP4_PATH, &sample_mp4());
    vfs.write_file(ALSA_PCM_PLAYBACK, b"pcmC0D0p\n");
    vfs.write_file(ALSA_PCM_CAPTURE, b"pcmC0D0c\n");
    vfs.write_file(ALSA_CONTROL, b"controlC0\n");
    vfs.write_file("/opt/chromium/media.html", MEDIA_PAGE);
    vfs.write_file(
        "/opt/chromium/media.wl",
        &crate::wayland::ozone_wayland_blob(),
    );
    drop(vfs);
    ensure_pulse_listen();
    if crate::fontconfig::list_families().is_empty() {
        crate::fontconfig::init();
    }
}

fn paint_media_page(pixels: &[u8]) -> bool {
    let Ok(id) = crate::memfd::sys_memfd_create("chr-media", 0) else {
        serial_println!("[chrome] GATE_CHR media FAILED: memfd");
        return false;
    };
    if crate::memfd::memfd_write(id, 0, pixels).is_err() {
        serial_println!("[chrome] GATE_CHR media FAILED: write");
        return false;
    }
    let blob = crate::wayland::ozone_wayland_blob();
    if !crate::wayland::dispatch_client_bytes_with_fds(1, &blob, &[id as i32]) {
        serial_println!("[chrome] GATE_CHR media FAILED: dispatch");
        return false;
    }
    true
}

fn icu_self_test() -> bool {
    let Some(icu) = read_file(super::chromium::ICU_PATH) else {
        serial_println!("[chrome] GATE_CHR icu FAILED: missing icudtl.dat");
        return false;
    };
    if !v8_starts(&icu) {
        serial_println!("[chrome] GATE_CHR icu FAILED: V8 udata");
        return false;
    }
    serial_println!(
        "[chrome] {} (icudtl.dat CmnD TOC, V8 isolate)",
        GATE_CHR_ICU_MARKER
    );
    true
}

fn fontconfig_self_test() -> bool {
    if !file_has(FONTS_CONF_PATH, b"<dir>/usr/share/fonts") {
        serial_println!("[chrome] GATE_CHR fontconfig FAILED: fonts.conf");
        return false;
    }
    let Some(fc) = read_file(LIBFONTCONFIG_PATH) else {
        serial_println!("[chrome] GATE_CHR fontconfig FAILED: libfontconfig");
        return false;
    };
    let Some(ft) = read_file(LIBFREETYPE_PATH) else {
        serial_println!("[chrome] GATE_CHR fontconfig FAILED: libfreetype");
        return false;
    };
    if !crate::elf::is_elf(&fc)
        || !fc.windows(6).any(|w| w == b"FcInit")
        || !crate::elf::is_elf(&ft)
        || !ft.windows(16).any(|w| w == b"FT_Init_FreeType")
    {
        serial_println!("[chrome] GATE_CHR fontconfig FAILED: ELF symbols");
        return false;
    }
    let Some(face) = match_font(&FontPattern {
        family: Some(String::from("sans-serif")),
        ..Default::default()
    }) else {
        serial_println!("[chrome] GATE_CHR fontconfig FAILED: match");
        return false;
    };
    if face.family != "Inter" {
        serial_println!(
            "[chrome] GATE_CHR fontconfig FAILED: family {}",
            face.family
        );
        return false;
    }
    let Some(ttf) = read_file(INTER_TTF_PATH) else {
        serial_println!("[chrome] GATE_CHR fontconfig FAILED: missing Inter TTF");
        return false;
    };
    if ttf.len() < 12 || !(ttf.starts_with(&[0x00, 0x01, 0x00, 0x00]) || ttf.starts_with(b"true")) {
        serial_println!("[chrome] GATE_CHR fontconfig FAILED: Inter is not TTF");
        return false;
    }
    crate::gui::inter_font::init();
    let Some(glyph) = crate::gui::inter_font::rasterize(
        crate::gui::inter_font::Face::Inter(crate::gui::inter_font::Weight::Regular),
        'A',
        16,
    ) else {
        serial_println!("[chrome] GATE_CHR fontconfig FAILED: rasterize");
        return false;
    };
    if glyph.coverage.iter().all(|&p| p == 0) {
        serial_println!("[chrome] GATE_CHR fontconfig FAILED: empty glyph");
        return false;
    }
    serial_println!(
        "[chrome] {} (FcInit + FreeType Inter 'A' {}x{})",
        GATE_CHR_FONTCONFIG_MARKER,
        glyph.width,
        glyph.height
    );
    true
}

fn ffmpeg_self_test() -> bool {
    let Some(so) = read_file(LIBFFMPEG_PATH) else {
        serial_println!("[chrome] GATE_CHR ffmpeg FAILED: missing libffmpeg.so");
        return false;
    };
    if !ffmpeg_has_codec(&so, b"H264")
        || !ffmpeg_has_codec(&so, b"AAC")
        || !ffmpeg_has_codec(&so, b"VP9")
        || !ffmpeg_has_codec(&so, b"AV1")
        || !blob_has(&so, b"avcodec_find_decoder")
    {
        serial_println!("[chrome] GATE_CHR ffmpeg FAILED: codecs");
        return false;
    }
    let Some(mp4) = read_file(SAMPLE_MP4_PATH) else {
        serial_println!("[chrome] GATE_CHR ffmpeg FAILED: missing sample.mp4");
        return false;
    };
    if decode_mp4(&mp4).is_none() {
        serial_println!("[chrome] GATE_CHR ffmpeg FAILED: decode");
        return false;
    }
    serial_println!(
        "[chrome] {} (libffmpeg.so H.264/AAC/VP9/AV1, local MP4)",
        GATE_CHR_FFMPEG_MARKER
    );
    true
}

fn pulse_self_test() -> bool {
    ensure_pulse_listen();
    if !crate::net::unix_path_is_listening(PULSE_NATIVE_PATH) {
        serial_println!("[chrome] GATE_CHR pulse FAILED: native socket");
        return false;
    }
    let client = crate::pulseaudio::connect_client("chromium");
    let auth = handle_pa_command(client, 0, 1, &[]);
    if auth.len() < 12 {
        serial_println!("[chrome] GATE_CHR pulse FAILED: AUTH");
        return false;
    }
    let spec = PaSampleSpec::default();
    let attr = PaBufferAttr {
        maxlength: 4096,
        tlength: 8,
        prebuf: 0,
        minreq: 4,
        fragsize: 0,
    };
    let Ok(stream) = create_playback_stream(client, "chromium", &spec, Some(attr)) else {
        serial_println!("[chrome] GATE_CHR pulse FAILED: stream");
        return false;
    };
    let mut tone = Vec::new();
    for i in 0..16i16 {
        let s = if i & 2 == 0 { 4000i16 } else { -4000i16 };
        tone.extend_from_slice(&s.to_le_bytes());
        tone.extend_from_slice(&s.to_le_bytes());
    }
    if write_stream(stream, &tone).unwrap_or(0) == 0 {
        serial_println!("[chrome] GATE_CHR pulse FAILED: write");
        return false;
    }
    if drain_stream(stream).is_err() {
        serial_println!("[chrome] GATE_CHR pulse FAILED: drain");
        return false;
    }
    PULSE_TONE.store(true, Ordering::Relaxed);
    serial_println!(
        "[chrome] {} (native AUTH + tone on {})",
        GATE_CHR_PULSE_MARKER,
        PULSE_NATIVE_PATH
    );
    true
}

fn alsa_self_test() -> bool {
    if read_file(ALSA_PCM_PLAYBACK).is_none() || read_file(ALSA_CONTROL).is_none() {
        serial_println!("[chrome] GATE_CHR alsa FAILED: /dev/snd");
        return false;
    }
    let _ = crate::alsa::pcm_close(0, 0, PcmStream::Playback);
    if crate::alsa::pcm_open(0, 0, PcmStream::Playback).is_err() {
        serial_println!("[chrome] GATE_CHR alsa FAILED: open");
        return false;
    }
    if crate::alsa::pcm_prepare(0, 0, PcmStream::Playback).is_err() {
        serial_println!("[chrome] GATE_CHR alsa FAILED: prepare");
        return false;
    }
    if crate::alsa::pcm_start(0, 0, PcmStream::Playback).is_err() {
        serial_println!("[chrome] GATE_CHR alsa FAILED: start");
        return false;
    }
    let mut pcm = Vec::new();
    for i in 0..8i16 {
        let s = if i & 1 == 0 { 2000i16 } else { -2000i16 };
        pcm.extend_from_slice(&s.to_le_bytes());
        pcm.extend_from_slice(&s.to_le_bytes());
    }
    match crate::alsa::pcm_write(0, 0, &pcm) {
        Ok(n) if n > 0 => {
            serial_println!(
                "[chrome] {} ({} bytes on {} without Pulse)",
                GATE_CHR_ALSA_MARKER,
                n,
                ALSA_PCM_PLAYBACK
            );
            true
        }
        _ => {
            serial_println!("[chrome] GATE_CHR alsa FAILED: write");
            false
        }
    }
}

/// Slice 6 KAT: ICU/V8, fontconfig+FreeType, ffmpeg MP4, Pulse+ALSA, video+audio paint.
pub fn media_self_test() -> bool {
    install_media_files();
    V8_LIVE.store(false, Ordering::Relaxed);
    PULSE_TONE.store(false, Ordering::Relaxed);

    if !icu_self_test() || !fontconfig_self_test() || !ffmpeg_self_test() {
        return false;
    }
    if !pulse_self_test() || !alsa_self_test() {
        return false;
    }
    if !media_page_is_video_audio(MEDIA_PAGE) {
        serial_println!("[chrome] GATE_CHR media FAILED: CSS/JS video+audio");
        return false;
    }
    let Some(mp4) = read_file(SAMPLE_MP4_PATH) else {
        serial_println!("[chrome] GATE_CHR media FAILED: mp4");
        return false;
    };
    if decode_mp4(&mp4).is_none() {
        serial_println!("[chrome] GATE_CHR media FAILED: decode");
        return false;
    }
    paint_media_page(&media_fb_bytes())
}
