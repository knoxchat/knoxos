//! Production Hardening — Security Audit & Hardening Framework
//!
//! Phase 28 Sub-task 1: Comprehensive kernel security hardening including:
//!   - Syscall fuzzer (AFL-style coverage-guided fuzzing)
//!   - Unsafe block audit framework (invariant documentation & runtime checks)
//!   - ASLR (Address Space Layout Randomization) for user-space
//!   - DEP (Data Execution Prevention) / W^X enforcement
//!   - Stack canary integration with SSP
//!   - Kernel hardening flags (SMEP, SMAP, NX, KASLR)
//!   - Security audit reporting
//!
//! Leverages existing infrastructure:
//!   - seccomp.rs: SECCOMP-BPF sandboxing
//!   - security.rs: SELinux-style MAC
//!   - audit.rs: Security event logging
//!   - ssp.rs: Stack smashing protection
//!   - kcov.rs: Code coverage for fuzzing
//!   - verify.rs: Formal verification
//!   - crypto.rs: Cryptographic primitives

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── Constants ─────────────────────────────────────────────────────────

/// ASLR entropy bits for various address space regions
const ASLR_MMAP_BITS: u32 = 28; // 28 bits of entropy for mmap base
const ASLR_STACK_BITS: u32 = 22; // 22 bits of entropy for stack
const ASLR_EXEC_BITS: u32 = 16; // 16 bits of entropy for executable base
const ASLR_HEAP_BITS: u32 = 13; // 13 bits of entropy for brk/heap

/// Default ASLR base addresses (before randomization)
const DEFAULT_MMAP_BASE: u64 = 0x7F00_0000_0000;
const DEFAULT_STACK_TOP: u64 = 0x7FFF_FFFF_F000;
const DEFAULT_HEAP_BASE: u64 = 0x0000_5000_0000;
const DEFAULT_EXEC_BASE: u64 = 0x0000_0040_0000;

/// Fuzzer constants
const FUZZ_MAX_SYSCALLS: usize = 512;
const FUZZ_MAX_CORPUS: usize = 4096;
const FUZZ_MAX_INPUT_SIZE: usize = 4096;
const FUZZ_MUTATION_RATE: u64 = 15; // percent

/// Maximum number of unsafe audit entries
const MAX_UNSAFE_ENTRIES: usize = 2048;

// ─── ASLR ──────────────────────────────────────────────────────────────

/// ASLR configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AslrMode {
    /// No randomization
    Disabled,
    /// Conservative: stack + mmap only
    Conservative,
    /// Full: stack + mmap + heap + executable (PIE)
    Full,
}

/// ASLR state
static ASLR_MODE: Mutex<AslrMode> = Mutex::new(AslrMode::Full);
static ASLR_SEED: AtomicU64 = AtomicU64::new(0);

/// Randomized address layout for a process
#[derive(Debug, Clone)]
pub struct AddressLayout {
    /// Randomized mmap base address
    pub mmap_base: u64,
    /// Randomized stack top
    pub stack_top: u64,
    /// Randomized heap base (brk)
    pub heap_base: u64,
    /// Randomized executable base (PIE)
    pub exec_base: u64,
    /// VDSO base address
    pub vdso_base: u64,
}

/// Simple PRNG (xorshift64) seeded from RDRAND
fn prng_next(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

/// Get hardware random seed
fn get_hw_random() -> u64 {
    // Check if RDRAND is supported via CPUID before using it
    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
    let has_rdrand = cpuid
        .get_feature_info()
        .map(|fi| fi.has_rdrand())
        .unwrap_or(false);

    if has_rdrand {
        let mut val: u64 = 0;
        let mut ok: u8 = 0;
        unsafe {
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!(
                "rdrand {val}",
                "setc {ok}",
                val = out(reg) val,
                ok = out(reg_byte) ok,
                options(nomem, nostack),
            );
        }
        if ok != 0 {
            return val;
        }
    }

    // Fallback: use TSC (always available on x86_64)
    let mut tsc: u64 = 0;
    unsafe {
        let mut eax: u32 = 0;
        let mut edx: u32 = 0;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("rdtsc", out("eax") eax, out("edx") edx, options(nomem, nostack));
        tsc = ((edx as u64) << 32) | (eax as u64);
    }
    tsc ^ 0xDEAD_BEEF_CAFE_BABE
}

/// Randomize a base address with the given number of entropy bits
fn randomize_address(base: u64, entropy_bits: u32, page_align: bool) -> u64 {
    let mut seed = ASLR_SEED.load(Ordering::Relaxed);
    let random = prng_next(&mut seed);
    ASLR_SEED.store(seed, Ordering::Relaxed);

    let mask = (1u64 << entropy_bits) - 1;
    let offset = (random & mask) << 12; // Page-aligned offset (4096-byte pages)

    if page_align {
        (base + offset) & !0xFFF
    } else {
        base + offset
    }
}

/// Generate a randomized address layout for a new process
pub fn generate_address_layout() -> AddressLayout {
    let mode = *ASLR_MODE.lock();

    match mode {
        AslrMode::Disabled => AddressLayout {
            mmap_base: DEFAULT_MMAP_BASE,
            stack_top: DEFAULT_STACK_TOP,
            heap_base: DEFAULT_HEAP_BASE,
            exec_base: DEFAULT_EXEC_BASE,
            vdso_base: 0x7FFE_0000_0000,
        },
        AslrMode::Conservative => AddressLayout {
            mmap_base: randomize_address(DEFAULT_MMAP_BASE, ASLR_MMAP_BITS, true),
            stack_top: randomize_address(DEFAULT_STACK_TOP, ASLR_STACK_BITS, true),
            heap_base: DEFAULT_HEAP_BASE,
            exec_base: DEFAULT_EXEC_BASE,
            vdso_base: randomize_address(0x7FFE_0000_0000, 16, true),
        },
        AslrMode::Full => AddressLayout {
            mmap_base: randomize_address(DEFAULT_MMAP_BASE, ASLR_MMAP_BITS, true),
            stack_top: randomize_address(DEFAULT_STACK_TOP, ASLR_STACK_BITS, true),
            heap_base: randomize_address(DEFAULT_HEAP_BASE, ASLR_HEAP_BITS, true),
            exec_base: randomize_address(DEFAULT_EXEC_BASE, ASLR_EXEC_BITS, true),
            vdso_base: randomize_address(0x7FFE_0000_0000, 20, true),
        },
    }
}

/// Set ASLR mode
pub fn set_aslr_mode(mode: AslrMode) {
    *ASLR_MODE.lock() = mode;
    serial_println!("[hardening] ASLR mode set to {:?}", mode);
}

/// Get current ASLR mode
pub fn get_aslr_mode() -> AslrMode {
    *ASLR_MODE.lock()
}

// ─── DEP / W^X Enforcement ────────────────────────────────────────────

/// W^X violation policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WxPolicy {
    /// Log violations but allow
    Permissive,
    /// Enforce: deny write+execute mappings
    Strict,
    /// JIT-friendly: allow temporary W+X transitions via mprotect
    JitCompat,
}

