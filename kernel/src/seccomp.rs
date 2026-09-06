/// Seccomp-BPF Syscall Filtering — Linux-compatible secure computing mode
/// Implements seccomp(2) with BPF-based syscall filtering for sandboxing
///
/// Modes:
///   - SECCOMP_MODE_STRICT: Only read/write/exit/sigreturn allowed
///   - SECCOMP_MODE_FILTER: BPF program decides per-syscall
///
/// Actions:
///   - SECCOMP_RET_ALLOW:  Allow the syscall
///   - SECCOMP_RET_KILL:   Kill the process (SIGSYS)
///   - SECCOMP_RET_TRAP:   Send SIGSYS with additional info
///   - SECCOMP_RET_ERRNO:  Return specified errno
///   - SECCOMP_RET_LOG:    Allow but log the syscall
///   - SECCOMP_RET_TRACE:  Notify ptrace tracer
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

use crate::process::Pid;
use crate::serial_println;

/// Seccomp mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeccompMode {
    /// No seccomp filtering
    Disabled,
    /// Strict mode: only read(0), write(1), exit(60), exit_group(231), sigreturn(15)
    Strict,
    /// Filter mode: BPF program decides
    Filter,
}

/// Seccomp BPF instruction (simplified)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct BpfInsn {
    /// Instruction opcode
    pub code: u16,
    /// Jump targets
    pub jt: u8,
    pub jf: u8,
    /// Constant/offset
    pub k: u32,
}

/// BPF opcodes
pub const BPF_LD: u16 = 0x00;
pub const BPF_JMP: u16 = 0x05;
pub const BPF_RET: u16 = 0x06;
pub const BPF_W: u16 = 0x00;
pub const BPF_ABS: u16 = 0x20;
pub const BPF_JEQ: u16 = 0x10;
pub const BPF_K: u16 = 0x00;

/// Seccomp return actions
pub const SECCOMP_RET_KILL_PROCESS: u32 = 0x80000000;
pub const SECCOMP_RET_KILL_THREAD: u32 = 0x00000000;
pub const SECCOMP_RET_TRAP: u32 = 0x00030000;
pub const SECCOMP_RET_ERRNO: u32 = 0x00050000;
pub const SECCOMP_RET_LOG: u32 = 0x7ffc0000;
pub const SECCOMP_RET_ALLOW: u32 = 0x7fff0000;

/// Seccomp data structure passed to BPF (matches Linux struct seccomp_data)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SeccompData {
    /// Syscall number
    pub nr: u32,
    /// CPU architecture (AUDIT_ARCH_X86_64 = 0xC000003E)
    pub arch: u32,
    /// Instruction pointer at time of syscall
    pub instruction_pointer: u64,
    /// Syscall arguments
    pub args: [u64; 6],
}

/// Offset of fields in SeccompData for BPF ABS loads
pub const SECCOMP_DATA_NR: u32 = 0;
pub const SECCOMP_DATA_ARCH: u32 = 4;
pub const SECCOMP_DATA_ARGS: u32 = 16;

/// Per-process seccomp state
#[derive(Debug, Clone)]
pub struct SeccompState {
    pub mode: SeccompMode,
    /// BPF filter program (for Filter mode)
    pub filters: Vec<Vec<BpfInsn>>,
    /// Cached allowed syscalls for fast lookup (optimization)
    pub allowed_cache: Vec<u64>,
    /// Log violations
    pub log_violations: bool,
    /// Count of filtered syscalls
    pub filter_count: u64,
}

impl Default for SeccompState {
    fn default() -> Self {
        Self::new()
    }
}

impl SeccompState {
    pub fn new() -> Self {
        Self {
            mode: SeccompMode::Disabled,
            filters: Vec::new(),
            allowed_cache: Vec::new(),
            log_violations: false,
            filter_count: 0,
        }
    }

    /// Set strict mode — only read/write/exit/exit_group/sigreturn
    pub fn set_strict(&mut self) {
        self.mode = SeccompMode::Strict;
        serial_println!("[seccomp] Strict mode enabled");
    }

    /// Add a BPF filter program
    pub fn add_filter(&mut self, program: Vec<BpfInsn>) -> Result<(), i32> {
        if program.is_empty() || program.len() > 4096 {
            return Err(-22); // EINVAL
        }
        // Validate the program (basic checks)
        if !validate_bpf(&program) {
            return Err(-22);
        }
        self.filters.push(program);
        if self.mode == SeccompMode::Disabled {
            self.mode = SeccompMode::Filter;
        }
        serial_println!("[seccomp] Filter added ({} total)", self.filters.len());
        Ok(())
    }
}

