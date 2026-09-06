/// Locale & Language Settings
///
/// Provides internationalization (i18n) support for the KnoxOS GUI:
///   - Locale identifiers (language + region)
///   - Translated UI strings for all built-in panels
///   - Number, date, and time formatting based on locale
///   - RTL detection per locale
///
/// Ships with English (US), English (UK), German, French, Spanish,
/// Portuguese, Japanese, Korean, Russian, and Arabic translations.
extern crate alloc;
use alloc::string::String;
use core::sync::atomic::{AtomicU8, Ordering};

// ─── Locale IDs ──────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum LocaleId {
    EnUs = 0,
    EnGb = 1,
    DeDe = 2,
    FrFr = 3,
    EsEs = 4,
    PtBr = 5,
    JaJp = 6,
    KoKr = 7,
    RuRu = 8,
    ArSa = 9,
}

impl LocaleId {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => LocaleId::EnUs,
            1 => LocaleId::EnGb,
            2 => LocaleId::DeDe,
            3 => LocaleId::FrFr,
            4 => LocaleId::EsEs,
            5 => LocaleId::PtBr,
            6 => LocaleId::JaJp,
            7 => LocaleId::KoKr,
            8 => LocaleId::RuRu,
            9 => LocaleId::ArSa,
            _ => LocaleId::EnUs,
        }
    }

    /// BCP 47 language tag
    pub fn tag(self) -> &'static str {
        match self {
            LocaleId::EnUs => "en-US",
            LocaleId::EnGb => "en-GB",
            LocaleId::DeDe => "de-DE",
            LocaleId::FrFr => "fr-FR",
            LocaleId::EsEs => "es-ES",
            LocaleId::PtBr => "pt-BR",
            LocaleId::JaJp => "ja-JP",
            LocaleId::KoKr => "ko-KR",
            LocaleId::RuRu => "ru-RU",
            LocaleId::ArSa => "ar-SA",
        }
    }

    /// Native language name for display
    pub fn native_name(self) -> &'static str {
        match self {
            LocaleId::EnUs => "English (US)",
            LocaleId::EnGb => "English (UK)",
            LocaleId::DeDe => "Deutsch",
            LocaleId::FrFr => "Français",
            LocaleId::EsEs => "Español",
            LocaleId::PtBr => "Português (Brasil)",
            LocaleId::JaJp => "日本語",
            LocaleId::KoKr => "한국어",
            LocaleId::RuRu => "Русский",
            LocaleId::ArSa => "العربية",
        }
    }

    /// Is this locale written right-to-left?
    pub fn is_rtl(self) -> bool {
        matches!(self, LocaleId::ArSa)
    }

    /// Thousands separator
    pub fn thousands_sep(self) -> char {
        match self {
            LocaleId::DeDe | LocaleId::FrFr | LocaleId::PtBr | LocaleId::RuRu => '.',
            LocaleId::EsEs => '.',
            _ => ',',
        }
    }

    /// Decimal separator
    pub fn decimal_sep(self) -> char {
        match self {
            LocaleId::DeDe | LocaleId::FrFr | LocaleId::PtBr | LocaleId::RuRu | LocaleId::EsEs => {
                ','
            }
            _ => '.',
        }
    }

    /// Date format pattern (D=day, M=month, Y=year)
    pub fn date_format(self) -> &'static str {
        match self {
            LocaleId::EnUs => "MM/DD/YYYY",
            LocaleId::EnGb
            | LocaleId::DeDe
            | LocaleId::FrFr
            | LocaleId::EsEs
            | LocaleId::PtBr
            | LocaleId::RuRu
            | LocaleId::ArSa => "DD/MM/YYYY",
            LocaleId::JaJp | LocaleId::KoKr => "YYYY/MM/DD",
        }
    }

    /// Does this locale use 24-hour time?
    pub fn use_24h(self) -> bool {
        !matches!(self, LocaleId::EnUs | LocaleId::EnGb)
    }

    pub fn count() -> u8 {
        10
    }
}

// ─── Global State ────────────────────────────────────────────────────

