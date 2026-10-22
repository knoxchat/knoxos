/// Fontconfig — Font Discovery and Configuration System
///
/// Provides Linux-compatible font configuration for desktop applications.
/// Vivaldi/Chromium requires fontconfig for:
///   - Font discovery (fc-list equivalent)
///   - Font matching (family, style, weight, width)
///   - Font fallback chains (for missing glyphs → CJK, emoji, symbols)
///   - Font substitution rules (/etc/fonts/fonts.conf)
///   - FreeType2 integration (font metrics, glyph outlines)
///
/// KnoxOS font locations:
///   - /usr/share/fonts/       (system fonts)
///   - /usr/local/share/fonts/ (user-installed)
///   - ~/.local/share/fonts/   (per-user fonts)
///   - /opt/vivaldi/resources/ (Vivaldi's bundled fonts)
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// FONT PROPERTIES
// ═══════════════════════════════════════════════════════════════════════

/// Font weight constants (matching CSS/fontconfig values)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FontWeight {
    Thin = 100,
    ExtraLight = 200,
    Light = 300,
    Regular = 400,
    Medium = 500,
    SemiBold = 600,
    Bold = 700,
    ExtraBold = 800,
    Black = 900,
}

impl FontWeight {
    pub fn from_u32(v: u32) -> Self {
        match v {
            0..=149 => Self::Thin,
            150..=249 => Self::ExtraLight,
            250..=349 => Self::Light,
            350..=449 => Self::Regular,
            450..=549 => Self::Medium,
            550..=649 => Self::SemiBold,
            650..=749 => Self::Bold,
            750..=849 => Self::ExtraBold,
            _ => Self::Black,
        }
    }
}

/// Font style (slant)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

/// Font width (stretch)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontWidth {
    UltraCondensed,
    ExtraCondensed,
    Condensed,
    SemiCondensed,
    Normal,
    SemiExpanded,
    Expanded,
    ExtraExpanded,
    UltraExpanded,
}

/// Font format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFormat {
    TrueType,  // .ttf
    OpenType,  // .otf
    Type1,     // .pfb/.pfa
    WOFF,      // .woff
    WOFF2,     // .woff2
    BitmapPCF, // .pcf
    BitmapBDF, // .bdf
}

// ═══════════════════════════════════════════════════════════════════════
// FONT ENTRY
// ═══════════════════════════════════════════════════════════════════════

/// A font face registered in the font database
#[derive(Debug, Clone)]
pub struct FontFace {
    pub family: String,
    pub style: FontStyle,
    pub weight: FontWeight,
    pub width: FontWidth,
    pub format: FontFormat,
    pub file_path: String,
    pub face_index: u32, // face index within TTC/OTC collections
    pub postscript_name: Option<String>,
    pub full_name: String,
    pub languages: Vec<String>, // BCP47 language tags
    pub scalable: bool,
    pub outline: bool,
    pub color: bool,             // color emoji font
    pub variable: bool,          // variable/OpenType font
    pub charset_pages: Vec<u32>, // Unicode page coverage (page = 256 codepoints)
}

/// Font pattern (query/match specification)
#[derive(Debug, Clone, Default)]
pub struct FontPattern {
    pub family: Option<String>,
    pub style: Option<FontStyle>,
    pub weight: Option<FontWeight>,
    pub width: Option<FontWidth>,
    pub size: Option<f32>,
    pub lang: Option<String>,
    pub scalable: Option<bool>,
}

/// Font substitution rule
#[derive(Debug, Clone)]
pub struct SubstitutionRule {
    pub pattern: FontPattern,
    pub replacement: String,
    pub priority: u32,
    pub rule_type: SubstitutionType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubstitutionType {
    Replace, // Replace matching family with another
    Prepend, // Add family before existing list
    Append,  // Add family after existing list
}

// ═══════════════════════════════════════════════════════════════════════
// FONT DATABASE
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    /// All known fonts (family → list of faces)
    static ref FONT_DATABASE: Mutex<BTreeMap<String, Vec<FontFace>>> = Mutex::new(BTreeMap::new());

    /// Font substitution rules
    static ref SUBSTITUTIONS: Mutex<Vec<SubstitutionRule>> = Mutex::new(Vec::new());

    /// Generic family aliases
    static ref GENERIC_ALIASES: Mutex<BTreeMap<String, Vec<String>>> = Mutex::new(BTreeMap::new());

    /// Font directory scan paths
    static ref FONT_DIRS: Mutex<Vec<String>> = Mutex::new(Vec::new());

    /// Cache of font files (path → parsed metadata)
    static ref FONT_CACHE: Mutex<BTreeMap<String, FontFace>> = Mutex::new(BTreeMap::new());
}

/// Register a font face in the database
pub fn register_font(face: FontFace) {
    let family = face.family.clone();
    let mut db = FONT_DATABASE.lock();
    db.entry(family).or_default().push(face);
}

