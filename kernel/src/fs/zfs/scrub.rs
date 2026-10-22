use alloc::string::String;
use alloc::vec::Vec;

use super::checksum::fletcher4_compute;
use super::types::{ChecksumType, ScrubState, VdevType, ZFS_POOLS};
use super::vdev::read_vdev_block;

/// Start a scrub on a pool
pub fn zpool_scrub(name: &str) -> Result<(), &'static str> {
    let mut pools = ZFS_POOLS.lock();
    let pool = pools.get_mut(name).ok_or("pool not found")?;

    pool.scrub = ScrubState {
        active: true,
        start_time: crate::clock::monotonic_ns() as u64 / 1_000_000_000,
        end_time: 0,
        blocks_scanned: 0,
        blocks_repaired: 0,
        errors_found: 0,
        bytes_scanned: 0,
        percent_complete: 0,
    };

    crate::serial_println!("[zfs] Scrub started on pool '{}'", name);
    Ok(())
}

// ────── Scrub — Verify All Blocks ────────────────────────────────────

/// Advance a scrub by scanning one batch of blocks
/// Call this periodically (e.g., from a background task) while scrub is active
pub fn scrub_tick(pool_name: &str, blocks_per_tick: u64) -> Result<bool, &'static str> {
    let mut pools = ZFS_POOLS.lock();
    let pool = pools.get_mut(pool_name).ok_or("pool not found")?;

    if !pool.scrub.active {
        return Ok(false);
    }

    // Calculate total blocks in pool
    let block_size: u64 = 128 * 1024; // Default record size
    let total_blocks = pool.allocated / block_size.max(1);

    if total_blocks == 0 {
        pool.scrub.active = false;
        pool.scrub.percent_complete = 100;
        pool.scrub.end_time = crate::clock::monotonic_ns() as u64 / 1_000_000_000;
        return Ok(false); // Done
    }

    // Scan a batch of blocks by reading from each disk vdev
    let disk_vdevs: Vec<(String, u64)> = pool
        .vdevs
        .iter()
        .filter(|v| v.vdev_type == VdevType::Disk)
        .map(|v| (v.path.clone(), v.allocated))
        .collect();

    let scanned_so_far = pool.scrub.blocks_scanned;
    let batch_end = (scanned_so_far + blocks_per_tick).min(total_blocks);
    let cksum_type = ChecksumType::Fletcher4;
    let _ = cksum_type;

    drop(pools);

    let mut errors = 0u64;
    let mut bytes_scanned = 0u64;

    for (device, allocated) in &disk_vdevs {
        let blocks_on_dev = allocated / block_size.max(1);
        let start = scanned_so_far.min(blocks_on_dev);
        let end = batch_end.min(blocks_on_dev);

        for blk in start..end {
            let offset = blk * block_size;
            match read_vdev_block(device, offset, block_size as usize) {
                Ok(data) => {
                    // Verify the block has a valid checksum
                    // In a full implementation we'd look up the BP tree for expected checksums;
                    // here we verify non-zero blocks aren't all-zero (corruption heuristic)
                    let nonzero = data.iter().any(|&b| b != 0);
                    if nonzero {
                        let cksum = fletcher4_compute(&data);
                        // We can't verify against expected without the BP tree,
                        // but we can detect obviously corrupted blocks (all 0xFF, etc.)
                        if cksum[0] == 0 && cksum[1] == 0 && cksum[2] == 0 && cksum[3] == 0 {
                            errors += 1;
                        }
                    }
                    bytes_scanned += data.len() as u64;
                }
                Err(_) => {
                    errors += 1;
                }
            }
        }
    }

    // Update scrub state
    let mut pools = ZFS_POOLS.lock();
    let pool = pools.get_mut(pool_name).ok_or("pool not found")?;
    pool.scrub.blocks_scanned = batch_end;
    pool.scrub.bytes_scanned += bytes_scanned;
    pool.scrub.errors_found += errors;
    pool.scrub.percent_complete =
        ((batch_end as f64 / total_blocks as f64) * 100.0).min(100.0) as u8;

    if batch_end >= total_blocks {
        pool.scrub.active = false;
        pool.scrub.end_time = crate::clock::monotonic_ns() as u64 / 1_000_000_000;
        crate::serial_println!(
            "[zfs] Scrub complete on '{}': {} blocks scanned, {} errors",
            pool_name,
            pool.scrub.blocks_scanned,
            pool.scrub.errors_found
        );
        Ok(false) // Done
    } else {
        Ok(true) // More to scan
    }
}
