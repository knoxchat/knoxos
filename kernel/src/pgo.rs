//! Profile-Guided Optimization (PGO) — Build system support
//!
//! Provides infrastructure for profile-guided optimization:
//! instrumented builds that collect runtime profiling data,
//! and optimized builds that use that data.
//! Covers status.md item 22.12 (PGO).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

/// PGO build mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PgoMode {
    /// Normal build — no PGO
    None,
    /// Instrumented build — collecting profile data
    Instrument,
    /// Optimized build — using collected profile data
    Optimize,
}

/// Function execution counter
#[derive(Debug, Clone)]
pub struct FunctionProfile {
    pub name: String,
    pub call_count: u64,
    pub total_cycles: u64,
    pub is_hot: bool,
}

/// Branch profile data
#[derive(Debug, Clone)]
pub struct BranchProfile {
    pub location: String,
    pub taken_count: u64,
    pub not_taken_count: u64,
}

/// PGO profile data collected during instrumented run
#[derive(Debug)]
struct ProfileData {
    mode: PgoMode,
    functions: BTreeMap<String, FunctionProfile>,
    branches: Vec<BranchProfile>,
    total_samples: u64,
    hot_threshold: u64,
}

lazy_static::lazy_static! {
    static ref PROFILE: Mutex<ProfileData> = Mutex::new(ProfileData {
        mode: PgoMode::None,
        functions: BTreeMap::new(),
        branches: Vec::new(),
        total_samples: 0,
        hot_threshold: 1000,
    });
}

static SAMPLE_COUNT: AtomicU64 = AtomicU64::new(0);

/// Record a function call (called from instrumented code)
pub fn record_function_call(name: &str, cycles: u64) {
    SAMPLE_COUNT.fetch_add(1, Ordering::Relaxed);

    let mut data = PROFILE.lock();
    if data.mode != PgoMode::Instrument {
        return;
    }

    let threshold = data.hot_threshold;
    let entry = data
        .functions
        .entry(String::from(name))
        .or_insert(FunctionProfile {
            name: String::from(name),
            call_count: 0,
            total_cycles: 0,
            is_hot: false,
        });

    entry.call_count += 1;
    entry.total_cycles += cycles;
    entry.is_hot = entry.call_count > threshold;
    data.total_samples += 1;
}

/// Record a branch taken/not-taken (called from instrumented code)
pub fn record_branch(location: &str, taken: bool) {
    let mut data = PROFILE.lock();
    if data.mode != PgoMode::Instrument {
        return;
    }

    if let Some(branch) = data.branches.iter_mut().find(|b| b.location == location) {
        if taken {
            branch.taken_count += 1;
        } else {
            branch.not_taken_count += 1;
        }
    } else {
        data.branches.push(BranchProfile {
            location: String::from(location),
            taken_count: if taken { 1 } else { 0 },
            not_taken_count: if taken { 0 } else { 1 },
        });
    }
}

/// Set PGO mode
pub fn set_mode(mode: PgoMode) {
    let mut data = PROFILE.lock();
    data.mode = mode;
    if mode == PgoMode::Instrument {
        data.functions.clear();
        data.branches.clear();
        data.total_samples = 0;
    }
    crate::serial_println!("[pgo] Mode set to {:?}", mode);
}

/// Get current PGO mode
pub fn current_mode() -> PgoMode {
    PROFILE.lock().mode
}

/// Get the top N hottest functions
pub fn hot_functions(n: usize) -> Vec<FunctionProfile> {
    let data = PROFILE.lock();
    let mut funcs: Vec<_> = data.functions.values().cloned().collect();
    funcs.sort_by_key(|b| core::cmp::Reverse(b.call_count));
    funcs.truncate(n);
    funcs
}

/// Get branch prediction accuracy for a location
pub fn branch_bias(location: &str) -> Option<f32> {
    let data = PROFILE.lock();
    data.branches
        .iter()
        .find(|b| b.location == location)
        .map(|b| {
            let total = b.taken_count + b.not_taken_count;
            if total == 0 {
                0.5
            } else {
                b.taken_count as f32 / total as f32
            }
        })
}

/// Export profile data as a summary string
pub fn export_profile() -> String {
    let data = PROFILE.lock();
    let mut out = String::from("# PGO Profile Data\n");
    out.push_str(&alloc::format!("total_samples: {}\n", data.total_samples));
    out.push_str(&alloc::format!("functions: {}\n", data.functions.len()));
    out.push_str(&alloc::format!("branches: {}\n", data.branches.len()));

    out.push_str("\n## Hot Functions\n");
    let mut funcs: Vec<_> = data.functions.values().collect();
    funcs.sort_by_key(|b| core::cmp::Reverse(b.call_count));
    for f in funcs.iter().take(20) {
        out.push_str(&alloc::format!(
            "  {} — {} calls, {} cycles/call avg\n",
            f.name,
            f.call_count,
            f.total_cycles.checked_div(f.call_count).unwrap_or(0)
        ));
    }

    out.push_str("\n## Branch Biases\n");
    for b in data.branches.iter().take(20) {
        let total = b.taken_count + b.not_taken_count;
        let bias = if total > 0 {
            b.taken_count as f32 / total as f32
        } else {
            0.5
        };
        out.push_str(&alloc::format!(
            "  {} — {:.1}% taken ({}/{})\n",
            b.location,
            bias * 100.0,
            b.taken_count,
            total
        ));
    }

    out
}

/// Get total profile samples
pub fn sample_count() -> u64 {
    SAMPLE_COUNT.load(Ordering::Relaxed)
}

/// Initialize PGO subsystem
pub fn init() {
    crate::serial_println!("[pgo] Profile-guided optimization subsystem initialized");
}
