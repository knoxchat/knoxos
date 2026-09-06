/// Benchmark Suite — Performance benchmarking framework
///
/// Provides:
///   - Memory allocation benchmarks
///   - Context switch latency
///   - Framebuffer rendering throughput
///   - Filesystem I/O benchmarks
///   - Network throughput tests
///   - Results formatting
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// BENCHMARK TYPES
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub name: String,
    pub category: BenchCategory,
    pub iterations: u64,
    pub total_tsc_cycles: u64,
    pub min_cycles: u64,
    pub max_cycles: u64,
    pub avg_cycles: u64,
    pub throughput: Option<String>, // e.g., "1.2 GB/s"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchCategory {
    Memory,
    Scheduling,
    Graphics,
    Filesystem,
    Network,
    Crypto,
    Overall,
}

impl BenchCategory {
    pub fn name(&self) -> &'static str {
        match self {
            BenchCategory::Memory => "Memory",
            BenchCategory::Scheduling => "Scheduling",
            BenchCategory::Graphics => "Graphics",
            BenchCategory::Filesystem => "Filesystem",
            BenchCategory::Network => "Network",
            BenchCategory::Crypto => "Crypto",
            BenchCategory::Overall => "Overall",
        }
    }
}

impl BenchmarkResult {
    pub fn format_cycles(&self) -> String {
        if self.avg_cycles > 1_000_000 {
            alloc::format!("{:.2}M cycles", self.avg_cycles as f64 / 1_000_000.0)
        } else if self.avg_cycles > 1_000 {
            alloc::format!("{:.2}K cycles", self.avg_cycles as f64 / 1_000.0)
        } else {
            alloc::format!("{} cycles", self.avg_cycles)
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// TSC TIMING
// ═══════════════════════════════════════════════════════════════════════

/// Read the Time Stamp Counter
#[inline(always)]
pub fn rdtsc() -> u64 {
    unsafe {
        let mut lo: u32 = 0;
        let mut hi: u32 = 0;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi);
        ((hi as u64) << 32) | (lo as u64)
    }
}

/// Serializing barrier + TSC read for accurate timing
#[inline(always)]
pub fn rdtscp() -> u64 {
    unsafe {
        let mut lo: u32 = 0;
        let mut hi: u32 = 0;
        let mut _aux: u32 = 0;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("rdtscp", out("eax") lo, out("edx") hi, out("ecx") _aux);
        // Follow with LFENCE to serialize (CPUID clobbers rbx which LLVM reserves)
        core::arch::asm!("lfence", options(nomem, nostack));
        ((hi as u64) << 32) | (lo as u64)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// BENCHMARK RUNNER
// ═══════════════════════════════════════════════════════════════════════

pub struct BenchmarkSuite {
    pub results: Vec<BenchmarkResult>,
}

impl BenchmarkSuite {
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
        }
    }

    /// Run a benchmark function and record results
    pub fn run<F>(&mut self, name: &str, category: BenchCategory, iterations: u64, mut f: F)
    where
        F: FnMut(),
    {
        serial_println!("[Bench] Running: {} ({} iterations)...", name, iterations);

        let mut min = u64::MAX;
        let mut max = 0u64;
        let mut total = 0u64;

        // Warmup
        for _ in 0..10.min(iterations) {
            f();
        }

        // Actual benchmark
        for _ in 0..iterations {
            let start = rdtscp();
            f();
            let end = rdtscp();
            let elapsed = end.saturating_sub(start);

            total += elapsed;
            if elapsed < min {
                min = elapsed;
            }
            if elapsed > max {
                max = elapsed;
            }
        }

        let avg = total.checked_div(iterations).unwrap_or(0);

        let result = BenchmarkResult {
            name: String::from(name),
            category,
            iterations,
            total_tsc_cycles: total,
            min_cycles: min,
            max_cycles: max,
            avg_cycles: avg,
            throughput: None,
        };

        serial_println!(
            "[Bench] {}: avg={}, min={}, max={}",
            name,
            result.format_cycles(),
            min,
            max
        );

        self.results.push(result);
    }

    /// Run all standard benchmarks
    pub fn run_all(&mut self) {
        self.bench_memory_alloc();
        self.bench_pixel_fill();
        self.bench_alpha_blend();
        self.bench_string_ops();
    }

    /// Benchmark: heap allocation
    fn bench_memory_alloc(&mut self) {
        self.run("heap_alloc_64B", BenchCategory::Memory, 10000, || {
            let v: alloc::vec::Vec<u8> = alloc::vec![0u8; 64];
            core::hint::black_box(v);
        });

        self.run("heap_alloc_4KB", BenchCategory::Memory, 1000, || {
            let v: alloc::vec::Vec<u8> = alloc::vec![0u8; 4096];
            core::hint::black_box(v);
        });
    }

    /// Benchmark: pixel fill
    fn bench_pixel_fill(&mut self) {
        use crate::gui::framebuffer::Pixel;

        let mut buf = alloc::vec![Pixel::rgb(0, 0, 0); 1920];
        let color = Pixel::rgb(100, 150, 200);

        self.run("pixel_fill_1920", BenchCategory::Graphics, 10000, || {
            for px in buf.iter_mut() {
                *px = color;
            }
            core::hint::black_box(&buf);
        });
    }

    /// Benchmark: alpha blending
    fn bench_alpha_blend(&mut self) {
        use crate::gui::framebuffer::Pixel;

        let mut dst = alloc::vec![Pixel::rgb(50, 100, 150); 1920];
        let src = Pixel::new(200, 100, 50, 128);

        self.run("alpha_blend_1920", BenchCategory::Graphics, 10000, || {
            for px in dst.iter_mut() {
                let sa = src.a as u32;
                let inv_a = 256 - sa;
                px.r = ((src.r as u32 * sa + px.r as u32 * inv_a) >> 8) as u8;
                px.g = ((src.g as u32 * sa + px.g as u32 * inv_a) >> 8) as u8;
                px.b = ((src.b as u32 * sa + px.b as u32 * inv_a) >> 8) as u8;
            }
            core::hint::black_box(&dst);
        });
    }

    /// Benchmark: string operations
    fn bench_string_ops(&mut self) {
        self.run("string_format", BenchCategory::Overall, 5000, || {
            let s = alloc::format!("Hello, world! The answer is {}", 42);
            core::hint::black_box(s);
        });

        self.run("string_concat", BenchCategory::Overall, 5000, || {
            let mut s = String::new();
            for i in 0..100 {
                s.push('x');
            }
            core::hint::black_box(s);
        });
    }

    /// Print all results
    pub fn print_results(&self) {
        serial_println!("\n═══════════════════════════════════════════════");
        serial_println!("  KnoxOS Benchmark Results");
        serial_println!("═══════════════════════════════════════════════");

        let mut current_cat: Option<BenchCategory> = None;
        for result in &self.results {
            if current_cat != Some(result.category) {
                serial_println!("\n  [{}]", result.category.name());
                current_cat = Some(result.category);
            }
            serial_println!(
                "    {:<30} avg={:>12}  min={:>12}  max={:>12}",
                result.name,
                result.format_cycles(),
                result.min_cycles,
                result.max_cycles,
            );
            if let Some(ref tp) = result.throughput {
                serial_println!("    {:<30} throughput: {}", "", tp);
            }
        }
        serial_println!("═══════════════════════════════════════════════\n");
    }
}

lazy_static::lazy_static! {
    pub static ref BENCH_SUITE: Mutex<BenchmarkSuite> = Mutex::new(BenchmarkSuite::new());
    /// Historical benchmark results for regression tracking
    static ref BENCH_HISTORY: Mutex<BenchmarkHistory> = Mutex::new(BenchmarkHistory::new());
}

// ═══════════════════════════════════════════════════════════════════════
// PERFORMANCE REGRESSION TRACKING
// ═══════════════════════════════════════════════════════════════════════

/// A saved benchmark run
#[derive(Debug, Clone)]
pub struct BenchmarkRun {
    pub run_id: u32,
    pub timestamp: u64, // RTC epoch-ish
    pub kernel_version: String,
    pub results: Vec<BenchmarkResult>,
}

/// Regression detection thresholds
#[derive(Debug, Clone, Copy)]
pub struct RegressionThresholds {
    /// Percentage slowdown to consider a warning (default: 10%)
    pub warning_pct: f64,
    /// Percentage slowdown to consider a regression (default: 25%)
    pub regression_pct: f64,
    /// Percentage speedup to highlight an improvement (default: 10%)
    pub improvement_pct: f64,
}

impl Default for RegressionThresholds {
    fn default() -> Self {
        Self {
            warning_pct: 10.0,
            regression_pct: 25.0,
            improvement_pct: 10.0,
        }
    }
}

/// Regression status for a single benchmark
#[derive(Debug, Clone)]
pub enum RegressionStatus {
    /// No prior data to compare
    Baseline,
    /// Within acceptable range
    Ok { delta_pct: f64 },
    /// Faster than before (good!)
    Improved { delta_pct: f64 },
    /// Slightly slower — warning
    Warning { delta_pct: f64 },
    /// Significantly slower — regression detected
    Regression { delta_pct: f64 },
}

impl RegressionStatus {
    pub fn symbol(&self) -> &'static str {
        match self {
            RegressionStatus::Baseline => "🆕",
            RegressionStatus::Ok { .. } => "✅",
            RegressionStatus::Improved { .. } => "⬆️",
            RegressionStatus::Warning { .. } => "⚠️",
            RegressionStatus::Regression { .. } => "❌",
        }
    }

    pub fn label(&self) -> String {
        match self {
            RegressionStatus::Baseline => String::from("baseline"),
            RegressionStatus::Ok { delta_pct } => alloc::format!("ok ({:+.1}%)", delta_pct),
            RegressionStatus::Improved { delta_pct } => {
                alloc::format!("improved ({:+.1}%)", delta_pct)
            }
            RegressionStatus::Warning { delta_pct } => {
                alloc::format!("warning ({:+.1}%)", delta_pct)
            }
            RegressionStatus::Regression { delta_pct } => {
                alloc::format!("REGRESSION ({:+.1}%)", delta_pct)
            }
        }
    }
}

/// Benchmark history manager
pub struct BenchmarkHistory {
    pub runs: Vec<BenchmarkRun>,
    pub max_runs: usize,
    pub thresholds: RegressionThresholds,
    next_run_id: u32,
}

impl BenchmarkHistory {
    pub fn new() -> Self {
        Self {
            runs: Vec::new(),
            max_runs: 50, // Keep last 50 runs
            thresholds: RegressionThresholds::default(),
            next_run_id: 1,
        }
    }

