/// Internationalization (i18n) — Localization framework for KnoxOS
///
/// Provides:
///   - gettext/ICU message translation framework
///   - Multiple keyboard layout switching with indicator
///   - IME (Input Method Editor) UI for CJK composition
///   - Date/time format localization
///   - Number/currency format localization
///   - RTL UI mirroring (full layout flip)
///   - Translation files for built-in apps
///   - Language pack download and install
///   - Spell checker integration
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

// ═══════════════════════════════════════════════════════════════════════
// gettext / ICU Message Translation Framework
// ═══════════════════════════════════════════════════════════════════════

/// Translation catalog for one locale/domain
#[derive(Debug, Clone)]
pub struct TranslationCatalog {
    pub locale: String,
    pub domain: String,
    pub messages: BTreeMap<String, String>,
    pub plural_forms: String,
}

lazy_static::lazy_static! {
    static ref CATALOGS: Mutex<Vec<TranslationCatalog>> = Mutex::new(Vec::new());
    static ref CURRENT_LOCALE: Mutex<String> = Mutex::new(String::from("en_US"));
    static ref CURRENT_DOMAIN: Mutex<String> = Mutex::new(String::from("knoxos"));
}

/// Set current locale
pub fn set_locale(locale: &str) {
    *CURRENT_LOCALE.lock() = String::from(locale);
    crate::serial_println!("[i18n] Locale set to '{}'", locale);
}

/// Get current locale
pub fn get_locale() -> String {
    CURRENT_LOCALE.lock().clone()
}

/// Set text domain for translations
pub fn textdomain(domain: &str) {
    *CURRENT_DOMAIN.lock() = String::from(domain);
}

/// Load a .mo/.po translation catalog
pub fn load_catalog(locale: &str, domain: &str, data: &[(&str, &str)]) {
    let mut messages = BTreeMap::new();
    for (msgid, msgstr) in data {
        messages.insert(String::from(*msgid), String::from(*msgstr));
    }
    CATALOGS.lock().push(TranslationCatalog {
        locale: String::from(locale),
        domain: String::from(domain),
        messages,
        plural_forms: String::from("nplurals=2; plural=(n != 1);"),
    });
    crate::serial_println!(
        "[i18n] Loaded catalog: locale={}, domain={}",
        locale,
        domain
    );
}

/// Translate a message (gettext)
pub fn gettext(msgid: &str) -> String {
    let locale = CURRENT_LOCALE.lock().clone();
    let domain = CURRENT_DOMAIN.lock().clone();
    let catalogs = CATALOGS.lock();
    for cat in catalogs.iter() {
        if cat.locale == locale && cat.domain == domain {
            if let Some(msg) = cat.messages.get(msgid) {
                return msg.clone();
            }
        }
    }
    String::from(msgid) // Fallback to original string
}

/// Translate with domain (dgettext)
pub fn dgettext(domain: &str, msgid: &str) -> String {
    let locale = CURRENT_LOCALE.lock().clone();
    let catalogs = CATALOGS.lock();
    for cat in catalogs.iter() {
        if cat.locale == locale && cat.domain == domain {
            if let Some(msg) = cat.messages.get(msgid) {
                return msg.clone();
            }
        }
    }
    String::from(msgid)
}

// ═══════════════════════════════════════════════════════════════════════
// Multiple Keyboard Layout Switching
// ═══════════════════════════════════════════════════════════════════════

/// Keyboard layout
#[derive(Debug, Clone)]
pub struct KeyboardLayout {
    pub name: String, // e.g., "us", "de", "fr"
    pub description: String,
    pub variant: String, // e.g., "dvorak", "colemak"
}

lazy_static::lazy_static! {
    static ref KEYBOARD_LAYOUTS: Mutex<Vec<KeyboardLayout>> = Mutex::new(Vec::new());
    static ref ACTIVE_LAYOUT_IDX: Mutex<usize> = Mutex::new(0);
}

/// Add a keyboard layout
pub fn add_keyboard_layout(name: &str, description: &str, variant: &str) {
    KEYBOARD_LAYOUTS.lock().push(KeyboardLayout {
        name: String::from(name),
        description: String::from(description),
        variant: String::from(variant),
    });
}

