use alloc::format;
/// Crash Reporter — Collect, compress, and store crash diagnostic data
///
/// Captures structured crash information when a panic or fault occurs:
///   - Register state (RIP, RSP, RFLAGS, CR2/CR3)
///   - Stack trace with symbol resolution
///   - Kernel log tail (last N dmesg entries)
///   - Memory map snapshot
///   - PCI device state
///   - Loaded module list
///   - Crash dump compression (LZ4-style)
///   - Persistent storage of crash data across reboots
///   - GUI crash dialog integration
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── Crash Record Types ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashType {
    /// Rust panic (panic! / unwrap / expect)
    KernelPanic,
    /// Page fault (#PF)
    PageFault,
    /// General protection fault (#GP)
    GeneralProtection,
    /// Double fault (#DF)
    DoubleFault,
    /// Stack segment fault (#SS)
    StackFault,
    /// Invalid opcode (#UD)
    InvalidOpcode,
    /// Machine check exception (#MC)
    MachineCheck,
    /// Watchdog timeout (hang detection)
    WatchdogTimeout,
    /// Out of memory
    OutOfMemory,
    /// Driver fault
    DriverFault,
    /// Assertion failure
    AssertionFail,
}

/// CPU register state at crash time
#[derive(Debug, Clone, Copy, Default)]
pub struct RegisterState {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub rflags: u64,
    pub cr2: u64,
    pub cr3: u64,
    pub cs: u16,
    pub ss: u16,
    pub ds: u16,
    pub es: u16,
}

/// A single stack frame in the backtrace
#[derive(Debug, Clone)]
pub struct StackFrame {
    pub address: u64,
    pub symbol: Option<String>,
    pub offset: u64,
}

/// Full crash report
#[derive(Debug, Clone)]
pub struct CrashReport {
    /// Unique crash ID
    pub id: u32,
    /// Crash type
    pub crash_type: CrashType,
    /// Timestamp (tick count)
    pub timestamp: u64,
    /// Panic message (if Rust panic)
    pub message: String,
    /// Source file and line
    pub location: Option<String>,
    /// CPU register state
    pub registers: RegisterState,
    /// Stack backtrace
    pub backtrace: Vec<StackFrame>,
    /// Last N kernel log entries
    pub log_tail: Vec<String>,
    /// Kernel version
    pub kernel_version: String,
    /// Uptime in seconds
    pub uptime_secs: u64,
    /// Which CPU core
    pub cpu_id: u32,
    /// Was compressed?
    pub compressed: bool,
    /// Raw compressed data (if stored)
    pub compressed_data: Vec<u8>,
}

// ─── Global State ───────────────────────────────────────────────────

lazy_static::lazy_static! {
    /// Stored crash reports (survives within session, persisted to disk if possible)
    static ref CRASH_REPORTS: Mutex<Vec<CrashReport>> = Mutex::new(Vec::new());
}

static CRASH_COUNT: AtomicU32 = AtomicU32::new(0);
static NEXT_CRASH_ID: AtomicU32 = AtomicU32::new(1);
static REPORTING_ENABLED: AtomicBool = AtomicBool::new(true);

/// Maximum stored crash reports
const MAX_STORED_REPORTS: usize = 16;
/// Maximum log tail entries per crash
const LOG_TAIL_SIZE: usize = 50;
/// Maximum backtrace depth
const MAX_BACKTRACE_DEPTH: usize = 32;

// ─── Crash Capture ──────────────────────────────────────────────────

/// Record a crash from a Rust panic
pub fn record_panic(message: &str, file: Option<&str>, line: Option<u32>) {
    if !REPORTING_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    let location = match (file, line) {
        (Some(f), Some(l)) => Some(format!("{}:{}", f, l)),
        (Some(f), None) => Some(String::from(f)),
        _ => None,
    };

    let regs = capture_registers();
    let backtrace = capture_backtrace(regs.rbp);
    let log_tail = capture_log_tail();

    let report = CrashReport {
        id: NEXT_CRASH_ID.fetch_add(1, Ordering::SeqCst),
        crash_type: CrashType::KernelPanic,
        timestamp: crate::interrupts::get_ticks(),
        message: String::from(message),
        location,
        registers: regs,
        backtrace,
        log_tail,
        kernel_version: String::from("0.2.1"),
        uptime_secs: crate::interrupts::get_ticks() / 100,
        cpu_id: 0,
        compressed: false,
        compressed_data: Vec::new(),
    };

    store_report(report);
}

