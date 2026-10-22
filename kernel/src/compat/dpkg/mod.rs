/// DPKG — Debian Package Format (.deb) Parser and Installer
///
/// Implements the Debian binary package format for KnoxOS:
///   - `ar` archive parsing (outer container)
///   - `debian-binary` version validation (2.0)
///   - `control.tar` extraction (package metadata, maintainer scripts)
///   - `data.tar` extraction (actual file payload)
///   - Dependency resolution against KPM database
///   - Pre/post install/remove script execution
///   - File conflict detection
///   - Package database integration (dpkg status)
///
/// Vivaldi is distributed as `vivaldi-stable_amd64.deb`, which is a
/// standard Debian binary package containing a Chromium-based browser.
///
/// .deb format:
///   ar archive containing:
///     1. debian-binary   (text: "2.0\n")
///     2. control.tar.xz  (metadata: control, md5sums, postinst, etc.)
///     3. data.tar.xz     (actual files: /opt/vivaldi/*, /usr/bin/vivaldi, etc.)
///
/// Split into submodules for maintainability:
///   ar         — ar archive parsing
///   tar        — POSIX/UStar tarball extraction
///   compress   — gzip/xz/zstd decompression
///   control    — debian control file and dependency parsing
///   database   — installed-package records and ownership maps
///   install    — .deb installation engine
///   provision  — KnoxOS virtual packages for Vivaldi
///   query      — status, listing, inspect, and remove APIs
use alloc::vec::Vec;

use crate::serial_println;

mod ar;
mod compress;
mod control;
mod database;
mod install;
mod provision;
mod query;
mod tar;

pub use ar::*;
pub use compress::*;
pub use control::*;
pub use database::*;
pub use install::*;
pub use provision::*;
pub use query::*;
pub use tar::*;

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Initialize the dpkg subsystem
pub fn init() {
    serial_println!("[dpkg] Debian package manager initialized");
    serial_println!("[dpkg] Supported formats: .deb (ar + tar.gz/xz/zst)");

    // Pre-provision core library packages that KnoxOS provides natively
    provision_vivaldi_dependencies();

    serial_println!(
        "[dpkg] {} packages in database",
        database::DEB_DATABASE.lock().len()
    );
    let _ = deb_self_test();
}

pub const GATE_DPKG_MARKER: &str = "GATE_DPKG deb";
const GATE_DPKG_PAYLOAD: &[u8] = b"knoxos-dpkg\n";

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

fn tar_file(name: &str, payload: &[u8]) -> Vec<u8> {
    let mut h = [0u8; 512];
    let nb = name.as_bytes();
    h[..nb.len().min(100)].copy_from_slice(&nb[..nb.len().min(100)]);
    h[100..108].copy_from_slice(b"0000644\0");
    let size = alloc::format!("{:011o}", payload.len());
    h[124..135].copy_from_slice(size.as_bytes());
    h[156] = b'0';
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
    out.extend_from_slice(&[0u8; 1024]);
    out
}

fn build_gate_deb() -> Vec<u8> {
    let control =
        b"Package: knoxos-gate\nVersion: 1.0\nArchitecture: all\nDescription: DPKG gate\n";
    let control_tar = tar_file("control", control);
    let data_tar = tar_file("./GATE.TXT", GATE_DPKG_PAYLOAD);
    let mut deb = Vec::from(&b"!<arch>\n"[..]);
    deb.extend_from_slice(&ar_member("debian-binary", b"2.0\n"));
    deb.extend_from_slice(&ar_member("control.tar", &control_tar));
    deb.extend_from_slice(&ar_member("data.tar", &data_tar));
    deb
}

fn deb_self_test() -> bool {
    match install::install_deb(&build_gate_deb()) {
        Ok(ctrl) if ctrl.package == "knoxos-gate" => {}
        Ok(ctrl) => {
            serial_println!("[dpkg] GATE_DPKG FAILED package={}", ctrl.package);
            return false;
        }
        Err(e) => {
            serial_println!("[dpkg] GATE_DPKG FAILED install {:?}", e);
            return false;
        }
    }
    let got = crate::vfs::read_file_dispatch("/GATE.TXT");
    let in_db = database::DEB_DATABASE.lock().contains_key("knoxos-gate");
    if got.as_deref() == Some(GATE_DPKG_PAYLOAD) && in_db {
        serial_println!(
            "[dpkg] {} (ar+tar install {})",
            GATE_DPKG_MARKER,
            GATE_DPKG_PAYLOAD.len()
        );
        true
    } else {
        serial_println!(
            "[dpkg] GATE_DPKG FAILED vfs={} db={}",
            got.as_ref().map(|d| d.len()).unwrap_or(0),
            in_db
        );
        false
    }
}