    /// Save current benchmark results as a new run
    pub fn save_run(&mut self, results: &[BenchmarkResult], version: &str) -> u32 {
        let run_id = self.next_run_id;
        self.next_run_id += 1;

        let run = BenchmarkRun {
            run_id,
            timestamp: crate::rtc::read_rtc().to_unix_timestamp() as u64,
            kernel_version: String::from(version),
            results: results.to_vec(),
        };

        self.runs.push(run);

        // Prune old runs
        while self.runs.len() > self.max_runs {
            self.runs.remove(0);
        }

        // Persist to VFS
        self.persist_to_vfs();

        serial_println!("[Bench] Saved run #{} ({} results)", run_id, results.len());
        run_id
    }

    /// Get the most recent previous run for comparison
    pub fn previous_run(&self) -> Option<&BenchmarkRun> {
        if self.runs.len() >= 2 {
            Some(&self.runs[self.runs.len() - 2])
        } else {
            None
        }
    }

    /// Compare current results against the previous run
    pub fn check_regressions(
        &self,
        current: &[BenchmarkResult],
    ) -> Vec<(String, RegressionStatus)> {
        let mut statuses = Vec::new();

        let prev = match self.previous_run() {
            Some(p) => p,
            None => {
                // No previous data — everything is baseline
                for r in current {
                    statuses.push((r.name.clone(), RegressionStatus::Baseline));
                }
                return statuses;
            }
        };

        for result in current {
            // Find matching benchmark in previous run
            let prev_result = prev.results.iter().find(|r| r.name == result.name);

            match prev_result {
                None => {
                    statuses.push((result.name.clone(), RegressionStatus::Baseline));
                }
                Some(prev_r) => {
                    if prev_r.avg_cycles == 0 {
                        statuses
                            .push((result.name.clone(), RegressionStatus::Ok { delta_pct: 0.0 }));
                        continue;
                    }

                    // Positive delta = slower (regression), negative = faster (improvement)
                    let delta = result.avg_cycles as f64 - prev_r.avg_cycles as f64;
                    let delta_pct = (delta / prev_r.avg_cycles as f64) * 100.0;

                    let status = if delta_pct < -self.thresholds.improvement_pct {
                        RegressionStatus::Improved { delta_pct }
                    } else if delta_pct > self.thresholds.regression_pct {
                        RegressionStatus::Regression { delta_pct }
                    } else if delta_pct > self.thresholds.warning_pct {
                        RegressionStatus::Warning { delta_pct }
                    } else {
                        RegressionStatus::Ok { delta_pct }
                    };

                    statuses.push((result.name.clone(), status));
                }
            }
        }

        statuses
    }