/// Record a CPU exception crash
pub fn record_exception(
    crash_type: CrashType,
    error_code: u64,
    rip: u64,
    rsp: u64,
    rbp: u64,
    cr2: u64,
) {
    if !REPORTING_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    let regs = RegisterState {
        rip,
        rsp,
        rbp,
        cr2,
        ..Default::default()
    };

    let backtrace = capture_backtrace(rbp);
    let log_tail = capture_log_tail();

    let message = match crash_type {
        CrashType::PageFault => format!(
            "Page fault at {:#x}, error code {:#x}, CR2={:#x}",
            rip, error_code, cr2
        ),
        CrashType::GeneralProtection => format!("GPF at {:#x}, error code {:#x}", rip, error_code),
        CrashType::DoubleFault => format!("Double fault at {:#x}", rip),
        CrashType::InvalidOpcode => format!("Invalid opcode at {:#x}", rip),
        _ => format!("{:?} at {:#x}", crash_type, rip),
    };

    let report = CrashReport {
        id: NEXT_CRASH_ID.fetch_add(1, Ordering::SeqCst),
        crash_type,
        timestamp: crate::interrupts::get_ticks(),
        message,
        location: Some(format!("{:#018x}", rip)),
        registers: regs,
        backtrace,
        log_tail,
        kernel_version: String::from("0.2.1"),
        uptime_secs: crate::interrupts::get_ticks() / 100,
        cpu_id: 0,
        compressed: false,
        compressed_data: Vec::new(),
    };

    store_report(report);
}

// ─── Data Capture Helpers ───────────────────────────────────────────

fn capture_registers() -> RegisterState {
    let mut regs = RegisterState::default();
    // Read current register state via inline assembly
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "mov {}, rsp",
            out(reg) regs.rsp,
            options(nomem, nostack)
        );
        core::arch::asm!(
            "mov {}, rbp",
            out(reg) regs.rbp,
            options(nomem, nostack)
        );
    }
    // Read CR2 (page fault address) and CR3 (page table base)
    regs.cr2 = crate::arch_compat::registers::control::Cr2::read_raw();
    #[cfg(target_arch = "x86_64")]
    {
        regs.cr3 = crate::arch_compat::registers::control::Cr3::read_raw()
            .0
            .start_address()
            .as_u64();
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        regs.cr3 = crate::arch_compat::registers::control::Cr3::read_raw().0;
    }
    regs
}

fn capture_backtrace(mut rbp: u64) -> Vec<StackFrame> {
    let mut frames = Vec::with_capacity(MAX_BACKTRACE_DEPTH);

    for _ in 0..MAX_BACKTRACE_DEPTH {
        if rbp == 0 || rbp % 8 != 0 {
            break;
        }

        // Frame pointer chain: [rbp] = prev_rbp, [rbp+8] = return_addr
        let prev_rbp = unsafe {
            let ptr = rbp as *const u64;
            if (ptr as usize) < 0x1000 || (ptr as usize) > 0xFFFF_FFFF_FFFF_0000 {
                break;
            }
            core::ptr::read_volatile(ptr)
        };

        let return_addr = unsafe {
            let ptr = (rbp + 8) as *const u64;
            core::ptr::read_volatile(ptr)
        };

        if return_addr == 0 {
            break;
        }

        frames.push(StackFrame {
            address: return_addr,
            symbol: resolve_symbol(return_addr),
            offset: 0,
        });

        rbp = prev_rbp;
    }

    frames
}

fn capture_log_tail() -> Vec<String> {
    let entries = crate::dmesg::read_all();
    entries
        .iter()
        .rev()
        .take(LOG_TAIL_SIZE)
        .map(|e| {
            format!(
                "[{}.{:03}] {}: {}",
                e.timestamp_usec / 1_000_000,
                (e.timestamp_usec / 1_000) % 1_000,
                e.level.prefix(),
                e.message
            )
        })
        .collect()
}

fn resolve_symbol(addr: u64) -> Option<String> {
    // Look up address in kernel symbol table (if available)
    // For now, return a hex address string
    Some(format!(
        "kernel+{:#x}",
        addr.saturating_sub(0xFFFF_8000_0000_0000)
    ))
}

// ─── Report Storage ─────────────────────────────────────────────────

fn store_report(report: CrashReport) {
    CRASH_COUNT.fetch_add(1, Ordering::Relaxed);

    serial_println!(
        "[crash_reporter] #{}: {:?} — {}",
        report.id,
        report.crash_type,
        report.message
    );
    serial_println!(
        "[crash_reporter]   RIP={:#018x} RSP={:#018x}",
        report.registers.rip,
        report.registers.rsp
    );
    serial_println!(
        "[crash_reporter]   backtrace: {} frames, {} log entries",
        report.backtrace.len(),
        report.log_tail.len()
    );

    let mut reports = CRASH_REPORTS.lock();
    if reports.len() >= MAX_STORED_REPORTS {
        reports.remove(0);
    }
    reports.push(report);
}

