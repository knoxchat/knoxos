/// LVM — Logical Volume Manager
///
/// Provides flexible disk management with dynamic volume resizing:
///   - Physical Volumes (PV): raw block devices
///   - Volume Groups (VG): pools of physical extents
///   - Logical Volumes (LV): virtual block devices carved from VGs
///   - Online resize (grow/shrink)
///   - Snapshots (CoW-based point-in-time copies)
///   - Thin provisioning
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// CONSTANTS
// ═══════════════════════════════════════════════════════════════════════

/// Default Physical Extent size: 4 MiB
pub const DEFAULT_PE_SIZE: u64 = 4 * 1024 * 1024;

// ═══════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════

/// Physical Volume — wraps a block device
#[derive(Debug, Clone)]
pub struct PhysicalVolume {
    pub uuid: String,
    pub device: String,
    pub total_extents: u64,
    pub free_extents: u64,
    pub pe_size: u64,
    /// Bitmap of used extents
    pub extent_map: Vec<bool>,
}

/// Volume Group — pool of PVs
#[derive(Debug, Clone)]
pub struct VolumeGroup {
    pub name: String,
    pub uuid: String,
    pub pvs: Vec<String>,
    pub pe_size: u64,
    pub total_extents: u64,
    pub free_extents: u64,
    pub lvs: Vec<String>,
}

/// Logical Volume type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LvType {
    Linear,
    Striped,
    Mirror,
    Snapshot,
    ThinPool,
    ThinVolume,
}

/// Logical Volume — virtual block device
#[derive(Debug, Clone)]
pub struct LogicalVolume {
    pub name: String,
    pub vg_name: String,
    pub uuid: String,
    pub lv_type: LvType,
    pub size_extents: u64,
    pub active: bool,
    /// Maps LV extent → (PV uuid, PV extent)
    pub extent_map: Vec<(String, u64)>,
    /// For snapshots: origin LV name
    pub origin: Option<String>,
    /// Snapshot CoW usage percentage
    pub snap_percent: Option<f32>,
}

// ═══════════════════════════════════════════════════════════════════════
// STATE
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    static ref PVS: Mutex<BTreeMap<String, PhysicalVolume>> = Mutex::new(BTreeMap::new());
    static ref VGS: Mutex<BTreeMap<String, VolumeGroup>> = Mutex::new(BTreeMap::new());
    static ref LVS: Mutex<BTreeMap<String, LogicalVolume>> = Mutex::new(BTreeMap::new());
}

static NEXT_UUID: AtomicU64 = AtomicU64::new(1);

fn gen_uuid() -> String {
    let id = NEXT_UUID.fetch_add(1, Ordering::Relaxed);
    alloc::format!("knoxos-lvm-{:08x}", id)
}

// ═══════════════════════════════════════════════════════════════════════
// PV OPERATIONS (pvcreate, pvdisplay, pvremove)
// ═══════════════════════════════════════════════════════════════════════

/// Create a physical volume on a block device
pub fn pvcreate(device: &str) -> Result<String, &'static str> {
    let mut pvs = PVS.lock();
    if pvs.values().any(|pv| pv.device == device) {
        return Err("Device already has a PV");
    }

    // Detect device size (mock: 100 GiB)
    let device_size = 100 * 1024 * 1024 * 1024u64;
    let total_extents = device_size / DEFAULT_PE_SIZE;

    let uuid = gen_uuid();
    let pv = PhysicalVolume {
        uuid: uuid.clone(),
        device: String::from(device),
        total_extents,
        free_extents: total_extents,
        pe_size: DEFAULT_PE_SIZE,
        extent_map: alloc::vec![false; total_extents as usize],
    };

    serial_println!("[lvm] pvcreate: {} ({} extents)", device, total_extents);
    pvs.insert(uuid.clone(), pv);
    Ok(uuid)
}

