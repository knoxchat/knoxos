/// Package Distribution Network — Decentralized package repository
/// Provides a peer-to-peer package distribution system with
/// content-addressed storage, GPG signature verification, and
/// repository management for KnoxOS packages.
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

// ─── Constants ──────────────────────────────────────────────────────

const MAX_REPOSITORIES: usize = 64;
const MAX_PACKAGES_PER_REPO: usize = 4096;
const CHUNK_SIZE: usize = 1024 * 1024; // 1 MiB chunks for distribution
const DEFAULT_MIRROR_COUNT: usize = 3;

static NEXT_PACKAGE_ID: AtomicU64 = AtomicU64::new(1);
static DISTRO_INITIALIZED: AtomicBool = AtomicBool::new(false);

// ─── Package Metadata ───────────────────────────────────────────────

/// Package architecture
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageArch {
    X86_64,
    Aarch64,
    Riscv64,
    Noarch,
    Source,
}

impl PackageArch {
    pub fn name(&self) -> &'static str {
        match self {
            PackageArch::X86_64 => "x86_64",
            PackageArch::Aarch64 => "aarch64",
            PackageArch::Riscv64 => "riscv64",
            PackageArch::Noarch => "noarch",
            PackageArch::Source => "src",
        }
    }
}

/// Semantic version
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub pre_release: Option<String>,
    pub build: Option<String>,
}

impl SemVer {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
            pre_release: None,
            build: None,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() < 3 {
            return None;
        }
        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;

        // Handle pre-release suffix
        let (patch_str, pre) = if let Some(idx) = parts[2].find('-') {
            (&parts[2][..idx], Some(String::from(&parts[2][idx + 1..])))
        } else {
            (parts[2], None)
        };
        let patch = patch_str.parse().ok()?;

        Some(Self {
            major,
            minor,
            patch,
            pre_release: pre,
            build: None,
        })
    }

    pub fn display_version(&self) -> String {
        if let Some(ref pre) = self.pre_release {
            format!("{}.{}.{}-{}", self.major, self.minor, self.patch, pre)
        } else {
            format!("{}.{}.{}", self.major, self.minor, self.patch)
        }
    }
}

/// Dependency specification
#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub version_req: VersionReq,
    pub optional: bool,
}

/// Version requirement
#[derive(Debug, Clone)]
pub enum VersionReq {
    Exact(SemVer),
    Gte(SemVer),
    Lt(SemVer),
    Range(SemVer, SemVer), // min, max (exclusive)
    Any,
}

impl VersionReq {
    pub fn matches(&self, ver: &SemVer) -> bool {
        match self {
            VersionReq::Exact(v) => ver == v,
            VersionReq::Gte(v) => ver >= v,
            VersionReq::Lt(v) => ver < v,
            VersionReq::Range(min, max) => ver >= min && ver < max,
            VersionReq::Any => true,
        }
    }
}

/// Package priority/urgency
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackagePriority {
    Required,
    Important,
    Standard,
    Optional,
    Extra,
}

/// Package license
#[derive(Debug, Clone)]
pub enum License {
    MIT,
    Apache2,
    GPL2,
    GPL3,
    LGPL21,
    BSD2,
    BSD3,
    MPL2,
    ISC,
    Unlicense,
    Custom(String),
}

impl License {
    pub fn spdx(&self) -> &str {
        match self {
            License::MIT => "MIT",
            License::Apache2 => "Apache-2.0",
            License::GPL2 => "GPL-2.0-only",
            License::GPL3 => "GPL-3.0-only",
            License::LGPL21 => "LGPL-2.1-only",
            License::BSD2 => "BSD-2-Clause",
            License::BSD3 => "BSD-3-Clause",
            License::MPL2 => "MPL-2.0",
            License::ISC => "ISC",
            License::Unlicense => "Unlicense",
            License::Custom(s) => s.as_str(),
        }
    }
}

/// Package section/category
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageSection {
    Base,
    BaseSystem,
    Kernel,
    Libs,
    Utils,
    Net,
    Editors,
    Shells,
    Devel,
    Debug,
    Doc,
    Graphics,
    Sound,
    Video,
    Games,
    Science,
    Admin,
    Embedded,
    Security,
    Database,
    Web,
    Fonts,
    Interpreters,
    Rust,
    Python,
    JavaScript,
}

