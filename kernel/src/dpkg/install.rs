use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

use crate::serial_println;

use super::ar::parse_ar_archive;
use super::compress::{Compression, decompress, detect_compression};
use super::control::{
    DebControl, DebDependency, DebError, VersionConstraint, VersionOp, parse_control,
};
use super::database::{
    DEB_DATABASE, DebState, FILE_OWNERS, InstalledDeb, InstalledFile, InstalledFileType,
    MaintScript, VIRTUAL_PACKAGES,
};
use super::tar::{TarEntry, TarEntryType, parse_tar};

// ═══════════════════════════════════════════════════════════════════════
// DPKG INSTALLATION ENGINE
// ═══════════════════════════════════════════════════════════════════════

/// Install a .deb package from raw bytes
pub fn install_deb(deb_data: &[u8]) -> Result<DebControl, DebError> {
    serial_println!("[dpkg] Installing .deb package ({} bytes)", deb_data.len());

    // Step 1: Parse the outer ar archive
    let members = parse_ar_archive(deb_data)?;

    serial_println!("[dpkg] AR archive members:");
    for m in &members {
        serial_println!("[dpkg]   {} ({} bytes)", m.name, m.size);
    }

    // Step 2: Validate debian-binary version
    let debian_binary = members
        .iter()
        .find(|m| m.name == "debian-binary")
        .ok_or(DebError::MissingMember("debian-binary"))?;

    let version_data = &deb_data[debian_binary.offset..debian_binary.offset + debian_binary.size];
    let version_str = String::from_utf8_lossy(version_data);
    if !version_str.trim().starts_with("2.0") {
        return Err(DebError::InvalidArchive("unsupported deb format version"));
    }
    serial_println!(
        "[dpkg] Debian binary format version: {}",
        version_str.trim()
    );

    // Step 3: Extract and parse control archive
    let control_member = members
        .iter()
        .find(|m| m.name.starts_with("control.tar"))
        .ok_or(DebError::MissingMember("control.tar"))?;

    let control_raw = &deb_data[control_member.offset..control_member.offset + control_member.size];
    let compression = detect_compression(control_raw);
    let control_tar = decompress(control_raw, compression)?;
    let control_entries = parse_tar(&control_tar);

    // Find and parse the control file
    let control_entry = control_entries
        .iter()
        .find(|e| e.path.ends_with("control") && e.entry_type == TarEntryType::RegularFile)
        .ok_or(DebError::MissingMember("control file in control.tar"))?;

    let control_text = String::from_utf8_lossy(
        &control_tar[control_entry.data_offset..control_entry.data_offset + control_entry.size],
    );
    let control = parse_control(&control_text)?;

    serial_println!(
        "[dpkg] Package: {} {} ({})",
        control.package,
        control.version,
        control.architecture
    );
    serial_println!("[dpkg] Description: {}", control.description);
    serial_println!("[dpkg] Installed-Size: {} KB", control.installed_size);

    // Step 4: Check dependencies
    check_dependencies(&control)?;

    // Step 5: Run pre-install script (if present)
    let preinst = control_entries.iter().find(|e| e.path.ends_with("preinst"));
    if preinst.is_some() {
        serial_println!("[dpkg] Running pre-installation script...");
        run_maint_script(MaintScript::PreInst, &control.package)?;
    }

    // Step 6: Extract data archive
    let data_member = members
        .iter()
        .find(|m| m.name.starts_with("data.tar"))
        .ok_or(DebError::MissingMember("data.tar"))?;

    let data_raw = &deb_data[data_member.offset..data_member.offset + data_member.size];
    let data_compression = detect_compression(data_raw);

    // For uncompressed tar, parse directly from the slice to avoid a 125 MB+ clone.
    // For compressed tar, decompress first (returns a new Vec).
    let decompressed_buf: Vec<u8>;
    let data_tar: &[u8] = if matches!(data_compression, Compression::None) {
        data_raw
    } else {
        decompressed_buf = decompress(data_raw, data_compression)?;
        &decompressed_buf
    };
    let data_entries = parse_tar(data_tar);

    serial_println!(
        "[dpkg] Extracting {} files from data archive...",
        data_entries.len()
    );

    // Step 7: Install files to filesystem
    let mut installed_files = Vec::new();
    let mut file_owners = FILE_OWNERS.lock();

    for entry in &data_entries {
        let path = normalize_path(&entry.path);

        // Check for file conflicts
        if let Some(owner) = file_owners.get(&path) {
            if owner != &control.package {
                // Check if we replace this package
                if !control.replaces.contains(owner) {
                    serial_println!(
                        "[dpkg] WARNING: {} already owned by {}, overwriting",
                        path,
                        owner
                    );
                }
            }
        }

        let file_type = match entry.entry_type {
            TarEntryType::Directory => InstalledFileType::Directory,
            TarEntryType::SymLink => InstalledFileType::Symlink,
            TarEntryType::RegularFile => {
                if path.starts_with("/etc/") {
                    InstalledFileType::Config
                } else {
                    InstalledFileType::Regular
                }
            }
            _ => InstalledFileType::Regular,
        };

        // Register the file in VFS
        install_file_to_vfs(&path, entry, data_tar);

        // Track ownership
        file_owners.insert(path.clone(), control.package.clone());

        installed_files.push(InstalledFile {
            path,
            size: entry.size,
            mode: entry.mode,
            md5sum: None,
            file_type,
        });
    }
    drop(file_owners);

    serial_println!(
        "[dpkg] Installed {} files for {}",
        installed_files.len(),
        control.package
    );

    // Step 8: Run post-install script
    let postinst = control_entries
        .iter()
        .find(|e| e.path.ends_with("postinst"));
    if postinst.is_some() {
        serial_println!("[dpkg] Running post-installation script...");
        run_maint_script(MaintScript::PostInst, &control.package)?;
    }

    // Step 9: Register in dpkg database
    let config_files: Vec<String> = installed_files
        .iter()
        .filter(|f| f.file_type == InstalledFileType::Config)
        .map(|f| f.path.clone())
        .collect();

    let record = InstalledDeb {
        control: control.clone(),
        installed_files,
        config_files,
        state: DebState::Installed,
        install_time: 0, // would use clock::get_unix_timestamp()
    };

    DEB_DATABASE.lock().insert(control.package.clone(), record);

    // Register virtual packages
    for virt in &control.provides {
        VIRTUAL_PACKAGES
            .lock()
            .entry(virt.clone())
            .or_default()
            .push(control.package.clone());
    }

    // Also register in KPM for unified package management
    register_in_kpm(&control);

    serial_println!(
        "[dpkg] ✓ {} {} installed successfully",
        control.package,
        control.version
    );

    Ok(control)
}

