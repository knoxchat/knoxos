use crate::context::DESKTOP_PID;
use crate::process::Pid;
use crate::serial_println;
use alloc::collections::BTreeMap;

/// POSIX wait status: normal exit is `code << 8`; a negative code is a signal.
pub fn wait_status(exit_code: i32) -> i32 {
    if exit_code < 0 {
        (-exit_code) & 0x7F
    } else {
        (exit_code & 0xFF) << 8
    }
}

/// Tear down a task's execution resources. Leaves a zombie unless a waiter
/// is already parked, so a later `wait4` can reap it.
fn retire(pid: Pid, code: i32) {
    serial_println!("[user_task] PID {} exited with {}", pid, code);

    let join_status = if code < 0 { 0 } else { code as u64 };
    crate::threads::thread_exit(pid, join_status);
    deliver_join_result(pid, join_status as i64);

    let parent = {
        let mut table = crate::process::PROCESS_TABLE.lock();
        table.exit_process(pid, code);
        table.get_process(pid).map(|p| p.ppid).unwrap_or(0)
    };

    crate::scheduler::remove_process(pid);
    crate::fd::destroy_fd_table(pid);

    let running_here = pid == crate::context::current_pid();
    if running_here {
        // The syscall / #PF is still on this CR3 and this kernel stack.
        // Leave the boot tables before freeing the L4; keep the stack
        // allocation until the parent reaps (see `reap_child`).
        crate::vmm::activate_kernel_cr3();
        crate::context::invalidate_rip(pid);
    } else {
        crate::context::destroy_process_context(pid);
    }
    crate::vmm::destroy_address_space(pid);
    crate::pgrp::unregister_process(pid);

    if parent > 0 && has_waiter(parent) {
        let reaped = crate::process::PROCESS_TABLE.lock().waitpid(pid);
        if let Some((reaped_pid, status)) = reaped {
            deliver_wait_result(parent, reaped_pid, status);
            crate::signals::destroy_process_signals(reaped_pid);
        }
    } else if parent > 0 {
        let _ = crate::signals::kill(parent, crate::signals::Signal::SIGCHLD, pid);
    }
}

/// Retire the current task and continue on someone else.
///
/// Called from an `exit` syscall. Never returns when another task can run; if
/// nothing else is runnable it returns so the syscall can fall back to a plain
/// return, which is the best that can be done with no task to switch to.
pub fn finish_current(pid: Pid, code: i32) {
    retire(pid, code);
    resume_next_or_return();
}

/// Kill a task that may or may not currently own the CPU (SIGKILL, SIGSEGV).
pub fn terminate(pid: Pid, code: i32) {
    if pid <= DESKTOP_PID {
        return;
    }
    let current = crate::context::current_pid();
    retire(pid, code);
    if pid == current {
        resume_next_or_return();
    }
}

/// Save this task's Ring 3 state and block it until `wake` is called.
///
/// Used by a syscall that cannot complete synchronously. The saved context is
/// what the scheduler later resumes.
pub fn park_current(pid: Pid) {
    let rip = crate::usermode::pending_user_rip();
    let rsp = crate::usermode::current_user_rsp();
    crate::context::set_user_context(pid, rip, rsp, 0);
    crate::scheduler::block_current();
    serial_println!(
        "[user_task] PID {} parked at rip={:#x} rsp={:#x}",
        pid,
        rip,
        rsp
    );
    resume_next_or_return();
}

/// Make a blocked task runnable again.
pub fn wake(pid: Pid) {
    crate::scheduler::wake_process(pid);
}

/// A parent parked in `wait4`, and where its status word must land.
struct WaitRegistration {
    wstatus_ptr: u64,
}

lazy_static::lazy_static! {
    static ref WAITERS: spin::Mutex<BTreeMap<Pid, WaitRegistration>> =
        spin::Mutex::new(BTreeMap::new());
}

fn has_waiter(pid: Pid) -> bool {
    WAITERS.lock().contains_key(&pid)
}

/// Park the current task in `wait4` until one of its children exits.
///
/// Records the Ring 3 resume point and the `wstatus` pointer so the parent can
/// be handed a real result later, then gives up the CPU.
pub fn park_for_wait(wstatus_ptr: u64) {
    let pid = crate::context::current_pid();
    WAITERS.lock().insert(pid, WaitRegistration { wstatus_ptr });

    let rip = crate::usermode::pending_user_rip();
    let rsp = crate::usermode::current_user_rsp();
    crate::context::set_user_context(pid, rip, rsp, 0);
    crate::scheduler::block_current();
    serial_println!(
        "[user_task] PID {} parked in wait4 at rip={:#x} rsp={:#x}",
        pid,
        rip,
        rsp
    );
    resume_next_or_return();
}

