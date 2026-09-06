/// Signal Handling - Linux-compatible signal delivery and handling
/// Implements POSIX signals for process control
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::process::Pid;
use crate::serial_println;

/// Signal numbers (Linux x86_64 compatible)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u32)]
pub enum Signal {
    SIGHUP = 1,
    SIGINT = 2,
    SIGQUIT = 3,
    SIGILL = 4,
    SIGTRAP = 5,
    SIGABRT = 6,
    SIGBUS = 7,
    SIGFPE = 8,
    SIGKILL = 9,
    SIGUSR1 = 10,
    SIGSEGV = 11,
    SIGUSR2 = 12,
    SIGPIPE = 13,
    SIGALRM = 14,
    SIGTERM = 15,
    SIGSTKFLT = 16,
    SIGCHLD = 17,
    SIGCONT = 18,
    SIGSTOP = 19,
    SIGTSTP = 20,
    SIGTTIN = 21,
    SIGTTOU = 22,
    SIGURG = 23,
    SIGXCPU = 24,
    SIGXFSZ = 25,
    SIGVTALRM = 26,
    SIGPROF = 27,
    SIGWINCH = 28,
    SIGIO = 29,
    SIGPWR = 30,
    SIGSYS = 31,
}

impl Signal {
    /// Convert from signal number
    pub fn from_number(num: u32) -> Option<Self> {
        match num {
            1 => Some(Signal::SIGHUP),
            2 => Some(Signal::SIGINT),
            3 => Some(Signal::SIGQUIT),
            4 => Some(Signal::SIGILL),
            5 => Some(Signal::SIGTRAP),
            6 => Some(Signal::SIGABRT),
            7 => Some(Signal::SIGBUS),
            8 => Some(Signal::SIGFPE),
            9 => Some(Signal::SIGKILL),
            10 => Some(Signal::SIGUSR1),
            11 => Some(Signal::SIGSEGV),
            12 => Some(Signal::SIGUSR2),
            13 => Some(Signal::SIGPIPE),
            14 => Some(Signal::SIGALRM),
            15 => Some(Signal::SIGTERM),
            16 => Some(Signal::SIGSTKFLT),
            17 => Some(Signal::SIGCHLD),
            18 => Some(Signal::SIGCONT),
            19 => Some(Signal::SIGSTOP),
            20 => Some(Signal::SIGTSTP),
            21 => Some(Signal::SIGTTIN),
            22 => Some(Signal::SIGTTOU),
            23 => Some(Signal::SIGURG),
            24 => Some(Signal::SIGXCPU),
            25 => Some(Signal::SIGXFSZ),
            26 => Some(Signal::SIGVTALRM),
            27 => Some(Signal::SIGPROF),
            28 => Some(Signal::SIGWINCH),
            29 => Some(Signal::SIGIO),
            30 => Some(Signal::SIGPWR),
            31 => Some(Signal::SIGSYS),
            _ => None,
        }
    }

    /// Default action for this signal
    pub fn default_action(&self) -> SignalAction {
        match self {
            Signal::SIGHUP
            | Signal::SIGINT
            | Signal::SIGTERM
            | Signal::SIGUSR1
            | Signal::SIGUSR2
            | Signal::SIGPIPE
            | Signal::SIGALRM
            | Signal::SIGPROF
            | Signal::SIGVTALRM
            | Signal::SIGSTKFLT
            | Signal::SIGIO
            | Signal::SIGPWR => SignalAction::Terminate,

            Signal::SIGQUIT
            | Signal::SIGILL
            | Signal::SIGABRT
            | Signal::SIGBUS
            | Signal::SIGFPE
            | Signal::SIGSEGV
            | Signal::SIGXCPU
            | Signal::SIGXFSZ
            | Signal::SIGSYS
            | Signal::SIGTRAP => SignalAction::CoreDump,

            Signal::SIGKILL => SignalAction::Terminate,

            Signal::SIGSTOP | Signal::SIGTSTP | Signal::SIGTTIN | Signal::SIGTTOU => {
                SignalAction::Stop
            }

            Signal::SIGCONT => SignalAction::Continue,

            Signal::SIGCHLD | Signal::SIGURG | Signal::SIGWINCH => SignalAction::Ignore,
        }
    }

    /// Whether this signal can be caught/blocked
    pub fn is_catchable(&self) -> bool {
        !matches!(self, Signal::SIGKILL | Signal::SIGSTOP)
    }
}

/// What to do when a signal is received
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalAction {
    /// Terminate the process
    Terminate,
    /// Terminate and produce core dump
    CoreDump,
    /// Stop (suspend) the process
    Stop,
    /// Continue a stopped process
    Continue,
    /// Ignore the signal
    Ignore,
    /// Custom handler (function pointer as u64)
    Handler(u64),
}

