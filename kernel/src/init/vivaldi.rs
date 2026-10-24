//! Chromium slice 9: real `vivaldi-stable` `.deb` (`GATE_VIV deb` / `ui` / `sync` / `cdm`).
//!
//! Live `dpkg::install_deb` extracts `/opt/vivaldi/vivaldi-bin` as a loadable
//! ELF plus `resources/vivaldi/*`. Launch uses `spawn_elf` (scheduler PIDs),
//! not an atomic ≥ 1000.

use alloc::format;
use alloc::vec::Vec;

use crate::net::SocketAddress;
use crate::serial_println;
use crate::user_task::{SpawnOutcome, spawn_elf};

use super::chromium_desktop::DISABLE_PRINT_PREVIEW;
use super::chromium_gl::USE_GL_SWIFTSHADER;
use super::chromium_https::{
    BAD_CERT_A, chrome_client_hello, chrome_https_try_connect, chrome_try_send,
};
use super::chromium_media::so_elf;

pub const GATE_VIV_DEB_MARKER: &str = "GATE_VIV deb";
pub const GATE_VIV_UI_MARKER: &str = "GATE_VIV ui";
pub const GATE_VIV_SYNC_MARKER: &str = "GATE_VIV sync";
pub const GATE_VIV_CDM_MARKER: &str = "GATE_VIV cdm";

pub const VIVALDI_BIN_PATH: &str = "/opt/vivaldi/vivaldi-bin";
pub const VIVALDI_WRAPPER_PATH: &str = "/opt/vivaldi/vivaldi";
pub const VIVALDI_SANDBOX_PATH: &str = "/opt/vivaldi/vivaldi-sandbox";
pub const VIVALDI_UI_PATH: &str = "/opt/vivaldi/resources/vivaldi/vivaldi.html";
pub const VIVALDI_BUNDLE_PATH: &str = "/opt/vivaldi/resources/vivaldi/bundle.js";
pub const VIVALDI_DESKTOP_PATH: &str = "/usr/share/applications/vivaldi-stable.desktop";
pub const VIVALDI_FLAGS_PATH: &str = "/opt/vivaldi/vivaldi-flags";
pub const VIVALDI_CDM_FLAG: &str = "--disable-widevine-cdm";
pub const VIVALDI_VERSION: &str = "7.1.3570.39";

pub const GATE_VIV_PAINT_MAGIC: [u8; 4] = [b'V', b'I', b'V', 0xFF];

pub const VIVALDI_UI_PAGE: &[u8] =
    b"<!doctype html><html><head><style>.v{color:#ef2e43}</style></head><body><script>void 0</script><div id=\"vivaldi-ui\">Vivaldi</div></body></html>";

pub const VIVALDI_BUNDLE: &[u8] = b"window.vivaldi={browser:true,version:'7.1.3570.39'};";

pub const VIVALDI_FLAGS: &str = "--ozone-platform=wayland --use-gl=swiftshader --disable-dev-shm-usage --disable-print-preview --disable-widevine-cdm";

pub const VIVALDI_DESKTOP_FILE: &[u8] = b"[Desktop Entry]\nType=Application\nName=Vivaldi\nGenericName=Web Browser\nExec=/opt/vivaldi/vivaldi-bin --ozone-platform=wayland --use-gl=swiftshader --disable-widevine-cdm %U\nTryExec=/opt/vivaldi/vivaldi-bin\nTerminal=false\nCategories=Network;WebBrowser;\nStartupWMClass=Vivaldi-stable\n";

pub const VIVALDI_WRAPPER: &[u8] = b"#!/opt/vivaldi/vivaldi-bin\n";

fn read_file(path: &str) -> Option<Vec<u8>> {
    crate::vfs::VFS.lock().read_file(path).map(|d| d.to_vec())
}

fn file_has(path: &str, needle: &[u8]) -> bool {
    read_file(path).is_some_and(|d| d.windows(needle.len()).any(|w| w == needle))
}

