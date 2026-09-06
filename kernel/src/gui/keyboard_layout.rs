use super::event_types::KeyCode;
/// International Keyboard Layouts
///
/// Maps physical key codes to Unicode characters based on the selected
/// keyboard layout. Uses the project's own `event_types::KeyCode`.
///
/// Supported layouts:
///   - US QWERTY (default)
///   - UK QWERTY (ISO)
///   - German QWERTZ
///   - French AZERTY
///   - Spanish QWERTY
///   - Portuguese QWERTY
///   - Japanese Romaji (US-based with IME hook)
///   - Korean Dubeolsik (basic Latin layer)
///   - Russian ЙЦУКЕН (Cyrillic)
///   - Arabic (standard)
///
/// Dead key sequences (accent composition) are supported for European layouts.
use core::sync::atomic::{AtomicU8, Ordering};

// ─── Layout IDs ──────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum LayoutId {
    UsQwerty = 0,
    UkQwerty = 1,
    DeQwertz = 2,
    FrAzerty = 3,
    EsQwerty = 4,
    PtQwerty = 5,
    JpRomaji = 6,
    KoDubeolsik = 7,
    RuJcuken = 8,
    ArStandard = 9,
}

impl LayoutId {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => LayoutId::UsQwerty,
            1 => LayoutId::UkQwerty,
            2 => LayoutId::DeQwertz,
            3 => LayoutId::FrAzerty,
            4 => LayoutId::EsQwerty,
            5 => LayoutId::PtQwerty,
            6 => LayoutId::JpRomaji,
            7 => LayoutId::KoDubeolsik,
            8 => LayoutId::RuJcuken,
            9 => LayoutId::ArStandard,
            _ => LayoutId::UsQwerty,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            LayoutId::UsQwerty => "US English (QWERTY)",
            LayoutId::UkQwerty => "UK English (QWERTY)",
            LayoutId::DeQwertz => "German (QWERTZ)",
            LayoutId::FrAzerty => "French (AZERTY)",
            LayoutId::EsQwerty => "Spanish (QWERTY)",
            LayoutId::PtQwerty => "Portuguese (QWERTY)",
            LayoutId::JpRomaji => "Japanese (Romaji)",
            LayoutId::KoDubeolsik => "Korean (Dubeolsik)",
            LayoutId::RuJcuken => "Russian (ЙЦУКЕН)",
            LayoutId::ArStandard => "Arabic (Standard)",
        }
    }

    pub fn short_code(self) -> &'static str {
        match self {
            LayoutId::UsQwerty => "US",
            LayoutId::UkQwerty => "UK",
            LayoutId::DeQwertz => "DE",
            LayoutId::FrAzerty => "FR",
            LayoutId::EsQwerty => "ES",
            LayoutId::PtQwerty => "PT",
            LayoutId::JpRomaji => "JP",
            LayoutId::KoDubeolsik => "KO",
            LayoutId::RuJcuken => "RU",
            LayoutId::ArStandard => "AR",
        }
    }

    pub fn count() -> u8 {
        10
    }
}

// ─── Global State ────────────────────────────────────────────────────

static ACTIVE_LAYOUT: AtomicU8 = AtomicU8::new(0);

/// Dead-key state for accent composition
static DEAD_KEY: spin::Mutex<Option<char>> = spin::Mutex::new(None);

/// Get the current layout
pub fn active_layout() -> LayoutId {
    LayoutId::from_u8(ACTIVE_LAYOUT.load(Ordering::Relaxed))
}

/// Set the keyboard layout
pub fn set_layout(layout: LayoutId) {
    ACTIVE_LAYOUT.store(layout as u8, Ordering::Relaxed);
    *DEAD_KEY.lock() = None;
}

/// Cycle to the next layout (e.g. on Alt+Shift)
pub fn cycle_layout() {
    let cur = ACTIVE_LAYOUT.load(Ordering::Relaxed);
    let next = (cur + 1) % LayoutId::count();
    ACTIVE_LAYOUT.store(next, Ordering::Relaxed);
    *DEAD_KEY.lock() = None;
}

