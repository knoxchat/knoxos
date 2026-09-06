use crate::serial_println;
/// Performance Monitoring & Profiling
///
/// Hardware performance counters (PMC), function profiling, memory
/// profiling, latency histograms, and perf event sampling.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Performance counter type
#[derive(Debug, Clone, Copy)]
pub enum PerfCounter {
    CpuCycles,
    Instructions,
    CacheMisses,
    CacheReferences,
    BranchMisses,
    BranchInstructions,
    BusCycles,
    PageFaults,
    ContextSwitches,
}

/// A configured performance event
#[derive(Debug)]
pub struct PerfEvent {
    pub counter: PerfCounter,
    pub msr_select: u32, // IA32_PERFEVTSELx
    pub msr_count: u32,  // IA32_PMCx
    pub enabled: bool,
    pub value: u64,
}

/// Function profile entry
#[derive(Debug, Clone)]
pub struct ProfileEntry {
    pub name: String,
    pub call_count: u64,
    pub total_cycles: u64,
    pub min_cycles: u64,
    pub max_cycles: u64,
}

/// Latency histogram bucket
#[derive(Debug, Clone)]
pub struct HistBucket {
    pub lower_us: u64,
    pub upper_us: u64,
    pub count: u64,
}

/// Perf monitor state
pub struct PerfMonitor {
    pub events: Vec<PerfEvent>,
    pub profiles: Vec<ProfileEntry>,
    pub histogram: Vec<HistBucket>,
    pub sampling_enabled: bool,
    pub sample_period: u64,
}

lazy_static::lazy_static! {
    static ref PERF: Mutex<PerfMonitor> = Mutex::new(PerfMonitor {
        events: Vec::new(),
        profiles: Vec::new(),
        histogram: Vec::new(),
        sampling_enabled: false,
        sample_period: 10000,
    });
}

impl PerfMonitor {
    /// Enable a hardware performance counter
    pub fn enable_counter(&mut self, counter: PerfCounter) {
        let (select, event_code): (u32, u32) = match counter {
            PerfCounter::CpuCycles => (0x186, 0x003C),
            PerfCounter::Instructions => (0x186, 0x00C0),
            PerfCounter::CacheMisses => (0x187, 0x412E),
            PerfCounter::CacheReferences => (0x187, 0x4F2E),
            PerfCounter::BranchMisses => (0x188, 0x00C5),
            PerfCounter::BranchInstructions => (0x188, 0x00C4),
            _ => (0x186, 0x003C),
        };

        self.events.push(PerfEvent {
            counter,
            msr_select: select,
            msr_count: select + 0x100, // simplified
            enabled: true,
            value: 0,
        });

        serial_println!(
            "[PERF] Counter {:?} enabled (event=0x{:x})",
            counter,
            event_code
        );
    }

    /// Read all counters
    pub fn read_counters(&mut self) {
        for event in &mut self.events {
            if event.enabled {
                // Would use rdmsr to read IA32_PMCx
                event.value += 1000; // placeholder
            }
        }
    }

    /// Record a function profile sample
    pub fn record_profile(&mut self, name: &str, cycles: u64) {
        if let Some(entry) = self.profiles.iter_mut().find(|e| e.name == name) {
            entry.call_count += 1;
            entry.total_cycles += cycles;
            if cycles < entry.min_cycles {
                entry.min_cycles = cycles;
            }
            if cycles > entry.max_cycles {
                entry.max_cycles = cycles;
            }
        } else {
            self.profiles.push(ProfileEntry {
                name: String::from(name),
                call_count: 1,
                total_cycles: cycles,
                min_cycles: cycles,
                max_cycles: cycles,
            });
        }
    }

    /// Record a latency sample into histogram
    pub fn record_latency(&mut self, latency_us: u64) {
        // Find or create bucket
        let bucket_size = 100; // 100us buckets
        let lower = (latency_us / bucket_size) * bucket_size;
        let upper = lower + bucket_size;

        if let Some(b) = self.histogram.iter_mut().find(|b| b.lower_us == lower) {
            b.count += 1;
        } else {
            self.histogram.push(HistBucket {
                lower_us: lower,
                upper_us: upper,
                count: 1,
            });
        }
    }

    /// Print top functions by cycle count
    pub fn print_top_functions(&self, n: usize) {
        let mut sorted: Vec<&ProfileEntry> = self.profiles.iter().collect();
        sorted.sort_by_key(|b| core::cmp::Reverse(b.total_cycles));
        serial_println!("[PERF] Top {} functions by cycles:", n);
        for (i, entry) in sorted.iter().take(n).enumerate() {
            serial_println!(
                "[PERF]   {}. {} - {}cy ({}x, avg={})",
                i + 1,
                entry.name,
                entry.total_cycles,
                entry.call_count,
                entry.total_cycles / entry.call_count.max(1)
            );
        }
    }

    /// Reset all counters and profiles
    pub fn reset(&mut self) {
        for event in &mut self.events {
            event.value = 0;
        }
        self.profiles.clear();
        self.histogram.clear();
        serial_println!("[PERF] All counters reset");
    }
}

pub fn init() {
    serial_println!("[PERF] Performance monitor initialized");
}
