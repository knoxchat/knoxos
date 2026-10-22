use alloc::string::String;
use alloc::vec::Vec;

use super::arc::{arc_insert, arc_lookup};
use super::checksum::{fletcher4_compute, verify_checksum};
use super::compress::{lz4_compress, lz4_decompress};
use super::types::{
    BlockPointer, BlockType, ChecksumType, Compression, DiskVirtualAddress, VdevType, ZFS_POOLS,
};
use super::vdev::{read_vdev_block, write_vdev_block};

// ────── Block Pointer I/O (checksummed reads/writes) ─────────────────

/// Read a block via its BlockPointer, verifying checksum
/// Tries each DVA in order, falls back on checksum failure
pub fn bp_read(pool_name: &str, bp: &BlockPointer) -> Result<Vec<u8>, &'static str> {
    // Check ARC first
    if let Some(cached) = arc_lookup(pool_name, bp) {
        return Ok(cached);
    }

    let pools = ZFS_POOLS.lock();
    let pool = pools.get(pool_name).ok_or("pool not found")?;

    // Try each DVA copy
    for dva in &bp.dva {
        if dva.asize == 0 {
            continue;
        }

        // Find the vdev
        let vdev = pool
            .vdevs
            .iter()
            .find(|v| v.id == dva.vdev_id)
            .ok_or("vdev not found")?;

        let device = &vdev.path;
        let raw = match read_vdev_block(device, dva.offset, bp.psize as usize * 512) {
            Ok(d) => d,
            Err(_) => continue, // Try next DVA
        };

        // Verify checksum
        if verify_checksum(&raw, &bp.checksum, bp.checksum_type) {
            // Decompress if needed
            let data = match bp.compress {
                Compression::Off => raw,
                Compression::Lz4 => lz4_decompress(&raw),
                _ => raw, // Other algorithms pass through for now
            };

            // Insert into ARC for future reads
            drop(pools);
            arc_insert(pool_name, bp, data.clone());
            return Ok(data);
        }
        // Checksum failed — try next DVA copy
    }

    Err("all DVA copies failed checksum verification")
}

/// Write a block, compute checksum, and return the new BlockPointer
pub fn bp_write(
    pool_name: &str,
    data: &[u8],
    block_type: BlockType,
    level: u8,
) -> Result<BlockPointer, &'static str> {
    let mut pools = ZFS_POOLS.lock();
    let pool = pools.get_mut(pool_name).ok_or("pool not found")?;

    // Get compression and checksum settings from root dataset
    let (compress, cksum_type) = {
        let root = pool.datasets.values().next();
        match root {
            Some(ds) => (ds.compression, ds.checksum),
            None => (Compression::Lz4, ChecksumType::Fletcher4),
        }
    };

    // Compress
    let (physical, compression) = match compress {
        Compression::Lz4 => {
            let compressed = lz4_compress(data);
            if compressed.len() < data.len() {
                (compressed, Compression::Lz4)
            } else {
                (data.to_vec(), Compression::Off)
            }
        }
        _ => (data.to_vec(), Compression::Off),
    };

    // Compute checksum on the physical (compressed) data
    let checksum = fletcher4_compute(&physical);

    // Allocate space on vdevs — simple bump allocator within the pool
    let psize_sectors = physical.len().div_ceil(512) as u32;
    let lsize_sectors = data.len().div_ceil(512) as u32;

    // Find a vdev with space and write to it
    let mut dva = [DiskVirtualAddress::default(); 3];
    let mut wrote = false;
    let mut write_device = String::new();
    let mut write_offset = 0u64;
    for vdev in pool.vdevs.iter_mut() {
        if vdev.vdev_type != VdevType::Disk {
            continue;
        }
        if vdev.allocated + (psize_sectors as u64 * 512) > vdev.total_space {
            continue;
        }
        let offset = vdev.allocated;
        dva[0] = DiskVirtualAddress {
            vdev_id: vdev.id,
            offset,
            asize: psize_sectors,
        };
        write_device = vdev.path.clone();
        write_offset = offset;
        wrote = true;
        break;
    }

    if !wrote {
        return Err("no space on any vdev");
    }

    // Drop the lock, write to disk, then re-acquire
    drop(pools);
    write_vdev_block(&write_device, write_offset, &physical)?;

    let mut pools = ZFS_POOLS.lock();
    let pool = pools.get_mut(pool_name).ok_or("pool not found")?;

    // Update vdev allocation
    if let Some(v) = pool.vdevs.iter_mut().find(|v| v.id == dva[0].vdev_id) {
        v.allocated += psize_sectors as u64 * 512;
    }
    let txg = pool.txg;

    let bp = BlockPointer {
        dva,
        lsize: lsize_sectors,
        psize: psize_sectors,
        compress: compression,
        checksum_type: cksum_type,
        block_type,
        level,
        birth_txg: txg,
        fill_count: 1,
        checksum,
    };

    // Update pool space accounting
    let alloc_bytes = psize_sectors as u64 * 512;
    pool.allocated += alloc_bytes;
    pool.free = pool.total_space.saturating_sub(pool.allocated);
    pool.capacity = ((pool.allocated as f64 / pool.total_space as f64) * 100.0) as u8;

    // Cache in ARC
    drop(pools);
    arc_insert(pool_name, &bp, data.to_vec());

    Ok(bp)
}