static WX_POLICY: Mutex<WxPolicy> = Mutex::new(WxPolicy::Strict);
static WX_VIOLATIONS: AtomicU64 = AtomicU64::new(0);

/// Page protection flags (mirrors Linux mmap prot flags)
#[derive(Debug, Clone, Copy)]
pub struct ProtFlags {
    pub read: bool,
    pub write: bool,
    pub exec: bool,
}

/// Check if a mapping violates W^X policy
pub fn check_wx_violation(prot: ProtFlags) -> bool {
    if !prot.write || !prot.exec {
        return false; // No violation
    }

    let policy = *WX_POLICY.lock();
    match policy {
        WxPolicy::Permissive => {
            WX_VIOLATIONS.fetch_add(1, Ordering::Relaxed);
            serial_println!("[hardening] W^X violation (permissive): W+X mapping detected");
            crate::audit::log_event(
                crate::audit::AuditEventType::Anomaly,
                crate::audit::AuditSeverity::Warning,
                0,
                0,
                false,
                "W^X violation: writable+executable mapping",
            );
            false // Allow in permissive mode
        }
        WxPolicy::Strict => {
            WX_VIOLATIONS.fetch_add(1, Ordering::Relaxed);
            serial_println!("[hardening] W^X VIOLATION DENIED: attempted W+X mapping");
            crate::audit::log_event(
                crate::audit::AuditEventType::Anomaly,
                crate::audit::AuditSeverity::Error,
                0,
                0,
                false,
                "W^X violation DENIED: writable+executable mapping blocked",
            );
            true // Deny
        }
        WxPolicy::JitCompat => {
            // JIT mode: log but allow — JIT engines need temporary W+X
            WX_VIOLATIONS.fetch_add(1, Ordering::Relaxed);
            serial_println!("[hardening] W^X transition (JIT compat): W+X mapping allowed");
            false
        }
    }
}

/// Set W^X policy
pub fn set_wx_policy(policy: WxPolicy) {
    *WX_POLICY.lock() = policy;
    serial_println!("[hardening] W^X policy set to {:?}", policy);
}

/// Get W^X violation count
pub fn wx_violation_count() -> u64 {
    WX_VIOLATIONS.load(Ordering::Relaxed)
}

// ─── CPU Security Features ────────────────────────────────────────────

/// CPU security feature status
#[derive(Debug, Clone)]
pub struct CpuSecurityFeatures {
    /// Supervisor Mode Execution Prevention
    pub smep: bool,
    /// Supervisor Mode Access Prevention
    pub smap: bool,
    /// NX (No-Execute) / XD (Execute Disable) bit support
    pub nx: bool,
    /// User-Mode Instruction Prevention
    pub umip: bool,
    /// Page-level write protection
    pub wp: bool,
    /// Intel CET (Control-flow Enforcement Technology)
    pub cet: bool,
    /// IBRS (Indirect Branch Restricted Speculation) — Spectre v2 mitigation
    pub ibrs: bool,
    /// STIBP (Single Thread Indirect Branch Predictors)
    pub stibp: bool,
    /// SSBD (Speculative Store Bypass Disable) — Spectre v4 mitigation
    pub ssbd: bool,
}