/// Signal disposition for a process
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalDisposition {
    Default,
    Ignore,
    Handler(u64),
}

/// A pending signal
#[derive(Debug, Clone)]
pub struct PendingSignal {
    pub signal: Signal,
    pub sender_pid: Pid,
    pub timestamp: u64,
}

/// Signal mask (bitset of blocked signals)
#[derive(Debug, Clone, Copy)]
pub struct SignalMask(pub u64);

impl SignalMask {
    pub fn empty() -> Self {
        SignalMask(0)
    }

    pub fn full() -> Self {
        SignalMask(0xFFFFFFFF)
    }

    pub fn is_blocked(&self, sig: Signal) -> bool {
        // SIGKILL and SIGSTOP can never be blocked
        if !sig.is_catchable() {
            return false;
        }
        self.0 & (1 << (sig as u32)) != 0
    }

    pub fn block(&mut self, sig: Signal) {
        if sig.is_catchable() {
            self.0 |= 1 << (sig as u32);
        }
    }

    pub fn unblock(&mut self, sig: Signal) {
        self.0 &= !(1 << (sig as u32));
    }
}

/// Per-process signal state
pub struct ProcessSignals {
    /// Signal dispositions (how to handle each signal)
    pub dispositions: BTreeMap<u32, SignalDisposition>,
    /// Pending signals queue
    pub pending: Vec<PendingSignal>,
    /// Blocked signal mask
    pub mask: SignalMask,
}

impl Default for ProcessSignals {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessSignals {
    pub fn new() -> Self {
        Self {
            dispositions: BTreeMap::new(),
            pending: Vec::new(),
            mask: SignalMask::empty(),
        }
    }

    /// Set signal disposition
    pub fn set_handler(&mut self, sig: Signal, disposition: SignalDisposition) -> Result<(), i32> {
        if !sig.is_catchable() {
            return Err(-22); // EINVAL - can't change SIGKILL/SIGSTOP
        }
        self.dispositions.insert(sig as u32, disposition);
        Ok(())
    }

    /// Get the effective action for a signal
    pub fn get_action(&self, sig: Signal) -> SignalAction {
        match self.dispositions.get(&(sig as u32)) {
            Some(SignalDisposition::Ignore) => SignalAction::Ignore,
            Some(SignalDisposition::Handler(addr)) => SignalAction::Handler(*addr),
            _ => sig.default_action(),
        }
    }

    /// Queue a signal
    pub fn send_signal(&mut self, sig: Signal, sender_pid: Pid) {
        static TIMESTAMP: AtomicU64 = AtomicU64::new(0);
        self.pending.push(PendingSignal {
            signal: sig,
            sender_pid,
            timestamp: TIMESTAMP.fetch_add(1, Ordering::Relaxed),
        });
    }

    /// Dequeue the next deliverable signal (not blocked)
    pub fn dequeue_signal(&mut self) -> Option<PendingSignal> {
        let pos = self
            .pending
            .iter()
            .position(|s| !self.mask.is_blocked(s.signal))?;
        Some(self.pending.remove(pos))
    }

