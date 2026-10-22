use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use super::types::{
    ChecksumType, Compression, Dataset, DatasetType, Snapshot, ZFS_POOLS, ZfsPropValue,
};
use super::util::{format_size, generate_guid, parse_size};

// ────── Dataset Operations ───────────────────────────────────────────

/// Create a new dataset (filesystem or volume)
pub fn zfs_create(full_name: &str, ds_type: DatasetType) -> Result<(), &'static str> {
    let pool_name = full_name.split('/').next().ok_or("invalid dataset name")?;
    let mut pools = ZFS_POOLS.lock();
    let pool = pools.get_mut(pool_name).ok_or("pool not found")?;

    if pool.datasets.contains_key(full_name) {
        return Err("dataset already exists");
    }

    let parent_mountpoint = if full_name.contains('/') {
        let parent = &full_name[..full_name.rfind('/').unwrap()];
        pool.datasets
            .get(parent)
            .map(|ds| ds.mountpoint.clone())
            .unwrap_or_else(|| alloc::format!("/{}", pool_name))
    } else {
        alloc::format!("/{}", pool_name)
    };

    let ds_name_part = full_name.rsplit('/').next().unwrap_or(full_name);
    let mountpoint = alloc::format!("{}/{}", parent_mountpoint, ds_name_part);

    let ds = Dataset {
        name: String::from(full_name),
        dataset_type: ds_type,
        pool_name: String::from(pool_name),
        guid: generate_guid(),
        creation: crate::clock::monotonic_ns() as u64 / 1_000_000_000,
        used: 0,
        available: pool.free,
        referenced: 0,
        compress_ratio: 1.0,
        mountpoint,
        compression: Compression::Lz4,
        checksum: ChecksumType::Fletcher4,
        dedup: false,
        encryption: false,
        atime: true,
        exec: true,
        readonly: false,
        quota: 0,
        reservation: 0,
        record_size: 128 * 1024,
        snapshots: Vec::new(),
        clones: Vec::new(),
        properties: BTreeMap::new(),
    };

    pool.datasets.insert(String::from(full_name), ds);
    crate::serial_println!("[zfs] Dataset '{}' created", full_name);
    Ok(())
}

/// Destroy a dataset
pub fn zfs_destroy(full_name: &str) -> Result<(), &'static str> {
    let pool_name = full_name.split('/').next().ok_or("invalid dataset name")?;
    let mut pools = ZFS_POOLS.lock();
    let pool = pools.get_mut(pool_name).ok_or("pool not found")?;

    if pool.datasets.remove(full_name).is_some() {
        crate::serial_println!("[zfs] Dataset '{}' destroyed", full_name);
        Ok(())
    } else {
        Err("dataset not found")
    }
}

/// Create a snapshot
pub fn zfs_snapshot(snap_name: &str) -> Result<(), &'static str> {
    // snap_name format: pool/dataset@snapname
    let parts: Vec<&str> = snap_name.split('@').collect();
    if parts.len() != 2 {
        return Err("invalid snapshot name (use pool/dataset@snapname)");
    }
    let dataset_name = parts[0];
    let snap_tag = parts[1];
    let pool_name = dataset_name.split('/').next().ok_or("invalid dataset")?;
    let _ = snap_tag;

    let mut pools = ZFS_POOLS.lock();
    let pool = pools.get_mut(pool_name).ok_or("pool not found")?;

    let ds = pool
        .datasets
        .get_mut(dataset_name)
        .ok_or("dataset not found")?;

    let snapshot = Snapshot {
        name: String::from(snap_name),
        dataset: String::from(dataset_name),
        creation: crate::clock::monotonic_ns() as u64 / 1_000_000_000,
        used: 0,
        referenced: ds.referenced,
        txg: pool.txg,
    };

    ds.snapshots.push(String::from(snap_name));
    pool.snapshots.insert(String::from(snap_name), snapshot);
    pool.txg += 1;

    crate::serial_println!(
        "[zfs] Snapshot '{}' created at txg {}",
        snap_name,
        pool.txg - 1
    );
    Ok(())
}

/// Rollback to a snapshot — restores dataset to snapshot's state
pub fn zfs_rollback(snap_name: &str) -> Result<(), &'static str> {
    let parts: Vec<&str> = snap_name.split('@').collect();
    if parts.len() != 2 {
        return Err("invalid snapshot name");
    }
    let dataset_name = parts[0];
    let pool_name = dataset_name.split('/').next().ok_or("invalid dataset")?;

    let mut pools = ZFS_POOLS.lock();
    let pool = pools.get_mut(pool_name).ok_or("pool not found")?;

    let snap = pool.snapshots.get(snap_name).ok_or("snapshot not found")?;
    let snap_referenced = snap.referenced;
    let snap_txg = snap.txg;
    let snap_name_owned = snap.name.clone();

    let ds = pool
        .datasets
        .get_mut(dataset_name)
        .ok_or("dataset not found")?;

    // Free space used after the snapshot
    let freed = ds.used.saturating_sub(snap_referenced);
    pool.allocated = pool.allocated.saturating_sub(freed);
    pool.free = pool.total_space.saturating_sub(pool.allocated);

    // Restore dataset metrics to snapshot point
    ds.used = snap_referenced;
    ds.referenced = snap_referenced;

    // Remove all snapshots taken after this one
    ds.snapshots.retain(|s| {
        if let Some(later_snap) = pool.snapshots.get(s.as_str()) {
            later_snap.txg <= snap_txg
        } else {
            false
        }
    });

    // Remove later snapshots from pool too
    let to_remove: Vec<String> = pool
        .snapshots
        .iter()
        .filter(|(_, s)| s.dataset == dataset_name && s.txg > snap_txg)
        .map(|(k, _)| k.clone())
        .collect();
    for key in to_remove {
        pool.snapshots.remove(&key);
    }

    pool.txg += 1;

    crate::serial_println!(
        "[zfs] Rolled back '{}' to snapshot '{}' (freed {})",
        dataset_name,
        snap_name_owned,
        format_size(freed)
    );
    Ok(())
}