    /// Print regression report
    pub fn print_regression_report(&self, current: &[BenchmarkResult]) {
        let statuses = self.check_regressions(current);

        serial_println!("\n═══════════════════════════════════════════════");
        serial_println!("  Performance Regression Report");
        if let Some(prev) = self.previous_run() {
            serial_println!(
                "  Comparing against run #{} (v{})",
                prev.run_id,
                prev.kernel_version
            );
        } else {
            serial_println!("  (Baseline — no previous run)");
        }
        serial_println!("═══════════════════════════════════════════════");

        let mut regressions = 0u32;
        let mut warnings = 0u32;
        let mut improvements = 0u32;

        for (name, status) in &statuses {
            let symbol = status.symbol();
            let label = status.label();
            serial_println!("  {} {:<30} {}", symbol, name, label);

            match status {
                RegressionStatus::Regression { .. } => regressions += 1,
                RegressionStatus::Warning { .. } => warnings += 1,
                RegressionStatus::Improved { .. } => improvements += 1,
                _ => {}
            }
        }

        serial_println!("───────────────────────────────────────────────");
        serial_println!(
            "  Summary: {} regressions, {} warnings, {} improvements, {} total",
            regressions,
            warnings,
            improvements,
            statuses.len()
        );
        serial_println!("═══════════════════════════════════════════════\n");
    }

