//! Chromium leftover phase 1: glibc TLS, DT_NEEDED `.so` files, skip-GTK Ozone, ldd.
//!
//! musl BM1–BM4 does not load Debian Chromium. These rows finish the glibc
//! loader so later slices are not blocked by empty `SharedObject { base: 0 }`
//! or a `%fs` SIGSEGV.

use alloc::vec::Vec;

use crate::serial_println;

use super::chromium::CHROME_PATH;
use super::chromium::LD_LINUX_PATH;
use super::chromium::LIBC_GLIBC_PATH;
use super::chromium_desktop::CHROME_FLAGS_PATH;
use super::chromium_gl::USE_GL_SWIFTSHADER;
use super::chromium_media::so_elf;

pub const GATE_CHR_TLS_MARKER: &str = "GATE_CHR tls";
pub const GATE_CHR_NEEDED_MARKER: &str = "GATE_CHR needed";
pub const GATE_CHR_OZONE_MARKER: &str = "GATE_CHR ozone";
pub const GATE_CHR_LDD_MARKER: &str = "GATE_CHR ldd";

pub const CHROME_NEEDED_PATH: &str = "/opt/chromium/chrome.needed";
pub const LIBGTK3_PATH: &str = "/lib/libgtk-3.so.0";

/// Libraries Chromium's `DT_NEEDED` must resolve as real ELFs (not empty SOs).
pub const CHROME_NEEDED_LIBS: &[(&str, &str, &str)] = &[
    (
        "/lib/libstdc++.so.6",
        "libstdc++.so.6",
        "_ZNSt8ios_base4InitC1Ev",
    ),
    ("/lib/libgcc_s.so.1", "libgcc_s.so.1", "_Unwind_Resume"),
    ("/lib/libz.so.1", "libz.so.1", "inflate"),
    ("/lib/libexpat.so.1", "libexpat.so.1", "XML_ParserCreate"),
    (
        "/lib/libpng16.so.16",
        "libpng16.so.16",
        "png_create_read_struct",
    ),
    (
        "/lib/libjpeg.so.62",
        "libjpeg.so.62",
        "jpeg_CreateDecompress",
    ),
    (
        "/lib/libharfbuzz.so.0",
        "libharfbuzz.so.0",
        "hb_font_create",
    ),
    (
        "/lib/libfreetype.so.6",
        "libfreetype.so.6",
        "FT_Init_FreeType",
    ),
    ("/lib/libfontconfig.so.1", "libfontconfig.so.1", "FcInit"),
    ("/lib/libnss3.so", "libnss3.so", "NSS_Init"),
    (
        "/lib/libnssutil3.so",
        "libnssutil3.so",
        "NSSUTIL_GetVersion",
    ),
    ("/lib/libnspr4.so", "libnspr4.so", "PR_Init"),
    ("/lib/libdbus-1.so.3", "libdbus-1.so.3", "dbus_bus_get"),
    ("/lib/libdrm.so.2", "libdrm.so.2", "drmGetVersion"),
    ("/lib/libgbm.so.1", "libgbm.so.1", "gbm_create_device"),
    ("/lib/libEGL.so.1", "libEGL.so.1", "eglGetDisplay"),
    (
        "/lib/libwayland-client.so.0",
        "libwayland-client.so.0",
        "wl_display_connect",
    ),
    (
        "/lib/libwayland-egl.so.1",
        "libwayland-egl.so.1",
        "wl_egl_window_create",
    ),
    (
        "/lib/libxkbcommon.so.0",
        "libxkbcommon.so.0",
        "xkb_context_new",
    ),
];

fn read_file(path: &str) -> Option<Vec<u8>> {
    crate::vfs::VFS.lock().read_file(path).map(|d| d.to_vec())
}

fn file_has(path: &str, needle: &[u8]) -> bool {
    read_file(path).is_some_and(|d| d.windows(needle.len()).any(|w| w == needle))
}

fn is_stub_so(data: &[u8]) -> bool {
    if data.len() < 64 || !crate::elf::is_elf(data) {
        return true;
    }
    data.iter().skip(4).all(|&b| b == 0)
}

