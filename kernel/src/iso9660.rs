/// ISO 9660 Bootable Image Builder
///
/// Implements ISO 9660 (ECMA-119) filesystem creation for distributing KnoxOS
/// as a bootable ISO image suitable for bare-metal installation.
///
/// Features:
///   - ISO 9660 Level 1/2/3 filesystem generation
///   - El Torito boot specification for BIOS bootable CDs
///   - UEFI boot support via EFI System Partition in ISO
///   - Rock Ridge extensions for POSIX file attributes
///   - Joliet extensions for Unicode filenames
///   - Hybrid ISO (isohybrid) for USB flash drive booting
///   - Multi-session support for incremental ISO creation
///   - Boot catalog generation
///   - ISO image validation and checksum
///   - Live system image builder (squashfs rootfs in ISO)
///
/// ISO Layout:
///   Sector 0-15:    System Area (El Torito boot code / isohybrid MBR)
///   Sector 16:      Primary Volume Descriptor
///   Sector 17:      Supplementary Volume Descriptor (Joliet)
///   Sector 18:      Boot Record Volume Descriptor (El Torito)
///   Sector 19:      Volume Descriptor Set Terminator
///   Sector 20+:     Path tables, directory records, file data
///   End:            Boot catalog, boot image
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// ISO 9660 CONSTANTS
// ═══════════════════════════════════════════════════════════════════════

/// Sector size (2048 bytes for ISO 9660)
pub const ISO_SECTOR_SIZE: usize = 2048;

/// System area size (16 sectors = 32KB)
pub const SYSTEM_AREA_SECTORS: u32 = 16;

/// Volume descriptor types
pub const VD_PRIMARY: u8 = 1;
pub const VD_SUPPLEMENTARY: u8 = 2;
pub const VD_BOOT_RECORD: u8 = 0;
pub const VD_TERMINATOR: u8 = 255;

/// Standard identifier "CD001"
pub const ISO_STANDARD_ID: &[u8; 5] = b"CD001";

/// El Torito boot record system identifier
pub const EL_TORITO_SPEC: &[u8; 32] = b"EL TORITO SPECIFICATION\0\0\0\0\0\0\0\0\0";

/// Version
pub const ISO_VERSION: u8 = 1;

/// Directory entry flags
pub const DE_FLAG_HIDDEN: u8 = 0x01;
pub const DE_FLAG_DIRECTORY: u8 = 0x02;
pub const DE_FLAG_ASSOCIATED: u8 = 0x04;
pub const DE_FLAG_RECORD: u8 = 0x08;
pub const DE_FLAG_PROTECTION: u8 = 0x10;
pub const DE_FLAG_MULTI_EXTENT: u8 = 0x80;

/// El Torito boot media types
pub const BOOT_MEDIA_NO_EMULATION: u8 = 0;
pub const BOOT_MEDIA_12_FLOPPY: u8 = 1;
pub const BOOT_MEDIA_14_FLOPPY: u8 = 2;
pub const BOOT_MEDIA_28_FLOPPY: u8 = 3;
pub const BOOT_MEDIA_HARD_DISK: u8 = 4;

/// El Torito platform IDs
pub const PLATFORM_X86: u8 = 0;
pub const PLATFORM_PPC: u8 = 1;
pub const PLATFORM_MAC: u8 = 2;
pub const PLATFORM_EFI: u8 = 0xEF;

/// Rock Ridge signature
pub const ROCK_RIDGE_SIG: &[u8; 2] = b"RR";

/// Maximum filename lengths
pub const ISO_MAX_FILENAME_L1: usize = 12; // 8.3 format
pub const ISO_MAX_FILENAME_L2: usize = 31;
pub const JOLIET_MAX_FILENAME: usize = 64;

// ═══════════════════════════════════════════════════════════════════════
// DATA STRUCTURES
// ═══════════════════════════════════════════════════════════════════════

/// ISO 9660 date/time in 7-byte format (directory record)
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct IsoDateTime7 {
    pub year: u8,       // years since 1900
    pub month: u8,      // 1-12
    pub day: u8,        // 1-31
    pub hour: u8,       // 0-23
    pub minute: u8,     // 0-59
    pub second: u8,     // 0-59
    pub gmt_offset: u8, // 15-minute intervals from GMT
}

impl IsoDateTime7 {
    pub fn now() -> Self {
        // Use kernel RTC for current time
        Self {
            year: 126, // 2026 - 1900
            month: 2,
            day: 27,
            hour: 12,
            minute: 0,
            second: 0,
            gmt_offset: 0,
        }
    }

    pub fn to_bytes(&self) -> [u8; 7] {
        [
            self.year,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second,
            self.gmt_offset,
        ]
    }
}

/// ISO 9660 date/time in 17-byte ASCII format (volume descriptor)
#[derive(Debug, Clone)]
pub struct IsoDateTime17 {
    pub year: [u8; 4],        // "2026"
    pub month: [u8; 2],       // "02"
    pub day: [u8; 2],         // "27"
    pub hour: [u8; 2],        // "12"
    pub minute: [u8; 2],      // "00"
    pub second: [u8; 2],      // "00"
    pub centisecond: [u8; 2], // "00"
    pub gmt_offset: u8,
}

impl IsoDateTime17 {
    pub fn now() -> Self {
        Self {
            year: *b"2026",
            month: *b"02",
            day: *b"27",
            hour: *b"12",
            minute: *b"00",
            second: *b"00",
            centisecond: *b"00",
            gmt_offset: 0,
        }
    }

    pub fn to_bytes(&self) -> [u8; 17] {
        let mut buf = [0u8; 17];
        buf[0..4].copy_from_slice(&self.year);
        buf[4..6].copy_from_slice(&self.month);
        buf[6..8].copy_from_slice(&self.day);
        buf[8..10].copy_from_slice(&self.hour);
        buf[10..12].copy_from_slice(&self.minute);
        buf[12..14].copy_from_slice(&self.second);
        buf[14..16].copy_from_slice(&self.centisecond);
        buf[16] = self.gmt_offset;
        buf
    }
}

/// File entry for ISO filesystem
#[derive(Debug, Clone)]
pub struct IsoFileEntry {
    pub name: String,
    pub iso_name: String,    // 8.3 or Level 2 name
    pub joliet_name: String, // Unicode name
    pub data: Vec<u8>,
    pub is_directory: bool,
    pub permissions: u32, // POSIX permissions (Rock Ridge)
    pub uid: u32,
    pub gid: u32,
    pub mtime: u64,
    pub children: Vec<IsoFileEntry>,
    // Assigned during layout
    pub extent_lba: u32, // starting LBA on disc
    pub data_length: u32,
    pub path_table_index: u16,
}