static ACTIVE_LOCALE: AtomicU8 = AtomicU8::new(0);

/// Get the current locale
pub fn active_locale() -> LocaleId {
    LocaleId::from_u8(ACTIVE_LOCALE.load(Ordering::Relaxed))
}

/// Set the active locale
pub fn set_locale(locale: LocaleId) {
    ACTIVE_LOCALE.store(locale as u8, Ordering::Relaxed);
}

/// Get all available locales
pub fn all_locales() -> [LocaleId; 10] {
    [
        LocaleId::EnUs,
        LocaleId::EnGb,
        LocaleId::DeDe,
        LocaleId::FrFr,
        LocaleId::EsEs,
        LocaleId::PtBr,
        LocaleId::JaJp,
        LocaleId::KoKr,
        LocaleId::RuRu,
        LocaleId::ArSa,
    ]
}

// ─── Translation Keys ────────────────────────────────────────────────

/// Well-known UI string identifiers used across all panels
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UiString {
    // Desktop & system
    Desktop,
    Settings,
    FileExplorer,
    Terminal,
    TextEditor,
    Browser,
    AppStore,
    Shutdown,
    Restart,
    LogOut,
    LockScreen,
    Search,

    // Common actions
    Ok,
    Cancel,
    Apply,
    Save,
    Open,
    Close,
    Delete,
    Rename,
    Copy,
    Paste,
    Cut,
    Undo,
    Redo,
    SelectAll,

    // Settings panels
    Appearance,
    Network,
    Sound,
    Display,
    Keyboard,
    Mouse,
    Users,
    About,
    Language,
    Theme,
    Wallpaper,

    // File explorer
    Home,
    Documents,
    Downloads,
    Pictures,
    Music,
    Videos,
    Trash,
    NewFolder,
    NewFile,

    // System info
    CpuUsage,
    MemoryUsage,
    DiskUsage,
    Uptime,
}

/// Get the translated string for a UI element
pub fn tr(key: UiString) -> &'static str {
    let locale = active_locale();
    translate(locale, key)
}

fn translate(locale: LocaleId, key: UiString) -> &'static str {
    match locale {
        LocaleId::EnUs | LocaleId::EnGb => en(key),
        LocaleId::DeDe => de(key),
        LocaleId::FrFr => fr(key),
        LocaleId::EsEs => es(key),
        LocaleId::PtBr => pt(key),
        LocaleId::JaJp => ja(key),
        LocaleId::KoKr => ko(key),
        LocaleId::RuRu => ru(key),
        LocaleId::ArSa => ar(key),
    }
}

// ─── Translation Tables ──────────────────────────────────────────────

fn en(key: UiString) -> &'static str {
    match key {
        UiString::Desktop => "Desktop",
        UiString::Settings => "Settings",
        UiString::FileExplorer => "File Explorer",
        UiString::Terminal => "Terminal",
        UiString::TextEditor => "Text Editor",
        UiString::Browser => "Browser",
        UiString::AppStore => "App Store",
        UiString::Shutdown => "Shut Down",
        UiString::Restart => "Restart",
        UiString::LogOut => "Log Out",
        UiString::LockScreen => "Lock Screen",
        UiString::Search => "Search",
        UiString::Ok => "OK",
        UiString::Cancel => "Cancel",
        UiString::Apply => "Apply",
        UiString::Save => "Save",
        UiString::Open => "Open",
        UiString::Close => "Close",
        UiString::Delete => "Delete",
        UiString::Rename => "Rename",
        UiString::Copy => "Copy",
        UiString::Paste => "Paste",
        UiString::Cut => "Cut",
        UiString::Undo => "Undo",
        UiString::Redo => "Redo",
        UiString::SelectAll => "Select All",
        UiString::Appearance => "Appearance",
        UiString::Network => "Network",
        UiString::Sound => "Sound",
        UiString::Display => "Display",
        UiString::Keyboard => "Keyboard",
        UiString::Mouse => "Mouse",
        UiString::Users => "Users",
        UiString::About => "About",
        UiString::Language => "Language",
        UiString::Theme => "Theme",
        UiString::Wallpaper => "Wallpaper",
        UiString::Home => "Home",
        UiString::Documents => "Documents",
        UiString::Downloads => "Downloads",
        UiString::Pictures => "Pictures",
        UiString::Music => "Music",
        UiString::Videos => "Videos",
        UiString::Trash => "Trash",
        UiString::NewFolder => "New Folder",
        UiString::NewFile => "New File",
        UiString::CpuUsage => "CPU Usage",
        UiString::MemoryUsage => "Memory Usage",
        UiString::DiskUsage => "Disk Usage",
        UiString::Uptime => "Uptime",
    }
}

