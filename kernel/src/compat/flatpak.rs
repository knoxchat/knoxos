/// Flatpak & AppImage — Container-based application packaging
///
/// Provides sandbox execution for portable applications:
///   - Flatpak: OCI-like layered filesystem with runtime dependencies
///   - AppImage: Single-file portable executables with bundled libs
///   - Sandbox: filesystem/network/IPC isolation via namespaces
///   - Portal: controlled access to host resources (files, print, camera)
///   - Runtime management: shared runtimes for Flatpak apps
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── AppImage Support ───────────────────────────────────────────────

/// AppImage magic bytes: "AI\x02" at offset 8
const APPIMAGE_MAGIC: [u8; 3] = [0x41, 0x49, 0x02];

#[derive(Debug, Clone)]
pub struct AppImage {
    /// Unique ID
    pub id: u32,
    /// Application name (from .desktop file in image)
    pub name: String,
    /// Version
    pub version: String,
    /// Path to the .AppImage file
    pub path: String,
    /// Size in bytes
    pub size: u64,
    /// Extracted mount point (FUSE-like in-memory FS)
    pub mount_point: Option<String>,
    /// Whether currently running
    pub running: bool,
    /// SquashFS offset within the AppImage file
    pub fs_offset: u64,
}

/// AppImage type (Type 1 = ISO 9660, Type 2 = SquashFS)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppImageType {
    Type1Iso,
    Type2SquashFS,
}

// ─── Flatpak Support ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FlatpakApp {
    pub id: u32,
    pub app_id: String, // e.g., "org.mozilla.Firefox"
    pub name: String,
    pub version: String,
    pub runtime: String, // e.g., "org.freedesktop.Platform/x86_64/23.08"
    pub installed_size: u64,
    pub permissions: FlatpakPermissions,
    pub running: bool,
}

#[derive(Debug, Clone, Default)]
pub struct FlatpakPermissions {
    pub filesystem_access: Vec<String>, // e.g., ["home", "host", "/tmp"]
    pub network: bool,
    pub ipc: bool,
    pub x11: bool,
    pub wayland: bool,
    pub pulseaudio: bool,
    pub dbus_session: Vec<String>,
    pub dbus_system: Vec<String>,
    pub devices: Vec<String>, // e.g., ["dri", "all"]
}

#[derive(Debug, Clone)]
pub struct FlatpakRuntime {
    pub id: String,
    pub version: String,
    pub arch: String,
    pub size: u64,
}

// ─── Portal System (host resource access broker) ────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortalType {
    FileChooser,
    OpenUri,
    Print,
    Screenshot,
    Notification,
    Camera,
    Location,
    Clipboard,
}

#[derive(Debug, Clone)]
pub struct PortalRequest {
    pub portal_type: PortalType,
    pub app_id: String,
    pub granted: bool,
}

// ─── Sandbox Configuration ──────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Isolated filesystem root
    pub rootfs: String,
    /// Bind mounts (host_path -> sandbox_path)
    pub bind_mounts: BTreeMap<String, String>,
    /// Read-only bind mounts
    pub ro_bind_mounts: BTreeMap<String, String>,
    /// Environment variables
    pub env: BTreeMap<String, String>,
    /// Whether network is accessible
    pub net_enabled: bool,
    /// Whether IPC is accessible
    pub ipc_enabled: bool,
    /// PID namespace isolation
    pub pid_ns: bool,
    /// Resource limits
    pub memory_limit_mb: u64,
    pub cpu_shares: u32,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            rootfs: String::from("/run/sandbox"),
            bind_mounts: BTreeMap::new(),
            ro_bind_mounts: {
                let mut m = BTreeMap::new();
                m.insert(String::from("/usr"), String::from("/usr"));
                m.insert(String::from("/lib"), String::from("/lib"));
                m.insert(
                    String::from("/etc/resolv.conf"),
                    String::from("/etc/resolv.conf"),
                );
                m
            },
            env: BTreeMap::new(),
            net_enabled: false,
            ipc_enabled: false,
            pid_ns: true,
            memory_limit_mb: 2048,
            cpu_shares: 1024,
        }
    }
}

// ─── Global State ───────────────────────────────────────────────────

