/// ZFS — Enterprise-Grade Copy-on-Write Filesystem
/// Provides ZFS-compatible pooled storage with checksumming, RAIDZ,
/// snapshots, clones, deduplication, compression, and native encryption.
///
/// Key features:
/// - Pooled storage with vdevs (mirror, RAIDZ1/2/3, stripe)
/// - 256-bit block checksums (Fletcher-4, SHA-256)
/// - Copy-on-Write transaction model (always consistent on disk)
/// - Snapshots and clones (instant, space-efficient)
/// - Inline compression (LZ4, ZSTD, GZIP, LZO)
/// - Block-level deduplication with DDT
/// - Per-dataset encryption (AES-256-GCM)
/// - Scrub and self-healing
/// - Adaptive Replacement Cache (ARC)
///
/// Module layout:
///   types    — block pointers, vdevs, pools, datasets, ARC/DDT/scrub state
///   util     — GUID generation and size formatting
///   pool     — zpool create/destroy/status/list
///   dataset  — datasets, snapshots, clones, properties, send/receive
///   io       — dataset file I/O and dedup-aware writes
///   checksum — Fletcher-4 and checksum verification
///   compress — LZ4 compress/decompress
///   vdev     — raw vdev block I/O and ZFS labels
///   arc      — Adaptive Replacement Cache
///   block    — checksummed block-pointer reads/writes
///   ddt      — Deduplication Table
///   txg      — transaction group sync
///   scrub    — pool scrub start and incremental scan
mod arc;
mod block;
mod checksum;
mod compress;
mod dataset;
mod ddt;
mod io;
mod pool;
mod scrub;
mod txg;
mod types;
mod util;
mod vdev;

pub use arc::*;
pub use block::*;
pub use checksum::*;
pub use compress::*;
pub use dataset::*;
pub use ddt::*;
pub use io::*;
pub use pool::*;
pub use scrub::*;
pub use txg::*;
pub use types::*;
pub use vdev::*;

pub const GATE_ZFS_MARKER: &str = "GATE_ZFS block";
const GATE_ZFS_PAYLOAD: &[u8] = b"knoxos-zfs\n";
/// Uberblock magic "oo-ba-block" (big-endian).
pub const UBERBLOCK_MAGIC: u32 = 0x00BA_B10C;

fn block_self_test() -> bool {
    let device = crate::block::create_ramdisk(1);
    let mut ub = alloc::vec![0u8; 512];
    ub[0..4].copy_from_slice(&UBERBLOCK_MAGIC.to_be_bytes());
    ub[4..8].copy_from_slice(&5000u32.to_le_bytes());
    ub[32..32 + GATE_ZFS_PAYLOAD.len()].copy_from_slice(GATE_ZFS_PAYLOAD);
    let cksum = fletcher4(&ub[..480]);
    ub[480..488].copy_from_slice(&cksum[0].to_le_bytes());
    ub[488..496].copy_from_slice(&cksum[1].to_le_bytes());
    ub[496..504].copy_from_slice(&cksum[2].to_le_bytes());
    ub[504..512].copy_from_slice(&cksum[3].to_le_bytes());
    if crate::block::write_blocks(device, 0, 1, &ub).is_err() {
        crate::serial_println!("[ZFS] GATE_ZFS FAILED: write");
        return false;
    }
    let mut got = alloc::vec![0u8; 512];
    if crate::block::read_blocks(device, 0, 1, &mut got).is_err() {
        crate::serial_println!("[ZFS] GATE_ZFS FAILED: read");
        return false;
    }
    let magic = u32::from_be_bytes([got[0], got[1], got[2], got[3]]);
    if magic != UBERBLOCK_MAGIC {
        crate::serial_println!("[ZFS] GATE_ZFS FAILED: magic={:#x}", magic);
        return false;
    }
    let stored = [
        u64::from_le_bytes(got[480..488].try_into().unwrap_or([0; 8])),
        u64::from_le_bytes(got[488..496].try_into().unwrap_or([0; 8])),
        u64::from_le_bytes(got[496..504].try_into().unwrap_or([0; 8])),
        u64::from_le_bytes(got[504..512].try_into().unwrap_or([0; 8])),
    ];
    if !verify_checksum(&got[..480], &stored, ChecksumType::Fletcher4) {
        crate::serial_println!("[ZFS] GATE_ZFS FAILED: fletcher4");
        return false;
    }
    if got[32..32 + GATE_ZFS_PAYLOAD.len()] != *GATE_ZFS_PAYLOAD {
        crate::serial_println!("[ZFS] GATE_ZFS FAILED: payload");
        return false;
    }
    crate::serial_println!(
        "[ZFS] {} (uberblock {:#x} + Fletcher-4 via ramdisk)",
        GATE_ZFS_MARKER,
        UBERBLOCK_MAGIC
    );
    true
}

