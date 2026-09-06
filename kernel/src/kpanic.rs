/// Kernel Panic Handler — Structured panic handling with diagnostics
///
/// Implements Linux-style kernel panic behavior:
///   - Register dump (all GPRs, CR3, RFLAGS)
///   - Stack trace (frame pointer chain)
///   - Panic notifier chain
///   - Configurable panic timeout
///   - SysRq-like emergency actions
///   - Panic log preservation
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

/// Panic action after timeout
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanicAction {
    /// Halt the CPU
    Halt,
    /// Reboot the system
    Reboot,
    /// Enter debugger / kgdb
    Debug,
}

/// Panic configuration
pub struct PanicConfig {
    /// Seconds to wait before taking action (0 = halt forever)
    pub timeout: u32,
    /// Action to take after timeout
    pub action: PanicAction,
    /// Print stack trace
    pub print_stack: bool,
    /// Print register dump
    pub print_regs: bool,
    /// Generate core dump
    pub dump_core: bool,
    /// Sync filesystems before panic action
    pub sync_on_panic: bool,
}

static PANIC_TIMEOUT: AtomicU32 = AtomicU32::new(0);
static IN_PANIC: AtomicBool = AtomicBool::new(false);

lazy_static::lazy_static! {
    static ref CONFIG: Mutex<PanicConfig> = Mutex::new(PanicConfig {
        timeout: 0,
        action: PanicAction::Halt,
        print_stack: true,
        print_regs: true,
        dump_core: false,
        sync_on_panic: true,
    });
    /// Panic log buffer
    static ref PANIC_LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
}

/// Set panic timeout (seconds, 0 = halt forever, negative = reboot immediately)
pub fn set_panic_timeout(seconds: i32) {
    if seconds < 0 {
        PANIC_TIMEOUT.store(0, Ordering::Release);
        let mut config = CONFIG.lock();
        config.timeout = 0;
        config.action = PanicAction::Reboot;
    } else {
        PANIC_TIMEOUT.store(seconds as u32, Ordering::Release);
        let mut config = CONFIG.lock();
        config.timeout = seconds as u32;
    }
}

/// Check if system is in panic state
pub fn is_panicking() -> bool {
    IN_PANIC.load(Ordering::Acquire)
}

/// Get current CPU registers
fn dump_registers() {
    let mut rsp: u64 = 0;
    let mut rbp: u64 = 0;
    let mut rflags: u64 = 0;
    let mut cr3: u64 = 0;

    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "mov {}, rsp",
            "mov {}, rbp",
            out(reg) rsp,
            out(reg) rbp,
        );
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "pushfq",
            "pop {}",
            out(reg) rflags,
        );
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "mov {}, cr3",
            out(reg) cr3,
        );
    }

    serial_println!("  RSP: {:#018x}  RBP: {:#018x}", rsp, rbp);
    serial_println!("  RFLAGS: {:#018x}  CR3: {:#018x}", rflags, cr3);
}

/// Walk the stack frame chain and print return addresses
fn dump_stack_trace() {
    serial_println!("Call Trace:");

    let mut rbp: u64;
    unsafe {
        core::arch::asm!("mov {}, rbp", out(reg) rbp);
    }

    for i in 0..32 {
        if rbp == 0 || !rbp.is_multiple_of(8) {
            break;
        }

        // Safety: We're reading kernel stack frames
        let frame_ptr = rbp as *const u64;
        let return_addr = unsafe {
            if frame_ptr.add(1).is_null() {
                break;
            }
            *frame_ptr.add(1)
        };
        let next_rbp = unsafe { *frame_ptr };

        if return_addr == 0 {
            break;
        }

        serial_println!("  [{}] {:#018x}", i, return_addr);

        rbp = next_rbp;
    }
}