/// Validate a BPF program (simplified validation)
fn validate_bpf(program: &[BpfInsn]) -> bool {
    if program.is_empty() {
        return false;
    }
    // Must end with a RET instruction
    let last = &program[program.len() - 1];
    if last.code & 0x07 != BPF_RET {
        return false;
    }
    // Check all jump targets are in bounds
    for (i, insn) in program.iter().enumerate() {
        if insn.code & 0x07 == BPF_JMP {
            let jt_target = i + 1 + insn.jt as usize;
            let jf_target = i + 1 + insn.jf as usize;
            if jt_target >= program.len() || jf_target >= program.len() {
                return false;
            }
        }
    }
    true
}

/// Execute a BPF program against seccomp data
fn execute_bpf(program: &[BpfInsn], data: &SeccompData) -> u32 {
    let data_bytes = unsafe {
        core::slice::from_raw_parts(
            data as *const SeccompData as *const u8,
            core::mem::size_of::<SeccompData>(),
        )
    };

    let mut accumulator: u32 = 0;
    let mut pc: usize = 0;

    while pc < program.len() {
        let insn = &program[pc];
        let class = insn.code & 0x07;

        match class {
            0x00 => {
                // BPF_LD
                let mode = insn.code & 0xe0;
                if mode == BPF_ABS {
                    // Load from seccomp_data at absolute offset
                    let offset = insn.k as usize;
                    let size = insn.code & 0x18;
                    if size == BPF_W {
                        // 32-bit load
                        if offset + 4 <= data_bytes.len() {
                            accumulator = u32::from_ne_bytes([
                                data_bytes[offset],
                                data_bytes[offset + 1],
                                data_bytes[offset + 2],
                                data_bytes[offset + 3],
                            ]);
                        }
                    }
                }
                pc += 1;
            }
            0x05 => {
                // BPF_JMP
                let op = insn.code & 0xf0;
                if op == BPF_JEQ {
                    if accumulator == insn.k {
                        pc += 1 + insn.jt as usize;
                    } else {
                        pc += 1 + insn.jf as usize;
                    }
                } else {
                    // JA (unconditional)
                    pc += 1 + insn.k as usize;
                }
            }
            0x06 => {
                // BPF_RET
                return insn.k;
            }
            _ => {
                pc += 1;
            }
        }
    }

    // Default: kill
    SECCOMP_RET_KILL_PROCESS
}

lazy_static::lazy_static! {
    /// Per-process seccomp state
    static ref SECCOMP_STATE: Mutex<BTreeMap<Pid, SeccompState>> = Mutex::new(BTreeMap::new());
}

/// Initialize seccomp state for a process
pub fn create_process_seccomp(pid: Pid) {
    SECCOMP_STATE.lock().insert(pid, SeccompState::new());
}

/// Remove seccomp state for a process
pub fn destroy_process_seccomp(pid: Pid) {
    SECCOMP_STATE.lock().remove(&pid);
}

/// Inherit seccomp state on fork (filters are inherited)
pub fn inherit_seccomp(parent: Pid, child: Pid) {
    let state = SECCOMP_STATE.lock();
    if let Some(parent_state) = state.get(&parent) {
        let child_state = parent_state.clone();
        drop(state);
        SECCOMP_STATE.lock().insert(child, child_state);
    }
}

/// Check if a syscall is allowed by seccomp policy
///
/// Returns:
///   - Ok(()) if allowed
///   - Err(errno) if denied with specific errno
///   - Panics / kills process if SECCOMP_RET_KILL
pub fn check_syscall(pid: Pid, syscall_nr: u64, args: [u64; 6]) -> Result<(), i32> {
    let mut states = SECCOMP_STATE.lock();
    let state = match states.get_mut(&pid) {
        Some(s) => s,
        None => return Ok(()), // No seccomp state = allow
    };

    match state.mode {
        SeccompMode::Disabled => Ok(()),
        SeccompMode::Strict => {
            // Only allow: read(0), write(1), exit(60), exit_group(231), sigreturn(15)
            match syscall_nr {
                0 | 1 | 15 | 60 | 231 => Ok(()),
                _ => {
                    serial_println!(
                        "[seccomp] STRICT: PID {} blocked syscall {}",
                        pid,
                        syscall_nr
                    );
                    Err(-1) // EPERM — will also send SIGKILL
                }
            }
        }
        SeccompMode::Filter => {
            let data = SeccompData {
                nr: syscall_nr as u32,
                arch: 0xC000003E,       // AUDIT_ARCH_X86_64
                instruction_pointer: 0, // Not tracked currently
                args,
            };

            // Run all filters; most restrictive action wins
            let mut result = SECCOMP_RET_ALLOW;
            for filter in &state.filters {
                let action = execute_bpf(filter, &data);
                // Lower action value = more restrictive
                if action < result {
                    result = action;
                }
            }

            state.filter_count += 1;

            let action = result & 0xffff0000;
            match action {
                SECCOMP_RET_ALLOW => Ok(()),
                SECCOMP_RET_LOG => {
                    serial_println!(
                        "[seccomp] LOG: PID {} syscall {} allowed (logged)",
                        pid,
                        syscall_nr
                    );
                    Ok(())
                }
                SECCOMP_RET_ERRNO => {
                    let errno = (result & 0xffff) as i32;
                    serial_println!(
                        "[seccomp] ERRNO: PID {} syscall {} denied (errno={})",
                        pid,
                        syscall_nr,
                        errno
                    );
                    Err(-errno)
                }
                SECCOMP_RET_TRAP => {
                    serial_println!(
                        "[seccomp] TRAP: PID {} syscall {} -> SIGSYS",
                        pid,
                        syscall_nr
                    );
                    // Send SIGSYS to process
                    let _ = crate::signals::kill(pid, crate::signals::Signal::SIGSYS, 0);
                    Err(-1)
                }
                SECCOMP_RET_KILL_THREAD | SECCOMP_RET_KILL_PROCESS => {
                    serial_println!(
                        "[seccomp] KILL: PID {} syscall {} -> killed",
                        pid,
                        syscall_nr
                    );
                    // Kill the process
                    let _ = crate::signals::kill(pid, crate::signals::Signal::SIGKILL, 0);
                    Err(-1)
                }
                _ => Ok(()),
            }
        }
    }
}