fn de(key: UiString) -> &'static str {
    match key {
        UiString::Desktop => "Schreibtisch",
        UiString::Settings => "Einstellungen",
        UiString::FileExplorer => "Dateimanager",
        UiString::Terminal => "Terminal",
        UiString::TextEditor => "Texteditor",
        UiString::Browser => "Browser",
        UiString::AppStore => "App Store",
        UiString::Shutdown => "Herunterfahren",
        UiString::Restart => "Neustart",
        UiString::LogOut => "Abmelden",
        UiString::LockScreen => "Bildschirm sperren",
        UiString::Search => "Suchen",
        UiString::Ok => "OK",
        UiString::Cancel => "Abbrechen",
        UiString::Apply => "Anwenden",
        UiString::Save => "Speichern",
        UiString::Open => "Öffnen",
        UiString::Close => "Schließen",
        UiString::Delete => "Löschen",
        UiString::Rename => "Umbenennen",
        UiString::Copy => "Kopieren",
        UiString::Paste => "Einfügen",
        UiString::Cut => "Ausschneiden",
        UiString::Undo => "Rückgängig",
        UiString::Redo => "Wiederholen",
        UiString::SelectAll => "Alles auswählen",
        UiString::Appearance => "Erscheinungsbild",
        UiString::Network => "Netzwerk",
        UiString::Sound => "Ton",
        UiString::Display => "Anzeige",
        UiString::Keyboard => "Tastatur",
        UiString::Mouse => "Maus",
        UiString::Users => "Benutzer",
        UiString::About => "Über",
        UiString::Language => "Sprache",
        UiString::Theme => "Design",
        UiString::Wallpaper => "Hintergrundbild",
        UiString::Home => "Startseite",
        UiString::Documents => "Dokumente",
        UiString::Downloads => "Downloads",
        UiString::Pictures => "Bilder",
        UiString::Music => "Musik",
        UiString::Videos => "Videos",
        UiString::Trash => "Papierkorb",
        UiString::NewFolder => "Neuer Ordner",
        UiString::NewFile => "Neue Datei",
        UiString::CpuUsage => "CPU-Auslastung",
        UiString::MemoryUsage => "Speichernutzung",
        UiString::DiskUsage => "Festplattennutzung",
        UiString::Uptime => "Betriebszeit",
    }
}

fn fr(key: UiString) -> &'static str {
    match key {
        UiString::Desktop => "Bureau",
        UiString::Settings => "Paramètres",
        UiString::FileExplorer => "Explorateur de fichiers",
        UiString::Terminal => "Terminal",
        UiString::TextEditor => "Éditeur de texte",
        UiString::Browser => "Navigateur",
        UiString::AppStore => "App Store",
        UiString::Shutdown => "Éteindre",
        UiString::Restart => "Redémarrer",
        UiString::LogOut => "Déconnexion",
        UiString::LockScreen => "Verrouiller l'écran",
        UiString::Search => "Rechercher",
        UiString::Ok => "OK",
        UiString::Cancel => "Annuler",
        UiString::Apply => "Appliquer",
        UiString::Save => "Enregistrer",
        UiString::Open => "Ouvrir",
        UiString::Close => "Fermer",
        UiString::Delete => "Supprimer",
        UiString::Rename => "Renommer",
        UiString::Copy => "Copier",
        UiString::Paste => "Coller",
        UiString::Cut => "Couper",
        UiString::Undo => "Annuler",
        UiString::Redo => "Rétablir",
        UiString::SelectAll => "Tout sélectionner",
        UiString::Appearance => "Apparence",
        UiString::Network => "Réseau",
        UiString::Sound => "Son",
        UiString::Display => "Affichage",
        UiString::Keyboard => "Clavier",
        UiString::Mouse => "Souris",
        UiString::Users => "Utilisateurs",
        UiString::About => "À propos",
        UiString::Language => "Langue",
        UiString::Theme => "Thème",
        UiString::Wallpaper => "Fond d'écran",
        UiString::Home => "Accueil",
        UiString::Documents => "Documents",
        UiString::Downloads => "Téléchargements",
        UiString::Pictures => "Images",
        UiString::Music => "Musique",
        UiString::Videos => "Vidéos",
        UiString::Trash => "Corbeille",
        UiString::NewFolder => "Nouveau dossier",
        UiString::NewFile => "Nouveau fichier",
        UiString::CpuUsage => "Utilisation CPU",
        UiString::MemoryUsage => "Utilisation mémoire",
        UiString::DiskUsage => "Utilisation disque",
        UiString::Uptime => "Temps de fonctionnement",
    }
}