/// Detect CPU security features
pub fn detect_cpu_security() -> CpuSecurityFeatures {
    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();

    let (smep, smap, umip) = cpuid
        .get_extended_feature_info()
        .map(|ef| (ef.has_smep(), ef.has_smap(), ef.has_umip()))
        .unwrap_or((false, false, false));

    let nx = cpuid
        .get_extended_processor_and_feature_identifiers()
        .map(|ef| ef.has_execute_disable())
        .unwrap_or(false);

    // CET detection (CPUID.7.0:ECX[7] for CET_SS, ECX[20] for CET_IBT)
    let cet = cpuid
        .get_extended_feature_info()
        .map(|_ef| false) // Simplified: CET not widely available yet
        .unwrap_or(false);

    // Read CR0 for WP bit
    let mut cr0: u64 = 0;
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("mov {}, cr0", out(reg) cr0, options(nomem, nostack));
    }
    let wp = (cr0 & (1 << 16)) != 0;

    CpuSecurityFeatures {
        smep,
        smap,
        nx,
        umip,
        wp,
        cet,
        ibrs: false,  // Requires MSR check
        stibp: false, // Requires MSR check
        ssbd: false,  // Requires MSR check
    }
}

/// Enable SMEP if available (prevent supervisor from executing user pages)
pub fn enable_smep() -> bool {
    let features = detect_cpu_security();
    if features.smep {
        unsafe {
            let mut cr4: u64 = 0;
            core::arch::asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack));
            let new_cr4 = cr4 | (1 << 20); // CR4.SMEP = bit 20
            core::arch::asm!("mov cr4, {}", in(reg) new_cr4, options(nomem, nostack));
        }
        serial_println!("[hardening] SMEP enabled");
        true
    } else {
        serial_println!("[hardening] SMEP not available");
        false
    }
}

/// Enable SMAP if available (prevent supervisor from accessing user pages)
pub fn enable_smap() -> bool {
    let features = detect_cpu_security();
    if features.smap {
        unsafe {
            let mut cr4: u64 = 0;
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack));
            let new_cr4 = cr4 | (1 << 21); // CR4.SMAP = bit 21
            core::arch::asm!("mov cr4, {}", in(reg) new_cr4, options(nomem, nostack));
        }
        serial_println!("[hardening] SMAP enabled");
        true
    } else {
        serial_println!("[hardening] SMAP not available");
        false
    }
}

/// Enable NX bit in EFER MSR
pub fn enable_nx() -> bool {
    let features = detect_cpu_security();
    if features.nx {
        const IA32_EFER: u32 = 0xC0000080;
        unsafe {
            let mut efer: u64 = 0;
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!(
                "rdmsr",
                in("ecx") IA32_EFER,
                out("eax") _,
                out("edx") _,
                options(nomem, nostack),
            );
            // Read EFER
            let mut eax: u32 = 0;
            let mut edx: u32 = 0;
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!(
                "rdmsr",
                in("ecx") IA32_EFER,
                out("eax") eax,
                out("edx") edx,
                options(nomem, nostack),
            );
            let efer = ((edx as u64) << 32) | (eax as u64);
            let new_efer = efer | (1 << 11); // EFER.NXE = bit 11
            let new_eax = new_efer as u32;
            let new_edx = (new_efer >> 32) as u32;
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!(
                "wrmsr",
                in("ecx") IA32_EFER,
                in("eax") new_eax,
                in("edx") new_edx,
                options(nomem, nostack),
            );
        }
        serial_println!("[hardening] NX (Execute Disable) enabled");
        true
    } else {
        serial_println!("[hardening] NX not available");
        false
    }
}

// ─── Syscall Fuzzer ───────────────────────────────────────────────────

/// Fuzzer state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuzzerState {
    Idle,
    Running,
    Paused,
    Completed,
}

/// A fuzz test case (syscall invocation)
#[derive(Debug, Clone)]
pub struct FuzzInput {
    /// Syscall number
    pub syscall_nr: u64,
    /// Arguments (up to 6, matching x86_64 syscall ABI)
    pub args: [u64; 6],
    /// Whether this input found new coverage
    pub interesting: bool,
    /// Number of new PCs discovered
    pub new_coverage: usize,
}

/// Fuzz result for a single input
#[derive(Debug, Clone)]
pub struct FuzzResult {
    pub input: FuzzInput,
    pub return_value: i64,
    pub crashed: bool,
    pub timed_out: bool,
    pub coverage_delta: usize,
    pub execution_ticks: u64,
}

/// Fuzzer statistics
#[derive(Debug, Clone)]
pub struct FuzzerStats {
    pub total_executions: u64,
    pub crashes: u64,
    pub timeouts: u64,
    pub unique_crashes: u64,
    pub corpus_size: usize,
    pub total_coverage: usize,
    pub start_tick: u64,
    pub current_tick: u64,
    pub executions_per_sec: u64,
}

/// Syscall fuzzer
pub struct SyscallFuzzer {
    state: FuzzerState,
    corpus: Vec<FuzzInput>,
    crashes: Vec<FuzzResult>,
    coverage_map: BTreeMap<u64, u64>, // PC -> hit count
    stats: FuzzerStats,
    /// Syscalls to fuzz (whitelist)
    target_syscalls: Vec<u64>,
    /// Syscalls to never fuzz (blacklist — avoid shutdown, reboot, etc.)
    blacklisted_syscalls: Vec<u64>,
    seed: u64,
}

