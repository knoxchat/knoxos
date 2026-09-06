use crate::serial_println;
/// Gettext / ICU Translation Framework
///
/// Provides message translation (i18n) using gettext-compatible .mo files.
/// Supports plurals, context disambiguation, and format string localization.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Plural form rule
#[derive(Debug, Clone)]
pub struct PluralRule {
    pub nplurals: u8,
    pub expression: String, // e.g. "n != 1" for English
}

/// A loaded translation catalog
pub struct Catalog {
    pub domain: String,
    pub locale: String,
    pub plural_rule: PluralRule,
    translations: BTreeMap<String, Vec<String>>, // msgid → [msgstr, msgstr_plural...]
    context_translations: BTreeMap<(String, String), Vec<String>>, // (context, msgid)
}

lazy_static::lazy_static! {
    static ref CATALOGS: Mutex<Vec<Catalog>> = Mutex::new(Vec::new());
    static ref ACTIVE_LOCALE: Mutex<String> = Mutex::new(String::from("en_US"));
}

impl Catalog {
    pub fn new(domain: &str, locale: &str) -> Self {
        Self {
            domain: String::from(domain),
            locale: String::from(locale),
            plural_rule: PluralRule {
                nplurals: 2,
                expression: String::from("n != 1"),
            },
            translations: BTreeMap::new(),
            context_translations: BTreeMap::new(),
        }
    }

    /// Load from .mo file binary data
    pub fn load_mo(&mut self, data: &[u8]) -> Result<(), &'static str> {
        if data.len() < 28 {
            return Err("Too short");
        }
        let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let le = magic == 0x950412DE;
        let be = magic == 0xDE120495;
        if !le && !be {
            return Err("Invalid .mo magic");
        }

        let read_u32 = |offset: usize| -> u32 {
            let b = [
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ];
            if le {
                u32::from_le_bytes(b)
            } else {
                u32::from_be_bytes(b)
            }
        };

        let nstrings = read_u32(8) as usize;
        let orig_table = read_u32(12) as usize;
        let trans_table = read_u32(16) as usize;

        for i in 0..nstrings {
            let orig_len = read_u32(orig_table + i * 8) as usize;
            let orig_off = read_u32(orig_table + i * 8 + 4) as usize;
            let trans_len = read_u32(trans_table + i * 8) as usize;
            let trans_off = read_u32(trans_table + i * 8 + 4) as usize;

            if orig_off + orig_len > data.len() || trans_off + trans_len > data.len() {
                continue;
            }

            let orig = core::str::from_utf8(&data[orig_off..orig_off + orig_len]).unwrap_or("");
            let trans = core::str::from_utf8(&data[trans_off..trans_off + trans_len]).unwrap_or("");

            if orig.is_empty() {
                // Header entry — parse plural-forms
                for line in trans.lines() {
                    if line.starts_with("Plural-Forms:") {
                        if let Some(np) = line.find("nplurals=") {
                            let rest = &line[np + 9..];
                            if let Some(semi) = rest.find(';') {
                                self.plural_rule.nplurals = rest[..semi].parse().unwrap_or(2);
                            }
                        }
                    }
                }
                continue;
            }

            // Handle context (msgctxt\x04msgid)
            if let Some(sep) = orig.find('\x04') {
                let ctx = String::from(&orig[..sep]);
                let msgid = String::from(&orig[sep + 1..]);
                let forms: Vec<String> = trans.split('\0').map(String::from).collect();
                self.context_translations.insert((ctx, msgid), forms);
            } else {
                let forms: Vec<String> = trans.split('\0').map(String::from).collect();
                self.translations.insert(String::from(orig), forms);
            }
        }

        serial_println!(
            "[GETTEXT] Loaded {} translations for {}/{}",
            nstrings,
            self.locale,
            self.domain
        );
        Ok(())
    }

    /// Translate a message
    pub fn gettext<'a>(&'a self, msgid: &'a str) -> &'a str {
        if let Some(forms) = self.translations.get(msgid) {
            if let Some(s) = forms.first() {
                return s;
            }
        }
        msgid
    }

    /// Translate with plural form
    pub fn ngettext<'a>(&'a self, msgid: &'a str, msgid_plural: &'a str, n: u64) -> &'a str {
        if let Some(forms) = self.translations.get(msgid) {
            let idx = self.eval_plural(n);
            if let Some(s) = forms.get(idx) {
                return s;
            }
        }
        if n == 1 { msgid } else { msgid_plural }
    }

    /// Translate with context
    pub fn pgettext<'a>(&'a self, context: &str, msgid: &'a str) -> &'a str {
        let key = (String::from(context), String::from(msgid));
        if let Some(forms) = self.context_translations.get(&key) {
            if let Some(s) = forms.first() {
                return s;
            }
        }
        msgid
    }

    fn eval_plural(&self, n: u64) -> usize {
        // Simple English rule: n != 1 → 1
        if n != 1 {
            1.min(self.plural_rule.nplurals as usize - 1)
        } else {
            0
        }
    }
}

/// Convenience: translate in the active locale
pub fn translate(domain: &str, msgid: &str) -> String {
    let locale = ACTIVE_LOCALE.lock().clone();
    let catalogs = CATALOGS.lock();
    for cat in catalogs.iter() {
        if cat.domain == domain && cat.locale == locale {
            return String::from(cat.gettext(msgid));
        }
    }
    String::from(msgid)
}

pub fn set_locale(locale: &str) {
    *ACTIVE_LOCALE.lock() = String::from(locale);
}

pub fn load_catalog(catalog: Catalog) {
    CATALOGS.lock().push(catalog);
}

pub fn init() {
    serial_println!("[GETTEXT] Translation framework loaded");
}