fn es(key: UiString) -> &'static str {
    match key {
        UiString::Desktop => "Escritorio",
        UiString::Settings => "Configuración",
        UiString::FileExplorer => "Explorador de archivos",
        UiString::Terminal => "Terminal",
        UiString::TextEditor => "Editor de texto",
        UiString::Browser => "Navegador",
        UiString::AppStore => "Tienda de apps",
        UiString::Shutdown => "Apagar",
        UiString::Restart => "Reiniciar",
        UiString::LogOut => "Cerrar sesión",
        UiString::LockScreen => "Bloquear pantalla",
        UiString::Search => "Buscar",
        UiString::Ok => "Aceptar",
        UiString::Cancel => "Cancelar",
        UiString::Apply => "Aplicar",
        UiString::Save => "Guardar",
        UiString::Open => "Abrir",
        UiString::Close => "Cerrar",
        UiString::Delete => "Eliminar",
        UiString::Rename => "Renombrar",
        UiString::Copy => "Copiar",
        UiString::Paste => "Pegar",
        UiString::Cut => "Cortar",
        UiString::Undo => "Deshacer",
        UiString::Redo => "Rehacer",
        UiString::SelectAll => "Seleccionar todo",
        UiString::Appearance => "Apariencia",
        UiString::Network => "Red",
        UiString::Sound => "Sonido",
        UiString::Display => "Pantalla",
        UiString::Keyboard => "Teclado",
        UiString::Mouse => "Ratón",
        UiString::Users => "Usuarios",
        UiString::About => "Acerca de",
        UiString::Language => "Idioma",
        UiString::Theme => "Tema",
        UiString::Wallpaper => "Fondo de pantalla",
        UiString::Home => "Inicio",
        UiString::Documents => "Documentos",
        UiString::Downloads => "Descargas",
        UiString::Pictures => "Imágenes",
        UiString::Music => "Música",
        UiString::Videos => "Vídeos",
        UiString::Trash => "Papelera",
        UiString::NewFolder => "Nueva carpeta",
        UiString::NewFile => "Nuevo archivo",
        UiString::CpuUsage => "Uso de CPU",
        UiString::MemoryUsage => "Uso de memoria",
        UiString::DiskUsage => "Uso de disco",
        UiString::Uptime => "Tiempo activo",
    }
}