/// Get all available layout IDs
pub fn all_layouts() -> [LayoutId; 10] {
    [
        LayoutId::UsQwerty,
        LayoutId::UkQwerty,
        LayoutId::DeQwertz,
        LayoutId::FrAzerty,
        LayoutId::EsQwerty,
        LayoutId::PtQwerty,
        LayoutId::JpRomaji,
        LayoutId::KoDubeolsik,
        LayoutId::RuJcuken,
        LayoutId::ArStandard,
    ]
}

// ─── Translate Key Code ──────────────────────────────────────────────

/// Translate a physical key code to a character.
/// Returns `Some(char)` for printable keys, `None` for modifiers/function keys.
pub fn translate(key: KeyCode, shift: bool, caps: bool, alt_gr: bool) -> Option<char> {
    let layout = active_layout();
    let raw = map_key(layout, key, shift, caps, alt_gr);

    // Dead key composition
    if let Some(ch) = raw {
        let mut dk = DEAD_KEY.lock();
        if is_dead_key(ch) {
            *dk = Some(ch);
            return None; // swallow dead key
        }
        if let Some(dead) = dk.take() {
            return Some(compose_dead(dead, ch));
        }
        return Some(ch);
    }

    raw
}

fn is_dead_key(ch: char) -> bool {
    matches!(
        ch,
        '\u{0300}' | '\u{0301}' | '\u{0302}' | '\u{0308}' | '\u{0327}' | '\u{0303}'
    )
}

fn compose_dead(dead: char, base: char) -> char {
    match (dead, base) {
        ('\u{0300}', 'a') => '\u{00E0}',
        ('\u{0300}', 'e') => '\u{00E8}',
        ('\u{0300}', 'i') => '\u{00EC}',
        ('\u{0300}', 'o') => '\u{00F2}',
        ('\u{0300}', 'u') => '\u{00F9}',
        ('\u{0300}', 'A') => '\u{00C0}',
        ('\u{0300}', 'E') => '\u{00C8}',
        ('\u{0301}', 'a') => '\u{00E1}',
        ('\u{0301}', 'e') => '\u{00E9}',
        ('\u{0301}', 'i') => '\u{00ED}',
        ('\u{0301}', 'o') => '\u{00F3}',
        ('\u{0301}', 'u') => '\u{00FA}',
        ('\u{0301}', 'A') => '\u{00C1}',
        ('\u{0301}', 'E') => '\u{00C9}',
        ('\u{0301}', 'I') => '\u{00CD}',
        ('\u{0301}', 'O') => '\u{00D3}',
        ('\u{0301}', 'U') => '\u{00DA}',
        ('\u{0302}', 'a') => '\u{00E2}',
        ('\u{0302}', 'e') => '\u{00EA}',
        ('\u{0302}', 'i') => '\u{00EE}',
        ('\u{0302}', 'o') => '\u{00F4}',
        ('\u{0302}', 'u') => '\u{00FB}',
        ('\u{0302}', 'A') => '\u{00C2}',
        ('\u{0302}', 'E') => '\u{00CA}',
        ('\u{0308}', 'a') => '\u{00E4}',
        ('\u{0308}', 'e') => '\u{00EB}',
        ('\u{0308}', 'i') => '\u{00EF}',
        ('\u{0308}', 'o') => '\u{00F6}',
        ('\u{0308}', 'u') => '\u{00FC}',
        ('\u{0308}', 'A') => '\u{00C4}',
        ('\u{0308}', 'O') => '\u{00D6}',
        ('\u{0308}', 'U') => '\u{00DC}',
        ('\u{0327}', 'c') => '\u{00E7}',
        ('\u{0327}', 'C') => '\u{00C7}',
        ('\u{0303}', 'n') => '\u{00F1}',
        ('\u{0303}', 'N') => '\u{00D1}',
        ('\u{0303}', 'a') => '\u{00E3}',
        ('\u{0303}', 'o') => '\u{00F5}',
        ('\u{0303}', 'A') => '\u{00C3}',
        ('\u{0303}', 'O') => '\u{00D5}',
        _ => base,
    }
}

// ─── Key Mapping per Layout ──────────────────────────────────────────

