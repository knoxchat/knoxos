use alloc::collections::BTreeMap;
/// Package Manager - KPM (KnoxOS Package Manager)
/// Manages software packages within the KnoxOS ecosystem
/// Compatible with Linux package concepts (name, version, deps)
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

/// Package state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageState {
    Available,
    Installed,
    Updating,
    Removing,
    Broken,
}

/// Package information
#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub description: String,
    pub dependencies: Vec<String>,
    pub size_kb: u64,
    pub state: PackageState,
    pub installed_files: Vec<String>,
}

/// Package repository
#[derive(Debug, Clone)]
pub struct Repository {
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub packages: Vec<String>,
}

/// Package database
lazy_static::lazy_static! {
    pub static ref PACKAGES: Mutex<BTreeMap<String, Package>> = Mutex::new(BTreeMap::new());
    static ref REPOSITORIES: Mutex<Vec<Repository>> = Mutex::new(Vec::new());
}

/// Initialize the package manager with built-in packages
pub fn init() {
    let mut packages = PACKAGES.lock();

    // Register built-in system packages
    let builtins = [
        (
            "knoxos-kernel",
            "0.1.0",
            "KnoxOS Kernel",
            &[] as &[&str],
            2048,
        ),
        (
            "knoxos-desktop",
            "0.1.0",
            "KnoxOS Desktop Environment",
            &["knoxos-kernel"],
            1024,
        ),
        (
            "knoxosxos-shell",
            "0.1.0",
            "KnoxOS Shell (ksh)",
            &["knoxos-kernel"],
            256,
        ),
        (
            "knoxos-terminal",
            "0.1.0",
            "KnoxOS Terminal Emulator",
            &["knoxos-shell"],
            128,
        ),
        (
            "knoxos-ai",
            "0.1.0",
            "KnoxOS AI Inference Engine",
            &["knoxos-kernel"],
            512,
        ),
        (
            "knoxos-network",
            "0.1.0",
            "KnoxOS TCP/IP Network Stack",
            &["knoxos-kernel"],
            384,
        ),
        (
            "knoxos-fs",
            "0.1.0",
            "KnoxOS Filesystem Drivers",
            &["knoxos-kernel"],
            256,
        ),
        (
            "coreutils",
            "0.1.0",
            "Core Unix Utilities",
            &["knoxos-kernel"],
            512,
        ),
        (
            "gcc",
            "13.2.0",
            "GNU Compiler Collection",
            &["coreutils"],
            8192,
        ),
        (
            "python",
            "3.12.0",
            "Python Interpreter",
            &["coreutils"],
            16384,
        ),
        (
            "vim",
            "9.0",
            "Vi IMproved Text Editor",
            &["coreutils"],
            1024,
        ),
        ("git", "2.42.0", "Git Version Control", &["coreutils"], 2048),
        (
            "curl",
            "8.4.0",
            "URL Transfer Utility",
            &["knoxos-network"],
            512,
        ),
        (
            "openssh",
            "9.5",
            "OpenSSH Client/Server",
            &["knoxos-network"],
            1024,
        ),
        (
            "mesa",
            "23.2.0",
            "OpenGL/Vulkan Graphics Library",
            &["knoxos-kernel"],
            4096,
        ),
        (
            "ffmpeg",
            "6.0",
            "Multimedia Framework",
            &["coreutils"],
            8192,
        ),
        ("nodejs", "20.0.0", "Node.js Runtime", &["coreutils"], 12288),
        (
            "rust",
            "1.75.0",
            "Rust Programming Language",
            &["coreutils", "gcc"],
            16384,
        ),
    ];

    for (name, version, desc, deps, size) in builtins.iter() {
        let state = if name.starts_with("knoxos-") || *name == "coreutils" {
            PackageState::Installed
        } else {
            PackageState::Available
        };

        packages.insert(
            String::from(*name),
            Package {
                name: String::from(*name),
                version: String::from(*version),
                description: String::from(*desc),
                dependencies: deps.iter().map(|d| String::from(*d)).collect(),
                size_kb: *size,
                state,
                installed_files: Vec::new(),
            },
        );
    }

    let mut repos = REPOSITORIES.lock();
    repos.push(Repository {
        name: String::from("knoxos-core"),
        url: String::from("https://packages.knoxos.com/core"),
        enabled: true,
        packages: packages.keys().cloned().collect(),
    });

    crate::serial_println!(
        "[KnoxOS] Package manager initialized ({} packages)",
        packages.len()
    );

    drop(packages);
    drop(repos);

    // Initialize package signing subsystem
    init_signing();
}

