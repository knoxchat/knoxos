//! CSS Engine — Basic CSS parsing and style application
//!
//! Provides a minimal CSS parser for the KnoxOS browser, supporting
//! common properties (color, background, font-size, margin, padding,
//! display, border, width, height).
//! Covers status.md item 9.77 (CSS support).

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

/// A CSS property value
#[derive(Debug, Clone)]
pub enum CssValue {
    /// A color value (ARGB)
    Color(u32),
    /// A length value in pixels
    Px(f32),
    /// A percentage value
    Percent(f32),
    /// A string keyword (e.g., "block", "none", "bold")
    Keyword(String),
    /// "auto"
    Auto,
    /// "inherit"
    Inherit,
}

/// A CSS rule (selector → declarations)
#[derive(Debug, Clone)]
pub struct CssRule {
    pub selector: CssSelector,
    pub declarations: Vec<CssDeclaration>,
}

/// CSS selector (simplified)
#[derive(Debug, Clone)]
pub enum CssSelector {
    /// Element selector: `p`, `div`, `h1`
    Element(String),
    /// Class selector: `.classname`
    Class(String),
    /// ID selector: `#id`
    Id(String),
    /// Universal: `*`
    Universal,
    /// Compound: `div.classname`
    Compound(Vec<CssSelector>),
}

/// A CSS declaration (property: value)
#[derive(Debug, Clone)]
pub struct CssDeclaration {
    pub property: String,
    pub value: CssValue,
}

/// Parsed stylesheet
#[derive(Debug, Clone)]
pub struct Stylesheet {
    pub rules: Vec<CssRule>,
}

/// Computed style for an element
#[derive(Debug, Clone, Default)]
pub struct ComputedStyle {
    pub color: Option<u32>,
    pub background_color: Option<u32>,
    pub font_size_px: Option<f32>,
    pub font_weight: Option<String>,
    pub display: Option<String>,
    pub margin_top: f32,
    pub margin_right: f32,
    pub margin_bottom: f32,
    pub margin_left: f32,
    pub padding_top: f32,
    pub padding_right: f32,
    pub padding_bottom: f32,
    pub padding_left: f32,
    pub width: Option<CssValue>,
    pub height: Option<CssValue>,
    pub border_width: f32,
    pub border_color: Option<u32>,
    pub text_align: Option<String>,
    pub text_decoration: Option<String>,
}

static PARSE_COUNT: AtomicU64 = AtomicU64::new(0);

/// Parse a CSS color string
pub fn parse_color(s: &str) -> Option<u32> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()? as u32;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()? as u32;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()? as u32;
                Some(0xFF000000 | (r << 16) | (g << 8) | b)
            }
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? as u32;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? as u32;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? as u32;
                Some(0xFF000000 | ((r * 17) << 16) | ((g * 17) << 8) | (b * 17))
            }
            _ => None,
        }
    } else {
        // Named colors
        match s.to_lowercase().as_str() {
            "black" => Some(0xFF000000),
            "white" => Some(0xFFFFFFFF),
            "red" => Some(0xFFFF0000),
            "green" => Some(0xFF008000),
            "blue" => Some(0xFF0000FF),
            "yellow" => Some(0xFFFFFF00),
            "cyan" => Some(0xFF00FFFF),
            "magenta" => Some(0xFFFF00FF),
            "gray" | "grey" => Some(0xFF808080),
            "orange" => Some(0xFFFFA500),
            "transparent" => Some(0x00000000),
            _ => None,
        }
    }
}

/// Parse a CSS length value
pub fn parse_length(s: &str) -> Option<CssValue> {
    let s = s.trim();
    if s == "auto" {
        return Some(CssValue::Auto);
    }
    if s == "inherit" {
        return Some(CssValue::Inherit);
    }
    if let Some(num) = s.strip_suffix("px") {
        let num = num.trim();
        if let Ok(v) = num.parse::<f32>() {
            return Some(CssValue::Px(v));
        }
    }
    if let Some(num) = s.strip_suffix('%') {
        let num = num.trim();
        if let Ok(v) = num.parse::<f32>() {
            return Some(CssValue::Percent(v));
        }
    }
    // Plain number = pixels
    if let Ok(v) = s.parse::<f32>() {
        return Some(CssValue::Px(v));
    }
    None
}