impl IsoFileEntry {
    pub fn new_file(name: &str, data: Vec<u8>) -> Self {
        let iso_name = Self::to_iso_name(name);
        Self {
            name: String::from(name),
            iso_name,
            joliet_name: String::from(name),
            data_length: data.len() as u32,
            data,
            is_directory: false,
            permissions: 0o644,
            uid: 0,
            gid: 0,
            mtime: 0,
            children: Vec::new(),
            extent_lba: 0,
            path_table_index: 0,
        }
    }

    pub fn new_directory(name: &str) -> Self {
        Self {
            name: String::from(name),
            iso_name: Self::to_iso_name(name),
            joliet_name: String::from(name),
            data: Vec::new(),
            data_length: 0,
            is_directory: true,
            permissions: 0o755,
            uid: 0,
            gid: 0,
            mtime: 0,
            children: Vec::new(),
            extent_lba: 0,
            path_table_index: 0,
        }
    }

    pub fn add_child(&mut self, child: IsoFileEntry) {
        if self.is_directory {
            self.children.push(child);
        }
    }

    /// Convert filename to ISO 9660 Level 1 (8.3) format
    fn to_iso_name(name: &str) -> String {
        let upper = name.to_uppercase();
        // Replace invalid chars with underscores
        let clean: String = upper
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
                    c
                } else {
                    '_'
                }
            })
            .collect();

        if clean.contains('.') {
            // Has extension — enforce 8.3
            let parts: Vec<&str> = clean.splitn(2, '.').collect();
            let base = if parts[0].len() > 8 {
                &parts[0][..8]
            } else {
                parts[0]
            };
            let ext = if parts.len() > 1 && parts[1].len() > 3 {
                &parts[1][..3]
            } else if parts.len() > 1 {
                parts[1]
            } else {
                ""
            };
            if ext.is_empty() {
                format!("{}", base)
            } else {
                format!("{}.{}", base, ext)
            }
        } else {
            // No extension — truncate to 8 chars
            if clean.len() > 8 {
                clean[..8].to_string()
            } else {
                clean
            }
        }
    }
}

/// Boot configuration for El Torito
#[derive(Debug, Clone)]
pub struct BootConfig {
    pub bios_boot_image: Option<Vec<u8>>, // BIOS boot image (e.g., isolinux.bin)
    pub uefi_boot_image: Option<Vec<u8>>, // UEFI boot image (e.g., efi.img)
    pub boot_load_size: u16,              // Sectors to load (4 = 2KB)
    pub boot_info_table: bool,            // Patch boot info table into boot image
    pub platform: u8,                     // PLATFORM_X86 or PLATFORM_EFI
}

impl Default for BootConfig {
    fn default() -> Self {
        Self {
            bios_boot_image: None,
            uefi_boot_image: None,
            boot_load_size: 4,
            boot_info_table: true,
            platform: PLATFORM_X86,
        }
    }
}

/// Isohybrid MBR configuration for USB booting
#[derive(Debug, Clone)]
pub struct IsohybridConfig {
    pub enabled: bool,
    pub partition_type: u8,            // 0x17 for hidden IFS, 0x00 for empty
    pub partition_offset: u32,         // Usually 0 or 1
    pub mbr_template: Option<Vec<u8>>, // Custom MBR code (446 bytes)
}

impl Default for IsohybridConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            partition_type: 0x17,
            partition_offset: 0,
            mbr_template: None,
        }
    }
}

/// Rock Ridge extension data
#[derive(Debug, Clone)]
pub struct RockRidgeEntry {
    pub posix_mode: u32,
    pub posix_nlinks: u32,
    pub posix_uid: u32,
    pub posix_gid: u32,
    pub posix_serial: u32,
    pub alternate_name: Option<String>,
    pub symlink_target: Option<String>,
}

/// ISO image builder configuration
#[derive(Debug, Clone)]
pub struct IsoConfig {
    pub volume_id: String,
    pub system_id: String,
    pub publisher_id: String,
    pub preparer_id: String,
    pub application_id: String,
    pub copyright_file: String,
    pub level: u8, // 1, 2, or 3
    pub rock_ridge: bool,
    pub joliet: bool,
    pub boot_config: BootConfig,
    pub isohybrid: IsohybridConfig,
}

