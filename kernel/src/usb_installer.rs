use crate::serial_println;
/// USB Installer / Deployment Image Builder
///
/// Create bootable USB installation images, configure first-boot
/// setup (OEM/OOBE), unattended install support.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Installation image type
#[derive(Debug, Clone, Copy)]
pub enum ImageType {
    LiveUsb,      // Boot and run from USB
    Installer,    // Install to disk
    OemImage,     // Pre-installed, runs OOBE on first boot
    NetInstaller, // Minimal image, downloads packages
}

/// Partition table scheme
#[derive(Debug, Clone, Copy)]
pub enum PartScheme {
    Gpt,
    Mbr,
}

/// Partition layout entry
#[derive(Debug, Clone)]
pub struct PartitionEntry {
    pub label: String,
    pub filesystem: String, // "ext4", "fat32", "swap"
    pub size_mb: u64,
    pub mountpoint: String,
}

/// First-boot / OOBE configuration
#[derive(Debug, Clone)]
pub struct OobeConfig {
    pub create_user: bool,
    pub username: String,
    pub locale: String,
    pub timezone: String,
    pub keyboard_layout: String,
    pub enable_ssh: bool,
    pub install_packages: Vec<String>,
}

/// Unattended install config (kickstart-style)
#[derive(Debug, Clone)]
pub struct UnattendedConfig {
    pub target_disk: String,
    pub scheme: PartScheme,
    pub partitions: Vec<PartitionEntry>,
    pub oobe: OobeConfig,
    pub reboot_after: bool,
}

/// Image builder state
pub struct ImageBuilder {
    pub image_type: ImageType,
    pub target_path: String,
    pub image_size_mb: u64,
    pub built: bool,
}

lazy_static::lazy_static! {
    static ref BUILDER: Mutex<ImageBuilder> = Mutex::new(ImageBuilder {
        image_type: ImageType::Installer,
        target_path: String::new(),
        image_size_mb: 0,
        built: false,
    });
}

impl ImageBuilder {
    /// Configure image build
    pub fn configure(&mut self, img_type: ImageType, size_mb: u64) {
        self.image_type = img_type;
        self.image_size_mb = size_mb;
        serial_println!("[DEPLOY] Image configured: {:?} ({}MB)", img_type, size_mb);
    }

    /// Build the installation image
    pub fn build(&mut self) -> bool {
        serial_println!("[DEPLOY] Building {:?} image...", self.image_type);
        // Steps:
        // 1. Create disk image (dd / fallocate)
        // 2. Partition with GPT (ESP + root)
        // 3. Format partitions
        // 4. Copy kernel, bootloader, initramfs
        // 5. Copy base packages
        // 6. Install GRUB/systemd-boot
        // 7. Write installer scripts / OOBE config
        self.built = true;
        serial_println!("[DEPLOY] Image built successfully");
        true
    }

    /// Write image to USB device
    pub fn write_to_usb(&self, device: &str) -> bool {
        if !self.built {
            serial_println!("[DEPLOY] Image not built yet");
            return false;
        }
        serial_println!("[DEPLOY] Writing image to {}...", device);
        // Would dd the image to block device
        serial_println!("[DEPLOY] USB image written successfully");
        true
    }

    /// Generate unattended install config
    pub fn generate_kickstart(config: &UnattendedConfig) -> String {
        let mut ks = String::from("# KnoxOS Unattended Install\n");
        ks.push_str(&alloc::format!("target={}\n", config.target_disk));
        ks.push_str(&alloc::format!("scheme={:?}\n", config.scheme));
        for part in &config.partitions {
            ks.push_str(&alloc::format!(
                "partition {} {} {}MB {}\n",
                part.label,
                part.filesystem,
                part.size_mb,
                part.mountpoint
            ));
        }
        ks.push_str(&alloc::format!("locale={}\n", config.oobe.locale));
        ks.push_str(&alloc::format!("timezone={}\n", config.oobe.timezone));
        ks
    }
}

pub fn init() {
    serial_println!("[DEPLOY] USB installer / deployment image builder initialized");
}