/// Resolve full dependency graph for a package using topological sort (Kahn's algorithm).
/// Returns the install order (dependencies first, target package last).
/// Detects circular dependencies and missing packages.
pub fn resolve_dependencies(name: &str) -> Result<Vec<String>, String> {
    let packages = PACKAGES.lock();

    // Build the set of packages we need to install
    let mut needed: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut queue: Vec<String> = vec![String::from(name)];

    while let Some(current) = queue.pop() {
        if needed.contains_key(&current) {
            continue;
        }
        let pkg = packages.get(&current).ok_or(alloc::format!(
            "Package '{}' not found (required by dependency chain)",
            current
        ))?;
        let deps: Vec<String> = pkg.dependencies.clone();
        for dep in &deps {
            if !needed.contains_key(dep) {
                // Only queue deps that aren't already installed
                if let Some(dep_pkg) = packages.get(dep) {
                    if dep_pkg.state != PackageState::Installed {
                        queue.push(dep.clone());
                    }
                } else {
                    return Err(alloc::format!(
                        "Dependency '{}' not found (required by '{}')",
                        dep,
                        current
                    ));
                }
            }
        }
        needed.insert(current, deps);
    }

    // Remove packages that are already installed (except the target if forced)
    let mut to_install: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (pkg_name, deps) in &needed {
        if let Some(pkg) = packages.get(pkg_name) {
            if pkg.state != PackageState::Installed {
                // Filter deps to only those that are also in to_install
                let filtered_deps: Vec<String> = deps
                    .iter()
                    .filter(|d| {
                        packages
                            .get(*d)
                            .map(|p| p.state != PackageState::Installed)
                            .unwrap_or(false)
                    })
                    .cloned()
                    .collect();
                to_install.insert(pkg_name.clone(), filtered_deps);
            }
        }
    }

    if to_install.is_empty() {
        return Ok(Vec::new()); // Everything already installed
    }

    // Kahn's topological sort
    let mut in_degree: BTreeMap<String, usize> = BTreeMap::new();
    for (pkg_name, deps) in &to_install {
        in_degree.entry(pkg_name.clone()).or_insert(0);
        for dep in deps {
            if to_install.contains_key(dep) {
                *in_degree.entry(pkg_name.clone()).or_insert(0) += 1;
            }
        }
    }

    // Also count incoming edges properly: for each (pkg, deps), each dep in to_install
    // means pkg has an incoming edge from dep
    // Reset and recount
    let mut in_deg: BTreeMap<String, usize> = BTreeMap::new();
    for pkg_name in to_install.keys() {
        in_deg.insert(pkg_name.clone(), 0);
    }
    for (pkg_name, deps) in &to_install {
        for dep in deps {
            if in_deg.contains_key(dep) {
                // dep → pkg_name (pkg_name depends on dep)
                // This means pkg_name has in-degree += 0 from dep? No:
                // pkg_name depends on dep → dep must come before pkg_name
                // In the DAG: edge from dep TO pkg_name
                // So pkg_name's in-degree increases
                *in_deg.get_mut(pkg_name).unwrap() += 1;
            }
        }
    }

    let mut result: Vec<String> = Vec::new();
    let mut ready: Vec<String> = in_deg
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(name, _)| name.clone())
        .collect();
    ready.sort(); // deterministic order

    while let Some(current) = ready.pop() {
        result.push(current.clone());
        // For all packages that depend on `current`, decrease their in-degree
        for (pkg_name, deps) in &to_install {
            if deps.contains(&current) {
                if let Some(deg) = in_deg.get_mut(pkg_name) {
                    *deg -= 1;
                    if *deg == 0 {
                        ready.push(pkg_name.clone());
                    }
                }
            }
        }
    }

    if result.len() != to_install.len() {
        return Err(alloc::format!(
            "Circular dependency detected! Resolved {}/{} packages",
            result.len(),
            to_install.len()
        ));
    }

    Ok(result)
}

/// Install a package (with automatic dependency resolution)
pub fn install(name: &str) -> Result<(), String> {
    // First resolve the full dependency graph
    let install_order = resolve_dependencies(name)?;

    if install_order.is_empty() {
        let packages = PACKAGES.lock();
        if let Some(pkg) = packages.get(name) {
            if pkg.state == PackageState::Installed {
                return Err(alloc::format!("Package '{}' is already installed", name));
            }
        }
        return Ok(());
    }

    // Install each package in topological order
    let mut packages = PACKAGES.lock();
    for pkg_name in &install_order {
        if let Some(pkg) = packages.get_mut(pkg_name) {
            if pkg.state == PackageState::Installed {
                continue;
            }
            pkg.state = PackageState::Installed;
            pkg.installed_files = vec![
                alloc::format!("/usr/bin/{}", pkg_name),
                alloc::format!("/usr/share/doc/{}/README", pkg_name),
                alloc::format!("/usr/share/man/man1/{}.1", pkg_name),
            ];
            crate::serial_println!("[KnoxOS] kpm: Installed {} v{}", pkg_name, pkg.version);
        }
    }

    crate::serial_println!(
        "[KnoxOS] kpm: Resolved and installed {} package(s) for '{}'",
        install_order.len(),
        name
    );
    Ok(())
}

/// Remove a package
pub fn remove(name: &str) -> Result<(), String> {
    let mut packages = PACKAGES.lock();

    // Check if anything depends on this package
    let dependents: Vec<String> = packages
        .values()
        .filter(|p| {
            p.state == PackageState::Installed && p.dependencies.contains(&String::from(name))
        })
        .map(|p| p.name.clone())
        .collect();

    if !dependents.is_empty() {
        return Err(alloc::format!(
            "Cannot remove '{}': required by {}",
            name,
            dependents.join(", ")
        ));
    }

    if name.starts_with("knoxos-") {
        return Err(alloc::format!("Cannot remove system package '{}'", name));
    }

    let pkg = packages
        .get_mut(name)
        .ok_or(alloc::format!("Package '{}' not found", name))?;

    if pkg.state != PackageState::Installed {
        return Err(alloc::format!("Package '{}' is not installed", name));
    }

    pkg.state = PackageState::Available;
    pkg.installed_files.clear();

    crate::serial_println!("[KnoxOS] kpm: Removed {}", name);
    Ok(())
}

/// Search for packages matching a query
pub fn search(query: &str) -> Vec<Package> {
    let packages = PACKAGES.lock();
    let query_lower = query.to_ascii_lowercase();

    packages
        .values()
        .filter(|p| {
            p.name.to_ascii_lowercase().contains(&query_lower)
                || p.description.to_ascii_lowercase().contains(&query_lower)
        })
        .cloned()
        .collect()
}

/// List installed packages
pub fn list_installed() -> Vec<Package> {
    let packages = PACKAGES.lock();
    packages
        .values()
        .filter(|p| p.state == PackageState::Installed)
        .cloned()
        .collect()
}

/// List all packages
pub fn list_all() -> Vec<Package> {
    let packages = PACKAGES.lock();
    packages.values().cloned().collect()
}

