/// XDG — freedesktop.org Base Directory & Desktop Integration
///
/// Implements the XDG Base Directory Specification and related standards:
///   - XDG_DATA_HOME, XDG_CONFIG_HOME, XDG_CACHE_HOME, XDG_RUNTIME_DIR
///   - XDG_DATA_DIRS, XDG_CONFIG_DIRS
///   - Desktop entry files (.desktop)
///   - MIME type detection and association
///   - Application launcher integration
///   - Icon theme specification
///   - Autostart directories
///
/// Vivaldi requires:
///   - MIME type handling (x-scheme-handler/http, x-scheme-handler/https)
///   - Desktop file at /usr/share/applications/vivaldi-stable.desktop
///   - Default browser registration
///   - XDG directories for profile data
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// XDG BASE DIRECTORIES
// ═══════════════════════════════════════════════════════════════════════

/// XDG directory paths for a user
#[derive(Debug, Clone)]
pub struct XdgDirs {
    pub data_home: String,        // ~/.local/share
    pub config_home: String,      // ~/.config
    pub cache_home: String,       // ~/.cache
    pub runtime_dir: String,      // /run/user/<uid>
    pub state_home: String,       // ~/.local/state
    pub data_dirs: Vec<String>,   // /usr/local/share:/usr/share
    pub config_dirs: Vec<String>, // /etc/xdg
}

impl Default for XdgDirs {
    fn default() -> Self {
        Self::for_user(1000, "knoxos")
    }
}

impl XdgDirs {
    pub fn for_user(uid: u32, username: &str) -> Self {
        let home = format!("/home/{}", username);
        Self {
            data_home: format!("{}/.local/share", home),
            config_home: format!("{}/.config", home),
            cache_home: format!("{}/.cache", home),
            runtime_dir: format!("/run/user/{}", uid),
            state_home: format!("{}/.local/state", home),
            data_dirs: vec![String::from("/usr/local/share"), String::from("/usr/share")],
            config_dirs: vec![String::from("/etc/xdg")],
        }
    }

    /// Get the environment variables for this XDG configuration
    pub fn to_env(&self) -> Vec<(String, String)> {
        vec![
            (String::from("XDG_DATA_HOME"), self.data_home.clone()),
            (String::from("XDG_CONFIG_HOME"), self.config_home.clone()),
            (String::from("XDG_CACHE_HOME"), self.cache_home.clone()),
            (String::from("XDG_RUNTIME_DIR"), self.runtime_dir.clone()),
            (String::from("XDG_STATE_HOME"), self.state_home.clone()),
            (String::from("XDG_DATA_DIRS"), self.data_dirs.join(":")),
            (String::from("XDG_CONFIG_DIRS"), self.config_dirs.join(":")),
        ]
    }
}

// ═══════════════════════════════════════════════════════════════════════
// DESKTOP ENTRY FILES (.desktop)
// ═══════════════════════════════════════════════════════════════════════