/// Switch to next keyboard layout (cycles)
pub fn switch_keyboard_layout() -> String {
    let layouts = KEYBOARD_LAYOUTS.lock();
    if layouts.is_empty() {
        return String::from("us");
    }
    let mut idx = ACTIVE_LAYOUT_IDX.lock();
    *idx = (*idx + 1) % layouts.len();
    let name = layouts[*idx].name.clone();
    crate::serial_println!("[i18n] Keyboard layout switched to '{}'", name);
    name
}

/// Get current layout name (for indicator)
pub fn current_keyboard_layout() -> String {
    let layouts = KEYBOARD_LAYOUTS.lock();
    let idx = *ACTIVE_LAYOUT_IDX.lock();
    layouts
        .get(idx)
        .map(|l| l.name.clone())
        .unwrap_or_else(|| String::from("us"))
}

/// List available layouts
pub fn list_keyboard_layouts() -> Vec<KeyboardLayout> {
    KEYBOARD_LAYOUTS.lock().clone()
}

// ═══════════════════════════════════════════════════════════════════════
// IME (Input Method Editor) UI for CJK Composition
// ═══════════════════════════════════════════════════════════════════════

/// IME state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImeState {
    Inactive,
    Composing,
    Selecting,
}

/// IME candidate
#[derive(Debug, Clone)]
pub struct ImeCandidate {
    pub text: String,
    pub reading: String,
    pub frequency: u32,
}

/// Input Method Editor
pub struct InputMethodEditor {
    pub state: ImeState,
    pub preedit: String, // Composing text
    pub candidates: Vec<ImeCandidate>,
    pub selected_idx: usize,
    pub language: String, // "zh", "ja", "ko"
}

lazy_static::lazy_static! {
    static ref IME: Mutex<InputMethodEditor> = Mutex::new(InputMethodEditor {
        state: ImeState::Inactive,
        preedit: String::new(),
        candidates: Vec::new(),
        selected_idx: 0,
        language: String::new(),
    });
}

/// Activate IME for a language
pub fn ime_activate(language: &str) {
    let mut ime = IME.lock();
    ime.state = ImeState::Composing;
    ime.language = String::from(language);
    ime.preedit.clear();
    ime.candidates.clear();
    crate::serial_println!("[IME] Activated for '{}'", language);
}

/// Feed a keypress to the IME
pub fn ime_key_input(ch: char) -> Option<String> {
    let mut ime = IME.lock();
    if ime.state == ImeState::Inactive {
        return None;
    }
    ime.preedit.push(ch);
    // In real implementation: look up dictionary for candidates
    ime.candidates.clear();
    let preedit_text = ime.preedit.clone();
    ime.candidates.push(ImeCandidate {
        text: preedit_text.clone(),
        reading: preedit_text,
        frequency: 100,
    });
    ime.state = ImeState::Selecting;
    None // Not yet committed
}

/// Commit the selected candidate
pub fn ime_commit() -> Option<String> {
    let mut ime = IME.lock();
    if ime.state != ImeState::Selecting {
        return None;
    }
    let text = ime.candidates.get(ime.selected_idx).map(|c| c.text.clone());
    ime.preedit.clear();
    ime.candidates.clear();
    ime.state = ImeState::Composing;
    text
}

/// Deactivate IME
pub fn ime_deactivate() {
    let mut ime = IME.lock();
    ime.state = ImeState::Inactive;
    ime.preedit.clear();
}

// ═══════════════════════════════════════════════════════════════════════
// Date/Time Format Localization
// ═══════════════════════════════════════════════════════════════════════

/// Localized date format
pub fn format_date(year: u32, month: u8, day: u8) -> String {
    let locale = get_locale();
    if locale.starts_with("en_US") {
        alloc::format!("{:02}/{:02}/{}", month, day, year)
    } else if locale.starts_with("de") || locale.starts_with("fr") {
        alloc::format!("{:02}.{:02}.{}", day, month, year)
    } else if locale.starts_with("ja") || locale.starts_with("zh") || locale.starts_with("ko") {
        alloc::format!("{}/{:02}/{:02}", year, month, day)
    } else if locale.starts_with("en_GB") {
        alloc::format!("{:02}/{:02}/{}", day, month, year)
    } else {
        alloc::format!("{}-{:02}-{:02}", year, month, day) // ISO 8601 fallback
    }
}