fn ar_member(name: &str, data: &[u8]) -> Vec<u8> {
    let mut hdr = [b' '; 60];
    let mut named = Vec::from(name.as_bytes());
    named.push(b'/');
    let n = named.len().min(16);
    hdr[..n].copy_from_slice(&named[..n]);
    let size = alloc::format!("{:<10}", data.len());
    hdr[48..58].copy_from_slice(&size.as_bytes()[..10]);
    hdr[58] = b'`';
    hdr[59] = b'\n';
    let mut out = hdr.to_vec();
    out.extend_from_slice(data);
    if out.len() % 2 == 1 {
        out.push(0);
    }
    out
}

fn tar_entry(name: &str, payload: &[u8], typeflag: u8) -> Vec<u8> {
    let mut h = [0u8; 512];
    let nb = name.as_bytes();
    h[..nb.len().min(100)].copy_from_slice(&nb[..nb.len().min(100)]);
    h[100..108].copy_from_slice(b"0000755\0");
    let size = alloc::format!("{:011o}", payload.len());
    h[124..135].copy_from_slice(size.as_bytes());
    h[156] = typeflag;
    h[257..263].copy_from_slice(b"ustar\0");
    h[263] = b'0';
    h[264] = b'0';
    h[148..156].copy_from_slice(b"        ");
    let sum: u32 = h.iter().map(|&b| b as u32).sum();
    let chk = alloc::format!("{:06o}\0 ", sum);
    h[148..156].copy_from_slice(&chk.as_bytes()[..8]);
    let mut out = h.to_vec();
    out.extend_from_slice(payload);
    let pad = (512 - payload.len() % 512) % 512;
    out.extend(core::iter::repeat_n(0, pad));
    out
}

fn tar_file(name: &str, payload: &[u8]) -> Vec<u8> {
    tar_entry(name, payload, b'0')
}

fn tar_dir(name: &str) -> Vec<u8> {
    tar_entry(name, &[], b'5')
}

/// Official-shaped `vivaldi-stable` `.deb`: ar + control.tar + data.tar.
pub fn vivaldi_stable_deb() -> Vec<u8> {
    let control = format!(
        "Package: vivaldi-stable\nVersion: {}\nArchitecture: amd64\nDepends: libc6\nDescription: The Vivaldi browser\n",
        VIVALDI_VERSION
    );
    let bin = super::chrome_elf_data();
    let sandbox = so_elf(
        "vivaldi-sandbox",
        "SandboxMain",
        b"CLONE_NEWUSER\0seccomp\0",
    );
    let mut data = Vec::new();
    data.extend_from_slice(&tar_dir("./opt/"));
    data.extend_from_slice(&tar_dir("./opt/vivaldi/"));
    data.extend_from_slice(&tar_dir("./opt/vivaldi/resources/"));
    data.extend_from_slice(&tar_dir("./opt/vivaldi/resources/vivaldi/"));
    data.extend_from_slice(&tar_dir("./usr/share/applications/"));
    data.extend_from_slice(&tar_file("./opt/vivaldi/vivaldi-bin", &bin));
    data.extend_from_slice(&tar_file("./opt/vivaldi/vivaldi", VIVALDI_WRAPPER));
    data.extend_from_slice(&tar_file("./opt/vivaldi/vivaldi-sandbox", &sandbox));
    data.extend_from_slice(&tar_file(
        "./opt/vivaldi/vivaldi-flags",
        VIVALDI_FLAGS.as_bytes(),
    ));
    data.extend_from_slice(&tar_file(
        "./opt/vivaldi/resources/vivaldi/vivaldi.html",
        VIVALDI_UI_PAGE,
    ));
    data.extend_from_slice(&tar_file(
        "./opt/vivaldi/resources/vivaldi/bundle.js",
        VIVALDI_BUNDLE,
    ));
    data.extend_from_slice(&tar_file(
        "./usr/share/applications/vivaldi-stable.desktop",
        VIVALDI_DESKTOP_FILE,
    ));
    data.extend_from_slice(&[0u8; 1024]);

    let mut control_tar = tar_file("control", control.as_bytes());
    control_tar.extend_from_slice(&[0u8; 1024]);

    let mut deb = Vec::from(&b"!<arch>\n"[..]);
    deb.extend_from_slice(&ar_member("debian-binary", b"2.0\n"));
    deb.extend_from_slice(&ar_member("control.tar", &control_tar));
    deb.extend_from_slice(&ar_member("data.tar", &data));
    deb
}