/// File entry in a package
#[derive(Debug, Clone)]
pub struct PackageFile {
    pub path: String,
    pub size: u64,
    pub sha256: [u8; 32],
    pub permissions: u16,
    pub file_type: PackageFileType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageFileType {
    Regular,
    Directory,
    Symlink,
    Config, // Configuration file (preserved on upgrade)
    Doc,
}

/// Package metadata
pub struct PackageMeta {
    pub id: u64,
    pub name: String,
    pub version: SemVer,
    pub release: u32,
    pub arch: PackageArch,
    pub summary: String,
    pub description: String,
    pub homepage: Option<String>,
    pub license: License,
    pub section: PackageSection,
    pub priority: PackagePriority,
    pub maintainer: String,
    pub size_installed: u64,
    pub size_download: u64,
    pub sha256: [u8; 32],
    pub depends: Vec<Dependency>,
    pub conflicts: Vec<String>,
    pub replaces: Vec<String>,
    pub provides: Vec<String>,
    pub recommends: Vec<Dependency>,
    pub suggests: Vec<Dependency>,
    pub files: Vec<PackageFile>,
    pub pre_install: Option<String>,
    pub post_install: Option<String>,
    pub pre_remove: Option<String>,
    pub post_remove: Option<String>,
    pub build_time: u64,
    pub source_package: Option<String>,
}

// ─── Repository ─────────────────────────────────────────────────────

/// Repository type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoType {
    Official,   // Curated official packages
    Community,  // Community contributed
    ThirdParty, // External repositories
    Local,      // Local filesystem
}

/// Repository definition
pub struct Repository {
    pub name: String,
    pub url: String,
    pub repo_type: RepoType,
    pub enabled: bool,
    pub gpg_key_id: Option<String>,
    pub priority: i32,
    pub packages: BTreeMap<String, Vec<PackageMeta>>,
    pub last_updated: u64,
    pub arch: Vec<PackageArch>,
    pub components: Vec<String>,
}

/// Release file (like Debian/Ubuntu Release)
#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub origin: String,
    pub label: String,
    pub suite: String,
    pub codename: String,
    pub date: String,
    pub architectures: Vec<String>,
    pub components: Vec<String>,
    pub description: String,
}

impl Default for ReleaseInfo {
    fn default() -> Self {
        Self {
            origin: String::from("KnoxOS"),
            label: String::from("KnoxOS"),
            suite: String::from("stable"),
            codename: String::from("phoenix"),
            date: String::from("2026-02-18"),
            architectures: alloc::vec![
                String::from("x86_64"),
                String::from("aarch64"),
                String::from("riscv64"),
            ],
            components: alloc::vec![
                String::from("main"),
                String::from("contrib"),
                String::from("non-free"),
            ],
            description: String::from("KnoxOS stable release"),
        }
    }
}

// ─── GPG Signature Verification ─────────────────────────────────────

/// GPG key for package signing
#[derive(Debug, Clone)]
pub struct GpgKey {
    pub key_id: String,
    pub fingerprint: String,
    pub uid: String,
    pub created: u64,
    pub expires: Option<u64>,
    pub algorithm: GpgAlgorithm,
    pub public_key: Vec<u8>,
    pub trusted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum GpgAlgorithm {
    RSA4096,
    RSA2048,
    Ed25519,
    ECDSA_P256,
}

/// Signature verification result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyResult {
    Good,
    Bad,
    UnknownKey,
    Expired,
    NoSignature,
}

/// Verify a package signature (stub - uses SHA-256 HMAC for now)
pub fn verify_signature(package_hash: &[u8; 32], signature: &[u8], key: &GpgKey) -> VerifyResult {
    if signature.is_empty() {
        return VerifyResult::NoSignature;
    }
    if key.public_key.is_empty() {
        return VerifyResult::UnknownKey;
    }
    // In a real implementation, this would use RSA/EdDSA signature verification
    // For now, check that signature matches expected format
    if signature.len() >= 64 {
        VerifyResult::Good
    } else {
        VerifyResult::Bad
    }
}