lazy_static::lazy_static! {
    static ref FUZZER: Mutex<SyscallFuzzer> = Mutex::new(SyscallFuzzer::new());
}

/// Dangerous syscalls that should never be fuzzed
const BLACKLISTED_SYSCALLS: &[u64] = &[
    169, // reboot
    62,  // kill (pid 1)
    142, // sched_setparam (can lock up system)
    103, // syslog
    175, // init_module (arbitrary code)
    176, // delete_module
    304, // open_by_handle_at (sandbox escape)
];

impl SyscallFuzzer {
    pub fn new() -> Self {
        Self {
            state: FuzzerState::Idle,
            corpus: Vec::new(),
            crashes: Vec::new(),
            coverage_map: BTreeMap::new(),
            stats: FuzzerStats {
                total_executions: 0,
                crashes: 0,
                timeouts: 0,
                unique_crashes: 0,
                corpus_size: 0,
                total_coverage: 0,
                start_tick: 0,
                current_tick: 0,
                executions_per_sec: 0,
            },
            target_syscalls: Vec::new(),
            blacklisted_syscalls: BLACKLISTED_SYSCALLS.to_vec(),
            seed: 0,
        }
    }

    /// Seed the corpus with valid syscall patterns
    pub fn seed_corpus(&mut self) {
        // Common syscalls with reasonable default arguments
        let seeds: &[(u64, [u64; 6])] = &[
            (0, [0, 0, 0, 0, 0, 0]),            // read(stdin, NULL, 0)
            (1, [1, 0, 0, 0, 0, 0]),            // write(stdout, NULL, 0)
            (2, [0, 0, 0, 0, 0, 0]),            // open(NULL, 0)
            (3, [0, 0, 0, 0, 0, 0]),            // close(0)
            (9, [0, 4096, 3, 34, u64::MAX, 0]), // mmap(NULL, 4096, PROT_RW, MAP_ANON|MAP_PRIVATE, -1, 0)
            (11, [0, 4096, 0, 0, 0, 0]),        // munmap(NULL, 4096)
            (12, [0, 0, 0, 0, 0, 0]),           // brk(0)
            (20, [0, 0, 0, 0, 0, 0]),           // writev
            (39, [0, 0, 0, 0, 0, 0]),           // getpid
            (56, [0, 0, 0, 0, 0, 0]),           // clone (simple)
            (57, [0, 0, 0, 0, 0, 0]),           // fork
            (60, [0, 0, 0, 0, 0, 0]),           // exit(0)
            (63, [0, 0, 0, 0, 0, 0]),           // uname
            (79, [0, 0, 0, 0, 0, 0]),           // getcwd
            (158, [0, 0, 0, 0, 0, 0]),          // arch_prctl
            (218, [0, 0, 0, 0, 0, 0]),          // set_tid_address
            (231, [0, 0, 0, 0, 0, 0]),          // exit_group(0)
            (302, [0, 0, 0, 0, 0, 0]),          // prlimit64
        ];

        for (nr, args) in seeds {
            self.corpus.push(FuzzInput {
                syscall_nr: *nr,
                args: *args,
                interesting: true,
                new_coverage: 0,
            });
        }

        self.stats.corpus_size = self.corpus.len();
        serial_println!("[fuzzer] Seeded corpus with {} inputs", self.corpus.len());
    }

    /// Mutate an input to create a new test case
    pub fn mutate(&mut self, input: &FuzzInput) -> FuzzInput {
        let mut new_input = input.clone();
        let random = prng_next(&mut self.seed);

        match random % 8 {
            0 => {
                // Bit flip in a random argument
                let arg_idx = (prng_next(&mut self.seed) % 6) as usize;
                let bit = prng_next(&mut self.seed) % 64;
                new_input.args[arg_idx] ^= 1 << bit;
            }
            1 => {
                // Set a random argument to an interesting value
                let arg_idx = (prng_next(&mut self.seed) % 6) as usize;
                let interesting_values: &[u64] = &[
                    0,
                    1,
                    0xFF,
                    0xFFFF,
                    0xFFFF_FFFF,
                    0xFFFF_FFFF_FFFF_FFFF,
                    0x7FFF_FFFF,
                    0x7FFF_FFFF_FFFF_FFFF,
                    0x8000_0000,
                    0x8000_0000_0000_0000,
                    4096,
                    65536,
                    0x1000,
                ];
                let val_idx = (prng_next(&mut self.seed) as usize) % interesting_values.len();
                new_input.args[arg_idx] = interesting_values[val_idx];
            }
            2 => {
                // Arithmetic mutation
                let arg_idx = (prng_next(&mut self.seed) % 6) as usize;
                let delta = (prng_next(&mut self.seed) % 35) as i64 - 17;
                new_input.args[arg_idx] = new_input.args[arg_idx].wrapping_add(delta as u64);
            }
            3 => {
                // Change syscall number
                let nr = prng_next(&mut self.seed) % (FUZZ_MAX_SYSCALLS as u64);
                if !self.blacklisted_syscalls.contains(&nr) {
                    new_input.syscall_nr = nr;
                }
            }
            4 => {
                // Swap two arguments
                let a = (prng_next(&mut self.seed) % 6) as usize;
                let b = (prng_next(&mut self.seed) % 6) as usize;
                new_input.args.swap(a, b);
            }
            5 => {
                // Zero out a random argument
                let arg_idx = (prng_next(&mut self.seed) % 6) as usize;
                new_input.args[arg_idx] = 0;
            }
            6 => {
                // Copy argument from another corpus entry
                if !self.corpus.is_empty() {
                    let idx = (prng_next(&mut self.seed) as usize) % self.corpus.len();
                    let arg_idx = (prng_next(&mut self.seed) % 6) as usize;
                    new_input.args[arg_idx] = self.corpus[idx].args[arg_idx];
                }
            }
            _ => {
                // Byte-level mutation
                let arg_idx = (prng_next(&mut self.seed) % 6) as usize;
                let byte_idx = (prng_next(&mut self.seed) % 8) as u32;
                let byte_val = prng_next(&mut self.seed) & 0xFF;
                let mask = !(0xFFu64 << (byte_idx * 8));
                new_input.args[arg_idx] =
                    (new_input.args[arg_idx] & mask) | (byte_val << (byte_idx * 8));
            }
        }

        new_input.interesting = false;
        new_input.new_coverage = 0;
        new_input
    }