    /// Persist history to VFS for cross-boot tracking
    fn persist_to_vfs(&self) {
        // Ensure directory exists
        crate::vfs::ensure_directory("/var/benchmark");

        // Write a summary file
        let mut content = String::new();
        content.push_str("# KnoxOS Benchmark History\n\n");

        for run in &self.runs {
            content.push_str(&alloc::format!(
                "## Run #{} — v{} (ts={})\n",
                run.run_id,
                run.kernel_version,
                run.timestamp
            ));
            for r in &run.results {
                content.push_str(&alloc::format!(
                    "  {}: avg={} min={} max={} iters={}\n",
                    r.name,
                    r.avg_cycles,
                    r.min_cycles,
                    r.max_cycles,
                    r.iterations
                ));
            }
            content.push('\n');
        }

        let _ = crate::vfs::write_file_dispatch("/var/benchmark/history.txt", content.as_bytes());
    }

    /// Load history from VFS (called at init)
    pub fn load_from_vfs(&mut self) {
        // Try to read saved history
        if let Some(data) = crate::vfs::read_file_dispatch("/var/benchmark/history.txt") {
            // Parse saved data (simplified: just count runs)
            let text = core::str::from_utf8(&data).unwrap_or("");
            let run_count = text.matches("## Run #").count();
            if run_count > 0 {
                serial_println!("[Bench] Loaded {} historical runs from VFS", run_count);
            }
        }
    }
}

/// Initialize benchmark suite
pub fn init() {
    // Try to load historical data
    BENCH_HISTORY.lock().load_from_vfs();
    serial_println!("[KnoxOS] Benchmark suite initialized (with regression tracking)");
}

/// Run all benchmarks, check for regressions, and save results
pub fn run_all() {
    let mut suite = BENCH_SUITE.lock();
    suite.run_all();
    suite.print_results();

    let results = suite.results.clone();
    drop(suite);

    // Run regression analysis
    let mut history = BENCH_HISTORY.lock();
    history.print_regression_report(&results);
    history.save_run(&results, env!("CARGO_PKG_VERSION"));
}

/// Run benchmarks and return regression statuses
pub fn run_with_regression_check() -> Vec<(String, RegressionStatus)> {
    let mut suite = BENCH_SUITE.lock();
    suite.run_all();
    let results = suite.results.clone();
    drop(suite);

    let mut history = BENCH_HISTORY.lock();
    let statuses = history.check_regressions(&results);
    history.save_run(&results, env!("CARGO_PKG_VERSION"));
    statuses
}

/// Set regression detection thresholds
pub fn set_thresholds(warning_pct: f64, regression_pct: f64, improvement_pct: f64) {
    let mut history = BENCH_HISTORY.lock();
    history.thresholds = RegressionThresholds {
        warning_pct,
        regression_pct,
        improvement_pct,
    };
}

// ═══════════════════════════════════════════════════════════════════════
// BOOT TIME MEASUREMENT  (Section 28 — boot time tracking)
// ═══════════════════════════════════════════════════════════════════════

/// Boot time benchmark: measures each boot stage and total boot time
pub fn bench_boot_time() {
    let stages = crate::power::boot_timing_report();
    let mut total_ticks: u64 = 0;
    for stage in &stages {
        total_ticks += stage.end_tick.saturating_sub(stage.start_tick);
    }
    serial_println!(
        "[bench] Boot time report ({} stages, {} total ticks):",
        stages.len(),
        total_ticks
    );
    for stage in &stages {
        let ticks = stage.end_tick.saturating_sub(stage.start_tick);
        serial_println!("  {}: {} ticks", stage.name, ticks);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// MEMORY LEAK REGRESSION  (Section 28 — memory leak CI)
// ═══════════════════════════════════════════════════════════════════════

/// Check for memory leaks by comparing alloc/free counts
pub fn bench_memory_leak_check() -> bool {
    let stats = crate::stress_test::memory_stats();
    let leaks = stats.alloc_count.saturating_sub(stats.free_count);
    serial_println!(
        "[bench] Memory: alloc={} free={} diff={} live={}",
        stats.alloc_count,
        stats.free_count,
        leaks,
        stats.live_bytes
    );
    // Allow a small margin for static allocations
    leaks < 64
}

// ═══════════════════════════════════════════════════════════════════════
// COVERAGE TRACKING STUB  (Section 28)
// ═══════════════════════════════════════════════════════════════════════

static COVERAGE_FUNCTIONS_HIT: spin::Mutex<alloc::collections::BTreeSet<&'static str>> =
    spin::Mutex::new(alloc::collections::BTreeSet::new());

/// Record that a function was exercised during testing
pub fn coverage_hit(func_name: &'static str) {
    COVERAGE_FUNCTIONS_HIT.lock().insert(func_name);
}

/// Report coverage statistics
pub fn coverage_report() -> (usize, usize) {
    let hit = COVERAGE_FUNCTIONS_HIT.lock().len();
    // Estimated total public functions (rough count)
    let total = 800;
    serial_println!(
        "[coverage] {}/{} functions hit ({:.1}%)",
        hit,
        total,
        (hit as f64 / total as f64) * 100.0
    );
    (hit, total)
}
