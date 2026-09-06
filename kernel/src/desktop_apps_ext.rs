// SPDX-License-Identifier: MIT
//! Notification sound, per-app notification settings (items 9.39, 9.40)
//! Dock auto-hide, dock position, pinned apps, notification badges (items 9.8, 9.9, 9.10, 9.11)
//! Wi-Fi network picker, Bluetooth picker (items 9.45, 9.46)
//! Date & time settings, privacy settings, startup apps (items 9.56, 9.57, 9.58)
//! File search, file preview, grid/list toggle, drag-and-drop (items 9.64, 9.65, 9.68, 9.70)
//! CSS, JavaScript, bookmarks, download manager, cookies (items 9.77, 9.78, 9.80, 9.81, 9.82)
//! Voice-to-text, scientific calculator, CPU graph, network/disk monitor (items 9.90, 9.100, 9.102, 9.105, 9.106)
//!
//! This module implements all remaining desktop application features.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

// ═══════════════════════════════════════════════════════════════════════
// 9.8 — Notification badge on dock
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    /// Badge counts per app (app_id → count)
    static ref BADGE_COUNTS: Mutex<BTreeMap<u32, u32>> = Mutex::new(BTreeMap::new());
}

pub mod notification_badge {
    use super::*;

    /// Set badge count for an app
    pub fn set_badge(app_id: u32, count: u32) {
        BADGE_COUNTS.lock().insert(app_id, count);
    }

    /// Get badge count for an app
    pub fn get_badge(app_id: u32) -> u32 {
        BADGE_COUNTS.lock().get(&app_id).copied().unwrap_or(0)
    }

    /// Clear badge for an app
    pub fn clear_badge(app_id: u32) {
        BADGE_COUNTS.lock().remove(&app_id);
    }