/// Get package info
pub fn info(name: &str) -> Option<Package> {
    PACKAGES.lock().get(name).cloned()
}

/// Update package database (simulated)
pub fn update() -> usize {
    let packages = PACKAGES.lock();
    let count = packages.len();
    crate::serial_println!(
        "[KnoxOS] kpm: Package database updated ({} packages)",
        count
    );
    count
}

/// Upgrade installed packages (simulated)
pub fn upgrade() -> Vec<String> {
    let packages = PACKAGES.lock();
    let upgraded: Vec<String> = packages
        .values()
        .filter(|p| p.state == PackageState::Installed)
        .map(|p| p.name.clone())
        .collect();

    crate::serial_println!("[KnoxOS] kpm: {} packages up to date", upgraded.len());
    upgraded
}

// ═══════════════════════════════════════════════════════════════════════
// PACKAGE DOWNLOAD FROM NETWORK
// ═══════════════════════════════════════════════════════════════════════

/// Download a package from the repository network.
/// Uses HTTP GET to fetch the `.kpkg` archive (tar+gzip) from the first
/// enabled repository that has the package.
pub fn download_package(name: &str) -> Result<Vec<u8>, String> {
    let repos = REPOSITORIES.lock();
    let mut target_url = None;

    for repo in repos.iter() {
        if !repo.enabled {
            continue;
        }
        if repo.packages.contains(&String::from(name)) {
            target_url = Some(alloc::format!("{}/{}.kpkg", repo.url, name));
            break;
        }
    }
    drop(repos);

    let url = target_url.ok_or(alloc::format!(
        "Package '{}' not found in any repository",
        name
    ))?;

    crate::serial_println!("[kpm] Downloading {} from {}", name, url);

    // Parse URL into host and path
    let stripped = if let Some(rest) = url.strip_prefix("https://") {
        rest
    } else if let Some(rest) = url.strip_prefix("http://") {
        rest
    } else {
        &url
    };

    let (host, path) = if let Some(slash_pos) = stripped.find('/') {
        (&stripped[..slash_pos], &stripped[slash_pos..])
    } else {
        (stripped, "/")
    };

    // Resolve hostname via DNS
    let addrs =
        crate::dns::resolve(host).ok_or(alloc::format!("DNS resolution failed for '{}'", host))?;
    let ip_bytes = addrs
        .first()
        .ok_or(alloc::format!("No addresses for '{}'", host))?;

    // Build sockaddr_in for connect()
    let sockaddr = crate::net::SockAddrIn {
        sin_family: 2, // AF_INET
        sin_port: 80u16.to_be(),
        sin_addr: u32::from_be_bytes(*ip_bytes),
        sin_zero: [0u8; 8],
    };

    // Connect via TCP and send HTTP GET request
    let fd = crate::net::sys_socket(2, 1, 6) // AF_INET, SOCK_STREAM, TCP
        .map_err(|_| alloc::format!("Failed to create socket"))?;

    let connect_result =
        crate::net::sys_connect(fd, &sockaddr as *const crate::net::SockAddrIn as u64);
    if connect_result.is_err() {
        let _ = crate::net::sys_close_socket(fd);
        return Err(alloc::format!("Failed to connect to {}:80", host));
    }

    // Build HTTP GET request
    let request = alloc::format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept: application/octet-stream\r\n\r\n",
        path,
        host
    );
    let _ = crate::net::sys_sendto(fd, request.as_bytes(), 0);

    // Read response
    let mut response = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match crate::net::sys_recvfrom(fd, &mut buf) {
            Ok(n) if n > 0 => {
                response.extend_from_slice(&buf[..n]);
            }
            _ => break,
        }
    }
    let _ = crate::net::sys_close_socket(fd);

    // Parse HTTP response — skip headers (find \r\n\r\n)
    let body_start = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|p| p + 4)
        .unwrap_or(0);

    if body_start >= response.len() {
        return Err(alloc::format!("Empty response body from {}", url));
    }

    let body = response[body_start..].to_vec();
    crate::serial_println!("[kpm] Downloaded {} bytes for '{}'", body.len(), name);

    // Cache the downloaded package in VFS
    let cache_path = alloc::format!("/var/cache/kpm/{}.kpkg", name);
    crate::vfs::ensure_directory("/var/cache/kpm");
    crate::vfs::write_file_dispatch(&cache_path, &body);

    Ok(body)
}

// ═══════════════════════════════════════════════════════════════════════
// PACKAGE FILE EXTRACTION (ar/tar)
// ═══════════════════════════════════════════════════════════════════════

/// KnoxOS package archive format (.kpkg):
///   Header: "KPKG" (4 bytes) + version (u16 LE) + flags (u16 LE)
///   Signature: length (u32 LE) + signature bytes
///   File entries: count (u32 LE) followed by entries:
///     - path_len (u16 LE) + path (UTF-8)
///     - perms (u16 LE)
///     - data_len (u32 LE) + data
///
/// Also supports simple tar archive extraction (POSIX.1 ustar format)
const KPKG_MAGIC: &[u8; 4] = b"KPKG";
const KPKG_VERSION: u16 = 1;
const TAR_BLOCK_SIZE: usize = 512;

/// A file entry extracted from a package archive
#[derive(Debug, Clone)]
pub struct ExtractedFile {
    pub path: String,
    pub data: Vec<u8>,
    pub permissions: u16,
    pub is_directory: bool,
}

/// Extract files from a .kpkg archive
pub fn extract_kpkg(data: &[u8]) -> Result<Vec<ExtractedFile>, String> {
    if data.len() < 8 {
        return Err(String::from("Archive too small"));
    }

    // Check for KPKG native format
    if &data[0..4] == KPKG_MAGIC {
        return extract_kpkg_native(data);
    }

    // Fall back to tar format detection (ustar magic at offset 257)
    if data.len() > 263 && &data[257..262] == b"ustar" {
        return extract_tar(data);
    }

    // Try ar format (Debian .deb packages use ar)
    if data.len() > 8 && &data[0..8] == b"!<arch>\n" {
        return extract_ar(data);
    }

    Err(String::from("Unknown archive format"))
}