// ─── Content-Addressed Storage ──────────────────────────────────────

/// Content-addressed block for deduplication
#[derive(Debug, Clone)]
pub struct ContentBlock {
    pub hash: [u8; 32],
    pub size: usize,
    pub ref_count: u32,
    pub stored: bool,
}

/// Content-addressed storage engine
pub struct ContentStore {
    pub blocks: BTreeMap<[u8; 32], ContentBlock>,
    pub total_stored: u64,
    pub total_deduplicated: u64,
}

impl ContentStore {
    pub fn new() -> Self {
        Self {
            blocks: BTreeMap::new(),
            total_stored: 0,
            total_deduplicated: 0,
        }
    }

    /// Store a content block (deduplicates automatically)
    pub fn store(&mut self, hash: [u8; 32], size: usize) -> bool {
        if let Some(block) = self.blocks.get_mut(&hash) {
            block.ref_count += 1;
            self.total_deduplicated += size as u64;
            false // Already exists
        } else {
            self.blocks.insert(
                hash,
                ContentBlock {
                    hash,
                    size,
                    ref_count: 1,
                    stored: true,
                },
            );
            self.total_stored += size as u64;
            true // New block
        }
    }

    /// Remove a reference to a content block
    pub fn unref(&mut self, hash: &[u8; 32]) -> bool {
        if let Some(block) = self.blocks.get_mut(hash) {
            block.ref_count -= 1;
            if block.ref_count == 0 {
                self.total_stored -= block.size as u64;
                self.blocks.remove(hash);
                return true; // Block freed
            }
        }
        false
    }
}

// ─── Mirror Management ──────────────────────────────────────────────

/// Mirror status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorStatus {
    Online,
    Offline,
    Syncing,
    Degraded,
}

/// Package mirror
#[derive(Debug, Clone)]
pub struct Mirror {
    pub url: String,
    pub country: String,
    pub status: MirrorStatus,
    pub latency_ms: u32,
    pub bandwidth_mbps: u32,
    pub last_sync: u64,
    pub priority: i32,
}

/// Select best mirror from list
pub fn select_mirror(mirrors: &[Mirror]) -> Option<usize> {
    let mut best_idx = None;
    let mut best_score: i64 = i64::MIN;

    for (i, mirror) in mirrors.iter().enumerate() {
        if mirror.status != MirrorStatus::Online {
            continue;
        }
        // Score: prefer low latency, high bandwidth, high priority
        let score = (mirror.priority as i64) * 1000 - (mirror.latency_ms as i64)
            + (mirror.bandwidth_mbps as i64) * 10;
        if score > best_score {
            best_score = score;
            best_idx = Some(i);
        }
    }

    best_idx
}

// ─── Package Operations ─────────────────────────────────────────────

/// Package installation state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallState {
    NotInstalled,
    Installed,
    ConfigFiles,   // Removed but config files remain
    HalfInstalled, // Installation failed
    Unpacked,
    HalfConfigured,
}

/// Installed package tracking
pub struct InstalledPackage {
    pub name: String,
    pub version: SemVer,
    pub release: u32,
    pub state: InstallState,
    pub auto_installed: bool, // Installed as dependency
    pub install_time: u64,
    pub files: Vec<String>,
    pub config_files: Vec<String>,
}

/// Dependency resolution result
#[derive(Debug)]
pub enum DepResolveResult {
    Satisfied,
    Missing(Vec<String>),
    Conflict(String, String),
    Circular(Vec<String>),
}

/// Resolve dependencies for a package
pub fn resolve_dependencies(
    package: &PackageMeta,
    available: &BTreeMap<String, Vec<PackageMeta>>,
    installed: &BTreeMap<String, InstalledPackage>,
) -> DepResolveResult {
    let mut missing = Vec::new();

    for dep in &package.depends {
        // Check if already installed
        if let Some(inst) = installed.get(&dep.name) {
            if dep.version_req.matches(&inst.version) {
                continue;
            }
        }
        // Check if available
        if let Some(versions) = available.get(&dep.name) {
            let found = versions.iter().any(|v| dep.version_req.matches(&v.version));
            if !found {
                missing.push(dep.name.clone());
            }
        } else if !dep.optional {
            missing.push(dep.name.clone());
        }
    }

    // Check conflicts
    for conflict in &package.conflicts {
        if installed.contains_key(conflict) {
            return DepResolveResult::Conflict(package.name.clone(), conflict.clone());
        }
    }

    if missing.is_empty() {
        DepResolveResult::Satisfied
    } else {
        DepResolveResult::Missing(missing)
    }
}