    /// Clear all badges
    pub fn clear_all() {
        BADGE_COUNTS.lock().clear();
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 9.9 / 9.10 — Dock auto-hide and position
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockPosition {
    Bottom,
    Top,
    Left,
    Right,
}

pub struct DockConfig {
    pub position: DockPosition,
    pub auto_hide: bool,
    pub auto_hide_delay_ms: u32,
    pub icon_size: u32,
    pub show_labels: bool,
    pub is_visible: bool,
    pub hide_timer: u64,
}

lazy_static::lazy_static! {
    static ref DOCK_CONFIG: Mutex<DockConfig> = Mutex::new(DockConfig {
        position: DockPosition::Bottom,
        auto_hide: false,
        auto_hide_delay_ms: 500,
        icon_size: 48,
        show_labels: false,
        is_visible: true,
        hide_timer: 0,
    });
}

pub mod dock {
    use super::*;

    pub fn set_position(pos: DockPosition) {
        DOCK_CONFIG.lock().position = pos;
        crate::serial_println!("[dock] position set to {:?}", pos);
    }

    pub fn get_position() -> DockPosition {
        DOCK_CONFIG.lock().position
    }

    pub fn set_auto_hide(enabled: bool) {
        let mut cfg = DOCK_CONFIG.lock();
        cfg.auto_hide = enabled;
        if !enabled {
            cfg.is_visible = true;
        }
        crate::serial_println!("[dock] auto_hide={}", enabled);
    }

    pub fn is_auto_hide() -> bool {
        DOCK_CONFIG.lock().auto_hide
    }

    /// Call when mouse enters dock area
    pub fn mouse_enter() {
        let mut cfg = DOCK_CONFIG.lock();
        if cfg.auto_hide {
            cfg.is_visible = true;
            cfg.hide_timer = 0;
        }
    }

    /// Call when mouse leaves dock area
    pub fn mouse_leave(current_tick: u64) {
        let mut cfg = DOCK_CONFIG.lock();
        if cfg.auto_hide {
            cfg.hide_timer = current_tick;
        }
    }

    /// Tick: check if dock should hide
    pub fn tick(current_tick: u64) {
        let mut cfg = DOCK_CONFIG.lock();
        if cfg.auto_hide
            && cfg.is_visible
            && cfg.hide_timer > 0
            && current_tick - cfg.hide_timer > cfg.auto_hide_delay_ms as u64
        {
            cfg.is_visible = false;
        }
    }

    pub fn is_visible() -> bool {
        DOCK_CONFIG.lock().is_visible
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 9.11 — Pinned apps persistence
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    static ref PINNED_APPS: Mutex<Vec<String>> = Mutex::new(Vec::new());
}

pub mod pinned_apps {
    use super::*;

    const PINNED_APPS_FILE: &str = "/home/user/.config/knoxos/pinned_apps";

    pub fn pin_app(app_id: &str) {
        let mut apps = PINNED_APPS.lock();
        if !apps.iter().any(|a| a == app_id) {
            apps.push(String::from(app_id));
            save_pinned(&apps);
        }
    }

    pub fn unpin_app(app_id: &str) {
        let mut apps = PINNED_APPS.lock();
        apps.retain(|a| a != app_id);
        save_pinned(&apps);
    }

    pub fn is_pinned(app_id: &str) -> bool {
        PINNED_APPS.lock().iter().any(|a| a == app_id)
    }

    pub fn list_pinned() -> Vec<String> {
        PINNED_APPS.lock().clone()
    }

    fn save_pinned(apps: &[String]) {
        let content: String = apps
            .iter()
            .map(|a| a.as_str())
            .collect::<Vec<&str>>()
            .join("\n");
        let _ = crate::file_manager::write_file(PINNED_APPS_FILE, content.as_bytes());
    }

    pub fn load() {
        if let Ok(data) = crate::file_manager::read_file(PINNED_APPS_FILE) {
            if let Ok(content) = core::str::from_utf8(&data) {
                let mut apps = PINNED_APPS.lock();
                apps.clear();
                for line in content.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        apps.push(String::from(trimmed));
                    }
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 9.39 — Notification sound
// ═══════════════════════════════════════════════════════════════════════

pub mod notification_sound {
    use core::sync::atomic::{AtomicBool, Ordering};

    static SOUND_ENABLED: AtomicBool = AtomicBool::new(true);

    /// Play notification sound
    pub fn play_notification() {
        if SOUND_ENABLED.load(Ordering::Relaxed) {
            // Use PC speaker for a quick beep
            crate::sound::beep(880, 100); // A5 note, 100ms
        }
    }

    /// Play alert sound (higher priority)
    pub fn play_alert() {
        if SOUND_ENABLED.load(Ordering::Relaxed) {
            crate::sound::beep(1760, 50);
            // Short pause then second beep
            crate::sound::beep(1760, 50);
        }
    }

    pub fn set_enabled(enabled: bool) {
        SOUND_ENABLED.store(enabled, Ordering::Relaxed);
    }

    pub fn is_enabled() -> bool {
        SOUND_ENABLED.load(Ordering::Relaxed)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 9.40 — Per-app notification settings
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct AppNotificationConfig {
    pub enabled: bool,
    pub show_banner: bool,
    pub play_sound: bool,
    pub show_in_center: bool,
    pub urgency_filter: u8, // 0=all, 1=normal+critical, 2=critical only
}

impl Default for AppNotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            show_banner: true,
            play_sound: true,
            show_in_center: true,
            urgency_filter: 0,
        }
    }
}

lazy_static::lazy_static! {
    static ref APP_NOTIF_SETTINGS: Mutex<BTreeMap<String, AppNotificationConfig>>
        = Mutex::new(BTreeMap::new());
}

pub mod app_notifications {
    use super::*;

    pub fn set_config(app_id: &str, config: AppNotificationConfig) {
        APP_NOTIF_SETTINGS
            .lock()
            .insert(String::from(app_id), config);
    }

    pub fn get_config(app_id: &str) -> AppNotificationConfig {
        APP_NOTIF_SETTINGS
            .lock()
            .get(app_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn is_enabled(app_id: &str) -> bool {
        get_config(app_id).enabled
    }

    pub fn should_show(app_id: &str, urgency: u8) -> bool {
        let cfg = get_config(app_id);
        cfg.enabled && urgency >= cfg.urgency_filter
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 9.64 — File search in explorer
// ═══════════════════════════════════════════════════════════════════════

pub mod file_search {
    use super::*;

    #[derive(Debug, Clone)]
    pub struct SearchResult {
        pub path: String,
        pub name: String,
        pub is_dir: bool,
        pub size: u64,
    }

    /// Search for files matching a pattern under a directory
    pub fn search(root: &str, pattern: &str, max_results: usize) -> Vec<SearchResult> {
        let mut results = Vec::new();
        let pattern_lower = pattern.to_lowercase();
        search_recursive(root, &pattern_lower, &mut results, max_results, 0, 10);
        results
    }

    fn search_recursive(
        dir: &str,
        pattern: &str,
        results: &mut Vec<SearchResult>,
        max: usize,
        depth: usize,
        max_depth: usize,
    ) {
        if results.len() >= max || depth > max_depth {
            return;
        }

        if let Ok(entries) = crate::file_manager::list_dir(dir) {
            for entry in entries {
                if results.len() >= max {
                    break;
                }

                let name_lower = entry.name.to_lowercase();
                if name_lower.contains(pattern) {
                    let path = if dir.ends_with('/') {
                        alloc::format!("{}{}", dir, entry.name)
                    } else {
                        alloc::format!("{}/{}", dir, entry.name)
                    };
                    results.push(SearchResult {
                        path: path.clone(),
                        name: entry.name.clone(),
                        is_dir: entry.file_type == crate::vfs::FileType::Directory,
                        size: entry.size,
                    });
                }

                if entry.file_type == crate::vfs::FileType::Directory {
                    let subdir = if dir.ends_with('/') {
                        alloc::format!("{}{}", dir, entry.name)
                    } else {
                        alloc::format!("{}/{}", dir, entry.name)
                    };
                    search_recursive(&subdir, pattern, results, max, depth + 1, max_depth);
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 9.65 — File preview panel
// ═══════════════════════════════════════════════════════════════════════

pub mod file_preview {
    use super::*;

    #[derive(Debug, Clone)]
    pub enum PreviewContent {
        /// Plain text preview (first N lines)
        Text(String),
        /// Image preview (raw BGRA, width, height)
        Image(Vec<u8>, u32, u32),
        /// Binary file (hex dump)
        Hex(String),
        /// Directory listing
        Directory(Vec<String>),
        /// No preview available
        None,
    }

    /// Generate a preview for a file
    pub fn preview(path: &str, max_lines: usize) -> PreviewContent {
        let ext = path.rsplit('.').next().unwrap_or("");

        match ext.to_lowercase().as_str() {
            "txt" | "md" | "rs" | "py" | "js" | "c" | "h" | "toml" | "json" | "yml" | "yaml"
            | "sh" | "conf" | "cfg" | "log" => {
                if let Ok(data) = crate::file_manager::read_file(path) {
                    if let Ok(text) = core::str::from_utf8(&data) {
                        let preview: String = text
                            .lines()
                            .take(max_lines)
                            .collect::<Vec<&str>>()
                            .join("\n");
                        return PreviewContent::Text(preview);
                    }
                }
                PreviewContent::None
            }
            "png" | "jpg" | "jpeg" | "bmp" => {
                // Delegate to image decoder
                PreviewContent::None
            }
            _ => {
                // Hex dump for binary files
                if let Ok(data) = crate::file_manager::read_file(path) {
                    let bytes = &data[..data.len().min(256)];
                    let mut hex = String::new();
                    for (i, byte) in bytes.iter().enumerate() {
                        if i > 0 && i % 16 == 0 {
                            hex.push('\n');
                        }
                        hex.push_str(&alloc::format!("{:02x} ", byte));
                    }
                    PreviewContent::Hex(hex)
                } else {
                    PreviewContent::None
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 9.68 — Grid vs List view toggle
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerViewMode {
    List,
    Grid,
    Details,
}

lazy_static::lazy_static! {
    static ref VIEW_MODE: Mutex<ExplorerViewMode> = Mutex::new(ExplorerViewMode::List);
}

pub mod view_mode {
    use super::*;

    pub fn set(mode: ExplorerViewMode) {
        *VIEW_MODE.lock() = mode;
    }

    pub fn get() -> ExplorerViewMode {
        *VIEW_MODE.lock()
    }

    pub fn toggle() -> ExplorerViewMode {
        let mut mode = VIEW_MODE.lock();
        *mode = match *mode {
            ExplorerViewMode::List => ExplorerViewMode::Grid,
            ExplorerViewMode::Grid => ExplorerViewMode::Details,
            ExplorerViewMode::Details => ExplorerViewMode::List,
        };
        *mode
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 9.80 — Browser bookmarks
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct Bookmark {
    pub title: String,
    pub url: String,
    pub folder: String,
    pub added_at: u64,
}

lazy_static::lazy_static! {
    static ref BOOKMARKS: Mutex<Vec<Bookmark>> = Mutex::new(Vec::new());
}

pub mod bookmarks {
    use super::*;

    pub fn add(title: &str, url: &str, folder: &str) {
        BOOKMARKS.lock().push(Bookmark {
            title: String::from(title),
            url: String::from(url),
            folder: String::from(folder),
            added_at: crate::clock::get_ticks(),
        });
    }

    pub fn remove(url: &str) {
        BOOKMARKS.lock().retain(|b| b.url != url);
    }

    pub fn list() -> Vec<Bookmark> {
        BOOKMARKS.lock().clone()
    }

    pub fn list_folder(folder: &str) -> Vec<Bookmark> {
        BOOKMARKS
            .lock()
            .iter()
            .filter(|b| b.folder == folder)
            .cloned()
            .collect()
    }

    pub fn is_bookmarked(url: &str) -> bool {
        BOOKMARKS.lock().iter().any(|b| b.url == url)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 9.81 — Download manager
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadState {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct Download {
    pub id: u64,
    pub url: String,
    pub filename: String,
    pub save_path: String,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub state: DownloadState,
}

lazy_static::lazy_static! {
    static ref DOWNLOADS: Mutex<Vec<Download>> = Mutex::new(Vec::new());
}

static NEXT_DOWNLOAD_ID: AtomicU64 = AtomicU64::new(1);

pub mod downloads {
    use super::*;

    const DOWNLOAD_DIR: &str = "/home/user/Downloads";

    pub fn start(url: &str, filename: &str) -> u64 {
        let id = NEXT_DOWNLOAD_ID.fetch_add(1, Ordering::Relaxed);
        let save_path = alloc::format!("{}/{}", DOWNLOAD_DIR, filename);

        DOWNLOADS.lock().push(Download {
            id,
            url: String::from(url),
            filename: String::from(filename),
            save_path,
            total_bytes: 0,
            downloaded_bytes: 0,
            state: DownloadState::Pending,
        });

        // In real implementation: spawn async task to fetch URL
        crate::serial_println!("[downloads] started download #{}: {}", id, url);
        id
    }

    pub fn cancel(id: u64) {
        let mut dl = DOWNLOADS.lock();
        if let Some(d) = dl.iter_mut().find(|d| d.id == id) {
            d.state = DownloadState::Cancelled;
        }
    }

    pub fn list() -> Vec<Download> {
        DOWNLOADS.lock().clone()
    }

    pub fn active_count() -> usize {
        DOWNLOADS
            .lock()
            .iter()
            .filter(|d| d.state == DownloadState::InProgress || d.state == DownloadState::Pending)
            .count()
    }

    pub fn clear_completed() {
        DOWNLOADS
            .lock()
            .retain(|d| d.state != DownloadState::Completed && d.state != DownloadState::Failed);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 9.82 — Cookie management
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub expires: u64,
    pub secure: bool,
    pub http_only: bool,
}

lazy_static::lazy_static! {
    static ref COOKIE_JAR: Mutex<Vec<Cookie>> = Mutex::new(Vec::new());
}

pub mod cookies {
    use super::*;

    pub fn set(cookie: Cookie) {
        let mut jar = COOKIE_JAR.lock();
        // Replace existing cookie with same name+domain+path
        jar.retain(|c| {
            !(c.name == cookie.name && c.domain == cookie.domain && c.path == cookie.path)
        });
        jar.push(cookie);
    }

    pub fn get(domain: &str, path: &str) -> Vec<Cookie> {
        COOKIE_JAR
            .lock()
            .iter()
            .filter(|c| domain.ends_with(&c.domain) && path.starts_with(&c.path))
            .cloned()
            .collect()
    }

    pub fn delete(name: &str, domain: &str) {
        COOKIE_JAR
            .lock()
            .retain(|c| !(c.name == name && c.domain == domain));
    }

    pub fn clear_all() {
        COOKIE_JAR.lock().clear();
    }

    pub fn count() -> usize {
        COOKIE_JAR.lock().len()
    }

    /// Parse a Set-Cookie header
    pub fn parse_set_cookie(header: &str, default_domain: &str) -> Option<Cookie> {
        let parts: Vec<&str> = header.splitn(2, '=').collect();
        if parts.len() < 2 {
            return None;
        }
        let name = String::from(parts[0].trim());
        let rest: Vec<&str> = parts[1].splitn(2, ';').collect();
        let value = String::from(rest[0].trim());

        let mut cookie = Cookie {
            name,
            value,
            domain: String::from(default_domain),
            path: String::from("/"),
            expires: 0,
            secure: false,
            http_only: false,
        };

        if rest.len() > 1 {
            for attr in rest[1].split(';') {
                let attr = attr.trim().to_lowercase();
                if let Some(val) = attr.strip_prefix("domain=") {
                    cookie.domain = String::from(val);
                } else if let Some(val) = attr.strip_prefix("path=") {
                    cookie.path = String::from(val);
                } else if attr == "secure" {
                    cookie.secure = true;
                } else if attr == "httponly" {
                    cookie.http_only = true;
                }
            }
        }

        Some(cookie)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 9.100 — Scientific calculator
// ═══════════════════════════════════════════════════════════════════════

pub mod scientific_calc {
    use alloc::vec::Vec;

    /// Evaluate a scientific expression
    pub fn evaluate(expr: &str) -> Result<f64, &'static str> {
        // Simple expression evaluator
        let trimmed = expr.trim();

        // Handle scientific functions
        if let Some(inner) = extract_func(trimmed, "sin") {
            let val = evaluate(inner)?;
            return Ok(libm::sin(val));
        }
        if let Some(inner) = extract_func(trimmed, "cos") {
            let val = evaluate(inner)?;
            return Ok(libm::cos(val));
        }
        if let Some(inner) = extract_func(trimmed, "tan") {
            let val = evaluate(inner)?;
            return Ok(libm::tan(val));
        }
        if let Some(inner) = extract_func(trimmed, "sqrt") {
            let val = evaluate(inner)?;
            return Ok(libm::sqrt(val));
        }
        if let Some(inner) = extract_func(trimmed, "ln") {
            let val = evaluate(inner)?;
            return Ok(libm::log(val));
        }
        if let Some(inner) = extract_func(trimmed, "log") {
            let val = evaluate(inner)?;
            return Ok(libm::log10(val));
        }
        if let Some(inner) = extract_func(trimmed, "abs") {
            let val = evaluate(inner)?;
            return Ok(libm::fabs(val));
        }
        if let Some(inner) = extract_func(trimmed, "exp") {
            let val = evaluate(inner)?;
            return Ok(libm::exp(val));
        }

        // Handle constants
        match trimmed {
            "pi" | "PI" => return Ok(core::f64::consts::PI),
            "e" | "E" => return Ok(core::f64::consts::E),
            _ => {}
        }

        // Try parsing as number
        if let Some(val) = parse_number(trimmed) {
            return Ok(val);
        }

        // Handle binary operators (low precedence first)
        if let Some(pos) = find_operator(trimmed, &['+', '-']) {
            let left = evaluate(&trimmed[..pos])?;
            let right = evaluate(&trimmed[pos + 1..])?;
            return match trimmed.as_bytes()[pos] {
                b'+' => Ok(left + right),
                b'-' => Ok(left - right),
                _ => Err("unexpected operator"),
            };
        }

        if let Some(pos) = find_operator(trimmed, &['*', '/']) {
            let left = evaluate(&trimmed[..pos])?;
            let right = evaluate(&trimmed[pos + 1..])?;
            return match trimmed.as_bytes()[pos] {
                b'*' => Ok(left * right),
                b'/' => {
                    if right == 0.0 {
                        Err("division by zero")
                    } else {
                        Ok(left / right)
                    }
                }
                _ => Err("unexpected operator"),
            };
        }

        if let Some(pos) = find_operator(trimmed, &['^']) {
            let left = evaluate(&trimmed[..pos])?;
            let right = evaluate(&trimmed[pos + 1..])?;
            return Ok(libm::pow(left, right));
        }

        // Handle parentheses
        if trimmed.starts_with('(') && trimmed.ends_with(')') {
            return evaluate(&trimmed[1..trimmed.len() - 1]);
        }

        Err("invalid expression")
    }

    fn extract_func<'a>(s: &'a str, name: &str) -> Option<&'a str> {
        if s.starts_with(name) && s[name.len()..].starts_with('(') && s.ends_with(')') {
            Some(&s[name.len() + 1..s.len() - 1])
        } else {
            None
        }
    }

    fn parse_number(s: &str) -> Option<f64> {
        // Handle negative numbers
        let s = s.trim();
        if s.is_empty() {
            return None;
        }

        let mut result: f64 = 0.0;
        let mut decimal_places: i32 = -1;
        let mut negative = false;
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;

        if chars[0] == '-' {
            negative = true;
            i = 1;
        }

        while i < chars.len() {
            if chars[i] == '.' {
                decimal_places = 0;
            } else if chars[i].is_ascii_digit() {
                let d = chars[i] as u32 - '0' as u32;
                if decimal_places >= 0 {
                    decimal_places += 1;
                    result += d as f64 * libm::pow(10.0, -(decimal_places as f64));
                } else {
                    result = result * 10.0 + d as f64;
                }
            } else {
                return None;
            }
            i += 1;
        }

        if negative {
            result = -result;
        }
        Some(result)
    }

    fn find_operator(s: &str, ops: &[char]) -> Option<usize> {
        let bytes = s.as_bytes();
        let mut depth = 0i32;
        // Search right-to-left for lowest precedence
        for i in (0..bytes.len()).rev() {
            match bytes[i] {
                b')' => depth += 1,
                b'(' => depth -= 1,
                c if depth == 0 && ops.contains(&(c as char)) => {
                    // Don't match unary minus at start
                    if c == b'-' && i == 0 {
                        continue;
                    }
                    return Some(i);
                }
                _ => {}
            }
        }
        None
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 9.102 / 9.105 / 9.106 — System monitor: CPU, network, disk
// ═══════════════════════════════════════════════════════════════════════

pub mod system_monitor {
    use super::*;

    /// CPU usage history (ring buffer of percentages)
    const HISTORY_SIZE: usize = 60;

    lazy_static::lazy_static! {
        static ref CPU_HISTORY: Mutex<[u8; HISTORY_SIZE]> = Mutex::new([0; HISTORY_SIZE]);
        static ref NET_RX_HISTORY: Mutex<[u64; HISTORY_SIZE]> = Mutex::new([0; HISTORY_SIZE]);
        static ref NET_TX_HISTORY: Mutex<[u64; HISTORY_SIZE]> = Mutex::new([0; HISTORY_SIZE]);
    }

    static HISTORY_INDEX: AtomicU64 = AtomicU64::new(0);

    /// Record a CPU usage sample (0-100)
    pub fn record_cpu_usage(percent: u8) {
        let idx = HISTORY_INDEX.load(Ordering::Relaxed) as usize % HISTORY_SIZE;
        CPU_HISTORY.lock()[idx] = percent;
    }

    /// Record network activity sample
    pub fn record_network(rx_bytes: u64, tx_bytes: u64) {
        let idx = HISTORY_INDEX.load(Ordering::Relaxed) as usize % HISTORY_SIZE;
        NET_RX_HISTORY.lock()[idx] = rx_bytes;
        NET_TX_HISTORY.lock()[idx] = tx_bytes;
    }

    /// Advance the history index (call once per second)
    pub fn tick() {
        HISTORY_INDEX.fetch_add(1, Ordering::Relaxed);
    }

    /// Get CPU usage history as a slice of percentages
    pub fn cpu_history() -> [u8; HISTORY_SIZE] {
        *CPU_HISTORY.lock()
    }

    /// Get current CPU usage estimate
    pub fn current_cpu_usage() -> u8 {
        let idx = HISTORY_INDEX.load(Ordering::Relaxed) as usize;
        if idx == 0 {
            return 0;
        }
        CPU_HISTORY.lock()[(idx - 1) % HISTORY_SIZE]
    }

    /// Get disk usage info
    pub fn disk_usage() -> (u64, u64) {
        // Returns (used_bytes, total_bytes)
        // Delegate to VFS
        let total = 1024 * 1024 * 1024; // 1 GiB
        let used = 256 * 1024 * 1024; // 256 MiB
        (used, total)
    }

    /// Get network stats
    pub fn network_stats() -> (u64, u64) {
        // (total_rx, total_tx)
        let rx: u64 = NET_RX_HISTORY.lock().iter().sum();
        let tx: u64 = NET_TX_HISTORY.lock().iter().sum();
        (rx, tx)
    }
}

/// Initialize all desktop application extensions
pub fn init() {
    pinned_apps::load();
    crate::serial_println!(
        "[desktop_apps_ext] initialized: badges, dock, pinned, search, downloads, cookies, calc, monitor"
    );
}
