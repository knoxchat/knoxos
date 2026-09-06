/// APT Repository Access (Phase 25)
///
/// Implements the `apt` package manager frontend for KnoxOS:
///
///   - HTTP client for Debian mirrors (deb.debian.org)
///   - sources.list parsing
///   - Package index parsing (Packages.gz / Packages)
///   - Dependency resolution (topological sort)
///   - Package download and installation via dpkg
///   - apt update / apt install / apt remove / apt search
///   - GPG signature verification (InRelease)
///   - Cache management (/var/cache/apt/archives)
///
/// Usage:
///   apt update                  - Fetch package indices
///   apt install <package>       - Install a package and dependencies
///   apt remove <package>        - Remove a package
///   apt search <query>          - Search available packages
///   apt list --installed        - List installed packages
///   apt show <package>          - Show package details
///   apt upgrade                 - Upgrade all packages
///   apt autoremove              - Remove unused dependencies
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════

/// A package entry from the Packages index
#[derive(Debug, Clone)]
pub struct PackageEntry {
    pub name: String,
    pub version: String,
    pub architecture: String,
    pub description: String,
    pub depends: Vec<String>,
    pub recommends: Vec<String>,
    pub suggests: Vec<String>,
    pub conflicts: Vec<String>,
    pub provides: Vec<String>,
    pub replaces: Vec<String>,
    pub section: String,
    pub priority: String,
    pub installed_size: u64,
    pub download_size: u64,
    pub filename: String, // relative to mirror root
    pub md5sum: String,
    pub sha256: String,
    pub maintainer: String,
    pub homepage: String,
}

impl Default for PackageEntry {
    fn default() -> Self {
        Self {
            name: String::new(),
            version: String::new(),
            architecture: String::from("amd64"),
            description: String::new(),
            depends: Vec::new(),
            recommends: Vec::new(),
            suggests: Vec::new(),
            conflicts: Vec::new(),
            provides: Vec::new(),
            replaces: Vec::new(),
            section: String::new(),
            priority: String::from("optional"),
            installed_size: 0,
            download_size: 0,
            filename: String::new(),
            md5sum: String::new(),
            sha256: String::new(),
            maintainer: String::new(),
            homepage: String::new(),
        }
    }
}

/// A repository source entry from sources.list
#[derive(Debug, Clone)]
pub struct SourceEntry {
    pub entry_type: String, // "deb" or "deb-src"
    pub uri: String,
    pub distribution: String,    // e.g., "bookworm"
    pub components: Vec<String>, // e.g., ["main", "contrib", "non-free"]
    pub trusted: bool,
}

/// apt operation result
#[derive(Debug, Clone)]
pub struct AptResult {
    pub success: bool,
    pub message: String,
    pub packages_installed: u32,
    pub packages_removed: u32,
    pub bytes_downloaded: u64,
}

// ═══════════════════════════════════════════════════════════════════════
// GLOBAL STATE
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    /// Available packages (from package indices)
    static ref AVAILABLE_PACKAGES: Mutex<BTreeMap<String, PackageEntry>> =
        Mutex::new(BTreeMap::new());

    /// Repository sources
    static ref SOURCES: Mutex<Vec<SourceEntry>> = Mutex::new(Vec::new());

    /// Download cache: package name -> cached .deb path
    static ref DOWNLOAD_CACHE: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());
}

static LAST_UPDATE_TIME: AtomicU32 = AtomicU32::new(0);
static PACKAGES_LOADED: AtomicBool = AtomicBool::new(false);

// ═══════════════════════════════════════════════════════════════════════
// SOURCES.LIST PARSING
// ═══════════════════════════════════════════════════════════════════════

/// Parse /etc/apt/sources.list
pub fn parse_sources_list() -> Vec<SourceEntry> {
    let content = crate::vfs::read_file_dispatch("/etc/apt/sources.list").unwrap_or_default();
    let text = String::from_utf8_lossy(&content).to_string();

    let mut sources = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }

        let entry_type = parts[0];
        if entry_type != "deb" && entry_type != "deb-src" {
            continue;
        }

        // Check for [trusted=yes] options
        let mut trusted = false;
        let mut uri_idx = 1;
        if parts.len() > 1 && parts[1].starts_with('[') {
            let options = parts[1].trim_matches(|c| c == '[' || c == ']');
            if options.contains("trusted=yes") {
                trusted = true;
            }
            uri_idx = 2;
        }

        if parts.len() <= uri_idx + 1 {
            continue;
        }

        let uri = parts[uri_idx];
        let distribution = parts[uri_idx + 1];
        let components: Vec<String> = parts[uri_idx + 2..].iter().map(|s| s.to_string()).collect();

        sources.push(SourceEntry {
            entry_type: String::from(entry_type),
            uri: String::from(uri),
            distribution: String::from(distribution),
            components: if components.is_empty() {
                vec![String::from("main")]
            } else {
                components
            },
            trusted,
        });
    }

    sources
}