/// Hand a reaped child's result to a parked parent and make it runnable.
///
/// Must be called *before* the waking task gives up the CPU, so the parent's
/// resume state is complete by the time the scheduler picks it.
pub fn deliver_wait_result(parent: Pid, child: Pid, status: i32) {
    if let Some(reg) = WAITERS.lock().remove(&parent) {
        let packed = wait_status(status);
        if reg.wstatus_ptr != 0 {
            crate::vmm::write_user_memory(parent, reg.wstatus_ptr, &packed.to_ne_bytes());
        }
        crate::context::set_user_retval(parent, child as i64);
        serial_println!(
            "[user_task] PID {} wait satisfied: child {} status {}",
            parent,
            child,
            packed
        );
    }
    wake(parent);
}

/// A parent parked in `thread_join`, waiting for `tid`.
struct JoinRegistration {
    tid: u32,
}

lazy_static::lazy_static! {
    static ref JOINERS: spin::Mutex<BTreeMap<Pid, JoinRegistration>> =
        spin::Mutex::new(BTreeMap::new());
}

/// Park the current task in `thread_join` until `tid` exits.
pub fn park_for_join(tid: u32) {
    let pid = crate::context::current_pid();
    JOINERS.lock().insert(pid, JoinRegistration { tid });

    let rip = crate::usermode::pending_user_rip();
    let rsp = crate::usermode::current_user_rsp();
    crate::context::set_user_context(pid, rip, rsp, 0);
    crate::scheduler::block_current();
    serial_println!(
        "[user_task] PID {} parked in thread_join({}) at rip={:#x} rsp={:#x}",
        pid,
        tid,
        rip,
        rsp
    );
    resume_next_or_return();
}

/// Hand a thread's exit status to a parked joiner.
pub fn deliver_join_result(tid: u32, status: i64) {
    let joiner = {
        let mut joiners = JOINERS.lock();
        joiners
            .iter()
            .find(|(_, r)| r.tid == tid)
            .map(|(pid, _)| *pid)
    };
    let Some(joiner) = joiner else {
        return;
    };
    JOINERS.lock().remove(&joiner);
    crate::context::set_user_retval(joiner, status);
    serial_println!(
        "[user_task] PID {} join satisfied: tid {} status {}",
        joiner,
        tid,
        status
    );
    wake(joiner);
}

/// Park the current task in `futex(FUTEX_WAIT)` until a matching wake.
pub fn park_for_futex() {
    let pid = crate::context::current_pid();
    let rip = crate::usermode::pending_user_rip();
    let rsp = crate::usermode::current_user_rsp();
    crate::context::set_user_context(pid, rip, rsp, 0);
    crate::scheduler::block_current();
    serial_println!(
        "[user_task] PID {} parked in futex at rip={:#x} rsp={:#x}",
        pid,
        rip,
        rsp
    );
    resume_next_or_return();
}

/// Pick the next runnable task and enter it, or return if there is none.
///
/// The desktop executor is the fallback: it always has a saved context once it
/// has run, and it keeps the GUI alive while every user task is blocked.
fn resume_next_or_return() {
    if crate::usermode::current_cpu_index() != 0 {
        crate::smp::ap_after_user_exit();
        return;
    }

    let picked = {
        let mut sched = crate::scheduler::SCHEDULER.lock();
        sched.clear_current();
        sched.schedule()
    };

    if let Some(next) = picked {
        if next != crate::context::IDLE_PID
            && !crate::context::is_ap_idle_pid(next)
            && crate::context::has_runnable_context(next)
        {
            unsafe {
                crate::context::abandon_current_and_enter(next);
            }
        }
    }
    if crate::context::has_runnable_context(DESKTOP_PID) {
        unsafe {
            crate::context::abandon_current_and_enter(DESKTOP_PID);
        }
    }
    if crate::context::has_runnable_context(crate::context::IDLE_PID) {
        unsafe {
            crate::context::abandon_current_and_enter(crate::context::IDLE_PID);
        }
    }
}

/// Whether a PID currently has a Ring 3 context waiting to be resumed.
pub fn is_runnable(pid: Pid) -> bool {
    crate::context::has_runnable_context(pid)
}
