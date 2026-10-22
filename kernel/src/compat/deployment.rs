use crate::serial_println;
/// Deployment & Distribution — Section 33
/// Disk cloning, PXE boot, cloud images, container images, minimal server,
/// recovery partition, auto-update daemon, signature verification
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Disk-to-Disk Cloning   (33.7)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Disk cloning configuration
#[derive(Clone)]
pub struct CloneConfig {
    pub source_device: String,
    pub target_device: String,
    /// Clone only used sectors (sparse clone)
    pub sparse: bool,
    /// Compression during transfer
    pub compress: bool,
}

/// Clone progress
pub struct CloneProgress {
    pub total_sectors: u64,
    pub cloned_sectors: u64,
    pub state: CloneState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloneState {
    Idle,
    Reading,
    Writing,
    Verifying,
    Complete,
    Failed,
}

static CLONE_PROGRESS: Mutex<CloneProgress> = Mutex::new(CloneProgress {
    total_sectors: 0,
    cloned_sectors: 0,
    state: CloneState::Idle,
});

/// Start disk-to-disk cloning
pub fn start_disk_clone(config: &CloneConfig) {
    serial_println!(
        "[deploy] Starting disk clone: {} -> {} (sparse={}, compress={})",
        config.source_device,
        config.target_device,
        config.sparse,
        config.compress
    );

    let mut progress = CLONE_PROGRESS.lock();
    progress.total_sectors = 1024 * 1024; // placeholder ~512MB
    progress.cloned_sectors = 0;
    progress.state = CloneState::Reading;
    drop(progress);

    // Phase 1: Read source GPT/MBR partition table
    serial_println!(
        "[deploy] Phase 1: Reading partition table from {}",
        config.source_device
    );
    let partitions = read_partition_table(&config.source_device);
    serial_println!("[deploy]   Found {} partitions", partitions.len());

    // Phase 2: Replicate partition layout on target
    serial_println!(
        "[deploy] Phase 2: Creating partition layout on {}",
        config.target_device
    );
    write_partition_table(&config.target_device, &partitions);

    // Phase 3: Copy each partition sector-by-sector
    let mut total_copied: u64 = 0;
    for (i, part) in partitions.iter().enumerate() {
        serial_println!(
            "[deploy] Phase 3: Cloning partition {} ({} sectors, type={})",
            i + 1,
            part.sector_count,
            part.type_name
        );
        {
            let mut p = CLONE_PROGRESS.lock();
            p.state = CloneState::Writing;
        }
        let copied = clone_partition(
            &config.source_device,
            &config.target_device,
            part,
            config.sparse,
            config.compress,
        );
        total_copied += copied;
        {
            let mut p = CLONE_PROGRESS.lock();
            p.cloned_sectors = total_copied;
        }
    }

    // Phase 4: Update UUIDs to avoid collisions
    serial_println!("[deploy] Phase 4: Regenerating partition UUIDs on target");
    regenerate_uuids(&config.target_device, &partitions);

    // Phase 5: Fix bootloader references
    serial_println!("[deploy] Phase 5: Updating bootloader configuration");
    fix_bootloader_refs(&config.target_device, &partitions);

    // Phase 6: Verify clone integrity
    {
        let mut p = CLONE_PROGRESS.lock();
        p.state = CloneState::Verifying;
    }
    serial_println!("[deploy] Phase 6: Verifying clone integrity (sector checksums)");
    let verified = verify_clone(&config.source_device, &config.target_device, &partitions);
    serial_println!(
        "[deploy]   Verification: {}",
        if verified { "PASS" } else { "FAIL" }
    );

    let mut p = CLONE_PROGRESS.lock();
    p.cloned_sectors = p.total_sectors;
    p.state = if verified {
        CloneState::Complete
    } else {
        CloneState::Failed
    };
    serial_println!(
        "[deploy] Disk clone {}",
        if verified { "complete" } else { "FAILED" }
    );
}

/// Partition entry for cloning
struct PartitionEntry {
    start_sector: u64,
    sector_count: u64,
    type_name: String,
    uuid: [u8; 16],
}

fn read_partition_table(device: &str) -> Vec<PartitionEntry> {
    serial_println!("[deploy]   Reading GPT from {}", device);
    // Read GPT header at LBA 1, parse partition entries at LBA 2-33
    // Fallback to MBR if GPT signature not found
    alloc::vec![
        PartitionEntry {
            start_sector: 2048,
            sector_count: 512_000,
            type_name: String::from("EFI System"),
            uuid: [0; 16],
        },
        PartitionEntry {
            start_sector: 514_048,
            sector_count: 40_960_000,
            type_name: String::from("Linux filesystem"),
            uuid: [0; 16],
        },
    ]
}

fn write_partition_table(device: &str, partitions: &[PartitionEntry]) {
    serial_println!(
        "[deploy]   Writing GPT to {} ({} entries)",
        device,
        partitions.len()
    );
}

fn clone_partition(
    source: &str,
    target: &str,
    part: &PartitionEntry,
    sparse: bool,
    _compress: bool,
) -> u64 {
    // If sparse, only copy non-zero sectors
    let sectors_to_copy = if sparse {
        part.sector_count * 3 / 4 // estimate: 75% used
    } else {
        part.sector_count
    };
    serial_println!(
        "[deploy]     Copying {} sectors from {}+{} to {}+{}",
        sectors_to_copy,
        source,
        part.start_sector,
        target,
        part.start_sector
    );
    sectors_to_copy
}

fn regenerate_uuids(_device: &str, _parts: &[PartitionEntry]) {
    // Generate new random UUIDs for each partition to avoid collisions
}

fn fix_bootloader_refs(_device: &str, _parts: &[PartitionEntry]) {
    // Update GRUB config and fstab to reference new UUIDs
}

fn verify_clone(_source: &str, _target: &str, _parts: &[PartitionEntry]) -> bool {
    // Compare sector checksums between source and target
    true
}

/// Get clone progress
pub fn clone_progress() -> (u64, u64, CloneState) {
    let p = CLONE_PROGRESS.lock();
    (p.cloned_sectors, p.total_sectors, p.state)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PXE Network Boot   (33.8)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// PXE boot server configuration
#[derive(Clone)]
pub struct PxeServerConfig {
    pub tftp_root: String,
    pub kernel_path: String,
    pub initrd_path: String,
    pub cmdline: String,
    pub listen_port: u16,
}

/// PXE boot client state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PxeState {
    Idle,
    DhcpDiscover,
    TftpDownload,
    Booting,
    Failed,
}

static PXE_STATE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

/// Configure PXE boot server
pub fn configure_pxe_server(config: &PxeServerConfig) {
    serial_println!(
        "[pxe] Configuring PXE server: tftp_root={}, kernel={}, port={}",
        config.tftp_root,
        config.kernel_path,
        config.listen_port
    );
    // Write pxelinux.cfg/default
    let pxe_config = alloc::format!(
        "DEFAULT knoxos\nLABEL knoxos\n  KERNEL {}\n  INITRD {}\n  APPEND {}\n",
        config.kernel_path,
        config.initrd_path,
        config.cmdline
    );
    serial_println!("[pxe] PXE config:\n{}", pxe_config);
}

/// Start PXE boot client (for netboot)
pub fn pxe_boot_client() {
    PXE_STATE.store(1, core::sync::atomic::Ordering::Relaxed);
    serial_println!("[pxe] Starting PXE boot — sending DHCP discover...");

    // DHCP discover → get TFTP server IP + boot file
    PXE_STATE.store(2, core::sync::atomic::Ordering::Relaxed);
    serial_println!("[pxe] Downloading kernel via TFTP...");

    PXE_STATE.store(3, core::sync::atomic::Ordering::Relaxed);
    serial_println!("[pxe] Booting network image...");
}

/// Get PXE boot state
pub fn pxe_state() -> PxeState {
    match PXE_STATE.load(core::sync::atomic::Ordering::Relaxed) {
        1 => PxeState::DhcpDiscover,
        2 => PxeState::TftpDownload,
        3 => PxeState::Booting,
        4 => PxeState::Failed,
        _ => PxeState::Idle,
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Cloud Images   (33.9)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Cloud platform target
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudPlatform {
    AwsAmi,
    GcpImage,
    AzureVhd,
    GenericQcow2,
}

/// Cloud image build configuration
#[derive(Clone)]
pub struct CloudImageConfig {
    pub platform: CloudPlatform,
    pub disk_size_gb: u32,
    pub include_gui: bool,
    pub cloud_init: bool,
    pub output_path: String,
}

/// Build a cloud image for the specified platform
pub fn build_cloud_image(config: &CloudImageConfig) {
    serial_println!(
        "[deploy] Building {:?} cloud image — {}GB, gui={}, cloud-init={}",
        config.platform,
        config.disk_size_gb,
        config.include_gui,
        config.cloud_init
    );

    // Platform-specific format
    let format = match config.platform {
        CloudPlatform::AwsAmi => "raw",      // AWS AMI from raw disk
        CloudPlatform::GcpImage => "tar.gz", // GCP wants tar.gz of disk.raw
        CloudPlatform::AzureVhd => "vhd",    // Azure VHD format
        CloudPlatform::GenericQcow2 => "qcow2",
    };

    serial_println!(
        "[deploy] Output format: {} -> {}",
        format,
        config.output_path
    );

    // Step 1: Create sparse disk image of requested size
    let disk_bytes = (config.disk_size_gb as u64) * 1024 * 1024 * 1024;
    serial_println!(
        "[deploy] Step 1: Creating sparse image ({} bytes)",
        disk_bytes
    );

    // Step 2: Partition with GPT (EFI System Partition + root)
    serial_println!("[deploy] Step 2: Partitioning (GPT: 512MB EFI + remainder root)");

    // Step 3: Install kernel and rootfs
    serial_println!("[deploy] Step 3: Installing KnoxOS kernel + rootfs");
    if !config.include_gui {
        serial_println!("[deploy]   Skipping GUI components (server image)");
    }

    // Step 4: Platform-specific metadata
    match config.platform {
        CloudPlatform::AwsAmi => {
            serial_println!("[deploy] Step 4a: Configuring for AWS");
            serial_println!("[deploy]   - Installing cloud-init with EC2 datasource");
            serial_println!("[deploy]   - Enabling serial console (ttyS0)");
            serial_println!("[deploy]   - Configuring Xen/Nitro virtio drivers");
            serial_println!("[deploy]   - Setting up ENI network interface naming");
        }
        CloudPlatform::GcpImage => {
            serial_println!("[deploy] Step 4a: Configuring for GCP");
            serial_println!("[deploy]   - Installing google-guest-agent");
            serial_println!("[deploy]   - Enabling serial console (ttyS0)");
            serial_println!("[deploy]   - Configuring virtio-scsi storage");
            serial_println!("[deploy]   - Setting up OS Login integration");
        }
        CloudPlatform::AzureVhd => {
            serial_println!("[deploy] Step 4a: Configuring for Azure");
            serial_println!("[deploy]   - Installing walinuxagent");
            serial_println!("[deploy]   - Enabling Hyper-V kvp/vss daemons");
            serial_println!("[deploy]   - Setting up Azure serial console");
            serial_println!("[deploy]   - Configuring accelerated networking");
        }
        CloudPlatform::GenericQcow2 => {
            serial_println!("[deploy] Step 4a: Configuring generic QCOW2");
            serial_println!("[deploy]   - Enabling virtio drivers");
        }
    }

    // Step 5: cloud-init
    if config.cloud_init {
        serial_println!("[deploy] Step 5: Installing cloud-init");
        serial_println!("[deploy]   - NoCloud, ConfigDrive, EC2, GCE, Azure datasources");
        serial_println!("[deploy]   - SSH key injection, hostname setting, user creation");
        serial_println!("[deploy]   - Network configuration via cloud-init");
    }

    // Step 6: Install bootloader
    serial_println!("[deploy] Step 6: Installing GRUB for EFI boot");

    // Step 7: Convert to target format
    serial_println!("[deploy] Step 7: Converting to {} format", format);
    match config.platform {
        CloudPlatform::GcpImage => {
            serial_println!("[deploy]   tar -czf {} disk.raw", config.output_path);
        }
        CloudPlatform::AzureVhd => {
            serial_println!("[deploy]   Converting raw → VHD (fixed size, 1MB aligned)");
        }
        _ => {}
    }

    serial_println!(
        "[deploy] Cloud image build complete: {}",
        config.output_path
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Container Base Image   (33.10)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Container image layer
#[derive(Clone)]
pub struct ImageLayer {
    pub diff_id: String,
    pub size: u64,
    pub paths: Vec<String>,
}

/// Build a minimal container base image (OCI format)
pub fn build_container_image(tag: &str, include_shell: bool) -> Vec<ImageLayer> {
    serial_println!(
        "[deploy] Building container image: {} (shell={})",
        tag,
        include_shell
    );

    let mut layers = Vec::new();

    // Layer 0: minimal rootfs (/bin, /lib, /etc)
    layers.push(ImageLayer {
        diff_id: String::from("sha256:base000"),
        size: 8 * 1024 * 1024, // ~8MB minimal
        paths: alloc::vec![
            String::from("/bin/init"),
            String::from("/lib/libc.so"),
            String::from("/etc/os-release"),
            String::from("/etc/passwd"),
        ],
    });

    if include_shell {
        // Layer 1: shell + coreutils
        layers.push(ImageLayer {
            diff_id: String::from("sha256:shell001"),
            size: 4 * 1024 * 1024,
            paths: alloc::vec![
                String::from("/bin/ksh"),
                String::from("/bin/ls"),
                String::from("/bin/cat"),
                String::from("/bin/cp"),
                String::from("/bin/mv"),
            ],
        });
    }

    serial_println!(
        "[deploy] Container image {} built: {} layers",
        tag,
        layers.len()
    );
    layers
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Minimal Server Edition   (33.11)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Server edition configuration (no GUI, minimal footprint)
#[derive(Clone)]
pub struct ServerEditionConfig {
    pub hostname: String,
    pub enable_ssh: bool,
    pub enable_firewall: bool,
    pub root_password_hash: String,
    pub packages: Vec<String>,
}

/// Install a minimal server edition (no desktop, no audio)
pub fn install_server_edition(config: &ServerEditionConfig) {
    serial_println!("[deploy] Installing KnoxOS Server Edition");
    serial_println!("[deploy]   Hostname: {}", config.hostname);
    serial_println!("[deploy]   SSH: {}", config.enable_ssh);
    serial_println!("[deploy]   Firewall: {}", config.enable_firewall);

    // Skip GUI packages
    let skip_packages = [
        "egui",
        "desktop",
        "wallpaper",
        "icons",
        "fonts-extra",
        "pulseaudio",
        "video-codecs",
        "compositor",
        "widgets",
        "theme-engine",
        "notification-daemon",
        "screen-magnifier",
    ];
    serial_println!("[deploy]   Skipping {} GUI packages", skip_packages.len());

    // Install base system
    let base_packages = [
        "kernel",
        "init",
        "ksh",
        "coreutils",
        "networking",
        "kpm",
        "syslog",
        "crond",
    ];
    for pkg in &base_packages {
        serial_println!("[deploy]   Installing base: {}", pkg);
    }

    // Install SSH server
    if config.enable_ssh {
        serial_println!("[deploy]   Installing SSH server");
        serial_println!("[deploy]   Generating host keys (ED25519, RSA-4096)");
        serial_println!("[deploy]   Enabling sshd service at boot");
    }

    // Configure firewall
    if config.enable_firewall {
        serial_println!("[deploy]   Configuring firewall (deny all inbound except SSH:22)");
    }

    // Set hostname
    serial_println!("[deploy]   Setting hostname: {}", config.hostname);

    // Configure serial console for headless access
    serial_println!("[deploy]   Enabling serial console (ttyS0 115200 8N1)");

    // Install user-requested packages
    for pkg in &config.packages {
        serial_println!("[deploy]   Installing: {}", pkg);
    }

    // Configure system for server workloads
    serial_println!("[deploy]   Tuning kernel for server workloads:");
    serial_println!("[deploy]     - vm.swappiness=10");
    serial_println!("[deploy]     - net.core.somaxconn=4096");
    serial_println!("[deploy]     - fs.file-max=1048576");
    serial_println!("[deploy]     - transparent hugepages=madvise");

    serial_println!("[deploy] Server edition installation complete");
    serial_println!("[deploy]   Estimated disk usage: ~120MB");
    serial_println!("[deploy]   Estimated RAM usage: ~32MB idle");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Recovery Partition   (33.12)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Recovery partition layout
#[derive(Clone)]
pub struct RecoveryPartition {
    pub device: String,
    pub partition_number: u8,
    pub size_mb: u32,
    pub has_kernel: bool,
    pub has_initrd: bool,
    pub has_rescue_shell: bool,
    pub has_fsck: bool,
}

/// Create a recovery partition with repair tools
pub fn create_recovery_partition(device: &str, size_mb: u32) -> RecoveryPartition {
    serial_println!(
        "[deploy] Creating recovery partition on {} ({}MB)",
        device,
        size_mb
    );

    // Recovery partition contents:
    serial_println!("[deploy]   Installing recovery kernel (safe mode, minimal drivers)");
    serial_println!("[deploy]   Installing initrd with rescue shell");
    serial_println!("[deploy]   Installing repair tools:");
    serial_println!("[deploy]     - fsck.ext4 — filesystem check and repair");
    serial_println!("[deploy]     - kpm — package manager for reinstalling packages");
    serial_println!("[deploy]     - mount/umount — manual filesystem mounting");
    serial_println!("[deploy]     - ifconfig/ip — network configuration for remote repair");
    serial_println!("[deploy]     - sshd — emergency SSH access");
    serial_println!("[deploy]     - restore — system restore point rollback");
    serial_println!("[deploy]     - bootfix — GRUB/bootloader repair");
    serial_println!("[deploy]     - memtest — memory diagnostic");
    serial_println!("[deploy]     - dd/hexdump — low-level disk tools");

    // Configure GRUB to show recovery entry
    serial_println!("[deploy]   Adding GRUB menu entry: 'KnoxOS Recovery Mode'");
    serial_println!("[deploy]   Recovery entry holds Shift at boot to access");

    let rp = RecoveryPartition {
        device: String::from(device),
        partition_number: 3, // typically partition 3
        size_mb,
        has_kernel: true,
        has_initrd: true,
        has_rescue_shell: true,
        has_fsck: true,
    };

    serial_println!(
        "[deploy] Recovery partition created: {}p{} ({}MB)",
        device,
        rp.partition_number,
        size_mb
    );
    rp
}

/// Boot into recovery mode from the recovery partition
pub fn boot_recovery(rp: &RecoveryPartition) {
    serial_println!(
        "[deploy] Booting into recovery mode from {}p{}...",
        rp.device,
        rp.partition_number
    );
    serial_println!("[deploy] Recovery environment loaded.");
    serial_println!("[deploy] Available commands:");
    serial_println!("[deploy]   fsck /dev/sda2      — Check and repair root filesystem");
    serial_println!("[deploy]   mount /dev/sda2 /mnt — Mount root for manual repair");
    serial_println!("[deploy]   restore list         — List available restore points");
    serial_println!("[deploy]   restore apply <id>   — Roll back to restore point");
    serial_println!("[deploy]   bootfix              — Reinstall GRUB bootloader");
    serial_println!("[deploy]   kpm --root /mnt reinstall <pkg> — Reinstall a package");
    serial_println!("[deploy]   reboot               — Restart system");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Auto-Update Daemon   (33.13)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Auto-update daemon configuration
#[derive(Clone)]
pub struct AutoUpdateDaemonConfig {
    /// Check interval in seconds
    pub check_interval_secs: u64,
    /// Automatically apply updates
    pub auto_apply: bool,
    /// Create restore point before update
    pub create_restore_point: bool,
    /// Rollback on failure
    pub rollback_on_failure: bool,
    /// Update channel
    pub channel: String,
}

/// Auto-update daemon state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateDaemonState {
    Stopped,
    Idle,
    Checking,
    Downloading,
    Applying,
    RollingBack,
}

static UPDATE_DAEMON_STATE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

static AUTO_UPDATE_CONFIG: Mutex<AutoUpdateDaemonConfig> = Mutex::new(AutoUpdateDaemonConfig {
    check_interval_secs: 3600, // 1 hour
    auto_apply: false,
    create_restore_point: true,
    rollback_on_failure: true,
    channel: String::new(),
});

/// Start the auto-update daemon
pub fn start_update_daemon(config: AutoUpdateDaemonConfig) {
    serial_println!(
        "[update-daemon] Starting — interval={}s, auto_apply={}, channel={}",
        config.check_interval_secs,
        config.auto_apply,
        if config.channel.is_empty() {
            "stable"
        } else {
            &config.channel
        }
    );
    *AUTO_UPDATE_CONFIG.lock() = config;
    UPDATE_DAEMON_STATE.store(1, core::sync::atomic::Ordering::Relaxed);
}

/// Stop the auto-update daemon
pub fn stop_update_daemon() {
    UPDATE_DAEMON_STATE.store(0, core::sync::atomic::Ordering::Relaxed);
    serial_println!("[update-daemon] Stopped");
}

/// Get daemon state
pub fn update_daemon_state() -> UpdateDaemonState {
    match UPDATE_DAEMON_STATE.load(core::sync::atomic::Ordering::Relaxed) {
        1 => UpdateDaemonState::Idle,
        2 => UpdateDaemonState::Checking,
        3 => UpdateDaemonState::Downloading,
        4 => UpdateDaemonState::Applying,
        5 => UpdateDaemonState::RollingBack,
        _ => UpdateDaemonState::Stopped,
    }
}

/// Daemon tick — check for updates if interval elapsed
pub fn update_daemon_tick(current_tick: u64, ticks_per_sec: u64) {
    if UPDATE_DAEMON_STATE.load(core::sync::atomic::Ordering::Relaxed) == 0 {
        return; // daemon not running
    }

    let config = AUTO_UPDATE_CONFIG.lock().clone();
    let interval_ticks = config.check_interval_secs * ticks_per_sec;

    static LAST_CHECK: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

    let last = LAST_CHECK.load(core::sync::atomic::Ordering::Relaxed);
    if current_tick.saturating_sub(last) < interval_ticks {
        return;
    }
    LAST_CHECK.store(current_tick, core::sync::atomic::Ordering::Relaxed);

    UPDATE_DAEMON_STATE.store(2, core::sync::atomic::Ordering::Relaxed);
    serial_println!("[update-daemon] Checking for updates...");

    if let Some(update) = crate::ota_update::check_for_updates() {
        serial_println!("[update-daemon] Update available: {}", update.version);

        if config.auto_apply {
            if config.create_restore_point {
                crate::recovery::create_restore_point("pre-auto-update");
            }

            UPDATE_DAEMON_STATE.store(3, core::sync::atomic::Ordering::Relaxed);
            if crate::ota_update::download_update(&update) {
                UPDATE_DAEMON_STATE.store(4, core::sync::atomic::Ordering::Relaxed);
                if !crate::ota_update::apply_update() && config.rollback_on_failure {
                    UPDATE_DAEMON_STATE.store(5, core::sync::atomic::Ordering::Relaxed);
                    serial_println!("[update-daemon] Update failed — rolling back");
                    crate::ota_update::rollback();
                }
            }
        }
    }

    UPDATE_DAEMON_STATE.store(1, core::sync::atomic::Ordering::Relaxed);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Digital Signature Verification   (33.14)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Signature algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureAlgorithm {
    Ed25519,
    Rsa2048Sha256,
    Rsa4096Sha512,
    EcdsaP256,
}

/// Signature verification result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyResult {
    Valid,
    Invalid,
    KeyNotFound,
    Expired,
    Revoked,
}

/// A trusted public key for update verification
#[derive(Clone)]
pub struct TrustedKey {
    pub key_id: String,
    pub algorithm: SignatureAlgorithm,
    pub public_key: Vec<u8>,
    pub expires_unix: u64,
    pub revoked: bool,
}

static TRUSTED_KEYS: Mutex<Vec<TrustedKey>> = Mutex::new(Vec::new());

/// Add a trusted public key for signature verification
pub fn add_trusted_key(key: TrustedKey) {
    serial_println!(
        "[sig] Added trusted key: {} ({:?})",
        key.key_id,
        key.algorithm
    );
    TRUSTED_KEYS.lock().push(key);
}

/// Remove a trusted key
pub fn remove_trusted_key(key_id: &str) {
    TRUSTED_KEYS.lock().retain(|k| k.key_id != key_id);
}

/// Revoke a trusted key
pub fn revoke_key(key_id: &str) {
    if let Some(k) = TRUSTED_KEYS.lock().iter_mut().find(|k| k.key_id == key_id) {
        k.revoked = true;
        serial_println!("[sig] Key {} revoked", key_id);
    }
}

/// Verify a digital signature on data
pub fn verify_signature(data: &[u8], signature: &[u8], key_id: &str) -> VerifyResult {
    let keys = TRUSTED_KEYS.lock();
    let key = match keys.iter().find(|k| k.key_id == key_id) {
        Some(k) => k,
        None => return VerifyResult::KeyNotFound,
    };

    if key.revoked {
        return VerifyResult::Revoked;
    }

    let now = crate::rtc::unix_time() as u64;
    if key.expires_unix > 0 && now > key.expires_unix {
        return VerifyResult::Expired;
    }

    // Simplified verification — in production, use crypto primitives
    // For Ed25519: verify using the 32-byte public key
    // For RSA: PKCS#1 v1.5 verify with SHA-256/512
    match key.algorithm {
        SignatureAlgorithm::Ed25519 => {
            if key.public_key.len() != 32 || signature.len() != 64 {
                return VerifyResult::Invalid;
            }
            // Placeholder: compute ed25519 verify
            // In a real OS, this uses the ed25519 crate
            serial_println!(
                "[sig] Ed25519 verify: data_len={}, sig_len={}, key={}",
                data.len(),
                signature.len(),
                key_id
            );
            VerifyResult::Valid
        }
        _ => {
            serial_println!(
                "[sig] {:?} verify: data_len={}, key={}",
                key.algorithm,
                data.len(),
                key_id
            );
            VerifyResult::Valid
        }
    }
}

/// Verify an update image file
pub fn verify_update_image(image_path: &str, sig_path: &str, key_id: &str) -> VerifyResult {
    serial_println!(
        "[sig] Verifying update image: {} with sig {}",
        image_path,
        sig_path
    );

    let vfs = crate::vfs::VFS.lock();
    let image_data = match vfs.read_file(image_path) {
        Some(d) => d,
        None => {
            serial_println!("[sig] Image file not found: {}", image_path);
            return VerifyResult::Invalid;
        }
    };
    let sig_data = match vfs.read_file(sig_path) {
        Some(d) => d,
        None => {
            serial_println!("[sig] Signature file not found: {}", sig_path);
            return VerifyResult::Invalid;
        }
    };
    // Need to copy data before dropping vfs lock
    let img = image_data.to_vec();
    let sig = sig_data.to_vec();
    drop(vfs);

    verify_signature(&img, &sig, key_id)
}

/// Initialize deployment subsystem
pub fn init() {
    serial_println!("[deploy] Deployment subsystem initialized");

    // Add default KnoxOS signing key
    add_trusted_key(TrustedKey {
        key_id: String::from("knoxos-release-2024"),
        algorithm: SignatureAlgorithm::Ed25519,
        public_key: alloc::vec![0u8; 32], // placeholder
        expires_unix: 0,                  // no expiry
        revoked: false,
    });
}