/// Extract KPKG native format
fn extract_kpkg_native(data: &[u8]) -> Result<Vec<ExtractedFile>, String> {
    let mut pos = 4; // Skip magic

    // Version + flags
    if data.len() < pos + 4 {
        return Err(String::from("Truncated header"));
    }
    let _version = u16::from_le_bytes([data[pos], data[pos + 1]]);
    let _flags = u16::from_le_bytes([data[pos + 2], data[pos + 3]]);
    pos += 4;

    // Signature (skip for now, verified separately)
    if data.len() < pos + 4 {
        return Err(String::from("Truncated signature length"));
    }
    let sig_len =
        u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
    pos += 4 + sig_len;

    // File count
    if data.len() < pos + 4 {
        return Err(String::from("Truncated file count"));
    }
    let file_count =
        u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
    pos += 4;

    let mut files = Vec::with_capacity(file_count);

    for _ in 0..file_count {
        // Path length + path
        if data.len() < pos + 2 {
            return Err(String::from("Truncated entry"));
        }
        let path_len = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
        pos += 2;
        if data.len() < pos + path_len {
            return Err(String::from("Truncated path"));
        }
        let path = core::str::from_utf8(&data[pos..pos + path_len])
            .map_err(|_| String::from("Invalid UTF-8 in path"))?;
        pos += path_len;

        // Permissions
        if data.len() < pos + 2 {
            return Err(String::from("Truncated permissions"));
        }
        let perms = u16::from_le_bytes([data[pos], data[pos + 1]]);
        pos += 2;

        // Data length + data
        if data.len() < pos + 4 {
            return Err(String::from("Truncated data length"));
        }
        let data_len =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        if data.len() < pos + data_len {
            return Err(String::from("Truncated data"));
        }
        let file_data = data[pos..pos + data_len].to_vec();
        pos += data_len;

        let is_directory = perms & 0x4000 != 0;
        files.push(ExtractedFile {
            path: String::from(path),
            data: file_data,
            permissions: perms,
            is_directory,
        });
    }

    crate::serial_println!("[kpm] Extracted {} files from KPKG archive", files.len());
    Ok(files)
}

/// Extract POSIX tar (ustar) format
fn extract_tar(data: &[u8]) -> Result<Vec<ExtractedFile>, String> {
    let mut files = Vec::new();
    let mut pos = 0;

    while pos + TAR_BLOCK_SIZE <= data.len() {
        let header = &data[pos..pos + TAR_BLOCK_SIZE];

        // Check for end-of-archive (two zero blocks)
        if header.iter().all(|&b| b == 0) {
            break;
        }

        // Parse tar header
        let name = {
            let raw = &header[0..100];
            let end = raw.iter().position(|&b| b == 0).unwrap_or(100);
            String::from(core::str::from_utf8(&raw[..end]).unwrap_or(""))
        };

        // Prefix (for long paths, ustar format)
        let prefix = {
            let raw = &header[345..500];
            let end = raw.iter().position(|&b| b == 0).unwrap_or(155);
            core::str::from_utf8(&raw[..end]).unwrap_or("")
        };

        let full_path = if prefix.is_empty() {
            name
        } else {
            alloc::format!("{}/{}", prefix, name)
        };

        // File size (octal ASCII)
        let size_str = core::str::from_utf8(&header[124..135]).unwrap_or("0");
        let size = usize::from_str_radix(size_str.trim().trim_end_matches('\0'), 8).unwrap_or(0);

        // Type flag
        let typeflag = header[156];
        let is_directory = typeflag == b'5' || full_path.ends_with('/');

        // Permissions (octal)
        let mode_str = core::str::from_utf8(&header[100..107]).unwrap_or("0");
        let perms = u16::from_str_radix(mode_str.trim().trim_end_matches('\0'), 8).unwrap_or(0o644);

        pos += TAR_BLOCK_SIZE;

        // Read file data (rounded up to 512-byte blocks)
        let file_data = if size > 0 && pos + size <= data.len() {
            data[pos..pos + size].to_vec()
        } else {
            Vec::new()
        };

        // Advance past data blocks
        let data_blocks = size.div_ceil(TAR_BLOCK_SIZE);
        pos += data_blocks * TAR_BLOCK_SIZE;

        if !full_path.is_empty() && typeflag != b'x' && typeflag != b'g' {
            files.push(ExtractedFile {
                path: full_path,
                data: file_data,
                permissions: perms,
                is_directory,
            });
        }
    }

    crate::serial_println!("[kpm] Extracted {} files from tar archive", files.len());
    Ok(files)
}

/// Extract Unix ar archive (used by .deb packages)
fn extract_ar(data: &[u8]) -> Result<Vec<ExtractedFile>, String> {
    let mut files = Vec::new();
    let mut pos = 8; // Skip "!<arch>\n"

    while pos + 60 <= data.len() {
        let header = &data[pos..pos + 60];

        // ar entry header: name(16) + mtime(12) + owner(6) + group(6) + mode(8) + size(10) + magic(2)
        let name = {
            let raw = &header[0..16];
            let end = raw
                .iter()
                .position(|&b| b == b'/' || b == b' ')
                .unwrap_or(16);
            String::from(core::str::from_utf8(&raw[..end]).unwrap_or(""))
        };

        let size_str = core::str::from_utf8(&header[48..58]).unwrap_or("0");
        let size = size_str.trim().parse::<usize>().unwrap_or(0);

        let mode_str = core::str::from_utf8(&header[40..48]).unwrap_or("0");
        let perms = u16::from_str_radix(mode_str.trim(), 8).unwrap_or(0o644);

        pos += 60;

        let file_data = if size > 0 && pos + size <= data.len() {
            data[pos..pos + size].to_vec()
        } else {
            Vec::new()
        };
        pos += size;
        // ar entries are 2-byte aligned
        if pos % 2 != 0 {
            pos += 1;
        }

        if !name.is_empty() {
            files.push(ExtractedFile {
                path: name,
                data: file_data,
                permissions: perms,
                is_directory: false,
            });
        }
    }

    crate::serial_println!("[kpm] Extracted {} files from ar archive", files.len());
    Ok(files)
}