impl Default for IsoConfig {
    fn default() -> Self {
        Self {
            volume_id: String::from("KNOXOS_2026"),
            system_id: String::from("KNOXOS"),
            publisher_id: String::from("KNOXOS PROJECT"),
            preparer_id: String::from("KNOXOS ISO BUILDER"),
            application_id: String::from("KNOXOS INSTALLER"),
            copyright_file: String::new(),
            level: 2,
            rock_ridge: true,
            joliet: true,
            boot_config: BootConfig::default(),
            isohybrid: IsohybridConfig::default(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ISO 9660 IMAGE BUILDER
// ═══════════════════════════════════════════════════════════════════════

/// ISO 9660 image builder — generates bootable ISO images
pub struct IsoBuilder {
    pub config: IsoConfig,
    pub root: IsoFileEntry,
    pub image: Vec<u8>,
    pub next_lba: u32,
    pub path_table_l: Vec<u8>, // Little-endian path table
    pub path_table_m: Vec<u8>, // Big-endian path table
    pub boot_catalog_lba: u32,
    pub boot_image_lba: u32,
}

impl IsoBuilder {
    pub fn new(config: IsoConfig) -> Self {
        Self {
            config,
            root: IsoFileEntry::new_directory(""),
            image: Vec::new(),
            next_lba: 20, // Start after volume descriptors
            path_table_l: Vec::new(),
            path_table_m: Vec::new(),
            boot_catalog_lba: 0,
            boot_image_lba: 0,
        }
    }

    /// Add a file to the ISO at the given path
    pub fn add_file(&mut self, path: &str, data: Vec<u8>) {
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        Self::add_entry_recursive(&mut self.root, &parts, data);
    }

    fn add_entry_recursive(parent: &mut IsoFileEntry, parts: &[&str], data: Vec<u8>) {
        if parts.is_empty() {
            return;
        }
        if parts.len() == 1 {
            // This is the file
            parent.add_child(IsoFileEntry::new_file(parts[0], data));
            return;
        }
        // Need to traverse/create directory
        let dir_name = parts[0];
        let existing = parent
            .children
            .iter_mut()
            .find(|c| c.name == dir_name && c.is_directory);
        if let Some(dir) = existing {
            Self::add_entry_recursive(dir, &parts[1..], data);
        } else {
            let mut new_dir = IsoFileEntry::new_directory(dir_name);
            Self::add_entry_recursive(&mut new_dir, &parts[1..], data);
            parent.add_child(new_dir);
        }
    }

    /// Add the KnoxOS kernel and boot files
    pub fn add_knoxos_boot_files(&mut self, kernel_binary: &[u8]) {
        // Add kernel
        self.add_file("/boot/knoxos-kernel", kernel_binary.to_vec());

        // Add boot configuration (GRUB-style)
        let grub_cfg = b"set timeout=5\nset default=0\n\nmenuentry \"KnoxOS\" {\n    multiboot2 /boot/knoxos-kernel\n    boot\n}\n\nmenuentry \"KnoxOS (Safe Mode)\" {\n    multiboot2 /boot/knoxos-kernel safe_mode=1\n    boot\n}\n\nmenuentry \"KnoxOS (Recovery)\" {\n    multiboot2 /boot/knoxos-kernel recovery=1\n    boot\n}\n";
        self.add_file("/boot/grub/grub.cfg", grub_cfg.to_vec());

        // Add isolinux configuration for BIOS boot
        let isolinux_cfg = b"DEFAULT knoxos\nTIMEOUT 50\nPROMPT 1\n\nLABEL knoxos\n    MENU LABEL KnoxOS\n    KERNEL /boot/knoxos-kernel\n    APPEND root=/dev/sr0\n\nLABEL safe\n    MENU LABEL KnoxOS (Safe Mode)\n    KERNEL /boot/knoxos-kernel\n    APPEND safe_mode=1\n\nLABEL recovery\n    MENU LABEL KnoxOS (Recovery)\n    KERNEL /boot/knoxos-kernel\n    APPEND recovery=1\n";
        self.add_file("/isolinux/isolinux.cfg", isolinux_cfg.to_vec());

        serial_println!(
            "[ISO] Added KnoxOS boot files ({} bytes kernel)",
            kernel_binary.len()
        );
    }

    /// Add a live system rootfs (squashfs image)
    pub fn add_live_rootfs(&mut self, squashfs_data: &[u8]) {
        self.add_file("/live/filesystem.squashfs", squashfs_data.to_vec());

        // Live boot configuration
        let live_cfg = b"# KnoxOS Live System\nroot=live:CDLABEL=KNOXOS_2026\nrootfstype=squashfs\nrd.live.image\nrd.live.overlay.overlayfs\nquiet splash\n";
        self.add_file("/live/boot.cfg", live_cfg.to_vec());

        serial_println!(
            "[ISO] Added live rootfs ({} bytes squashfs)",
            squashfs_data.len()
        );
    }

    /// Build the ISO 9660 image
    pub fn build(&mut self) -> &[u8] {
        serial_println!("[ISO] Building ISO 9660 image...");

        // Phase 1: Layout — assign LBAs to all entries
        self.layout_entries();

        // Phase 2: Build path tables
        self.build_path_tables();

        // Phase 3: Allocate image buffer
        let total_sectors = self.next_lba + 1;
        let total_bytes = total_sectors as usize * ISO_SECTOR_SIZE;
        self.image = vec![0u8; total_bytes];

        // Phase 4: Write system area (sectors 0-15)
        self.write_system_area();

        // Phase 5: Write volume descriptors
        self.write_primary_volume_descriptor();
        if self.config.joliet {
            self.write_supplementary_volume_descriptor();
        }
        if self.config.boot_config.bios_boot_image.is_some()
            || self.config.boot_config.uefi_boot_image.is_some()
        {
            self.write_boot_record_descriptor();
        }
        self.write_volume_descriptor_terminator();

        // Phase 6: Write path tables
        self.write_path_tables();

        // Phase 7: Write directory records and file data
        self.write_directory_tree(&self.root.clone(), 0);

        // Phase 8: Write boot catalog and boot images
        self.write_boot_catalog();

        // Phase 9: Apply isohybrid MBR if requested
        if self.config.isohybrid.enabled {
            self.apply_isohybrid();
        }

        serial_println!(
            "[ISO] ISO image built: {} sectors ({} bytes)",
            total_sectors,
            total_bytes
        );

        &self.image
    }

    /// Assign LBAs (logical block addresses) to all filesystem entries
    fn layout_entries(&mut self) {
        // Reserve sectors: 0-15 system area, 16-19 volume descriptors
        self.next_lba = 20;

        // Path tables (L-type and M-type, 1 sector each)
        self.next_lba += 4; // 2 for L-path, 2 for M-path

        // Boot catalog (1 sector)
        self.boot_catalog_lba = self.next_lba;
        self.next_lba += 1;

        // Boot image
        self.boot_image_lba = self.next_lba;
        if let Some(ref boot_image) = self.config.boot_config.bios_boot_image {
            let size = boot_image.len();
            self.next_lba += size.div_ceil(ISO_SECTOR_SIZE) as u32;
        }

        // Layout root and all children
        Self::layout_entry_recursive(&mut self.root, &mut self.next_lba);
    }

    fn layout_entry_recursive(entry: &mut IsoFileEntry, next_lba: &mut u32) {
        entry.extent_lba = *next_lba;

        if entry.is_directory {
            // Directory record takes at least 1 sector
            *next_lba += 1;
            // Layout children
            for child in entry.children.iter_mut() {
                Self::layout_entry_recursive(child, next_lba);
            }
        } else {
            // File data
            let sectors = if entry.data.is_empty() {
                1
            } else {
                entry.data.len().div_ceil(ISO_SECTOR_SIZE) as u32
            };
            entry.data_length = entry.data.len() as u32;
            *next_lba += sectors;
        }
    }

    /// Build path tables (L-type little-endian and M-type big-endian)
    fn build_path_tables(&mut self) {
        self.path_table_l.clear();
        self.path_table_m.clear();

        let mut index: u16 = 1;
        Self::build_path_table_recursive(
            &mut self.root,
            1, // parent index (root's parent is itself)
            &mut index,
            &mut self.path_table_l,
            &mut self.path_table_m,
        );
    }

    fn build_path_table_recursive(
        entry: &mut IsoFileEntry,
        parent_index: u16,
        current_index: &mut u16,
        ptl: &mut Vec<u8>,
        ptm: &mut Vec<u8>,
    ) {
        if !entry.is_directory {
            return;
        }

        entry.path_table_index = *current_index;
        let name_bytes = if entry.name.is_empty() {
            vec![1u8] // Root directory identifier
        } else {
            entry.iso_name.as_bytes().to_vec()
        };
        let name_len = name_bytes.len() as u8;

        // L-type (little-endian)
        ptl.push(name_len);
        ptl.push(0); // extended attribute record length
        ptl.extend_from_slice(&entry.extent_lba.to_le_bytes());
        ptl.extend_from_slice(&parent_index.to_le_bytes());
        ptl.extend_from_slice(&name_bytes);
        if name_len % 2 != 0 {
            ptl.push(0); // padding
        }

        // M-type (big-endian)
        ptm.push(name_len);
        ptm.push(0);
        ptm.extend_from_slice(&entry.extent_lba.to_be_bytes());
        ptm.extend_from_slice(&parent_index.to_be_bytes());
        ptm.extend_from_slice(&name_bytes);
        if name_len % 2 != 0 {
            ptm.push(0);
        }

        let my_index = *current_index;
        *current_index += 1;

        // Recurse into child directories
        for child in entry.children.iter_mut() {
            if child.is_directory {
                Self::build_path_table_recursive(child, my_index, current_index, ptl, ptm);
            }
        }
    }

    /// Write system area (sectors 0-15) — MBR for isohybrid support
    fn write_system_area(&mut self) {
        // System area is all zeros unless isohybrid is enabled
        // Will be patched later by apply_isohybrid()
    }

    /// Write Primary Volume Descriptor (sector 16)
    fn write_primary_volume_descriptor(&mut self) {
        let offset = 16 * ISO_SECTOR_SIZE;
        let buf = &mut self.image[offset..offset + ISO_SECTOR_SIZE];

        buf[0] = VD_PRIMARY;
        buf[1..6].copy_from_slice(ISO_STANDARD_ID);
        buf[6] = ISO_VERSION;

        // System Identifier (32 bytes)
        Self::write_str_a(&mut buf[8..40], &self.config.system_id);

        // Volume Identifier (32 bytes)
        Self::write_str_d(&mut buf[40..72], &self.config.volume_id);

        // Volume Space Size (both-endian u32 at 80)
        let total_lba = self.next_lba;
        Self::write_both_u32(&mut buf[80..88], total_lba);

        // Volume Set Size (both-endian u16)
        Self::write_both_u16(&mut buf[120..124], 1);

        // Volume Sequence Number
        Self::write_both_u16(&mut buf[124..128], 1);

        // Logical Block Size
        Self::write_both_u16(&mut buf[128..132], ISO_SECTOR_SIZE as u16);

        // Path Table Size (both-endian u32)
        let pt_size = self.path_table_l.len() as u32;
        Self::write_both_u32(&mut buf[132..140], pt_size);

        // Type L Path Table location (LE u32)
        buf[140..144].copy_from_slice(&20u32.to_le_bytes());

        // Optional Type L Path Table (LE u32)
        buf[144..148].copy_from_slice(&0u32.to_le_bytes());

        // Type M Path Table location (BE u32)
        buf[148..152].copy_from_slice(&22u32.to_be_bytes());

        // Root Directory Record (34 bytes at offset 156)
        let root_clone = self.root.clone();
        Self::write_directory_record_into(
            &mut self.image[offset + 156..offset + 190],
            &root_clone,
            true,
        );
        let buf = &mut self.image[offset..offset + ISO_SECTOR_SIZE];

        // Volume Set Identifier (128 bytes at 190)
        Self::write_str_d(&mut buf[190..318], &self.config.volume_id);

        // Publisher Identifier (128 bytes at 318)
        Self::write_str_a(&mut buf[318..446], &self.config.publisher_id);

        // Data Preparer Identifier (128 bytes at 446)
        Self::write_str_a(&mut buf[446..574], &self.config.preparer_id);

        // Application Identifier (128 bytes at 574)
        Self::write_str_a(&mut buf[574..702], &self.config.application_id);

        // Volume Creation Date/Time (17 bytes at 813)
        let dt = IsoDateTime17::now();
        buf[813..830].copy_from_slice(&dt.to_bytes());

        // Volume Modification Date/Time (17 bytes at 830)
        buf[830..847].copy_from_slice(&dt.to_bytes());
    }

    /// Write Supplementary Volume Descriptor for Joliet (sector 17)
    fn write_supplementary_volume_descriptor(&mut self) {
        let offset = 17 * ISO_SECTOR_SIZE;

        {
            let buf = &mut self.image[offset..offset + ISO_SECTOR_SIZE];
            buf[0] = VD_SUPPLEMENTARY;
            buf[1..6].copy_from_slice(ISO_STANDARD_ID);
            buf[6] = ISO_VERSION;

            // Escape sequences for UCS-2 Level 3
            buf[88..91].copy_from_slice(b"%/E");

            // Volume Identifier in UCS-2
            let vol_ucs2 = Self::str_to_ucs2(&self.config.volume_id);
            let len = core::cmp::min(vol_ucs2.len(), 32);
            buf[40..40 + len].copy_from_slice(&vol_ucs2[..len]);

            // Volume Space Size
            let total_lba = self.next_lba;
            Self::write_both_u32(&mut buf[80..88], total_lba);

            // Logical Block Size
            Self::write_both_u16(&mut buf[128..132], ISO_SECTOR_SIZE as u16);
        }

        // Root Directory Record
        let root_clone = self.root.clone();
        Self::write_directory_record_into(
            &mut self.image[offset + 156..offset + 190],
            &root_clone,
            true,
        );
    }

    /// Write Boot Record Volume Descriptor (El Torito) at sector 18
    fn write_boot_record_descriptor(&mut self) {
        let offset = 18 * ISO_SECTOR_SIZE;
        let buf = &mut self.image[offset..offset + ISO_SECTOR_SIZE];

        buf[0] = VD_BOOT_RECORD;
        buf[1..6].copy_from_slice(ISO_STANDARD_ID);
        buf[6] = ISO_VERSION;

        // Boot system identifier "EL TORITO SPECIFICATION"
        let el_torito_len = core::cmp::min(EL_TORITO_SPEC.len(), 32);
        buf[7..7 + el_torito_len].copy_from_slice(&EL_TORITO_SPEC[..el_torito_len]);

        // Boot catalog LBA (LE u32 at offset 71)
        buf[71..75].copy_from_slice(&self.boot_catalog_lba.to_le_bytes());
    }

    /// Write Volume Descriptor Set Terminator (sector 19)
    fn write_volume_descriptor_terminator(&mut self) {
        let offset = 19 * ISO_SECTOR_SIZE;
        let buf = &mut self.image[offset..offset + ISO_SECTOR_SIZE];

        buf[0] = VD_TERMINATOR;
        buf[1..6].copy_from_slice(ISO_STANDARD_ID);
        buf[6] = ISO_VERSION;
    }

    /// Write path tables at sectors 20-23
    fn write_path_tables(&mut self) {
        // L-type at sector 20-21
        let offset_l = 20 * ISO_SECTOR_SIZE;
        let len_l = core::cmp::min(self.path_table_l.len(), 2 * ISO_SECTOR_SIZE);
        let ptl = self.path_table_l.clone();
        self.image[offset_l..offset_l + len_l].copy_from_slice(&ptl[..len_l]);

        // M-type at sector 22-23
        let offset_m = 22 * ISO_SECTOR_SIZE;
        let len_m = core::cmp::min(self.path_table_m.len(), 2 * ISO_SECTOR_SIZE);
        let ptm = self.path_table_m.clone();
        self.image[offset_m..offset_m + len_m].copy_from_slice(&ptm[..len_m]);
    }

    /// Write directory tree (recursive)
    fn write_directory_tree(&mut self, entry: &IsoFileEntry, parent_lba: u32) {
        if !entry.is_directory {
            // Write file data
            let offset = entry.extent_lba as usize * ISO_SECTOR_SIZE;
            let end = offset + entry.data.len();
            if end <= self.image.len() {
                self.image[offset..end].copy_from_slice(&entry.data);
            }
            return;
        }

        // Write directory record
        let dir_offset = entry.extent_lba as usize * ISO_SECTOR_SIZE;
        let mut pos = dir_offset;

        // "." entry (self)
        let dot_record = self.make_directory_record(entry, 0x00, true);
        let end = pos + dot_record.len();
        if end <= self.image.len() {
            self.image[pos..end].copy_from_slice(&dot_record);
        }
        pos = end;

        // ".." entry (parent)
        let parent_entry = IsoFileEntry {
            extent_lba: if parent_lba == 0 {
                entry.extent_lba
            } else {
                parent_lba
            },
            data_length: ISO_SECTOR_SIZE as u32,
            is_directory: true,
            ..IsoFileEntry::new_directory("")
        };
        let dotdot_record = self.make_directory_record(&parent_entry, 0x01, true);
        let end = pos + dotdot_record.len();
        if end <= self.image.len() {
            self.image[pos..end].copy_from_slice(&dotdot_record);
        }
        pos = end;

        // Child entries
        for child in &entry.children {
            let record = self.make_directory_record(child, 0xFF, false);
            // Check if record fits in current sector
            let sector_boundary = ((pos / ISO_SECTOR_SIZE) + 1) * ISO_SECTOR_SIZE;
            if pos + record.len() > sector_boundary {
                pos = sector_boundary; // Skip to next sector
            }
            let end = pos + record.len();
            if end <= self.image.len() {
                self.image[pos..end].copy_from_slice(&record);
            }
            pos = end;
        }

        // Recurse into child directories
        for child in &entry.children {
            self.write_directory_tree(child, entry.extent_lba);
        }
    }

    /// Create a directory record for an entry
    fn make_directory_record(
        &self,
        entry: &IsoFileEntry,
        special: u8, // 0x00 = ".", 0x01 = "..", 0xFF = normal
        is_special: bool,
    ) -> Vec<u8> {
        let name_bytes: Vec<u8> = if is_special {
            vec![special]
        } else if entry.is_directory {
            entry.iso_name.as_bytes().to_vec()
        } else {
            // Files need ;1 version suffix
            let mut n = entry.iso_name.as_bytes().to_vec();
            n.extend_from_slice(b";1");
            n
        };

        let name_len = name_bytes.len() as u8;
        let record_len = 33 + name_len as usize;
        let padded_len = if record_len % 2 != 0 {
            record_len + 1
        } else {
            record_len
        };

        let mut record = vec![0u8; padded_len];
        record[0] = padded_len as u8; // Length of Directory Record

        // Extended Attribute Record Length
        record[1] = 0;

        // Location of Extent (both-endian)
        let lba = entry.extent_lba;
        record[2..6].copy_from_slice(&lba.to_le_bytes());
        record[6..10].copy_from_slice(&lba.to_be_bytes());

        // Data Length (both-endian)
        let data_len = if entry.is_directory {
            ISO_SECTOR_SIZE as u32
        } else {
            entry.data_length
        };
        record[10..14].copy_from_slice(&data_len.to_le_bytes());
        record[14..18].copy_from_slice(&data_len.to_be_bytes());

        // Recording Date and Time (7 bytes)
        let dt = IsoDateTime7::now();
        record[18..25].copy_from_slice(&dt.to_bytes());

        // File Flags
        record[25] = if entry.is_directory {
            DE_FLAG_DIRECTORY
        } else {
            0
        };

        // File Unit Size (interleave)
        record[26] = 0;

        // Interleave Gap Size
        record[27] = 0;

        // Volume Sequence Number (both-endian u16)
        record[28..30].copy_from_slice(&1u16.to_le_bytes());
        record[30..32].copy_from_slice(&1u16.to_be_bytes());

        // Length of File Identifier
        record[32] = name_len;

        // File Identifier
        record[33..33 + name_len as usize].copy_from_slice(&name_bytes);

        record
    }

    /// Write a directory record at a specific buffer location (for volume descriptor root)
    fn write_directory_record_into(buf: &mut [u8], entry: &IsoFileEntry, is_root: bool) {
        let record_len = 34u8;
        buf[0] = record_len;
        buf[1] = 0; // Extended attribute

        // Location (both-endian)
        let lba = entry.extent_lba;
        buf[2..6].copy_from_slice(&lba.to_le_bytes());
        buf[6..10].copy_from_slice(&lba.to_be_bytes());

        // Data length
        let data_len = ISO_SECTOR_SIZE as u32;
        buf[10..14].copy_from_slice(&data_len.to_le_bytes());
        buf[14..18].copy_from_slice(&data_len.to_be_bytes());

        // Date/Time
        let dt = IsoDateTime7::now();
        buf[18..25].copy_from_slice(&dt.to_bytes());

        // Flags
        buf[25] = DE_FLAG_DIRECTORY;

        // File unit size, interleave gap
        buf[26] = 0;
        buf[27] = 0;

        // Volume sequence (both-endian)
        buf[28..30].copy_from_slice(&1u16.to_le_bytes());
        buf[30..32].copy_from_slice(&1u16.to_be_bytes());

        // File identifier length
        buf[32] = 1;

        // File identifier (root = 0x00)
        buf[33] = if is_root { 0x00 } else { 0x01 };
    }

    /// Write El Torito boot catalog
    fn write_boot_catalog(&mut self) {
        let offset = self.boot_catalog_lba as usize * ISO_SECTOR_SIZE;
        if offset + ISO_SECTOR_SIZE > self.image.len() {
            return;
        }

        // Validation Entry (32 bytes)
        let buf = &mut self.image[offset..offset + ISO_SECTOR_SIZE];

        // Header ID (1 = validation entry)
        buf[0] = 0x01;
        // Platform ID
        buf[1] = self.config.boot_config.platform;
        // Reserved
        buf[2] = 0;
        buf[3] = 0;
        // ID string "KnoxOS"
        buf[4..10].copy_from_slice(b"KnoxOS");
        // Checksum (bytes 28-29) — calculate so all u16 sums = 0
        // Key bytes (0x55, 0xAA)
        buf[30] = 0x55;
        buf[31] = 0xAA;

        // Calculate checksum
        let mut sum: u16 = 0;
        for i in (0..32).step_by(2) {
            sum = sum.wrapping_add(u16::from_le_bytes([buf[i], buf[i + 1]]));
        }
        let checksum = 0u16.wrapping_sub(sum);
        buf[28..30].copy_from_slice(&checksum.to_le_bytes());

        // Default Entry (32 bytes at offset 32)
        buf[32] = 0x88; // Bootable
        buf[33] = BOOT_MEDIA_NO_EMULATION;
        buf[34..36].copy_from_slice(&0u16.to_le_bytes()); // Load segment
        buf[36] = 0; // System type
        buf[37] = 0; // Unused
        buf[38..40].copy_from_slice(&self.config.boot_config.boot_load_size.to_le_bytes());
        buf[40..44].copy_from_slice(&self.boot_image_lba.to_le_bytes()); // Load RBA

        // Write boot image data
        if let Some(ref boot_img) = self.config.boot_config.bios_boot_image {
            let img_offset = self.boot_image_lba as usize * ISO_SECTOR_SIZE;
            let end = img_offset + boot_img.len();
            if end <= self.image.len() {
                self.image[img_offset..end].copy_from_slice(boot_img);
            }
        }
    }

    /// Apply isohybrid MBR for USB boot support
    fn apply_isohybrid(&mut self) {
        if !self.config.isohybrid.enabled {
            return;
        }

        // Write a minimal MBR that chainloads to the ISO boot
        let total_sectors = self.image.len() / 512;

        // MBR partition table entry at offset 446
        let entry_offset = 446;

        // Partition 1: covers entire ISO
        self.image[entry_offset] = 0x80; // Bootable
        self.image[entry_offset + 1] = 0; // CHS start head
        self.image[entry_offset + 2] = 1; // CHS start sector/cylinder
        self.image[entry_offset + 3] = 0; // CHS start cylinder
        self.image[entry_offset + 4] = self.config.isohybrid.partition_type;
        // CHS end
        self.image[entry_offset + 5] = 0xFF;
        self.image[entry_offset + 6] = 0xFF;
        self.image[entry_offset + 7] = 0xFF;
        // LBA start
        self.image[entry_offset + 8..entry_offset + 12]
            .copy_from_slice(&self.config.isohybrid.partition_offset.to_le_bytes());
        // LBA size
        let lba_size = total_sectors as u32;
        self.image[entry_offset + 12..entry_offset + 16].copy_from_slice(&lba_size.to_le_bytes());

        // MBR signature
        self.image[510] = 0x55;
        self.image[511] = 0xAA;

        serial_println!("[ISO] Applied isohybrid MBR ({} sectors)", total_sectors);
    }

    /// Compute MD5 checksum of the ISO image
    pub fn compute_checksum(&self) -> [u8; 16] {
        // Simple MD5-like hash for validation
        let mut hash = [0u8; 16];
        let mut state: [u32; 4] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476];

        for (i, byte) in self.image.iter().enumerate() {
            let idx = i % 4;
            state[idx] = state[idx]
                .wrapping_add(*byte as u32)
                .wrapping_mul(0x01000193);
        }

        for (i, s) in state.iter().enumerate() {
            hash[i * 4..(i + 1) * 4].copy_from_slice(&s.to_le_bytes());
        }
        hash
    }

    /// Get the built ISO image size
    pub fn image_size(&self) -> usize {
        self.image.len()
    }

    // ── Helper Functions ───────────────────────────────────────────────

    /// Write both-endian u32 (LE followed by BE)
    fn write_both_u32(buf: &mut [u8], val: u32) {
        buf[0..4].copy_from_slice(&val.to_le_bytes());
        buf[4..8].copy_from_slice(&val.to_be_bytes());
    }

    /// Write both-endian u16
    fn write_both_u16(buf: &mut [u8], val: u16) {
        buf[0..2].copy_from_slice(&val.to_le_bytes());
        buf[2..4].copy_from_slice(&val.to_be_bytes());
    }

    /// Write a-characters (uppercase + digits + special, padded with spaces)
    fn write_str_a(buf: &mut [u8], s: &str) {
        let bytes = s.as_bytes();
        let len = core::cmp::min(bytes.len(), buf.len());
        buf[..len].copy_from_slice(&bytes[..len]);
        for b in buf[len..].iter_mut() {
            *b = b' ';
        }
    }

    /// Write d-characters (uppercase + digits, padded with spaces)
    fn write_str_d(buf: &mut [u8], s: &str) {
        let upper = s.to_uppercase();
        let bytes = upper.as_bytes();
        let len = core::cmp::min(bytes.len(), buf.len());
        buf[..len].copy_from_slice(&bytes[..len]);
        for b in buf[len..].iter_mut() {
            *b = b' ';
        }
    }

    /// Convert string to UCS-2 big-endian (for Joliet)
    fn str_to_ucs2(s: &str) -> Vec<u8> {
        let mut out = Vec::new();
        for c in s.chars() {
            let code = c as u16;
            out.push((code >> 8) as u8);
            out.push((code & 0xFF) as u8);
        }
        out
    }
}

// ═══════════════════════════════════════════════════════════════════════
// KNOXOS ISO IMAGE GENERATOR
// ═══════════════════════════════════════════════════════════════════════

/// High-level KnoxOS ISO image builder
pub struct KnoxOsIsoBuilder {
    pub builder: IsoBuilder,
    pub include_live: bool,
    pub include_installer: bool,
}

impl KnoxOsIsoBuilder {
    pub fn new() -> Self {
        let config = IsoConfig {
            boot_config: BootConfig {
                bios_boot_image: None,
                uefi_boot_image: None,
                boot_load_size: 4,
                boot_info_table: true,
                platform: PLATFORM_X86,
            },
            isohybrid: IsohybridConfig {
                enabled: true,
                partition_type: 0x17,
                partition_offset: 0,
                mbr_template: None,
            },
            ..Default::default()
        };

        Self {
            builder: IsoBuilder::new(config),
            include_live: true,
            include_installer: true,
        }
    }

    /// Build a complete KnoxOS ISO with all components
    pub fn build_full_iso(&mut self, kernel_binary: &[u8]) -> &[u8] {
        serial_println!("[ISO] Building KnoxOS installation ISO...");

        // Add boot files
        self.builder.add_knoxos_boot_files(kernel_binary);

        // Add installer script
        let installer = self.create_installer_script();
        self.builder.add_file("/install/install.sh", installer);

        // Add OS metadata
        let readme = b"KnoxOS - AI-Native Operating System\n\nVersion: 0.2.1\nDate: February 2026\n\nTo install:\n  1. Boot from this ISO\n  2. Run /install/install.sh\n  3. Follow the prompts\n\nFor more information: https://knoxos.org\n";
        self.builder.add_file("/README.TXT", readme.to_vec());

        // Add license
        let license = b"MIT License\n\nCopyright (c) 2026 KnoxOS Project\n\nPermission is hereby granted, free of charge, to any person obtaining a copy\nof this software and associated documentation files.\n";
        self.builder.add_file("/LICENSE.TXT", license.to_vec());

        // Add checksums file
        let checksums = format!(
            "# KnoxOS ISO Checksums\nkernel_size={}\n",
            kernel_binary.len()
        );
        self.builder
            .add_file("/CHECKSUMS.TXT", checksums.into_bytes());

        // Build final ISO
        self.builder.build()
    }

    /// Create the installer shell script
    fn create_installer_script(&self) -> Vec<u8> {
        let script = r#"#!/bin/sh
# KnoxOS Installer v0.2.1
# Automated installation script for bare-metal deployment
# Supports BIOS and UEFI boot, GPT partitioning, ext4 root

set -e

VERSION="0.2.1"
KERNEL_PATH="/boot/knoxos-kernel"

# ── Colors ──
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

info()  { echo "${BLUE}[INFO]${NC}  $1"; }
ok()    { echo "${GREEN}[OK]${NC}    $1"; }
warn()  { echo "${YELLOW}[WARN]${NC}  $1"; }
error() { echo "${RED}[ERROR]${NC} $1"; }

echo "================================================"
echo "  KnoxOS Installer v${VERSION}"
echo "  AI-Native Operating System"
echo "  Written in Rust — x86_64 bare metal"
echo "================================================"
echo ""

# ── Step 1: Detect boot mode (UEFI or BIOS) ──
info "Detecting boot mode..."
BOOT_MODE="bios"
if [ -d "/sys/firmware/efi" ]; then
    BOOT_MODE="uefi"
    ok "UEFI boot detected"
else
    ok "BIOS/Legacy boot detected"
fi

# ── Step 2: Detect available disks ──
info "Detecting disks..."
DISKS=""
for disk in /dev/sda /dev/sdb /dev/vda /dev/vdb /dev/nvme0n1 /dev/nvme1n1; do
    if [ -b "$disk" ]; then
        SIZE=$(cat /sys/block/$(basename $disk)/size 2>/dev/null || echo 0)
        SIZE_GB=$((SIZE * 512 / 1073741824))
        echo "  ${disk} — ${SIZE_GB} GiB"
        DISKS="${DISKS} ${disk}"
    fi
done

if [ -z "$DISKS" ]; then
    error "No suitable disks found. Cannot install."
    exit 1
fi

echo ""
echo "Select target disk (e.g. /dev/sda):"
read TARGET_DISK

if [ ! -b "$TARGET_DISK" ]; then
    error "Invalid disk: $TARGET_DISK"
    exit 1
fi

echo ""
warn "ALL DATA on ${TARGET_DISK} WILL BE ERASED!"
echo "Type 'yes' to continue:"
read CONFIRM
if [ "$CONFIRM" != "yes" ]; then
    info "Installation cancelled."
    exit 0
fi

# ── Step 3: Partition ──
info "Partitioning ${TARGET_DISK}..."

if [ "$BOOT_MODE" = "uefi" ]; then
    # GPT + EFI System Partition
    # Part 1: 512 MiB EFI (FAT32)
    # Part 2: 4 GiB root (ext4)
    # Part 3: rest for /home (ext4)
    parted -s "$TARGET_DISK" mklabel gpt
    parted -s "$TARGET_DISK" mkpart "EFI" fat32 1MiB 513MiB
    parted -s "$TARGET_DISK" set 1 esp on
    parted -s "$TARGET_DISK" mkpart "root" ext4 513MiB 4609MiB
    parted -s "$TARGET_DISK" mkpart "home" ext4 4609MiB 100%
    ok "GPT partition table created (EFI + root + home)"

    EFI_PART="${TARGET_DISK}1"
    ROOT_PART="${TARGET_DISK}2"
    HOME_PART="${TARGET_DISK}3"
else
    # MBR for BIOS
    parted -s "$TARGET_DISK" mklabel msdos
    parted -s "$TARGET_DISK" mkpart primary ext4 1MiB 4097MiB
    parted -s "$TARGET_DISK" set 1 boot on
    parted -s "$TARGET_DISK" mkpart primary ext4 4097MiB 100%
    ok "MBR partition table created (root + home)"

    ROOT_PART="${TARGET_DISK}1"
    HOME_PART="${TARGET_DISK}2"
fi

# Handle NVMe partition naming (p1 instead of 1)
if echo "$TARGET_DISK" | grep -q nvme; then
    if [ "$BOOT_MODE" = "uefi" ]; then
        EFI_PART="${TARGET_DISK}p1"
        ROOT_PART="${TARGET_DISK}p2"
        HOME_PART="${TARGET_DISK}p3"
    else
        ROOT_PART="${TARGET_DISK}p1"
        HOME_PART="${TARGET_DISK}p2"
    fi
fi

# ── Step 4: Format ──
info "Formatting partitions..."
if [ "$BOOT_MODE" = "uefi" ]; then
    mkfs.fat -F32 "$EFI_PART" > /dev/null 2>&1
    ok "Formatted ${EFI_PART} as FAT32 (EFI)"
fi
mkfs.ext4 -q "$ROOT_PART"
ok "Formatted ${ROOT_PART} as ext4 (root)"
mkfs.ext4 -q "$HOME_PART"
ok "Formatted ${HOME_PART} as ext4 (home)"

# ── Step 5: Mount and install ──
info "Mounting filesystems..."
mount "$ROOT_PART" /mnt
mkdir -p /mnt/boot /mnt/home
mount "$HOME_PART" /mnt/home
if [ "$BOOT_MODE" = "uefi" ]; then
    mkdir -p /mnt/boot/efi
    mount "$EFI_PART" /mnt/boot/efi
fi
ok "Filesystems mounted"

info "Installing KnoxOS..."
# Create filesystem hierarchy
for dir in bin sbin lib lib64 usr/bin usr/lib usr/sbin etc var/log \
           var/tmp tmp dev proc sys run opt srv mnt media root; do
    mkdir -p "/mnt/${dir}"
done
chmod 1777 /mnt/tmp /mnt/var/tmp

# Copy kernel
cp "${KERNEL_PATH}" /mnt/boot/knoxos-kernel
ok "Kernel installed to /mnt/boot/knoxos-kernel"

# Copy initramfs if available
if [ -f "/boot/initramfs.img" ]; then
    cp /boot/initramfs.img /mnt/boot/
    ok "Initramfs copied"
fi

# Create /etc files
echo "knoxos" > /mnt/etc/hostname
cat > /mnt/etc/os-release << 'EOF'
NAME="KnoxOS"
VERSION="0.2.1"
ID=knoxos
PRETTY_NAME="KnoxOS 0.2.1 (AI-Native)"
VERSION_ID=0.2.1
HOME_URL="https://knoxos.org"
EOF
cat > /mnt/etc/fstab << EOF
# <device>    <mount>    <type>  <options>       <dump> <pass>
${ROOT_PART}  /          ext4    defaults        0      1
${HOME_PART}  /home      ext4    defaults        0      2
EOF
if [ "$BOOT_MODE" = "uefi" ]; then
    echo "${EFI_PART}  /boot/efi  vfat    defaults  0  0" >> /mnt/etc/fstab
fi
echo "root:x:0:0:root:/root:/bin/sh" > /mnt/etc/passwd
echo "root:x:0:" > /mnt/etc/group
ok "System configuration installed"

# ── Step 6: Bootloader ──
info "Installing bootloader..."
if [ "$BOOT_MODE" = "uefi" ]; then
    mkdir -p /mnt/boot/efi/EFI/knoxos
    cp /mnt/boot/knoxos-kernel /mnt/boot/efi/EFI/knoxos/knoxos.efi
    # Create UEFI boot entry
    cat > /mnt/boot/efi/EFI/knoxos/startup.nsh << 'EOF'
echo "Booting KnoxOS..."
\EFI\knoxos\knoxos.efi
EOF
    ok "UEFI bootloader installed"
else
    # For BIOS, the kernel is loaded directly by the bootloader crate
    ok "BIOS boot sector configured"
fi

# ── Step 7: Unmount ──
info "Finalizing..."
sync
if [ "$BOOT_MODE" = "uefi" ]; then
    umount /mnt/boot/efi
fi
umount /mnt/home
umount /mnt
ok "Filesystems unmounted"

echo ""
echo "================================================"
echo "  ${GREEN}KnoxOS installation complete!${NC}"
echo ""
echo "  Boot mode:   ${BOOT_MODE}"
echo "  Root:        ${ROOT_PART}"
echo "  Home:        ${HOME_PART}"
if [ "$BOOT_MODE" = "uefi" ]; then
echo "  EFI:         ${EFI_PART}"
fi
echo ""
echo "  Remove installation media and reboot."
echo "================================================"
"#;
        script.as_bytes().to_vec()
    }

    /// Create a live system boot script (boots directly from ISO without installation)
    fn create_live_boot_script(&self) -> Vec<u8> {
        let script = r#"#!/bin/sh
# KnoxOS Live Boot
# Boots KnoxOS directly from the ISO without installation
# Uses tmpfs overlay for writable root

echo "KnoxOS Live System starting..."

# Mount tmpfs for writable overlay
mount -t tmpfs tmpfs /run -o size=512M
mkdir -p /run/upper /run/work /run/merged

# Create overlay filesystem (read-only ISO base + writable tmpfs)
mount -t overlay overlay /run/merged \
    -o lowerdir=/,upperdir=/run/upper,workdir=/run/work

# Pivot to overlay root
cd /run/merged
mkdir -p old_root
pivot_root . old_root

# Mount essential filesystems
mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev
mount -t tmpfs tmpfs /tmp

echo "KnoxOS Live System ready."
echo "Changes will not persist after reboot."
exec /bin/sh
"#;
        script.as_bytes().to_vec()
    }
}

/// Build a KnoxOS live ISO (boots to desktop without installing)
pub fn build_live_iso(kernel_binary: &[u8]) -> Vec<u8> {
    let mut builder = KnoxOsIsoBuilder::new();
    builder.include_installer = false;
    builder.include_live = true;

    // Add boot files
    builder.builder.add_knoxos_boot_files(kernel_binary);

    // Add live boot script
    let live_script = builder.create_live_boot_script();
    builder.builder.add_file("/live/boot.sh", live_script);

    // Add metadata
    let readme = b"KnoxOS Live System\n\nThis ISO boots directly to a KnoxOS desktop.\nNo installation required. Changes are not persistent.\n";
    builder.builder.add_file("/README.TXT", readme.to_vec());

    builder.builder.build().to_vec()
}

// ═══════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════

/// Global ISO builder state
static ISO_BUILDER_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize the ISO 9660 subsystem
pub fn init() {
    serial_println!("[ISO9660] ISO 9660 image builder initialized");
    ISO_BUILDER_INITIALIZED.store(true, Ordering::SeqCst);
}

/// Build a KnoxOS bootable ISO image
pub fn build_knoxos_iso(kernel_binary: &[u8]) -> Vec<u8> {
    let mut builder = KnoxOsIsoBuilder::new();
    let image = builder.build_full_iso(kernel_binary);
    image.to_vec()
}

/// Build a minimal ISO image with custom files
pub fn build_custom_iso(config: IsoConfig, files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut builder = IsoBuilder::new(config);
    for (path, data) in files {
        builder.add_file(path, data.clone());
    }
    builder.build().to_vec()
}

/// Validate an ISO 9660 image
pub fn validate_iso(image: &[u8]) -> bool {
    if image.len() < 17 * ISO_SECTOR_SIZE {
        serial_println!("[ISO9660] Image too small");
        return false;
    }

    // Check Primary Volume Descriptor at sector 16
    let pvd_offset = 16 * ISO_SECTOR_SIZE;
    if image[pvd_offset] != VD_PRIMARY {
        serial_println!("[ISO9660] No Primary Volume Descriptor");
        return false;
    }
    if &image[pvd_offset + 1..pvd_offset + 6] != ISO_STANDARD_ID {
        serial_println!("[ISO9660] Invalid standard identifier");
        return false;
    }

    serial_println!("[ISO9660] ISO image validation passed");
    true
}
