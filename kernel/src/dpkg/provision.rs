use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

use crate::serial_println;

use super::control::DebControl;
use super::database::{DEB_DATABASE, DebState, InstalledDeb, VIRTUAL_PACKAGES};

// ═══════════════════════════════════════════════════════════════════════
// VIVALDI-SPECIFIC DEPENDENCY PROVISIONING
// ═══════════════════════════════════════════════════════════════════════

/// Vivaldi's known Debian dependencies (from vivaldi-stable_amd64.deb)
/// These are the libraries Vivaldi links against at runtime.
pub const VIVALDI_DEPENDENCIES: &[&str] = &[
    "libasound2",         // ALSA audio
    "libatk-bridge2.0-0", // ATK accessibility bridge
    "libatk1.0-0",        // ATK accessibility toolkit
    "libatspi2.0-0",      // AT-SPI accessibility
    "libc6",              // GNU C Library
    "libcairo2",          // 2D graphics library
    "libcups2",           // CUPS printing
    "libdbus-1-3",        // D-Bus IPC
    "libdrm2",            // DRM userspace library
    "libexpat1",          // XML parser
    "libgbm1",            // Generic Buffer Management
    "libgcc-s1",          // GCC runtime
    "libglib2.0-0",       // GLib utilities
    "libgtk-3-0",         // GTK+ 3
    "libnspr4",           // Netscape Portable Runtime
    "libnss3",            // Network Security Services
    "libpango-1.0-0",     // Text rendering
    "libstdc++6",         // C++ standard library
    "libx11-6",           // X11 client library
    "libxcb1",            // X11 C bindings
    "libxcomposite1",     // X11 composite extension
    "libxdamage1",        // X11 damage extension
    "libxext6",           // X11 extensions
    "libxfixes3",         // X11 fixes extension
    "libxkbcommon0",      // XKB common library
    "libxrandr2",         // X11 RandR extension
    "wget",               // Download utility (or curl)
    "xdg-utils",          // XDG desktop utilities
];