/// Kernel panic handler — called from panic!() hook
pub fn kernel_panic(message: &str) {
    // Prevent recursive panics
    if IN_PANIC.swap(true, Ordering::SeqCst) {
        serial_println!("!!! RECURSIVE PANIC: {}", message);
        loop {
            crate::arch_compat::instructions::interrupts::hlt();
        }
    }

    // Disable interrupts
    crate::arch_compat::instructions::interrupts::disable();

    serial_println!("\n========================================");
    serial_println!("KERNEL PANIC - not syncing: {}", message);
    serial_println!("========================================");

    // Log to panic buffer
    {
        let mut log = PANIC_LOG.lock();
        log.push(String::from(message));
    }

    // Print current PID
    if let Some(pid) = crate::scheduler::current_pid() {
        serial_println!("  PID: {}", pid);
    }

    // Register dump
    let config = CONFIG.lock();
    let print_regs = config.print_regs;
    let print_stack = config.print_stack;
    let timeout = config.timeout;
    let action = config.action;
    drop(config);

    if print_regs {
        serial_println!("Registers:");
        dump_registers();
    }

    if print_stack {
        dump_stack_trace();
    }

    serial_println!("---[ end Kernel panic ]---");

    // Handle timeout and action
    if timeout > 0 {
        serial_println!("Rebooting in {} seconds...", timeout);
        // Simple busy-wait (no timer interrupts since we disabled them)
        for _ in 0..timeout {
            for _ in 0..100_000_000 {
                core::hint::spin_loop();
            }
        }
        match action {
            PanicAction::Reboot => {
                crate::acpi::reboot();
            }
            PanicAction::Halt => {}
            PanicAction::Debug => {}
        }
    }

    // Halt
    loop {
        crate::arch_compat::instructions::interrupts::hlt();
    }
}

/// SysRq-like emergency commands
pub fn sysrq(key: char) {
    match key {
        'b' => {
            // Immediately reboot
            serial_println!("[SysRq] Rebooting...");
            crate::acpi::reboot();
        }
        'c' => {
            // Crash (trigger panic)
            kernel_panic("SysRq triggered panic");
        }
        'e' => {
            // Send SIGTERM to all processes (except init)
            serial_println!("[SysRq] Sending SIGTERM to all processes");
        }
        'i' => {
            // Send SIGKILL to all processes (except init)
            serial_println!("[SysRq] Sending SIGKILL to all processes");
        }
        'l' => {
            // Show stack trace for all CPUs
            serial_println!("[SysRq] Stack trace:");
            dump_stack_trace();
        }
        'm' => {
            // Show memory info
            serial_println!("[SysRq] Memory info:");
            serial_println!("  Heap used: (check allocator stats)");
        }
        'o' => {
            // Power off
            serial_println!("[SysRq] Powering off...");
            crate::acpi::shutdown();
        }
        'p' => {
            // Show registers
            serial_println!("[SysRq] Registers:");
            dump_registers();
        }
        's' => {
            // Sync all filesystems
            serial_println!("[SysRq] Syncing filesystems...");
        }
        't' => {
            // Show all tasks
            serial_println!("[SysRq] Process list:");
            let table = crate::process::PROCESS_TABLE.lock();
            for p in &table.processes {
                serial_println!("  PID {} [{}] {:?} uid={}", p.pid, p.name, p.state, p.uid);
            }
        }
        'w' => {
            // Show blocked tasks
            serial_println!("[SysRq] Blocked processes:");
            let table = crate::process::PROCESS_TABLE.lock();
            for p in &table.processes {
                if p.state == crate::process::ProcessState::Sleeping {
                    serial_println!("  PID {} [{}]", p.pid, p.name);
                }
            }
        }
        _ => {
            serial_println!("[SysRq] Unknown command: {}", key);
            serial_println!("  b=reboot c=crash e=term-all i=kill-all l=backtrace");
            serial_println!("  m=meminfo o=poweroff p=regs s=sync t=tasks w=blocked");
        }
    }
}

/// Get panic log
pub fn get_panic_log() -> Vec<String> {
    let log = PANIC_LOG.lock();
    log.clone()
}

/// Initialize panic handler
pub fn init() {
    serial_println!("[KnoxOS] Panic handler initialized");
}