fn map_key(layout: LayoutId, key: KeyCode, shift: bool, caps: bool, alt_gr: bool) -> Option<char> {
    match layout {
        LayoutId::UsQwerty | LayoutId::JpRomaji | LayoutId::KoDubeolsik => map_us(key, shift, caps),
        LayoutId::UkQwerty => map_uk(key, shift, caps, alt_gr),
        LayoutId::DeQwertz => map_de(key, shift, caps, alt_gr),
        LayoutId::FrAzerty => map_fr(key, shift, caps, alt_gr),
        LayoutId::EsQwerty => map_es(key, shift, caps, alt_gr),
        LayoutId::PtQwerty => map_pt(key, shift, caps, alt_gr),
        LayoutId::RuJcuken => map_ru(key, shift, caps),
        LayoutId::ArStandard => map_ar(key, shift, caps),
    }
}

/// US QWERTY
fn map_us(key: KeyCode, shift: bool, caps: bool) -> Option<char> {
    let upper = shift ^ caps;
    match key {
        KeyCode::A => Some(if upper { 'A' } else { 'a' }),
        KeyCode::B => Some(if upper { 'B' } else { 'b' }),
        KeyCode::C => Some(if upper { 'C' } else { 'c' }),
        KeyCode::D => Some(if upper { 'D' } else { 'd' }),
        KeyCode::E => Some(if upper { 'E' } else { 'e' }),
        KeyCode::F => Some(if upper { 'F' } else { 'f' }),
        KeyCode::G => Some(if upper { 'G' } else { 'g' }),
        KeyCode::H => Some(if upper { 'H' } else { 'h' }),
        KeyCode::I => Some(if upper { 'I' } else { 'i' }),
        KeyCode::J => Some(if upper { 'J' } else { 'j' }),
        KeyCode::K => Some(if upper { 'K' } else { 'k' }),
        KeyCode::L => Some(if upper { 'L' } else { 'l' }),
        KeyCode::M => Some(if upper { 'M' } else { 'm' }),
        KeyCode::N => Some(if upper { 'N' } else { 'n' }),
        KeyCode::O => Some(if upper { 'O' } else { 'o' }),
        KeyCode::P => Some(if upper { 'P' } else { 'p' }),
        KeyCode::Q => Some(if upper { 'Q' } else { 'q' }),
        KeyCode::R => Some(if upper { 'R' } else { 'r' }),
        KeyCode::S => Some(if upper { 'S' } else { 's' }),
        KeyCode::T => Some(if upper { 'T' } else { 't' }),
        KeyCode::U => Some(if upper { 'U' } else { 'u' }),
        KeyCode::V => Some(if upper { 'V' } else { 'v' }),
        KeyCode::W => Some(if upper { 'W' } else { 'w' }),
        KeyCode::X => Some(if upper { 'X' } else { 'x' }),
        KeyCode::Y => Some(if upper { 'Y' } else { 'y' }),
        KeyCode::Z => Some(if upper { 'Z' } else { 'z' }),

        KeyCode::Digit1 => Some(if shift { '!' } else { '1' }),
        KeyCode::Digit2 => Some(if shift { '@' } else { '2' }),
        KeyCode::Digit3 => Some(if shift { '#' } else { '3' }),
        KeyCode::Digit4 => Some(if shift { '$' } else { '4' }),
        KeyCode::Digit5 => Some(if shift { '%' } else { '5' }),
        KeyCode::Digit6 => Some(if shift { '^' } else { '6' }),
        KeyCode::Digit7 => Some(if shift { '&' } else { '7' }),
        KeyCode::Digit8 => Some(if shift { '*' } else { '8' }),
        KeyCode::Digit9 => Some(if shift { '(' } else { '9' }),
        KeyCode::Digit0 => Some(if shift { ')' } else { '0' }),

        KeyCode::Minus => Some(if shift { '_' } else { '-' }),
        KeyCode::Equal => Some(if shift { '+' } else { '=' }),
        KeyCode::BracketLeft => Some(if shift { '{' } else { '[' }),
        KeyCode::BracketRight => Some(if shift { '}' } else { ']' }),
        KeyCode::Backslash => Some(if shift { '|' } else { '\\' }),
        KeyCode::Semicolon => Some(if shift { ':' } else { ';' }),
        KeyCode::Quote => Some(if shift { '"' } else { '\'' }),
        KeyCode::Backquote => Some(if shift { '~' } else { '`' }),
        KeyCode::Comma => Some(if shift { '<' } else { ',' }),
        KeyCode::Period => Some(if shift { '>' } else { '.' }),
        KeyCode::Slash => Some(if shift { '?' } else { '/' }),

        KeyCode::Space => Some(' '),
        KeyCode::Tab => Some('\t'),

        _ => None,
    }
}