fn pt(key: UiString) -> &'static str {
    match key {
        UiString::Desktop => "Área de trabalho",
        UiString::Settings => "Configurações",
        UiString::FileExplorer => "Explorador de arquivos",
        UiString::Terminal => "Terminal",
        UiString::TextEditor => "Editor de texto",
        UiString::Browser => "Navegador",
        UiString::AppStore => "Loja de apps",
        UiString::Shutdown => "Desligar",
        UiString::Restart => "Reiniciar",
        UiString::LogOut => "Encerrar sessão",
        UiString::LockScreen => "Bloquear tela",
        UiString::Search => "Pesquisar",
        UiString::Ok => "OK",
        UiString::Cancel => "Cancelar",
        UiString::Apply => "Aplicar",
        UiString::Save => "Salvar",
        UiString::Open => "Abrir",
        UiString::Close => "Fechar",
        UiString::Delete => "Excluir",
        UiString::Rename => "Renomear",
        UiString::Copy => "Copiar",
        UiString::Paste => "Colar",
        UiString::Cut => "Recortar",
        UiString::Undo => "Desfazer",
        UiString::Redo => "Refazer",
        UiString::SelectAll => "Selecionar tudo",
        UiString::Appearance => "Aparência",
        UiString::Network => "Rede",
        UiString::Sound => "Som",
        UiString::Display => "Tela",
        UiString::Keyboard => "Teclado",
        UiString::Mouse => "Mouse",
        UiString::Users => "Usuários",
        UiString::About => "Sobre",
        UiString::Language => "Idioma",
        UiString::Theme => "Tema",
        UiString::Wallpaper => "Papel de parede",
        UiString::Home => "Início",
        UiString::Documents => "Documentos",
        UiString::Downloads => "Downloads",
        UiString::Pictures => "Imagens",
        UiString::Music => "Música",
        UiString::Videos => "Vídeos",
        UiString::Trash => "Lixeira",
        UiString::NewFolder => "Nova pasta",
        UiString::NewFile => "Novo arquivo",
        UiString::CpuUsage => "Uso da CPU",
        UiString::MemoryUsage => "Uso de memória",
        UiString::DiskUsage => "Uso de disco",
        UiString::Uptime => "Tempo ativo",
    }
}

fn ja(key: UiString) -> &'static str {
    match key {
        UiString::Desktop => "デスクトップ",
        UiString::Settings => "設定",
        UiString::FileExplorer => "ファイルマネージャ",
        UiString::Terminal => "ターミナル",
        UiString::TextEditor => "テキストエディタ",
        UiString::Browser => "ブラウザ",
        UiString::AppStore => "アプリストア",
        UiString::Shutdown => "シャットダウン",
        UiString::Restart => "再起動",
        UiString::LogOut => "ログアウト",
        UiString::LockScreen => "画面ロック",
        UiString::Search => "検索",
        UiString::Ok => "OK",
        UiString::Cancel => "キャンセル",
        UiString::Apply => "適用",
        UiString::Save => "保存",
        UiString::Open => "開く",
        UiString::Close => "閉じる",
        UiString::Delete => "削除",
        UiString::Rename => "名前変更",
        UiString::Copy => "コピー",
        UiString::Paste => "貼り付け",
        UiString::Cut => "切り取り",
        UiString::Undo => "元に戻す",
        UiString::Redo => "やり直し",
        UiString::SelectAll => "すべて選択",
        UiString::Appearance => "外観",
        UiString::Network => "ネットワーク",
        UiString::Sound => "サウンド",
        UiString::Display => "ディスプレイ",
        UiString::Keyboard => "キーボード",
        UiString::Mouse => "マウス",
        UiString::Users => "ユーザー",
        UiString::About => "このPCについて",
        UiString::Language => "言語",
        UiString::Theme => "テーマ",
        UiString::Wallpaper => "壁紙",
        UiString::Home => "ホーム",
        UiString::Documents => "書類",
        UiString::Downloads => "ダウンロード",
        UiString::Pictures => "写真",
        UiString::Music => "ミュージック",
        UiString::Videos => "ビデオ",
        UiString::Trash => "ゴミ箱",
        UiString::NewFolder => "新規フォルダ",
        UiString::NewFile => "新規ファイル",
        UiString::CpuUsage => "CPU使用率",
        UiString::MemoryUsage => "メモリ使用率",
        UiString::DiskUsage => "ディスク使用率",
        UiString::Uptime => "稼働時間",
    }
}

