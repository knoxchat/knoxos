use super::types::{DdtEntry, DiskVirtualAddress, ZFS_POOLS};

// ────── DDT (Deduplication Table) ────────────────────────────────────

/// Look up data in the dedup table by its checksum
/// Returns the existing DVA if a duplicate block already exists
pub fn ddt_lookup(pool_name: &str, checksum: &[u64; 4]) -> Option<DiskVirtualAddress> {
    let pools = ZFS_POOLS.lock();
    let pool = pools.get(pool_name)?;

    // DDT is keyed by a hash of the full checksum
    let key = checksum[0]
        .wrapping_mul(0x9E3779B97F4A7C15)
        .wrapping_add(checksum[1])
        .wrapping_mul(0x517CC1B727220A95)
        .wrapping_add(checksum[2])
        .wrapping_mul(0x6C62272E07BB0142)
        .wrapping_add(checksum[3]);

    pool.ddt.get(&key).map(|entry| entry.dva)
}

/// Insert a new entry into the DDT, or increment its ref count
pub fn ddt_insert(
    pool_name: &str,
    checksum: [u64; 4],
    dva: DiskVirtualAddress,
    logical_size: u64,
    physical_size: u64,
) {
    let mut pools = ZFS_POOLS.lock();
    let pool = match pools.get_mut(pool_name) {
        Some(p) => p,
        None => return,
    };

    let key = checksum[0]
        .wrapping_mul(0x9E3779B97F4A7C15)
        .wrapping_add(checksum[1])
        .wrapping_mul(0x517CC1B727220A95)
        .wrapping_add(checksum[2])
        .wrapping_mul(0x6C62272E07BB0142)
        .wrapping_add(checksum[3]);

    if let Some(entry) = pool.ddt.get_mut(&key) {
        entry.ref_count += 1;
        // Update dedup ratio
        let total_logical = entry.logical_size * entry.ref_count;
        pool.dedup_ratio = total_logical as f32 / entry.physical_size as f32;
    } else {
        pool.ddt.insert(
            key,
            DdtEntry {
                checksum,
                dva,
                ref_count: 1,
                physical_size,
                logical_size,
            },
        );
    }
}

/// Decrement DDT reference count; free the block if count reaches zero
pub fn ddt_deref(pool_name: &str, checksum: &[u64; 4]) -> bool {
    let mut pools = ZFS_POOLS.lock();
    let pool = match pools.get_mut(pool_name) {
        Some(p) => p,
        None => return false,
    };

    let key = checksum[0]
        .wrapping_mul(0x9E3779B97F4A7C15)
        .wrapping_add(checksum[1])
        .wrapping_mul(0x517CC1B727220A95)
        .wrapping_add(checksum[2])
        .wrapping_mul(0x6C62272E07BB0142)
        .wrapping_add(checksum[3]);

    if let Some(entry) = pool.ddt.get_mut(&key) {
        entry.ref_count -= 1;
        if entry.ref_count == 0 {
            let freed = entry.physical_size;
            pool.ddt.remove(&key);
            // Return freed space to pool
            pool.allocated = pool.allocated.saturating_sub(freed);
            pool.free = pool.total_space.saturating_sub(pool.allocated);
            return true; // Block was freed
        }
    }
    false
}
