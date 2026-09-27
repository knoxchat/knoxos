use core::sync::atomic::{AtomicU32, Ordering};

use super::runqueue::{dequeue_from_cpu, enqueue_on_cpu};
use super::state::CPU_DATA;

/// Reserved AP idle threads live at `context::ap_idle_pid`.
static LAST_USER_CPU: AtomicU32 = AtomicU32::new(0);
static LAST_USER_PID: AtomicU32 = AtomicU32::new(0);
static AP_IN_IDLE: AtomicU32 = AtomicU32::new(0);
static AP_TIMER_STARTED: AtomicU32 = AtomicU32::new(0);

/// Record that a Ring 3 syscall ran on `cpu`. Gate I3 reads this.
pub fn note_user_syscall(cpu: u32, pid: u32) {
    LAST_USER_CPU.store(cpu, Ordering::Release);
    LAST_USER_PID.store(pid, Ordering::Release);
}

pub fn last_user_cpu() -> u32 {
    LAST_USER_CPU.load(Ordering::Acquire)
}

pub fn last_user_pid() -> u32 {
    LAST_USER_PID.load(Ordering::Acquire)
}

pub fn clear_last_user_cpu() {
    LAST_USER_CPU.store(0, Ordering::Release);
    LAST_USER_PID.store(0, Ordering::Release);
}

pub fn ap_in_idle(cpu: u32) -> bool {
    let bit = 1u32 << cpu.min(31);
    AP_IN_IDLE.load(Ordering::Acquire) & bit != 0
}

fn set_ap_idle_flag(cpu: u32, idle: bool) {
    let bit = 1u32 << cpu.min(31);
    if idle {
        AP_IN_IDLE.fetch_or(bit, Ordering::Release);
    } else {
        AP_IN_IDLE.fetch_and(!bit, Ordering::Release);
    }
}

/// AP idle: start this core's LAPIC timer, then run queued Ring 3 tasks.
pub extern "C" fn ap_idle_loop() -> ! {
    let cpu = crate::usermode::current_cpu_index();
    crate::context::set_current_pid(crate::context::ap_idle_pid(cpu));
    loop {
        set_ap_idle_flag(cpu, true);
        maybe_start_ap_timer(cpu);
        try_enter_ap_task(cpu);
        // Do not HLT: `enqueue_on_cpu` from the BSP races with a
        // check-then-HLT idle (Gate I3). Spin with IF=1 so the APIC timer
        // still fires; QEMU APs are not power-managed.
        core::hint::spin_loop();
    }
}

fn maybe_start_ap_timer(cpu: u32) {
    if !crate::apic_timer::is_initialized() {
        return;
    }
    let bit = 1u32 << cpu.min(31);
    if AP_TIMER_STARTED.load(Ordering::Relaxed) & bit != 0 {
        return;
    }
    crate::apic_timer::start_periodic(10_000);
    AP_TIMER_STARTED.fetch_or(bit, Ordering::Release);
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!("sti", options(nomem, nostack));
    }
}

fn try_enter_ap_task(cpu: u32) {
    while let Some(pid) = dequeue_from_cpu(cpu as usize) {
        if pid <= crate::context::DESKTOP_PID || crate::context::is_ap_idle_pid(pid) {
            continue;
        }
        if !crate::context::has_runnable_context(pid) {
            // Not ready yet — put it back so a later idle pass can take it.
            enqueue_on_cpu(cpu as usize, pid);
            break;
        }
        {
            let mut cpus = CPU_DATA.lock();
            if let Some(slot) = cpus.get_mut(cpu as usize) {
                slot.current_pid = pid;
                slot.context_switches += 1;
            }
        }
        set_ap_idle_flag(cpu, false);
        unsafe {
            crate::context::enter_task_local(pid);
        }
    }
}

/// After a Ring 3 task exits on an AP: run the next queued task or idle.
/// Never returns (enters the AP idle context / another user task).
pub fn ap_after_user_exit() {
    let cpu = crate::usermode::current_cpu_index();
    try_enter_ap_task(cpu);
    let idle = crate::context::ap_idle_pid(cpu);
    crate::context::set_current_pid(idle);
    if crate::context::has_runnable_context(idle) {
        set_ap_idle_flag(cpu, true);
        unsafe {
            crate::context::enter_task_local(idle);
        }
    }
    // Fallback if the idle context was never created.
    ap_idle_loop();
}
