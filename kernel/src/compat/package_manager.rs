/// Package Manager — software package management for KnoxOS
///
/// Provides:
///   - Package metadata (name, version, dependencies)
///   - Dependency resolution with topological sort
///   - Package installation/removal
///   - Repository management (local + remote)
///   - Version comparison (semver)
///   - Auto-update checking
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// VERSION
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        Some(Self {
            major: parts[0].parse().ok()?,
            minor: parts[1].parse().ok()?,
            patch: parts[2].parse().ok()?,
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
    }
}

impl core::fmt::Display for Version {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PACKAGE
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageState {
    Available,
    Installed,
    UpdateAvailable,
    Broken,
}

#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub min_version: Option<Version>,
}

#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub version: Version,
    pub description: String,
    pub dependencies: Vec<Dependency>,
    pub size_bytes: u64,
    pub installed_files: Vec<String>,
    pub state: PackageState,
}

impl Package {
    pub fn new(name: &str, version: Version, description: &str) -> Self {
        Self {
            name: String::from(name),
            version,
            description: String::from(description),
            dependencies: Vec::new(),
            size_bytes: 0,
            installed_files: Vec::new(),
            state: PackageState::Available,
        }
    }

    pub fn add_dependency(&mut self, name: &str, min_version: Option<Version>) {
        self.dependencies.push(Dependency {
            name: String::from(name),
            min_version,
        });
    }
}

// ═══════════════════════════════════════════════════════════════════════
// REPOSITORY
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct Repository {
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub packages: Vec<Package>,
    pub last_updated: u64,
}

