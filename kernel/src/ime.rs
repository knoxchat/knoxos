use crate::serial_println;
/// Input Method Editor (IME)
///
/// CJK composition, candidate window, pre-edit string management,
/// Pinyin/Hangul/Kana input support.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImeMode {
    Direct,
    Pinyin,
    Hangul,
    Kana,
    Cangjie,
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub text: String,
    pub frequency: u32,
}

pub struct ImeState {
    pub mode: ImeMode,
    pub preedit: String,
    pub candidates: Vec<Candidate>,
    pub selected: usize,
    pub active: bool,
}

lazy_static::lazy_static! {
    static ref IME: Mutex<ImeState> = Mutex::new(ImeState {
        mode: ImeMode::Direct,
        preedit: String::new(),
        candidates: Vec::new(),
        selected: 0,
        active: false,
    });
}

impl ImeState {
    pub fn set_mode(&mut self, mode: ImeMode) {
        self.mode = mode;
        self.preedit.clear();
        self.candidates.clear();
        serial_println!("[IME] Mode: {:?}", mode);
    }

    pub fn feed_key(&mut self, ch: char) {
        if self.mode == ImeMode::Direct {
            return;
        }
        self.preedit.push(ch);
        self.lookup_candidates();
    }

    fn lookup_candidates(&mut self) {
        self.candidates.clear();
        // Would query dictionary based on preedit
        serial_println!("[IME] Lookup: '{}'", self.preedit);
    }

    pub fn select(&mut self, idx: usize) -> Option<String> {
        if idx < self.candidates.len() {
            let text = self.candidates[idx].text.clone();
            self.preedit.clear();
            self.candidates.clear();
            Some(text)
        } else {
            None
        }
    }

    pub fn commit_raw(&mut self) -> String {
        let s = self.preedit.clone();
        self.preedit.clear();
        self.candidates.clear();
        s
    }

    pub fn toggle(&mut self) {
        self.active = !self.active;
        if !self.active {
            self.preedit.clear();
            self.candidates.clear();
        }
        serial_println!("[IME] {}", if self.active { "enabled" } else { "disabled" });
    }
}

pub fn init() {
    serial_println!("[IME] Input method editor initialized");
}