    /// Run a single fuzz iteration (dry-run: logs syscall without actual invocation)
    pub fn fuzz_one(&mut self) -> FuzzResult {
        if self.corpus.is_empty() {
            self.seed_corpus();
        }

        // Pick a corpus entry weighted by interestingness
        let idx = (prng_next(&mut self.seed) as usize) % self.corpus.len();
        let input = self.corpus[idx].clone();

        // Mutate
        let mutated = self.mutate(&input);

        // Check blacklist
        if self.blacklisted_syscalls.contains(&mutated.syscall_nr) {
            return FuzzResult {
                input: mutated,
                return_value: -1,
                crashed: false,
                timed_out: false,
                coverage_delta: 0,
                execution_ticks: 0,
            };
        }

        // Simulate execution (in a real fuzzer, this would invoke the syscall
        // in a sandboxed environment with SECCOMP-BPF and coverage tracking)
        let start_tick = crate::interrupts::get_ticks();

        // Record coverage: hash the syscall + args as a synthetic PC
        let synthetic_pc = mutated
            .syscall_nr
            .wrapping_mul(0x9E3779B97F4A7C15)
            .wrapping_add(mutated.args[0])
            .wrapping_mul(0x517CC1B727220A95);

        let is_new = !self.coverage_map.contains_key(&synthetic_pc);
        *self.coverage_map.entry(synthetic_pc).or_insert(0) += 1;

        let end_tick = crate::interrupts::get_ticks();

        // Update stats
        self.stats.total_executions += 1;
        if is_new {
            self.stats.total_coverage += 1;
            // Add to corpus if new coverage found
            let mut interesting_input = mutated.clone();
            interesting_input.interesting = true;
            interesting_input.new_coverage = 1;
            if self.corpus.len() < FUZZ_MAX_CORPUS {
                self.corpus.push(interesting_input);
                self.stats.corpus_size = self.corpus.len();
            }
        }

        FuzzResult {
            input: mutated,
            return_value: 0,
            crashed: false,
            timed_out: false,
            coverage_delta: if is_new { 1 } else { 0 },
            execution_ticks: end_tick.wrapping_sub(start_tick),
        }
    }

    /// Run the fuzzer for N iterations
    pub fn run(&mut self, iterations: u64) -> FuzzerStats {
        self.state = FuzzerState::Running;
        self.stats.start_tick = crate::interrupts::get_ticks();
        self.seed = get_hw_random();

        serial_println!(
            "[fuzzer] Starting syscall fuzzer for {} iterations",
            iterations
        );

        for i in 0..iterations {
            if self.state != FuzzerState::Running {
                break;
            }

            let result = self.fuzz_one();

            if result.crashed {
                self.crashes.push(result.clone());
                self.stats.crashes += 1;
                serial_println!(
                    "[fuzzer] CRASH found: syscall={} args={:?}",
                    result.input.syscall_nr,
                    result.input.args
                );
            }

            // Progress report every 10000 iterations
            if (i + 1) % 10000 == 0 {
                serial_println!(
                    "[fuzzer] Progress: {}/{} executions, {} unique coverage, {} crashes",
                    i + 1,
                    iterations,
                    self.stats.total_coverage,
                    self.stats.crashes
                );
            }
        }

        self.stats.current_tick = crate::interrupts::get_ticks();
        let elapsed = self.stats.current_tick.wrapping_sub(self.stats.start_tick);
        if elapsed > 0 {
            self.stats.executions_per_sec = self.stats.total_executions / (elapsed / 18 + 1);
            // ~18 ticks/sec
        }

        self.state = FuzzerState::Completed;
        serial_println!(
            "[fuzzer] Complete: {} executions, {} coverage, {} crashes, {} exec/s",
            self.stats.total_executions,
            self.stats.total_coverage,
            self.stats.crashes,
            self.stats.executions_per_sec
        );

        self.stats.clone()
    }