/// Match a font pattern against the database
pub fn match_font(pattern: &FontPattern) -> Option<FontFace> {
    let db = FONT_DATABASE.lock();
    let aliases = GENERIC_ALIASES.lock();

    // Build list of families to try
    let mut families_to_try = Vec::new();
    if let Some(ref family) = pattern.family {
        families_to_try.push(family.clone());
        // Add aliases
        if let Some(alias_list) = aliases.get(family) {
            families_to_try.extend(alias_list.clone());
        }
    }

    // Apply substitution rules
    let substitutions = SUBSTITUTIONS.lock();
    for rule in substitutions.iter() {
        if let Some(ref pat_family) = rule.pattern.family {
            if families_to_try.contains(pat_family) {
                match rule.rule_type {
                    SubstitutionType::Replace => {
                        families_to_try.retain(|f| f != pat_family);
                        families_to_try.push(rule.replacement.clone());
                    }
                    SubstitutionType::Prepend => {
                        families_to_try.insert(0, rule.replacement.clone());
                    }
                    SubstitutionType::Append => {
                        families_to_try.push(rule.replacement.clone());
                    }
                }
            }
        }
    }
    drop(substitutions);

    // Search for best match
    for family in &families_to_try {
        let normalized = family.to_ascii_lowercase();
        for (db_family, faces) in db.iter() {
            if db_family.to_ascii_lowercase() == normalized {
                // Find best match within family
                if let Some(best) = find_best_face(faces, pattern) {
                    return Some(best);
                }
            }
        }
    }

    // Fallback: return first available font
    if let Some(faces) = db.values().next() {
        return faces.first().cloned();
    }

    None
}

/// Find the best matching face within a family
fn find_best_face(faces: &[FontFace], pattern: &FontPattern) -> Option<FontFace> {
    let mut best: Option<&FontFace> = None;
    let mut best_score = u32::MAX;

    for face in faces {
        let mut score = 0u32;

        // Weight matching
        if let Some(want_weight) = pattern.weight {
            let diff = (want_weight as i32 - face.weight as i32).unsigned_abs();
            score += diff;
        }

        // Style matching
        if let Some(want_style) = pattern.style {
            if face.style != want_style {
                score += 1000;
            }
        }

        // Width matching
        if let Some(want_width) = pattern.width {
            if face.width != want_width {
                score += 500;
            }
        }

        // Prefer scalable fonts
        if pattern.scalable == Some(true) && !face.scalable {
            score += 10000;
        }

        if score < best_score {
            best_score = score;
            best = Some(face);
        }
    }

    best.cloned()
}

/// List all font families
pub fn list_families() -> Vec<String> {
    FONT_DATABASE.lock().keys().cloned().collect()
}

/// List all fonts matching a pattern
pub fn list_fonts(pattern: &FontPattern) -> Vec<FontFace> {
    let db = FONT_DATABASE.lock();
    let mut results = Vec::new();

    for faces in db.values() {
        for face in faces {
            let mut matches = true;
            if let Some(ref family) = pattern.family {
                if !face
                    .family
                    .to_ascii_lowercase()
                    .contains(&family.to_ascii_lowercase())
                {
                    matches = false;
                }
            }
            if let Some(style) = pattern.style {
                if face.style != style {
                    matches = false;
                }
            }
            if let Some(weight) = pattern.weight {
                if face.weight != weight {
                    matches = false;
                }
            }
            if matches {
                results.push(face.clone());
            }
        }
    }

    results
}

/// Add a font directory to the search path
pub fn add_font_dir(path: &str) {
    FONT_DIRS.lock().push(path.to_string());
}

/// Scan font directories and populate the database
pub fn scan_font_dirs() {
    let dirs = FONT_DIRS.lock().clone();
    let mut count = 0;

    for dir in &dirs {
        serial_println!("[fontconfig] Scanning {}", dir);
        // In a full implementation, this would use VFS to list and parse font files
        count += scan_single_dir(dir);
    }

    serial_println!(
        "[fontconfig] Scanned {} fonts from {} directories",
        count,
        dirs.len()
    );
}

fn scan_single_dir(dir: &str) -> usize {
    // VFS directory scan would go here
    // For now, register the scan path and return count of pre-registered fonts
    0
}

// ═══════════════════════════════════════════════════════════════════════
// DEFAULT CONFIGURATION (equivalent to /etc/fonts/fonts.conf)
// ═══════════════════════════════════════════════════════════════════════