impl Repository {
    pub fn new(name: &str, url: &str) -> Self {
        Self {
            name: String::from(name),
            url: String::from(url),
            enabled: true,
            packages: Vec::new(),
            last_updated: 0,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PACKAGE MANAGER
// ═══════════════════════════════════════════════════════════════════════

pub struct PackageManager {
    /// Installed packages
    installed: BTreeMap<String, Package>,
    /// Available repositories
    repositories: Vec<Repository>,
    /// Package install root directory
    pub install_root: String,
}

impl PackageManager {
    pub fn new() -> Self {
        Self {
            installed: BTreeMap::new(),
            repositories: Vec::new(),
            install_root: String::from("/usr"),
        }
    }

    /// Add a repository
    pub fn add_repository(&mut self, repo: Repository) {
        serial_println!("[PkgMgr] Added repository: {}", repo.name);
        self.repositories.push(repo);
    }

    /// Remove a repository by name
    pub fn remove_repository(&mut self, name: &str) -> bool {
        let before = self.repositories.len();
        self.repositories.retain(|r| r.name != name);
        self.repositories.len() < before
    }

    /// Search for a package across all enabled repositories
    pub fn search(&self, query: &str) -> Vec<&Package> {
        let mut results = Vec::new();
        let q = query.to_ascii_lowercase();
        for repo in &self.repositories {
            if !repo.enabled {
                continue;
            }
            for pkg in &repo.packages {
                if pkg.name.contains(query) || pkg.description.to_ascii_lowercase().contains(&q) {
                    results.push(pkg);
                }
            }
        }
        results
    }

    /// Resolve dependencies for a package (topological sort)
    pub fn resolve_dependencies(&self, package_name: &str) -> Result<Vec<String>, String> {
        let mut order = Vec::new();
        let mut visited = Vec::new();
        let mut in_progress = Vec::new();

        self.resolve_recursive(package_name, &mut order, &mut visited, &mut in_progress)?;
        Ok(order)
    }

    fn resolve_recursive(
        &self,
        name: &str,
        order: &mut Vec<String>,
        visited: &mut Vec<String>,
        in_progress: &mut Vec<String>,
    ) -> Result<(), String> {
        let name_s = String::from(name);

        if in_progress.contains(&name_s) {
            return Err(alloc::format!("Circular dependency: {}", name));
        }
        if visited.contains(&name_s) {
            return Ok(());
        }

        in_progress.push(name_s.clone());

        // Find the package in repositories
        if let Some(pkg) = self.find_package(name) {
            for dep in &pkg.dependencies {
                // Skip already-installed deps that meet version requirements
                if let Some(installed) = self.installed.get(&dep.name) {
                    if let Some(ref min_ver) = dep.min_version {
                        if installed.version >= *min_ver {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
                self.resolve_recursive(&dep.name, order, visited, in_progress)?;
            }
        }

        in_progress.retain(|x| x != name);
        visited.push(name_s.clone());

        // Don't add already-installed packages to install order
        if !self.installed.contains_key(name) {
            order.push(name_s);
        }

        Ok(())
    }

    /// Find a package in repositories
    fn find_package(&self, name: &str) -> Option<Package> {
        for repo in &self.repositories {
            if !repo.enabled {
                continue;
            }
            for pkg in &repo.packages {
                if pkg.name == name {
                    return Some(pkg.clone());
                }
            }
        }
        None
    }

    /// Install a package (with dependency resolution)
    pub fn install(&mut self, name: &str) -> Result<(), String> {
        // Check if already installed
        if self.installed.contains_key(name) {
            return Err(alloc::format!("Package '{}' is already installed", name));
        }

        // Resolve dependencies
        let install_order = self.resolve_dependencies(name)?;

        serial_println!("[PkgMgr] Install order: {:?}", install_order);

        for pkg_name in &install_order {
            if let Some(mut pkg) = self.find_package(pkg_name) {
                serial_println!("[PkgMgr] Installing {} v{}", pkg.name, pkg.version);
                pkg.state = PackageState::Installed;
                self.installed.insert(pkg.name.clone(), pkg);
            } else {
                return Err(alloc::format!("Package '{}' not found", pkg_name));
            }
        }

        Ok(())
    }

    /// Remove a package
    pub fn remove(&mut self, name: &str) -> Result<(), String> {
        // Check reverse dependencies
        for pkg in self.installed.values() {
            for dep in &pkg.dependencies {
                if dep.name == name {
                    return Err(alloc::format!(
                        "Cannot remove '{}': required by '{}'",
                        name,
                        pkg.name
                    ));
                }
            }
        }

        if self.installed.remove(name).is_some() {
            serial_println!("[PkgMgr] Removed package: {}", name);
            Ok(())
        } else {
            Err(alloc::format!("Package '{}' is not installed", name))
        }
    }

    /// Check for available updates
    pub fn check_updates(&self) -> Vec<(String, Version, Version)> {
        let mut updates = Vec::new();
        for (name, installed) in &self.installed {
            if let Some(available) = self.find_package(name) {
                if available.version > installed.version {
                    updates.push((
                        name.clone(),
                        installed.version.clone(),
                        available.version.clone(),
                    ));
                }
            }
        }
        updates
    }

    /// Upgrade a specific package
    pub fn upgrade(&mut self, name: &str) -> Result<(), String> {
        if let Some(available) = self.find_package(name) {
            if let Some(installed) = self.installed.get(name) {
                if available.version <= installed.version {
                    return Err(alloc::format!("'{}' is already up to date", name));
                }
            }
            let mut pkg = available;
            pkg.state = PackageState::Installed;
            self.installed.insert(pkg.name.clone(), pkg);
            serial_println!("[PkgMgr] Upgraded: {}", name);
            Ok(())
        } else {
            Err(alloc::format!("Package '{}' not found", name))
        }
    }

    /// List installed packages
    pub fn list_installed(&self) -> Vec<&Package> {
        self.installed.values().collect()
    }

    /// Get package info
    pub fn get_info(&self, name: &str) -> Option<&Package> {
        self.installed.get(name).or_else(|| {
            for repo in &self.repositories {
                for pkg in &repo.packages {
                    if pkg.name == name {
                        return Some(pkg);
                    }
                }
            }
            None
        })
    }
}

lazy_static::lazy_static! {
    pub static ref PKG_MANAGER: Mutex<PackageManager> = Mutex::new(PackageManager::new());
}

/// Initialize package manager with default repository
pub fn init() {
    let mut mgr = PKG_MANAGER.lock();
    let mut default_repo = Repository::new("knoxos-core", "https://packages.knoxos.dev/core");

    // Add some built-in system packages
    let mut base = Package::new("knoxos-base", Version::new(1, 0, 0), "KnoxOS base system");
    base.state = PackageState::Installed;
    mgr.installed.insert(base.name.clone(), base);

    default_repo.packages.push(Package::new(
        "knoxos-desktop",
        Version::new(1, 0, 0),
        "KnoxOS desktop environment",
    ));
    default_repo.packages.push(Package::new(
        "knoxos-utils",
        Version::new(1, 0, 0),
        "KnoxOS utility programs",
    ));

    mgr.add_repository(default_repo);
    drop(mgr);

    serial_println!("[KnoxOS] Package manager initialized");
}
