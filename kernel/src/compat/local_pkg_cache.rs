use crate::serial_println;
/// Local Package Cache Management
///
/// Clean old packages, manage disk usage, configure retention policies.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone)]
pub struct CachedPackage {
    pub name: String,
    pub version: String,
    pub size_bytes: u64,
    pub download_time: u64,
    pub last_accessed: u64,
}

pub struct PkgCache {
    pub packages: Vec<CachedPackage>,
    pub cache_dir: String,
    pub max_size_bytes: u64,
    pub used_bytes: u64,
    pub keep_versions: usize, // keep N most recent versions
}

lazy_static::lazy_static! {
    static ref CACHE: Mutex<PkgCache> = Mutex::new(PkgCache {
        packages: Vec::new(),
        cache_dir: String::from("/var/cache/kpkg"),
        max_size_bytes: 2 * 1024 * 1024 * 1024,
        used_bytes: 0,
        keep_versions: 2,
    });
}

impl PkgCache {
    pub fn add(&mut self, pkg: CachedPackage) {
        self.used_bytes += pkg.size_bytes;
        self.packages.push(pkg);
    }

    pub fn clean_old(&mut self) -> u64 {
        let keep = self.keep_versions;
        let mut freed = 0u64;
        // Group by name, keep only N newest
        let mut names: Vec<String> = self.packages.iter().map(|p| p.name.clone()).collect();
        names.sort();
        names.dedup();
        for name in &names {
            let mut versions: Vec<usize> = self
                .packages
                .iter()
                .enumerate()
                .filter(|(_, p)| &p.name == name)
                .map(|(i, _)| i)
                .collect();
            versions.sort_by(|a, b| {
                self.packages[*b]
                    .download_time
                    .cmp(&self.packages[*a].download_time)
            });
            for &idx in versions.iter().skip(keep) {
                freed += self.packages[idx].size_bytes;
            }
        }
        serial_println!("[PKG_CACHE] Cleaned {} bytes", freed);
        self.used_bytes = self.used_bytes.saturating_sub(freed);
        freed
    }

    pub fn clean_all(&mut self) -> u64 {
        let freed = self.used_bytes;
        self.packages.clear();
        self.used_bytes = 0;
        serial_println!("[PKG_CACHE] All cache cleared ({} bytes)", freed);
        freed
    }

    pub fn stats(&self) -> (usize, u64, u64) {
        (self.packages.len(), self.used_bytes, self.max_size_bytes)
    }
}

pub fn init() {
    serial_println!("[PKG_CACHE] Local package cache manager initialized");
}