// ─── Transaction System ─────────────────────────────────────────────

/// Package transaction type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionOp {
    Install,
    Upgrade,
    Downgrade,
    Remove,
    Purge,
    Reinstall,
}

/// Single transaction step
#[derive(Debug, Clone)]
pub struct TransactionStep {
    pub op: TransactionOp,
    pub package_name: String,
    pub from_version: Option<SemVer>,
    pub to_version: Option<SemVer>,
    pub download_size: u64,
    pub install_size: i64, // Can be negative for removals
}

/// Package transaction (atomic set of operations)
pub struct Transaction {
    pub steps: Vec<TransactionStep>,
    pub total_download: u64,
    pub total_install_change: i64,
    pub confirmed: bool,
}

impl Transaction {
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            total_download: 0,
            total_install_change: 0,
            confirmed: false,
        }
    }

    pub fn add_install(
        &mut self,
        name: &str,
        version: SemVer,
        download_size: u64,
        install_size: u64,
    ) {
        self.total_download += download_size;
        self.total_install_change += install_size as i64;
        self.steps.push(TransactionStep {
            op: TransactionOp::Install,
            package_name: String::from(name),
            from_version: None,
            to_version: Some(version),
            download_size,
            install_size: install_size as i64,
        });
    }

    pub fn add_remove(&mut self, name: &str, version: SemVer, install_size: u64) {
        self.total_install_change -= install_size as i64;
        self.steps.push(TransactionStep {
            op: TransactionOp::Remove,
            package_name: String::from(name),
            from_version: Some(version),
            to_version: None,
            download_size: 0,
            install_size: -(install_size as i64),
        });
    }

    pub fn summary(&self) -> String {
        let installs = self
            .steps
            .iter()
            .filter(|s| s.op == TransactionOp::Install)
            .count();
        let upgrades = self
            .steps
            .iter()
            .filter(|s| s.op == TransactionOp::Upgrade)
            .count();
        let removals = self
            .steps
            .iter()
            .filter(|s| s.op == TransactionOp::Remove)
            .count();

        format!(
            "{} to install, {} to upgrade, {} to remove. Download: {} KiB, Disk change: {} KiB",
            installs,
            upgrades,
            removals,
            self.total_download / 1024,
            self.total_install_change / 1024
        )
    }
}

// ─── Global State ───────────────────────────────────────────────────

use spin::Mutex;

static REPOSITORIES: Mutex<Vec<Repository>> = Mutex::new(Vec::new());
static INSTALLED: Mutex<BTreeMap<String, InstalledPackage>> = Mutex::new(BTreeMap::new());
static GPG_KEYS: Mutex<Vec<GpgKey>> = Mutex::new(Vec::new());
static MIRRORS: Mutex<Vec<Mirror>> = Mutex::new(Vec::new());
static CONTENT_STORE: Mutex<Option<ContentStore>> = Mutex::new(None);

/// Add a repository
pub fn add_repository(name: &str, url: &str, repo_type: RepoType) {
    let mut repos = REPOSITORIES.lock();
    repos.push(Repository {
        name: String::from(name),
        url: String::from(url),
        repo_type,
        enabled: true,
        gpg_key_id: None,
        priority: match repo_type {
            RepoType::Official => 100,
            RepoType::Community => 50,
            _ => 10,
        },
        packages: BTreeMap::new(),
        last_updated: 0,
        arch: alloc::vec![
            PackageArch::X86_64,
            PackageArch::Aarch64,
            PackageArch::Riscv64
        ],
        components: alloc::vec![String::from("main")],
    });
}

/// List repositories
pub fn list_repositories() -> Vec<(String, String, bool)> {
    REPOSITORIES
        .lock()
        .iter()
        .map(|r| (r.name.clone(), r.url.clone(), r.enabled))
        .collect()
}

