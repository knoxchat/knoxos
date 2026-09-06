// ptrace.rs — Process tracing/debugging (strace, gdb, ltrace support)
// Linux ptrace(2) implementation for debugging and system call tracing

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

/// ptrace request types
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u64)]
pub enum PtraceRequest {
    TraceMe = 0,
    PeekText = 1,
    PeekData = 2,
    PeekUser = 3,
    PokeText = 4,
    PokeData = 5,
    PokeUser = 6,
    Cont = 7,
    Kill = 8,
    SingleStep = 9,
    GetRegs = 12,
    SetRegs = 13,
    GetFpRegs = 14,
    SetFpRegs = 15,
    Attach = 16,
    Detach = 17,
    GetFpXRegs = 18,
    SetFpXRegs = 19,
    Syscall = 24,
    SetOptions = 0x4200,
    GetEventMsg = 0x4201,
    GetSigInfo = 0x4202,
    SetSigInfo = 0x4203,
    GetRegSet = 0x4204,
    SetRegSet = 0x4205,
    Seize = 0x4206,
    Interrupt = 0x4207,
    Listen = 0x4208,
    PeekSigInfo = 0x4209,
    GetSigMask = 0x420A,
    SetSigMask = 0x420B,
    SeccompGetFilter = 0x420C,
}

/// ptrace options (set via PTRACE_SETOPTIONS)
pub const PTRACE_O_TRACESYSGOOD: u64 = 0x00000001;
pub const PTRACE_O_TRACEFORK: u64 = 0x00000002;
pub const PTRACE_O_TRACEVFORK: u64 = 0x00000004;
pub const PTRACE_O_TRACECLONE: u64 = 0x00000008;
pub const PTRACE_O_TRACEEXEC: u64 = 0x00000010;
pub const PTRACE_O_TRACEVFORKDONE: u64 = 0x00000020;
pub const PTRACE_O_TRACEEXIT: u64 = 0x00000040;
pub const PTRACE_O_TRACESECCOMP: u64 = 0x00000080;
pub const PTRACE_O_EXITKILL: u64 = 0x00100000;
pub const PTRACE_O_SUSPEND_SECCOMP: u64 = 0x00200000;

/// ptrace events
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PtraceEvent {
    Fork = 1,
    Vfork = 2,
    Clone = 3,
    Exec = 4,
    VforkDone = 5,
    Exit = 6,
    Seccomp = 7,
    Stop = 128,
}

/// Tracee stop reason
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StopReason {
    SyscallEntry,
    SyscallExit,
    Signal(u8),
    Event(PtraceEvent),
    SingleStep,
    GroupStop,
}

/// x86_64 register set
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct UserRegs {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rax: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub orig_rax: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
    pub fs_base: u64,
    pub gs_base: u64,
    pub ds: u64,
    pub es: u64,
    pub fs: u64,
    pub gs: u64,
}

/// A ptrace tracee
#[derive(Debug)]
pub struct Tracee {
    pub pid: u64,
    pub tracer_pid: u64,
    pub options: u64,
    pub stopped: bool,
    pub stop_reason: Option<StopReason>,
    pub single_step: bool,
    pub syscall_trace: bool,
    pub regs: UserRegs,
    pub event_msg: u64,
    pub pending_signal: Option<u8>,
    /// Syscall entry/exit toggle
    pub in_syscall: bool,
}