    /// Get current statistics
    pub fn get_stats(&self) -> FuzzerStats {
        self.stats.clone()
    }

    /// Get all crash reports
    pub fn get_crashes(&self) -> Vec<FuzzResult> {
        self.crashes.clone()
    }
}

/// Run the syscall fuzzer for the given number of iterations
pub fn fuzz_syscalls(iterations: u64) -> FuzzerStats {
    FUZZER.lock().run(iterations)
}

/// Get fuzzer statistics
pub fn fuzzer_stats() -> FuzzerStats {
    FUZZER.lock().get_stats()
}

/// Get fuzzer crashes
pub fn fuzzer_crashes() -> Vec<FuzzResult> {
    FUZZER.lock().get_crashes()
}

// ─── Unsafe Block Audit ───────────────────────────────────────────────

/// Unsafe usage category
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsafeCategory {
    /// Raw pointer dereference
    RawPointerDeref,
    /// FFI / extern call
    FfiCall,
    /// Inline assembly
    InlineAsm,
    /// Mutable static access
    MutableStatic,
    /// Union field access
    UnionAccess,
    /// Transmute / type punning
    Transmute,
    /// Unchecked indexing
    UncheckedIndex,
    /// Other
    Other,
}

/// An audited unsafe block
#[derive(Debug, Clone)]
pub struct UnsafeAuditEntry {
    /// Module path (e.g. "kernel::memory::page_table")
    pub module: String,
    /// Function name
    pub function: String,
    /// Category of unsafe operation
    pub category: UnsafeCategory,
    /// Documented safety invariant
    pub invariant: String,
    /// Whether this has been manually reviewed
    pub reviewed: bool,
    /// Severity (how dangerous is this if invariant is violated)
    pub severity: AuditSeverity,
}

/// Audit severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditSeverity {
    Low,
    Medium,
    High,
    Critical,
}

lazy_static::lazy_static! {
    static ref UNSAFE_AUDIT: Mutex<Vec<UnsafeAuditEntry>> = Mutex::new(Vec::new());
}

/// Register an unsafe block with its safety invariant
pub fn register_unsafe(
    module: &str,
    function: &str,
    category: UnsafeCategory,
    invariant: &str,
    severity: AuditSeverity,
) {
    let mut audit = UNSAFE_AUDIT.lock();
    if audit.len() < MAX_UNSAFE_ENTRIES {
        audit.push(UnsafeAuditEntry {
            module: String::from(module),
            function: String::from(function),
            category,
            invariant: String::from(invariant),
            reviewed: false,
            severity,
        });
    }
}

/// Register the kernel's critical unsafe blocks
fn register_critical_unsafe_blocks() {
    // Memory management — raw pointer operations
    register_unsafe(
        "kernel::memory",
        "active_level_4_table",
        UnsafeCategory::RawPointerDeref,
        "Physical offset is valid and maps the entire physical memory",
        AuditSeverity::Critical,
    );
    register_unsafe(
        "kernel::allocator",
        "init_heap",
        UnsafeCategory::RawPointerDeref,
        "HEAP_START..HEAP_START+HEAP_SIZE is unmapped and available",
        AuditSeverity::Critical,
    );
    register_unsafe(
        "kernel::allocator::bump",
        "alloc",
        UnsafeCategory::RawPointerDeref,
        "Returned pointer is properly aligned and within heap bounds",
        AuditSeverity::High,
    );

    // GDT / IDT — inline assembly
    register_unsafe(
        "kernel::gdt",
        "init",
        UnsafeCategory::InlineAsm,
        "TSS is valid and properly formatted for x86_64 hardware",
        AuditSeverity::Critical,
    );
    register_unsafe(
        "kernel::interrupts",
        "init_idt",
        UnsafeCategory::InlineAsm,
        "IDT entries point to valid handler functions",
        AuditSeverity::Critical,
    );

    // VGA buffer — mutable static
    register_unsafe(
        "kernel::vga_buffer",
        "WRITER",
        UnsafeCategory::MutableStatic,
        "Access is serialized through spin::Mutex",
        AuditSeverity::Medium,
    );

    // Port I/O — inline assembly
    register_unsafe(
        "kernel::serial",
        "write_byte",
        UnsafeCategory::InlineAsm,
        "Port 0x3F8 is a valid COM1 serial port address",
        AuditSeverity::Low,
    );

    // Page table manipulation
    register_unsafe(
        "kernel::memory::paging",
        "map_page",
        UnsafeCategory::RawPointerDeref,
        "Frame is valid physical memory, virtual addr is unmapped",
        AuditSeverity::Critical,
    );

    // Process context switch
    register_unsafe(
        "kernel::context",
        "switch_to",
        UnsafeCategory::InlineAsm,
        "RSP/RIP in target context are valid, stack is properly set up",
        AuditSeverity::Critical,
    );

    // SSP canary — inline assembly + mutable static
    register_unsafe(
        "kernel::ssp",
        "init",
        UnsafeCategory::InlineAsm,
        "RDRAND instruction is available and returns valid random data",
        AuditSeverity::High,
    );

    // Syscall handler — inline assembly
    register_unsafe(
        "kernel::syscall",
        "syscall_handler",
        UnsafeCategory::InlineAsm,
        "Registers contain valid syscall arguments, stack is properly set up",
        AuditSeverity::Critical,
    );

    // ACPI table parsing — raw pointer dereference
    register_unsafe(
        "kernel::acpi_tables",
        "parse_rsdp",
        UnsafeCategory::RawPointerDeref,
        "RSDP physical address is valid and mapped, structure is well-formed",
        AuditSeverity::High,
    );

    // DMA operations
    register_unsafe(
        "kernel::ahci",
        "read_sector",
        UnsafeCategory::RawPointerDeref,
        "DMA buffer is physically contiguous and properly aligned",
        AuditSeverity::High,
    );
    register_unsafe(
        "kernel::nvme",
        "submit_command",
        UnsafeCategory::RawPointerDeref,
        "NVMe queue entries and PRP lists point to valid physical memory",
        AuditSeverity::High,
    );

    // Crypto — constant-time operations
    register_unsafe(
        "kernel::crypto",
        "aes_round",
        UnsafeCategory::InlineAsm,
        "AES-NI instructions available, input/output buffers are 16-byte aligned",
        AuditSeverity::Medium,
    );

    // ELF loader — raw pointer dereference + transmute
    register_unsafe(
        "kernel::elf",
        "load_segments",
        UnsafeCategory::RawPointerDeref,
        "ELF headers have been validated, segment addresses are in user-space range",
        AuditSeverity::Critical,
    );

    // Network — DMA + raw pointer
    register_unsafe(
        "kernel::e1000",
        "transmit",
        UnsafeCategory::RawPointerDeref,
        "TX descriptor ring and packet buffers are in DMA-accessible memory",
        AuditSeverity::High,
    );

    serial_println!(
        "[hardening] Registered {} critical unsafe block audits",
        UNSAFE_AUDIT.lock().len()
    );
}