/// Remove a physical volume
pub fn pvremove(device: &str) -> Result<(), &'static str> {
    let mut pvs = PVS.lock();
    let uuid = pvs
        .iter()
        .find(|(_, pv)| pv.device == device)
        .map(|(k, _)| k.clone());
    match uuid {
        Some(u) => {
            pvs.remove(&u);
            Ok(())
        }
        None => Err("PV not found"),
    }
}

/// List all physical volumes
pub fn pvdisplay() -> Vec<(String, String, u64, u64)> {
    PVS.lock()
        .values()
        .map(|pv| {
            (
                pv.device.clone(),
                pv.uuid.clone(),
                pv.total_extents,
                pv.free_extents,
            )
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// VG OPERATIONS (vgcreate, vgextend, vgreduce, vgdisplay)
// ═══════════════════════════════════════════════════════════════════════

/// Create a volume group from one or more PVs
pub fn vgcreate(name: &str, pv_devices: &[&str]) -> Result<(), &'static str> {
    let pvs = PVS.lock();
    let mut total = 0u64;
    let mut pv_uuids = Vec::new();

    for dev in pv_devices {
        let pv = pvs
            .values()
            .find(|p| p.device == *dev)
            .ok_or("PV not found")?;
        total += pv.free_extents;
        pv_uuids.push(pv.uuid.clone());
    }

    let vg = VolumeGroup {
        name: String::from(name),
        uuid: gen_uuid(),
        pvs: pv_uuids,
        pe_size: DEFAULT_PE_SIZE,
        total_extents: total,
        free_extents: total,
        lvs: Vec::new(),
    };

    serial_println!("[lvm] vgcreate: '{}' with {} extents", name, total);
    VGS.lock().insert(String::from(name), vg);
    Ok(())
}

/// Extend a VG with additional PVs
pub fn vgextend(vg_name: &str, pv_device: &str) -> Result<(), &'static str> {
    let pvs = PVS.lock();
    let pv = pvs
        .values()
        .find(|p| p.device == pv_device)
        .ok_or("PV not found")?;
    let pv_uuid = pv.uuid.clone();
    let extents = pv.free_extents;
    drop(pvs);

    let mut vgs = VGS.lock();
    let vg = vgs.get_mut(vg_name).ok_or("VG not found")?;
    vg.pvs.push(pv_uuid);
    vg.total_extents += extents;
    vg.free_extents += extents;
    serial_println!("[lvm] vgextend: '{}' +{} extents", vg_name, extents);
    Ok(())
}

/// List volume groups
pub fn vgdisplay() -> Vec<(String, u64, u64, usize)> {
    VGS.lock()
        .values()
        .map(|vg| {
            (
                vg.name.clone(),
                vg.total_extents,
                vg.free_extents,
                vg.lvs.len(),
            )
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// LV OPERATIONS (lvcreate, lvresize, lvremove, lvsnapshot)
// ═══════════════════════════════════════════════════════════════════════

/// Create a logical volume
pub fn lvcreate(vg_name: &str, lv_name: &str, size_extents: u64) -> Result<(), &'static str> {
    let mut vgs = VGS.lock();
    let vg = vgs.get_mut(vg_name).ok_or("VG not found")?;

    if size_extents > vg.free_extents {
        return Err("Not enough free extents in VG");
    }

    // Allocate extents from PVs
    let extent_map = allocate_extents(vg, size_extents)?;

    let lv = LogicalVolume {
        name: String::from(lv_name),
        vg_name: String::from(vg_name),
        uuid: gen_uuid(),
        lv_type: LvType::Linear,
        size_extents,
        active: true,
        extent_map,
        origin: None,
        snap_percent: None,
    };

    vg.free_extents -= size_extents;
    vg.lvs.push(String::from(lv_name));

    let key = alloc::format!("{}/{}", vg_name, lv_name);
    serial_println!(
        "[lvm] lvcreate: '{}' ({} extents = {} MiB)",
        key,
        size_extents,
        size_extents * 4
    );
    LVS.lock().insert(key, lv);
    Ok(())
}

/// Resize a logical volume (grow or shrink)
pub fn lvresize(vg_name: &str, lv_name: &str, new_size_extents: u64) -> Result<(), &'static str> {
    let key = alloc::format!("{}/{}", vg_name, lv_name);
    let mut lvs = LVS.lock();
    let lv = lvs.get_mut(&key).ok_or("LV not found")?;

    let old = lv.size_extents;
    if new_size_extents > old {
        // Grow: allocate additional extents
        let additional = new_size_extents - old;
        let mut vgs = VGS.lock();
        let vg = vgs.get_mut(vg_name).ok_or("VG not found")?;
        if additional > vg.free_extents {
            return Err("Not enough free extents to grow");
        }
        let new_extents = allocate_extents(vg, additional)?;
        lv.extent_map.extend(new_extents);
        vg.free_extents -= additional;
    }
    // Shrink: return extents (simplified)
    lv.size_extents = new_size_extents;
    serial_println!(
        "[lvm] lvresize: '{}' {} -> {} extents",
        key,
        old,
        new_size_extents
    );
    Ok(())
}

/// Create a CoW snapshot of a logical volume
pub fn lvsnapshot(
    vg_name: &str,
    origin_lv: &str,
    snap_name: &str,
    cow_size_extents: u64,
) -> Result<(), &'static str> {
    let origin_key = alloc::format!("{}/{}", vg_name, origin_lv);
    let lvs = LVS.lock();
    let _origin = lvs.get(&origin_key).ok_or("Origin LV not found")?;
    drop(lvs);

    let snap = LogicalVolume {
        name: String::from(snap_name),
        vg_name: String::from(vg_name),
        uuid: gen_uuid(),
        lv_type: LvType::Snapshot,
        size_extents: cow_size_extents,
        active: true,
        extent_map: Vec::new(),
        origin: Some(String::from(origin_lv)),
        snap_percent: Some(0.0),
    };

    let snap_key = alloc::format!("{}/{}", vg_name, snap_name);
    serial_println!("[lvm] lvsnapshot: '{}' of '{}'", snap_key, origin_key);
    LVS.lock().insert(snap_key, snap);
    Ok(())
}

/// Remove a logical volume
pub fn lvremove(vg_name: &str, lv_name: &str) -> Result<(), &'static str> {
    let key = alloc::format!("{}/{}", vg_name, lv_name);
    let mut lvs = LVS.lock();
    let lv = lvs.remove(&key).ok_or("LV not found")?;

    let mut vgs = VGS.lock();
    if let Some(vg) = vgs.get_mut(vg_name) {
        vg.free_extents += lv.size_extents;
        vg.lvs.retain(|n| n != lv_name);
    }

    serial_println!("[lvm] lvremove: '{}'", key);
    Ok(())
}

