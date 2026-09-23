/// User Task — scheduled Ring 3 processes
///
/// This is the difference between "a program ran once at boot" and "the OS
/// runs programs". It owns the lifecycle every Ring 3 task shares:
///
///   * `spawn` — build the task (address space, ELF, stack, context, fd
///     table, signal state, kernel stack) and put it on the run queue.
///   * `finish_current` — a task called `exit`. Retire it, wake its parent,
///     and give the CPU to whoever is runnable next.
///   * `park_current` — a task blocked in a syscall (wait4 / pause). Save a
///     Ring 3 resume point, block it, and switch away.
///   * `wake` / `terminate` — the thing a task was waiting for happened, or
///     a fatal signal arrived.
///
/// The address-space switch happens inside `context::enter_context`, so a
/// task always runs on its own CR3 and its own kernel stack.
use crate::context::DESKTOP_PID;
use crate::process::Pid;
use crate::serial_println;
use alloc::collections::BTreeMap;

/// Per-task kernel-stack size used while the task is in Ring 0 (syscalls and
/// interrupts). `ProcessContext::new` allocates this; it is the stack that
/// `TSS.RSP0` and `syscall`'s `gs:[8]` must point at while the task runs.
pub const KERNEL_STACK_SIZE: usize = 32 * 1024;