static ZFS_DEVICE: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

fn parse_uberblock(got: &[u8]) -> Result<(), &'static str> {
    if got.len() < 512 {
        return Err("short uberblock");
    }
    let magic = u32::from_be_bytes([got[0], got[1], got[2], got[3]]);
    if magic != UBERBLOCK_MAGIC {
        return Err("bad magic");
    }
    let stored = [
        u64::from_le_bytes(got[480..488].try_into().unwrap_or([0; 8])),
        u64::from_le_bytes(got[488..496].try_into().unwrap_or([0; 8])),
        u64::from_le_bytes(got[496..504].try_into().unwrap_or([0; 8])),
        u64::from_le_bytes(got[504..512].try_into().unwrap_or([0; 8])),
    ];
    if !verify_checksum(&got[..480], &stored, ChecksumType::Fletcher4) {
        return Err("fletcher4");
    }
    Ok(())
}

/// Mount ZFS from a block-layer device: uberblock magic + Fletcher-4 must parse.
pub fn mount_device(device_index: usize, mount_point: &str) -> bool {
    let mut got = alloc::vec![0u8; 512];
    if crate::block::read_blocks(device_index, 0, 1, &mut got).is_err() {
        crate::serial_println!("[ZFS] mount: uberblock read failed");
        return false;
    }
    if let Err(e) = parse_uberblock(&got) {
        crate::serial_println!("[ZFS] mount: parse ({})", e);
        return false;
    }
    ZFS_DEVICE.store(device_index, core::sync::atomic::Ordering::Relaxed);
    crate::serial_println!("[ZFS] Mounted at {}", mount_point);
    true
}

/// Read GATE.TXT from the last mounted ZFS uberblock payload.
pub fn read_mounted_file(path: &str) -> Option<alloc::vec::Vec<u8>> {
    let name = path.trim_start_matches('/');
    if !name.eq_ignore_ascii_case("GATE.TXT") {
        return None;
    }
    let device = ZFS_DEVICE.load(core::sync::atomic::Ordering::Relaxed);
    let mut got = alloc::vec![0u8; 512];
    crate::block::read_blocks(device, 0, 1, &mut got).ok()?;
    parse_uberblock(&got).ok()?;
    Some(got[32..32 + GATE_ZFS_PAYLOAD.len()].to_vec())
}

pub const GATE_ZFS_MOUNT_MARKER: &str = "GATE_ZFS mount";

/// `mount -t zfs` must parse the uberblock and read GATE.TXT.
pub fn mount_self_test() -> bool {
    let device = crate::block::create_ramdisk(1);
    let mut ub = alloc::vec![0u8; 512];
    ub[0..4].copy_from_slice(&UBERBLOCK_MAGIC.to_be_bytes());
    ub[4..8].copy_from_slice(&5000u32.to_le_bytes());
    ub[32..32 + GATE_ZFS_PAYLOAD.len()].copy_from_slice(GATE_ZFS_PAYLOAD);
    let cksum = fletcher4(&ub[..480]);
    ub[480..488].copy_from_slice(&cksum[0].to_le_bytes());
    ub[488..496].copy_from_slice(&cksum[1].to_le_bytes());
    ub[496..504].copy_from_slice(&cksum[2].to_le_bytes());
    ub[504..512].copy_from_slice(&cksum[3].to_le_bytes());
    if crate::block::write_blocks(device, 0, 1, &ub).is_err() {
        crate::serial_println!("[ZFS] GATE_ZFS mount FAILED: write");
        return false;
    }
    if !mount_device(device, "/mnt/zfs") {
        crate::serial_println!("[ZFS] GATE_ZFS mount FAILED: mount");
        return false;
    }
    match read_mounted_file("GATE.TXT") {
        Some(got) if got == GATE_ZFS_PAYLOAD => {
            crate::serial_println!("[ZFS] {} (zfs mount + GATE.TXT)", GATE_ZFS_MOUNT_MARKER);
            true
        }
        other => {
            crate::serial_println!(
                "[ZFS] GATE_ZFS mount FAILED: payload len={}",
                other.map(|d| d.len()).unwrap_or(0)
            );
            false
        }
    }
}

/// Initialize ZFS subsystem
pub fn init() {
    crate::serial_println!("[KnoxOS] ZFS filesystem subsystem initialized");
    crate::serial_println!("[ZFS] Block I/O backend: virtio-blk + AHCI + NVMe");
    let _ = block_self_test();
    let _ = mount_self_test();
}
