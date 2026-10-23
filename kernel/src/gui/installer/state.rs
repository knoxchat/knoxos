use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use spin::Mutex;

use super::types::{BootloaderType, DiskInfo, FsType, PartitionScheme};

// ─── Global State ────────────────────────────────────────────────────

/// Whether the installer is currently active
pub(super) static INSTALLER_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Current wizard step (0-based)
pub(super) static CURRENT_STEP: AtomicU8 = AtomicU8::new(0);

/// Installation progress (0-100)
pub(super) static INSTALL_PROGRESS: AtomicU8 = AtomicU8::new(0);

/// Installation complete flag
pub(super) static INSTALL_COMPLETE: AtomicBool = AtomicBool::new(false);

/// Animation timestamp
static _INSTALLER_ANIM_TSC: AtomicU64 = AtomicU64::new(0);

/// Installer data (form fields, selections)
pub(super) static INSTALLER_STATE: Mutex<InstallerState> = Mutex::new(InstallerState::new_const());

// ─── Installer State ─────────────────────────────────────────────────

pub(super) struct InstallerState {
    // Step 0: Language / Welcome
    pub(super) language_idx: usize,

    // Step 1: Disk selection
    pub(super) disks: [DiskInfo; 8],
    pub(super) disk_count: usize,
    pub(super) selected_disk: usize,
    pub(super) partition_scheme: PartitionScheme,

    // Step 2: Filesystem
    pub(super) root_fs: FsType,

    // Step 3: Bootloader
    pub(super) bootloader: BootloaderType,

    // Step 4: User account
    pub(super) full_name: [u8; 64],
    pub(super) full_name_len: usize,
    pub(super) username: [u8; 32],
    pub(super) username_len: usize,
    pub(super) password: [u8; 64],
    pub(super) password_len: usize,
    pub(super) password_confirm: [u8; 64],
    pub(super) password_confirm_len: usize,
    pub(super) hostname: [u8; 64],
    pub(super) hostname_len: usize,
    /// Which field is focused: 0=fullname, 1=username, 2=password, 3=confirm, 4=hostname
    pub(super) focused_field: u8,

    // Step 5: Timezone
    pub(super) timezone_idx: usize,

    // Step 6: Install
    pub(super) install_log: [u8; 2048],
    pub(super) install_log_len: usize,

    // Validation error
    pub(super) error_msg: [u8; 128],
    pub(super) error_len: usize,
}

impl InstallerState {
    pub(super) const fn new_const() -> Self {
        Self {
            language_idx: 0,
            disks: [
                DiskInfo::empty(),
                DiskInfo::empty(),
                DiskInfo::empty(),
                DiskInfo::empty(),
                DiskInfo::empty(),
                DiskInfo::empty(),
                DiskInfo::empty(),
                DiskInfo::empty(),
            ],
            disk_count: 0,
            selected_disk: 0,
            partition_scheme: PartitionScheme::EraseAll,
            root_fs: FsType::Ext4,
            bootloader: BootloaderType::Grub,
            full_name: [0u8; 64],
            full_name_len: 0,
            username: [0u8; 32],
            username_len: 0,
            password: [0u8; 64],
            password_len: 0,
            password_confirm: [0u8; 64],
            password_confirm_len: 0,
            hostname: [0u8; 64],
            hostname_len: 0,
            focused_field: 0,
            timezone_idx: 0,
            install_log: [0u8; 2048],
            install_log_len: 0,
            error_msg: [0u8; 128],
            error_len: 0,
        }
    }

    pub(super) fn set_error(&mut self, msg: &str) {
        let bytes = msg.as_bytes();
        let len = bytes.len().min(self.error_msg.len());
        self.error_msg[..len].copy_from_slice(&bytes[..len]);
        self.error_len = len;
    }

    pub(super) fn clear_error(&mut self) {
        self.error_len = 0;
    }

    pub(super) fn append_log(&mut self, msg: &str) {
        for &b in msg.as_bytes() {
            if self.install_log_len < self.install_log.len() {
                self.install_log[self.install_log_len] = b;
                self.install_log_len += 1;
            }
        }
        if self.install_log_len < self.install_log.len() {
            self.install_log[self.install_log_len] = b'\n';
            self.install_log_len += 1;
        }
    }
}

/// Check if installer is currently active
pub fn is_active() -> bool {
    INSTALLER_ACTIVE.load(Ordering::Relaxed)
}

/// Close the installer
pub fn close_installer() {
    INSTALLER_ACTIVE.store(false, Ordering::Relaxed)
}
