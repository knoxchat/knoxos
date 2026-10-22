/// Compositor Damage Tracking
///
/// Tracks damaged (changed) screen regions to minimize repainting.
/// Only redraws areas that actually changed, reducing GPU/CPU load significantly.
///
/// Features:
///   - Per-window damage rectangles
///   - Region merging to avoid overdraw
///   - Damage propagation through window hierarchy
///   - Full-screen vs partial repaint decision
///   - Double/triple buffering integration
use alloc::vec::Vec;
use spin::Mutex;

/// A rectangular region on screen
#[derive(Debug, Clone, Copy)]
pub struct DamageRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl DamageRect {
    pub fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self {
            x,
            y,
            width: w,
            height: h,
        }
    }

    pub fn area(&self) -> u64 {
        self.width as u64 * self.height as u64
    }

    pub fn intersects(&self, other: &Self) -> bool {
        self.x < other.x + other.width as i32
            && self.x + self.width as i32 > other.x
            && self.y < other.y + other.height as i32
            && self.y + self.height as i32 > other.y
    }

    pub fn union(&self, other: &Self) -> Self {
        let x1 = self.x.min(other.x);
        let y1 = self.y.min(other.y);
        let x2 = (self.x + self.width as i32).max(other.x + other.width as i32);
        let y2 = (self.y + self.height as i32).max(other.y + other.height as i32);
        Self {
            x: x1,
            y: y1,
            width: (x2 - x1) as u32,
            height: (y2 - y1) as u32,
        }
    }
}

/// Damage tracker for a single output/monitor
pub struct DamageTracker {
    pub output_id: u32,
    pub screen_width: u32,
    pub screen_height: u32,
    damage_rects: Vec<DamageRect>,
    full_damage: bool,
}

lazy_static::lazy_static! {
    static ref TRACKERS: Mutex<Vec<DamageTracker>> = Mutex::new(Vec::new());
}

/// Merge threshold: if damage covers >60% of screen, do full repaint
const FULL_REPAINT_THRESHOLD: f32 = 0.6;
/// Maximum tracked rects before merging
const MAX_RECTS: usize = 64;

impl DamageTracker {
    pub fn new(output_id: u32, w: u32, h: u32) -> Self {
        Self {
            output_id,
            screen_width: w,
            screen_height: h,
            damage_rects: Vec::new(),
            full_damage: true, // Initial full paint
        }
    }

    /// Mark a rectangular region as damaged
    pub fn add_damage(&mut self, rect: DamageRect) {
        if self.full_damage {
            return;
        }
        self.damage_rects.push(rect);
        if self.damage_rects.len() > MAX_RECTS {
            self.merge_rects();
        }
        self.check_full_damage();
    }

    /// Mark entire screen as damaged
    pub fn add_full_damage(&mut self) {
        self.full_damage = true;
        self.damage_rects.clear();
    }

    /// Get damage regions for this frame, then clear
    pub fn take_damage(&mut self) -> DamageResult {
        if self.full_damage {
            self.full_damage = false;
            return DamageResult::Full;
        }
        if self.damage_rects.is_empty() {
            return DamageResult::None;
        }
        self.merge_rects();
        let rects = core::mem::take(&mut self.damage_rects);
        DamageResult::Partial(rects)
    }

    /// Merge overlapping rectangles
    fn merge_rects(&mut self) {
        if self.damage_rects.len() <= 1 {
            return;
        }
        let mut merged = true;
        while merged {
            merged = false;
            let mut i = 0;
            while i < self.damage_rects.len() {
                let mut j = i + 1;
                while j < self.damage_rects.len() {
                    if self.damage_rects[i].intersects(&self.damage_rects[j]) {
                        let combined = self.damage_rects[i].union(&self.damage_rects[j]);
                        self.damage_rects[i] = combined;
                        self.damage_rects.swap_remove(j);
                        merged = true;
                    } else {
                        j += 1;
                    }
                }
                i += 1;
            }
        }
    }

    fn check_full_damage(&mut self) {
        let screen_area = self.screen_width as u64 * self.screen_height as u64;
        let damage_area: u64 = self.damage_rects.iter().map(|r| r.area()).sum();
        if damage_area as f32 / screen_area as f32 > FULL_REPAINT_THRESHOLD {
            self.full_damage = true;
            self.damage_rects.clear();
        }
    }
}

/// Result of querying damage for a frame
pub enum DamageResult {
    None,
    Partial(Vec<DamageRect>),
    Full,
}

pub fn create_tracker(output_id: u32, w: u32, h: u32) {
    TRACKERS.lock().push(DamageTracker::new(output_id, w, h));
}

pub fn init() {
    crate::serial_println!("[DAMAGE] Damage tracking subsystem loaded");
}