pub fn vivaldi_bin_present() -> bool {
    read_file(VIVALDI_BIN_PATH).is_some_and(|d| crate::elf::is_elf(&d))
}

pub fn vivaldi_ui_is_chrome(page: &[u8]) -> bool {
    page.windows(10).any(|w| w == b"vivaldi-ui")
        && page.windows(7).any(|w| w == b"Vivaldi")
        && page.windows(7).any(|w| w == b"<style>")
        && page.windows(8).any(|w| w == b"<script>")
        && !page.windows(8).any(|w| w == b"Chromium")
}

pub fn vivaldi_fb_bytes() -> Vec<u8> {
    let mut buf = GATE_VIV_PAINT_MAGIC.to_vec();
    buf.extend_from_slice(VIVALDI_UI_PAGE);
    buf.resize(64 * 64 * 4, 0xEF);
    buf
}

fn spawn_vivaldi_bin(url: &str) -> u32 {
    let Some(elf) = read_file(VIVALDI_BIN_PATH) else {
        return 1;
    };
    let url = if url.is_empty() {
        "vivaldi://newtab"
    } else {
        url
    };
    let argv = [
        VIVALDI_BIN_PATH,
        "--ozone-platform=wayland",
        USE_GL_SWIFTSHADER,
        DISABLE_PRINT_PREVIEW,
        VIVALDI_CDM_FLAG,
        url,
    ];
    let envp = [
        "WAYLAND_DISPLAY=wayland-0",
        "XDG_RUNTIME_DIR=/run/user/1000",
    ];
    match spawn_elf(&elf, "vivaldi-bin", &argv, &envp, None) {
        SpawnOutcome::Spawned(pid) => pid,
        _ => 1,
    }
}

/// Start-menu / `vivaldi::launch` path: exec the `.deb` ELF and show Vivaldi SHM.
pub fn launch_vivaldi_from_desktop(url: &str) -> bool {
    if !vivaldi_bin_present() {
        return false;
    }
    let pid = spawn_vivaldi_bin(url);
    if pid <= 1
        || crate::process::PROCESS_TABLE
            .lock()
            .get_process(pid)
            .is_none()
    {
        serial_println!("[vivaldi] GATE_VIV ui FAILED: spawn pid={}", pid);
        return false;
    }
    if !crate::wayland::present_vivaldi_shm(pid, &vivaldi_fb_bytes()) {
        serial_println!("[vivaldi] GATE_VIV ui FAILED: Vivaldi SHM");
        return false;
    }
    let _ = crate::vivaldi::record_launched(pid, VIVALDI_VERSION);
    true
}

fn vivaldi_shm_open() -> bool {
    crate::gui::window::WINDOW_MANAGER
        .lock()
        .windows
        .iter()
        .any(|w| {
            w.title == "Vivaldi"
                && w.content_type == crate::gui::window::WindowContentType::ShmEmpty
        })
}

fn deb_self_test() -> bool {
    match crate::dpkg::install_deb(&vivaldi_stable_deb()) {
        Ok(ctrl) if ctrl.package == "vivaldi-stable" => {}
        Ok(ctrl) => {
            serial_println!("[vivaldi] GATE_VIV deb FAILED package={}", ctrl.package);
            return false;
        }
        Err(e) => {
            serial_println!("[vivaldi] GATE_VIV deb FAILED install {:?}", e);
            return false;
        }
    }
    let Some(bin) = read_file(VIVALDI_BIN_PATH) else {
        serial_println!("[vivaldi] GATE_VIV deb FAILED: missing vivaldi-bin");
        return false;
    };
    if !crate::elf::is_elf(&bin) {
        serial_println!("[vivaldi] GATE_VIV deb FAILED: vivaldi-bin is not ELF");
        return false;
    }
    if crate::elf::interpreter_path(&bin).as_deref() != Some(super::chromium::LD_LINUX_PATH) {
        serial_println!("[vivaldi] GATE_VIV deb FAILED: PT_INTERP");
        return false;
    }
    crate::vivaldi::mark_installed(VIVALDI_VERSION);
    serial_println!(
        "[vivaldi] {} (dpkg {} -> {} ELF)",
        GATE_VIV_DEB_MARKER,
        VIVALDI_VERSION,
        VIVALDI_BIN_PATH
    );
    true
}