/// Pre-provision all Vivaldi dependencies as virtual packages
/// This registers KnoxOS equivalents for all required libraries
pub fn provision_vivaldi_dependencies() {
    serial_println!("[dpkg] Provisioning Vivaldi browser dependencies...");

    let mut db = DEB_DATABASE.lock();
    let mut virtuals = VIRTUAL_PACKAGES.lock();

    // System libraries provided by KnoxOS kernel
    let knoxos_provided = [
        // C/C++ runtime (provided by musl/libc_funcs compat layer)
        (
            "libc6",
            "2.36-9",
            "GNU C Library (KnoxOS musl compat)",
            &["libc6-compat"] as &[&str],
        ),
        (
            "libgcc-s1",
            "13.2.0-7",
            "GCC runtime (KnoxOS built-in)",
            &[],
        ),
        (
            "libstdc++6",
            "13.2.0-7",
            "C++ standard library (KnoxOS built-in)",
            &[],
        ),
        // Audio (provided by alsa.rs)
        (
            "libasound2",
            "1.2.8-1",
            "ALSA audio (KnoxOS ALSA subsystem)",
            &["libasound2-data"],
        ),
        // Graphics (provided by drm.rs + gpu.rs + wayland.rs)
        (
            "libdrm2",
            "2.4.115-1",
            "DRM library (KnoxOS DRM subsystem)",
            &["libdrm-common"],
        ),
        (
            "libgbm1",
            "23.3.3-1",
            "GBM buffer management (KnoxOS GPU)",
            &[],
        ),
        // D-Bus (provided by dbus.rs)
        ("libdbus-1-3", "1.14.10-1", "D-Bus IPC (KnoxOS D-Bus)", &[]),
        // XML (provided by kernel)
        ("libexpat1", "2.5.0-2", "XML parser (KnoxOS built-in)", &[]),
        // X11 / display (provided by wayland.rs + Xwayland compat)
        (
            "libx11-6",
            "1.8.7-1",
            "X11 client (KnoxOS X11 compat)",
            &["libx11-data"],
        ),
        (
            "libxcb1",
            "1.15-1",
            "XCB protocol (KnoxOS Wayland→X11)",
            &[],
        ),
        (
            "libxcomposite1",
            "0.4.6-1",
            "X composite (KnoxOS compositor)",
            &[],
        ),
        (
            "libxdamage1",
            "1.1.6-1",
            "X damage (KnoxOS compositor)",
            &[],
        ),
        (
            "libxext6",
            "1.3.5-1",
            "X extensions (KnoxOS X11 compat)",
            &[],
        ),
        ("libxfixes3", "6.0.1-1", "X fixes (KnoxOS X11 compat)", &[]),
        ("libxrandr2", "1.5.3-1", "X RandR (KnoxOS multimon)", &[]),
        ("libxkbcommon0", "1.6.0-1", "XKB common (KnoxOS input)", &[]),
        // Accessibility
        (
            "libatk1.0-0",
            "2.50.0-1",
            "ATK accessibility (KnoxOS a11y)",
            &[],
        ),
        (
            "libatk-bridge2.0-0",
            "2.50.0-1",
            "ATK bridge (KnoxOS a11y)",
            &[],
        ),
        ("libatspi2.0-0", "2.50.0-1", "AT-SPI (KnoxOS a11y)", &[]),
        // Text/graphics rendering
        (
            "libpango-1.0-0",
            "1.50.14-1",
            "Pango text rendering (KnoxOS fonts)",
            &[],
        ),
        (
            "libcairo2",
            "1.18.0-1",
            "Cairo 2D graphics (KnoxOS rendering)",
            &[],
        ),
        // GTK+
        ("libglib2.0-0", "2.78.3-1", "GLib (KnoxOS GLib compat)", &[]),
        ("libgtk-3-0", "3.24.38-4", "GTK+ 3 (KnoxOS GTK compat)", &[]),
        // NSS/NSPR (crypto)
        ("libnss3", "3.94-1", "NSS crypto (KnoxOS TLS/crypto)", &[]),
        (
            "libnspr4",
            "4.35-1.1",
            "NSPR runtime (KnoxOS NSPR compat)",
            &[],
        ),
        // Printing
        (
            "libcups2",
            "2.4.7-1",
            "CUPS printing (KnoxOS print stub)",
            &[],
        ),
        // Utilities
        (
            "wget",
            "1.21.4-1",
            "Download utility (KnoxOS HTTP client)",
            &[],
        ),
        (
            "xdg-utils",
            "1.1.3-4.1",
            "XDG desktop utils (KnoxOS XDG)",
            &[],
        ),
    ];

    for (name, version, description, provides_list) in &knoxos_provided {
        let record = InstalledDeb {
            control: DebControl {
                package: name.to_string(),
                version: version.to_string(),
                architecture: String::from("amd64"),
                maintainer: String::from("KnoxOS System"),
                installed_size: 0,
                depends: Vec::new(),
                pre_depends: Vec::new(),
                recommends: Vec::new(),
                suggests: Vec::new(),
                conflicts: Vec::new(),
                replaces: Vec::new(),
                provides: provides_list.iter().map(|s| s.to_string()).collect(),
                section: String::from("libs"),
                priority: String::from("required"),
                homepage: String::from("https://knoxos.dev"),
                description: description.to_string(),
                extra_fields: BTreeMap::new(),
            },
            installed_files: Vec::new(),
            config_files: Vec::new(),
            state: DebState::Installed,
            install_time: 0,
        };

        db.insert(name.to_string(), record);

        // Register virtual/provided packages
        for virt in *provides_list {
            virtuals
                .entry(virt.to_string())
                .or_default()
                .push(name.to_string());
        }
    }

    serial_println!(
        "[dpkg] Provisioned {} virtual packages for Vivaldi compatibility",
        knoxos_provided.len()
    );
}