/// Clone a snapshot to a new dataset
pub fn zfs_clone(snap_name: &str, clone_name: &str) -> Result<(), &'static str> {
    let parts: Vec<&str> = snap_name.split('@').collect();
    if parts.len() != 2 {
        return Err("invalid snapshot name");
    }
    let pool_name = parts[0].split('/').next().ok_or("invalid")?;

    let mut pools = ZFS_POOLS.lock();
    let pool = pools.get_mut(pool_name).ok_or("pool not found")?;

    let snap = pool.snapshots.get(snap_name).ok_or("snapshot not found")?;
    let referenced = snap.referenced;

    let ds = Dataset {
        name: String::from(clone_name),
        dataset_type: DatasetType::Filesystem,
        pool_name: String::from(pool_name),
        guid: generate_guid(),
        creation: crate::clock::monotonic_ns() as u64 / 1_000_000_000,
        used: 0,
        available: pool.free,
        referenced,
        compress_ratio: 1.0,
        mountpoint: alloc::format!("/{}", clone_name),
        compression: Compression::Lz4,
        checksum: ChecksumType::Fletcher4,
        dedup: false,
        encryption: false,
        atime: true,
        exec: true,
        readonly: false,
        quota: 0,
        reservation: 0,
        record_size: 128 * 1024,
        snapshots: Vec::new(),
        clones: Vec::new(),
        properties: BTreeMap::new(),
    };

    pool.datasets.insert(String::from(clone_name), ds);
    crate::serial_println!("[zfs] Cloned '{}' → '{}'", snap_name, clone_name);
    Ok(())
}

/// Set a dataset property
pub fn zfs_set(dataset: &str, property: &str, value: &str) -> Result<(), &'static str> {
    let pool_name = dataset.split('/').next().ok_or("invalid dataset")?;
    let mut pools = ZFS_POOLS.lock();
    let pool = pools.get_mut(pool_name).ok_or("pool not found")?;
    let ds = pool.datasets.get_mut(dataset).ok_or("dataset not found")?;

    match property {
        "compression" => {
            ds.compression = match value {
                "off" => Compression::Off,
                "lz4" => Compression::Lz4,
                "zstd" => Compression::Zstd,
                "gzip" => Compression::Gzip,
                "lzo" => Compression::Lzo,
                _ => return Err("invalid compression"),
            };
        }
        "dedup" => ds.dedup = value == "on",
        "atime" => ds.atime = value == "on",
        "exec" => ds.exec = value == "on",
        "readonly" => ds.readonly = value == "on",
        "quota" => {
            ds.quota = parse_size(value).ok_or("invalid size")?;
        }
        "reservation" => {
            ds.reservation = parse_size(value).ok_or("invalid size")?;
        }
        "recordsize" => {
            ds.record_size = parse_size(value).ok_or("invalid size")? as u32;
        }
        "mountpoint" => {
            ds.mountpoint = String::from(value);
        }
        _ => {
            ds.properties.insert(
                String::from(property),
                ZfsPropValue::Str(String::from(value)),
            );
        }
    }

    crate::serial_println!("[zfs] Set {}={} on {}", property, value, dataset);
    Ok(())
}

/// List datasets in a pool
pub fn zfs_list(pool_name: &str) -> Vec<String> {
    let pools = ZFS_POOLS.lock();
    let mut result = Vec::new();
    result.push(String::from(
        "NAME                     USED  AVAIL  REFER  MOUNTPOINT",
    ));

    if let Some(pool) = pools.get(pool_name) {
        for ds in pool.datasets.values() {
            result.push(alloc::format!(
                "{:<25}{:<6}{:<7}{:<7}{}",
                ds.name,
                format_size(ds.used),
                format_size(ds.available),
                format_size(ds.referenced),
                ds.mountpoint
            ));
        }
    }
    result
}

/// Send a dataset (for zfs send | zfs receive replication)
pub fn zfs_send(snap_name: &str) -> Result<Vec<u8>, &'static str> {
    let parts: Vec<&str> = snap_name.split('@').collect();
    if parts.len() != 2 {
        return Err("invalid snapshot name");
    }
    let pool_name = parts[0].split('/').next().ok_or("invalid dataset")?;
    let pools = ZFS_POOLS.lock();
    let pool = pools.get(pool_name).ok_or("pool not found")?;
    if !pool.snapshots.contains_key(snap_name) {
        return Err("snapshot not found");
    }

    // In a real implementation, this would serialize the delta between snapshots
    let mut stream = Vec::new();
    // ZFS send stream header
    stream.extend_from_slice(b"ZFS_SEND_STREAM\x00");
    stream.extend_from_slice(&1u32.to_le_bytes()); // version
    crate::serial_println!("[zfs] Send stream generated for '{}'", snap_name);
    Ok(stream)
}

/// Receive a dataset stream
pub fn zfs_receive(pool_name: &str, _dataset: &str, _stream: &[u8]) -> Result<(), &'static str> {
    let pools = ZFS_POOLS.lock();
    if !pools.contains_key(pool_name) {
        return Err("pool not found");
    }
    crate::serial_println!(
        "[zfs] Receive stream applied to '{}/{}'",
        pool_name,
        _dataset
    );
    Ok(())
}
