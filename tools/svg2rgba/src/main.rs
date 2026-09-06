/// SVG → BGRA rasterizer for KnoxOS icons
///
/// Walks the icons/ directory tree, renders ALL SVG files at 16/24/32/48 px,
/// packs them into a single binary blob, and generates icon_data.rs with
/// include_bytes! and a register_all() function for the icon theme system.
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Render sizes (largest first for quality)
const SIZES: &[u32] = &[48, 32, 24, 16];

/// Icon category subdirectories in ./icons/
const CATEGORIES: &[&str] = &[
    "actions",
    "animations",
    "apps",
    "categories",
    "devices",
    "emblems",
    "mimetypes",
    "panel",
    "places",
    "status",
];

/// Map category dir name → IconCategory Rust variant name
fn category_variant(cat: &str) -> &'static str {
    match cat {
        "actions" => "Actions",
        "animations" => "Animations",
        "apps" => "Apps",
        "categories" => "Categories",
        "devices" => "Devices",
        "emblems" => "Emblems",
        "mimetypes" => "Mimetypes",
        "panel" => "Panel",
        "places" => "Places",
        "status" => "Status",
        _ => panic!("Unknown category: {}", cat),
    }
}

fn render_svg(svg_path: &Path, size: u32) -> Option<Vec<u8>> {
    let svg_data = fs::read(svg_path).ok()?;
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(&svg_data, &opt).ok()?;

    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;

    let tree_size = tree.size();
    let sx = size as f32 / tree_size.width();
    let sy = size as f32 / tree_size.height();
    let scale = sx.min(sy);

    let offset_x = (size as f32 - tree_size.width() * scale) / 2.0;
    let offset_y = (size as f32 - tree_size.height() * scale) / 2.0;

    let transform =
        tiny_skia::Transform::from_scale(scale, scale).post_translate(offset_x, offset_y);

    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // resvg outputs premultiplied RGBA. Convert to straight BGRA for our framebuffer.
    let mut bgra = Vec::with_capacity((size * size * 4) as usize);
    for pixel in pixmap.pixels() {
        let r = pixel.red();
        let g = pixel.green();
        let b = pixel.blue();
        let a = pixel.alpha();
        // Un-premultiply
        let (r, g, b) = if a == 0 {
            (0, 0, 0)
        } else if a == 255 {
            (r, g, b)
        } else {
            (
                ((r as u16 * 255) / a as u16).min(255) as u8,
                ((g as u16 * 255) / a as u16).min(255) as u8,
                ((b as u16 * 255) / a as u16).min(255) as u8,
            )
        };
        // BGRA order for framebuffer
        bgra.push(b);
        bgra.push(g);
        bgra.push(r);
        bgra.push(a);
    }

    Some(bgra)
}

/// Record for one rasterized icon at one size
struct IconRecord {
    category: String,
    name: String,
    size: u32,
    offset: usize,
    length: usize,
}

/// Legacy alias definition: old constant name → (category, svg stem, size)
struct LegacyAlias {
    const_name: &'static str,
    category: &'static str,
    svg_name: &'static str,
    size: u32,
}