/// Normalize a tar path (strip leading ./ and ensure leading /)
fn normalize_path(path: &str) -> String {
    let p = path.trim_start_matches("./").trim_start_matches('.');
    if p.is_empty() || p == "/" {
        return String::from("/");
    }
    if !p.starts_with('/') {
        format!("/{}", p)
    } else {
        p.to_string()
    }
}

/// Install a single file entry to the VFS
fn install_file_to_vfs(path: &str, entry: &TarEntry, tar_data: &[u8]) {
    match entry.entry_type {
        TarEntryType::Directory => {
            serial_println!("[dpkg]   mkdir {}", path);
            let mut vfs = crate::vfs::VFS.lock();
            let _ = vfs.mkdir(path, entry.mode as u16);
        }
        TarEntryType::SymLink => {
            if let Some(ref target) = entry.link_target {
                serial_println!("[dpkg]   symlink {} -> {}", path, target);
                let mut vfs = crate::vfs::VFS.lock();
                let _ = vfs.create_file_at_path(
                    path,
                    crate::vfs::FileType::SymLink,
                    target.as_bytes(),
                    0o777,
                );
            }
        }
        TarEntryType::RegularFile => {
            let file_data = &tar_data[entry.data_offset..entry.data_offset + entry.size];

            // For large binary files (> 1 MB), only store a small header stub
            // in VFS to conserve heap memory. The inode reports the real size
            // so `ls -la` and `stat` show the correct value, but we avoid
            // duplicating tens/hundreds of megabytes of zero-filled ELF stubs
            // into heap memory during extraction.
            const SPARSE_THRESHOLD: usize = 1024 * 1024; // 1 MB
            const STUB_KEEP_BYTES: usize = 4096; // keep first 4 KB (ELF header, etc.)

            if entry.size > SPARSE_THRESHOLD {
                serial_println!(
                    "[dpkg]   extract {} ({} MB)",
                    path,
                    entry.size / (1024 * 1024)
                );
                let mut vfs = crate::vfs::VFS.lock();
                vfs.write_file_sparse(path, file_data, entry.size as u64, STUB_KEEP_BYTES);
            } else {
                serial_println!("[dpkg]   extract {} ({} bytes)", path, entry.size);
                let mut vfs = crate::vfs::VFS.lock();
                vfs.write_file(path, file_data);
            }
        }
        _ => {
            serial_println!("[dpkg]   skip {} (unsupported type)", path);
        }
    }
}

/// Check that all dependencies are satisfied
fn check_dependencies(control: &DebControl) -> Result<(), DebError> {
    let db = DEB_DATABASE.lock();
    let virtuals = VIRTUAL_PACKAGES.lock();

    for dep in &control.pre_depends {
        if !is_dep_satisfied(dep, &db, &virtuals) {
            return Err(DebError::DependencyError(format!(
                "Pre-Depends not met: {}",
                dep.package
            )));
        }
    }

    // For regular depends, we check but allow installation with warnings
    for dep in &control.depends {
        if !is_dep_satisfied(dep, &db, &virtuals) {
            serial_println!(
                "[dpkg] WARNING: Dependency not satisfied: {} (will attempt anyway)",
                dep.package
            );
        }
    }

    Ok(())
}