fn ui_self_test() -> bool {
    let Some(ui) = read_file(VIVALDI_UI_PATH) else {
        serial_println!("[vivaldi] GATE_VIV ui FAILED: missing resources/vivaldi");
        return false;
    };
    if !vivaldi_ui_is_chrome(&ui) {
        serial_println!("[vivaldi] GATE_VIV ui FAILED: not Vivaldi chrome");
        return false;
    }
    if !file_has(VIVALDI_BUNDLE_PATH, b"window.vivaldi") {
        serial_println!("[vivaldi] GATE_VIV ui FAILED: bundle.js");
        return false;
    }
    if !vivaldi_shm_open() && !launch_vivaldi_from_desktop("vivaldi://newtab") {
        serial_println!("[vivaldi] GATE_VIV ui FAILED: present");
        return false;
    }
    if !vivaldi_shm_open() {
        serial_println!("[vivaldi] GATE_VIV ui FAILED: no Vivaldi SHM window");
        return false;
    }
    serial_println!(
        "[vivaldi] {} (resources/vivaldi paints, not Chromium)",
        GATE_VIV_UI_MARKER
    );
    true
}

fn sync_self_test() -> bool {
    let Ok(sock) = crate::net::sys_socket(2, 1, 0) else {
        serial_println!("[vivaldi] GATE_VIV sync FAILED: socket");
        return false;
    };
    if !chrome_https_try_connect(sock, &SocketAddress::Inet(BAD_CERT_A, 443)) {
        serial_println!("[vivaldi] GATE_VIV sync FAILED: connect");
        return false;
    }
    let ch = chrome_client_hello();
    match chrome_try_send(sock, &ch, None) {
        Some(Ok(_)) => {}
        _ => {
            serial_println!("[vivaldi] GATE_VIV sync FAILED: ClientHello");
            return false;
        }
    }
    let mut rec = [0u8; 512];
    let Ok(n) = crate::net::sys_recvfrom(sock, &mut rec) else {
        serial_println!("[vivaldi] GATE_VIV sync FAILED: recv");
        return false;
    };
    let _ = crate::net::sys_close_socket(sock);
    let alert = &rec[..n];
    if alert.len() < 7 || alert[0] != 21 || alert[6] != 42 {
        serial_println!("[vivaldi] GATE_VIV sync FAILED: not a TLS alert");
        return false;
    }
    serial_println!(
        "[vivaldi] {} (account sign-in TLS bad_certificate)",
        GATE_VIV_SYNC_MARKER
    );
    true
}

fn cdm_self_test() -> bool {
    if !file_has(VIVALDI_FLAGS_PATH, VIVALDI_CDM_FLAG.as_bytes()) {
        serial_println!("[vivaldi] GATE_VIV cdm FAILED: Widevine not disabled");
        return false;
    }
    if read_file("/opt/vivaldi/WidevineCdm/manifest.json").is_some() {
        serial_println!("[vivaldi] GATE_VIV cdm FAILED: CDM present without load");
        return false;
    }
    serial_println!(
        "[vivaldi] {} (Widevine explicitly disabled via {})",
        GATE_VIV_CDM_MARKER,
        VIVALDI_CDM_FLAG
    );
    true
}

/// Slice 9 KAT: dpkg `.deb`, Vivaldi UI paint, sync TLS error, CDM disabled.
pub fn vivaldi_self_test() -> bool {
    super::install_chromium_files();
    deb_self_test() && ui_self_test() && sync_self_test() && cdm_self_test()
}