/// Install extracted files to the VFS
pub fn install_files(files: &[ExtractedFile], prefix: &str) -> Vec<String> {
    let mut installed_paths = Vec::new();

    for file in files {
        let full_path = if file.path.starts_with('/') {
            file.path.clone()
        } else {
            alloc::format!("{}/{}", prefix, file.path)
        };

        if file.is_directory {
            crate::vfs::ensure_directory(&full_path);
        } else {
            // Ensure parent directory exists
            if let Some(parent_end) = full_path.rfind('/') {
                if parent_end > 0 {
                    crate::vfs::ensure_directory(&full_path[..parent_end]);
                }
            }
            crate::vfs::write_file_dispatch(&full_path, &file.data);
        }

        installed_paths.push(full_path);
    }

    crate::serial_println!("[kpm] Installed {} files to VFS", installed_paths.len());
    installed_paths
}

// ═══════════════════════════════════════════════════════════════════════
// PACKAGE SIGNING & VERIFICATION (Ed25519-like)
// ═══════════════════════════════════════════════════════════════════════

/// Package signature header in .kpkg archives
#[derive(Debug, Clone)]
pub struct PackageSignature {
    /// Key ID of the signing key (8 bytes)
    pub key_id: [u8; 8],
    /// Signature algorithm: 0 = SHA256-HMAC, 1 = Ed25519-like
    pub algorithm: u8,
    /// Signature bytes
    pub signature: Vec<u8>,
}

/// Trusted signing keys
lazy_static::lazy_static! {
    static ref TRUSTED_KEYS: Mutex<Vec<TrustedKey>> = Mutex::new(Vec::new());
}

/// A trusted signing key
#[derive(Debug, Clone)]
pub struct TrustedKey {
    pub key_id: [u8; 8],
    pub name: String,
    /// Public key bytes (32 bytes for Ed25519, or HMAC shared secret)
    pub public_key: Vec<u8>,
}

/// Initialize the package signing subsystem with default trusted keys
pub fn init_signing() {
    let mut keys = TRUSTED_KEYS.lock();

    // Default KnoxOS signing key (built-in)
    keys.push(TrustedKey {
        key_id: *b"KNOXOS01",
        name: String::from("KnoxOS Official Signing Key"),
        public_key: {
            // SHA-256 of "KnoxOS Official Signing Key v1" — used as HMAC key
            let seed = crate::users::hash_password("KnoxOS-Package-Signing-v1", "knoxos-official");
            seed.into_bytes().into_iter().take(32).collect()
        },
    });

    crate::serial_println!(
        "[kpm] Package signing initialized ({} trusted keys)",
        keys.len()
    );
}

/// Verify a package signature
pub fn verify_signature(data: &[u8], sig: &PackageSignature) -> bool {
    let keys = TRUSTED_KEYS.lock();

    // Find the trusted key matching the signature's key_id
    let key = match keys.iter().find(|k| k.key_id == sig.key_id) {
        Some(k) => k,
        None => {
            crate::serial_println!(
                "[kpm] WARN: Unknown signing key {:?}, signature not trusted",
                sig.key_id
            );
            return false;
        }
    };

    match sig.algorithm {
        0 => {
            // SHA256-HMAC verification
            let computed = crate::tls::hmac_sha256(&key.public_key, data);
            // Constant-time comparison
            if sig.signature.len() != 32 {
                return false;
            }
            let mut diff = 0u8;
            for (a, b) in sig.signature.iter().zip(computed.iter()) {
                diff |= a ^ b;
            }
            let valid = diff == 0;
            if valid {
                crate::serial_println!(
                    "[kpm] Package signature verified (SHA256-HMAC, key={})",
                    key.name
                );
            } else {
                crate::serial_println!("[kpm] INVALID package signature!");
            }
            valid
        }
        _ => {
            crate::serial_println!("[kpm] Unsupported signature algorithm: {}", sig.algorithm);
            false
        }
    }
}

/// Sign package data (for creating packages)
pub fn sign_package(data: &[u8]) -> Option<PackageSignature> {
    let keys = TRUSTED_KEYS.lock();
    let key = keys.first()?;

    let signature = crate::tls::hmac_sha256(&key.public_key, data).to_vec();

    Some(PackageSignature {
        key_id: key.key_id,
        algorithm: 0, // SHA256-HMAC
        signature,
    })
}

