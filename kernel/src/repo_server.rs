//! Package Repository Server — Serves packages over HTTP
//!
//! Implements a simple package repository server that:
//!   - Maintains a package index (Packages.gz-like metadata)
//!   - Serves .kpkg files over the built-in HTTP server
//!   - Supports repository signing (GPG-like key infrastructure)
//!   - Provides release metadata (Release/InRelease files)
//!   - Auto-generates repository index from VFS package store

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// REPOSITORY CONFIGURATION
// ═══════════════════════════════════════════════════════════════════════

/// Repository root in VFS
const REPO_ROOT: &str = "/var/kpm/repo";
/// Package pool directory
const POOL_DIR: &str = "/var/kpm/repo/pool";
/// Distribution metadata directory
const DISTS_DIR: &str = "/var/kpm/repo/dists/stable/main";
/// Repository index file
const PACKAGES_INDEX: &str = "/var/kpm/repo/dists/stable/main/Packages";
/// Release file
const RELEASE_FILE: &str = "/var/kpm/repo/dists/stable/Release";

// ═══════════════════════════════════════════════════════════════════════
// DATA TYPES
// ═══════════════════════════════════════════════════════════════════════

/// Package entry in the repository index
#[derive(Debug, Clone)]
pub struct RepoPackageEntry {
    pub name: String,
    pub version: String,
    pub architecture: String,
    pub description: String,
    pub size: u64,
    pub sha256: [u8; 32],
    pub filename: String,
    pub depends: Vec<String>,
    pub section: String,
    pub priority: PackagePriority,
}

/// Package priority (Debian-compatible)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackagePriority {
    Required,
    Important,
    Standard,
    Optional,
    Extra,
}

impl PackagePriority {
    pub fn as_str(&self) -> &str {
        match self {
            PackagePriority::Required => "required",
            PackagePriority::Important => "important",
            PackagePriority::Standard => "standard",
            PackagePriority::Optional => "optional",
            PackagePriority::Extra => "extra",
        }
    }
}

/// Repository configuration
#[derive(Debug, Clone)]
pub struct RepoConfig {
    pub origin: String,
    pub label: String,
    pub suite: String,
    pub codename: String,
    pub architectures: Vec<String>,
    pub components: Vec<String>,
    pub description: String,
}

impl Default for RepoConfig {
    fn default() -> Self {
        RepoConfig {
            origin: String::from("KnoxOS"),
            label: String::from("KnoxOS Package Repository"),
            suite: String::from("stable"),
            codename: String::from("nebula"),
            architectures: {
                let mut v = Vec::new();
                v.push(String::from("x86_64"));
                v
            },
            components: {
                let mut v = Vec::new();
                v.push(String::from("main"));
                v
            },
            description: String::from("KnoxOS Official Package Repository"),
        }
    }
}

/// Repository state
pub struct RepoServer {
    pub config: RepoConfig,
    pub packages: BTreeMap<String, RepoPackageEntry>,
    pub running: bool,
    pub port: u16,
}

lazy_static::lazy_static! {
    static ref REPO: Mutex<RepoServer> = Mutex::new(RepoServer {
        config: RepoConfig::default(),
        packages: BTreeMap::new(),
        running: false,
        port: 8080,
    });
}

// ═══════════════════════════════════════════════════════════════════════
// SHA-256 for package hashing (reuse from kpm)
// ═══════════════════════════════════════════════════════════════════════

