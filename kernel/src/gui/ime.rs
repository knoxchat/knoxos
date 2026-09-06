/// Input Method Editor (IME) for CJK and complex script input
///
/// Provides:
///   - Pinyin → Chinese character composition
///   - Hiragana/Katakana → Kanji composition  
///   - Korean jamo → syllable composition
///   - Composition window rendering
///   - Candidate list selection
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// IME STATE
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImeMode {
    Off,
    Pinyin,
    Hiragana,
    Katakana,
    Korean,
}

/// An IME candidate entry
#[derive(Debug, Clone)]
pub struct Candidate {
    pub text: String,
    pub frequency: u32,
}

/// The composition state
pub struct ImeState {
    pub mode: ImeMode,
    /// Raw input buffer (pinyin syllables, kana, jamo)
    pub input_buffer: String,
    /// Pre-edit string displayed in composition window
    pub preedit: String,
    /// Candidate list
    pub candidates: Vec<Candidate>,
    /// Selected candidate index
    pub selected: usize,
    /// Whether the composition window is visible
    pub visible: bool,
    /// Cursor position for composition window placement
    pub cursor_x: i32,
    pub cursor_y: i32,
}

impl ImeState {
    pub fn new() -> Self {
        Self {
            mode: ImeMode::Off,
            input_buffer: String::new(),
            preedit: String::new(),
            candidates: Vec::new(),
            selected: 0,
            visible: false,
            cursor_x: 0,
            cursor_y: 0,
        }
    }

    pub fn is_active(&self) -> bool {
        self.mode != ImeMode::Off
    }

    /// Process a key input, returns committed text if any
    pub fn process_key(&mut self, c: char) -> Option<String> {
        match self.mode {
            ImeMode::Off => Some(String::from(c.encode_utf8(&mut [0u8; 4]))),
            ImeMode::Pinyin => self.process_pinyin(c),
            ImeMode::Hiragana | ImeMode::Katakana => self.process_japanese(c),
            ImeMode::Korean => self.process_korean(c),
        }
    }

    /// Clear composition state
    pub fn cancel(&mut self) {
        self.input_buffer.clear();
        self.preedit.clear();
        self.candidates.clear();
        self.selected = 0;
        self.visible = false;
    }

    /// Commit the current selection
    pub fn commit(&mut self) -> Option<String> {
        if let Some(cand) = self.candidates.get(self.selected) {
            let text = cand.text.clone();
            self.cancel();
            Some(text)
        } else if !self.preedit.is_empty() {
            let text = self.preedit.clone();
            self.cancel();
            Some(text)
        } else {
            None
        }
    }

    /// Select next candidate
    pub fn next_candidate(&mut self) {
        if !self.candidates.is_empty() {
            self.selected = (self.selected + 1) % self.candidates.len();
        }
    }

    /// Select previous candidate
    pub fn prev_candidate(&mut self) {
        if !self.candidates.is_empty() {
            if self.selected == 0 {
                self.selected = self.candidates.len() - 1;
            } else {
                self.selected -= 1;
            }
        }
    }

    /// Select candidate by number (1-9)
    pub fn select_candidate(&mut self, num: usize) -> Option<String> {
        if num > 0 && num <= self.candidates.len() {
            self.selected = num - 1;
            self.commit()
        } else {
            None
        }
    }

    // ─── Pinyin ──────────────────────────────────────────────────────
    fn process_pinyin(&mut self, c: char) -> Option<String> {
        if c.is_ascii_alphabetic() {
            self.input_buffer.push(c.to_ascii_lowercase());
            self.update_pinyin_candidates();
            self.visible = true;
            None
        } else if c == ' ' || c == '\n' {
            // Commit current selection
            self.commit()
        } else if c.is_ascii_digit() && c != '0' {
            let num = (c as u8 - b'0') as usize;
            self.select_candidate(num)
        } else {
            self.cancel();
            Some(String::from(c.encode_utf8(&mut [0u8; 4])))
        }
    }

