/// Initramfs Boot Integration (P1)
///
/// Manages the early-boot sequence for KnoxOS using initramfs:
///
///   - Extract initramfs into VFS root
///   - Execute /init (PID 1) from initramfs
///   - Discover and mount real root filesystem
///   - switch_root: pivot from initramfs to real rootfs
///   - Clean up initramfs memory
///   - Build minimal initramfs images (busybox + /init script)
///
/// Boot flow:
///   1. Bootloader loads kernel + initramfs CPIO
///   2. Kernel extracts initramfs into tmpfs /
///   3. Kernel executes /init from initramfs
///   4. /init mounts real root (ext4 on /dev/sda1)
///   5. switch_root to real root, exec real /sbin/init
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use spin::Mutex;

use crate::cpio;
use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════

/// Boot stage tracking
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum BootStage {
    /// Kernel just started, no initramfs yet
    KernelInit = 0,
    /// Initramfs loaded and extracted
    InitramfsExtracted = 1,
    /// /init is running from initramfs
    InitRunning = 2,
    /// Real root filesystem discovered
    RootDiscovered = 3,
    /// Real root filesystem mounted
    RootMounted = 4,
    /// switch_root completed, running from real root
    SwitchedRoot = 5,
    /// Boot complete, real init (systemd/sysvinit) running
    BootComplete = 6,
}

impl From<u8> for BootStage {
    fn from(v: u8) -> Self {
        match v {
            0 => BootStage::KernelInit,
            1 => BootStage::InitramfsExtracted,
            2 => BootStage::InitRunning,
            3 => BootStage::RootDiscovered,
            4 => BootStage::RootMounted,
            5 => BootStage::SwitchedRoot,
            6 => BootStage::BootComplete,
            _ => BootStage::KernelInit,
        }
    }
}

/// Root filesystem specification (from kernel cmdline or probing)
#[derive(Debug, Clone)]
pub struct RootSpec {
    pub device: String,     // e.g., "/dev/sda1"
    pub fstype: String,     // e.g., "ext4"
    pub mount_opts: String, // e.g., "rw,errors=remount-ro"
    pub init_path: String,  // e.g., "/sbin/init"
}