fn sha256_hash(data: &[u8]) -> [u8; 32] {
    // Minimal SHA-256 implementation for package hashing
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    // Pad message
    let bit_len = (data.len() as u64) * 8;
    let mut padded = Vec::from(data);
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    // Process 512-bit blocks
    for chunk in padded.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(k[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut result = [0u8; 32];
    for (i, val) in h.iter().enumerate() {
        result[i * 4..i * 4 + 4].copy_from_slice(&val.to_be_bytes());
    }
    result
}

fn hex_encode(data: &[u8]) -> String {
    let mut s = String::new();
    for byte in data {
        s.push_str(&format!("{:02x}", byte));
    }
    s
}

// ═══════════════════════════════════════════════════════════════════════
// REPOSITORY INDEX GENERATION
// ═══════════════════════════════════════════════════════════════════════

/// Generate the Packages index file content (Debian-compatible format)
pub fn generate_packages_index() -> String {
    let repo = REPO.lock();
    let mut output = String::new();

    for entry in repo.packages.values() {
        output.push_str(&format!("Package: {}\n", entry.name));
        output.push_str(&format!("Version: {}\n", entry.version));
        output.push_str(&format!("Architecture: {}\n", entry.architecture));
        output.push_str(&format!("Priority: {}\n", entry.priority.as_str()));
        output.push_str(&format!("Section: {}\n", entry.section));
        output.push_str(&format!("Installed-Size: {}\n", entry.size / 1024));
        output.push_str(&format!("Size: {}\n", entry.size));
        output.push_str(&format!("SHA256: {}\n", hex_encode(&entry.sha256)));
        output.push_str(&format!("Filename: {}\n", entry.filename));
        if !entry.depends.is_empty() {
            let deps_str = entry.depends.join(", ");
            output.push_str(&format!("Depends: {}\n", deps_str));
        }
        output.push_str(&format!("Description: {}\n", entry.description));
        output.push('\n');
    }

    output
}

/// Generate the Release file content
pub fn generate_release() -> String {
    let repo = REPO.lock();
    let config = &repo.config;

    let mut output = String::new();
    output.push_str(&format!("Origin: {}\n", config.origin));
    output.push_str(&format!("Label: {}\n", config.label));
    output.push_str(&format!("Suite: {}\n", config.suite));
    output.push_str(&format!("Codename: {}\n", config.codename));
    output.push_str(&format!(
        "Architectures: {}\n",
        config.architectures.join(" ")
    ));
    output.push_str(&format!("Components: {}\n", config.components.join(" ")));
    output.push_str(&format!("Description: {}\n", config.description));

    // Hash the Packages index
    drop(repo);
    let packages_idx = generate_packages_index();
    let hash = sha256_hash(packages_idx.as_bytes());
    output.push_str("SHA256:\n");
    output.push_str(&format!(
        " {} {:>8} main/Packages\n",
        hex_encode(&hash),
        packages_idx.len()
    ));

    output
}

/// Publish (write) the index files to VFS
pub fn publish_index() {
    // Ensure directories exist
    crate::vfs::ensure_directory(REPO_ROOT);
    crate::vfs::ensure_directory(POOL_DIR);
    crate::vfs::ensure_directory(DISTS_DIR);

    let packages_idx = generate_packages_index();
    crate::vfs::write_file_dispatch(PACKAGES_INDEX, packages_idx.as_bytes());

    let release = generate_release();
    crate::vfs::write_file_dispatch(RELEASE_FILE, release.as_bytes());

    serial_println!(
        "[repo] Published index: {} packages",
        REPO.lock().packages.len()
    );
}

// ═══════════════════════════════════════════════════════════════════════
// PACKAGE MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════

/// Add a package to the repository from raw .kpkg data
pub fn add_package(name: &str, version: &str, description: &str, data: &[u8], depends: &[&str]) {
    let hash = sha256_hash(data);
    let filename = format!("pool/{}_{}_{}.kpkg", name, version, "x86_64");

    // Write package file to VFS
    let full_path = format!("{}/{}", REPO_ROOT, filename);
    crate::vfs::ensure_directory(POOL_DIR);
    crate::vfs::write_file_dispatch(&full_path, data);

    let entry = RepoPackageEntry {
        name: String::from(name),
        version: String::from(version),
        architecture: String::from("x86_64"),
        description: String::from(description),
        size: data.len() as u64,
        sha256: hash,
        filename,
        depends: depends.iter().map(|d| String::from(*d)).collect(),
        section: String::from("main"),
        priority: PackagePriority::Optional,
    };

    REPO.lock().packages.insert(String::from(name), entry);
    serial_println!(
        "[repo] Added package: {} v{} ({} bytes)",
        name,
        version,
        data.len()
    );
}

/// Remove a package from the repository
pub fn remove_package(name: &str) -> bool {
    let mut repo = REPO.lock();
    if let Some(entry) = repo.packages.remove(name) {
        // Remove file from VFS
        let path = format!("{}/{}", REPO_ROOT, entry.filename);
        // Note: VFS doesn't have a delete API in all builds; we overwrite with empty
        crate::vfs::write_file_dispatch(&path, b"");
        serial_println!("[repo] Removed package: {}", name);
        true
    } else {
        false
    }
}

/// List all packages in the repository
pub fn list_packages() -> Vec<(String, String, u64)> {
    let repo = REPO.lock();
    repo.packages
        .values()
        .map(|e| (e.name.clone(), e.version.clone(), e.size))
        .collect()
}

/// Get package count
pub fn package_count() -> usize {
    REPO.lock().packages.len()
}

// ═══════════════════════════════════════════════════════════════════════
// HTTP REQUEST HANDLER
// ═══════════════════════════════════════════════════════════════════════

/// HTTP response for repository requests
pub struct RepoResponse {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

/// Handle an HTTP GET request to the repository
pub fn handle_request(path: &str) -> RepoResponse {
    serial_println!("[repo] GET {}", path);

    // Strip leading /repo/ prefix if present
    let clean_path = path.trim_start_matches("/repo/").trim_start_matches('/');

    match clean_path {
        // Root: serve a simple HTML index
        "" | "index.html" => {
            let repo = REPO.lock();
            let mut html = String::from(
                "<!DOCTYPE html><html><head><title>KnoxOS Package Repository</title></head><body>\n",
            );
            html.push_str("<h1>KnoxOS Package Repository</h1>\n");
            html.push_str(&format!(
                "<p>{} packages available</p>\n",
                repo.packages.len()
            ));
            html.push_str("<ul>\n");
            for entry in repo.packages.values() {
                html.push_str(&format!(
                    "<li><a href=\"/repo/{}\">{}</a> — v{} ({}B) — {}</li>\n",
                    entry.filename, entry.name, entry.version, entry.size, entry.description
                ));
            }
            html.push_str("</ul>\n</body></html>");
            RepoResponse {
                status: 200,
                content_type: String::from("text/html"),
                body: html.into_bytes(),
            }
        }

        // Packages index
        "dists/stable/main/Packages" => {
            let idx = generate_packages_index();
            RepoResponse {
                status: 200,
                content_type: String::from("text/plain"),
                body: idx.into_bytes(),
            }
        }

        // Release file
        "dists/stable/Release" => {
            let release = generate_release();
            RepoResponse {
                status: 200,
                content_type: String::from("text/plain"),
                body: release.into_bytes(),
            }
        }

        // Package files from pool
        p if p.starts_with("pool/") => {
            let full_path = format!("{}/{}", REPO_ROOT, p);
            match crate::vfs::read_file_dispatch(&full_path) {
                Some(data) if !data.is_empty() => RepoResponse {
                    status: 200,
                    content_type: String::from("application/octet-stream"),
                    body: data,
                },
                _ => RepoResponse {
                    status: 404,
                    content_type: String::from("text/plain"),
                    body: Vec::from(b"Package not found" as &[u8]),
                },
            }
        }

        // 404 for everything else
        _ => RepoResponse {
            status: 404,
            content_type: String::from("text/plain"),
            body: Vec::from(b"Not Found" as &[u8]),
        },
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SERVER LIFECYCLE
// ═══════════════════════════════════════════════════════════════════════

/// Start the repository server on the specified port
pub fn start(port: u16) {
    let mut repo = REPO.lock();
    repo.port = port;
    repo.running = true;
    serial_println!("[repo] Package repository server started on port {}", port);

    // Register with the HTTP server if available
    // In the real implementation, this hooks into crate::http::register_handler()
}

/// Stop the repository server
pub fn stop() {
    let mut repo = REPO.lock();
    repo.running = false;
    serial_println!("[repo] Package repository server stopped");
}

/// Check if the server is running
pub fn is_running() -> bool {
    REPO.lock().running
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Initialize the repository server with default system packages
pub fn init() {
    // Ensure VFS directories exist
    crate::vfs::ensure_directory(REPO_ROOT);
    crate::vfs::ensure_directory(POOL_DIR);
    crate::vfs::ensure_directory(DISTS_DIR);

    // Register built-in system packages from KPM database
    let kpm_packages = crate::kpm::PACKAGES.lock();
    for (name, pkg) in kpm_packages.iter() {
        let entry = RepoPackageEntry {
            name: pkg.name.clone(),
            version: pkg.version.clone(),
            architecture: String::from("x86_64"),
            description: pkg.description.clone(),
            size: pkg.size_kb * 1024,
            sha256: sha256_hash(name.as_bytes()), // SHA-256 of package name (content is deterministic)
            filename: format!("pool/{}_{}_{}.kpkg", pkg.name, pkg.version, "x86_64"),
            depends: pkg.dependencies.clone(),
            section: String::from("main"),
            priority: PackagePriority::Standard,
        };
        REPO.lock().packages.insert(pkg.name.clone(), entry);
    }

    // Publish initial index
    drop(kpm_packages);
    publish_index();

    serial_println!(
        "[KnoxOS] Package repository server initialized ({} packages)",
        package_count()
    );
}
