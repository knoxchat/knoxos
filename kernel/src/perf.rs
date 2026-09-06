// perf.rs — Performance monitoring counters (PMC)
// Hardware performance counters, software events, tracepoints

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

/// Performance event types (matches Linux perf_type_id)
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u32)]
pub enum PerfType {
    Hardware = 0,
    Software = 1,
    Tracepoint = 2,
    HwCache = 3,
    Raw = 4,
    Breakpoint = 5,
}

/// Hardware events
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u64)]
pub enum HwEvent {
    CpuCycles = 0,
    Instructions = 1,
    CacheReferences = 2,
    CacheMisses = 3,
    BranchInstructions = 4,
    BranchMisses = 5,
    BusCycles = 6,
    StalledCyclesFrontend = 7,
    StalledCyclesBackend = 8,
    RefCpuCycles = 9,
}

/// Software events
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u64)]
pub enum SwEvent {
    CpuClock = 0,
    TaskClock = 1,
    PageFaults = 2,
    ContextSwitches = 3,
    CpuMigrations = 4,
    PageFaultsMin = 5,
    PageFaultsMax = 6,
    AlignmentFaults = 7,
    EmulationFaults = 8,
    Dummy = 9,
    BpfOutput = 10,
    CgroupSwitches = 11,
}

/// Cache types for HW_CACHE events
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u64)]
pub enum CacheType {
    L1D = 0,
    L1I = 1,
    LL = 2,
    DTLB = 3,
    ITLB = 4,
    BPU = 5, // Branch prediction unit
    Node = 6,
}

/// Cache operations
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u64)]
pub enum CacheOp {
    Read = 0,
    Write = 1,
    Prefetch = 2,
}

/// perf_event_attr — event configuration
#[derive(Debug, Clone, Copy)]
pub struct PerfEventAttr {
    pub event_type: PerfType,
    pub config: u64, // event-specific config
    pub sample_period: u64,
    pub sample_type: u64,
    pub read_format: u64,
    pub flags: u64,
    pub wakeup_events: u32,
    pub exclude_user: bool,
    pub exclude_kernel: bool,
    pub exclude_hv: bool,
    pub exclude_idle: bool,
    pub inherit: bool,
    pub pinned: bool,
    pub exclusive: bool,
    pub disabled: bool,
    pub enable_on_exec: bool,
    pub task: bool,
    pub watermark: bool,
}

impl PerfEventAttr {
    pub fn new(event_type: PerfType, config: u64) -> Self {
        PerfEventAttr {
            event_type,
            config,
            sample_period: 0,
            sample_type: 0,
            read_format: 0,
            flags: 0,
            wakeup_events: 0,
            exclude_user: false,
            exclude_kernel: false,
            exclude_hv: false,
            exclude_idle: false,
            inherit: false,
            pinned: false,
            exclusive: false,
            disabled: true,
            enable_on_exec: false,
            task: false,
            watermark: false,
        }
    }
}

/// Sample data types
pub const PERF_SAMPLE_IP: u64 = 1;
pub const PERF_SAMPLE_TID: u64 = 2;
pub const PERF_SAMPLE_TIME: u64 = 4;
pub const PERF_SAMPLE_ADDR: u64 = 8;
pub const PERF_SAMPLE_READ: u64 = 16;
pub const PERF_SAMPLE_CALLCHAIN: u64 = 32;
pub const PERF_SAMPLE_ID: u64 = 64;
pub const PERF_SAMPLE_CPU: u64 = 128;
pub const PERF_SAMPLE_PERIOD: u64 = 256;
pub const PERF_SAMPLE_STREAM_ID: u64 = 512;
pub const PERF_SAMPLE_RAW: u64 = 1024;
pub const PERF_SAMPLE_BRANCH_STACK: u64 = 2048;
pub const PERF_SAMPLE_REGS_USER: u64 = 4096;
pub const PERF_SAMPLE_STACK_USER: u64 = 8192;

/// A perf event sample
#[derive(Debug, Clone)]
pub struct PerfSample {
    pub ip: u64, // Instruction pointer
    pub pid: u64,
    pub tid: u64,
    pub time: u64,
    pub addr: u64,
    pub cpu: u32,
    pub period: u64,
    pub value: u64,
}

/// An active perf event
#[derive(Debug)]
pub struct PerfEvent {
    pub id: u64,
    pub attr: PerfEventAttr,
    pub pid: i64, // -1 = all processes
    pub cpu: i32, // -1 = all CPUs
    pub group_fd: i64,
    pub enabled: bool,
    pub count: u64,
    pub time_enabled: u64,
    pub time_running: u64,
    pub samples: Vec<PerfSample>,
    pub overflow_count: u64,
}