/// A parsed .desktop file
#[derive(Debug, Clone)]
pub struct DesktopEntry {
    pub entry_type: DesktopEntryType,
    pub name: String,
    pub generic_name: Option<String>,
    pub comment: Option<String>,
    pub icon: Option<String>,
    pub exec: Option<String>,
    pub try_exec: Option<String>,
    pub path: Option<String>,
    pub terminal: bool,
    pub no_display: bool,
    pub hidden: bool,
    pub categories: Vec<String>,
    pub mime_types: Vec<String>,
    pub keywords: Vec<String>,
    pub startup_notify: bool,
    pub startup_wm_class: Option<String>,
    pub actions: Vec<DesktopAction>,
    pub file_path: String, // Path of the .desktop file itself
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopEntryType {
    Application,
    Link,
    Directory,
}

/// A desktop action (e.g., "Open New Window", "Open New Private Window")
#[derive(Debug, Clone)]
pub struct DesktopAction {
    pub id: String,
    pub name: String,
    pub exec: Option<String>,
    pub icon: Option<String>,
}

/// Parse a .desktop file from text
pub fn parse_desktop_entry(text: &str, file_path: &str) -> Option<DesktopEntry> {
    let mut fields: BTreeMap<String, String> = BTreeMap::new();
    let mut actions: Vec<DesktopAction> = Vec::new();
    let mut current_section = String::new();
    let mut action_fields: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current_section = line[1..line.len() - 1].to_string();
            continue;
        }
        if let Some(eq) = line.find('=') {
            let key = line[..eq].trim().to_string();
            let value = line[eq + 1..].trim().to_string();

            if current_section == "Desktop Entry" {
                fields.insert(key, value);
            } else if let Some(action_name) = current_section.strip_prefix("Desktop Action ") {
                let action_id = action_name.to_string();
                action_fields
                    .entry(action_id)
                    .or_default()
                    .insert(key, value);
            }
        }
    }

    let entry_type = match fields.get("Type").map(|s| s.as_str()) {
        Some("Application") => DesktopEntryType::Application,
        Some("Link") => DesktopEntryType::Link,
        Some("Directory") => DesktopEntryType::Directory,
        _ => DesktopEntryType::Application,
    };

    let name = fields.get("Name").cloned()?;

    // Parse actions
    if let Some(action_list) = fields.get("Actions") {
        for action_id in action_list.split(';').filter(|s| !s.is_empty()) {
            if let Some(af) = action_fields.get(action_id) {
                actions.push(DesktopAction {
                    id: action_id.to_string(),
                    name: af.get("Name").cloned().unwrap_or_default(),
                    exec: af.get("Exec").cloned(),
                    icon: af.get("Icon").cloned(),
                });
            }
        }
    }

    Some(DesktopEntry {
        entry_type,
        name,
        generic_name: fields.get("GenericName").cloned(),
        comment: fields.get("Comment").cloned(),
        icon: fields.get("Icon").cloned(),
        exec: fields.get("Exec").cloned(),
        try_exec: fields.get("TryExec").cloned(),
        path: fields.get("Path").cloned(),
        terminal: fields.get("Terminal").map(|s| s == "true").unwrap_or(false),
        no_display: fields
            .get("NoDisplay")
            .map(|s| s == "true")
            .unwrap_or(false),
        hidden: fields.get("Hidden").map(|s| s == "true").unwrap_or(false),
        categories: fields
            .get("Categories")
            .map(|s| {
                s.split(';')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default(),
        mime_types: fields
            .get("MimeType")
            .map(|s| {
                s.split(';')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default(),
        keywords: fields
            .get("Keywords")
            .map(|s| {
                s.split(';')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default(),
        startup_notify: fields
            .get("StartupNotify")
            .map(|s| s == "true")
            .unwrap_or(false),
        startup_wm_class: fields.get("StartupWMClass").cloned(),
        actions,
        file_path: file_path.to_string(),
    })
}

// ═══════════════════════════════════════════════════════════════════════
// MIME TYPE DATABASE
// ═══════════════════════════════════════════════════════════════════════

/// MIME type entry
#[derive(Debug, Clone)]
pub struct MimeType {
    pub mime: String,
    pub description: String,
    pub extensions: Vec<String>,
    pub icon: Option<String>,
    pub parent_types: Vec<String>,
}

/// MIME type → default application mapping
#[derive(Debug, Clone)]
pub struct MimeAssociation {
    pub mime_type: String,
    pub desktop_file: String,
}

lazy_static::lazy_static! {
    /// Registered desktop entries
    static ref DESKTOP_ENTRIES: Mutex<BTreeMap<String, DesktopEntry>> = Mutex::new(BTreeMap::new());

    /// MIME type database
    static ref MIME_TYPES: Mutex<BTreeMap<String, MimeType>> = Mutex::new(BTreeMap::new());

    /// Default applications (MIME → .desktop)
    static ref DEFAULT_APPS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

    /// XDG directories for the default user
    static ref USER_DIRS: Mutex<XdgDirs> = Mutex::new(XdgDirs::default());
}

/// Register a desktop entry
pub fn register_desktop_entry(entry: DesktopEntry) {
    let key = entry.file_path.clone();

    // Register MIME associations
    for mime in &entry.mime_types {
        let mut defaults = DEFAULT_APPS.lock();
        // Only set as default if no default exists yet
        defaults.entry(mime.clone()).or_insert(key.clone());
    }

    DESKTOP_ENTRIES.lock().insert(key, entry);
}

/// Get the default application for a MIME type
pub fn get_default_app(mime_type: &str) -> Option<DesktopEntry> {
    let defaults = DEFAULT_APPS.lock();
    let desktop_file = defaults.get(mime_type)?;
    DESKTOP_ENTRIES.lock().get(desktop_file).cloned()
}

/// Set the default application for a MIME type
pub fn set_default_app(mime_type: &str, desktop_file: &str) {
    DEFAULT_APPS
        .lock()
        .insert(mime_type.to_string(), desktop_file.to_string());
}

/// List all registered applications
pub fn list_applications() -> Vec<DesktopEntry> {
    DESKTOP_ENTRIES.lock().values().cloned().collect()
}

/// Detect MIME type from filename extension
pub fn detect_mime_type(filename: &str) -> String {
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "html" | "htm" => String::from("text/html"),
        "css" => String::from("text/css"),
        "js" => String::from("application/javascript"),
        "json" => String::from("application/json"),
        "xml" => String::from("application/xml"),
        "txt" => String::from("text/plain"),
        "md" => String::from("text/markdown"),
        "pdf" => String::from("application/pdf"),
        "png" => String::from("image/png"),
        "jpg" | "jpeg" => String::from("image/jpeg"),
        "gif" => String::from("image/gif"),
        "svg" => String::from("image/svg+xml"),
        "webp" => String::from("image/webp"),
        "mp3" => String::from("audio/mpeg"),
        "mp4" => String::from("video/mp4"),
        "webm" => String::from("video/webm"),
        "ogg" => String::from("audio/ogg"),
        "wav" => String::from("audio/wav"),
        "zip" => String::from("application/zip"),
        "tar" => String::from("application/x-tar"),
        "gz" => String::from("application/gzip"),
        "deb" => String::from("application/vnd.debian.binary-package"),
        "desktop" => String::from("application/x-desktop"),
        "so" => String::from("application/x-sharedlib"),
        _ => String::from("application/octet-stream"),
    }
}

/// Open a URI or file with the appropriate application
pub fn xdg_open(uri: &str) -> Result<(), String> {
    serial_println!("[xdg] xdg-open: {}", uri);

    // Determine MIME type or URI scheme
    let mime_type = if uri.starts_with("http://") || uri.starts_with("https://") {
        String::from("x-scheme-handler/https")
    } else if uri.starts_with("mailto:") {
        String::from("x-scheme-handler/mailto")
    } else if uri.starts_with("file://") {
        let path = uri.trim_start_matches("file://");
        detect_mime_type(path)
    } else {
        detect_mime_type(uri)
    };

    // Find default application
    if let Some(app) = get_default_app(&mime_type) {
        serial_println!(
            "[xdg] Opening with: {} ({})",
            app.name,
            app.exec.as_deref().unwrap_or("(no exec)")
        );
        // In a full implementation, exec the application with the URI as argument
        Ok(())
    } else {
        Err(format!("No application registered for {}", mime_type))
    }
}

// ═══════════════════════════════════════════════════════════════════════
// VIVALDI DESKTOP ENTRY
// ═══════════════════════════════════════════════════════════════════════

/// Create and register Vivaldi's desktop entry
fn register_vivaldi_desktop_entry() {
    let vivaldi_desktop = DesktopEntry {
        entry_type: DesktopEntryType::Application,
        name: String::from("Vivaldi"),
        generic_name: Some(String::from("Web Browser")),
        comment: Some(String::from(
            "Access the Internet with Vivaldi, a browser for our friends",
        )),
        icon: Some(String::from("vivaldi")),
        exec: Some(String::from(
            "/opt/vivaldi/vivaldi --enable-features=WaylandWindowDecorations %U",
        )),
        try_exec: Some(String::from("/opt/vivaldi/vivaldi")),
        path: None,
        terminal: false,
        no_display: false,
        hidden: false,
        categories: vec![String::from("Network"), String::from("WebBrowser")],
        mime_types: vec![
            String::from("text/html"),
            String::from("text/xml"),
            String::from("application/xhtml+xml"),
            String::from("application/xml"),
            String::from("application/rss+xml"),
            String::from("application/rdf+xml"),
            String::from("x-scheme-handler/http"),
            String::from("x-scheme-handler/https"),
            String::from("x-scheme-handler/ftp"),
            String::from("x-scheme-handler/mailto"),
            String::from("x-scheme-handler/webcal"),
        ],
        keywords: vec![
            String::from("Internet"),
            String::from("WWW"),
            String::from("Browser"),
            String::from("Web"),
            String::from("Vivaldi"),
        ],
        startup_notify: true,
        startup_wm_class: Some(String::from("Vivaldi-stable")),
        actions: vec![
            DesktopAction {
                id: String::from("new-window"),
                name: String::from("New Window"),
                exec: Some(String::from("/opt/vivaldi/vivaldi --new-window")),
                icon: None,
            },
            DesktopAction {
                id: String::from("new-private-window"),
                name: String::from("New Private Window"),
                exec: Some(String::from("/opt/vivaldi/vivaldi --incognito")),
                icon: None,
            },
        ],
        file_path: String::from("/usr/share/applications/vivaldi-stable.desktop"),
    };

    register_desktop_entry(vivaldi_desktop);
    serial_println!("[xdg] Registered Vivaldi desktop entry");
}

/// Set up default MIME type associations for common web content
fn setup_default_mime_types() {
    let mut mimes = MIME_TYPES.lock();

    let common_mimes = [
        (
            "text/html",
            "HTML Document",
            &["html", "htm"][..],
            "text-html",
        ),
        (
            "text/plain",
            "Plain Text",
            &["txt", "text"][..],
            "text-plain",
        ),
        (
            "application/pdf",
            "PDF Document",
            &["pdf"][..],
            "application-pdf",
        ),
        ("image/png", "PNG Image", &["png"][..], "image-png"),
        (
            "image/jpeg",
            "JPEG Image",
            &["jpg", "jpeg"][..],
            "image-jpeg",
        ),
        ("image/svg+xml", "SVG Image", &["svg"][..], "image-svg+xml"),
        ("video/mp4", "MP4 Video", &["mp4", "m4v"][..], "video-mp4"),
        ("video/webm", "WebM Video", &["webm"][..], "video-webm"),
        ("audio/mpeg", "MP3 Audio", &["mp3"][..], "audio-mpeg"),
        (
            "application/zip",
            "ZIP Archive",
            &["zip"][..],
            "application-zip",
        ),
        (
            "application/vnd.debian.binary-package",
            "Debian Package",
            &["deb"][..],
            "application-x-deb",
        ),
    ];

    for (mime, desc, exts, icon) in &common_mimes {
        mimes.insert(
            mime.to_string(),
            MimeType {
                mime: mime.to_string(),
                description: desc.to_string(),
                extensions: exts.iter().map(|e| e.to_string()).collect(),
                icon: Some(icon.to_string()),
                parent_types: Vec::new(),
            },
        );
    }
}

/// Create XDG runtime directories
fn create_xdg_directories() {
    let dirs = USER_DIRS.lock();

    let paths = [
        dirs.data_home.as_str(),
        dirs.config_home.as_str(),
        dirs.cache_home.as_str(),
        dirs.runtime_dir.as_str(),
        dirs.state_home.as_str(),
        // Vivaldi-specific directories
        "/home/knoxos/.config/vivaldi",
        "/home/knoxos/.cache/vivaldi",
        "/home/knoxos/.local/share/vivaldi",
    ];

    for path in &paths {
        serial_println!("[xdg] Creating directory: {}", path);
        // In a full implementation: vfs::mkdir_p(path, 0o700)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Initialize the XDG desktop integration subsystem
pub fn init() {
    serial_println!("[xdg] Initializing XDG desktop integration...");

    // Create XDG directories
    create_xdg_directories();

    // Set up MIME type database
    setup_default_mime_types();

    // Register Vivaldi's desktop entry
    register_vivaldi_desktop_entry();

    serial_println!(
        "[xdg] {} desktop entries, {} MIME types, {} default apps",
        DESKTOP_ENTRIES.lock().len(),
        MIME_TYPES.lock().len(),
        DEFAULT_APPS.lock().len()
    );
}