/// UK QWERTY
fn map_uk(key: KeyCode, shift: bool, caps: bool, alt_gr: bool) -> Option<char> {
    match key {
        KeyCode::Digit2 if alt_gr => Some('\u{20AC}'), // €
        KeyCode::Digit2 => Some(if shift { '"' } else { '2' }),
        KeyCode::Digit3 => Some(if shift { '\u{00A3}' } else { '3' }), // £
        KeyCode::Quote => Some(if shift { '@' } else { '\'' }),
        KeyCode::Backslash => Some(if shift { '~' } else { '#' }),
        KeyCode::Backquote => Some(if shift { '\u{00AC}' } else { '`' }), // ¬
        _ => {
            let _ = alt_gr;
            map_us(key, shift, caps)
        }
    }
}

/// German QWERTZ
fn map_de(key: KeyCode, shift: bool, caps: bool, alt_gr: bool) -> Option<char> {
    let upper = shift ^ caps;
    match key {
        KeyCode::Y => Some(if upper { 'Z' } else { 'z' }),
        KeyCode::Z => Some(if upper { 'Y' } else { 'y' }),

        KeyCode::Digit2 if alt_gr => Some('\u{00B2}'), // ²
        KeyCode::Digit3 if alt_gr => Some('\u{00B3}'), // ³
        KeyCode::Digit7 if alt_gr => Some('{'),
        KeyCode::Digit8 if alt_gr => Some('['),
        KeyCode::Digit9 if alt_gr => Some(']'),
        KeyCode::Digit0 if alt_gr => Some('}'),
        KeyCode::Minus if alt_gr => Some('\\'),

        KeyCode::Digit2 => Some(if shift { '"' } else { '2' }),
        KeyCode::Digit3 => Some(if shift { '\u{00A7}' } else { '3' }), // §
        KeyCode::Digit6 => Some(if shift { '&' } else { '6' }),
        KeyCode::Digit7 => Some(if shift { '/' } else { '7' }),
        KeyCode::Digit8 => Some(if shift { '(' } else { '8' }),
        KeyCode::Digit9 => Some(if shift { ')' } else { '9' }),
        KeyCode::Digit0 => Some(if shift { '=' } else { '0' }),

        KeyCode::Minus => Some(if shift { '?' } else { '\u{00DF}' }), // ß
        KeyCode::Equal => Some(if shift { '`' } else { '\u{0301}' }), // dead acute
        KeyCode::BracketLeft => Some(if upper { '\u{00DC}' } else { '\u{00FC}' }), // Ü/ü
        KeyCode::BracketRight if alt_gr => Some('~'),
        KeyCode::BracketRight => Some(if shift { '*' } else { '+' }),
        KeyCode::Semicolon => Some(if upper { '\u{00D6}' } else { '\u{00F6}' }), // Ö/ö
        KeyCode::Quote => Some(if upper { '\u{00C4}' } else { '\u{00E4}' }),     // Ä/ä
        KeyCode::Backquote => Some(if shift { '\u{00B0}' } else { '\u{0302}' }), // ° / dead circumflex
        KeyCode::Backslash => Some(if shift { '\'' } else { '#' }),
        KeyCode::Comma => Some(if shift { ';' } else { ',' }),
        KeyCode::Period => Some(if shift { ':' } else { '.' }),
        KeyCode::Slash => Some(if shift { '_' } else { '-' }),

        _ => {
            let _ = alt_gr;
            map_us(key, shift, caps)
        }
    }
}