fn ko(key: UiString) -> &'static str {
    match key {
        UiString::Desktop => "바탕화면",
        UiString::Settings => "설정",
        UiString::FileExplorer => "파일 탐색기",
        UiString::Terminal => "터미널",
        UiString::TextEditor => "텍스트 편집기",
        UiString::Browser => "브라우저",
        UiString::AppStore => "앱 스토어",
        UiString::Shutdown => "시스템 종료",
        UiString::Restart => "다시 시작",
        UiString::LogOut => "로그아웃",
        UiString::LockScreen => "화면 잠금",
        UiString::Search => "검색",
        UiString::Ok => "확인",
        UiString::Cancel => "취소",
        UiString::Apply => "적용",
        UiString::Save => "저장",
        UiString::Open => "열기",
        UiString::Close => "닫기",
        UiString::Delete => "삭제",
        UiString::Rename => "이름 바꾸기",
        UiString::Copy => "복사",
        UiString::Paste => "붙여넣기",
        UiString::Cut => "잘라내기",
        UiString::Undo => "실행 취소",
        UiString::Redo => "다시 실행",
        UiString::SelectAll => "모두 선택",
        UiString::Appearance => "모양",
        UiString::Network => "네트워크",
        UiString::Sound => "소리",
        UiString::Display => "디스플레이",
        UiString::Keyboard => "키보드",
        UiString::Mouse => "마우스",
        UiString::Users => "사용자",
        UiString::About => "정보",
        UiString::Language => "언어",
        UiString::Theme => "테마",
        UiString::Wallpaper => "배경화면",
        UiString::Home => "홈",
        UiString::Documents => "문서",
        UiString::Downloads => "다운로드",
        UiString::Pictures => "사진",
        UiString::Music => "음악",
        UiString::Videos => "비디오",
        UiString::Trash => "휴지통",
        UiString::NewFolder => "새 폴더",
        UiString::NewFile => "새 파일",
        UiString::CpuUsage => "CPU 사용량",
        UiString::MemoryUsage => "메모리 사용량",
        UiString::DiskUsage => "디스크 사용량",
        UiString::Uptime => "가동 시간",
    }
}

fn ru(key: UiString) -> &'static str {
    match key {
        UiString::Desktop => "Рабочий стол",
        UiString::Settings => "Настройки",
        UiString::FileExplorer => "Файловый менеджер",
        UiString::Terminal => "Терминал",
        UiString::TextEditor => "Текстовый редактор",
        UiString::Browser => "Браузер",
        UiString::AppStore => "Магазин приложений",
        UiString::Shutdown => "Выключить",
        UiString::Restart => "Перезагрузка",
        UiString::LogOut => "Выйти",
        UiString::LockScreen => "Заблокировать",
        UiString::Search => "Поиск",
        UiString::Ok => "ОК",
        UiString::Cancel => "Отмена",
        UiString::Apply => "Применить",
        UiString::Save => "Сохранить",
        UiString::Open => "Открыть",
        UiString::Close => "Закрыть",
        UiString::Delete => "Удалить",
        UiString::Rename => "Переименовать",
        UiString::Copy => "Копировать",
        UiString::Paste => "Вставить",
        UiString::Cut => "Вырезать",
        UiString::Undo => "Отменить",
        UiString::Redo => "Повторить",
        UiString::SelectAll => "Выделить всё",
        UiString::Appearance => "Внешний вид",
        UiString::Network => "Сеть",
        UiString::Sound => "Звук",
        UiString::Display => "Экран",
        UiString::Keyboard => "Клавиатура",
        UiString::Mouse => "Мышь",
        UiString::Users => "Пользователи",
        UiString::About => "О системе",
        UiString::Language => "Язык",
        UiString::Theme => "Тема",
        UiString::Wallpaper => "Обои",
        UiString::Home => "Домой",
        UiString::Documents => "Документы",
        UiString::Downloads => "Загрузки",
        UiString::Pictures => "Изображения",
        UiString::Music => "Музыка",
        UiString::Videos => "Видео",
        UiString::Trash => "Корзина",
        UiString::NewFolder => "Новая папка",
        UiString::NewFile => "Новый файл",
        UiString::CpuUsage => "Загрузка ЦП",
        UiString::MemoryUsage => "Использование памяти",
        UiString::DiskUsage => "Использование диска",
        UiString::Uptime => "Время работы",
    }
}

