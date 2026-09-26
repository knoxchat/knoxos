use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

use crate::serial_println;

use super::ar::parse_ar_archive;
use super::compress::{Compression, decompress, detect_compression};
use super::control::{DebControl, DebError};
use super::database::{
    DEB_DATABASE, DebState, FILE_OWNERS, InstalledDeb, InstalledFileType, MaintScript,
};
use super::install::run_maint_script;
use super::tar::{TarEntryType, parse_tar};

// ═══════════════════════════════════════════════════════════════════════
// DPKG QUERY / STATUS
// ═══════════════════════════════════════════════════════════════════════

/// Query the dpkg database for a package
pub fn query(name: &str) -> Option<InstalledDeb> {
    DEB_DATABASE.lock().get(name).cloned()
}

/// List all installed deb packages
pub fn list_installed() -> Vec<InstalledDeb> {
    DEB_DATABASE.lock().values().cloned().collect()
}

/// Check if a package is installed
pub fn is_installed(name: &str) -> bool {
    DEB_DATABASE
        .lock()
        .get(name)
        .map(|p| p.state == DebState::Installed)
        .unwrap_or(false)
}

/// Get all files owned by a package
pub fn list_files(name: &str) -> Vec<String> {
    DEB_DATABASE
        .lock()
        .get(name)
        .map(|p| p.installed_files.iter().map(|f| f.path.clone()).collect())
        .unwrap_or_default()
}