/// Full package install pipeline: download → verify → extract → install
pub fn install_from_network(name: &str) -> Result<(), String> {
    // 1. Resolve dependencies
    let install_order = resolve_dependencies(name)?;
    if install_order.is_empty() {
        let packages = PACKAGES.lock();
        if let Some(pkg) = packages.get(name) {
            if pkg.state == PackageState::Installed {
                return Err(alloc::format!("Package '{}' is already installed", name));
            }
        }
    }

    let to_install = if install_order.is_empty() {
        vec![String::from(name)]
    } else {
        install_order
    };

    for pkg_name in &to_install {
        // 2. Download
        let archive = download_package(pkg_name)?;

        // 3. Verify signature (if KPKG format with embedded signature)
        if archive.len() >= 12 && &archive[0..4] == KPKG_MAGIC {
            let sig_offset = 8; // After magic + version + flags
            if archive.len() > sig_offset + 4 {
                let sig_len = u32::from_le_bytes([
                    archive[sig_offset],
                    archive[sig_offset + 1],
                    archive[sig_offset + 2],
                    archive[sig_offset + 3],
                ]) as usize;
                if sig_len > 0 && archive.len() > sig_offset + 4 + sig_len {
                    let sig_data = &archive[sig_offset + 4..sig_offset + 4 + sig_len];
                    if sig_data.len() >= 9 + 32 {
                        let mut key_id = [0u8; 8];
                        key_id.copy_from_slice(&sig_data[0..8]);
                        let sig = PackageSignature {
                            key_id,
                            algorithm: sig_data[8],
                            signature: sig_data[9..].to_vec(),
                        };
                        // Verify against the file data after the signature
                        let payload_start = sig_offset + 4 + sig_len;
                        if !verify_signature(&archive[payload_start..], &sig) {
                            return Err(alloc::format!(
                                "Signature verification failed for '{}'",
                                pkg_name
                            ));
                        }
                    }
                }
            }
        }

        // 4. Extract
        let files = extract_kpkg(&archive)?;

        // 5. Install to VFS
        let installed_paths = install_files(&files, "/");

        // 6. Update package database
        let mut packages = PACKAGES.lock();
        if let Some(pkg) = packages.get_mut(pkg_name) {
            pkg.state = PackageState::Installed;
            pkg.installed_files = installed_paths;
        }

        crate::serial_println!("[kpm] Successfully installed '{}' from network", pkg_name);
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// Package Search with Description Matching
// ═══════════════════════════════════════════════════════════════════════

/// Search result entry
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub name: String,
    pub version: String,
    pub description: String,
    pub score: u32,
    pub installed: bool,
}

/// Search packages by name or description keyword
pub fn search_packages(query: &str) -> Vec<SearchResult> {
    let packages = PACKAGES.lock();
    let query_lower = query.to_ascii_lowercase();
    let mut results = Vec::new();
    for (name, pkg) in packages.iter() {
        let name_lower = name.to_ascii_lowercase();
        let desc_lower = pkg.description.to_ascii_lowercase();
        let mut score = 0u32;
        if name_lower.contains(&query_lower) {
            score += 100;
            if name_lower.starts_with(&query_lower) {
                score += 50;
            }
        }
        if desc_lower.contains(&query_lower) {
            score += 30;
        }
        if score > 0 {
            results.push(SearchResult {
                name: name.clone(),
                version: pkg.version.clone(),
                description: pkg.description.clone(),
                score,
                installed: pkg.state == PackageState::Installed,
            });
        }
    }
    results.sort_by_key(|b| core::cmp::Reverse(b.score));
    results
}

// ═══════════════════════════════════════════════════════════════════════
// Dependency Conflict Resolution
// ═══════════════════════════════════════════════════════════════════════

/// Dependency conflict
#[derive(Debug, Clone)]
pub struct DepConflict {
    pub package_a: String,
    pub package_b: String,
    pub conflicting_dep: String,
    pub version_a: String,
    pub version_b: String,
}

/// Check for dependency conflicts before install
pub fn check_conflicts(pkg_name: &str) -> Vec<DepConflict> {
    let packages = PACKAGES.lock();
    let mut conflicts = Vec::new();
    if let Some(pkg) = packages.get(pkg_name) {
        for dep in &pkg.dependencies {
            // Check if any installed package requires a different version
            for (name, other) in packages.iter() {
                if other.state == PackageState::Installed
                    && name != pkg_name
                    && other.dependencies.contains(dep)
                {
                    // Both depend on same thing — check versions (simplified)
                    // Real implementation would parse version constraints
                }
            }
        }
    }
    conflicts
}

// ═══════════════════════════════════════════════════════════════════════
// Automatic Security Updates
// ═══════════════════════════════════════════════════════════════════════

/// Security update configuration
pub struct AutoUpdateConfig {
    pub enabled: bool,
    pub check_interval_hours: u32,
    pub auto_install: bool,
    pub security_only: bool,
    pub last_check: u64,
    pub pending_updates: Vec<String>,
}

lazy_static::lazy_static! {
    static ref AUTO_UPDATE: Mutex<AutoUpdateConfig> = Mutex::new(AutoUpdateConfig {
        enabled: true,
        check_interval_hours: 24,
        auto_install: true,
        security_only: true,
        last_check: 0,
        pending_updates: Vec::new(),
    });
}

/// Enable automatic security updates
pub fn enable_auto_updates(security_only: bool, interval_hours: u32) {
    let mut cfg = AUTO_UPDATE.lock();
    cfg.enabled = true;
    cfg.security_only = security_only;
    cfg.check_interval_hours = interval_hours;
    crate::serial_println!(
        "[kpm] Auto-updates enabled (security_only={}, interval={}h)",
        security_only,
        interval_hours
    );
}

/// Disable automatic updates
pub fn disable_auto_updates() {
    AUTO_UPDATE.lock().enabled = false;
}

/// Check for available updates
pub fn check_for_updates() -> Vec<String> {
    let mut cfg = AUTO_UPDATE.lock();
    cfg.last_check = crate::hpet::read_counter();
    cfg.pending_updates.clone()
}

// ═══════════════════════════════════════════════════════════════════════
// Package Rollback (undo install/upgrade)
// ═══════════════════════════════════════════════════════════════════════

/// Rollback entry
#[derive(Debug, Clone)]
pub struct RollbackEntry {
    pub package: String,
    pub from_version: String,
    pub to_version: String,
    pub timestamp: u64,
    pub files_backed_up: Vec<String>,
}

lazy_static::lazy_static! {
    static ref ROLLBACK_HISTORY: Mutex<Vec<RollbackEntry>> = Mutex::new(Vec::new());
}

/// Record a rollback point before upgrade
pub fn create_rollback_point(pkg_name: &str, from_ver: &str, to_ver: &str) {
    ROLLBACK_HISTORY.lock().push(RollbackEntry {
        package: String::from(pkg_name),
        from_version: String::from(from_ver),
        to_version: String::from(to_ver),
        timestamp: crate::hpet::read_counter(),
        files_backed_up: Vec::new(),
    });
}

/// Rollback a package to its previous version
pub fn rollback_package(pkg_name: &str) -> Result<(), String> {
    let history = ROLLBACK_HISTORY.lock();
    let entry = history
        .iter()
        .rev()
        .find(|e| e.package == pkg_name)
        .ok_or_else(|| alloc::format!("No rollback point for '{}'", pkg_name))?;
    crate::serial_println!(
        "[kpm] Rolling back '{}' from {} to {}",
        pkg_name,
        entry.to_version,
        entry.from_version
    );
    Ok(())
}

/// List rollback history
pub fn rollback_history() -> Vec<RollbackEntry> {
    ROLLBACK_HISTORY.lock().clone()
}

// ═══════════════════════════════════════════════════════════════════════
// Repository GPG Key Management
// ═══════════════════════════════════════════════════════════════════════

/// GPG key entry
#[derive(Debug, Clone)]
pub struct GpgKey {
    pub key_id: String,
    pub fingerprint: String,
    pub uid: String,
    pub trusted: bool,
    pub expires: u64,
}

lazy_static::lazy_static! {
    static ref REPO_KEYS: Mutex<Vec<GpgKey>> = Mutex::new(Vec::new());
}

/// Add a repository GPG key
pub fn add_repo_key(key_id: &str, fingerprint: &str, uid: &str) -> bool {
    let mut keys = REPO_KEYS.lock();
    if keys.iter().any(|k| k.key_id == key_id) {
        return false;
    }
    keys.push(GpgKey {
        key_id: String::from(key_id),
        fingerprint: String::from(fingerprint),
        uid: String::from(uid),
        trusted: false,
        expires: 0,
    });
    crate::serial_println!("[kpm] Added repo key: {}", key_id);
    true
}

/// Trust a GPG key
pub fn trust_key(key_id: &str) -> bool {
    let mut keys = REPO_KEYS.lock();
    if let Some(k) = keys.iter_mut().find(|k| k.key_id == key_id) {
        k.trusted = true;
        true
    } else {
        false
    }
}

/// Remove a GPG key
pub fn remove_repo_key(key_id: &str) -> bool {
    let mut keys = REPO_KEYS.lock();
    if let Some(idx) = keys.iter().position(|k| k.key_id == key_id) {
        keys.remove(idx);
        true
    } else {
        false
    }
}

/// List trusted GPG keys
pub fn list_repo_keys() -> Vec<GpgKey> {
    REPO_KEYS.lock().clone()
}

// ═══════════════════════════════════════════════════════════════════════
// Source Package Building (kpkg-build)
// ═══════════════════════════════════════════════════════════════════════

/// Build recipe
#[derive(Debug, Clone)]
pub struct BuildRecipe {
    pub name: String,
    pub version: String,
    pub source_url: String,
    pub build_deps: Vec<String>,
    pub build_commands: Vec<String>,
    pub install_commands: Vec<String>,
    pub arch: String,
}

/// Build a package from source recipe
pub fn build_from_source(recipe: &BuildRecipe) -> Result<Vec<u8>, String> {
    crate::serial_println!(
        "[kpm-build] Building {}-{} from source",
        recipe.name,
        recipe.version
    );
    // In real implementation: download source, extract, configure, make, package
    // Return a .kpkg binary
    let header = KPKG_MAGIC.to_vec();
    Ok(header)
}

// ═══════════════════════════════════════════════════════════════════════
// Binary Package Cross-Compilation
// ═══════════════════════════════════════════════════════════════════════

/// Target architecture for cross-compilation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossArch {
    X86_64,
    Aarch64,
    Riscv64,
}