/// French AZERTY
fn map_fr(key: KeyCode, shift: bool, caps: bool, alt_gr: bool) -> Option<char> {
    let upper = shift ^ caps;
    match key {
        KeyCode::A => Some(if upper { 'Q' } else { 'q' }),
        KeyCode::Q => Some(if upper { 'A' } else { 'a' }),
        KeyCode::W => Some(if upper { 'Z' } else { 'z' }),
        KeyCode::Z => Some(if upper { 'W' } else { 'w' }),
        KeyCode::M => Some(if shift { '?' } else { ',' }),

        KeyCode::Digit1 => Some(if shift { '1' } else { '&' }),
        KeyCode::Digit2 if alt_gr => Some('~'),
        KeyCode::Digit2 => Some(if shift { '2' } else { '\u{00E9}' }), // é
        KeyCode::Digit3 if alt_gr => Some('#'),
        KeyCode::Digit3 => Some(if shift { '3' } else { '"' }),
        KeyCode::Digit4 if alt_gr => Some('{'),
        KeyCode::Digit4 => Some(if shift { '4' } else { '\'' }),
        KeyCode::Digit5 if alt_gr => Some('['),
        KeyCode::Digit5 => Some(if shift { '5' } else { '(' }),
        KeyCode::Digit6 if alt_gr => Some('|'),
        KeyCode::Digit6 => Some(if shift { '6' } else { '-' }),
        KeyCode::Digit7 if alt_gr => Some('`'),
        KeyCode::Digit7 => Some(if shift { '7' } else { '\u{00E8}' }), // è
        KeyCode::Digit8 if alt_gr => Some('\\'),
        KeyCode::Digit8 => Some(if shift { '8' } else { '_' }),
        KeyCode::Digit9 if alt_gr => Some('^'),
        KeyCode::Digit9 => Some(if shift { '9' } else { '\u{00E7}' }), // ç
        KeyCode::Digit0 if alt_gr => Some('@'),
        KeyCode::Digit0 => Some(if shift { '0' } else { '\u{00E0}' }), // à

        KeyCode::Minus if alt_gr => Some(']'),
        KeyCode::Minus => Some(if shift { '\u{00B0}' } else { ')' }), // °
        KeyCode::Equal if alt_gr => Some('}'),
        KeyCode::Equal => Some(if shift { '+' } else { '=' }),

        KeyCode::Semicolon => Some(if upper { 'M' } else { 'm' }),
        KeyCode::Quote => Some(if shift { '%' } else { '\u{00F9}' }), // ù
        KeyCode::Backquote => Some(if shift { '~' } else { '\u{00B2}' }), // ²
        KeyCode::Backslash => Some(if shift { '\u{00B5}' } else { '*' }), // µ
        KeyCode::Comma => Some(if shift { '.' } else { ';' }),
        KeyCode::Period => Some(if shift { '/' } else { ':' }),
        KeyCode::Slash => Some(if shift { '\u{00A7}' } else { '!' }), // §

        _ => {
            let _ = alt_gr;
            let _ = caps;
            map_us(key, shift, caps)
        }
    }
}

/// Spanish QWERTY
fn map_es(key: KeyCode, shift: bool, caps: bool, alt_gr: bool) -> Option<char> {
    let upper = shift ^ caps;
    match key {
        KeyCode::Quote => Some(if shift { '\u{0308}' } else { '\u{0301}' }), // dead diaeresis/acute
        KeyCode::BracketLeft => Some(if shift { '\u{0302}' } else { '\u{0300}' }), // dead circumflex/grave
        KeyCode::Semicolon => Some(if upper { '\u{00D1}' } else { '\u{00F1}' }),   // Ñ/ñ
        KeyCode::Minus => Some(if shift { '_' } else { '\'' }),
        KeyCode::Equal => Some(if shift { '\u{00BF}' } else { '\u{00A1}' }), // ¿/¡
        KeyCode::Backquote => Some(if shift { '\u{00AA}' } else { '\u{00BA}' }), // ª/º
        KeyCode::Slash => Some(if shift { '_' } else { '-' }),
        _ => {
            let _ = alt_gr;
            map_us(key, shift, caps)
        }
    }
}

/// Portuguese QWERTY
fn map_pt(key: KeyCode, shift: bool, caps: bool, alt_gr: bool) -> Option<char> {
    let upper = shift ^ caps;
    match key {
        KeyCode::Quote => Some(if shift { '\u{0300}' } else { '\u{0301}' }), // dead grave/acute
        KeyCode::BracketLeft => Some(if shift { '*' } else { '+' }),
        KeyCode::BracketRight => Some(if shift { '`' } else { '\u{0327}' }), // dead cedilla
        KeyCode::Semicolon => Some(if upper { '\u{00C7}' } else { '\u{00E7}' }), // Ç/ç
        KeyCode::Backquote => Some(if shift { '|' } else { '\\' }),
        KeyCode::Backslash => Some(if shift { '\u{00BB}' } else { '\u{00AB}' }), // »/«
        KeyCode::Minus => Some(if shift { '?' } else { '\'' }),
        KeyCode::Slash => Some(if shift { '_' } else { '-' }),
        _ => {
            let _ = alt_gr;
            map_us(key, shift, caps)
        }
    }
}

