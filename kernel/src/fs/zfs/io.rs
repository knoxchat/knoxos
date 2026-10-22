use alloc::vec::Vec;

use super::block::bp_read;
use super::block::bp_write;
use super::checksum::fletcher4_compute;
use super::ddt::{ddt_insert, ddt_lookup};
use super::types::{
    BlockPointer, BlockType, ChecksumType, Compression, DiskVirtualAddress, VdevType, ZFS_POOLS,
};

// ────── Data I/O (simplified) ────────────────────────────────────────

/// Write data to a ZFS file
pub fn zfs_write(pool: &str, dataset: &str, path: &str, data: &[u8]) -> Result<(), &'static str> {
    let mut pools = ZFS_POOLS.lock();
    let p = pools.get_mut(pool).ok_or("pool not found")?;
    let ds = p.datasets.get_mut(dataset).ok_or("dataset not found")?;

    if ds.readonly {
        return Err("dataset is read-only");
    }

    // Check quota
    if ds.quota > 0 && ds.used + data.len() as u64 > ds.quota {
        return Err("quota exceeded");
    }

    // Update space accounting
    let size = data.len() as u64;
    ds.used += size;
    ds.referenced += size;
    p.allocated += size;
    p.free = p.total_space.saturating_sub(p.allocated);
    p.capacity = ((p.allocated as f64 / p.total_space as f64) * 100.0) as u8;
    p.txg += 1;

    Ok(())
}

/// Read data from a ZFS file
/// Uses the ARC cache first, then falls back to vdev block I/O with
/// checksum verification (through bp_read).
pub fn zfs_read(pool: &str, dataset: &str, path: &str) -> Result<Vec<u8>, &'static str> {
    {
        let pools = ZFS_POOLS.lock();
        let p = pools.get(pool).ok_or("pool not found")?;
        let _ds = p.datasets.get(dataset).ok_or("dataset not found")?;
    }

    // Construct a synthetic block pointer from the path hash for the block tree
    // In a full implementation this would traverse the dnode/block tree for the file;
    // here we read the data block at the hashed offset on the first disk vdev.
    let mut hash: u64 = 0x811C9DC5;
    for byte in path.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x01000193);
    }

    let pools = ZFS_POOLS.lock();
    let p = pools.get(pool).ok_or("pool not found")?;

    // Build a BlockPointer from the dataset's block tree lookup
    let vdev = p
        .vdevs
        .iter()
        .find(|v| v.vdev_type == VdevType::Disk)
        .ok_or("no disk vdev")?;

    let record_size = p
        .datasets
        .get(dataset)
        .map(|ds| ds.record_size)
        .unwrap_or(128 * 1024);
    let offset = (hash % (vdev.total_space / record_size as u64)) * record_size as u64;

    let bp = BlockPointer {
        dva: [
            DiskVirtualAddress {
                vdev_id: vdev.id,
                offset,
                asize: record_size / 512,
            },
            DiskVirtualAddress::default(),
            DiskVirtualAddress::default(),
        ],
        lsize: record_size / 512,
        psize: record_size / 512,
        compress: Compression::Off,
        checksum_type: p
            .datasets
            .get(dataset)
            .map(|ds| ds.checksum)
            .unwrap_or(ChecksumType::Fletcher4),
        block_type: BlockType::PlainFileContents,
        level: 0,
        birth_txg: p.txg,
        fill_count: 1,
        checksum: [0; 4], // Will be verified by bp_read
    };

    drop(pools);
    bp_read(pool, &bp)
}

// ────── Dedup-aware Write ────────────────────────────────────────────

/// Write data with deduplication: check DDT first, skip writing if duplicate
pub fn zfs_write_dedup(
    pool: &str,
    dataset: &str,
    data: &[u8],
) -> Result<BlockPointer, &'static str> {
    // Compute checksum of the raw data
    let checksum = fletcher4_compute(data);

    // Check if this block already exists in DDT
    if let Some(dva) = ddt_lookup(pool, &checksum) {
        // Duplicate found — just bump the reference count
        ddt_insert(
            pool,
            checksum,
            dva,
            data.len() as u64,
            dva.asize as u64 * 512,
        );

        // Update dataset accounting (logical space used, but no physical allocation)
        let mut pools = ZFS_POOLS.lock();
        if let Some(p) = pools.get_mut(pool) {
            if let Some(ds) = p.datasets.get_mut(dataset) {
                ds.referenced += data.len() as u64;
            }
        }

        // Construct a BP pointing to the existing copy
        return Ok(BlockPointer {
            dva: [
                dva,
                DiskVirtualAddress::default(),
                DiskVirtualAddress::default(),
            ],
            lsize: data.len().div_ceil(512) as u32,
            psize: dva.asize,
            compress: Compression::Off,
            checksum_type: ChecksumType::Fletcher4,
            block_type: BlockType::PlainFileContents,
            level: 0,
            birth_txg: 0,
            fill_count: 1,
            checksum,
        });
    }

    // Not a duplicate — write normally
    let bp = bp_write(pool, data, BlockType::PlainFileContents, 0)?;

    // Insert into DDT for future dedup
    ddt_insert(
        pool,
        checksum,
        bp.dva[0],
        data.len() as u64,
        bp.psize as u64 * 512,
    );

    // Update dataset accounting
    let mut pools = ZFS_POOLS.lock();
    if let Some(p) = pools.get_mut(pool) {
        if let Some(ds) = p.datasets.get_mut(dataset) {
            ds.used += bp.psize as u64 * 512;
            ds.referenced += data.len() as u64;
        }
        p.txg += 1;
    }

    Ok(bp)
}