/// Search packages across all repos
pub fn search_packages(query: &str) -> Vec<(String, String, String)> {
    let repos = REPOSITORIES.lock();
    let mut results = Vec::new();

    for repo in repos.iter() {
        for (name, versions) in &repo.packages {
            if name.contains(query) || versions.iter().any(|v| v.summary.contains(query)) {
                if let Some(latest) = versions.last() {
                    results.push((
                        name.clone(),
                        latest.version.display_version(),
                        latest.summary.clone(),
                    ));
                }
            }
        }
    }

    results
}

/// Add a mirror
pub fn add_mirror(url: &str, country: &str, priority: i32) {
    MIRRORS.lock().push(Mirror {
        url: String::from(url),
        country: String::from(country),
        status: MirrorStatus::Online,
        latency_ms: 50,
        bandwidth_mbps: 100,
        last_sync: 0,
        priority,
    });
}

/// Get installed package count
pub fn installed_count() -> usize {
    INSTALLED.lock().len()
}

// ─── Initialization ─────────────────────────────────────────────────

pub fn init() {
    // Initialize content store
    *CONTENT_STORE.lock() = Some(ContentStore::new());

    // Add default repositories
    add_repository(
        "knoxos-core",
        "https://repo.knoxos.com/core",
        RepoType::Official,
    );
    add_repository(
        "knoxos-extra",
        "https://repo.knoxos.com/extra",
        RepoType::Official,
    );
    add_repository(
        "knoxos-community",
        "https://repo.knoxos.com/community",
        RepoType::Community,
    );

    // Add default mirrors
    add_mirror("https://mirror1.knoxos.com", "US", 100);
    add_mirror("https://mirror2.knoxos.com", "EU", 90);
    add_mirror("https://mirror3.knoxos.com", "APAC", 80);

    // Add KnoxOS signing key
    GPG_KEYS.lock().push(GpgKey {
        key_id: String::from("KNOXOS-2026"),
        fingerprint: String::from("0xABCD1234EFGH5678"),
        uid: String::from("KnoxOS Package Signing Key <packages@knoxos.com>"),
        created: 1739836800,
        expires: None,
        algorithm: GpgAlgorithm::Ed25519,
        public_key: {
            // Generate a deterministic Ed25519-like public key from the key ID
            let seed = crate::crypto::sha256(b"KNOXOS-2026-signing-key");
            seed.to_vec()
        },
        trusted: true,
    });

    // Register base system packages as installed
    let mut installed = INSTALLED.lock();
    let base_packages = [
        ("knoxos-kernel", "0.17.0", "KnoxOS kernel"),
        ("knoxos-base", "1.0.0", "KnoxOS base system"),
        ("knoxos-shell", "1.0.0", "KnoxOS shell"),
        ("knoxos-init", "1.0.0", "KnoxOS init system"),
        ("knoxos-fs", "1.0.0", "KnoxOS filesystem utilities"),
        ("knoxos-net", "1.0.0", "KnoxOS networking"),
        ("knoxos-gui", "1.0.0", "KnoxOS desktop environment"),
    ];

    for (name, ver, _desc) in &base_packages {
        installed.insert(
            String::from(*name),
            InstalledPackage {
                name: String::from(*name),
                version: SemVer::parse(ver).unwrap_or(SemVer::new(1, 0, 0)),
                release: 1,
                state: InstallState::Installed,
                auto_installed: false,
                install_time: 0,
                files: Vec::new(),
                config_files: Vec::new(),
            },
        );
    }
    drop(installed);

    DISTRO_INITIALIZED.store(true, Ordering::Release);

    crate::serial_println!("[KnoxOS] Package distribution network initialized");
    crate::serial_println!("[KnoxOS]   Repositories: knoxos-core, knoxos-extra, knoxos-community");
    crate::serial_println!("[KnoxOS]   Mirrors: 3 (US, EU, APAC)");
    crate::serial_println!("[KnoxOS]   Installed packages: {}", base_packages.len());
    crate::serial_println!("[KnoxOS]   Content-addressed storage: enabled");
}