/// Russian ЙЦУКЕН
fn map_ru(key: KeyCode, shift: bool, caps: bool) -> Option<char> {
    let upper = shift ^ caps;
    let (lo, hi) = match key {
        KeyCode::Q => ('\u{0439}', '\u{0419}'),            // й Й
        KeyCode::W => ('\u{0446}', '\u{0426}'),            // ц Ц
        KeyCode::E => ('\u{0443}', '\u{0423}'),            // у У
        KeyCode::R => ('\u{043A}', '\u{041A}'),            // к К
        KeyCode::T => ('\u{0435}', '\u{0415}'),            // е Е
        KeyCode::Y => ('\u{043D}', '\u{041D}'),            // н Н
        KeyCode::U => ('\u{0433}', '\u{0413}'),            // г Г
        KeyCode::I => ('\u{0448}', '\u{0428}'),            // ш Ш
        KeyCode::O => ('\u{0449}', '\u{0429}'),            // щ Щ
        KeyCode::P => ('\u{0437}', '\u{0417}'),            // з З
        KeyCode::BracketLeft => ('\u{0445}', '\u{0425}'),  // х Х
        KeyCode::BracketRight => ('\u{044A}', '\u{042A}'), // ъ Ъ
        KeyCode::A => ('\u{0444}', '\u{0424}'),            // ф Ф
        KeyCode::S => ('\u{044B}', '\u{042B}'),            // ы Ы
        KeyCode::D => ('\u{0432}', '\u{0412}'),            // в В
        KeyCode::F => ('\u{0430}', '\u{0410}'),            // а А
        KeyCode::G => ('\u{043F}', '\u{041F}'),            // п П
        KeyCode::H => ('\u{0440}', '\u{0420}'),            // р Р
        KeyCode::J => ('\u{043E}', '\u{041E}'),            // о О
        KeyCode::K => ('\u{043B}', '\u{041B}'),            // л Л
        KeyCode::L => ('\u{0434}', '\u{0414}'),            // д Д
        KeyCode::Semicolon => ('\u{0436}', '\u{0416}'),    // ж Ж
        KeyCode::Quote => ('\u{044D}', '\u{042D}'),        // э Э
        KeyCode::Z => ('\u{044F}', '\u{042F}'),            // я Я
        KeyCode::X => ('\u{0447}', '\u{0427}'),            // ч Ч
        KeyCode::C => ('\u{0441}', '\u{0421}'),            // с С
        KeyCode::V => ('\u{043C}', '\u{041C}'),            // м М
        KeyCode::B => ('\u{0438}', '\u{0418}'),            // и И
        KeyCode::N => ('\u{0442}', '\u{0422}'),            // т Т
        KeyCode::M => ('\u{044C}', '\u{042C}'),            // ь Ь
        KeyCode::Comma => ('\u{0431}', '\u{0411}'),        // б Б
        KeyCode::Period => ('\u{044E}', '\u{042E}'),       // ю Ю
        KeyCode::Backquote => ('\u{0451}', '\u{0401}'),    // ё Ё

        KeyCode::Digit1 => return Some(if shift { '!' } else { '1' }),
        KeyCode::Digit2 => return Some(if shift { '"' } else { '2' }),
        KeyCode::Digit3 => return Some(if shift { '\u{2116}' } else { '3' }), // №
        KeyCode::Digit4 => return Some(if shift { ';' } else { '4' }),
        KeyCode::Digit5 => return Some(if shift { '%' } else { '5' }),
        KeyCode::Digit6 => return Some(if shift { ':' } else { '6' }),
        KeyCode::Digit7 => return Some(if shift { '?' } else { '7' }),
        KeyCode::Digit8 => return Some(if shift { '*' } else { '8' }),
        KeyCode::Digit9 => return Some(if shift { '(' } else { '9' }),
        KeyCode::Digit0 => return Some(if shift { ')' } else { '0' }),

        KeyCode::Minus => return Some(if shift { '_' } else { '-' }),
        KeyCode::Equal => return Some(if shift { '+' } else { '=' }),
        KeyCode::Slash => return Some(if shift { ',' } else { '.' }),

        KeyCode::Space => return Some(' '),
        KeyCode::Tab => return Some('\t'),
        _ => return None,
    };
    Some(if upper { hi } else { lo })
}

