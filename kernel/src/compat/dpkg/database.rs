use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use super::control::DebControl;

// ═══════════════════════════════════════════════════════════════════════
// DPKG DATABASE
// ═══════════════════════════════════════════════════════════════════════

/// Installed .deb package record
#[derive(Debug, Clone)]
pub struct InstalledDeb {
    pub control: DebControl,
    pub installed_files: Vec<InstalledFile>,
    pub config_files: Vec<String>,
    pub state: DebState,
    pub install_time: u64,
}

#[derive(Debug, Clone)]
pub struct InstalledFile {
    pub path: String,
    pub size: usize,
    pub mode: u32,
    pub md5sum: Option<String>,
    pub file_type: InstalledFileType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstalledFileType {
    Regular,
    Directory,
    Symlink,
    Config,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebState {
    NotInstalled,
    HalfInstalled,
    Unpacked,
    HalfConfigured,
    Installed,
    ConfigFiles,
}

/// Maintainer script types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaintScript {
    PreInst,
    PostInst,
    PreRm,
    PostRm,
}

/// DPKG database
lazy_static::lazy_static! {
    /// All installed deb packages
    pub(super) static ref DEB_DATABASE: Mutex<BTreeMap<String, InstalledDeb>> = Mutex::new(BTreeMap::new());

    /// Virtual packages provided by installed packages
    pub(super) static ref VIRTUAL_PACKAGES: Mutex<BTreeMap<String, Vec<String>>> = Mutex::new(BTreeMap::new());

    /// File → package ownership map
    pub(super) static ref FILE_OWNERS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

    /// Diversion table (dpkg-divert)
    pub(super) static ref DIVERSIONS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());
}
