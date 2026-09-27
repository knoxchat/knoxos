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

/// Initialize ZFS subsystem
pub fn init() {
    crate::serial_println!("[KnoxOS] ZFS filesystem subsystem initialized");
    crate::serial_println!("[ZFS] Block I/O backend: virtio-blk + AHCI + NVMe");
}
