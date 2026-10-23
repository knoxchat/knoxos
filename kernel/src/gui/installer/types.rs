use alloc::string::String;
use alloc::vec::Vec;

// ─── Constants ───────────────────────────────────────────────────────

pub(super) const CARD_WIDTH: usize = 800;
pub(super) const CARD_HEIGHT: usize = 560;
pub(super) const SIDEBAR_WIDTH: usize = 200;
pub(super) const BUTTON_WIDTH: usize = 120;
pub(super) const BUTTON_HEIGHT: usize = 36;
pub(super) const STEP_COUNT: usize = 7;

// ─── Step Definitions ────────────────────────────────────────────────

pub const STEP_NAMES: [&str; STEP_COUNT] = [
    "Welcome",
    "Disk Setup",
    "Filesystem",
    "Bootloader",
    "User Account",
    "Timezone",
    "Install",
];

// ─── Filesystem & Bootloader Options ─────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub enum FsType {
    Ext4,
    Btrfs,
    Xfs,
    Fat32,
}

impl FsType {
    pub(super) fn label(self) -> &'static str {
        match self {
            FsType::Ext4 => "ext4",
            FsType::Btrfs => "btrfs",
            FsType::Xfs => "XFS",
            FsType::Fat32 => "FAT32 (EFI)",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum BootloaderType {
    Grub,
    SystemdBoot,
}

impl BootloaderType {
    pub(super) fn label(self) -> &'static str {
        match self {
            BootloaderType::Grub => "GRUB 2",
            BootloaderType::SystemdBoot => "systemd-boot",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum PartitionScheme {
    /// Erase entire disk with automatic layout: EFI + swap + root
    EraseAll,
    /// Manual partitioning (advanced)
    Manual,
}

// ─── Disk Descriptor ─────────────────────────────────────────────────

#[derive(Clone)]
pub struct DiskInfo {
    pub name: [u8; 32],
    pub name_len: usize,
    pub size_mb: u64,
    pub model: [u8; 48],
    pub model_len: usize,
}

impl DiskInfo {
    pub(super) const fn empty() -> Self {
        Self {
            name: [0u8; 32],
            name_len: 0,
            size_mb: 0,
            model: [0u8; 48],
            model_len: 0,
        }
    }
}

// ─── Timezone ────────────────────────────────────────────────────────

pub(super) const TIMEZONES: [&str; 12] = [
    "UTC",
    "America/New_York",
    "America/Chicago",
    "America/Denver",
    "America/Los_Angeles",
    "Europe/London",
    "Europe/Berlin",
    "Europe/Moscow",
    "Asia/Tokyo",
    "Asia/Shanghai",
    "Asia/Kolkata",
    "Australia/Sydney",
];

/// GPUI snapshot (D11.10).
pub struct InstallSnap {
    pub step: usize,
    pub title: String,
    pub body: String,
    pub options: Vec<String>,
    pub selected: Option<usize>,
    pub next_label: String,
    pub can_back: bool,
    pub progress: u8,
}