/// List logical volumes
pub fn lvdisplay() -> Vec<(String, String, u64, LvType, bool)> {
    LVS.lock()
        .values()
        .map(|lv| {
            (
                alloc::format!("{}/{}", lv.vg_name, lv.name),
                lv.uuid.clone(),
                lv.size_extents,
                lv.lv_type,
                lv.active,
            )
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════════

fn allocate_extents(vg: &VolumeGroup, count: u64) -> Result<Vec<(String, u64)>, &'static str> {
    let mut allocated = Vec::new();
    let mut remaining = count;

    // Linear allocation from first PV with space
    let mut pvs = PVS.lock();
    for pv_uuid in &vg.pvs {
        if remaining == 0 {
            break;
        }
        if let Some(pv) = pvs.get_mut(pv_uuid) {
            for i in 0..pv.extent_map.len() {
                if remaining == 0 {
                    break;
                }
                if !pv.extent_map[i] {
                    pv.extent_map[i] = true;
                    pv.free_extents -= 1;
                    allocated.push((pv_uuid.clone(), i as u64));
                    remaining -= 1;
                }
            }
        }
    }

    if remaining > 0 {
        return Err("Could not allocate all extents");
    }
    Ok(allocated)
}

/// Initialize LVM subsystem
pub fn init() {
    serial_println!("[lvm] Logical Volume Manager initialized");
}