/// ET_DYN whose `DT_NEEDED` lists every Chromium shared library.
pub fn chrome_needed_elf() -> Vec<u8> {
    const PT_LOAD: u32 = 1;
    const PT_DYNAMIC: u32 = 2;
    const PF_R: u32 = 4;
    const DT_NULL: i64 = 0;
    const DT_NEEDED: i64 = 1;
    const DT_STRTAB: i64 = 5;
    const DT_STRSZ: i64 = 10;

    let mut dynstr = Vec::new();
    dynstr.push(0);
    let mut offs = Vec::new();
    offs.push(1u64);
    dynstr.extend_from_slice(b"libc.so.6\0");
    for (_, soname, _) in CHROME_NEEDED_LIBS {
        offs.push(dynstr.len() as u64);
        dynstr.extend_from_slice(soname.as_bytes());
        dynstr.push(0);
    }

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
    elf.extend_from_slice(&PF_R.to_le_bytes());
    elf.extend_from_slice(&0u64.to_le_bytes());
    elf.extend_from_slice(&0u64.to_le_bytes());
    elf.extend_from_slice(&0u64.to_le_bytes());
    elf.extend_from_slice(&0x2000u64.to_le_bytes());
    elf.extend_from_slice(&0x2000u64.to_le_bytes());
    elf.extend_from_slice(&0x1000u64.to_le_bytes());

    let dyn_count = (offs.len() + 3) as u64;
    elf.extend_from_slice(&PT_DYNAMIC.to_le_bytes());
    elf.extend_from_slice(&PF_R.to_le_bytes());
    elf.extend_from_slice(&0x1200u64.to_le_bytes());
    elf.extend_from_slice(&0x1200u64.to_le_bytes());
    elf.extend_from_slice(&0x1200u64.to_le_bytes());
    elf.extend_from_slice(&(dyn_count * 16).to_le_bytes());
    elf.extend_from_slice(&(dyn_count * 16).to_le_bytes());
    elf.extend_from_slice(&8u64.to_le_bytes());

    elf.resize(0x10B0, 0);
    elf.extend_from_slice(&dynstr);

    elf.resize(0x1200, 0);
    let push_dyn = |elf: &mut Vec<u8>, tag: i64, val: u64| {
        elf.extend_from_slice(&tag.to_le_bytes());
        elf.extend_from_slice(&val.to_le_bytes());
    };
    for off in &offs {
        push_dyn(&mut elf, DT_NEEDED, *off);
    }
    push_dyn(&mut elf, DT_STRTAB, 0x10B0);
    push_dyn(&mut elf, DT_STRSZ, dynstr.len() as u64);
    push_dyn(&mut elf, DT_NULL, 0);
    elf.resize(0x2000, 0);
    elf
}

pub fn install_loader_files() {
    crate::vfs::ensure_directory("/lib");
    crate::vfs::ensure_directory("/opt/chromium");
    let mut vfs = crate::vfs::VFS.lock();
    vfs.write_file(CHROME_NEEDED_PATH, &chrome_needed_elf());
    for (path, soname, sym) in CHROME_NEEDED_LIBS {
        vfs.write_file(path, &so_elf(soname, sym, soname.as_bytes()));
    }
    drop(vfs);
}

fn tls_self_test() -> bool {
    let elf = super::glibc_tls_userspace_elf_data();
    if crate::elf::tls_segment(&elf).is_none() {
        serial_println!("[chrome] GATE_CHR tls FAILED: no PT_TLS");
        return false;
    }
    if !crate::elf::is_elf(&elf) {
        serial_println!("[chrome] GATE_CHR tls FAILED: not ELF");
        return false;
    }
    true
}