const LEGACY_ALIASES: &[LegacyAlias] = &[
    // 48px aliases (the originals)
    LegacyAlias {
        const_name: "COMPUTER",
        category: "devices",
        svg_name: "computer",
        size: 48,
    },
    LegacyAlias {
        const_name: "FOLDER",
        category: "places",
        svg_name: "folder",
        size: 48,
    },
    LegacyAlias {
        const_name: "DOCUMENT",
        category: "mimetypes",
        svg_name: "text-x-generic",
        size: 48,
    },
    LegacyAlias {
        const_name: "TERMINAL",
        category: "apps",
        svg_name: "terminal-1",
        size: 48,
    },
    LegacyAlias {
        const_name: "BROWSER",
        category: "apps",
        svg_name: "internet-web-browser",
        size: 48,
    },
    LegacyAlias {
        const_name: "MEDIA_PLAYER",
        category: "apps",
        svg_name: "multimedia-video-player",
        size: 48,
    },
    LegacyAlias {
        const_name: "SETTINGS",
        category: "apps",
        svg_name: "org.gnome.Settings",
        size: 48,
    },
    LegacyAlias {
        const_name: "GAME",
        category: "categories",
        svg_name: "applications-games",
        size: 48,
    },
    LegacyAlias {
        const_name: "FILE_MANAGER",
        category: "apps",
        svg_name: "file-manager",
        size: 48,
    },
    LegacyAlias {
        const_name: "AI_BRAIN",
        category: "apps",
        svg_name: "utilities-x-terminal",
        size: 48,
    },
    LegacyAlias {
        const_name: "SEARCH",
        category: "apps",
        svg_name: "system-search",
        size: 48,
    },
    LegacyAlias {
        const_name: "APP_LAUNCHER",
        category: "apps",
        svg_name: "appgrid",
        size: 48,
    },
    // 24px aliases
    LegacyAlias {
        const_name: "COMPUTER_24",
        category: "devices",
        svg_name: "computer",
        size: 24,
    },
    LegacyAlias {
        const_name: "FOLDER_24",
        category: "places",
        svg_name: "folder",
        size: 24,
    },
    LegacyAlias {
        const_name: "DOCUMENT_24",
        category: "mimetypes",
        svg_name: "text-x-generic",
        size: 24,
    },
    LegacyAlias {
        const_name: "TERMINAL_24",
        category: "apps",
        svg_name: "terminal-1",
        size: 24,
    },
    LegacyAlias {
        const_name: "BROWSER_24",
        category: "apps",
        svg_name: "internet-web-browser",
        size: 24,
    },
    LegacyAlias {
        const_name: "MEDIA_PLAYER_24",
        category: "apps",
        svg_name: "multimedia-video-player",
        size: 24,
    },
    LegacyAlias {
        const_name: "SETTINGS_24",
        category: "apps",
        svg_name: "org.gnome.Settings",
        size: 24,
    },
    LegacyAlias {
        const_name: "GAME_24",
        category: "categories",
        svg_name: "applications-games",
        size: 24,
    },
    LegacyAlias {
        const_name: "FILE_MANAGER_24",
        category: "apps",
        svg_name: "file-manager",
        size: 24,
    },
    LegacyAlias {
        const_name: "AI_BRAIN_24",
        category: "apps",
        svg_name: "utilities-x-terminal",
        size: 24,
    },
    LegacyAlias {
        const_name: "SEARCH_24",
        category: "apps",
        svg_name: "system-search",
        size: 24,
    },
    LegacyAlias {
        const_name: "APP_LAUNCHER_24",
        category: "apps",
        svg_name: "appgrid",
        size: 24,
    },
    // 16px aliases
    LegacyAlias {
        const_name: "COMPUTER_16",
        category: "devices",
        svg_name: "computer",
        size: 16,
    },
    LegacyAlias {
        const_name: "FOLDER_16",
        category: "places",
        svg_name: "folder",
        size: 16,
    },
    LegacyAlias {
        const_name: "DOCUMENT_16",
        category: "mimetypes",
        svg_name: "text-x-generic",
        size: 16,
    },
    LegacyAlias {
        const_name: "TERMINAL_16",
        category: "apps",
        svg_name: "terminal-1",
        size: 16,
    },
    LegacyAlias {
        const_name: "BROWSER_16",
        category: "apps",
        svg_name: "internet-web-browser",
        size: 16,
    },
    LegacyAlias {
        const_name: "MEDIA_PLAYER_16",
        category: "apps",
        svg_name: "multimedia-video-player",
        size: 16,
    },
    LegacyAlias {
        const_name: "SETTINGS_16",
        category: "apps",
        svg_name: "org.gnome.Settings",
        size: 16,
    },
    LegacyAlias {
        const_name: "GAME_16",
        category: "categories",
        svg_name: "applications-games",
        size: 16,
    },
    LegacyAlias {
        const_name: "FILE_MANAGER_16",
        category: "apps",
        svg_name: "file-manager",
        size: 16,
    },
    LegacyAlias {
        const_name: "AI_BRAIN_16",
        category: "apps",
        svg_name: "utilities-x-terminal",
        size: 16,
    },
    LegacyAlias {
        const_name: "SEARCH_16",
        category: "apps",
        svg_name: "system-search",
        size: 16,
    },
    LegacyAlias {
        const_name: "APP_LAUNCHER_16",
        category: "apps",
        svg_name: "appgrid",
        size: 16,
    },
];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("Usage: svg2rgba <icons_root_dir> <output_blob.bin> <output.rs>");
        std::process::exit(1);
    }

    let icons_root = Path::new(&args[1]);
    let blob_path = Path::new(&args[2]);
    let rs_path = Path::new(&args[3]);

    let mut blob: Vec<u8> = Vec::new();
    let mut records: Vec<IconRecord> = Vec::new();
    let mut success = 0u32;
    let mut failed = 0u32;

    // Walk every category, discover all SVGs, rasterize at all sizes
    for &category in CATEGORIES {
        let cat_dir = icons_root.join(category);
        if !cat_dir.is_dir() {
            eprintln!("WARNING: Category dir not found: {}", cat_dir.display());
            continue;
        }

        let mut entries: Vec<PathBuf> = fs::read_dir(&cat_dir)
            .expect("Failed to read category dir")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map_or(false, |ext| ext == "svg"))
            .collect();
        entries.sort();

        for svg_path in &entries {
            let name = svg_path.file_stem().unwrap().to_str().unwrap();

            for &size in SIZES {
                match render_svg(svg_path, size) {
                    Some(bgra) => {
                        let offset = blob.len();
                        let length = bgra.len();
                        blob.extend_from_slice(&bgra);
                        records.push(IconRecord {
                            category: category.to_string(),
                            name: name.to_string(),
                            size,
                            offset,
                            length,
                        });
                        success += 1;
                    }
                    None => {
                        eprintln!("FAILED: {}/{} @{}px", category, name, size);
                        failed += 1;
                    }
                }
            }
        }
        eprintln!("  {} — {} SVGs rendered", category, entries.len());
    }

    // Write binary blob
    fs::write(blob_path, &blob).expect("Failed to write blob");
    let unique_icons = records.len() / SIZES.len();
    eprintln!(
        "Blob: {} bytes ({:.1} MB) — {} icons × {} sizes = {} entries",
        blob.len(),
        blob.len() as f64 / 1048576.0,
        unique_icons,
        SIZES.len(),
        records.len()
    );

    // ── Generate Rust source ──────────────────────────────────────────
    let mut out = fs::File::create(rs_path).expect("Failed to create output .rs");

    writeln!(out, "//! Auto-generated icon data — DO NOT EDIT").unwrap();
    writeln!(
        out,
        "//! Generated by tools/svg2rgba from {} SVG icons",
        unique_icons
    )
    .unwrap();
    writeln!(
        out,
        "//! {} entries across {} sizes (16/24/32/48 px)",
        records.len(),
        SIZES.len()
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "use super::icon_theme::{{IconCategory, register_icon}};"
    )
    .unwrap();
    writeln!(out).unwrap();

    // Constants
    writeln!(out, "pub const ICON_SIZE: u32 = 48;").unwrap();
    writeln!(out, "pub const ICON_STRIDE: usize = 48 * 4;").unwrap();
    writeln!(out, "pub const ICON_BYTES: usize = 48 * 48 * 4;").unwrap();
    writeln!(out, "pub const ICON_BYTES_24: usize = 24 * 24 * 4;").unwrap();
    writeln!(out, "pub const ICON_BYTES_16: usize = 16 * 16 * 4;").unwrap();
    writeln!(out).unwrap();

    // Blob include
    writeln!(
        out,
        "/// Packed BGRA icon blob ({:.1} MB, {} icons)",
        blob.len() as f64 / 1048576.0,
        unique_icons
    )
    .unwrap();
    writeln!(
        out,
        "static ICON_BLOB: &[u8] = include_bytes!(\"icon_blob.bin\");"
    )
    .unwrap();
    writeln!(out).unwrap();

    // register_all() — registers every icon into icon_theme
    writeln!(
        out,
        "/// Register all {} icons into the icon theme registry.",
        records.len()
    )
    .unwrap();
    writeln!(out, "pub fn register_all() {{").unwrap();
    for rec in &records {
        let variant = category_variant(&rec.category);
        writeln!(
            out,
            "    register_icon(IconCategory::{}, \"{}\", {}, {sz}, {sz}, &ICON_BLOB[{}..{}]);",
            variant,
            rec.name,
            rec.size,
            rec.offset,
            rec.offset + rec.length,
            sz = rec.size
        )
        .unwrap();
    }
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();

    // Legacy aliases — backward-compatible constants for icons.rs consumers
    // Write inline byte arrays to avoid const-indexing issues
    writeln!(
        out,
        "// ═══ Backward-compatible aliases for legacy icon_data consumers ═══"
    )
    .unwrap();

    for alias in LEGACY_ALIASES {
        if let Some(rec) = records.iter().find(|r| {
            r.category == alias.category && r.name == alias.svg_name && r.size == alias.size
        }) {
            let byte_count = (rec.size * rec.size * 4) as usize;
            let data = &blob[rec.offset..rec.offset + rec.length];
            write!(
                out,
                "#[rustfmt::skip]\npub static {}: [u8; {}] = [\n",
                alias.const_name, byte_count
            )
            .unwrap();
            for (i, chunk) in data.chunks(4).enumerate() {
                if i % 16 == 0 && i > 0 {
                    write!(out, "\n").unwrap();
                }
                if i % 16 == 0 {
                    write!(out, "    ").unwrap();
                }
                write!(out, "{},{},{},{},", chunk[0], chunk[1], chunk[2], chunk[3]).unwrap();
            }
            writeln!(out, "\n];").unwrap();
        } else {
            let byte_count = alias.size * alias.size * 4;
            writeln!(
                out,
                "pub static {}: [u8; {}] = [0u8; {}]; // not found",
                alias.const_name, byte_count, byte_count
            )
            .unwrap();
        }
    }

    eprintln!(
        "Generated {} — {} icons, {} entries, {} failed",
        rs_path.display(),
        unique_icons,
        success,
        failed
    );
}
