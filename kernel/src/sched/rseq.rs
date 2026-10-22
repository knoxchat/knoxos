// rseq.rs — Restartable sequences
// Per-CPU critical sections for lock-free data structures (Linux 4.18+)

use alloc::collections::BTreeMap;
use lazy_static::lazy_static;
use spin::Mutex;

/// rseq registration flags
pub const RSEQ_FLAG_UNREGISTER: u32 = 1;

/// rseq CPU ID special values
pub const RSEQ_CPU_ID_UNINITIALIZED: i32 = -1;
pub const RSEQ_CPU_ID_REGISTRATION_FAILED: i32 = -2;

/// rseq signature (used to validate rseq critical section)
pub const RSEQ_SIG: u32 = 0x53053053;

/// rseq cs flags
pub const RSEQ_CS_FLAG_NO_RESTART_ON_PREEMPT: u32 = 1;
pub const RSEQ_CS_FLAG_NO_RESTART_ON_SIGNAL: u32 = 2;
pub const RSEQ_CS_FLAG_NO_RESTART_ON_MIGRATE: u32 = 4;

/// rseq struct layout (matches Linux struct rseq)
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Rseq {
    /// Current CPU ID (updated by kernel on context switch)
    pub cpu_id_start: u32,
    /// Cache-aligned CPU ID
    pub cpu_id: u32,
    /// Pointer to current rseq_cs (critical section descriptor)
    pub rseq_cs: u64,
    /// Flags
    pub flags: u32,
    /// Node ID (for NUMA)
    pub node_id: u32,
    /// Memory Management Unit ID
    pub mm_cid: u32,
}

/// rseq critical section descriptor
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct RseqCs {
    /// Version (must be 0)
    pub version: u32,
    /// Flags
    pub flags: u32,
    /// Start IP of the critical section
    pub start_ip: u64,
    /// Length of the critical section in bytes
    pub post_commit_offset: u64,
    /// Abort handler IP (jumped to on preemption)
    pub abort_ip: u64,
}

/// Per-thread rseq registration state
#[derive(Debug, Clone)]
pub struct RseqRegistration {
    pub tid: u64,
    pub rseq_addr: u64, // User-space address of struct rseq
    pub rseq_len: u32,
    pub signature: u32,
    pub registered: bool,
    pub current_cpu: u32,
    pub current_node: u32,
}

impl RseqRegistration {
    pub fn new(tid: u64, addr: u64, len: u32, sig: u32) -> Self {
        RseqRegistration {
            tid,
            rseq_addr: addr,
            rseq_len: len,
            signature: sig,
            registered: true,
            current_cpu: 0,
            current_node: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RseqError {
    AlreadyRegistered, // EBUSY
    InvalidArg,        // EINVAL
    NotRegistered,     // ENOENT
    InvalidSignature,  // EPERM
    Fault,             // EFAULT
}

lazy_static! {
    static ref REGISTRATIONS: Mutex<BTreeMap<u64, RseqRegistration>> = Mutex::new(BTreeMap::new());
}

/// sys_rseq — register/unregister restartable sequence for current thread
pub fn sys_rseq(
    tid: u64,
    rseq_addr: u64,
    rseq_len: u32,
    flags: u32,
    sig: u32,
) -> Result<(), RseqError> {
    let mut regs = REGISTRATIONS.lock();

    if flags & RSEQ_FLAG_UNREGISTER != 0 {
        // Unregister
        let reg = regs.get(&tid).ok_or(RseqError::NotRegistered)?;

        // Verify signature matches
        if reg.signature != sig {
            return Err(RseqError::InvalidSignature);
        }

        regs.remove(&tid);
        return Ok(());
    }

    // Register
    if regs.contains_key(&tid) {
        return Err(RseqError::AlreadyRegistered);
    }

    // Validate rseq_len (minimum size of struct rseq)
    if rseq_len < 32 {
        return Err(RseqError::InvalidArg);
    }

    // Validate alignment
    if !rseq_addr.is_multiple_of(32) {
        return Err(RseqError::InvalidArg);
    }

    let reg = RseqRegistration::new(tid, rseq_addr, rseq_len, sig);
    regs.insert(tid, reg);

    Ok(())
}

/// Called on context switch — update CPU ID in rseq struct
pub fn on_context_switch(tid: u64, new_cpu: u32) {
    let mut regs = REGISTRATIONS.lock();
    if let Some(reg) = regs.get_mut(&tid) {
        reg.current_cpu = new_cpu;
        // In real implementation, would update user-space rseq struct:
        // - Set cpu_id_start and cpu_id to new_cpu
        // - Check if thread was in a rseq critical section
        // - If so, restart it (set RIP to abort_ip)
    }
}

/// Called on preemption — check if thread was in a rseq critical section
pub fn on_preempt(tid: u64) -> Option<u64> {
    let regs = REGISTRATIONS.lock();
    if let Some(reg) = regs.get(&tid) {
        if !reg.registered {
            return None;
        }
        // In real implementation:
        // 1. Read rseq_cs pointer from user memory
        // 2. If non-zero, check if current RIP is within [start_ip, start_ip + post_commit_offset)
        // 3. If yes, set RIP to abort_ip (restartable sequence aborted)
        // 4. Clear rseq_cs pointer
        // Return abort_ip if in critical section
        let _ = reg;
    }
    None
}

/// Called on signal delivery — similar to preemption
pub fn on_signal(tid: u64) -> Option<u64> {
    on_preempt(tid) // Same logic
}

/// Called on CPU migration — similar to preemption
pub fn on_migrate(tid: u64, new_cpu: u32) -> Option<u64> {
    on_context_switch(tid, new_cpu);
    on_preempt(tid)
}

/// Check if thread has rseq registered
pub fn is_registered(tid: u64) -> bool {
    REGISTRATIONS.lock().contains_key(&tid)
}

/// Get rseq info for a thread (for /proc/[pid]/task/[tid])
pub fn get_rseq_info(tid: u64) -> Option<RseqInfo> {
    let regs = REGISTRATIONS.lock();
    regs.get(&tid).map(|r| RseqInfo {
        rseq_addr: r.rseq_addr,
        rseq_len: r.rseq_len,
        signature: r.signature,
        current_cpu: r.current_cpu,
        current_node: r.current_node,
    })
}

#[derive(Debug, Clone)]
pub struct RseqInfo {
    pub rseq_addr: u64,
    pub rseq_len: u32,
    pub signature: u32,
    pub current_cpu: u32,
    pub current_node: u32,
}

/// Clean up on thread exit
pub fn thread_exit(tid: u64) {
    REGISTRATIONS.lock().remove(&tid);
}

/// Initialize rseq subsystem
pub fn init() {
    crate::serial_println!(
        "  rseq subsystem initialized (restartable sequences, per-CPU critical sections)"
    );
}