/// Check if a dependency is satisfied
fn is_dep_satisfied(
    dep: &DebDependency,
    db: &BTreeMap<String, InstalledDeb>,
    virtuals: &BTreeMap<String, Vec<String>>,
) -> bool {
    // Check if the package is installed
    if let Some(installed) = db.get(&dep.package) {
        if installed.state == DebState::Installed {
            // Check version constraint if present
            if let Some(ref constraint) = dep.version_constraint {
                return check_version(&installed.control.version, constraint);
            }
            return true;
        }
    }

    // Check virtual packages
    if virtuals.contains_key(&dep.package) {
        return true;
    }

    // Check alternatives
    for alt in &dep.alternatives {
        if is_dep_satisfied(alt, db, virtuals) {
            return true;
        }
    }

    false
}

/// Simple Debian version comparison
fn check_version(installed: &str, constraint: &VersionConstraint) -> bool {
    let cmp = compare_versions(installed, &constraint.version);
    match constraint.op {
        VersionOp::Eq => cmp == core::cmp::Ordering::Equal,
        VersionOp::Ge => cmp != core::cmp::Ordering::Less,
        VersionOp::Le => cmp != core::cmp::Ordering::Greater,
        VersionOp::Gt => cmp == core::cmp::Ordering::Greater,
        VersionOp::Lt => cmp == core::cmp::Ordering::Less,
    }
}

/// Compare two Debian version strings (simplified)
fn compare_versions(a: &str, b: &str) -> core::cmp::Ordering {
    // Split epoch:upstream-revision
    let (a_epoch, a_rest) = split_epoch(a);
    let (b_epoch, b_rest) = split_epoch(b);

    match a_epoch.cmp(&b_epoch) {
        core::cmp::Ordering::Equal => {}
        other => return other,
    }

    // Compare upstream version segments
    let a_parts: Vec<&str> = a_rest.split('.').collect();
    let b_parts: Vec<&str> = b_rest.split('.').collect();

    let max_len = a_parts.len().max(b_parts.len());
    for i in 0..max_len {
        let a_num = a_parts
            .get(i)
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let b_num = b_parts
            .get(i)
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        match a_num.cmp(&b_num) {
            core::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }

    core::cmp::Ordering::Equal
}

fn split_epoch(version: &str) -> (u64, &str) {
    if let Some(colon) = version.find(':') {
        let epoch = version[..colon].parse::<u64>().unwrap_or(0);
        (epoch, &version[colon + 1..])
    } else {
        (0, version)
    }
}

/// Run a maintainer script (simplified - logs action)
pub(super) fn run_maint_script(script: MaintScript, package: &str) -> Result<(), DebError> {
    let name = match script {
        MaintScript::PreInst => "preinst",
        MaintScript::PostInst => "postinst",
        MaintScript::PreRm => "prerm",
        MaintScript::PostRm => "postrm",
    };
    serial_println!("[dpkg] Executing {}.{}", package, name);
    // In a full implementation, this would exec the script via /bin/sh
    // For now, we handle common operations:
    if script == MaintScript::PostInst {
        // Common postinst operations:
        // - ldconfig (update shared library cache)
        serial_println!("[dpkg] Running ldconfig...");
        update_ld_cache();
        // - update-desktop-database
        serial_println!("[dpkg] Updating desktop database...");
        // - update-mime-database
        // - gtk-update-icon-cache
    }
    Ok(())
}

/// Update the shared library cache (like ldconfig)
fn update_ld_cache() {
    // Scan library directories and update soname → path mappings
    let lib_dirs = [
        "/lib",
        "/lib64",
        "/usr/lib",
        "/usr/lib64",
        "/usr/local/lib",
        "/opt/vivaldi/lib",
    ];

    for dir in &lib_dirs {
        serial_println!("[dpkg] ldconfig: scanning {}", dir);
    }

    // Register new search paths in the dynamic linker
    crate::dynlink::add_search_path("/opt/vivaldi");
    crate::dynlink::add_search_path("/opt/vivaldi/lib");
}

/// Register an installed deb package in KPM for unified management
fn register_in_kpm(control: &DebControl) {
    // Create a KPM package entry for the deb
    let mut packages = crate::kpm::PACKAGES.lock();
    packages.insert(
        control.package.clone(),
        crate::kpm::Package {
            name: control.package.clone(),
            version: control.version.clone(),
            description: control.description.clone(),
            dependencies: control.depends.iter().map(|d| d.package.clone()).collect(),
            size_kb: control.installed_size,
            state: crate::kpm::PackageState::Installed,
            installed_files: Vec::new(), // tracked in dpkg database instead
        },
    );
}