/// Result of trying to spawn a user task.
pub enum SpawnOutcome {
    Spawned(Pid),
    NoElf,
    NoMemory(&'static str),
}

/// Build a Ring 3 task from an ELF image and enqueue it.
///
/// On success the task is on the run queue with its own CR3, ready for the
/// next `switch_to`. It does not start on its own — callers either yield to
/// the scheduler or (for the boot path) enter it directly.
pub fn spawn_elf(
    elf_data: &[u8],
    name: &str,
    argv: &[&str],
    envp: &[&str],
    entry_override: Option<u64>,
) -> SpawnOutcome {
    if !crate::elf::is_elf(elf_data) {
        return SpawnOutcome::NoElf;
    }

    let parent = {
        let ctx = crate::context::current_pid();
        if ctx == 0 {
            crate::scheduler::current_pid().unwrap_or(1)
        } else {
            ctx
        }
    };

    let pid = {
        let mut table = crate::process::PROCESS_TABLE.lock();
        let pid = table.spawn(name, parent);
        if let Some(proc) = table.get_process_mut(pid) {
            proc.has_address_space = true;
        }
        pid
    };

    if !crate::vmm::create_address_space(pid) {
        serial_println!("[user_task] PID {}: address space allocation failed", pid);
        crate::process::destroy_process(pid);
        return SpawnOutcome::NoMemory("address space");
    }

    let entry = match crate::vmm::load_elf_into_address_space(pid, elf_data) {
        Ok((entry, _brk)) => entry_override.unwrap_or(entry),
        Err(e) => {
            serial_println!("[user_task] PID {}: ELF load failed: {}", pid, e);
            crate::process::destroy_process(pid);
            return SpawnOutcome::NoMemory("ELF load");
        }
    };

    let stack_top = crate::vmm::STACK_TOP;
    if crate::vmm::setup_user_stack(pid, stack_top, crate::vmm::STACK_SIZE).is_none() {
        serial_println!("[user_task] PID {}: stack mapping failed", pid);
        crate::process::destroy_process(pid);
        return SpawnOutcome::NoMemory("stack");
    }
    let initial_rsp = crate::vmm::setup_initial_stack(pid, stack_top, argv, envp, entry, 0, 0)
        .unwrap_or(stack_top - 8);

    crate::fd::create_fd_table(pid);
    crate::signals::create_process_signals(pid);
    let uid = crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .map(|p| p.uid)
        .unwrap_or(0);
    crate::capabilities::init_process_caps(pid, parent);
    crate::capabilities::apply_exec_caps(pid, uid);
    let _ = crate::pgrp::setpgid(pid, pid);

    let cr3 = crate::vmm::get_cr3(pid).unwrap_or(0);
    crate::context::create_user_process_context(pid, entry, initial_rsp, cr3);

    {
        let mut table = crate::process::PROCESS_TABLE.lock();
        if let Some(proc) = table.get_process_mut(pid) {
            proc.entry_point = entry;
            proc.user_stack_top = initial_rsp;
        }
    }

    crate::scheduler::add_process(pid, 0);
    serial_println!(
        "[user_task] spawned PID {} '{}' entry={:#x} rsp={:#x} cr3={:#x}",
        pid,
        name,
        entry,
        initial_rsp,
        cr3
    );
    SpawnOutcome::Spawned(pid)
}

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

/// Pick the next runnable task and enter it, or return if there is none.
///
/// The desktop executor is the fallback: it always has a saved context once it
/// has run, and it keeps the GUI alive while every user task is blocked.
fn resume_next_or_return() {
    let picked = {
        let mut sched = crate::scheduler::SCHEDULER.lock();
        sched.clear_current();
        sched.schedule()
    };

    if let Some(next) = picked {
        if next != crate::context::IDLE_PID && crate::context::has_runnable_context(next) {
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

/// Switch to `pid` and keep running user tasks until they all block or exit.
///
/// A single `switch_to` is not enough: `wait4` parks the parent and
/// `resume_next_or_return` may hand the CPU back to the desktop *before*
/// the child has run. Drain the user run queue so Gate B4's parent actually
/// finishes (and can be reaped) before the next demo starts.
unsafe fn run_until_desktop(pid: Pid) {
    crate::context::switch_to(pid);
    loop {
        let next = {
            let mut sched = crate::scheduler::SCHEDULER.lock();
            sched.clear_current();
            sched.schedule()
        };
        match next {
            Some(n) if n > DESKTOP_PID && crate::context::has_runnable_context(n) => {
                crate::context::switch_to(n);
            }
            _ => break,
        }
    }
}

fn spawn_or_log(elf: &[u8], name: &str) -> Option<Pid> {
    match spawn_elf(elf, name, &[name], &[], None) {
        SpawnOutcome::Spawned(pid) => Some(pid),
        SpawnOutcome::NoElf => {
            serial_println!("[user_task] {}: ELF invalid", name);
            None
        }
        SpawnOutcome::NoMemory(what) => {
            serial_println!("[user_task] {}: no memory for {}", name, what);
            None
        }
    }
}

fn reap_child(pid: Pid) -> bool {
    let reaped = crate::process::PROCESS_TABLE.lock().waitpid(pid);
    if reaped.is_some() {
        crate::signals::destroy_process_signals(pid);
        crate::context::destroy_process_context(pid);
        return true;
    }
    crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .is_none()
}

// ─── Gate B3/B4/B5 boot demonstration ───────────────────────────────────

/// Serial marker the integration test waits for once a child has been spawned,
/// run in Ring 3 via `execve`, exited, and reaped.
pub const GATE_B3_MARKER: &str = "GATE_B3 wait complete";
/// Fork child ran and parent reaped it.
pub const GATE_B4_MARKER: &str = "GATE_B4 fork complete";
/// SIGKILL / SIGSEGV / PTY SIGINT all took down a user task.
pub const GATE_B5_MARKER: &str = "GATE_B5 signals complete";
/// `/bin/sh` ran in Ring 3 with a kernel PTY as its controlling terminal.
pub const GATE_B6_MARKER: &str = "GATE_B6 sh complete";
/// Loopback UDP send/recv from a Ring 3 program.
pub const GATE_D1_MARKER: &str = "GATE_D1 loopback complete";
/// Custom SIGINT handler returned via rt_sigreturn.
pub const GATE_B7_MARKER: &str = "GATE_B7 sigreturn complete";
/// Timer IRQ switched a spinning Ring 3 task that never syscalled.
pub const GATE_B8_MARKER: &str = "GATE_B8 timer preempt complete";
/// Ring 3 client presented a buffer; not an in-kernel WindowContentType app.
pub const GATE_F1_MARKER: &str = "GATE_F1 client isolated";

/// Run the scheduled-userspace demonstrations; returns when they complete.
pub fn run_gate_demos() {
    if !crate::vmm::ready() {
        serial_println!("[user_task] Gate B3+ skipped: VMM not ready");
        return;
    }
    serial_println!("[user_task] ── Gate B3–B8 + D1 + F1–F4: scheduled Ring 3 ──");
    unsafe {
        crate::context::run_in_desktop_context(gate_boot_body);
    }
    serial_println!("[user_task] ── Gate B3–B8 + D1 + F1–F4: done ──");
}

extern "C" fn gate_boot_body() {
    run_gate_b3();
    run_gate_b4();
    run_gate_b5();
    run_gate_b6();
    run_gate_d1();
    run_gate_b7();
    run_gate_b8();
    run_gate_f1();
    run_gate_f3();
    run_gate_f4();
}

fn run_gate_b3() {
    serial_println!("[user_task] Gate B3: execve(/bin/hello) + waitpid");
    let elf = crate::init::exec_hello_elf_data();
    let Some(pid) = spawn_or_log(&elf, "init-hello") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    if reap_child(pid) {
        serial_println!("[user_task] {} (pid={})", GATE_B3_MARKER, pid);
    } else {
        serial_println!(
            "[user_task] Gate B3 FAILED: pid {} still present after yield",
            pid
        );
    }
}

fn run_gate_b4() {
    serial_println!("[user_task] Gate B4: fork + child runs + wait4");
    let elf = crate::init::fork_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "fork-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate B4 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_b5() {
    serial_println!("[user_task] Gate B5: SIGKILL, SIGSEGV, PTY SIGINT");
    let mut ok_kill = false;
    let mut ok_segv = false;
    let mut ok_int = false;

    // SIGKILL a parked pause() task.
    let pause = crate::init::pause_userspace_elf_data();
    if let Some(pid) = spawn_or_log(&pause, "pause-kill") {
        unsafe {
            run_until_desktop(pid);
        }
        let _ = crate::signals::kill(pid, crate::signals::Signal::SIGKILL, 0);
        ok_kill = reap_child(pid);
        serial_println!("[user_task] Gate B5 SIGKILL pid={} reaped={}", pid, ok_kill);
    }

    // SIGSEGV: load a null pointer in Ring 3.
    let boom = crate::init::segfault_userspace_elf_data();
    if let Some(pid) = spawn_or_log(&boom, "segfault") {
        unsafe {
            run_until_desktop(pid);
        }
        ok_segv = reap_child(pid);
        serial_println!("[user_task] Gate B5 SIGSEGV pid={} reaped={}", pid, ok_segv);
    }

    // SIGINT from PTY Ctrl+C to the foreground process group.
    if let Some(pid) = spawn_or_log(&pause, "pause-int") {
        let _ = crate::pgrp::setpgid(pid, pid);
        if let Ok((pty, _)) = crate::pty::openpty() {
            let _ = crate::pty::open_slave(pty);
            let _ = crate::pty::pty_ioctl(pty, crate::pty::TIOCSPGRP, pid as u64);
            unsafe {
                run_until_desktop(pid);
            }
            let _ = crate::pty::write_master(pty, &[0x03]);
            crate::signals::deliver_signals(pid);
            ok_int = reap_child(pid);
            serial_println!("[user_task] Gate B5 SIGINT pid={} reaped={}", pid, ok_int);
        }
    }

    if ok_kill && ok_segv && ok_int {
        serial_println!("[user_task] {}", GATE_B5_MARKER);
    } else {
        serial_println!(
            "[user_task] Gate B5 partial: kill={} segv={} int={}",
            ok_kill,
            ok_segv,
            ok_int
        );
    }
}

fn run_gate_b6() {
    serial_println!("[user_task] Gate B6: /bin/sh on a PTY");
    let elf = {
        let vfs = crate::vfs::VFS.lock();
        match vfs.read_file("/bin/sh") {
            Some(data) => data.to_vec(),
            None => {
                serial_println!("[user_task] Gate B6 FAILED: /bin/sh missing");
                return;
            }
        }
    };
    let Some(pid) = spawn_or_log(&elf, "sh") else {
        return;
    };
    let _ = crate::pgrp::setpgid(pid, pid);
    let Ok((pty, _)) = crate::pty::openpty() else {
        serial_println!("[user_task] Gate B6 FAILED: openpty");
        return;
    };
    let _ = crate::pty::open_slave(pty);
    let _ = crate::pty::pty_ioctl(pty, crate::pty::TIOCSPGRP, pid as u64);
    crate::fd::attach_pty_stdio(pid, pty);
    unsafe {
        run_until_desktop(pid);
    }
    let mut prompt = [0u8; 8];
    let n = crate::pty::read_master(pty, &mut prompt).unwrap_or(0);
    let saw_prompt = n >= 2 && &prompt[..2] == b"$ ";
    let reaped = reap_child(pid);
    if reaped && saw_prompt {
        serial_println!("[user_task] {} (pid={} pty={})", GATE_B6_MARKER, pid, pty);
    } else {
        serial_println!(
            "[user_task] Gate B6 partial: pid={} reaped={} prompt={} n={}",
            pid,
            reaped,
            saw_prompt,
            n
        );
    }
}

fn run_gate_d1() {
    serial_println!("[user_task] Gate D1: loopback sockets send/recv");
    let kernel_ok = crate::net::loopback_self_test();
    if !kernel_ok {
        serial_println!("[user_task] Gate D1 FAILED: kernel loopback self-test");
        return;
    }

    let elf = crate::init::loopback_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "loopback") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    if reap_child(pid) {
        serial_println!("[user_task] {} (pid={})", GATE_D1_MARKER, pid);
    } else {
        serial_println!(
            "[user_task] Gate D1 FAILED: pid {} still present after yield",
            pid
        );
    }
}

fn run_gate_b7() {
    serial_println!("[user_task] Gate B7: SIGINT handler + rt_sigreturn");
    let elf = crate::init::sigreturn_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "sigreturn") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let _ = crate::signals::kill(pid, crate::signals::Signal::SIGINT, 0);
    crate::signals::deliver_signals(pid);
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!(
        "[user_task] Gate B7 pid={} reaped={} (marker from userspace)",
        pid,
        reaped
    );
}

fn run_gate_b8() {
    serial_println!("[user_task] Gate B8: timer preempt spinning Ring 3");
    let spin_elf = crate::init::spin_userspace_elf_data();
    let writer_elf = crate::init::preempt_writer_elf_data();
    let Some(spinner) = spawn_or_log(&spin_elf, "spin") else {
        return;
    };
    let Some(writer) = spawn_or_log(&writer_elf, "preempt-w") else {
        terminate(spinner, -(crate::signals::Signal::SIGKILL as i32));
        let _ = reap_child(spinner);
        return;
    };

    serial_println!(
        "[user_task] Gate B8: switching to spinner pid={} (writer={}) ticks={}",
        spinner,
        writer,
        crate::apic_timer::total_ticks()
    );
    let before = crate::scheduler::user_preempt_count();
    unsafe {
        crate::context::switch_to(spinner);
    }
    serial_println!(
        "[user_task] Gate B8: returned from spinner ticks={}",
        crate::apic_timer::total_ticks()
    );
    let preempts = crate::scheduler::user_preempt_count().saturating_sub(before);

    // Spinner never exits; kill it so it cannot steal the CPU again.
    terminate(spinner, -(crate::signals::Signal::SIGKILL as i32));
    if crate::context::has_runnable_context(writer) {
        unsafe {
            crate::context::switch_to(writer);
        }
    }
    let writer_reaped = reap_child(writer);
    let spinner_reaped = reap_child(spinner);
    if preempts > 0 {
        serial_println!(
            "[user_task] {} (preempts={} writer_reaped={} spinner_reaped={})",
            GATE_B8_MARKER,
            preempts,
            writer_reaped,
            spinner_reaped
        );
    } else {
        serial_println!(
            "[user_task] Gate B8 FAILED: no timer preempt (writer_reaped={})",
            writer_reaped
        );
    }
}

fn run_gate_f1() {
    serial_println!("[user_task] Gate F1: Ring 3 SHM client present");
    let elf = crate::init::display_client_elf_data();
    let Some(pid) = spawn_or_log(&elf, "wl-client") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!(
        "[user_task] Gate F1 pid={} reaped={} (marker from userspace)",
        pid,
        reaped
    );
}

fn run_gate_f3() {
    serial_println!("[user_task] Gate F3: Ring 3 terminal SHM client");
    let elf = crate::init::terminal_client_elf_data();
    let Some(pid) = spawn_or_log(&elf, "term") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!(
        "[user_task] Gate F3 pid={} reaped={} (marker from userspace)",
        pid,
        reaped
    );
}

fn run_gate_f4() {
    serial_println!("[user_task] Gate F4: Ring 3 launcher SHM client");
    let elf = crate::init::launcher_client_elf_data();
    let Some(pid) = spawn_or_log(&elf, "paint") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!(
        "[user_task] Gate F4 pid={} reaped={} (marker from userspace)",
        pid,
        reaped
    );
}

/// Spawn a Ring 3 SHM launcher without waiting (start-menu clicks).
pub fn spawn_launcher_app(name: &str) {
    let elf = crate::init::launcher_client_elf_data();
    let _ = spawn_or_log(&elf, name);
}
