// SPDX-License-Identifier: MIT
//! GUI visual regression tests (item 19.3)
//!
//! Captures framebuffer screenshots and compares them against
//! reference images to detect visual regressions.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

/// A captured screenshot for comparison
#[derive(Debug, Clone)]
pub struct Screenshot {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Raw BGRA pixel data
    pub pixels: Vec<u32>,
}

/// Comparison result
#[derive(Debug, Clone)]
pub struct CompareResult {
    pub name: String,
    /// Number of pixels that differ
    pub diff_pixels: u64,
    /// Total pixels
    pub total_pixels: u64,
    /// Percentage difference (0.0 - 100.0)
    pub diff_percent: f64,
    /// Whether the test passed (diff < threshold)
    pub passed: bool,
}

/// Visual test case
#[derive(Debug, Clone)]
pub struct VisualTest {
    pub name: String,
    pub reference: Option<Screenshot>,
    pub threshold_percent: f64,
}

lazy_static::lazy_static! {
    static ref TEST_CASES: Mutex<Vec<VisualTest>> = Mutex::new(Vec::new());
    static ref RESULTS: Mutex<Vec<CompareResult>> = Mutex::new(Vec::new());
}

static TESTS_RUN: AtomicU64 = AtomicU64::new(0);
static TESTS_PASSED: AtomicU64 = AtomicU64::new(0);

/// Default comparison threshold (0.1% pixel difference allowed)
const DEFAULT_THRESHOLD: f64 = 0.1;

/// Capture the current framebuffer as a screenshot
pub fn capture_framebuffer() -> Option<Screenshot> {
    let fb_guard = crate::gui::FRAMEBUFFER.lock();
    let fb = fb_guard.as_ref()?;
    let width = fb.width as u32;
    let height = fb.height as u32;
    let bpp = fb.bytes_per_pixel;
    let mut pixels = Vec::with_capacity((width * height) as usize);

    for y in 0..height as usize {
        for x in 0..width as usize {
            let offset = y * fb.pitch + x * bpp;
            if offset + 3 < fb.buffer.len() {
                // BGRA layout: buffer is [B, G, R, A]
                let b = fb.buffer[offset] as u32;
                let g = fb.buffer[offset + 1] as u32;
                let r = fb.buffer[offset + 2] as u32;
                let a = if bpp >= 4 {
                    fb.buffer[offset + 3] as u32
                } else {
                    0xFF
                };
                pixels.push((a << 24) | (r << 16) | (g << 8) | b);
            } else {
                pixels.push(0);
            }
        }
    }

    Some(Screenshot {
        name: String::from("fullscreen"),
        width,
        height,
        pixels,
    })
}

/// Capture a specific rectangular region
pub fn capture_region(x: u32, y: u32, w: u32, h: u32) -> Option<Screenshot> {
    let fb_guard = crate::gui::FRAMEBUFFER.lock();
    let fb = fb_guard.as_ref()?;
    let fb_w = fb.width as u32;
    let fb_h = fb.height as u32;
    let bpp = fb.bytes_per_pixel;

    // Clamp to framebuffer bounds
    let x1 = x.min(fb_w);
    let y1 = y.min(fb_h);
    let x2 = (x + w).min(fb_w);
    let y2 = (y + h).min(fb_h);
    let rw = x2 - x1;
    let rh = y2 - y1;

    if rw == 0 || rh == 0 {
        return None;
    }

    let mut pixels = Vec::with_capacity((rw * rh) as usize);
    for row in y1..y2 {
        for col in x1..x2 {
            let offset = (row as usize) * fb.pitch + (col as usize) * bpp;
            if offset + 3 < fb.buffer.len() {
                let b = fb.buffer[offset] as u32;
                let g = fb.buffer[offset + 1] as u32;
                let r = fb.buffer[offset + 2] as u32;
                let a = if bpp >= 4 {
                    fb.buffer[offset + 3] as u32
                } else {
                    0xFF
                };
                pixels.push((a << 24) | (r << 16) | (g << 8) | b);
            } else {
                pixels.push(0);
            }
        }
    }

    Some(Screenshot {
        name: alloc::format!("region_{}x{}+{}+{}", rw, rh, x1, y1),
        width: rw,
        height: rh,
        pixels,
    })
}