    /// Check if there are any pending unblocked signals
    pub fn has_pending(&self) -> bool {
        self.pending.iter().any(|s| !self.mask.is_blocked(s.signal))
    }
}

/// Global signal state per process
lazy_static::lazy_static! {
    pub static ref PROCESS_SIGNALS: Mutex<BTreeMap<Pid, ProcessSignals>> = {
        let mut map = BTreeMap::new();
        map.insert(0, ProcessSignals::new()); // kernel
        map.insert(1, ProcessSignals::new()); // init
        map.insert(2, ProcessSignals::new()); // knoxos-desktop
        Mutex::new(map)
    };
}

/// Send a signal to a process
pub fn kill(target_pid: Pid, sig: Signal, sender_pid: Pid) -> Result<(), i32> {
    let mut signals = PROCESS_SIGNALS.lock();
    let proc_signals = signals.get_mut(&target_pid).ok_or(-3i32)?; // ESRCH

    // Queue the signal
    proc_signals.send_signal(sig, sender_pid);

    // Handle uncatchable signals immediately
    match sig {
        Signal::SIGKILL => {
            serial_println!("[KnoxOS] SIGKILL -> PID {}", target_pid);
            drop(signals);
            crate::process::PROCESS_TABLE.lock().kill(target_pid);
        }
        Signal::SIGSTOP => {
            serial_println!("[KnoxOS] SIGSTOP -> PID {}", target_pid);
            // Would suspend the process
        }
        _ => {}
    }

    Ok(())
}

/// Create signal state for a new process
pub fn create_process_signals(pid: Pid) {
    PROCESS_SIGNALS.lock().insert(pid, ProcessSignals::new());
}

/// Remove signal state when process exits
pub fn destroy_process_signals(pid: Pid) {
    PROCESS_SIGNALS.lock().remove(&pid);
}

/// Fork signal state from parent to child (called during fork())
/// Child inherits signal dispositions (handlers, masks) from parent.
/// Pending signals are NOT inherited.
pub fn fork_process_signals(parent_pid: Pid, child_pid: Pid) {
    let mut signals = PROCESS_SIGNALS.lock();
    let child_state = if let Some(parent_state) = signals.get(&parent_pid) {
        let mut new_state = ProcessSignals::new();
        // Copy signal mask
        new_state.mask = parent_state.mask;
        // Copy signal dispositions
        new_state.dispositions = parent_state.dispositions.clone();
        // Do NOT copy pending signals
        new_state
    } else {
        ProcessSignals::new()
    };
    signals.insert(child_pid, child_state);
}

/// Reset signal dispositions after exec() (called during exec())
/// Signals set to SIG_DFL stay SIG_DFL.
/// Signals set to SIG_IGN stay SIG_IGN.
/// Signals set to a handler are reset to SIG_DFL (since handler address is invalid after exec).
/// Pending signals are preserved.
/// Signal mask is preserved.
pub fn exec_reset_signals(pid: Pid) {
    let mut signals = PROCESS_SIGNALS.lock();
    if let Some(proc_signals) = signals.get_mut(&pid) {
        // Reset all custom handler dispositions to default
        // SIG_IGN stays, but handlers get reset to default
        let to_reset: Vec<u32> = proc_signals
            .dispositions
            .iter()
            .filter(|(_, d)| matches!(d, SignalDisposition::Handler(_)))
            .map(|(&sig, _)| sig)
            .collect();
        for sig in to_reset {
            proc_signals.dispositions.remove(&sig);
        }
    }
}

/// Deliver pending signals for a process
pub fn deliver_signals(pid: Pid) {
    let mut signals = PROCESS_SIGNALS.lock();
    if let Some(proc_signals) = signals.get_mut(&pid) {
        while let Some(pending) = proc_signals.dequeue_signal() {
            let action = proc_signals.get_action(pending.signal);
            match action {
                SignalAction::Terminate | SignalAction::CoreDump => {
                    serial_println!(
                        "[KnoxOS] Signal {:?} terminates PID {}",
                        pending.signal,
                        pid
                    );
                    drop(signals);
                    crate::process::PROCESS_TABLE.lock().kill(pid);
                    return;
                }
                SignalAction::Stop => {
                    serial_println!("[KnoxOS] Signal {:?} stops PID {}", pending.signal, pid);
                    // Would change process state to Stopped
                }
                SignalAction::Continue => {
                    serial_println!("[KnoxOS] Signal {:?} continues PID {}", pending.signal, pid);
                    // Would resume a stopped process
                }
                SignalAction::Ignore => {}
                SignalAction::Handler(addr) => {
                    serial_println!(
                        "[KnoxOS] Signal {:?} -> handler {:#x} for PID {}",
                        pending.signal,
                        addr,
                        pid
                    );
                    // Set up signal trampoline on user stack
                    drop(signals);
                    setup_signal_frame(pid, pending.signal, addr);
                    return;
                }
            }
        }
    }
}

// ─── Signal Frame / Trampoline for User-Mode Delivery ─────────────────

/// Signal frame pushed onto the user stack before invoking a signal handler.
/// When the handler returns, it executes a sigreturn trampoline that
/// invokes the rt_sigreturn syscall to restore the saved context.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SignalFrame {
    /// Trampoline code: mov rax, 15 (rt_sigreturn); syscall
    pub trampoline: [u8; 16],
    /// Saved user RIP (return address after sigreturn)
    pub saved_rip: u64,
    /// Saved user RSP
    pub saved_rsp: u64,
    /// Saved user RFLAGS
    pub saved_rflags: u64,
    /// Saved general-purpose registers
    pub saved_rax: u64,
    pub saved_rbx: u64,
    pub saved_rcx: u64,
    pub saved_rdx: u64,
    pub saved_rsi: u64,
    pub saved_rdi: u64,
    pub saved_rbp: u64,
    pub saved_r8: u64,
    pub saved_r9: u64,
    pub saved_r10: u64,
    pub saved_r11: u64,
    pub saved_r12: u64,
    pub saved_r13: u64,
    pub saved_r14: u64,
    pub saved_r15: u64,
    /// Signal number
    pub signo: u32,
    /// Padding
    pub _pad: u32,
    /// Blocked signal mask at time of delivery
    pub saved_mask: u64,
}