/// Arabic standard
fn map_ar(key: KeyCode, shift: bool, caps: bool) -> Option<char> {
    let upper = shift ^ caps;
    let (lo, hi) = match key {
        KeyCode::Q => ('\u{0636}', '\u{064E}'), // ض  َ
        KeyCode::W => ('\u{0635}', '\u{064B}'), // ص  ً
        KeyCode::E => ('\u{062B}', '\u{064F}'), // ث  ُ
        KeyCode::R => ('\u{0642}', '\u{064C}'), // ق  ٌ
        KeyCode::T => ('\u{0641}', '\u{0625}'), // ف إ
        KeyCode::Y => ('\u{063A}', '\u{0625}'), // غ إ
        KeyCode::U => ('\u{0639}', '\u{2018}'), // ع '
        KeyCode::I => ('\u{0647}', '\u{00F7}'), // ه ÷
        KeyCode::O => ('\u{062E}', '\u{00D7}'), // خ ×
        KeyCode::P => ('\u{062D}', '\u{061B}'), // ح ؛
        KeyCode::BracketLeft => ('\u{062C}', '<'),
        KeyCode::BracketRight => ('\u{062F}', '>'),
        KeyCode::A => ('\u{0634}', '\u{0650}'),     // ش  ِ
        KeyCode::S => ('\u{0633}', '\u{064D}'),     // س  ٍ
        KeyCode::D => ('\u{064A}', ']'),            // ي
        KeyCode::F => ('\u{0628}', '['),            // ب
        KeyCode::G => ('\u{0644}', '\u{0623}'),     // ل أ
        KeyCode::H => ('\u{0627}', '\u{0623}'),     // ا أ
        KeyCode::J => ('\u{062A}', '\u{0640}'),     // ت ـ
        KeyCode::K => ('\u{0646}', '\u{060C}'),     // ن ،
        KeyCode::L => ('\u{0645}', '/'),            // م
        KeyCode::Semicolon => ('\u{0643}', ':'),    // ك
        KeyCode::Quote => ('\u{0637}', '"'),        // ط
        KeyCode::Z => ('\u{0626}', '~'),            // ئ
        KeyCode::X => ('\u{0621}', '\u{0652}'),     // ء  ْ
        KeyCode::C => ('\u{0624}', '{'),            // ؤ
        KeyCode::V => ('\u{0631}', '}'),            // ر
        KeyCode::B => ('\u{0644}', '\u{0622}'),     // ل آ  (simplified: lam)
        KeyCode::N => ('\u{0649}', '\u{0622}'),     // ى آ
        KeyCode::M => ('\u{0629}', '\u{2018}'),     // ة '
        KeyCode::Comma => ('\u{0648}', ','),        // و
        KeyCode::Period => ('\u{0632}', '.'),       // ز
        KeyCode::Slash => ('\u{0638}', '\u{061F}'), // ظ ؟

        KeyCode::Digit1 => return Some(if shift { '!' } else { '1' }),
        KeyCode::Digit2 => return Some(if shift { '@' } else { '2' }),
        KeyCode::Digit3 => return Some(if shift { '#' } else { '3' }),
        KeyCode::Digit4 => return Some(if shift { '$' } else { '4' }),
        KeyCode::Digit5 => return Some(if shift { '%' } else { '5' }),
        KeyCode::Digit6 => return Some(if shift { '^' } else { '6' }),
        KeyCode::Digit7 => return Some(if shift { '&' } else { '7' }),
        KeyCode::Digit8 => return Some(if shift { '*' } else { '8' }),
        KeyCode::Digit9 => return Some(if shift { '(' } else { '9' }),
        KeyCode::Digit0 => return Some(if shift { ')' } else { '0' }),

        KeyCode::Space => return Some(' '),
        KeyCode::Tab => return Some('\t'),
        _ => return None,
    };
    Some(if upper { hi } else { lo })
}