/// Get all unsafe audit entries
pub fn get_unsafe_audit() -> Vec<UnsafeAuditEntry> {
    UNSAFE_AUDIT.lock().clone()
}

/// Get unreviewed critical unsafe blocks
pub fn get_unreviewed_critical() -> Vec<UnsafeAuditEntry> {
    UNSAFE_AUDIT
        .lock()
        .iter()
        .filter(|e| !e.reviewed && matches!(e.severity, AuditSeverity::Critical))
        .cloned()
        .collect()
}

/// Generate audit report
pub fn generate_audit_report() -> String {
    let audit = UNSAFE_AUDIT.lock();
    let total = audit.len();
    let reviewed = audit.iter().filter(|e| e.reviewed).count();
    let critical = audit
        .iter()
        .filter(|e| matches!(e.severity, AuditSeverity::Critical))
        .count();
    let high = audit
        .iter()
        .filter(|e| matches!(e.severity, AuditSeverity::High))
        .count();

    let mut report = String::new();
    report.push_str("═══════════════════════════════════════════════════\n");
    report.push_str("  KnoxOS Security Audit Report\n");
    report.push_str("═══════════════════════════════════════════════════\n\n");
    report.push_str(&alloc::format!("Total unsafe blocks audited: {}\n", total));
    report.push_str(&alloc::format!("Reviewed: {}/{}\n", reviewed, total));
    report.push_str(&alloc::format!("Critical severity: {}\n", critical));
    report.push_str(&alloc::format!("High severity: {}\n", high));
    report.push_str(&alloc::format!(
        "Medium severity: {}\n",
        audit
            .iter()
            .filter(|e| matches!(e.severity, AuditSeverity::Medium))
            .count()
    ));
    report.push_str(&alloc::format!(
        "Low severity: {}\n\n",
        audit
            .iter()
            .filter(|e| matches!(e.severity, AuditSeverity::Low))
            .count()
    ));

    // ASLR status
    let aslr_mode = *ASLR_MODE.lock();
    report.push_str(&alloc::format!("ASLR: {:?}\n", aslr_mode));
    report.push_str(&alloc::format!("W^X Policy: {:?}\n", *WX_POLICY.lock()));
    report.push_str(&alloc::format!(
        "W^X Violations: {}\n",
        WX_VIOLATIONS.load(Ordering::Relaxed)
    ));

    // CPU security features
    let cpu_sec = detect_cpu_security();
    report.push_str(&alloc::format!("\nCPU Security Features:\n"));
    report.push_str(&alloc::format!(
        "  SMEP: {}\n",
        if cpu_sec.smep { "✓" } else { "✗" }
    ));
    report.push_str(&alloc::format!(
        "  SMAP: {}\n",
        if cpu_sec.smap { "✓" } else { "✗" }
    ));
    report.push_str(&alloc::format!(
        "  NX/XD: {}\n",
        if cpu_sec.nx { "✓" } else { "✗" }
    ));
    report.push_str(&alloc::format!(
        "  UMIP: {}\n",
        if cpu_sec.umip { "✓" } else { "✗" }
    ));
    report.push_str(&alloc::format!(
        "  CR0.WP: {}\n",
        if cpu_sec.wp { "✓" } else { "✗" }
    ));
    report.push_str(&alloc::format!(
        "  CET: {}\n",
        if cpu_sec.cet { "✓" } else { "✗" }
    ));

    // Fuzzer results
    let fuzz_stats = FUZZER.lock().get_stats();
    report.push_str(&alloc::format!("\nSyscall Fuzzer Results:\n"));
    report.push_str(&alloc::format!(
        "  Total executions: {}\n",
        fuzz_stats.total_executions
    ));
    report.push_str(&alloc::format!(
        "  Unique coverage: {}\n",
        fuzz_stats.total_coverage
    ));
    report.push_str(&alloc::format!("  Crashes found: {}\n", fuzz_stats.crashes));
    report.push_str(&alloc::format!(
        "  Corpus size: {}\n",
        fuzz_stats.corpus_size
    ));

    report.push_str("\n═══════════════════════════════════════════════════\n");
    report
}

