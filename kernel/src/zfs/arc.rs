use alloc::vec::Vec;

use super::types::{Arc, ArcEntry, ArcType, BlockPointer, ZFS_POOLS};

// ────── ARC (Adaptive Replacement Cache) ─────────────────────────────

/// Hash a block pointer into a cache key
fn arc_key(bp: &BlockPointer) -> u64 {
    let dva = &bp.dva[0];
    (dva.vdev_id as u64)
        .wrapping_mul(0x9E3779B97F4A7C15)
        .wrapping_add(dva.offset)
        .wrapping_mul(0x517CC1B727220A95)
        .wrapping_add(bp.birth_txg)
}

/// Look up a block in the ARC, returning cached data if present
pub fn arc_lookup(pool_name: &str, bp: &BlockPointer) -> Option<Vec<u8>> {
    let mut pools = ZFS_POOLS.lock();
    let pool = pools.get_mut(pool_name)?;
    let key = arc_key(bp);

    if let Some(entry) = pool.arc.entries.get_mut(&key) {
        pool.arc.hits += 1;
        entry.access_count += 1;
        entry.last_access = crate::clock::monotonic_ns() as u64;

        // Promote MRU → MFU on second access (ARC adaptive behavior)
        if entry.arc_type == ArcType::Mru && entry.access_count >= 2 {
            let size = entry.data.len() as u64;
            entry.arc_type = ArcType::Mfu;
            pool.arc.mru_size = pool.arc.mru_size.saturating_sub(size);
            pool.arc.mfu_size += size;
        }

        Some(entry.data.clone())
    } else {
        pool.arc.misses += 1;
        None
    }
}

/// Insert a block into the ARC, evicting if necessary
pub fn arc_insert(pool_name: &str, bp: &BlockPointer, data: Vec<u8>) {
    let mut pools = ZFS_POOLS.lock();
    let pool = match pools.get_mut(pool_name) {
        Some(p) => p,
        None => return,
    };

    let key = arc_key(bp);
    let data_len = data.len() as u64;

    // Evict if over capacity
    while pool.arc.current_size + data_len > pool.arc.max_size && !pool.arc.entries.is_empty() {
        arc_evict_one(&mut pool.arc);
    }

    pool.arc.entries.insert(
        key,
        ArcEntry {
            data,
            access_count: 1,
            last_access: crate::clock::monotonic_ns() as u64,
            arc_type: ArcType::Mru,
        },
    );
    pool.arc.current_size += data_len;
    pool.arc.mru_size += data_len;
}

/// Evict one entry from ARC (prefer MRU ghost, then least-recently-used MRU)
fn arc_evict_one(arc: &mut Arc) {
    // Find the oldest MRU entry to evict
    let mut evict_key = None;
    let mut oldest_access = u64::MAX;

    for (&key, entry) in arc.entries.iter() {
        if entry.arc_type == ArcType::Mru && entry.last_access < oldest_access {
            oldest_access = entry.last_access;
            evict_key = Some(key);
        }
    }

    // If no MRU entry, evict oldest MFU
    if evict_key.is_none() {
        for (&key, entry) in arc.entries.iter() {
            if entry.last_access < oldest_access {
                oldest_access = entry.last_access;
                evict_key = Some(key);
            }
        }
    }

    if let Some(key) = evict_key {
        if let Some(entry) = arc.entries.remove(&key) {
            let size = entry.data.len() as u64;
            arc.current_size = arc.current_size.saturating_sub(size);
            match entry.arc_type {
                ArcType::Mru => arc.mru_size = arc.mru_size.saturating_sub(size),
                ArcType::Mfu => arc.mfu_size = arc.mfu_size.saturating_sub(size),
                _ => {}
            }
        }
    }
}