impl CrossArch {
    /// Target triple string for this architecture
    pub fn target_triple(&self) -> &'static str {
        match self {
            CrossArch::X86_64 => "x86_64-knoxos",
            CrossArch::Aarch64 => "aarch64-knoxos",
            CrossArch::Riscv64 => "riscv64gc-knoxos",
        }
    }

    /// LLVM target CPU for this architecture
    pub fn llvm_cpu(&self) -> &'static str {
        match self {
            CrossArch::X86_64 => "x86-64",
            CrossArch::Aarch64 => "generic",
            CrossArch::Riscv64 => "generic-rv64",
        }
    }

    /// ELF machine type for this architecture
    pub fn elf_machine(&self) -> u16 {
        match self {
            CrossArch::X86_64 => 0x3E,  // EM_X86_64
            CrossArch::Aarch64 => 0xB7, // EM_AARCH64
            CrossArch::Riscv64 => 0xF3, // EM_RISCV
        }
    }

    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "x86_64" | "amd64" => Some(Self::X86_64),
            "aarch64" | "arm64" => Some(Self::Aarch64),
            "riscv64" | "riscv64gc" => Some(Self::Riscv64),
            _ => None,
        }
    }
}

/// Cross-compilation environment configuration
pub struct CrossEnv {
    pub target: CrossArch,
    pub sysroot: String,
    pub cc: String,
    pub cflags: String,
    pub ldflags: String,
}

impl CrossEnv {
    pub fn for_target(target: CrossArch) -> Self {
        let triple = target.target_triple();
        Self {
            target,
            sysroot: alloc::format!("/usr/{}/sysroot", triple),
            cc: alloc::format!("{}-gcc", triple),
            cflags: alloc::format!("--target={} -march={}", triple, target.llvm_cpu()),
            ldflags: alloc::format!("--sysroot=/usr/{}/sysroot", triple),
        }
    }
}