impl Tracee {
    pub fn new(pid: u64, tracer_pid: u64) -> Self {
        Tracee {
            pid,
            tracer_pid,
            options: 0,
            stopped: true,
            stop_reason: None,
            single_step: false,
            syscall_trace: false,
            regs: UserRegs::default(),
            event_msg: 0,
            pending_signal: None,
            in_syscall: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PtraceError {
    NotFound,       // ESRCH
    PermDenied,     // EPERM
    AlreadyTraced,  // EPERM
    InvalidRequest, // EIO
    InvalidAddr,    // EIO
    NotStopped,     // Not in stopped state
}

lazy_static! {
    static ref TRACEES: Mutex<BTreeMap<u64, Tracee>> = Mutex::new(BTreeMap::new());
}

/// PTRACE_TRACEME — tracee calls this to allow parent to trace it
pub fn sys_ptrace_traceme(pid: u64) -> Result<(), PtraceError> {
    let mut tracees = TRACEES.lock();
    if tracees.contains_key(&pid) {
        return Err(PtraceError::AlreadyTraced);
    }

    // Parent PID from process table
    let ppid = crate::process::PROCESS_TABLE
        .lock()
        .processes
        .get(pid as usize)
        .map(|p| p.ppid as u64)
        .unwrap_or(1);

    tracees.insert(pid, Tracee::new(pid, ppid));
    Ok(())
}

/// PTRACE_ATTACH — tracer attaches to a running process
pub fn sys_ptrace_attach(tracer_pid: u64, target_pid: u64) -> Result<(), PtraceError> {
    // Check target exists
    let exists = crate::process::PROCESS_TABLE
        .lock()
        .processes
        .get(target_pid as usize)
        .is_some();
    if !exists {
        return Err(PtraceError::NotFound);
    }

    let mut tracees = TRACEES.lock();
    if tracees.contains_key(&target_pid) {
        return Err(PtraceError::AlreadyTraced);
    }

    let mut tracee = Tracee::new(target_pid, tracer_pid);
    tracee.stopped = true;
    tracee.stop_reason = Some(StopReason::Signal(19)); // SIGSTOP
    tracees.insert(target_pid, tracee);

    // Send SIGSTOP to target
    let _ = crate::signals::kill(
        target_pid as u32,
        crate::signals::Signal::SIGSTOP,
        tracer_pid as u32,
    );

    Ok(())
}

/// PTRACE_SEIZE — like attach but doesn't stop the tracee
pub fn sys_ptrace_seize(tracer_pid: u64, target_pid: u64, options: u64) -> Result<(), PtraceError> {
    let exists = crate::process::PROCESS_TABLE
        .lock()
        .processes
        .get(target_pid as usize)
        .is_some();
    if !exists {
        return Err(PtraceError::NotFound);
    }

    let mut tracees = TRACEES.lock();
    if tracees.contains_key(&target_pid) {
        return Err(PtraceError::AlreadyTraced);
    }

    let mut tracee = Tracee::new(target_pid, tracer_pid);
    tracee.options = options;
    tracee.stopped = false;
    tracees.insert(target_pid, tracee);

    Ok(())
}

/// PTRACE_DETACH — detach from tracee
pub fn sys_ptrace_detach(pid: u64) -> Result<(), PtraceError> {
    let mut tracees = TRACEES.lock();
    tracees.remove(&pid).ok_or(PtraceError::NotFound)?;
    Ok(())
}

/// PTRACE_CONT — continue stopped tracee
pub fn sys_ptrace_cont(pid: u64, signal: u64) -> Result<(), PtraceError> {
    let mut tracees = TRACEES.lock();
    let tracee = tracees.get_mut(&pid).ok_or(PtraceError::NotFound)?;

    if !tracee.stopped {
        return Err(PtraceError::NotStopped);
    }

    tracee.stopped = false;
    tracee.stop_reason = None;
    tracee.single_step = false;
    tracee.syscall_trace = false;

    if signal > 0 && signal <= 64 {
        tracee.pending_signal = Some(signal as u8);
    }

    Ok(())
}

/// PTRACE_SYSCALL — continue but stop at next syscall entry/exit
pub fn sys_ptrace_syscall(pid: u64, signal: u64) -> Result<(), PtraceError> {
    let mut tracees = TRACEES.lock();
    let tracee = tracees.get_mut(&pid).ok_or(PtraceError::NotFound)?;

    if !tracee.stopped {
        return Err(PtraceError::NotStopped);
    }

    tracee.stopped = false;
    tracee.stop_reason = None;
    tracee.syscall_trace = true;
    tracee.single_step = false;

    if signal > 0 && signal <= 64 {
        tracee.pending_signal = Some(signal as u8);
    }

    Ok(())
}

/// PTRACE_SINGLESTEP — execute one instruction then stop
pub fn sys_ptrace_singlestep(pid: u64, signal: u64) -> Result<(), PtraceError> {
    let mut tracees = TRACEES.lock();
    let tracee = tracees.get_mut(&pid).ok_or(PtraceError::NotFound)?;

    if !tracee.stopped {
        return Err(PtraceError::NotStopped);
    }

    tracee.stopped = false;
    tracee.stop_reason = None;
    tracee.single_step = true;

    if signal > 0 && signal <= 64 {
        tracee.pending_signal = Some(signal as u8);
    }

    Ok(())
}

/// PTRACE_GETREGS — read tracee registers
pub fn sys_ptrace_getregs(pid: u64) -> Result<UserRegs, PtraceError> {
    let tracees = TRACEES.lock();
    let tracee = tracees.get(&pid).ok_or(PtraceError::NotFound)?;

    if !tracee.stopped {
        return Err(PtraceError::NotStopped);
    }

    Ok(tracee.regs)
}

/// PTRACE_SETREGS — write tracee registers
pub fn sys_ptrace_setregs(pid: u64, regs: UserRegs) -> Result<(), PtraceError> {
    let mut tracees = TRACEES.lock();
    let tracee = tracees.get_mut(&pid).ok_or(PtraceError::NotFound)?;

    if !tracee.stopped {
        return Err(PtraceError::NotStopped);
    }

    tracee.regs = regs;
    Ok(())
}

/// PTRACE_SETOPTIONS — set ptrace options
pub fn sys_ptrace_setoptions(pid: u64, options: u64) -> Result<(), PtraceError> {
    let mut tracees = TRACEES.lock();
    let tracee = tracees.get_mut(&pid).ok_or(PtraceError::NotFound)?;
    tracee.options = options;
    Ok(())
}

/// PTRACE_GETEVENTMSG — get event message (child PID for fork/clone, exit status for exit)
pub fn sys_ptrace_geteventmsg(pid: u64) -> Result<u64, PtraceError> {
    let tracees = TRACEES.lock();
    let tracee = tracees.get(&pid).ok_or(PtraceError::NotFound)?;
    Ok(tracee.event_msg)
}

/// PTRACE_PEEKDATA / PTRACE_PEEKTEXT — read word from tracee memory
pub fn sys_ptrace_peek(pid: u64, addr: u64) -> Result<u64, PtraceError> {
    let tracees = TRACEES.lock();
    let _tracee = tracees.get(&pid).ok_or(PtraceError::NotFound)?;

    // In real implementation, would read from tracee's address space
    // For now, return address as placeholder
    let _ = addr;
    Ok(0)
}

/// PTRACE_POKEDATA / PTRACE_POKETEXT — write word to tracee memory
pub fn sys_ptrace_poke(pid: u64, addr: u64, data: u64) -> Result<(), PtraceError> {
    let tracees = TRACEES.lock();
    let _tracee = tracees.get(&pid).ok_or(PtraceError::NotFound)?;

    // In real implementation, would write to tracee's address space
    let _ = addr;
    let _ = data;
    Ok(())
}

/// Called from syscall handler — check if process is being traced
pub fn check_syscall_trace(pid: u64, syscall_nr: u64, args: [u64; 6], is_entry: bool) -> bool {
    let mut tracees = TRACEES.lock();
    if let Some(tracee) = tracees.get_mut(&pid) {
        if tracee.syscall_trace {
            tracee.stopped = true;
            tracee.stop_reason = Some(if is_entry {
                StopReason::SyscallEntry
            } else {
                StopReason::SyscallExit
            });
            tracee.in_syscall = !tracee.in_syscall;

            // Save syscall info in registers
            tracee.regs.orig_rax = syscall_nr;
            tracee.regs.rdi = args[0];
            tracee.regs.rsi = args[1];
            tracee.regs.rdx = args[2];
            tracee.regs.r10 = args[3];
            tracee.regs.r8 = args[4];
            tracee.regs.r9 = args[5];

            return true; // Process should stop
        }
    }
    false
}

/// Notify tracer of an event (fork, exec, exit, etc.)
pub fn notify_event(pid: u64, event: PtraceEvent, msg: u64) {
    let mut tracees = TRACEES.lock();
    if let Some(tracee) = tracees.get_mut(&pid) {
        let should_stop = match event {
            PtraceEvent::Fork => tracee.options & PTRACE_O_TRACEFORK != 0,
            PtraceEvent::Vfork => tracee.options & PTRACE_O_TRACEVFORK != 0,
            PtraceEvent::Clone => tracee.options & PTRACE_O_TRACECLONE != 0,
            PtraceEvent::Exec => tracee.options & PTRACE_O_TRACEEXEC != 0,
            PtraceEvent::VforkDone => tracee.options & PTRACE_O_TRACEVFORKDONE != 0,
            PtraceEvent::Exit => tracee.options & PTRACE_O_TRACEEXIT != 0,
            PtraceEvent::Seccomp => tracee.options & PTRACE_O_TRACESECCOMP != 0,
            PtraceEvent::Stop => true,
        };

        if should_stop {
            tracee.stopped = true;
            tracee.stop_reason = Some(StopReason::Event(event));
            tracee.event_msg = msg;
        }
    }
}

/// Check if a process is being traced
pub fn is_traced(pid: u64) -> bool {
    TRACEES.lock().contains_key(&pid)
}

/// Get the tracer PID for a given tracee
pub fn get_tracer(pid: u64) -> Option<u64> {
    TRACEES.lock().get(&pid).map(|t| t.tracer_pid)
}

/// Check if tracee is stopped
pub fn is_stopped(pid: u64) -> bool {
    TRACEES.lock().get(&pid).is_some_and(|t| t.stopped)
}

/// Initialize ptrace subsystem
pub fn init() {
    crate::serial_println!(
        "  ptrace subsystem initialized (TRACEME, ATTACH, SEIZE, SYSCALL, SINGLESTEP)"
    );
}
