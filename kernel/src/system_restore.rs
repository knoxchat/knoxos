use crate::serial_println;
/// System Restore Points
///
/// Btrfs/ZFS-style snapshots before major changes, rollback support,
/// scheduled automatic snapshots.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone)]
pub struct RestorePoint {
    pub id: u64,
    pub label: String,
    pub timestamp: u64,
    pub snapshot_subvol: String,
    pub size_bytes: u64,
    pub auto_created: bool,
}

pub struct RestoreManager {
    pub points: Vec<RestorePoint>,
    pub max_points: usize,
    pub auto_before_upgrade: bool,
    pub next_id: u64,
}

lazy_static::lazy_static! {
    static ref RESTORE: Mutex<RestoreManager> = Mutex::new(RestoreManager {
        points: Vec::new(),
        max_points: 10,
        auto_before_upgrade: true,
        next_id: 1,
    });
}

impl RestoreManager {
    pub fn create_point(&mut self, label: &str, auto: bool) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        let point = RestorePoint {
            id,
            label: String::from(label),
            timestamp: 0,
            snapshot_subvol: alloc::format!("/.snapshots/{}", id),
            size_bytes: 0,
            auto_created: auto,
        };
        serial_println!("[RESTORE] Point #{} created: {}", id, label);
        self.points.push(point);

        // Trim oldest
        while self.points.len() > self.max_points {
            let removed = self.points.remove(0);
            serial_println!("[RESTORE] Pruned old point #{}", removed.id);
        }
        id
    }

    pub fn rollback(&self, id: u64) -> bool {
        if let Some(point) = self.points.iter().find(|p| p.id == id) {
            serial_println!(
                "[RESTORE] Rolling back to #{}: {} ({})",
                id,
                point.label,
                point.snapshot_subvol
            );
            // Would swap root subvolume to snapshot
            true
        } else {
            false
        }
    }

    pub fn delete_point(&mut self, id: u64) -> bool {
        if let Some(pos) = self.points.iter().position(|p| p.id == id) {
            let removed = self.points.remove(pos);
            serial_println!("[RESTORE] Deleted point #{}: {}", id, removed.label);
            true
        } else {
            false
        }
    }

    pub fn list(&self) -> &[RestorePoint] {
        &self.points
    }
}

pub fn init() {
    serial_println!("[RESTORE] System restore point manager initialized");
}