/// Localized time format
pub fn format_time(hour: u8, minute: u8, second: u8) -> String {
    let locale = get_locale();
    if locale.starts_with("en_US") {
        let (h12, ampm) = if hour == 0 {
            (12, "AM")
        } else if hour < 12 {
            (hour, "AM")
        } else if hour == 12 {
            (12, "PM")
        } else {
            (hour - 12, "PM")
        };
        alloc::format!("{}:{:02}:{:02} {}", h12, minute, second, ampm)
    } else {
        alloc::format!("{:02}:{:02}:{:02}", hour, minute, second)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Number/Currency Format Localization
// ═══════════════════════════════════════════════════════════════════════

/// Format a number with locale-appropriate separators
pub fn format_number(value: i64) -> String {
    let locale = get_locale();
    let (thousands_sep, _decimal_sep) = if locale.starts_with("de")
        || locale.starts_with("fr")
        || locale.starts_with("es")
        || locale.starts_with("pt")
    {
        ('.', ',')
    } else {
        (',', '.')
    };
    let abs_val = if value < 0 { -value } else { value } as u64;
    let s = alloc::format!("{}", abs_val);
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(thousands_sep);
        }
        result.push(ch);
    }
    if value < 0 {
        result.push('-');
    }
    result.chars().rev().collect()
}