impl Default for RootSpec {
    fn default() -> Self {
        Self {
            device: String::from("/dev/sda1"),
            fstype: String::from("ext4"),
            mount_opts: String::from("rw"),
            init_path: String::from("/sbin/init"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// GLOBAL STATE
// ═══════════════════════════════════════════════════════════════════════

static BOOT_STAGE: AtomicU8 = AtomicU8::new(0);
static SWITCH_ROOT_DONE: AtomicBool = AtomicBool::new(false);

lazy_static::lazy_static! {
    static ref ROOT_SPEC: Mutex<RootSpec> = Mutex::new(RootSpec::default());
    static ref INITRAMFS_PATHS: Mutex<Vec<String>> = Mutex::new(Vec::new());
}

pub fn get_boot_stage() -> BootStage {
    BootStage::from(BOOT_STAGE.load(Ordering::SeqCst))
}

fn set_boot_stage(stage: BootStage) {
    BOOT_STAGE.store(stage as u8, Ordering::SeqCst);
    serial_println!("[initramfs-boot] Boot stage: {:?}", stage);
}

// ═══════════════════════════════════════════════════════════════════════
// INITRAMFS EXTRACTION
// ═══════════════════════════════════════════════════════════════════════

/// Extract the initramfs CPIO archive into the VFS root
pub fn extract_initramfs() -> Result<usize, &'static str> {
    if !cpio::is_loaded() {
        return Err("No initramfs loaded");
    }

    serial_println!("[initramfs-boot] Extracting initramfs into VFS...");

    let initramfs = cpio::INITRAMFS.lock();
    let mut extracted = 0usize;
    let mut paths = Vec::new();

    for entry in &initramfs.entries {
        let path = normalize_path(&entry.name);

        if entry.is_dir() {
            crate::vfs::ensure_directory(&path);
            paths.push(path);
            extracted += 1;
        } else if entry.is_file() {
            // Extract file data
            let data = cpio::get_file_data(&initramfs.data, entry);
            // Ensure parent directory
            if let Some(parent) = parent_dir(&path) {
                crate::vfs::ensure_directory(&parent);
            }
            crate::vfs::create_file_dispatch(&path, data);
            paths.push(path);
            extracted += 1;
        } else if entry.is_symlink() {
            // Symlink target is the file data
            let target = cpio::get_file_data(&initramfs.data, entry);
            let target_str = core::str::from_utf8(target).unwrap_or("");
            crate::vfs::create_symlink_dispatch(&path, target_str);
            paths.push(path);
            extracted += 1;
        }
    }

    *INITRAMFS_PATHS.lock() = paths;

    serial_println!(
        "[initramfs-boot] Extracted {} entries ({} files, {} dirs)",
        extracted,
        initramfs.total_files,
        initramfs.total_dirs
    );

    set_boot_stage(BootStage::InitramfsExtracted);
    Ok(extracted)
}

/// Normalize a CPIO path to absolute
fn normalize_path(name: &str) -> String {
    let stripped = name.trim_start_matches("./").trim_start_matches('/');
    if stripped.is_empty() {
        String::from("/")
    } else {
        format!("/{}", stripped)
    }
}

/// Get parent directory of a path
fn parent_dir(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    if let Some(pos) = trimmed.rfind('/') {
        if pos == 0 {
            Some(String::from("/"))
        } else {
            Some(String::from(&trimmed[..pos]))
        }
    } else {
        None
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ROOT FILESYSTEM DISCOVERY
// ═══════════════════════════════════════════════════════════════════════

/// Parse kernel command line for root= parameter
pub fn parse_root_from_cmdline(cmdline: &str) -> Option<RootSpec> {
    let mut spec = RootSpec::default();
    let mut found_root = false;

    for param in cmdline.split_whitespace() {
        if let Some(value) = param.strip_prefix("root=") {
            spec.device = String::from(value);
            found_root = true;
        } else if let Some(value) = param.strip_prefix("rootfstype=") {
            spec.fstype = String::from(value);
        } else if let Some(value) = param.strip_prefix("rootflags=") {
            spec.mount_opts = String::from(value);
        } else if let Some(value) = param.strip_prefix("init=") {
            spec.init_path = String::from(value);
        } else if param == "ro" {
            spec.mount_opts = String::from("ro");
        } else if param == "rw" {
            spec.mount_opts = String::from("rw");
        }
    }

    if found_root { Some(spec) } else { None }
}

/// Probe for root filesystem on available block devices
pub fn discover_root_filesystem() -> Result<RootSpec, &'static str> {
    serial_println!("[initramfs-boot] Discovering root filesystem...");

    // First check kernel cmdline
    let cmdline = crate::vfs::read_file_dispatch("/proc/cmdline")
        .map(|data| String::from_utf8_lossy(&data).to_string())
        .unwrap_or_default();

    if let Some(spec) = parse_root_from_cmdline(&cmdline) {
        serial_println!(
            "[initramfs-boot] Root from cmdline: {} ({})",
            spec.device,
            spec.fstype
        );
        *ROOT_SPEC.lock() = spec.clone();
        set_boot_stage(BootStage::RootDiscovered);
        return Ok(spec);
    }

    // Probe block devices for known filesystems
    let devices = vec![
        "/dev/sda1",
        "/dev/sda2",
        "/dev/sda",
        "/dev/nvme0n1p1",
        "/dev/nvme0n1p2",
        "/dev/vda1",
        "/dev/vda",
    ];

    for dev in &devices {
        if let Some(fstype) = probe_filesystem(dev) {
            serial_println!("[initramfs-boot] Found {} on {}", fstype, dev);
            let spec = RootSpec {
                device: String::from(*dev),
                fstype,
                mount_opts: String::from("rw"),
                init_path: String::from("/sbin/init"),
            };
            *ROOT_SPEC.lock() = spec.clone();
            set_boot_stage(BootStage::RootDiscovered);
            return Ok(spec);
        }
    }

    Err("No root filesystem found")
}

/// Probe a block device for its filesystem type
fn probe_filesystem(device: &str) -> Option<String> {
    // Read first 4KB to check superblock signatures
    let data = crate::vfs::read_file_dispatch(device)?;
    if data.len() < 2048 {
        return None;
    }

    // ext2/3/4: magic at offset 1080 = 0x438
    if data.len() >= 0x43a {
        let magic = u16::from_le_bytes([data[0x438], data[0x439]]);
        if magic == 0xEF53 {
            // Check for ext4 features
            if data.len() >= 0x464 {
                let incompat =
                    u32::from_le_bytes([data[0x460], data[0x461], data[0x462], data[0x463]]);
                if incompat & 0x0040 != 0 {
                    // EXTENTS feature → ext4
                    return Some(String::from("ext4"));
                }
            }
            return Some(String::from("ext2"));
        }
    }

    // FAT32: check BPB signature
    if data.len() >= 512 && data[510] == 0x55 && data[511] == 0xAA && &data[82..90] == b"FAT32   " {
        return Some(String::from("vfat"));
    }

    // Btrfs: magic at offset 0x10040
    if data.len() >= 0x10048 && &data[0x10040..0x10048] == b"_BHRfS_M" {
        return Some(String::from("btrfs"));
    }

    None
}

// ═══════════════════════════════════════════════════════════════════════
// MOUNT ROOT & SWITCH ROOT
// ═══════════════════════════════════════════════════════════════════════

/// Mount the real root filesystem
pub fn mount_real_root() -> Result<(), &'static str> {
    let spec = ROOT_SPEC.lock().clone();
    serial_println!(
        "[initramfs-boot] Mounting {} ({}) at /mnt/root...",
        spec.device,
        spec.fstype
    );

    crate::vfs::ensure_directory("/mnt/root");

    // Mount the real root filesystem
    match crate::mount::do_mount(&spec.device, "/mnt/root", &spec.fstype, &spec.mount_opts) {
        Ok(()) => {
            serial_println!("[initramfs-boot] Root filesystem mounted at /mnt/root");
            set_boot_stage(BootStage::RootMounted);
            Ok(())
        }
        Err(e) => {
            serial_println!("[initramfs-boot] Failed to mount root: {}", e);
            Err("mount failed")
        }
    }
}

/// switch_root: Pivot from initramfs to the real root filesystem
///
/// This is the critical transition point. We:
///   1. Move mount points from initramfs to new root
///   2. chroot/pivot_root to new root
///   3. Free initramfs memory
///   4. exec real /sbin/init
pub fn switch_root() -> Result<(), &'static str> {
    let spec = ROOT_SPEC.lock().clone();
    serial_println!("[initramfs-boot] switch_root to /mnt/root...");

    // Verify real root has required files
    let init_path = format!("/mnt/root{}", spec.init_path);
    if crate::vfs::read_file_dispatch(&init_path).is_none() {
        // Try fallback init paths
        let fallbacks = ["/sbin/init", "/bin/init", "/init", "/lib/systemd/systemd"];
        let mut found = false;
        for fb in &fallbacks {
            let full = format!("/mnt/root{}", fb);
            if crate::vfs::read_file_dispatch(&full).is_some() {
                serial_println!("[initramfs-boot] Using {} as init", fb);
                ROOT_SPEC.lock().init_path = String::from(*fb);
                found = true;
                break;
            }
        }
        if !found {
            serial_println!("[initramfs-boot] WARNING: No init found, using emergency shell");
        }
    }

    // Step 1: Move special mounts to new root
    move_mount("/proc", "/mnt/root/proc");
    move_mount("/sys", "/mnt/root/sys");
    move_mount("/dev", "/mnt/root/dev");
    move_mount("/run", "/mnt/root/run");

    // Step 2: Delete initramfs files (free memory)
    cleanup_initramfs();

    // Step 3: Pivot root
    // In a real kernel, this uses pivot_root(2) or chroot(2)
    // We simulate by reassigning the VFS root mount point
    crate::vfs::set_root("/mnt/root");

    // Step 4: Mark as switched
    SWITCH_ROOT_DONE.store(true, Ordering::SeqCst);
    set_boot_stage(BootStage::SwitchedRoot);

    serial_println!("[initramfs-boot] switch_root complete — running from real rootfs");

    // Step 5: Execute real init
    let init = ROOT_SPEC.lock().init_path.clone();
    serial_println!("[initramfs-boot] Executing {} as PID 1...", init);
    crate::elf::exec_elf_path(&init, &["--system"]);

    set_boot_stage(BootStage::BootComplete);
    Ok(())
}

/// Move a mount point from old location to new location
fn move_mount(from: &str, to: &str) {
    crate::vfs::ensure_directory(to);
    if let Err(e) = crate::mount::do_move_mount(from, to) {
        serial_println!(
            "[initramfs-boot] Warning: failed to move mount {} -> {}: {}",
            from,
            to,
            e
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// INITRAMFS CLEANUP
// ═══════════════════════════════════════════════════════════════════════

/// Free initramfs data (reclaim memory after switch_root)
fn cleanup_initramfs() {
    serial_println!("[initramfs-boot] Cleaning up initramfs...");

    let paths = INITRAMFS_PATHS.lock().clone();
    let mut freed = 0usize;

    // Delete files first, then directories (reverse order)
    for path in paths.iter().rev() {
        if crate::vfs::remove_dispatch(path).is_ok() {
            freed += 1;
        }
    }

    // Clear the CPIO data buffer
    {
        let mut initramfs = cpio::INITRAMFS.lock();
        let data_size = initramfs.data.len();
        initramfs.data.clear();
        initramfs.entries.clear();
        serial_println!(
            "[initramfs-boot] Freed {} initramfs entries, {} bytes",
            freed,
            data_size
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// INITRAMFS IMAGE BUILDER
// ═══════════════════════════════════════════════════════════════════════

/// Build a minimal initramfs CPIO image
///
/// Includes:
///   - /init script (shell script to mount root and switch_root)
///   - /bin/busybox (if available)
///   - /etc/fstab
///   - Essential device nodes
pub fn build_initramfs(root_device: &str, root_fstype: &str) -> Vec<u8> {
    serial_println!("[initramfs-boot] Building initramfs image...");

    let mut cpio_data = Vec::new();
    let mut ino_counter = 300000u32;

    // /init script
    let init_script = format!(
        "#!/bin/sh\n\
         # KnoxOS initramfs /init\n\
         \n\
         mount -t proc proc /proc\n\
         mount -t sysfs sysfs /sys\n\
         mount -t devtmpfs devtmpfs /dev\n\
         \n\
         echo \"KnoxOS initramfs starting...\"\n\
         \n\
         # Mount real root\n\
         mkdir -p /mnt/root\n\
         mount -t {} {} /mnt/root\n\
         \n\
         if [ ! -x /mnt/root/sbin/init ]; then\n\
             echo \"ERROR: No init found on root filesystem\"\n\
             exec /bin/sh\n\
         fi\n\
         \n\
         # Switch to real root\n\
         exec switch_root /mnt/root /sbin/init\n",
        root_fstype, root_device
    );

    // Directories
    let dirs = [
        ".", "bin", "dev", "etc", "lib", "lib64", "mnt", "mnt/root", "proc", "run", "sbin", "sys",
        "tmp", "usr", "usr/bin", "usr/lib", "usr/sbin",
    ];

    for dir in &dirs {
        ino_counter += 1;
        write_cpio_entry(
            &mut cpio_data,
            dir,
            ino_counter,
            cpio::S_IFDIR | 0o755,
            0,
            0,
            &[],
        );
    }

    // /init script (executable)
    ino_counter += 1;
    write_cpio_entry(
        &mut cpio_data,
        "init",
        ino_counter,
        cpio::S_IFREG | 0o755,
        0,
        0,
        init_script.as_bytes(),
    );

    // /etc/fstab
    let fstab = format!(
        "# /etc/fstab: initramfs\n\
         {} / {} defaults 0 1\n\
         proc /proc proc defaults 0 0\n\
         sysfs /sys sysfs defaults 0 0\n\
         devtmpfs /dev devtmpfs defaults 0 0\n",
        root_device, root_fstype
    );
    ino_counter += 1;
    write_cpio_entry(
        &mut cpio_data,
        "etc/fstab",
        ino_counter,
        cpio::S_IFREG | 0o644,
        0,
        0,
        fstab.as_bytes(),
    );

    // TRAILER
    write_cpio_entry(&mut cpio_data, "TRAILER!!!", 0, 0, 0, 0, &[]);

    // Pad to 512-byte boundary
    while cpio_data.len() % 512 != 0 {
        cpio_data.push(0);
    }

    serial_println!(
        "[initramfs-boot] Built initramfs: {} bytes",
        cpio_data.len()
    );
    cpio_data
}

/// Write a single CPIO newc entry
fn write_cpio_entry(
    buf: &mut Vec<u8>,
    name: &str,
    ino: u32,
    mode: u32,
    uid: u32,
    gid: u32,
    data: &[u8],
) {
    let namesize = name.len() + 1; // include null terminator
    let filesize = data.len();

    // Header (110 bytes)
    let header = format!(
        "070701\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}",
        ino,
        mode,
        uid,
        gid,
        1u32, // nlink
        0u32, // mtime
        filesize as u32,
        0u32, // devmajor
        0u32, // devminor
        0u32, // rdevmajor
        0u32, // rdevminor
        namesize as u32,
        0u32, // check
    );

    buf.extend_from_slice(header.as_bytes());

    // Filename + null terminator
    buf.extend_from_slice(name.as_bytes());
    buf.push(0);

    // Pad to 4-byte boundary after name
    while (buf.len()) % 4 != 0 {
        buf.push(0);
    }

    // File data
    buf.extend_from_slice(data);

    // Pad to 4-byte boundary after data
    while (buf.len()) % 4 != 0 {
        buf.push(0);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// FULL BOOT SEQUENCE
// ═══════════════════════════════════════════════════════════════════════

/// Run the complete initramfs boot sequence
pub fn boot_sequence() -> Result<(), &'static str> {
    serial_println!("[initramfs-boot] ═══════════════════════════════════════");
    serial_println!("[initramfs-boot]   KnoxOS Initramfs Boot Sequence");
    serial_println!("[initramfs-boot] ═══════════════════════════════════════");

    // Stage 1: Extract initramfs
    serial_println!("[initramfs-boot] Stage 1: Extracting initramfs...");
    let count = extract_initramfs()?;
    serial_println!("[initramfs-boot] Extracted {} entries", count);

    // Stage 2: Discover root filesystem
    serial_println!("[initramfs-boot] Stage 2: Discovering root filesystem...");
    let spec = discover_root_filesystem()?;
    serial_println!("[initramfs-boot] Root: {} ({})", spec.device, spec.fstype);

    // Stage 3: Mount real root
    serial_println!("[initramfs-boot] Stage 3: Mounting real root filesystem...");
    mount_real_root()?;

    // Stage 4: switch_root
    serial_println!("[initramfs-boot] Stage 4: switch_root...");
    switch_root()?;

    serial_println!("[initramfs-boot] ═══════════════════════════════════════");
    serial_println!("[initramfs-boot]   Boot sequence complete!");
    serial_println!("[initramfs-boot] ═══════════════════════════════════════");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

pub fn init() {
    serial_println!("[initramfs-boot] Initramfs boot subsystem initialized");
    serial_println!("[initramfs-boot]   Boot stage: {:?}", get_boot_stage());

    // If initramfs is already loaded (from bootloader), auto-start
    if cpio::is_loaded() {
        serial_println!("[initramfs-boot] Initramfs detected, starting boot sequence...");
        if let Err(e) = boot_sequence() {
            serial_println!(
                "[initramfs-boot] Boot failed: {}, falling back to emergency shell",
                e
            );
        }
    } else {
        serial_println!("[initramfs-boot] No initramfs loaded, skipping boot sequence");
        serial_println!("[initramfs-boot] Continuing with in-memory rootfs");
    }
}