/// Remove a deb package
pub fn remove_deb(name: &str, purge: bool) -> Result<(), DebError> {
    let mut db = DEB_DATABASE.lock();
    let pkg = db.get(name).ok_or(DebError::InstallError(format!(
        "Package {} not found",
        name
    )))?;

    if pkg.state != DebState::Installed && pkg.state != DebState::ConfigFiles {
        return Err(DebError::InstallError(format!(
            "Package {} is not installed",
            name
        )));
    }

    serial_println!("[dpkg] Removing {}...", name);

    // Run prerm script
    run_maint_script(MaintScript::PreRm, name)?;

    // Remove files (except config files unless purging)
    let mut file_owners = FILE_OWNERS.lock();
    if let Some(pkg) = db.get(name) {
        for file in &pkg.installed_files {
            if !purge && file.file_type == InstalledFileType::Config {
                continue;
            }
            file_owners.remove(&file.path);
            serial_println!("[dpkg]   rm {}", file.path);
        }
    }
    drop(file_owners);

    // Run postrm script
    run_maint_script(MaintScript::PostRm, name)?;

    if purge {
        db.remove(name);
    } else {
        if let Some(pkg) = db.get_mut(name) {
            pkg.state = DebState::ConfigFiles;
            pkg.installed_files
                .retain(|f| f.file_type == InstalledFileType::Config);
        }
    }

    serial_println!(
        "[dpkg] ✓ {} removed{}",
        name,
        if purge { " (purged)" } else { "" }
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// QUERY API (for shell commands)
// ═══════════════════════════════════════════════════════════════════════

/// Count total installed files across all packages
pub fn installed_file_count() -> usize {
    FILE_OWNERS.lock().len()
}

/// Register a virtual .deb package (not from an actual .deb file)
pub fn register_virtual_deb(
    name: &str,
    version: &str,
    arch: &str,
    description: &str,
    installed_size: u64,
) {
    use alloc::vec;

    let mut db = DEB_DATABASE.lock();
    if db.contains_key(name) {
        return;
    }

    let control = DebControl {
        package: name.to_string(),
        version: version.to_string(),
        architecture: arch.to_string(),
        description: description.to_string(),
        maintainer: String::from("KnoxOS dpkg"),
        installed_size,
        depends: Vec::new(),
        pre_depends: Vec::new(),
        recommends: Vec::new(),
        suggests: Vec::new(),
        conflicts: Vec::new(),
        replaces: Vec::new(),
        provides: vec![String::from("www-browser")],
        section: String::from("web"),
        priority: String::from("optional"),
        homepage: String::from("https://vivaldi.com"),
        extra_fields: BTreeMap::new(),
    };

    let installed_pkg = InstalledDeb {
        control,
        state: DebState::Installed,
        installed_files: Vec::new(),
        config_files: Vec::new(),
        install_time: 0,
    };

    db.insert(String::from(name), installed_pkg);

    // Also register in kpm
    let mut packages = crate::kpm::PACKAGES.lock();
    packages.insert(
        name.to_string(),
        crate::kpm::Package {
            name: name.to_string(),
            version: version.to_string(),
            description: description.to_string(),
            dependencies: Vec::new(),
            size_kb: installed_size / 1024,
            state: crate::kpm::PackageState::Installed,
            installed_files: Vec::new(),
        },
    );

    serial_println!("[dpkg] Registered virtual package: {} {}", name, version);
}

/// List packages (optionally filtered by pattern)
/// Returns: Vec<(name, version, arch, description)>
pub fn list_packages(pattern: Option<&str>) -> alloc::vec::Vec<(String, String, String, String)> {
    let db = DEB_DATABASE.lock();
    let mut results = alloc::vec::Vec::new();

    for (name, pkg) in db.iter() {
        if pkg.state != DebState::Installed && pkg.state != DebState::ConfigFiles {
            continue;
        }
        if let Some(pat) = pattern {
            if !name.contains(pat) && !pkg.control.description.contains(pat) {
                continue;
            }
        }
        results.push((
            pkg.control.package.clone(),
            pkg.control.version.clone(),
            pkg.control.architecture.clone(),
            pkg.control.description.clone(),
        ));
    }

    results
}

/// Query status of a specific package (dpkg -s output format)
pub fn query_status(name: &str) -> Option<String> {
    let db = DEB_DATABASE.lock();
    let pkg = db.get(name)?;

    let status_str = match pkg.state {
        DebState::Installed => "install ok installed",
        DebState::ConfigFiles => "deinstall ok config-files",
        DebState::HalfInstalled => "install reinstreq half-installed",
        DebState::Unpacked => "install ok unpacked",
        _ => "unknown ok not-installed",
    };

    let mut out = String::new();
    use core::fmt::Write;
    writeln!(out, "Package: {}", pkg.control.package).unwrap();
    writeln!(out, "Status: {}", status_str).unwrap();
    writeln!(out, "Priority: {}", pkg.control.priority).unwrap();
    writeln!(out, "Section: {}", pkg.control.section).unwrap();
    writeln!(out, "Installed-Size: {}", pkg.control.installed_size).unwrap();
    writeln!(out, "Maintainer: {}", pkg.control.maintainer).unwrap();
    writeln!(out, "Architecture: {}", pkg.control.architecture).unwrap();
    writeln!(out, "Version: {}", pkg.control.version).unwrap();
    if !pkg.control.depends.is_empty() {
        let dep_strs: alloc::vec::Vec<String> = pkg
            .control
            .depends
            .iter()
            .map(|d| {
                if let Some(ref ver) = d.version_constraint {
                    alloc::format!("{} ({:?})", d.package, ver)
                } else {
                    d.package.clone()
                }
            })
            .collect();
        writeln!(out, "Depends: {}", dep_strs.join(", ")).unwrap();
    }
    writeln!(out, "Description: {}", pkg.control.description).unwrap();
    if !pkg.control.homepage.is_empty() {
        writeln!(out, "Homepage: {}", pkg.control.homepage).unwrap();
    }

    Some(out)
}

/// List files installed by a package (dpkg -L)
pub fn list_package_files(name: &str) -> Option<alloc::vec::Vec<String>> {
    let db = DEB_DATABASE.lock();
    let pkg = db.get(name)?;
    if pkg.state != DebState::Installed {
        return None;
    }
    Some(pkg.installed_files.iter().map(|f| f.path.clone()).collect())
}

/// Inspect a .deb file and return human-readable info (dpkg --info)
pub fn inspect_deb(deb_data: &[u8]) -> Result<String, DebError> {
    let members = parse_ar_archive(deb_data)?;

    let control_member = members
        .iter()
        .find(|m| m.name.starts_with("control.tar"))
        .ok_or(DebError::MissingMember("control.tar"))?;

    let control_raw = &deb_data[control_member.offset..control_member.offset + control_member.size];
    let compression = detect_compression(control_raw);
    let control_tar = decompress(control_raw, compression)?;
    let control_entries = parse_tar(&control_tar);

    let control_entry = control_entries
        .iter()
        .find(|e| e.path.ends_with("control") && e.entry_type == TarEntryType::RegularFile)
        .ok_or(DebError::MissingMember("control"))?;

    let control_text = String::from_utf8_lossy(
        &control_tar[control_entry.data_offset..control_entry.data_offset + control_entry.size],
    );

    let mut output = String::from(" new Debian package, version 2.0.\n");
    for line in control_text.lines() {
        output.push(' ');
        output.push_str(line);
        output.push('\n');
    }

    Ok(output)
}

/// List contents of a .deb file (dpkg --contents)
pub fn list_deb_contents(deb_data: &[u8]) -> Result<String, DebError> {
    let members = parse_ar_archive(deb_data)?;

    let data_member = members
        .iter()
        .find(|m| m.name.starts_with("data.tar"))
        .ok_or(DebError::MissingMember("data.tar"))?;

    let data_raw = &deb_data[data_member.offset..data_member.offset + data_member.size];
    let compression = detect_compression(data_raw);
    let decompressed_buf2: Vec<u8>;
    let data_tar: &[u8] = if matches!(compression, Compression::None) {
        data_raw
    } else {
        decompressed_buf2 = decompress(data_raw, compression)?;
        &decompressed_buf2
    };
    let entries = parse_tar(data_tar);

    let mut output = String::new();
    use core::fmt::Write;
    for entry in &entries {
        let type_char = match entry.entry_type {
            TarEntryType::Directory => 'd',
            TarEntryType::SymLink => 'l',
            _ => '-',
        };
        writeln!(
            output,
            "{}{:<10} root/root {:>8} {}",
            type_char, "rwxr-xr-x", entry.size, entry.path
        )
        .unwrap();
    }

    Ok(output)
}