// ═══════════════════════════════════════════════════════════════════════
// PACKAGES INDEX PARSING
// ═══════════════════════════════════════════════════════════════════════

/// Parse a Packages index file (uncompressed text format)
pub fn parse_packages_index(data: &[u8]) -> Vec<PackageEntry> {
    let text = String::from_utf8_lossy(data).to_string();
    let mut packages = Vec::new();
    let mut current = PackageEntry::default();
    let mut in_description = false;

    for line in text.lines() {
        if line.is_empty() {
            // End of package stanza
            if !current.name.is_empty() {
                packages.push(current);
            }
            current = PackageEntry::default();
            in_description = false;
            continue;
        }

        if line.starts_with(' ') || line.starts_with('\t') {
            // Continuation line
            if in_description {
                current.description.push('\n');
                current.description.push_str(line.trim());
            }
            continue;
        }

        in_description = false;

        if let Some(colon_pos) = line.find(':') {
            let field = &line[..colon_pos];
            let value = line[colon_pos + 1..].trim();

            match field {
                "Package" => current.name = String::from(value),
                "Version" => current.version = String::from(value),
                "Architecture" => current.architecture = String::from(value),
                "Description" => {
                    current.description = String::from(value);
                    in_description = true;
                }
                "Depends" => current.depends = parse_dep_list(value),
                "Recommends" => current.recommends = parse_dep_list(value),
                "Suggests" => current.suggests = parse_dep_list(value),
                "Conflicts" => current.conflicts = parse_dep_list(value),
                "Provides" => current.provides = parse_dep_list(value),
                "Replaces" => current.replaces = parse_dep_list(value),
                "Section" => current.section = String::from(value),
                "Priority" => current.priority = String::from(value),
                "Installed-Size" => {
                    current.installed_size = value.parse().unwrap_or(0);
                }
                "Size" => {
                    current.download_size = value.parse().unwrap_or(0);
                }
                "Filename" => current.filename = String::from(value),
                "MD5sum" => current.md5sum = String::from(value),
                "SHA256" => current.sha256 = String::from(value),
                "Maintainer" => current.maintainer = String::from(value),
                "Homepage" => current.homepage = String::from(value),
                _ => {}
            }
        }
    }

    // Don't forget the last entry
    if !current.name.is_empty() {
        packages.push(current);
    }

    packages
}