lazy_static::lazy_static! {
    static ref APPIMAGES: Mutex<Vec<AppImage>> = Mutex::new(Vec::new());
    static ref FLATPAK_APPS: Mutex<Vec<FlatpakApp>> = Mutex::new(Vec::new());
    static ref FLATPAK_RUNTIMES: Mutex<Vec<FlatpakRuntime>> = Mutex::new(Vec::new());
    static ref PORTAL_LOG: Mutex<Vec<PortalRequest>> = Mutex::new(Vec::new());
}

static NEXT_ID: AtomicU32 = AtomicU32::new(1);

// ─── AppImage API ───────────────────────────────────────────────────

/// Check if a file is an AppImage
pub fn is_appimage(data: &[u8]) -> bool {
    if data.len() < 16 {
        return false;
    }
    // Check for ELF header + AppImage magic at offset 8
    data[0] == 0x7F
        && data[1] == b'E'
        && data[2] == b'L'
        && data[3] == b'F'
        && data[8] == APPIMAGE_MAGIC[0]
        && data[9] == APPIMAGE_MAGIC[1]
        && data[10] == APPIMAGE_MAGIC[2]
}

/// Detect AppImage type
pub fn detect_type(data: &[u8]) -> Option<AppImageType> {
    if !is_appimage(data) {
        return None;
    }
    // Type 2 uses SquashFS (most common)
    // Look for SquashFS magic "hsqs" after the ELF binary
    for offset in (4096..data.len().min(1024 * 1024)).step_by(4096) {
        if offset + 4 <= data.len() && &data[offset..offset + 4] == b"hsqs" {
            return Some(AppImageType::Type2SquashFS);
        }
    }
    Some(AppImageType::Type1Iso)
}

/// Register an AppImage for execution
pub fn register_appimage(name: &str, path: &str, data: &[u8]) -> Option<u32> {
    let app_type = detect_type(data)?;

    let fs_offset = match app_type {
        AppImageType::Type2SquashFS => {
            // Find SquashFS offset
            let mut offset = 0u64;
            for o in (4096..data.len().min(1024 * 1024)).step_by(4096) {
                if o + 4 <= data.len() && &data[o..o + 4] == b"hsqs" {
                    offset = o as u64;
                    break;
                }
            }
            offset
        }
        AppImageType::Type1Iso => 0,
    };

    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let image = AppImage {
        id,
        name: String::from(name),
        version: String::from("1.0.0"),
        path: String::from(path),
        size: data.len() as u64,
        mount_point: None,
        running: false,
        fs_offset,
    };

    serial_println!(
        "[appimage] registered: {} (type={:?}, fs_offset={:#x})",
        name,
        app_type,
        fs_offset
    );
    APPIMAGES.lock().push(image);
    Some(id)
}

/// Run an AppImage in a sandbox
pub fn run_appimage(id: u32) -> bool {
    let mut images = APPIMAGES.lock();
    if let Some(img) = images.iter_mut().find(|i| i.id == id) {
        if img.running {
            return false;
        }

        // Create sandbox
        let mut sandbox = SandboxConfig {
            rootfs: format!("/run/appimage/{}", id),
            net_enabled: true,
            ..Default::default()
        };

        img.mount_point = Some(sandbox.rootfs.clone());
        img.running = true;

        serial_println!(
            "[appimage] started: {} (sandbox={})",
            img.name,
            sandbox.rootfs
        );
        true
    } else {
        false
    }
}

// ─── Flatpak API ────────────────────────────────────────────────────

/// Install a Flatpak runtime
pub fn install_runtime(id: &str, version: &str, arch: &str, size: u64) {
    let rt = FlatpakRuntime {
        id: String::from(id),
        version: String::from(version),
        arch: String::from(arch),
        size,
    };
    serial_println!("[flatpak] runtime installed: {}/{}/{}", id, arch, version);
    FLATPAK_RUNTIMES.lock().push(rt);
}

