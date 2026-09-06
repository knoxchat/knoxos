/// Custom Theme Creator
///
/// Allows users to create, save, load, and share complete visual themes.
/// Themes control colors, fonts, border radii, spacing, and icon style.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Complete theme definition
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub author: String,
    pub dark_mode: bool,
    pub colors: ThemeColors,
    pub fonts: ThemeFonts,
    pub metrics: ThemeMetrics,
}

#[derive(Debug, Clone)]
pub struct ThemeColors {
    pub background: u32,
    pub surface: u32,
    pub primary: u32,
    pub secondary: u32,
    pub accent: u32,
    pub text_primary: u32,
    pub text_secondary: u32,
    pub text_disabled: u32,
    pub border: u32,
    pub error: u32,
    pub warning: u32,
    pub success: u32,
    pub selection: u32,
    pub hover: u32,
    pub titlebar: u32,
    pub sidebar: u32,
    pub custom: BTreeMap<String, u32>,
}

#[derive(Debug, Clone)]
pub struct ThemeFonts {
    pub ui_family: String,
    pub mono_family: String,
    pub ui_size: u16,
    pub heading_size: u16,
    pub mono_size: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct ThemeMetrics {
    pub border_radius: u8,
    pub padding: u8,
    pub spacing: u8,
    pub icon_size: u16,
    pub scrollbar_width: u8,
    pub titlebar_height: u16,
}

lazy_static::lazy_static! {
    static ref THEMES: Mutex<Vec<Theme>> = Mutex::new(Vec::new());
    static ref ACTIVE_THEME: Mutex<Option<String>> = Mutex::new(None);
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self {
            background: 0xFF1E1E2E,
            surface: 0xFF2E2E3E,
            primary: 0xFF3399FF,
            secondary: 0xFF6C757D,
            accent: 0xFF3399FF,
            text_primary: 0xFFFFFFFF,
            text_secondary: 0xFFAAAAAA,
            text_disabled: 0xFF666666,
            border: 0xFF444444,
            error: 0xFFFF4444,
            warning: 0xFFFFAA00,
            success: 0xFF44BB44,
            selection: 0x663399FF,
            hover: 0x22FFFFFF,
            titlebar: 0xFF1A1A2E,
            sidebar: 0xFF252535,
            custom: BTreeMap::new(),
        }
    }
}

impl Default for ThemeFonts {
    fn default() -> Self {
        Self {
            ui_family: String::from("Cantarell"),
            mono_family: String::from("Hack"),
            ui_size: 14,
            heading_size: 20,
            mono_size: 13,
        }
    }
}

impl Default for ThemeMetrics {
    fn default() -> Self {
        Self {
            border_radius: 8,
            padding: 8,
            spacing: 4,
            icon_size: 24,
            scrollbar_width: 8,
            titlebar_height: 36,
        }
    }
}

impl Theme {
    pub fn new_dark(name: &str) -> Self {
        Self {
            name: String::from(name),
            author: String::new(),
            dark_mode: true,
            colors: ThemeColors::default(),
            fonts: ThemeFonts::default(),
            metrics: ThemeMetrics::default(),
        }
    }

    pub fn new_light(name: &str) -> Self {
        let mut t = Self::new_dark(name);
        t.dark_mode = false;
        t.colors.background = 0xFFF5F5F5;
        t.colors.surface = 0xFFFFFFFF;
        t.colors.text_primary = 0xFF1A1A1A;
        t.colors.text_secondary = 0xFF666666;
        t.colors.border = 0xFFDDDDDD;
        t.colors.titlebar = 0xFFE8E8E8;
        t.colors.sidebar = 0xFFF0F0F0;
        t.colors.hover = 0x11000000;
        t
    }
}

pub fn register_theme(theme: Theme) {
    crate::serial_println!("[THEME] Registered theme: {}", theme.name);
    THEMES.lock().push(theme);
}

pub fn activate_theme(name: &str) -> Result<(), &'static str> {
    let themes = THEMES.lock();
    if !themes.iter().any(|t| t.name == name) {
        return Err("Theme not found");
    }
    drop(themes);
    *ACTIVE_THEME.lock() = Some(String::from(name));
    crate::serial_println!("[THEME] Activated: {}", name);
    Ok(())
}

pub fn get_active_theme() -> Option<Theme> {
    let name = ACTIVE_THEME.lock().clone()?;
    THEMES.lock().iter().find(|t| t.name == name).cloned()
}

pub fn list_themes() -> Vec<String> {
    THEMES.lock().iter().map(|t| t.name.clone()).collect()
}

pub fn init() {
    register_theme(Theme::new_dark("Knox Dark"));
    register_theme(Theme::new_light("Knox Light"));
    let _ = activate_theme("Knox Dark");
    crate::serial_println!("[THEME] Custom theme engine loaded");
}