/// Set seccomp mode for current process
pub fn seccomp_set_mode_strict(pid: Pid) -> Result<(), i32> {
    let mut states = SECCOMP_STATE.lock();
    let state = states.entry(pid).or_default();
    if state.mode != SeccompMode::Disabled {
        return Err(-1); // Can't change mode once set
    }
    state.set_strict();
    Ok(())
}

/// Add a seccomp BPF filter for a process
pub fn seccomp_set_mode_filter(pid: Pid, program: Vec<BpfInsn>) -> Result<(), i32> {
    let mut states = SECCOMP_STATE.lock();
    let state = states.entry(pid).or_default();
    state.add_filter(program)
}

/// Helper: Build a simple "allow all except blacklist" filter
pub fn build_blacklist_filter(blocked_syscalls: &[u32]) -> Vec<BpfInsn> {
    let mut program = Vec::new();

    // Load syscall number
    program.push(BpfInsn {
        code: BPF_LD | BPF_W | BPF_ABS,
        jt: 0,
        jf: 0,
        k: SECCOMP_DATA_NR,
    });

    // Check each blocked syscall
    for (i, &nr) in blocked_syscalls.iter().enumerate() {
        let remaining = blocked_syscalls.len() - i - 1;
        program.push(BpfInsn {
            code: BPF_JMP | BPF_JEQ | BPF_K,
            jt: (remaining + 1) as u8, // Jump to KILL
            jf: 0,                     // Continue checking
            k: nr,
        });
    }

    // Allow (default)
    program.push(BpfInsn {
        code: BPF_RET | BPF_K,
        jt: 0,
        jf: 0,
        k: SECCOMP_RET_ALLOW,
    });

    // Kill (for matched syscalls)
    program.push(BpfInsn {
        code: BPF_RET | BPF_K,
        jt: 0,
        jf: 0,
        k: SECCOMP_RET_KILL_PROCESS,
    });

    program
}

/// Helper: Build a "whitelist only" filter
pub fn build_whitelist_filter(allowed_syscalls: &[u32]) -> Vec<BpfInsn> {
    let mut program = Vec::new();

    // Load syscall number
    program.push(BpfInsn {
        code: BPF_LD | BPF_W | BPF_ABS,
        jt: 0,
        jf: 0,
        k: SECCOMP_DATA_NR,
    });

    // Check each allowed syscall
    for (i, &nr) in allowed_syscalls.iter().enumerate() {
        let remaining = allowed_syscalls.len() - i - 1;
        program.push(BpfInsn {
            code: BPF_JMP | BPF_JEQ | BPF_K,
            jt: (remaining + 1) as u8, // Jump to ALLOW
            jf: 0,                     // Continue checking
            k: nr,
        });
    }

    // Kill (default for unmatched)
    program.push(BpfInsn {
        code: BPF_RET | BPF_K,
        jt: 0,
        jf: 0,
        k: SECCOMP_RET_KILL_PROCESS,
    });

    // Allow (for matched)
    program.push(BpfInsn {
        code: BPF_RET | BPF_K,
        jt: 0,
        jf: 0,
        k: SECCOMP_RET_ALLOW,
    });

    program
}

/// Get seccomp statistics for a process
pub fn get_stats(pid: Pid) -> Option<(SeccompMode, u64)> {
    let states = SECCOMP_STATE.lock();
    states.get(&pid).map(|s| (s.mode, s.filter_count))
}

/// Initialize seccomp subsystem
pub fn init() {
    serial_println!("[KnoxOS] Seccomp-BPF syscall filtering initialized");
}