/// Compare two screenshots pixel-by-pixel
pub fn compare(a: &Screenshot, b: &Screenshot, threshold: f64) -> CompareResult {
    let total = (a.width as u64) * (a.height as u64);

    if a.width != b.width || a.height != b.height {
        return CompareResult {
            name: String::from("size_mismatch"),
            diff_pixels: total,
            total_pixels: total,
            diff_percent: 100.0,
            passed: false,
        };
    }

    let mut diff_count: u64 = 0;
    let len = a.pixels.len().min(b.pixels.len());

    for i in 0..len {
        let pa = a.pixels[i];
        let pb = b.pixels[i];

        if pa != pb {
            // Check if the difference is significant (>5 per channel)
            let dr = ((pa >> 16) & 0xFF) as i32 - ((pb >> 16) & 0xFF) as i32;
            let dg = ((pa >> 8) & 0xFF) as i32 - ((pb >> 8) & 0xFF) as i32;
            let db = (pa & 0xFF) as i32 - (pb & 0xFF) as i32;

            if dr.abs() > 5 || dg.abs() > 5 || db.abs() > 5 {
                diff_count += 1;
            }
        }
    }

    let diff_percent = if total > 0 {
        (diff_count as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    CompareResult {
        name: String::new(),
        diff_pixels: diff_count,
        total_pixels: total,
        diff_percent,
        passed: diff_percent <= threshold,
    }
}

/// Register a visual test case
pub fn register_test(name: &str, reference: Option<Screenshot>, threshold: f64) {
    TEST_CASES.lock().push(VisualTest {
        name: String::from(name),
        reference,
        threshold_percent: threshold,
    });
}

/// Run a specific visual test
pub fn run_test(name: &str) -> Option<CompareResult> {
    TESTS_RUN.fetch_add(1, Ordering::Relaxed);

    let test_cases = TEST_CASES.lock();
    let test = test_cases.iter().find(|t| t.name == name)?;
    let reference = test.reference.as_ref()?;

    let current = capture_framebuffer()?;
    let mut result = compare(reference, &current, test.threshold_percent);
    result.name = String::from(name);

    if result.passed {
        TESTS_PASSED.fetch_add(1, Ordering::Relaxed);
        crate::serial_println!(
            "[visual_test] PASS: {} (diff={:.3}%)",
            name,
            result.diff_percent
        );
    } else {
        crate::serial_println!(
            "[visual_test] FAIL: {} (diff={:.3}%, threshold={:.3}%)",
            name,
            result.diff_percent,
            test.threshold_percent
        );
    }

    RESULTS.lock().push(result.clone());
    Some(result)
}

/// Run all registered visual tests
pub fn run_all() -> Vec<CompareResult> {
    let names: Vec<String> = TEST_CASES.lock().iter().map(|t| t.name.clone()).collect();
    let mut results = Vec::new();

    for name in &names {
        if let Some(result) = run_test(name) {
            results.push(result);
        }
    }

    let passed = results.iter().filter(|r| r.passed).count();
    let total = results.len();
    crate::serial_println!("[visual_test] {}/{} tests passed", passed, total);

    results
}

/// Save a screenshot as a reference
pub fn save_reference(name: &str, screenshot: &Screenshot) {
    let path = alloc::format!("/home/user/.knoxos/visual_tests/{}.ref", name);
    // Serialize width, height, then pixel data
    let mut data = Vec::with_capacity(8 + screenshot.pixels.len() * 4);
    data.extend_from_slice(&screenshot.width.to_le_bytes());
    data.extend_from_slice(&screenshot.height.to_le_bytes());
    for &px in &screenshot.pixels {
        data.extend_from_slice(&px.to_le_bytes());
    }
    let _ = crate::file_manager::write_file(&path, &data);
    crate::serial_println!("[visual_test] saved reference: {}", name);
}

/// Load a reference screenshot
pub fn load_reference(name: &str) -> Option<Screenshot> {
    let path = alloc::format!("/home/user/.knoxos/visual_tests/{}.ref", name);
    let data = crate::file_manager::read_file(&path).ok()?;
    if data.len() < 8 {
        return None;
    }

    let width = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let height = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let pixel_count = (width as usize) * (height as usize);

    if data.len() < 8 + pixel_count * 4 {
        return None;
    }

    let mut pixels = Vec::with_capacity(pixel_count);
    for i in 0..pixel_count {
        let offset = 8 + i * 4;
        let px = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        pixels.push(px);
    }

    Some(Screenshot {
        name: String::from(name),
        width,
        height,
        pixels,
    })
}

pub fn stats() -> (u64, u64) {
    (
        TESTS_RUN.load(Ordering::Relaxed),
        TESTS_PASSED.load(Ordering::Relaxed),
    )
}

/// Initialize the visual regression test system
pub fn init() {
    let _ = crate::file_manager::mkdir("/home/user/.knoxos/visual_tests", 0o755);
    crate::serial_println!("[visual_test] visual regression test system initialized");
}