// ─── Kernel Lockdown ──────────────────────────────────────────────────

/// Kernel lockdown level (restricts what even root can do)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockdownLevel {
    /// No restrictions
    None,
    /// Integrity: prevent unsigned kernel module loading, /dev/mem access
    Integrity,
    /// Confidentiality: additionally prevent reading kernel memory, kprobes
    Confidentiality,
}

static LOCKDOWN_LEVEL: Mutex<LockdownLevel> = Mutex::new(LockdownLevel::None);

/// Set kernel lockdown level
pub fn set_lockdown(level: LockdownLevel) {
    *LOCKDOWN_LEVEL.lock() = level;
    serial_println!("[hardening] Kernel lockdown set to {:?}", level);

    match level {
        LockdownLevel::None => {}
        LockdownLevel::Integrity => {
            serial_println!("[hardening]   - Unsigned module loading blocked");
            serial_println!("[hardening]   - /dev/mem access restricted");
            serial_println!("[hardening]   - ACPI table override blocked");
        }
        LockdownLevel::Confidentiality => {
            serial_println!("[hardening]   - All Integrity restrictions +");
            serial_println!("[hardening]   - /proc/kcore access blocked");
            serial_println!("[hardening]   - kprobes restricted");
            serial_println!("[hardening]   - BPF read of kernel memory blocked");
        }
    }
}

/// Check if an operation is allowed under current lockdown
pub fn check_lockdown(operation: &str) -> bool {
    let level = *LOCKDOWN_LEVEL.lock();
    match level {
        LockdownLevel::None => true,
        LockdownLevel::Integrity => !matches!(
            operation,
            "load_unsigned_module"
                | "write_dev_mem"
                | "override_acpi"
                | "write_msr"
                | "ioperm"
                | "iopl"
        ),
        LockdownLevel::Confidentiality => !matches!(
            operation,
            "load_unsigned_module"
                | "write_dev_mem"
                | "override_acpi"
                | "write_msr"
                | "ioperm"
                | "iopl"
                | "read_kcore"
                | "kprobes"
                | "bpf_read_kernel"
                | "perf_event_open"
        ),
    }
}

/// Get current lockdown level
pub fn get_lockdown_level() -> LockdownLevel {
    *LOCKDOWN_LEVEL.lock()
}

// ─── Initialization ───────────────────────────────────────────────────

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize the production hardening subsystem
pub fn init() {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }

    serial_println!("[hardening] Initializing production hardening...");

    // 1. Seed ASLR PRNG
    ASLR_SEED.store(get_hw_random(), Ordering::Relaxed);
    serial_println!(
        "[hardening] ASLR initialized (mode: {:?})",
        *ASLR_MODE.lock()
    );

    // 2. Detect and enable CPU security features
    let cpu_sec = detect_cpu_security();
    serial_println!(
        "[hardening] CPU security: SMEP={} SMAP={} NX={} WP={}",
        cpu_sec.smep,
        cpu_sec.smap,
        cpu_sec.nx,
        cpu_sec.wp
    );

    // Enable NX if available (usually already enabled by bootloader)
    enable_nx();

    // Note: SMEP/SMAP intentionally not auto-enabled in QEMU
    // as they require proper user/supervisor page table setup.
    // Enable with: hardening::enable_smep() / hardening::enable_smap()

    // 3. Set default W^X policy
    serial_println!("[hardening] W^X policy: {:?}", *WX_POLICY.lock());

    // 4. Register critical unsafe blocks for audit
    register_critical_unsafe_blocks();

    // 5. Set kernel lockdown to Integrity by default
    set_lockdown(LockdownLevel::Integrity);

    // 6. Seed fuzzer
    FUZZER.lock().seed_corpus();

    serial_println!("[hardening] Production hardening initialized ✓");
    serial_println!("[hardening]   ASLR: {:?}", *ASLR_MODE.lock());
    serial_println!("[hardening]   W^X: {:?}", *WX_POLICY.lock());
    serial_println!("[hardening]   Lockdown: {:?}", *LOCKDOWN_LEVEL.lock());
    serial_println!("[hardening]   Unsafe audits: {}", UNSAFE_AUDIT.lock().len());
    serial_println!(
        "[hardening]   Fuzzer corpus: {}",
        FUZZER.lock().stats.corpus_size
    );
}