/// Install a Flatpak application
pub fn install_app(
    app_id: &str,
    name: &str,
    version: &str,
    runtime: &str,
    permissions: FlatpakPermissions,
    size: u64,
) -> u32 {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let app = FlatpakApp {
        id,
        app_id: String::from(app_id),
        name: String::from(name),
        version: String::from(version),
        runtime: String::from(runtime),
        installed_size: size,
        permissions,
        running: false,
    };
    serial_println!(
        "[flatpak] installed: {} ({}) [runtime={}]",
        name,
        app_id,
        runtime
    );
    FLATPAK_APPS.lock().push(app);
    id
}

/// Run a Flatpak app in its sandbox
pub fn run_flatpak(id: u32) -> bool {
    let mut apps = FLATPAK_APPS.lock();
    if let Some(app) = apps.iter_mut().find(|a| a.id == id) {
        if app.running {
            return false;
        }

        // Verify runtime exists
        let runtimes = FLATPAK_RUNTIMES.lock();
        if !runtimes.iter().any(|r| app.runtime.contains(&r.id)) {
            serial_println!("[flatpak] runtime not found: {}", app.runtime);
            return false;
        }
        drop(runtimes);

        // Build sandbox from permissions
        let mut sandbox = SandboxConfig {
            net_enabled: app.permissions.network,
            ipc_enabled: app.permissions.ipc,
            rootfs: format!("/run/flatpak/{}", app.app_id),
            ..Default::default()
        };

        for path in &app.permissions.filesystem_access {
            match path.as_str() {
                "home" => {
                    sandbox
                        .bind_mounts
                        .insert(String::from("/home"), String::from("/home"));
                }
                "host" => {
                    sandbox
                        .bind_mounts
                        .insert(String::from("/"), String::from("/"));
                }
                p => {
                    sandbox.bind_mounts.insert(String::from(p), String::from(p));
                }
            }
        }

        app.running = true;
        serial_println!(
            "[flatpak] started: {} (net={}, ipc={})",
            app.name,
            sandbox.net_enabled,
            sandbox.ipc_enabled
        );
        true
    } else {
        false
    }
}

/// Uninstall a Flatpak app
pub fn uninstall_flatpak(id: u32) -> bool {
    let mut apps = FLATPAK_APPS.lock();
    if let Some(pos) = apps.iter().position(|a| a.id == id) {
        let name = apps[pos].name.clone();
        apps.remove(pos);
        serial_println!("[flatpak] uninstalled: {}", name);
        true
    } else {
        false
    }
}

// ─── Portal API ─────────────────────────────────────────────────────

/// Request portal access (host resource broker)
pub fn portal_request(app_id: &str, portal_type: PortalType) -> bool {
    // In a full implementation, this would show a user prompt
    let granted = match portal_type {
        PortalType::Notification | PortalType::Clipboard => true, // Auto-granted
        _ => true, // For now, grant all (would be user-prompted in production)
    };

    let req = PortalRequest {
        portal_type,
        app_id: String::from(app_id),
        granted,
    };
    serial_println!(
        "[portal] {:?} request from {}: {}",
        portal_type,
        app_id,
        if granted { "granted" } else { "denied" }
    );
    PORTAL_LOG.lock().push(req);
    granted
}

// ─── Query API ──────────────────────────────────────────────────────

pub fn list_appimages() -> Vec<AppImage> {
    APPIMAGES.lock().clone()
}

pub fn list_flatpak_apps() -> Vec<FlatpakApp> {
    FLATPAK_APPS.lock().clone()
}

pub fn list_runtimes() -> Vec<FlatpakRuntime> {
    FLATPAK_RUNTIMES.lock().clone()
}

pub fn flatpak_count() -> usize {
    FLATPAK_APPS.lock().len()
}

pub fn appimage_count() -> usize {
    APPIMAGES.lock().len()
}

/// Initialize Flatpak/AppImage subsystem
pub fn init() {
    // Install default runtime
    install_runtime("org.freedesktop.Platform", "23.08", "x86_64", 800_000_000);
    install_runtime("org.gnome.Platform", "45", "x86_64", 1_200_000_000);

    serial_println!(
        "[flatpak] initialized ({} runtimes)",
        FLATPAK_RUNTIMES.lock().len()
    );
    serial_println!("[appimage] initialized (Type 2 SquashFS supported)");
    serial_println!("[portal] portal broker active");
}
