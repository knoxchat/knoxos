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
}