/// Format currency
pub fn format_currency(amount: i64, cents: u8) -> String {
    let locale = get_locale();
    let num = format_number(amount);
    let (decimal_sep, symbol, before) = if locale.starts_with("en_US") {
        ('.', "$", true)
    } else if locale.starts_with("en_GB") {
        ('.', "£", true)
    } else if locale.starts_with("de") || locale.starts_with("fr") || locale.starts_with("es") {
        (',', "€", false)
    } else if locale.starts_with("ja") {
        ('.', "¥", true)
    } else {
        ('.', "$", true)
    };
    if before {
        alloc::format!("{}{}{}{:02}", symbol, num, decimal_sep, cents)
    } else {
        alloc::format!("{}{}{:02} {}", num, decimal_sep, cents, symbol)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// RTL UI Mirroring (full layout flip)
// ═══════════════════════════════════════════════════════════════════════

/// Check if current locale is RTL
pub fn is_rtl() -> bool {
    let locale = get_locale();
    locale.starts_with("ar")
        || locale.starts_with("he")
        || locale.starts_with("fa")
        || locale.starts_with("ur")
}

/// Mirror an X coordinate for RTL layout
pub fn mirror_x(x: i32, width: u32) -> i32 {
    if is_rtl() { width as i32 - x } else { x }
}

/// Mirror a rectangle's X position for RTL layout (full layout flip)
pub fn mirror_rect_x(x: i32, rect_width: u32, container_width: u32) -> i32 {
    if is_rtl() {
        container_width as i32 - x - rect_width as i32
    } else {
        x
    }
}

/// Mirror a horizontal layout — given child positions arranged LTR,
/// flip them for RTL within the parent container width.
pub fn mirror_layout_h(positions: &mut [(i32, u32)], container_width: u32) {
    if !is_rtl() {
        return;
    }
    for (x, w) in positions.iter_mut() {
        *x = container_width as i32 - *x - *w as i32;
    }
}

/// Mirror text alignment for RTL
pub fn rtl_align(align: TextAlign) -> TextAlign {
    if !is_rtl() {
        return align;
    }
    match align {
        TextAlign::Left => TextAlign::Right,
        TextAlign::Right => TextAlign::Left,
        TextAlign::Center => TextAlign::Center,
    }
}

/// Mirror scroll direction for RTL
pub fn rtl_scroll_delta(dx: i32) -> i32 {
    if is_rtl() { -dx } else { dx }
}

/// Get reading direction for current locale
pub fn reading_direction() -> LayoutDirection {
    if is_rtl() {
        LayoutDirection::RightToLeft
    } else {
        LayoutDirection::LeftToRight
    }
}

/// Layout direction enum used by the UI framework
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutDirection {
    LeftToRight,
    RightToLeft,
}

/// Mirror a margin/padding specification for RTL
pub fn mirror_margins(left: i32, right: i32) -> (i32, i32) {
    if is_rtl() {
        (right, left)
    } else {
        (left, right)
    }
}

/// Mirror taskbar/dock position for RTL (left↔right)
pub fn mirror_dock_position(pos: &str) -> &str {
    if !is_rtl() {
        return pos;
    }
    match pos {
        "left" => "right",
        "right" => "left",
        _ => pos,
    }
}

/// Mirror a list of flex children positions for RTL layout.
/// Each child is (x, width). After mirroring the children appear
/// in the same visual order but anchored from the right.
pub fn mirror_flex_row(children: &mut [(i32, u32)], container_width: u32) {
    if !is_rtl() {
        return;
    }
    // Reverse order and recalculate x offsets from right
    children.reverse();
    let mut cursor = 0i32;
    for (x, w) in children.iter_mut() {
        *x = container_width as i32 - cursor - *w as i32;
        cursor += *w as i32;
    }
}

/// Apply RTL transform to an icon position in a toolbar
pub fn rtl_icon_position(index: usize, count: usize, item_width: u32, container_width: u32) -> i32 {
    if is_rtl() {
        container_width as i32 - ((index + 1) as u32 * item_width) as i32
    } else {
        (index as u32 * item_width) as i32
    }
}

/// Text alignment
#[derive(Debug, Clone, Copy)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

// ═══════════════════════════════════════════════════════════════════════
// Translation Files for Built-in Apps
// ═══════════════════════════════════════════════════════════════════════

/// Initialize default translations for built-in apps.
/// Covers: File Manager, Terminal, Settings, Calculator, Text Editor,
/// System Monitor, Image Viewer, and common dialog strings.
pub fn load_builtin_translations() {
    // Spanish translations
    load_catalog(
        "es_ES",
        "knoxos",
        &[
            ("File", "Archivo"),
            ("Edit", "Editar"),
            ("View", "Ver"),
            ("Help", "Ayuda"),
            ("Settings", "Configuración"),
            ("Terminal", "Terminal"),
            ("Files", "Archivos"),
            ("Search", "Buscar"),
            ("Close", "Cerrar"),
            ("Open", "Abrir"),
            ("Save", "Guardar"),
            ("Cancel", "Cancelar"),
            ("Copy", "Copiar"),
            ("Paste", "Pegar"),
            ("Cut", "Cortar"),
            ("Delete", "Eliminar"),
            ("Rename", "Renombrar"),
            ("New Folder", "Nueva Carpeta"),
            ("New File", "Nuevo Archivo"),
            ("Properties", "Propiedades"),
            ("About", "Acerca de"),
            ("Shut Down", "Apagar"),
            ("Restart", "Reiniciar"),
            ("Log Out", "Cerrar Sesión"),
            ("Lock Screen", "Bloquear Pantalla"),
            ("Calculator", "Calculadora"),
            ("Text Editor", "Editor de Texto"),
            ("Image Viewer", "Visor de Imágenes"),
            ("System Monitor", "Monitor del Sistema"),
            ("Notifications", "Notificaciones"),
            ("Volume", "Volumen"),
            ("Brightness", "Brillo"),
            ("Wi-Fi", "Wi-Fi"),
            ("Bluetooth", "Bluetooth"),
            ("Battery", "Batería"),
            ("Undo", "Deshacer"),
            ("Redo", "Rehacer"),
            ("Select All", "Seleccionar Todo"),
            ("Find", "Buscar"),
            ("Replace", "Reemplazar"),
            ("Zoom In", "Acercar"),
            ("Zoom Out", "Alejar"),
            ("Yes", "Sí"),
            ("No", "No"),
            ("OK", "Aceptar"),
            ("Apply", "Aplicar"),
        ],
    );

    // German translations
    load_catalog(
        "de_DE",
        "knoxos",
        &[
            ("File", "Datei"),
            ("Edit", "Bearbeiten"),
            ("View", "Ansicht"),
            ("Help", "Hilfe"),
            ("Settings", "Einstellungen"),
            ("Terminal", "Terminal"),
            ("Files", "Dateien"),
            ("Search", "Suchen"),
            ("Close", "Schließen"),
            ("Open", "Öffnen"),
            ("Save", "Speichern"),
            ("Cancel", "Abbrechen"),
            ("Copy", "Kopieren"),
            ("Paste", "Einfügen"),
            ("Cut", "Ausschneiden"),
            ("Delete", "Löschen"),
            ("Rename", "Umbenennen"),
            ("New Folder", "Neuer Ordner"),
            ("New File", "Neue Datei"),
            ("Properties", "Eigenschaften"),
            ("About", "Über"),
            ("Shut Down", "Herunterfahren"),
            ("Restart", "Neustart"),
            ("Log Out", "Abmelden"),
            ("Lock Screen", "Bildschirm Sperren"),
            ("Calculator", "Rechner"),
            ("Text Editor", "Texteditor"),
            ("Image Viewer", "Bildbetrachter"),
            ("System Monitor", "Systemüberwachung"),
            ("Notifications", "Benachrichtigungen"),
            ("Volume", "Lautstärke"),
            ("Brightness", "Helligkeit"),
            ("Undo", "Rückgängig"),
            ("Redo", "Wiederholen"),
            ("Select All", "Alles Auswählen"),
            ("Find", "Suchen"),
            ("Replace", "Ersetzen"),
            ("Yes", "Ja"),
            ("No", "Nein"),
            ("OK", "OK"),
            ("Apply", "Anwenden"),
        ],
    );

    // French translations
    load_catalog(
        "fr_FR",
        "knoxos",
        &[
            ("File", "Fichier"),
            ("Edit", "Édition"),
            ("View", "Affichage"),
            ("Help", "Aide"),
            ("Settings", "Paramètres"),
            ("Terminal", "Terminal"),
            ("Files", "Fichiers"),
            ("Search", "Rechercher"),
            ("Close", "Fermer"),
            ("Open", "Ouvrir"),
            ("Save", "Enregistrer"),
            ("Cancel", "Annuler"),
            ("Copy", "Copier"),
            ("Paste", "Coller"),
            ("Cut", "Couper"),
            ("Delete", "Supprimer"),
            ("Rename", "Renommer"),
            ("New Folder", "Nouveau Dossier"),
            ("New File", "Nouveau Fichier"),
            ("Properties", "Propriétés"),
            ("About", "À propos"),
            ("Shut Down", "Éteindre"),
            ("Restart", "Redémarrer"),
            ("Log Out", "Déconnexion"),
            ("Lock Screen", "Verrouiller l'Écran"),
            ("Calculator", "Calculatrice"),
            ("Text Editor", "Éditeur de Texte"),
            ("Image Viewer", "Visionneuse"),
            ("System Monitor", "Moniteur Système"),
            ("Notifications", "Notifications"),
            ("Volume", "Volume"),
            ("Brightness", "Luminosité"),
            ("Undo", "Annuler"),
            ("Redo", "Rétablir"),
            ("Select All", "Tout Sélectionner"),
            ("Find", "Rechercher"),
            ("Replace", "Remplacer"),
            ("Yes", "Oui"),
            ("No", "Non"),
            ("OK", "OK"),
            ("Apply", "Appliquer"),
        ],
    );

    // Japanese translations
    load_catalog(
        "ja_JP",
        "knoxos",
        &[
            ("File", "ファイル"),
            ("Edit", "編集"),
            ("View", "表示"),
            ("Help", "ヘルプ"),
            ("Settings", "設定"),
            ("Close", "閉じる"),
            ("Open", "開く"),
            ("Save", "保存"),
            ("Cancel", "キャンセル"),
            ("Copy", "コピー"),
            ("Paste", "貼り付け"),
            ("Cut", "切り取り"),
            ("Delete", "削除"),
            ("Rename", "名前の変更"),
            ("New Folder", "新しいフォルダ"),
            ("New File", "新しいファイル"),
            ("Properties", "プロパティ"),
            ("About", "情報"),
            ("Shut Down", "シャットダウン"),
            ("Restart", "再起動"),
            ("Log Out", "ログアウト"),
            ("Lock Screen", "画面ロック"),
            ("Calculator", "電卓"),
            ("Notifications", "通知"),
            ("Search", "検索"),
            ("Undo", "元に戻す"),
            ("Redo", "やり直し"),
            ("Select All", "すべて選択"),
            ("Yes", "はい"),
            ("No", "いいえ"),
            ("OK", "OK"),
            ("Apply", "適用"),
        ],
    );

    // Arabic translations (RTL)
    load_catalog(
        "ar_SA",
        "knoxos",
        &[
            ("File", "ملف"),
            ("Edit", "تحرير"),
            ("View", "عرض"),
            ("Help", "مساعدة"),
            ("Settings", "الإعدادات"),
            ("Close", "إغلاق"),
            ("Open", "فتح"),
            ("Save", "حفظ"),
            ("Cancel", "إلغاء"),
            ("Copy", "نسخ"),
            ("Paste", "لصق"),
            ("Cut", "قص"),
            ("Delete", "حذف"),
            ("Search", "بحث"),
            ("Yes", "نعم"),
            ("No", "لا"),
            ("OK", "موافق"),
        ],
    );

    // Chinese Simplified translations
    load_catalog(
        "zh_CN",
        "knoxos",
        &[
            ("File", "文件"),
            ("Edit", "编辑"),
            ("View", "视图"),
            ("Help", "帮助"),
            ("Settings", "设置"),
            ("Close", "关闭"),
            ("Open", "打开"),
            ("Save", "保存"),
            ("Cancel", "取消"),
            ("Copy", "复制"),
            ("Paste", "粘贴"),
            ("Cut", "剪切"),
            ("Delete", "删除"),
            ("Search", "搜索"),
            ("Shut Down", "关机"),
            ("Restart", "重启"),
            ("Yes", "是"),
            ("No", "否"),
            ("OK", "确定"),
            ("Apply", "应用"),
        ],
    );

    // Korean translations
    load_catalog(
        "ko_KR",
        "knoxos",
        &[
            ("File", "파일"),
            ("Edit", "편집"),
            ("View", "보기"),
            ("Help", "도움말"),
            ("Settings", "설정"),
            ("Close", "닫기"),
            ("Open", "열기"),
            ("Save", "저장"),
            ("Cancel", "취소"),
            ("Copy", "복사"),
            ("Paste", "붙여넣기"),
            ("Delete", "삭제"),
            ("Search", "검색"),
            ("Yes", "예"),
            ("No", "아니오"),
            ("OK", "확인"),
        ],
    );

    // Portuguese translations
    load_catalog(
        "pt_BR",
        "knoxos",
        &[
            ("File", "Arquivo"),
            ("Edit", "Editar"),
            ("View", "Exibir"),
            ("Help", "Ajuda"),
            ("Settings", "Configurações"),
            ("Close", "Fechar"),
            ("Open", "Abrir"),
            ("Save", "Salvar"),
            ("Cancel", "Cancelar"),
            ("Copy", "Copiar"),
            ("Paste", "Colar"),
            ("Cut", "Recortar"),
            ("Delete", "Excluir"),
            ("Search", "Pesquisar"),
            ("Yes", "Sim"),
            ("No", "Não"),
            ("OK", "OK"),
            ("Apply", "Aplicar"),
        ],
    );

    // Register available language packs
    register_language_pack("es_ES", "Español", 320);
    register_language_pack("de_DE", "Deutsch", 310);
    register_language_pack("fr_FR", "Français", 315);
    register_language_pack("ja_JP", "日本語", 520);
    register_language_pack("ar_SA", "العربية", 290);
    register_language_pack("zh_CN", "简体中文", 480);
    register_language_pack("ko_KR", "한국어", 420);
    register_language_pack("pt_BR", "Português (Brasil)", 305);
    register_language_pack("ru_RU", "Русский", 340);
    register_language_pack("it_IT", "Italiano", 300);
    register_language_pack("nl_NL", "Nederlands", 290);
    register_language_pack("pl_PL", "Polski", 310);
    register_language_pack("tr_TR", "Türkçe", 285);
    register_language_pack("hi_IN", "हिन्दी", 350);
    register_language_pack("th_TH", "ไทย", 380);

    crate::serial_println!("[i18n] Loaded built-in translations (es, de, fr, ja, ar, zh, ko, pt)");
    crate::serial_println!("[i18n] Registered 15 language packs");
}

// ═══════════════════════════════════════════════════════════════════════
// Language Pack Download and Install
// ═══════════════════════════════════════════════════════════════════════

/// Language pack info
#[derive(Debug, Clone)]
pub struct LanguagePack {
    pub locale: String,
    pub name: String,
    pub size_kb: u32,
    pub installed: bool,
}

lazy_static::lazy_static! {
    static ref LANGUAGE_PACKS: Mutex<Vec<LanguagePack>> = Mutex::new(Vec::new());
}

/// Register available language pack
pub fn register_language_pack(locale: &str, name: &str, size_kb: u32) {
    LANGUAGE_PACKS.lock().push(LanguagePack {
        locale: String::from(locale),
        name: String::from(name),
        size_kb,
        installed: false,
    });
}

/// Install a language pack
pub fn install_language_pack(locale: &str) -> bool {
    let mut packs = LANGUAGE_PACKS.lock();
    if let Some(pack) = packs.iter_mut().find(|p| p.locale == locale) {
        pack.installed = true;
        crate::serial_println!("[i18n] Language pack '{}' installed", pack.name);
        true
    } else {
        false
    }
}

/// List language packs
pub fn list_language_packs() -> Vec<LanguagePack> {
    LANGUAGE_PACKS.lock().clone()
}

// ═══════════════════════════════════════════════════════════════════════
// Spell Checker Integration
// ═══════════════════════════════════════════════════════════════════════

/// Spell check result
#[derive(Debug, Clone)]
pub struct SpellCheckResult {
    pub word: String,
    pub correct: bool,
    pub suggestions: Vec<String>,
    pub offset: usize,
}

/// Simple spell checker (dictionary-based)
pub struct SpellChecker {
    pub dictionaries: BTreeMap<String, Vec<String>>,
    pub language: String,
}

lazy_static::lazy_static! {
    static ref SPELL_CHECKER: Mutex<SpellChecker> = Mutex::new(SpellChecker {
        dictionaries: BTreeMap::new(),
        language: String::from("en_US"),
    });
}

/// Load a dictionary for spell checking
pub fn load_dictionary(locale: &str, words: &[&str]) {
    let mut checker = SPELL_CHECKER.lock();
    let dict = checker
        .dictionaries
        .entry(String::from(locale))
        .or_default();
    for &word in words {
        dict.push(String::from(word));
    }
    crate::serial_println!("[spell] Loaded {} words for '{}'", words.len(), locale);
}

/// Check spelling of a word
pub fn spell_check_word(word: &str) -> bool {
    let checker = SPELL_CHECKER.lock();
    let lang = &checker.language;
    if let Some(dict) = checker.dictionaries.get(lang) {
        let lower = word.to_ascii_lowercase();
        dict.iter().any(|w| w.to_ascii_lowercase() == lower)
    } else {
        true // No dictionary = assume correct
    }
}

/// Suggest corrections for a misspelled word
pub fn spell_suggest(word: &str, max: usize) -> Vec<String> {
    let checker = SPELL_CHECKER.lock();
    let lang = &checker.language;
    let mut suggestions = Vec::new();
    if let Some(dict) = checker.dictionaries.get(lang) {
        let lower = word.to_ascii_lowercase();
        // Simple edit distance = 1 matcher
        for w in dict.iter() {
            let w_lower = w.to_ascii_lowercase();
            let len_diff = (w_lower.len() as i32 - lower.len() as i32).unsigned_abs() as usize;
            if len_diff <= 1 {
                // Count matching chars
                let matches = lower
                    .chars()
                    .zip(w_lower.chars())
                    .filter(|(a, b)| a == b)
                    .count();
                if matches >= lower.len().saturating_sub(2) {
                    suggestions.push(w.clone());
                    if suggestions.len() >= max {
                        break;
                    }
                }
            }
        }
    }
    suggestions
}

/// Initialize i18n subsystem
pub fn init() {
    // Set up default keyboard layout
    add_keyboard_layout("us", "English (US)", "");

    // Register default language packs
    register_language_pack("en_US", "English (US)", 0);
    register_language_pack("es_ES", "Español", 512);
    register_language_pack("de_DE", "Deutsch", 480);
    register_language_pack("fr_FR", "Français", 490);
    register_language_pack("ja_JP", "日本語", 1024);
    register_language_pack("zh_CN", "简体中文", 1200);
    register_language_pack("ko_KR", "한국어", 800);
    register_language_pack("ar_SA", "العربية", 400);
    register_language_pack("pt_BR", "Português (BR)", 470);
    register_language_pack("ru_RU", "Русский", 500);

    // Load built-in translations
    load_builtin_translations();

    crate::serial_println!("[KnoxOS] i18n subsystem initialized (gettext, IME, RTL, spell)");
}