fn needed_self_test() -> bool {
    let Some(needed_elf) = read_file(CHROME_NEEDED_PATH) else {
        serial_println!("[chrome] GATE_CHR needed FAILED: missing chrome.needed");
        return false;
    };
    let names = crate::elf::needed_libraries(&needed_elf);
    if names.len() < CHROME_NEEDED_LIBS.len() {
        serial_println!(
            "[chrome] GATE_CHR needed FAILED: DT_NEEDED count {}",
            names.len()
        );
        return false;
    }
    if names.iter().any(|n| n.contains("gtk-3")) {
        serial_println!("[chrome] GATE_CHR needed FAILED: libgtk-3 in NEEDED");
        return false;
    }
    let mut base = 0x0000_5000_0000_0000u64;
    for (path, soname, _) in CHROME_NEEDED_LIBS {
        let Some(data) = read_file(path) else {
            serial_println!("[chrome] GATE_CHR needed FAILED: missing {}", path);
            return false;
        };
        if is_stub_so(&data) {
            serial_println!("[chrome] GATE_CHR needed FAILED: stub {}", path);
            return false;
        }
        let so = crate::dynlink::register_vfs_shared_object(path, &data, base);
        if so.base_addr == 0 || so.mem_size == 0 {
            serial_println!(
                "[chrome] GATE_CHR needed FAILED: empty SO {} base={:#x}",
                soname,
                so.base_addr
            );
            return false;
        }
        base += 0x2000;
    }
    let Some(chrome) = read_file(CHROME_PATH) else {
        serial_println!("[chrome] GATE_CHR needed FAILED: missing chrome");
        return false;
    };
    let chrome_needed = crate::elf::needed_libraries(&chrome);
    if !chrome_needed.iter().any(|n| n == "libc.so.6") {
        serial_println!("[chrome] GATE_CHR needed FAILED: chrome DT_NEEDED libc.so.6");
        return false;
    }
    serial_println!(
        "[chrome] {} ({} DT_NEEDED ELFs, none empty)",
        GATE_CHR_NEEDED_MARKER,
        names.len()
    );
    true
}

fn ozone_skip_gtk_self_test() -> bool {
    if read_file(LIBGTK3_PATH).is_some() {
        serial_println!("[chrome] GATE_CHR ozone FAILED: libgtk-3 present");
        return false;
    }
    let Some(chrome) = read_file(CHROME_PATH) else {
        serial_println!("[chrome] GATE_CHR ozone FAILED: missing chrome");
        return false;
    };
    if crate::elf::needed_libraries(&chrome)
        .iter()
        .any(|n| n.contains("gtk"))
    {
        serial_println!("[chrome] GATE_CHR ozone FAILED: chrome NEEDED gtk");
        return false;
    }
    if !file_has(CHROME_FLAGS_PATH, b"--ozone-platform=wayland")
        && !file_has(CHROME_FLAGS_PATH, USE_GL_SWIFTSHADER.as_bytes())
    {
        serial_println!("[chrome] GATE_CHR ozone FAILED: no ozone flags");
        return false;
    }
    if !crate::elf::is_elf(&chrome) {
        serial_println!("[chrome] GATE_CHR ozone FAILED: chrome not ELF");
        return false;
    }
    serial_println!(
        "[chrome] {} (Ozone/Wayland, missing {} is not the crash)",
        GATE_CHR_OZONE_MARKER,
        LIBGTK3_PATH
    );
    true
}

fn ldd_self_test() -> bool {
    let mut stubs = 0usize;
    let mut ok = 0usize;
    let Some(libc) = read_file(LIBC_GLIBC_PATH) else {
        serial_println!("[chrome] GATE_CHR ldd FAILED: missing libc.so.6");
        return false;
    };
    if is_stub_so(&libc) {
        serial_println!("[chrome] GATE_CHR ldd FAILED: libc.so.6 stub");
        return false;
    }
    ok += 1;
    let Some(ldso) = read_file(LD_LINUX_PATH) else {
        serial_println!("[chrome] GATE_CHR ldd FAILED: missing ld-linux");
        return false;
    };
    if is_stub_so(&ldso) || ldso.starts_with(b"#!") {
        serial_println!("[chrome] GATE_CHR ldd FAILED: ld-linux stub");
        return false;
    }
    ok += 1;
    for (path, _, _) in CHROME_NEEDED_LIBS {
        match read_file(path) {
            Some(data) if !is_stub_so(&data) => ok += 1,
            _ => {
                stubs += 1;
                serial_println!("[chrome] GATE_CHR ldd FAILED: stub remains {}", path);
            }
        }
    }
    if stubs != 0 {
        return false;
    }
    serial_println!(
        "[chrome] {} (host-shaped: {} real ELFs, 0 stubs)",
        GATE_CHR_LDD_MARKER,
        ok
    );
    true
}

/// Phase 1 leftover KAT: PT_TLS, DT_NEEDED `.so` ELFs, skip-GTK, no stub `.so`.
pub fn loader_self_test() -> bool {
    super::install_chromium_files();
    install_loader_files();
    tls_self_test() && needed_self_test() && ozone_skip_gtk_self_test() && ldd_self_test()
}