impl PerfEvent {
    pub fn new(id: u64, attr: PerfEventAttr, pid: i64, cpu: i32, group_fd: i64) -> Self {
        PerfEvent {
            id,
            attr,
            pid,
            cpu,
            group_fd,
            enabled: !attr.disabled,
            count: 0,
            time_enabled: 0,
            time_running: 0,
            samples: Vec::new(),
            overflow_count: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PerfError {
    NotFound,
    InvalidEvent,
    TooMany,
    PermDenied,
    NotSupported,
}

lazy_static! {
    static ref EVENTS: Mutex<PerfEventTable> = Mutex::new(PerfEventTable::new());
    /// Global software counters
    static ref SW_COUNTERS: Mutex<SwCounters> = Mutex::new(SwCounters::new());
}

struct PerfEventTable {
    events: BTreeMap<u64, PerfEvent>,
    next_id: u64,
}

impl PerfEventTable {
    fn new() -> Self {
        PerfEventTable {
            events: BTreeMap::new(),
            next_id: 1,
        }
    }
}

/// Global software event counters
struct SwCounters {
    context_switches: u64,
    cpu_migrations: u64,
    page_faults: u64,
    page_faults_min: u64,
    page_faults_maj: u64,
    alignment_faults: u64,
}

impl SwCounters {
    fn new() -> Self {
        SwCounters {
            context_switches: 0,
            cpu_migrations: 0,
            page_faults: 0,
            page_faults_min: 0,
            page_faults_maj: 0,
            alignment_faults: 0,
        }
    }
}

/// perf_event_open — create a performance monitoring counter
pub fn sys_perf_event_open(
    attr: PerfEventAttr,
    pid: i64,
    cpu: i32,
    group_fd: i64,
    _flags: u64,
) -> Result<u64, PerfError> {
    let mut table = EVENTS.lock();
    let id = table.next_id;
    table.next_id += 1;

    let event = PerfEvent::new(id, attr, pid, cpu, group_fd);
    table.events.insert(id, event);

    Ok(id)
}

/// Enable a perf event
pub fn perf_event_enable(id: u64) -> Result<(), PerfError> {
    let mut table = EVENTS.lock();
    let event = table.events.get_mut(&id).ok_or(PerfError::NotFound)?;
    event.enabled = true;
    Ok(())
}

/// Disable a perf event
pub fn perf_event_disable(id: u64) -> Result<(), PerfError> {
    let mut table = EVENTS.lock();
    let event = table.events.get_mut(&id).ok_or(PerfError::NotFound)?;
    event.enabled = false;
    Ok(())
}

/// Reset a perf event counter
pub fn perf_event_reset(id: u64) -> Result<(), PerfError> {
    let mut table = EVENTS.lock();
    let event = table.events.get_mut(&id).ok_or(PerfError::NotFound)?;
    event.count = 0;
    event.time_enabled = 0;
    event.time_running = 0;
    event.samples.clear();
    Ok(())
}

/// Read a perf event counter
pub fn perf_event_read(id: u64) -> Result<PerfReadData, PerfError> {
    let table = EVENTS.lock();
    let event = table.events.get(&id).ok_or(PerfError::NotFound)?;

    Ok(PerfReadData {
        value: event.count,
        time_enabled: event.time_enabled,
        time_running: event.time_running,
        id: event.id,
    })
}

#[derive(Debug, Clone)]
pub struct PerfReadData {
    pub value: u64,
    pub time_enabled: u64,
    pub time_running: u64,
    pub id: u64,
}

/// Close/destroy a perf event
pub fn perf_event_close(id: u64) {
    EVENTS.lock().events.remove(&id);
}

/// Record a software event (called from kernel subsystems)
pub fn record_sw_event(event: SwEvent) {
    let mut counters = SW_COUNTERS.lock();
    match event {
        SwEvent::ContextSwitches => counters.context_switches += 1,
        SwEvent::CpuMigrations => counters.cpu_migrations += 1,
        SwEvent::PageFaults => counters.page_faults += 1,
        SwEvent::PageFaultsMin => counters.page_faults_min += 1,
        SwEvent::PageFaultsMax => counters.page_faults_maj += 1,
        SwEvent::AlignmentFaults => counters.alignment_faults += 1,
        _ => {}
    }

    // Update any active perf events watching this software event
    let mut table = EVENTS.lock();
    for event_entry in table.events.values_mut() {
        if !event_entry.enabled {
            continue;
        }
        if let PerfType::Software = event_entry.attr.event_type {
            if event_entry.attr.config == event as u64 {
                event_entry.count += 1;
            }
        }
    }
}

/// Read hardware performance counter via RDPMC
#[inline]
pub fn rdpmc(counter: u32) -> u64 {
    let mut low: u32 = 0;
    let mut high: u32 = 0;
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "rdpmc",
            in("ecx") counter,
            out("eax") low,
            out("edx") high,
        );
    }
    ((high as u64) << 32) | (low as u64)
}

/// Read TSC for timing
#[inline]
pub fn rdtsc() -> u64 {
    let mut low: u32 = 0;
    let mut high: u32 = 0;
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "rdtsc",
            out("eax") low,
            out("edx") high,
        );
    }
    ((high as u64) << 32) | (low as u64)
}

/// Get global software counter stats
pub fn get_sw_stats() -> SwStats {
    let counters = SW_COUNTERS.lock();
    SwStats {
        context_switches: counters.context_switches,
        cpu_migrations: counters.cpu_migrations,
        page_faults: counters.page_faults,
        page_faults_min: counters.page_faults_min,
        page_faults_maj: counters.page_faults_maj,
        alignment_faults: counters.alignment_faults,
    }
}

#[derive(Debug, Clone)]
pub struct SwStats {
    pub context_switches: u64,
    pub cpu_migrations: u64,
    pub page_faults: u64,
    pub page_faults_min: u64,
    pub page_faults_maj: u64,
    pub alignment_faults: u64,
}

/// Generate /proc/stat-like performance info
pub fn proc_perf_info() -> String {
    let counters = SW_COUNTERS.lock();
    alloc::format!(
        "ctxt {}\ncpu_migrations {}\npage_faults {}\npage_faults_min {}\npage_faults_maj {}\n",
        counters.context_switches,
        counters.cpu_migrations,
        counters.page_faults,
        counters.page_faults_min,
        counters.page_faults_maj,
    )
}

/// Initialize perf subsystem
pub fn init() {
    crate::serial_println!("  perf subsystem initialized (HW counters, SW events, RDPMC, RDTSC)");
}