impl SignalFrame {
    /// Create a new signal frame with trampoline code
    pub fn new(signo: u32) -> Self {
        let mut frame: Self = unsafe { core::mem::zeroed() };
        frame.signo = signo;

        // Trampoline code: calls rt_sigreturn (syscall 15 on x86_64)
        // mov rax, 15     -> 48 c7 c0 0f 00 00 00
        // syscall          -> 0f 05
        // int3 (safety)    -> cc
        frame.trampoline[0] = 0x48;
        frame.trampoline[1] = 0xc7;
        frame.trampoline[2] = 0xc0;
        frame.trampoline[3] = 0x0f;
        frame.trampoline[4] = 0x00;
        frame.trampoline[5] = 0x00;
        frame.trampoline[6] = 0x00;
        frame.trampoline[7] = 0x0f;
        frame.trampoline[8] = 0x05;
        frame.trampoline[9] = 0xcc;

        frame
    }
}

/// Set up a signal frame on the user stack and redirect execution to handler
fn setup_signal_frame(pid: Pid, signal: Signal, handler: u64) {
    // Get the current user context
    let ctx = match crate::context::get_user_context(pid) {
        Some(c) => c,
        None => {
            serial_println!("[SIG] Cannot deliver signal: no context for PID {}", pid);
            return;
        }
    };

    // Create signal frame
    let mut frame = SignalFrame::new(signal as u32);
    frame.saved_rip = ctx.rip;
    frame.saved_rsp = ctx.rsp;
    frame.saved_rflags = ctx.rflags;
    frame.saved_rax = ctx.rax;
    frame.saved_rbx = ctx.rbx;
    frame.saved_rcx = ctx.rcx;
    frame.saved_rdx = ctx.rdx;
    frame.saved_rsi = ctx.rsi;
    frame.saved_rdi = ctx.rdi;
    frame.saved_rbp = ctx.rbp;
    frame.saved_r8 = ctx.r8;
    frame.saved_r9 = ctx.r9;
    frame.saved_r10 = ctx.r10;
    frame.saved_r11 = ctx.r11;
    frame.saved_r12 = ctx.r12;
    frame.saved_r13 = ctx.r13;
    frame.saved_r14 = ctx.r14;
    frame.saved_r15 = ctx.r15;

    // Save blocked mask
    let signals = PROCESS_SIGNALS.lock();
    if let Some(proc_signals) = signals.get(&pid) {
        frame.saved_mask = proc_signals.mask.0;
    }
    drop(signals);

    // Push frame onto user stack (below current RSP)
    let frame_size = core::mem::size_of::<SignalFrame>() as u64;
    let new_rsp = (ctx.rsp - frame_size) & !0xF; // 16-byte aligned
    let trampoline_addr = new_rsp; // Trampoline is at the start of the frame

    // Write frame to user stack memory
    let frame_bytes = unsafe {
        core::slice::from_raw_parts(
            &frame as *const SignalFrame as *const u8,
            core::mem::size_of::<SignalFrame>(),
        )
    };

    // Write via VMM (safe mapping)
    crate::vmm::write_user_memory(pid, new_rsp, frame_bytes);

    // Block the delivered signal during handler execution
    let mut signals = PROCESS_SIGNALS.lock();
    if let Some(proc_signals) = signals.get_mut(&pid) {
        proc_signals.mask.block(signal);
    }
    drop(signals);

    // Modify the user context:
    // - RIP = handler address
    // - RSP = new stack with signal frame
    // - RDI = signal number (first argument to handler)
    // - Return address on stack = trampoline
    crate::context::set_user_context(pid, handler, new_rsp, signal as u64);

    serial_println!(
        "[SIG] Signal frame set up for PID {}: handler={:#x} rsp={:#x} signo={}",
        pid,
        handler,
        new_rsp,
        signal as u32
    );
}

/// Handle rt_sigreturn — restore context from signal frame
pub fn sigreturn(pid: Pid) -> Option<(u64, u64)> {
    // Read the signal frame from the current user RSP
    let ctx = crate::context::get_user_context(pid)?;
    let frame_rsp = ctx.rsp; // RSP points to the signal frame

    // Read signal frame from user memory
    let frame_size = core::mem::size_of::<SignalFrame>();
    let mut frame_bytes = alloc::vec![0u8; frame_size];
    crate::vmm::read_user_memory(pid, frame_rsp, &mut frame_bytes);

    let frame = unsafe { &*(frame_bytes.as_ptr() as *const SignalFrame) };

    // Restore signal mask
    let mut signals = PROCESS_SIGNALS.lock();
    if let Some(proc_signals) = signals.get_mut(&pid) {
        proc_signals.mask.0 = frame.saved_mask;
    }
    drop(signals);

    // Restore full user context
    crate::context::restore_user_context(pid, frame);

    serial_println!(
        "[SIG] sigreturn for PID {}: restored RIP={:#x}",
        pid,
        frame.saved_rip
    );

    Some((frame.saved_rip, frame.saved_rsp))
}

/// Initialize signal subsystem
pub fn init() {
    serial_println!("[KnoxOS] Signal handling initialized (31 POSIX signals)");
}
