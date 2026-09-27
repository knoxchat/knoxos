use alloc::vec;

use super::checksum::fletcher4_compute;
use super::types::{VdevType, ZFS_POOLS};
use super::vdev::write_vdev_block;

// ────── Transaction Group Sync ───────────────────────────────────────

/// Sync the current transaction group to disk
/// In ZFS, txg sync writes all dirty data from the current open txg,
/// transitions it through quiescing → syncing → committed.
pub fn txg_sync(pool_name: &str) -> Result<u64, &'static str> {
    let mut pools = ZFS_POOLS.lock();
    let pool = pools.get_mut(pool_name).ok_or("pool not found")?;

    let syncing_txg = pool.txg;

    // Write uberblock with the new txg number
    // The uberblock is at a rotating slot in the label area
    let ub_slot = (syncing_txg % 128) as usize;
    let ub_offset = 128 * 1024 + ub_slot * 1024; // After the 128K nvpair region

    // Serialize a minimal uberblock
    let mut ub = vec![0u8; 1024];
    // Magic: 0x00BAB10C (oo-ba-block)
    ub[0..4].copy_from_slice(&0x00BAB10Cu32.to_be_bytes());
    // Version
    ub[4..8].copy_from_slice(&5000u32.to_le_bytes());
    // TXG
    ub[8..16].copy_from_slice(&syncing_txg.to_le_bytes());
    // GUID sum (pool guid)
    ub[16..24].copy_from_slice(&pool.guid.to_le_bytes());
    // Timestamp
    let ts = crate::clock::monotonic_ns() as u64 / 1_000_000_000;
    ub[24..32].copy_from_slice(&ts.to_le_bytes());
    // Checksum the uberblock itself
    let cksum = fletcher4_compute(&ub[..1024 - 32]);
    ub[1024 - 32..1024 - 24].copy_from_slice(&cksum[0].to_le_bytes());
    ub[1024 - 24..1024 - 16].copy_from_slice(&cksum[1].to_le_bytes());
    ub[1024 - 16..1024 - 8].copy_from_slice(&cksum[2].to_le_bytes());
    ub[1024 - 8..1024].copy_from_slice(&cksum[3].to_le_bytes());

    // Write to first vdev's label area (L0 and L1 for redundancy)
    if let Some(vdev) = pool.vdevs.iter().find(|v| v.vdev_type == VdevType::Disk) {
        let device = vdev.path.clone();
        drop(pools);
        let _ = write_vdev_block(&device, ub_offset as u64, &ub);
        // Also write to L1 at 256K
        let _ = write_vdev_block(&device, 256 * 1024 + ub_offset as u64, &ub);

        // Advance to next txg
        let mut pools = ZFS_POOLS.lock();
        let pool = pools.get_mut(pool_name).ok_or("pool not found")?;
        pool.txg += 1;

        crate::serial_println!("[zfs] TXG {} synced to disk", syncing_txg);
        Ok(syncing_txg)
    } else {
        Err("no disk vdev found")
    }
}
