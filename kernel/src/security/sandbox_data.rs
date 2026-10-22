use crate::serial_println;
/// Sandboxed Application Data Isolation
///
/// Per-app data directories, restricting access to other apps' data,
/// XDG-like isolation, portal-based file access.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone)]
pub struct AppSandbox {
    pub app_id: String,
    pub data_dir: String,
    pub cache_dir: String,
    pub config_dir: String,
    pub allowed_paths: Vec<String>,
    pub network_access: bool,
    pub dbus_access: bool,
}

pub struct SandboxManager {
    pub sandboxes: Vec<AppSandbox>,
}

lazy_static::lazy_static! {
    static ref MGR: Mutex<SandboxManager> = Mutex::new(SandboxManager {
        sandboxes: Vec::new(),
    });
}

impl SandboxManager {
    pub fn create_sandbox(&mut self, app_id: &str) -> &AppSandbox {
        let sandbox = AppSandbox {
            app_id: String::from(app_id),
            data_dir: alloc::format!("/home/.app-data/{}/data", app_id),
            cache_dir: alloc::format!("/home/.app-data/{}/cache", app_id),
            config_dir: alloc::format!("/home/.app-data/{}/config", app_id),
            allowed_paths: Vec::new(),
            network_access: false,
            dbus_access: false,
        };
        serial_println!("[SANDBOX] Created for: {}", app_id);
        self.sandboxes.push(sandbox);
        self.sandboxes.last().unwrap()
    }

    pub fn grant_path(&mut self, app_id: &str, path: &str) {
        if let Some(sb) = self.sandboxes.iter_mut().find(|s| s.app_id == app_id) {
            sb.allowed_paths.push(String::from(path));
            serial_println!("[SANDBOX] {} granted access to {}", app_id, path);
        }
    }

    pub fn check_access(&self, app_id: &str, path: &str) -> bool {
        if let Some(sb) = self.sandboxes.iter().find(|s| s.app_id == app_id) {
            // Allow access to own directories
            if path.starts_with(&sb.data_dir)
                || path.starts_with(&sb.cache_dir)
                || path.starts_with(&sb.config_dir)
            {
                return true;
            }
            // Check granted paths
            sb.allowed_paths
                .iter()
                .any(|p| path.starts_with(p.as_str()))
        } else {
            false
        }
    }

    pub fn set_network(&mut self, app_id: &str, allowed: bool) {
        if let Some(sb) = self.sandboxes.iter_mut().find(|s| s.app_id == app_id) {
            sb.network_access = allowed;
        }
    }
}

pub fn init() {
    serial_println!("[SANDBOX] Application data isolation initialized");
}