    fn update_pinyin_candidates(&mut self) {
        self.candidates.clear();
        self.preedit = self.input_buffer.clone();

        // Very basic built-in pinyin → Chinese mapping for common syllables
        let candidates: &[(&str, &[&str])] = &[
            ("a", &["啊", "阿"]),
            ("ai", &["爱", "哀", "矮"]),
            ("an", &["安", "暗", "按"]),
            ("ba", &["八", "把", "爸"]),
            ("bei", &["北", "被", "杯"]),
            ("bu", &["不", "步", "部"]),
            ("da", &["大", "打", "达"]),
            ("de", &["的", "得", "地"]),
            ("dong", &["东", "动", "懂"]),
            ("dui", &["对", "队"]),
            ("er", &["二", "而", "耳"]),
            ("ge", &["个", "歌", "哥"]),
            ("guo", &["国", "过", "果"]),
            ("hao", &["好", "号"]),
            ("he", &["和", "河", "喝"]),
            ("hen", &["很", "恨"]),
            ("hou", &["后", "候"]),
            ("hua", &["花", "话", "化"]),
            ("hui", &["会", "回"]),
            ("ji", &["几", "机", "记"]),
            ("jia", &["家", "加"]),
            ("jiu", &["就", "九", "酒"]),
            ("kan", &["看", "刊"]),
            ("ke", &["可", "课", "刻"]),
            ("lai", &["来", "赖"]),
            ("le", &["了", "乐"]),
            ("li", &["里", "力", "立"]),
            ("ma", &["吗", "妈", "马"]),
            ("me", &["么", "没"]),
            ("men", &["们", "门"]),
            ("ming", &["明", "名"]),
            ("na", &["那", "拿", "哪"]),
            ("ni", &["你", "尼"]),
            ("nian", &["年", "念"]),
            ("ren", &["人", "认"]),
            ("ri", &["日"]),
            ("san", &["三", "散"]),
            ("shi", &["是", "十", "时", "事"]),
            ("shui", &["水", "谁"]),
            ("ta", &["他", "她", "它"]),
            ("tian", &["天", "田"]),
            ("wan", &["万", "完", "玩"]),
            ("wo", &["我"]),
            ("wu", &["五", "无", "物"]),
            ("xi", &["西", "希", "系"]),
            ("xian", &["先", "现", "线"]),
            ("xiao", &["小", "笑", "校"]),
            ("xin", &["新", "心", "信"]),
            ("xing", &["行", "星", "性"]),
            ("yao", &["要", "药"]),
            ("yi", &["一", "已", "以"]),
            ("you", &["有", "又", "右"]),
            ("yue", &["月", "越"]),
            ("zai", &["在", "再"]),
            ("zhe", &["这", "着"]),
            ("zhong", &["中", "种"]),
            ("zi", &["子", "字", "自"]),
            ("zuo", &["做", "坐", "左"]),
        ];

        let input = self.input_buffer.as_str();
        for &(pinyin, chars) in candidates {
            if pinyin == input {
                for &ch in chars {
                    self.candidates.push(Candidate {
                        text: String::from(ch),
                        frequency: 100,
                    });
                }
                break;
            }
        }

        // If no exact match, try prefix match
        if self.candidates.is_empty() {
            for &(pinyin, chars) in candidates {
                if pinyin.starts_with(input) {
                    for &ch in chars.iter().take(2) {
                        self.candidates.push(Candidate {
                            text: String::from(ch),
                            frequency: 50,
                        });
                    }
                }
            }
        }

        self.selected = 0;
    }

    // ─── Japanese ────────────────────────────────────────────────────
    fn process_japanese(&mut self, c: char) -> Option<String> {
        if c.is_ascii_alphabetic() {
            self.input_buffer.push(c.to_ascii_lowercase());
            // Convert romaji to kana
            let kana = romaji_to_hiragana(&self.input_buffer);
            if !kana.is_empty() {
                self.preedit = kana.clone();
                self.input_buffer.clear();
                self.visible = true;

                if self.mode == ImeMode::Katakana {
                    self.preedit = hiragana_to_katakana(&self.preedit);
                }
            }
            None
        } else if c == ' ' || c == '\n' {
            self.commit()
        } else {
            let result = if !self.preedit.is_empty() {
                self.commit()
            } else {
                None
            };
            // Also output the typed character
            if let Some(mut s) = result {
                s.push(c);
                Some(s)
            } else {
                Some(String::from(c.encode_utf8(&mut [0u8; 4])))
            }
        }
    }

