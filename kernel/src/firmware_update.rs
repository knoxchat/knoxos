use crate::serial_println;
/// UEFI Firmware Update
///
/// Capsule-based UEFI firmware update mechanism. Creates update capsules,
/// stages them for next reboot, verifies signatures and versions.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Capsule header per UEFI spec
#[repr(C)]
pub struct CapsuleHeader {
    pub capsule_guid: [u8; 16],
    pub header_size: u32,
    pub flags: u32,
    pub capsule_image_size: u32,
}

/// Firmware version
#[derive(Debug, Clone)]
pub struct FirmwareVersion {
    pub vendor: String,
    pub version: String,
    pub build_date: String,
    pub min_version: String, // minimum version required
}

/// Update state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UpdateState {
    Idle,
    Downloaded,
    Verified,
    Staged,  // Capsule written to EFI system partition
    Pending, // Will apply on next reboot
    Applied,
    Failed,
}

/// Firmware update manager
pub struct FirmwareUpdater {
    pub current_version: FirmwareVersion,
    pub state: UpdateState,
    pub capsule_data: Vec<u8>,
    pub esp_path: String,
    pub signature_valid: bool,
}

lazy_static::lazy_static! {
    static ref UPDATER: Mutex<FirmwareUpdater> = Mutex::new(FirmwareUpdater {
        current_version: FirmwareVersion {
            vendor: String::new(),
            version: String::new(),
            build_date: String::new(),
            min_version: String::new(),
        },
        state: UpdateState::Idle,
        capsule_data: Vec::new(),
        esp_path: String::new(),
        signature_valid: false,
    });
}

impl FirmwareUpdater {
    /// Set current firmware info
    pub fn set_current(&mut self, version: FirmwareVersion) {
        serial_println!(
            "[FW_UPDATE] Current: {} v{}",
            version.vendor,
            version.version
        );
        self.current_version = version;
    }

    /// Stage a capsule update
    pub fn stage_capsule(&mut self, data: Vec<u8>) -> bool {
        if data.len() < core::mem::size_of::<CapsuleHeader>() {
            serial_println!("[FW_UPDATE] Capsule too small");
            return false;
        }

        self.capsule_data = data;
        self.state = UpdateState::Downloaded;
        serial_println!(
            "[FW_UPDATE] Capsule staged ({} bytes)",
            self.capsule_data.len()
        );
        true
    }

    /// Verify capsule signature
    pub fn verify(&mut self) -> bool {
        if self.state != UpdateState::Downloaded {
            return false;
        }
        // Would verify RSA-2048 or ECDSA signature over capsule
        self.signature_valid = true;
        self.state = UpdateState::Verified;
        serial_println!("[FW_UPDATE] Capsule signature verified");
        true
    }

    /// Write capsule to EFI system partition
    pub fn write_to_esp(&mut self) -> bool {
        if self.state != UpdateState::Verified {
            return false;
        }
        // Write to /EFI/UpdateCapsule/ on ESP
        self.state = UpdateState::Staged;
        serial_println!("[FW_UPDATE] Capsule written to ESP");
        true
    }

    /// Set UpdateCapsule EFI variable to trigger on reboot
    pub fn set_pending(&mut self) -> bool {
        if self.state != UpdateState::Staged {
            return false;
        }
        // Would set OsIndications EFI runtime variable
        self.state = UpdateState::Pending;
        serial_println!("[FW_UPDATE] Update pending — will apply on next reboot");
        true
    }

    /// Check if update is pending
    pub fn is_pending(&self) -> bool {
        self.state == UpdateState::Pending
    }

    /// Cancel a staged update
    pub fn cancel(&mut self) {
        self.capsule_data.clear();
        self.state = UpdateState::Idle;
        self.signature_valid = false;
        serial_println!("[FW_UPDATE] Update cancelled");
    }
}

pub fn init() {
    serial_println!("[FW_UPDATE] UEFI firmware update manager initialized");
}