/// Set up default font configuration
fn setup_default_config() {
    let mut aliases = GENERIC_ALIASES.lock();

    // sans-serif family chain
    aliases.insert(
        String::from("sans-serif"),
        vec![
            String::from("Cantarell"),
            String::from("DejaVu Sans"),
            String::from("Liberation Sans"),
            String::from("Arial"),
            String::from("Helvetica"),
            String::from("Noto Sans"),
        ],
    );

    // serif family chain
    aliases.insert(
        String::from("serif"),
        vec![
            String::from("DejaVu Serif"),
            String::from("Liberation Serif"),
            String::from("Times New Roman"),
            String::from("Noto Serif"),
        ],
    );

    // monospace family chain
    aliases.insert(
        String::from("monospace"),
        vec![
            String::from("Hack"),
            String::from("DejaVu Sans Mono"),
            String::from("Liberation Mono"),
            String::from("Courier New"),
            String::from("Noto Sans Mono"),
        ],
    );

    // Emoji/symbol fallback
    aliases.insert(
        String::from("emoji"),
        vec![
            String::from("Noto Color Emoji"),
            String::from("Twemoji"),
            String::from("Segoe UI Emoji"),
        ],
    );

    // Chromium/Vivaldi-specific mappings
    aliases.insert(
        String::from("system-ui"),
        vec![String::from("Cantarell"), String::from("sans-serif")],
    );

    // Set default substitution rules for web fonts
    let mut subs = SUBSTITUTIONS.lock();

    // Map common web fonts to available alternatives
    let web_font_mappings = [
        ("Arial", "Liberation Sans"),
        ("Helvetica", "Liberation Sans"),
        ("Times New Roman", "Liberation Serif"),
        ("Courier New", "Liberation Mono"),
        ("Verdana", "DejaVu Sans"),
        ("Georgia", "DejaVu Serif"),
        ("Trebuchet MS", "DejaVu Sans"),
    ];

    for (from, to) in &web_font_mappings {
        subs.push(SubstitutionRule {
            pattern: FontPattern {
                family: Some(from.to_string()),
                ..Default::default()
            },
            replacement: to.to_string(),
            priority: 50,
            rule_type: SubstitutionType::Append,
        });
    }
}

/// Register built-in KnoxOS fonts (from the bitmap font generators)
fn register_builtin_fonts() {
    // These correspond to the fonts generated by tools/gen_*_font.py
    let builtins = [
        (
            "Cantarell",
            FontWeight::Regular,
            FontStyle::Normal,
            "/usr/share/fonts/cantarell/Cantarell-Regular.otf",
        ),
        (
            "Cantarell",
            FontWeight::Bold,
            FontStyle::Normal,
            "/usr/share/fonts/cantarell/Cantarell-Bold.otf",
        ),
        (
            "Cantarell",
            FontWeight::Light,
            FontStyle::Normal,
            "/usr/share/fonts/cantarell/Cantarell-Light.otf",
        ),
        (
            "Hack",
            FontWeight::Regular,
            FontStyle::Normal,
            "/usr/share/fonts/hack/Hack-Regular.ttf",
        ),
        (
            "Hack",
            FontWeight::Bold,
            FontStyle::Normal,
            "/usr/share/fonts/hack/Hack-Bold.ttf",
        ),
        (
            "Hack",
            FontWeight::Regular,
            FontStyle::Italic,
            "/usr/share/fonts/hack/Hack-Italic.ttf",
        ),
        (
            "Hack",
            FontWeight::Bold,
            FontStyle::Italic,
            "/usr/share/fonts/hack/Hack-BoldItalic.ttf",
        ),
    ];

    for (family, weight, style, path) in &builtins {
        register_font(FontFace {
            family: family.to_string(),
            style: *style,
            weight: *weight,
            width: FontWidth::Normal,
            format: if path.ends_with(".otf") {
                FontFormat::OpenType
            } else {
                FontFormat::TrueType
            },
            file_path: path.to_string(),
            face_index: 0,
            postscript_name: None,
            full_name: format!(
                "{} {}{}",
                family,
                match weight {
                    FontWeight::Bold => "Bold ",
                    FontWeight::Light => "Light ",
                    _ => "",
                },
                match style {
                    FontStyle::Italic => "Italic",
                    FontStyle::Oblique => "Oblique",
                    FontStyle::Normal => "",
                }
            )
            .trim()
            .to_string(),
            languages: vec![String::from("en")],
            scalable: true,
            outline: true,
            color: false,
            variable: false,
            charset_pages: vec![0, 1, 2, 3, 0x20, 0x21, 0xFB, 0xFE, 0xFF],
        });
    }

    serial_println!(
        "[fontconfig] Registered {} built-in font faces",
        builtins.len()
    );
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Initialize the fontconfig subsystem
pub fn init() {
    serial_println!("[fontconfig] Initializing font configuration...");

    // Set up font search directories
    add_font_dir("/usr/share/fonts");
    add_font_dir("/usr/local/share/fonts");
    add_font_dir("/usr/share/fonts/truetype");
    add_font_dir("/usr/share/fonts/opentype");

    // Set up default configuration (generic families, substitutions)
    setup_default_config();

    // Register built-in bitmap/vector fonts
    register_builtin_fonts();

    // Scan directories for additional fonts
    scan_font_dirs();

    let db = FONT_DATABASE.lock();
    let total_faces: usize = db.values().map(|v| v.len()).sum();
    serial_println!(
        "[fontconfig] Initialized: {} families, {} faces",
        db.len(),
        total_faces
    );
}