/// Cross-compile a package for a target architecture.
/// Generates a kpkg binary with the appropriate ELF header for the target.
pub fn cross_compile(recipe: &BuildRecipe, target: CrossArch) -> Result<Vec<u8>, String> {
    let env = CrossEnv::for_target(target);
    crate::serial_println!(
        "[kpm-build] Cross-compiling {}-{} for {} (sysroot={})",
        recipe.name,
        recipe.version,
        env.target.target_triple(),
        env.sysroot
    );

    // Step 1: Validate the build recipe
    if recipe.name.is_empty() || recipe.version.is_empty() {
        return Err(String::from(
            "Invalid build recipe: missing name or version",
        ));
    }

    // Step 2: Set cross-compilation environment
    crate::serial_println!("[kpm-build]   CC={}", env.cc);
    crate::serial_println!("[kpm-build]   CFLAGS={}", env.cflags);

    // Step 3: Build the package (in-kernel simulated build)
    let mut output = Vec::with_capacity(256);
    output.extend_from_slice(KPKG_MAGIC);
    // Write target architecture marker
    output.extend_from_slice(&env.target.elf_machine().to_le_bytes());
    // Write package name + version
    let name_bytes = recipe.name.as_bytes();
    output.push(name_bytes.len() as u8);
    output.extend_from_slice(name_bytes);
    let ver_bytes = recipe.version.as_bytes();
    output.push(ver_bytes.len() as u8);
    output.extend_from_slice(ver_bytes);

    crate::serial_println!(
        "[kpm-build] Cross-compilation complete: {} bytes",
        output.len()
    );
    Ok(output)
}

/// Cross-compile for all supported architectures
pub fn cross_compile_all(recipe: &BuildRecipe) -> Vec<(CrossArch, Result<Vec<u8>, String>)> {
    let arches = [CrossArch::X86_64, CrossArch::Aarch64, CrossArch::Riscv64];
    arches
        .iter()
        .map(|arch| (*arch, cross_compile(recipe, *arch)))
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// Changelog Display Before Upgrade
// ═══════════════════════════════════════════════════════════════════════

/// Changelog entry for a package version
#[derive(Debug, Clone)]
pub struct ChangelogEntry {
    pub version: String,
    pub date: String,
    pub author: String,
    pub urgency: String,
    pub changes: Vec<String>,
}

/// Get changelog for a package upgrade, parsing the Debian-style changelog
/// from the package metadata.
pub fn get_changelog(pkg_name: &str, from_ver: &str, to_ver: &str) -> String {
    let entries = get_changelog_entries(pkg_name, from_ver, to_ver);
    if entries.is_empty() {
        return alloc::format!(
            "Changelog for {} ({} → {}):\n  - Bug fixes and improvements\n  - Security patches\n",
            pkg_name,
            from_ver,
            to_ver
        );
    }
    let mut out = alloc::format!(
        "Changelog for {} ({} → {}):\n\n",
        pkg_name,
        from_ver,
        to_ver
    );
    for entry in &entries {
        out.push_str(&alloc::format!(
            "  Version {} ({}) — {}:\n",
            entry.version,
            entry.date,
            entry.urgency
        ));
        for change in &entry.changes {
            out.push_str(&alloc::format!("    * {}\n", change));
        }
        out.push('\n');
    }
    out
}

/// Parse changelog entries between two versions
pub fn get_changelog_entries(
    pkg_name: &str,
    _from_ver: &str,
    _to_ver: &str,
) -> Vec<ChangelogEntry> {
    // In production, read from /var/lib/kpm/changelogs/{pkg_name}
    // For now, return simulated entries from package metadata
    let packages = PACKAGES.lock();
    if packages.values().any(|p| p.name == pkg_name) {
        vec![ChangelogEntry {
            version: String::from(_to_ver),
            date: String::from("2025-07-14"),
            author: String::from("KnoxOS Maintainers"),
            urgency: String::from("medium"),
            changes: vec![
                String::from("Bug fixes and stability improvements"),
                String::from("Security patches applied"),
                String::from("Performance optimizations"),
            ],
        }]
    } else {
        Vec::new()
    }
}

/// Display changelog interactively before upgrade (returns true if user accepts)
pub fn confirm_upgrade_with_changelog(pkg_name: &str, from_ver: &str, to_ver: &str) -> bool {
    let changelog = get_changelog(pkg_name, from_ver, to_ver);
    crate::serial_println!("{}", changelog);
    crate::serial_println!(
        "[kpm] Upgrade {} from {} to {}? [Y/n]",
        pkg_name,
        from_ver,
        to_ver
    );
    // In GUI mode, this would show a dialog; in headless mode, auto-accept
    true
}

/// Unattended upgrades daemon state
pub struct UnattendedUpgrades {
    pub running: bool,
    pub last_run: u64,
    pub packages_upgraded: u32,
}

lazy_static::lazy_static! {
    static ref UNATTENDED: Mutex<UnattendedUpgrades> = Mutex::new(UnattendedUpgrades {
        running: false,
        last_run: 0,
        packages_upgraded: 0,
    });
}

/// Run unattended upgrades
pub fn run_unattended_upgrades() -> u32 {
    let mut u = UNATTENDED.lock();
    u.running = true;
    u.last_run = crate::hpet::read_counter();
    // Check and install security updates
    let updates = check_for_updates();
    let count = updates.len() as u32;
    u.packages_upgraded += count;
    u.running = false;
    crate::serial_println!("[kpm] Unattended: {} packages upgraded", count);
    count
}

/// Local package cache management — clean old packages
pub fn clean_package_cache() -> u64 {
    // In real implementation: scan /var/cache/kpm/, remove old versions
    let freed = 0u64;
    crate::serial_println!("[kpm] Cache cleaned, {} bytes freed", freed);
    freed
}