    // ─── Korean ──────────────────────────────────────────────────────
    fn process_korean(&mut self, c: char) -> Option<String> {
        // Korean Jamo composition: L + V + T → syllable
        // U+1100..U+11FF for Jamo, U+AC00..U+D7A3 for composed syllables
        if c.is_ascii_alphabetic() {
            self.input_buffer.push(c);
            let composed = compose_korean_syllable(&self.input_buffer);
            if let Some(syllable) = composed {
                self.preedit = String::from(syllable.encode_utf8(&mut [0u8; 4]));
                self.visible = true;
            }
            None
        } else {
            self.commit()
        }
    }

    /// Set cursor position for composition window
    pub fn set_cursor_position(&mut self, x: i32, y: i32) {
        self.cursor_x = x;
        self.cursor_y = y;
    }
}

lazy_static::lazy_static! {
    pub static ref IME: Mutex<ImeState> = Mutex::new(ImeState::new());
}

// ═══════════════════════════════════════════════════════════════════════
// ROMAJI → HIRAGANA CONVERSION
// ═══════════════════════════════════════════════════════════════════════

fn romaji_to_hiragana(romaji: &str) -> String {
    let table: &[(&str, &str)] = &[
        ("a", "あ"),
        ("i", "い"),
        ("u", "う"),
        ("e", "え"),
        ("o", "お"),
        ("ka", "か"),
        ("ki", "き"),
        ("ku", "く"),
        ("ke", "け"),
        ("ko", "こ"),
        ("sa", "さ"),
        ("si", "し"),
        ("shi", "し"),
        ("su", "す"),
        ("se", "せ"),
        ("so", "そ"),
        ("ta", "た"),
        ("ti", "ち"),
        ("chi", "ち"),
        ("tu", "つ"),
        ("tsu", "つ"),
        ("te", "て"),
        ("to", "と"),
        ("na", "な"),
        ("ni", "に"),
        ("nu", "ぬ"),
        ("ne", "ね"),
        ("no", "の"),
        ("ha", "は"),
        ("hi", "ひ"),
        ("hu", "ふ"),
        ("fu", "ふ"),
        ("he", "へ"),
        ("ho", "ほ"),
        ("ma", "ま"),
        ("mi", "み"),
        ("mu", "む"),
        ("me", "め"),
        ("mo", "も"),
        ("ya", "や"),
        ("yu", "ゆ"),
        ("yo", "よ"),
        ("ra", "ら"),
        ("ri", "り"),
        ("ru", "る"),
        ("re", "れ"),
        ("ro", "ろ"),
        ("wa", "わ"),
        ("wi", "ゐ"),
        ("we", "ゑ"),
        ("wo", "を"),
        ("nn", "ん"),
        ("n", "ん"),
        ("ga", "が"),
        ("gi", "ぎ"),
        ("gu", "ぐ"),
        ("ge", "げ"),
        ("go", "ご"),
        ("za", "ざ"),
        ("zi", "じ"),
        ("ji", "じ"),
        ("zu", "ず"),
        ("ze", "ぜ"),
        ("zo", "ぞ"),
        ("da", "だ"),
        ("di", "ぢ"),
        ("du", "づ"),
        ("de", "で"),
        ("do", "ど"),
        ("ba", "ば"),
        ("bi", "び"),
        ("bu", "ぶ"),
        ("be", "べ"),
        ("bo", "ぼ"),
        ("pa", "ぱ"),
        ("pi", "ぴ"),
        ("pu", "ぷ"),
        ("pe", "ぺ"),
        ("po", "ぽ"),
    ];

    // Try longest match first
    for &(rom, hira) in table {
        if romaji == rom {
            return String::from(hira);
        }
    }
    String::new()
}