fn ar(key: UiString) -> &'static str {
    match key {
        UiString::Desktop => "سطح المكتب",
        UiString::Settings => "الإعدادات",
        UiString::FileExplorer => "مدير الملفات",
        UiString::Terminal => "الطرفية",
        UiString::TextEditor => "محرر النصوص",
        UiString::Browser => "المتصفح",
        UiString::AppStore => "متجر التطبيقات",
        UiString::Shutdown => "إيقاف التشغيل",
        UiString::Restart => "إعادة التشغيل",
        UiString::LogOut => "تسجيل الخروج",
        UiString::LockScreen => "قفل الشاشة",
        UiString::Search => "بحث",
        UiString::Ok => "موافق",
        UiString::Cancel => "إلغاء",
        UiString::Apply => "تطبيق",
        UiString::Save => "حفظ",
        UiString::Open => "فتح",
        UiString::Close => "إغلاق",
        UiString::Delete => "حذف",
        UiString::Rename => "إعادة تسمية",
        UiString::Copy => "نسخ",
        UiString::Paste => "لصق",
        UiString::Cut => "قص",
        UiString::Undo => "تراجع",
        UiString::Redo => "إعادة",
        UiString::SelectAll => "تحديد الكل",
        UiString::Appearance => "المظهر",
        UiString::Network => "الشبكة",
        UiString::Sound => "الصوت",
        UiString::Display => "الشاشة",
        UiString::Keyboard => "لوحة المفاتيح",
        UiString::Mouse => "الفأرة",
        UiString::Users => "المستخدمون",
        UiString::About => "حول",
        UiString::Language => "اللغة",
        UiString::Theme => "السمة",
        UiString::Wallpaper => "خلفية الشاشة",
        UiString::Home => "الرئيسية",
        UiString::Documents => "المستندات",
        UiString::Downloads => "التنزيلات",
        UiString::Pictures => "الصور",
        UiString::Music => "الموسيقى",
        UiString::Videos => "الفيديو",
        UiString::Trash => "سلة المحذوفات",
        UiString::NewFolder => "مجلد جديد",
        UiString::NewFile => "ملف جديد",
        UiString::CpuUsage => "استخدام المعالج",
        UiString::MemoryUsage => "استخدام الذاكرة",
        UiString::DiskUsage => "استخدام القرص",
        UiString::Uptime => "وقت التشغيل",
    }
}

// ─── Number / Date Formatting ────────────────────────────────────────

/// Format an integer with locale-appropriate thousands separator
pub fn format_number(n: u64) -> String {
    let locale = active_locale();
    let sep = locale.thousands_sep();

    let s = alloc::format!("{}", n);
    if s.len() <= 3 {
        return s;
    }

    let mut out = String::with_capacity(s.len() + s.len() / 3);
    let bytes = s.as_bytes();
    let rem = bytes.len() % 3;

    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (i - rem) % 3 == 0 && (i != 0 || rem != 0) {
            if i == rem && rem == 0 {
                // skip
            } else {
                out.push(sep);
            }
        }
        out.push(b as char);
    }
    out
}

/// Format a date (day, month, year) according to locale
pub fn format_date(day: u8, month: u8, year: u16) -> String {
    let locale = active_locale();
    match locale.date_format() {
        "MM/DD/YYYY" => alloc::format!("{:02}/{:02}/{:04}", month, day, year),
        "YYYY/MM/DD" => alloc::format!("{:04}/{:02}/{:02}", year, month, day),
        _ => alloc::format!("{:02}/{:02}/{:04}", day, month, year), // DD/MM/YYYY
    }
}

/// Format a time (hour, minute) according to locale
pub fn format_time(hour: u8, minute: u8) -> String {
    let locale = active_locale();
    if locale.use_24h() {
        alloc::format!("{:02}:{:02}", hour, minute)
    } else {
        let (h12, ampm) = if hour == 0 {
            (12, "AM")
        } else if hour < 12 {
            (hour, "AM")
        } else if hour == 12 {
            (12, "PM")
        } else {
            (hour - 12, "PM")
        };
        alloc::format!("{}:{:02} {}", h12, minute, ampm)
    }
}