/// Tokenize CSS input into rules
pub fn parse_stylesheet(css: &str) -> Stylesheet {
    PARSE_COUNT.fetch_add(1, Ordering::Relaxed);
    let mut rules = Vec::new();
    let mut chars = css.chars().peekable();
    let mut current = String::new();

    while let Some(&ch) = chars.peek() {
        match ch {
            '{' => {
                chars.next();
                let selector_str = current.trim().to_string();
                current.clear();

                // Read declarations until '}'
                let mut decl_str = String::new();
                while let Some(&dch) = chars.peek() {
                    if dch == '}' {
                        chars.next();
                        break;
                    }
                    decl_str.push(dch);
                    chars.next();
                }

                let selector = parse_selector(&selector_str);
                let declarations = parse_declarations(&decl_str);
                rules.push(CssRule {
                    selector,
                    declarations,
                });
            }
            _ => {
                current.push(ch);
                chars.next();
            }
        }
    }

    Stylesheet { rules }
}

fn parse_selector(s: &str) -> CssSelector {
    let s = s.trim();
    if s == "*" {
        return CssSelector::Universal;
    }
    if let Some(rest) = s.strip_prefix('#') {
        return CssSelector::Id(String::from(rest));
    }
    if let Some(rest) = s.strip_prefix('.') {
        return CssSelector::Class(String::from(rest));
    }
    CssSelector::Element(String::from(s.to_lowercase().as_str()))
}

fn parse_declarations(s: &str) -> Vec<CssDeclaration> {
    let mut decls = Vec::new();
    for part in s.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(colon) = part.find(':') {
            let prop = part[..colon].trim().to_lowercase();
            let val_str = part[colon + 1..].trim();

            let value = if let Some(color) = parse_color(val_str) {
                CssValue::Color(color)
            } else if let Some(len) = parse_length(val_str) {
                len
            } else {
                CssValue::Keyword(String::from(val_str))
            };

            decls.push(CssDeclaration {
                property: String::from(prop.as_str()),
                value,
            });
        }
    }
    decls
}

/// Apply a stylesheet to compute style for an element
pub fn compute_style(
    stylesheet: &Stylesheet,
    element_tag: &str,
    element_class: Option<&str>,
    element_id: Option<&str>,
) -> ComputedStyle {
    let mut style = ComputedStyle::default();

    for rule in &stylesheet.rules {
        let matches = match &rule.selector {
            CssSelector::Universal => true,
            CssSelector::Element(tag) => tag == element_tag,
            CssSelector::Class(cls) => element_class == Some(cls.as_str()),
            CssSelector::Id(id) => element_id == Some(id.as_str()),
            CssSelector::Compound(_) => false, // Simplified
        };

        if matches {
            for decl in &rule.declarations {
                apply_declaration(&mut style, decl);
            }
        }
    }

    style
}

fn apply_declaration(style: &mut ComputedStyle, decl: &CssDeclaration) {
    match decl.property.as_str() {
        "color" => {
            if let CssValue::Color(c) = decl.value {
                style.color = Some(c);
            }
        }
        "background-color" | "background" => {
            if let CssValue::Color(c) = decl.value {
                style.background_color = Some(c);
            }
        }
        "font-size" => {
            if let CssValue::Px(v) = decl.value {
                style.font_size_px = Some(v);
            }
        }
        "font-weight" => {
            if let CssValue::Keyword(ref k) = decl.value {
                style.font_weight = Some(k.clone());
            }
        }
        "display" => {
            if let CssValue::Keyword(ref k) = decl.value {
                style.display = Some(k.clone());
            }
        }
        "text-align" => {
            if let CssValue::Keyword(ref k) = decl.value {
                style.text_align = Some(k.clone());
            }
        }
        "text-decoration" => {
            if let CssValue::Keyword(ref k) = decl.value {
                style.text_decoration = Some(k.clone());
            }
        }
        "margin" => {
            if let CssValue::Px(v) = decl.value {
                style.margin_top = v;
                style.margin_right = v;
                style.margin_bottom = v;
                style.margin_left = v;
            }
        }
        "padding" => {
            if let CssValue::Px(v) = decl.value {
                style.padding_top = v;
                style.padding_right = v;
                style.padding_bottom = v;
                style.padding_left = v;
            }
        }
        "width" => {
            style.width = Some(decl.value.clone());
        }
        "height" => {
            style.height = Some(decl.value.clone());
        }
        "border-width" => {
            if let CssValue::Px(v) = decl.value {
                style.border_width = v;
            }
        }
        "border-color" => {
            if let CssValue::Color(c) = decl.value {
                style.border_color = Some(c);
            }
        }
        _ => {} // Unknown property — ignore
    }
}

/// Get parse count
pub fn parse_count() -> u64 {
    PARSE_COUNT.load(Ordering::Relaxed)
}

/// Initialize the CSS engine
pub fn init() {
    crate::serial_println!(
        "[css_engine] CSS engine initialized (selectors, colors, lengths, box model)"
    );
}