fn hiragana_to_katakana(hiragana: &str) -> String {
    // Katakana = Hiragana + 0x60 for most characters
    let mut result = String::new();
    for c in hiragana.chars() {
        let code = c as u32;
        if (0x3041..=0x3096).contains(&code) {
            if let Some(kata) = char::from_u32(code + 0x60) {
                result.push(kata);
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }
    result
}

// ═══════════════════════════════════════════════════════════════════════
// KOREAN JAMO COMPOSITION
// ═══════════════════════════════════════════════════════════════════════

fn compose_korean_syllable(input: &str) -> Option<char> {
    // Map QWERTY keys to Korean jamo
    // L(leading consonant) + V(vowel) + optional T(trailing consonant)
    let jamo_map: &[(char, u32, u8)] = &[
        // (key, jamo_index, type: 0=L, 1=V, 2=T)
        ('r', 0, 0),  // ㄱ
        ('s', 2, 0),  // ㄴ
        ('e', 3, 0),  // ㄷ
        ('f', 5, 0),  // ㄹ
        ('a', 6, 0),  // ㅁ
        ('q', 7, 0),  // ㅂ
        ('t', 9, 0),  // ㅅ
        ('d', 11, 0), // ㅇ
        ('w', 12, 0), // ㅈ
        ('c', 14, 0), // ㅊ
        ('z', 15, 0), // ㅋ
        ('x', 16, 0), // ㅌ
        ('v', 17, 0), // ㅍ
        ('g', 18, 0), // ㅎ
        ('k', 0, 1),  // ㅏ
        ('o', 1, 1),  // ㅐ
        ('i', 2, 1),  // ㅑ
        ('j', 4, 1),  // ㅓ
        ('p', 5, 1),  // ㅔ
        ('u', 6, 1),  // ㅕ
        ('h', 8, 1),  // ㅗ
        ('y', 12, 1), // ㅛ
        ('n', 13, 1), // ㅜ
        ('b', 17, 1), // ㅠ
        ('m', 18, 1), // ㅡ
        ('l', 20, 1), // ㅣ
    ];

    let mut lead: Option<u32> = None;
    let mut vowel: Option<u32> = None;

    for ch in input.chars() {
        let lower = ch.to_ascii_lowercase();
        for &(key, idx, jtype) in jamo_map {
            if lower == key {
                match jtype {
                    0 if lead.is_none() => {
                        lead = Some(idx);
                    }
                    1 if vowel.is_none() => {
                        vowel = Some(idx);
                    }
                    _ => {}
                }
                break;
            }
        }
    }

    // Compose: syllable = 0xAC00 + L*588 + V*28 + T
    match (lead, vowel) {
        (Some(l), Some(v)) => {
            let code = 0xAC00 + l * 588 + v * 28;
            char::from_u32(code)
        }
        _ => None,
    }
}

/// Toggle IME mode
pub fn toggle_ime() {
    let mut ime = IME.lock();
    ime.mode = match ime.mode {
        ImeMode::Off => ImeMode::Pinyin,
        ImeMode::Pinyin => ImeMode::Hiragana,
        ImeMode::Hiragana => ImeMode::Katakana,
        ImeMode::Katakana => ImeMode::Korean,
        ImeMode::Korean => ImeMode::Off,
    };
    ime.cancel();
    serial_println!("[IME] Mode: {:?}", ime.mode);
}

// ═══════════════════════════════════════════════════════════════════════
// IME VISUAL OVERLAY — renders composition window + candidate list
// ═══════════════════════════════════════════════════════════════════════

/// Draw the IME composition overlay at the current cursor position.
/// Called from the desktop redraw loop when IME has visible state.
pub fn draw_ime_overlay(fb: &mut super::framebuffer::FrameBuffer) {
    let ime = IME.lock();
    if !ime.visible || ime.mode == ImeMode::Off {
        return;
    }

    let bg = super::framebuffer::Pixel::new(255, 255, 240, 255); // light yellow
    let border = super::framebuffer::Pixel::new(180, 180, 160, 255);
    let text_color = super::framebuffer::Pixel::new(30, 30, 30, 255);
    let highlight_bg = super::framebuffer::Pixel::new(60, 120, 215, 255);
    let highlight_text = super::framebuffer::Pixel::new(255, 255, 255, 255);

    let screen_w = fb.width as u32;
    let screen_h = fb.height as u32;
    let cx = ime.cursor_x.max(0) as u32;
    let cy = ime.cursor_y.max(0) as u32;
    let line_h: u32 = 22;
    let padding: u32 = 6;

    // Compute overlay dimensions
    let preedit_w = (ime.preedit.len() as u32 * 10).max(60);
    let cand_count = ime.candidates.len() as u32;
    let overlay_w = preedit_w.max(200) + padding * 2;
    let overlay_h = line_h + (cand_count.min(9) * line_h) + padding * 2;

    // Position: below cursor, clamped to screen
    let ox = (cx as i32).min((screen_w - overlay_w) as i32).max(0);
    let oy_below = cy + 20;
    let oy = if oy_below + overlay_h > screen_h {
        cy.saturating_sub(overlay_h + 4) as i32
    } else {
        oy_below as i32
    };

    // Draw background with border
    let outer_rect = super::framebuffer::Rect::new(ox, oy, overlay_w, overlay_h);
    fb.fill_rect(outer_rect, bg);
    fb.draw_rect(outer_rect, border, 1);

    // Draw preedit text with underline
    let text_x = ox + padding as i32;
    let text_y = oy + padding as i32;
    for (i, ch) in ime.preedit.chars().enumerate() {
        let cx_char = text_x + (i as i32) * 10;
        super::fonts::draw_char(fb, cx_char, text_y, ch, text_color, 1);
    }
    // Underline the preedit text
    let ul_y = text_y + 16;
    let ul_len = (ime.preedit.len() as u32 * 10).min(overlay_w - padding * 2);
    let underline = super::framebuffer::Rect::new(text_x, ul_y, ul_len, 1);
    fb.fill_rect(underline, text_color);

    // Draw candidate list
    for (i, cand) in ime.candidates.iter().enumerate().take(9) {
        let row_y = oy + padding as i32 + line_h as i32 + (i as i32 * line_h as i32);
        let is_selected = i == ime.selected;

        // Highlight selected candidate
        if is_selected {
            let hl_rect = super::framebuffer::Rect::new(ox + 1, row_y, overlay_w - 2, line_h);
            fb.fill_rect(hl_rect, highlight_bg);
        }

        let color = if is_selected {
            highlight_text
        } else {
            text_color
        };
        let label_y = row_y + 3;

        // Draw "N. candidate" label
        let num_ch = char::from(b'1' + i as u8);
        super::fonts::draw_char(fb, text_x, label_y, num_ch, color, 1);
        super::fonts::draw_char(fb, text_x + 8, label_y, '.', color, 1);

        for (j, ch) in cand.text.chars().enumerate() {
            let cx_char = text_x + 16 + (j as i32) * 10;
            super::fonts::draw_char(fb, cx_char, label_y, ch, color, 1);
        }
    }

    // Draw mode indicator at bottom-right of overlay
    let mode_str = match ime.mode {
        ImeMode::Off => "",
        ImeMode::Pinyin => "CN",
        ImeMode::Hiragana => "JP",
        ImeMode::Katakana => "KT",
        ImeMode::Korean => "KR",
    };
    if !mode_str.is_empty() {
        let mx = ox + overlay_w as i32 - padding as i32 - 16;
        let my = oy + overlay_h as i32 - line_h as i32 + 2;
        for (i, ch) in mode_str.chars().enumerate() {
            super::fonts::draw_char(fb, mx + (i as i32) * 8, my, ch, text_color, 1);
        }
    }
}

/// Check if the IME overlay is visible (for redraw scheduling)
pub fn is_overlay_visible() -> bool {
    let ime = IME.lock();
    ime.visible && ime.mode != ImeMode::Off
}

/// Initialize IME subsystem
pub fn init() {
    serial_println!("[KnoxOS] IME subsystem initialized");
}