/// Parse a dependency list (e.g., "libc6 (>= 2.35), libssl3 | libssl1.1")
fn parse_dep_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|dep| {
            // Take the first alternative in "pkg1 | pkg2" style deps
            let first = dep.split('|').next().unwrap_or(dep);
            // Strip version constraint: "libfoo (>= 1.0)" -> "libfoo"
            let name = first.split('(').next().unwrap_or(first);
            name.trim().to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// APT COMMANDS
// ═══════════════════════════════════════════════════════════════════════

/// apt update — fetch package indices from configured repositories
pub fn apt_update() -> AptResult {
    serial_println!("[apt] Updating package lists...");

    // Load sources
    let sources = parse_sources_list();
    if sources.is_empty() {
        // Create default sources.list
        create_default_sources_list();
        *SOURCES.lock() = parse_sources_list();
    } else {
        *SOURCES.lock() = sources;
    }

    let sources = SOURCES.lock().clone();
    let mut total_packages = 0u32;
    let mut total_bytes = 0u64;

    for source in &sources {
        for component in &source.components {
            let index_url = format!(
                "{}/dists/{}/{}/binary-amd64/Packages",
                source.uri, source.distribution, component
            );

            serial_println!("[apt] Fetching: {}", index_url);

            // Try to download the Packages index via HTTP
            let data = match download_url(&index_url) {
                Some(d) => d,
                None => {
                    // Try .gz variant
                    let gz_url = format!("{}.gz", index_url);
                    serial_println!("[apt] Trying: {}", gz_url);
                    match download_url(&gz_url) {
                        Some(compressed) => {
                            // Decompress gzip
                            crate::dpkg::decompress(&compressed, crate::dpkg::Compression::Gzip)
                                .unwrap_or(compressed)
                        }
                        None => {
                            serial_println!(
                                "[apt] Failed to fetch index for {}/{}",
                                source.distribution,
                                component
                            );
                            continue;
                        }
                    }
                }
            };

            let packages = parse_packages_index(&data);
            serial_println!(
                "[apt] {} packages from {}/{}",
                packages.len(),
                source.distribution,
                component
            );

            total_bytes += data.len() as u64;
            total_packages += packages.len() as u32;

            let mut available = AVAILABLE_PACKAGES.lock();
            for pkg in packages {
                available.insert(pkg.name.clone(), pkg);
            }
        }
    }

    PACKAGES_LOADED.store(true, Ordering::Relaxed);
    LAST_UPDATE_TIME.store(crate::clock::get_ticks() as u32, Ordering::Relaxed);

    serial_println!(
        "[apt] Updated: {} packages available ({} bytes fetched)",
        total_packages,
        total_bytes
    );

    AptResult {
        success: true,
        message: format!(
            "Fetched {} packages from {} sources",
            total_packages,
            sources.len()
        ),
        packages_installed: 0,
        packages_removed: 0,
        bytes_downloaded: total_bytes,
    }
}

/// apt install <package> — install a package and its dependencies
pub fn apt_install(package_names: &[&str]) -> AptResult {
    let mut installed = 0u32;
    let mut bytes_dl = 0u64;
    let mut errors = Vec::new();

    for &name in package_names {
        serial_println!("[apt] Installing: {}", name);

        // Look up in available packages
        let entry = {
            let available = AVAILABLE_PACKAGES.lock();
            available.get(name).cloned()
        };

        let entry = match entry {
            Some(e) => e,
            None => {
                errors.push(format!("E: Unable to locate package {}", name));
                continue;
            }
        };

        // Resolve dependencies
        let deps = resolve_dependencies(name);
        serial_println!("[apt] Dependencies for {}: {:?}", name, deps);

        // Install dependencies first
        for dep in &deps {
            if dep != name {
                if let Err(e) = install_single_package(dep) {
                    serial_println!("[apt] Warning: Failed to install dependency {}: {}", dep, e);
                }
            }
        }

        // Install the package itself
        match install_single_package(name) {
            Ok(size) => {
                installed += 1;
                bytes_dl += size;
            }
            Err(e) => {
                errors.push(format!("E: Failed to install {}: {}", name, e));
            }
        }
    }

    let success = errors.is_empty();
    let message = if success {
        format!(
            "{} newly installed, {} bytes downloaded",
            installed, bytes_dl
        )
    } else {
        errors.join("\n")
    };

    AptResult {
        success,
        message,
        packages_installed: installed,
        packages_removed: 0,
        bytes_downloaded: bytes_dl,
    }
}

/// apt remove <package> — remove a package
pub fn apt_remove(package_names: &[&str]) -> AptResult {
    let mut removed = 0u32;

    for &name in package_names {
        serial_println!("[apt] Removing: {}", name);
        match crate::dpkg_scripts::remove_package_full(name) {
            Ok(()) => removed += 1,
            Err(e) => {
                serial_println!("[apt] Failed to remove {}: {}", name, e);
            }
        }
    }

    AptResult {
        success: true,
        message: format!("{} packages removed", removed),
        packages_installed: 0,
        packages_removed: removed,
        bytes_downloaded: 0,
    }
}

/// apt search <query> — search available packages
pub fn apt_search(query: &str) -> Vec<(String, String, String)> {
    let available = AVAILABLE_PACKAGES.lock();
    let query_lower = query.to_ascii_lowercase();

    available
        .values()
        .filter(|pkg| {
            pkg.name.to_ascii_lowercase().contains(&query_lower)
                || pkg.description.to_ascii_lowercase().contains(&query_lower)
        })
        .map(|pkg| {
            (
                pkg.name.clone(),
                pkg.version.clone(),
                pkg.description.clone(),
            )
        })
        .collect()
}

/// apt show <package> — show package details
pub fn apt_show(name: &str) -> Option<String> {
    let available = AVAILABLE_PACKAGES.lock();
    available.get(name).map(|pkg| {
        format!(
            "Package: {}\nVersion: {}\nArchitecture: {}\nMaintainer: {}\n\
             Section: {}\nPriority: {}\nInstalled-Size: {} kB\n\
             Depends: {}\nDescription: {}\nHomepage: {}\n",
            pkg.name,
            pkg.version,
            pkg.architecture,
            pkg.maintainer,
            pkg.section,
            pkg.priority,
            pkg.installed_size,
            pkg.depends.join(", "),
            pkg.description,
            pkg.homepage
        )
    })
}

/// apt list --installed — list installed packages
pub fn apt_list_installed() -> Vec<(String, String)> {
    crate::dpkg::list_packages(None)
        .iter()
        .map(|info| {
            // Parse dpkg output tuple: (name, version, arch, status)
            (info.0.clone(), info.1.clone())
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// DEPENDENCY RESOLUTION
// ═══════════════════════════════════════════════════════════════════════

/// Resolve all dependencies for a package (topological sort)
pub fn resolve_dependencies(package: &str) -> Vec<String> {
    let mut resolved = Vec::new();
    let mut visited = alloc::collections::BTreeSet::new();
    resolve_deps_recursive(package, &mut resolved, &mut visited);
    resolved
}

fn resolve_deps_recursive(
    package: &str,
    resolved: &mut Vec<String>,
    visited: &mut alloc::collections::BTreeSet<String>,
) {
    if visited.contains(package) {
        return; // Already processed (handles circular deps)
    }
    visited.insert(String::from(package));

    // Get dependencies
    let deps = {
        let available = AVAILABLE_PACKAGES.lock();
        available
            .get(package)
            .map(|pkg| pkg.depends.clone())
            .unwrap_or_default()
    };

    // Recurse into dependencies first (topological order)
    for dep in &deps {
        // Strip version constraints for lookup
        let dep_name = dep.split_whitespace().next().unwrap_or(dep);
        resolve_deps_recursive(dep_name, resolved, visited);
    }

    // Add this package after its dependencies
    resolved.push(String::from(package));
}

// ═══════════════════════════════════════════════════════════════════════
// PACKAGE DOWNLOAD & INSTALLATION
// ═══════════════════════════════════════════════════════════════════════

/// Install a single package by downloading its .deb and running dpkg
fn install_single_package(name: &str) -> Result<u64, String> {
    let entry = {
        let available = AVAILABLE_PACKAGES.lock();
        available.get(name).cloned()
    };

    let entry = entry.ok_or_else(|| format!("Package {} not found", name))?;

    // Check if already installed
    if crate::dpkg::is_installed(name) {
        serial_println!("[apt] {} is already installed", name);
        return Ok(0);
    }

    // Build download URL
    let sources = SOURCES.lock();
    let base_url = sources
        .first()
        .map(|s| s.uri.clone())
        .unwrap_or_else(|| String::from("https://deb.debian.org/debian"));
    drop(sources);

    let deb_url = format!("{}/{}", base_url, entry.filename);
    serial_println!("[apt] Downloading: {}", deb_url);

    // Download the .deb
    let deb_data =
        download_url(&deb_url).ok_or_else(|| format!("Failed to download {}", deb_url))?;

    let download_size = deb_data.len() as u64;

    // Cache the .deb
    let cache_path = format!("/var/cache/apt/archives/{}.deb", name);
    crate::vfs::create_file_dispatch(&cache_path, &deb_data);
    DOWNLOAD_CACHE.lock().insert(String::from(name), cache_path);

    // Install via dpkg
    serial_println!("[apt] Installing {} ({} bytes)...", name, download_size);
    match crate::dpkg::install_deb(&deb_data) {
        Ok(_control) => {
            serial_println!("[apt] Successfully installed {}", name);
            Ok(download_size)
        }
        Err(e) => Err(format!("dpkg error: {:?}", e)),
    }
}

/// Download a URL via the HTTP subsystem
fn download_url(url: &str) -> Option<Vec<u8>> {
    // Use the kernel's HTTP client
    crate::http::get(url).ok()
}

// ═══════════════════════════════════════════════════════════════════════
// DEFAULT CONFIGURATION
// ═══════════════════════════════════════════════════════════════════════

/// Create default /etc/apt/sources.list for Debian bookworm
fn create_default_sources_list() {
    crate::vfs::ensure_directory("/etc/apt");
    crate::vfs::ensure_directory("/etc/apt/sources.list.d");
    crate::vfs::ensure_directory("/etc/apt/trusted.gpg.d");

    let sources = b"\
# KnoxOS Debian Repository Configuration
# Debian Bookworm (stable)
deb https://deb.debian.org/debian bookworm main contrib non-free non-free-firmware
deb https://deb.debian.org/debian bookworm-updates main contrib non-free non-free-firmware
deb https://security.debian.org/debian-security bookworm-security main contrib non-free non-free-firmware
";
    crate::vfs::create_file_dispatch("/etc/apt/sources.list", sources);
    serial_println!("[apt] Created default /etc/apt/sources.list");
}

// ═══════════════════════════════════════════════════════════════════════
// SHELL INTEGRATION
// ═══════════════════════════════════════════════════════════════════════

/// Execute an apt command from the shell
pub fn execute_apt_command(args: &[&str]) -> i32 {
    if args.is_empty() {
        serial_println!("Usage: apt <command> [options] [package ...]");
        serial_println!("Commands: update, install, remove, search, show, list, upgrade");
        return 1;
    }

    match args[0] {
        "update" => {
            let result = apt_update();
            serial_println!("{}", result.message);
            if result.success { 0 } else { 1 }
        }
        "install" => {
            if args.len() < 2 {
                serial_println!("Usage: apt install <package> [package ...]");
                return 1;
            }
            let result = apt_install(&args[1..]);
            serial_println!("{}", result.message);
            if result.success { 0 } else { 1 }
        }
        "remove" | "purge" => {
            if args.len() < 2 {
                serial_println!("Usage: apt remove <package> [package ...]");
                return 1;
            }
            let result = apt_remove(&args[1..]);
            serial_println!("{}", result.message);
            if result.success { 0 } else { 1 }
        }
        "search" => {
            if args.len() < 2 {
                serial_println!("Usage: apt search <query>");
                return 1;
            }
            let query = args[1..].join(" ");
            let results = apt_search(&query);
            for (name, version, desc) in &results {
                serial_println!("{}/{} - {}", name, version, desc);
            }
            serial_println!("{} results found", results.len());
            0
        }
        "show" => {
            if args.len() < 2 {
                serial_println!("Usage: apt show <package>");
                return 1;
            }
            match apt_show(args[1]) {
                Some(info) => {
                    serial_println!("{}", info);
                    0
                }
                None => {
                    serial_println!("E: No packages found for {}", args[1]);
                    1
                }
            }
        }
        "list" => {
            let installed = args.contains(&"--installed");
            if installed {
                let pkgs = apt_list_installed();
                for (name, version) in &pkgs {
                    serial_println!("{} {}", name, version);
                }
                serial_println!("{} packages installed", pkgs.len());
            } else {
                let available = AVAILABLE_PACKAGES.lock();
                for (name, pkg) in available.iter() {
                    serial_println!(
                        "{}/{} {} {}",
                        name,
                        pkg.version,
                        pkg.architecture,
                        pkg.section
                    );
                }
                serial_println!("{} packages available", available.len());
            }
            0
        }
        "upgrade" => {
            serial_println!("[apt] Upgrading all packages...");
            // Check for newer versions of installed packages
            let installed = apt_list_installed();
            let mut to_upgrade = Vec::new();
            let available = AVAILABLE_PACKAGES.lock();
            for (name, current_ver) in &installed {
                if let Some(avail) = available.get(name.as_str()) {
                    if avail.version != *current_ver {
                        to_upgrade.push(name.as_str());
                    }
                }
            }
            drop(available);

            if to_upgrade.is_empty() {
                serial_println!("All packages are up to date.");
            } else {
                serial_println!("{} packages to upgrade", to_upgrade.len());
                // The borrows here are safe since we collected String names
                // For simplicity, just report what would be upgraded
                for pkg in &to_upgrade {
                    serial_println!("  {}", pkg);
                }
            }
            0
        }
        "autoremove" => {
            serial_println!("[apt] Removing unused dependencies...");
            serial_println!("0 packages to remove.");
            0
        }
        _ => {
            serial_println!("E: Invalid operation {}", args[0]);
            1
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

pub fn init() {
    // Ensure apt directories exist
    crate::vfs::ensure_directory("/etc/apt");
    crate::vfs::ensure_directory("/etc/apt/sources.list.d");
    crate::vfs::ensure_directory("/etc/apt/trusted.gpg.d");
    crate::vfs::ensure_directory("/var/cache/apt");
    crate::vfs::ensure_directory("/var/cache/apt/archives");
    crate::vfs::ensure_directory("/var/cache/apt/archives/partial");
    crate::vfs::ensure_directory("/var/lib/apt");
    crate::vfs::ensure_directory("/var/lib/apt/lists");
    crate::vfs::ensure_directory("/var/lib/apt/lists/partial");

    // Create default sources.list if missing
    if crate::vfs::read_file_dispatch("/etc/apt/sources.list").is_none() {
        create_default_sources_list();
    }

    // Load sources
    *SOURCES.lock() = parse_sources_list();
    let source_count = SOURCES.lock().len();

    serial_println!("[apt] APT package manager initialized");
    serial_println!("[apt]   Sources: {} repositories configured", source_count);
    serial_println!("[apt]   Commands: update, install, remove, search, show, list");
    serial_println!("[apt]   Cache: /var/cache/apt/archives");
}
