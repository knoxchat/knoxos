use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use super::types::{
    ChecksumType, Compression, Dataset, DatasetType, PoolHealth, PoolState, ScrubState, Vdev,
    VdevState, VdevType, ZFS_POOLS, ZfsPool,
};
use super::util::{format_size, generate_guid};

// ────── Pool Operations ──────────────────────────────────────────────

/// Create a new ZFS pool
pub fn zpool_create(name: &str, vdev_type: VdevType, disks: &[&str]) -> Result<(), &'static str> {
    let mut pools = ZFS_POOLS.lock();
    if pools.contains_key(name) {
        return Err("pool already exists");
    }

    // Create vdevs from disk paths
    let mut vdevs = Vec::new();
    let total_space: u64;

    match vdev_type {
        VdevType::Disk => {
            // Single disk (stripe)
            for (i, disk) in disks.iter().enumerate() {
                vdevs.push(Vdev {
                    id: i as u32,
                    vdev_type: VdevType::Disk,
                    state: VdevState::Online,
                    path: String::from(*disk),
                    total_space: 1024 * 1024 * 1024, // 1GB per disk
                    allocated: 0,
                    checksum_errors: 0,
                    read_errors: 0,
                    write_errors: 0,
                    children: Vec::new(),
                });
            }
            total_space = disks.len() as u64 * 1024 * 1024 * 1024;
        }
        VdevType::Mirror | VdevType::Raidz1 | VdevType::Raidz2 | VdevType::Raidz3 => {
            if disks.len() < 2 {
                return Err("need at least 2 disks for redundancy");
            }
            let mut children = Vec::new();
            for (i, disk) in disks.iter().enumerate() {
                let child_id = (i + 1) as u32;
                children.push(child_id);
                vdevs.push(Vdev {
                    id: child_id,
                    vdev_type: VdevType::Disk,
                    state: VdevState::Online,
                    path: String::from(*disk),
                    total_space: 1024 * 1024 * 1024,
                    allocated: 0,
                    checksum_errors: 0,
                    read_errors: 0,
                    write_errors: 0,
                    children: Vec::new(),
                });
            }
            let parity = match vdev_type {
                VdevType::Mirror => disks.len() as u64 - 1,
                VdevType::Raidz1 => 1,
                VdevType::Raidz2 => 2,
                VdevType::Raidz3 => 3,
                _ => 0,
            };
            let usable = disks.len() as u64 - parity;
            total_space = usable * 1024 * 1024 * 1024;

            vdevs.insert(
                0,
                Vdev {
                    id: 0,
                    vdev_type,
                    state: VdevState::Online,
                    path: String::from("group0"),
                    total_space,
                    allocated: 0,
                    checksum_errors: 0,
                    read_errors: 0,
                    write_errors: 0,
                    children,
                },
            );
        }
        _ => {
            return Err("unsupported vdev type for pool creation");
        }
    }

    // Create root dataset
    let root_ds_name = String::from(name);
    let mut datasets = BTreeMap::new();
    datasets.insert(
        root_ds_name.clone(),
        Dataset {
            name: root_ds_name.clone(),
            dataset_type: DatasetType::Filesystem,
            pool_name: String::from(name),
            guid: generate_guid(),
            creation: crate::clock::monotonic_ns() as u64 / 1_000_000_000,
            used: 0,
            available: total_space,
            referenced: 0,
            compress_ratio: 1.0,
            mountpoint: alloc::format!("/{}", name),
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
        },
    );

    let pool = ZfsPool {
        name: String::from(name),
        guid: generate_guid(),
        state: PoolState::Active,
        health: PoolHealth::Online,
        txg: 1,
        total_space,
        allocated: 0,
        free: total_space,
        fragmentation: 0,
        capacity: 0,
        dedup_ratio: 1.0,
        version: 5000,
        vdevs,
        datasets,
        snapshots: BTreeMap::new(),
        ddt: BTreeMap::new(),
        scrub: ScrubState {
            active: false,
            start_time: 0,
            end_time: 0,
            blocks_scanned: 0,
            blocks_repaired: 0,
            errors_found: 0,
            bytes_scanned: 0,
            percent_complete: 0,
        },
        arc: super::types::Arc {
            max_size: 256 * 1024 * 1024, // 256MB ARC
            current_size: 0,
            mru_size: 0,
            mfu_size: 0,
            hits: 0,
            misses: 0,
            l2_hits: 0,
            l2_misses: 0,
            entries: BTreeMap::new(),
        },
        properties: BTreeMap::new(),
    };

    pools.insert(String::from(name), pool);
    crate::serial_println!("[zfs] Pool '{}' created: {} bytes total", name, total_space);
    Ok(())
}

/// Destroy a ZFS pool
pub fn zpool_destroy(name: &str) -> Result<(), &'static str> {
    let mut pools = ZFS_POOLS.lock();
    if pools.remove(name).is_some() {
        crate::serial_println!("[zfs] Pool '{}' destroyed", name);
        Ok(())
    } else {
        Err("pool not found")
    }
}

/// Get pool status
pub fn zpool_status(name: &str) -> Option<String> {
    let pools = ZFS_POOLS.lock();
    let pool = pools.get(name)?;

    let mut out = String::new();
    out.push_str(&alloc::format!("  pool: {}\n", pool.name));
    out.push_str(&alloc::format!(" state: {:?}\n", pool.health));
    out.push_str(&alloc::format!(
        "  scan: scrub {:?}\n",
        if pool.scrub.active {
            "in progress"
        } else {
            "none requested"
        }
    ));
    out.push_str("config:\n\n");
    out.push_str("\tNAME            STATE     READ WRITE CKSUM\n");
    for vdev in &pool.vdevs {
        out.push_str(&alloc::format!(
            "\t{:<16}{:?}     {}    {}    {}\n",
            vdev.path,
            vdev.state,
            vdev.read_errors,
            vdev.write_errors,
            vdev.checksum_errors
        ));
    }
    out.push_str("\nerrors: No known data errors\n");
    Some(out)
}

/// List all pools
pub fn zpool_list() -> Vec<String> {
    let pools = ZFS_POOLS.lock();
    let mut result = Vec::new();
    result.push(String::from(
        "NAME    SIZE    ALLOC   FREE    CAP  DEDUP  HEALTH  ALTROOT",
    ));
    for pool in pools.values() {
        result.push(alloc::format!(
            "{:<8}{:<8}{:<8}{:<8}{:>3}%  {:.2}x  {:?}  -",
            pool.name,
            format_size(pool.total_space),
            format_size(pool.allocated),
            format_size(pool.free),
            pool.capacity,
            pool.dedup_ratio,
            pool.health
        ));
    }
    result
}