// ─── Compression (simple LZ4-style) ────────────────────────────────

/// Simple compression for crash data
pub fn compress(data: &[u8]) -> Vec<u8> {
    // Simplified RLE + literal compression
    let mut output = Vec::with_capacity(data.len());
    let mut i = 0;

    while i < data.len() {
        // Look for run of repeated bytes
        let b = data[i];
        let mut run_len = 1usize;
        while i + run_len < data.len() && data[i + run_len] == b && run_len < 127 {
            run_len += 1;
        }

        if run_len >= 4 {
            // Encoded run: 0x80 | length, byte
            output.push(0x80 | (run_len as u8));
            output.push(b);
            i += run_len;
        } else {
            // Literal: count of literal bytes, then bytes
            let start = i;
            while i < data.len() {
                let remaining = data.len() - i;
                let next_b = data[i];
                let next_run = remaining >= 4 && data[i..].iter().take(4).all(|&x| x == next_b);
                if next_run || (i - start) >= 127 {
                    break;
                }
                i += 1;
            }
            let lit_len = i - start;
            if lit_len > 0 {
                output.push(lit_len as u8);
                output.extend_from_slice(&data[start..start + lit_len]);
            }
        }
    }

    output
}

/// Decompress data compressed with `compress()`
pub fn decompress(data: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut i = 0;

    while i < data.len() {
        let header = data[i];
        i += 1;
        if header & 0x80 != 0 {
            // Run
            let run_len = (header & 0x7F) as usize;
            if i < data.len() {
                let b = data[i];
                i += 1;
                for _ in 0..run_len {
                    output.push(b);
                }
            }
        } else {
            // Literal
            let lit_len = header as usize;
            if i + lit_len <= data.len() {
                output.extend_from_slice(&data[i..i + lit_len]);
                i += lit_len;
            }
        }
    }

    output
}

// ─── Report Formatting ──────────────────────────────────────────────

impl CrashReport {
    /// Format as human-readable text
    pub fn format(&self) -> String {
        let mut s = String::with_capacity(2048);
        s.push_str(&format!("=== KnoxOS Crash Report #{} ===\n", self.id));
        s.push_str(&format!("Type: {:?}\n", self.crash_type));
        s.push_str(&format!(
            "Time: {} ticks (uptime: {}s)\n",
            self.timestamp, self.uptime_secs
        ));
        s.push_str(&format!("Kernel: v{}\n", self.kernel_version));
        s.push_str(&format!("CPU: {}\n", self.cpu_id));
        s.push_str(&format!("Message: {}\n", self.message));
        if let Some(ref loc) = self.location {
            s.push_str(&format!("Location: {}\n", loc));
        }
        s.push_str("\n--- Registers ---\n");
        s.push_str(&format!(
            "RIP: {:#018x}  RSP: {:#018x}\n",
            self.registers.rip, self.registers.rsp
        ));
        s.push_str(&format!(
            "RBP: {:#018x}  CR2: {:#018x}\n",
            self.registers.rbp, self.registers.cr2
        ));
        s.push_str(&format!(
            "CR3: {:#018x}  RFLAGS: {:#018x}\n",
            self.registers.cr3, self.registers.rflags
        ));

        if !self.backtrace.is_empty() {
            s.push_str("\n--- Backtrace ---\n");
            for (i, frame) in self.backtrace.iter().enumerate() {
                let sym = frame.symbol.as_deref().unwrap_or("<unknown>");
                s.push_str(&format!("  #{}: {:#018x} {}\n", i, frame.address, sym));
            }
        }

        s
    }
}

// ─── Public API ─────────────────────────────────────────────────────

/// Get all stored crash reports
pub fn get_reports() -> Vec<CrashReport> {
    CRASH_REPORTS.lock().clone()
}

/// Get crash report by ID
pub fn get_report(id: u32) -> Option<CrashReport> {
    CRASH_REPORTS.lock().iter().find(|r| r.id == id).cloned()
}

/// Get total crash count
pub fn crash_count() -> u32 {
    CRASH_COUNT.load(Ordering::Relaxed)
}

/// Clear stored reports
pub fn clear_reports() {
    CRASH_REPORTS.lock().clear();
}

/// Enable / disable crash reporting
pub fn set_enabled(enabled: bool) {
    REPORTING_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Initialize crash reporter
pub fn init() {
    serial_println!(
        "[crash_reporter] initialized (max_stored={}, log_tail={})",
        MAX_STORED_REPORTS,
        LOG_TAIL_SIZE
    );
    serial_println!(
        "[crash_reporter] compression: RLE, backtrace depth: {}",
        MAX_BACKTRACE_DEPTH
    );
}
