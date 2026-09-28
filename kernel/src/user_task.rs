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
    crate::namespaces::inherit_namespaces(pid, parent);
    let _ = crate::pidns::on_fork(parent, pid);
    let uid = crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .map(|p| p.uid)
        .unwrap_or(0);
    crate::capabilities::init_process_caps(pid, parent);
    crate::capabilities::apply_exec_caps(pid, uid);
    crate::pgrp::register_process(pid, parent);
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
            Some(n)
                if n > DESKTOP_PID
                    && !crate::context::is_ap_idle_pid(n)
                    && crate::context::has_runnable_context(n) =>
            {
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
/// Timer IRQ saved user GPRs + FXSAVE (spinner RBX magic survived).
pub const GATE_I2_MARKER: &str = "GATE_I2 irq gprs";
/// Application Processor ran a Ring 3 task (`getcpu` reported CPU 1).
pub const GATE_I3_MARKER: &str = "GATE_I3 ap ring3";
/// Ring 3 client presented a buffer; not an in-kernel WindowContentType app.
pub const GATE_F1_MARKER: &str = "GATE_F1 client isolated";

/// Run the scheduled-userspace demonstrations; returns when they complete.
pub fn run_gate_demos() {
    if !crate::vmm::ready() {
        serial_println!("[user_task] Gate B3+ skipped: VMM not ready");
        return;
    }
    serial_println!(
        "[user_task] ── Gate B3–B8 + D1 + F1–F4 + J1 + J4 + K1 + K3 + L1 + L4 + M2–M4 + N2–N4 + O2–O4 + P2–P4: scheduled Ring 3 ──"
    );
    unsafe {
        crate::context::run_in_desktop_context(gate_boot_body);
    }
    serial_println!(
        "[user_task] ── Gate B3–B8 + D1 + F1–F4 + J1 + J4 + K1 + K3 + L1 + L4 + M2–M4 + N2–N4 + O2–O4 + P2–P4 + Q2–Q4 + R2–R4 + S2–S4 + T2–T4 + U2–U4 + V2–V4 + W2–W4 + X2–X4 + Y2–Y4 + Z2–Z4: done ──"
    );
}

extern "C" fn gate_boot_body() {
    run_gate_b3();
    run_gate_b4();
    run_gate_b5();
    run_gate_b6();
    run_gate_d1();
    run_gate_b7();
    run_gate_b8();
    run_gate_i3();
    run_gate_f1();
    run_gate_f3();
    run_gate_f4();
    run_gate_j1();
    run_gate_j4();
    run_gate_k1();
    run_gate_k3();
    run_gate_l1();
    run_gate_l4();
    run_gate_m2();
    run_gate_m3();
    run_gate_m4();
    run_gate_n2();
    run_gate_n3();
    run_gate_n4();
    run_gate_o2();
    run_gate_o3();
    run_gate_o4();
    run_gate_p2();
    run_gate_p3();
    run_gate_p4();
    run_gate_q2();
    run_gate_q3();
    run_gate_q4();
    run_gate_r2();
    run_gate_r3();
    run_gate_r4();
    run_gate_s2();
    run_gate_s3();
    run_gate_s4();
    run_gate_t2();
    run_gate_t3();
    run_gate_t4();
    run_gate_u2();
    run_gate_u3();
    run_gate_u4();
    run_gate_v2();
    run_gate_v3();
    run_gate_v4();
    run_gate_w2();
    run_gate_w3();
    run_gate_w4();
    run_gate_x2();
    run_gate_x3();
    run_gate_x4();
    run_gate_y2();
    run_gate_y3();
    run_gate_y4();
    run_gate_z2();
    run_gate_z3();
    run_gate_z4();
    run_gate_aa2();
    run_gate_aa3();
    run_gate_aa4();
    run_gate_ab2();
    run_gate_ab3();
    run_gate_ab4();
    run_gate_ac2();
    run_gate_ac3();
    run_gate_ac4();
    run_gate_ad2();
    run_gate_ad3();
    run_gate_ad4();
    run_gate_ae2();
    run_gate_ae3();
    run_gate_ae4();
    run_gate_af2();
    run_gate_af3();
    run_gate_af4();
    run_gate_ag2();
    run_gate_ag3();
    run_gate_ag4();
    run_gate_ah2();
    run_gate_ah3();
    run_gate_ah4();
    run_gate_ai2();
    run_gate_ai3();
    run_gate_ai4();
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

fn run_gate_j1() {
    serial_println!("[user_task] Gate J1: clone(CLONE_VM) child shares CR3");
    let elf = crate::init::clone_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "clone-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate J1 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_j4() {
    serial_println!("[user_task] Gate J4: bpf returns ENOSYS");
    let elf = crate::init::enosys_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "enosys-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate J4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_K1_MARKER: &str = "GATE_K1 thread join";
pub const GATE_K3_MARKER: &str = "GATE_K3 init userspace";

fn run_gate_k1() {
    serial_println!("[user_task] Gate K1: clone(CLONE_THREAD) + thread_join");
    let elf = crate::init::clone_thread_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "clone-thread") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate K1 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_k3() {
    serial_println!("[user_task] Gate K3: /sbin/init in Ring 3");
    let elf = {
        let vfs = crate::vfs::VFS.lock();
        match vfs.read_file("/sbin/init") {
            Some(data) => data.to_vec(),
            None => crate::init::builtin_init_elf_data(),
        }
    };
    let Some(pid) = spawn_or_log(&elf, "init") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let alive = crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .map(|p| p.state != crate::process::ProcessState::Zombie)
        .unwrap_or(false);
    if alive {
        serial_println!("[user_task] {} (pid={})", GATE_K3_MARKER, pid);
        terminate(pid, 0);
        let _ = reap_child(pid);
    } else {
        serial_println!(
            "[user_task] Gate K3 FAILED: /sbin/init pid={} did not stay parked",
            pid
        );
        let _ = reap_child(pid);
    }
}

pub const GATE_L1_MARKER: &str = "GATE_L1 tls fs";
pub const GATE_L4_MARKER: &str = "GATE_L4 enosys";

fn run_gate_l1() {
    serial_println!("[user_task] Gate L1: arch_prctl ARCH_SET_FS + %fs:0");
    let elf = crate::init::tls_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "tls-fs") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate L1 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_l4() {
    serial_println!("[user_task] Gate L4: quotactl returns ENOSYS");
    let elf = crate::init::quotactl_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "quotactl-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate L4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_M2_MARKER: &str = "GATE_M2 pipe";
pub const GATE_M3_MARKER: &str = "GATE_M3 futex";
pub const GATE_M4_MARKER: &str = "GATE_M4 enosys";

fn run_gate_m2() {
    serial_println!("[user_task] Gate M2: pipe() write/read round-trip");
    let elf = crate::init::pipe_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "pipe-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate M2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_m3() {
    serial_println!("[user_task] Gate M3: futex wait/wake");
    let elf = crate::init::futex_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "futex-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate M3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_m4() {
    serial_println!("[user_task] Gate M4: io_uring_setup returns ENOSYS");
    let elf = crate::init::io_uring_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "io-uring-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate M4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_N2_MARKER: &str = "GATE_N2 socketpair";
pub const GATE_N3_MARKER: &str = "GATE_N3 eventfd";
pub const GATE_N4_MARKER: &str = "GATE_N4 enosys";

fn run_gate_n2() {
    serial_println!("[user_task] Gate N2: socketpair write/read round-trip");
    let elf = crate::init::socketpair_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "socketpair-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate N2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_n3() {
    serial_println!("[user_task] Gate N3: eventfd write/read");
    let elf = crate::init::eventfd_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "eventfd-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate N3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_n4() {
    serial_println!("[user_task] Gate N4: userfaultfd returns ENOSYS");
    let elf = crate::init::userfaultfd_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "uffd-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate N4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_O2_MARKER: &str = "GATE_O2 epoll";
pub const GATE_O3_MARKER: &str = "GATE_O3 memfd";
pub const GATE_O4_MARKER: &str = "GATE_O4 enosys";

fn run_gate_o2() {
    serial_println!("[user_task] Gate O2: epoll wait on a pipe");
    let elf = crate::init::epoll_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "epoll-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate O2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_o3() {
    serial_println!("[user_task] Gate O3: memfd write/read");
    let elf = crate::init::memfd_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "memfd-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate O3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_o4() {
    serial_println!("[user_task] Gate O4: perf_event_open returns ENOSYS");
    let elf = crate::init::perf_event_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "perf-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate O4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_P2_MARKER: &str = "GATE_P2 timerfd";
pub const GATE_P3_MARKER: &str = "GATE_P3 signalfd";
pub const GATE_P4_MARKER: &str = "GATE_P4 enosys";

fn run_gate_p2() {
    serial_println!("[user_task] Gate P2: timerfd expire/read");
    let elf = crate::init::timerfd_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "timerfd-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate P2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_p3() {
    serial_println!("[user_task] Gate P3: signalfd SIGUSR1");
    let elf = crate::init::signalfd_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "signalfd-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate P3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_p4() {
    serial_println!("[user_task] Gate P4: fanotify_init returns ENOSYS");
    let elf = crate::init::fanotify_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "fanotify-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate P4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_Q2_MARKER: &str = "GATE_Q2 poll";
pub const GATE_Q3_MARKER: &str = "GATE_Q3 inotify";
pub const GATE_Q4_MARKER: &str = "GATE_Q4 enosys";

fn run_gate_q2() {
    serial_println!("[user_task] Gate Q2: poll on a pipe");
    let elf = crate::init::poll_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "poll-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate Q2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_q3() {
    serial_println!("[user_task] Gate Q3: inotify_init1 + add_watch + read");
    let elf = crate::init::inotify_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "inotify-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate Q3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_q4() {
    serial_println!("[user_task] Gate Q4: io_setup returns ENOSYS");
    let elf = crate::init::io_setup_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "aio-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate Q4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_R2_MARKER: &str = "GATE_R2 splice";
pub const GATE_R3_MARKER: &str = "GATE_R3 flock";
pub const GATE_R4_MARKER: &str = "GATE_R4 enosys";

fn run_gate_r2() {
    serial_println!("[user_task] Gate R2: splice pipe to pipe");
    let elf = crate::init::splice_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "splice-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate R2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_r3() {
    serial_println!("[user_task] Gate R3: flock exclusive lock then unlock");
    let elf = crate::init::flock_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "flock-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate R3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_r4() {
    serial_println!("[user_task] Gate R4: kexec_load returns ENOSYS");
    let elf = crate::init::kexec_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "kexec-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate R4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_S2_MARKER: &str = "GATE_S2 sendfile";
pub const GATE_S3_MARKER: &str = "GATE_S3 tee";
pub const GATE_S4_MARKER: &str = "GATE_S4 enosys";

fn run_gate_s2() {
    serial_println!("[user_task] Gate S2: sendfile file to pipe");
    let elf = crate::init::sendfile_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "sendfile-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate S2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_s3() {
    serial_println!("[user_task] Gate S3: tee without consuming the source");
    let elf = crate::init::tee_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "tee-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate S3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_s4() {
    serial_println!("[user_task] Gate S4: init_module returns ENOSYS");
    let elf = crate::init::init_module_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "initmod-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate S4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_T2_MARKER: &str = "GATE_T2 copy_file_range";
pub const GATE_T3_MARKER: &str = "GATE_T3 vmsplice";
pub const GATE_T4_MARKER: &str = "GATE_T4 enosys";

fn run_gate_t2() {
    serial_println!("[user_task] Gate T2: copy_file_range file to file");
    let elf = crate::init::copy_file_range_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "copyfr-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate T2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_t3() {
    serial_println!("[user_task] Gate T3: vmsplice user page into pipe");
    let elf = crate::init::vmsplice_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "vmsplice-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate T3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_t4() {
    serial_println!("[user_task] Gate T4: mount_setattr returns ENOSYS");
    let elf = crate::init::mount_setattr_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "mountattr-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate T4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_U2_MARKER: &str = "GATE_U2 xattr";
pub const GATE_U3_MARKER: &str = "GATE_U3 statx";
pub const GATE_U4_MARKER: &str = "GATE_U4 enosys";

fn run_gate_u2() {
    serial_println!("[user_task] Gate U2: setxattr/getxattr round-trip");
    let elf = crate::init::xattr_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "xattr-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate U2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_u3() {
    serial_println!("[user_task] Gate U3: statx file size");
    let elf = crate::init::statx_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "statx-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate U3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_u4() {
    serial_println!("[user_task] Gate U4: fsopen returns ENOSYS");
    let elf = crate::init::fsopen_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "fsopen-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate U4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_V2_MARKER: &str = "GATE_V2 fallocate";
pub const GATE_V3_MARKER: &str = "GATE_V3 utimensat";
pub const GATE_V4_MARKER: &str = "GATE_V4 enosys";

fn run_gate_v2() {
    serial_println!("[user_task] Gate V2: fallocate then statx size");
    let elf = crate::init::fallocate_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "fallocate-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate V2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_v3() {
    serial_println!("[user_task] Gate V3: utimensat then statx mtime");
    let elf = crate::init::utimensat_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "utimensat-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate V3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_v4() {
    serial_println!("[user_task] Gate V4: keyctl returns ENOSYS");
    let elf = crate::init::keyctl_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "keyctl-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate V4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_W2_MARKER: &str = "GATE_W2 umask";
pub const GATE_W3_MARKER: &str = "GATE_W3 symlink";
pub const GATE_W4_MARKER: &str = "GATE_W4 enosys";

fn run_gate_w2() {
    serial_println!("[user_task] Gate W2: umask applied on creat");
    let elf = crate::init::umask_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "umask-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate W2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_w3() {
    serial_println!("[user_task] Gate W3: symlink then readlink");
    let elf = crate::init::symlink_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "symlink-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate W3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_w4() {
    serial_println!("[user_task] Gate W4: ioperm returns ENOSYS");
    let elf = crate::init::ioperm_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "ioperm-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate W4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_X2_MARKER: &str = "GATE_X2 rename";
pub const GATE_X3_MARKER: &str = "GATE_X3 truncate";
pub const GATE_X4_MARKER: &str = "GATE_X4 enosys";

fn run_gate_x2() {
    serial_println!("[user_task] Gate X2: rename then read new path");
    let elf = crate::init::rename_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "rename-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate X2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_x3() {
    serial_println!("[user_task] Gate X3: truncate then statx size");
    let elf = crate::init::truncate_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "truncate-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate X3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_x4() {
    serial_println!("[user_task] Gate X4: iopl returns ENOSYS");
    let elf = crate::init::iopl_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "iopl-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate X4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_Y2_MARKER: &str = "GATE_Y2 chown";
pub const GATE_Y3_MARKER: &str = "GATE_Y3 mkdir";
pub const GATE_Y4_MARKER: &str = "GATE_Y4 enosys";

fn run_gate_y2() {
    serial_println!("[user_task] Gate Y2: chown then statx uid");
    let elf = crate::init::chown_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "chown-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate Y2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_y3() {
    serial_println!("[user_task] Gate Y3: mkdir then statx S_IFDIR");
    let elf = crate::init::mkdir_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "mkdir-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate Y3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_y4() {
    serial_println!("[user_task] Gate Y4: acct returns ENOSYS");
    let elf = crate::init::acct_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "acct-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate Y4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_Z2_MARKER: &str = "GATE_Z2 unlink";
pub const GATE_Z3_MARKER: &str = "GATE_Z3 chdir";
pub const GATE_Z4_MARKER: &str = "GATE_Z4 enosys";

fn run_gate_z2() {
    serial_println!("[user_task] Gate Z2: unlink then open fails");
    let elf = crate::init::unlink_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "unlink-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate Z2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_z3() {
    serial_println!("[user_task] Gate Z3: chdir then getcwd");
    let elf = crate::init::chdir_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "chdir-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate Z3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_z4() {
    serial_println!("[user_task] Gate Z4: swapon returns ENOSYS");
    let elf = crate::init::swapon_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "swapon-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate Z4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_AA2_MARKER: &str = "GATE_AA2 fchdir";
pub const GATE_AA3_MARKER: &str = "GATE_AA3 access";
pub const GATE_AA4_MARKER: &str = "GATE_AA4 enosys";

fn run_gate_aa2() {
    serial_println!("[user_task] Gate AA2: fchdir then getcwd");
    let elf = crate::init::fchdir_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "fchdir-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AA2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_aa3() {
    serial_println!("[user_task] Gate AA3: access F_OK then missing fails");
    let elf = crate::init::access_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "access-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AA3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_aa4() {
    serial_println!("[user_task] Gate AA4: modify_ldt returns ENOSYS");
    let elf = crate::init::modify_ldt_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "modify-ldt-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AA4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_AB2_MARKER: &str = "GATE_AB2 dup2";
pub const GATE_AB3_MARKER: &str = "GATE_AB3 uname";
pub const GATE_AB4_MARKER: &str = "GATE_AB4 enosys";

fn run_gate_ab2() {
    serial_println!("[user_task] Gate AB2: dup2 then read via new fd");
    let elf = crate::init::dup2_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "dup2-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AB2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_ab3() {
    serial_println!("[user_task] Gate AB3: uname sysname KnoxOS");
    let elf = crate::init::uname_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "uname-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AB3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_ab4() {
    serial_println!("[user_task] Gate AB4: sysfs returns ENOSYS");
    let elf = crate::init::sysfs_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "sysfs-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AB4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_AC2_MARKER: &str = "GATE_AC2 pread64";
pub const GATE_AC3_MARKER: &str = "GATE_AC3 getuid";
pub const GATE_AC4_MARKER: &str = "GATE_AC4 enosys";

fn run_gate_ac2() {
    serial_println!("[user_task] Gate AC2: pread64 at offset without moving pos");
    let elf = crate::init::pread64_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "pread64-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AC2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_ac3() {
    serial_println!("[user_task] Gate AC3: getuid is 0");
    let elf = crate::init::getuid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getuid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AC3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_ac4() {
    serial_println!("[user_task] Gate AC4: vhangup returns ENOSYS");
    let elf = crate::init::vhangup_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "vhangup-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AC4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_AD2_MARKER: &str = "GATE_AD2 pwrite64";
pub const GATE_AD3_MARKER: &str = "GATE_AD3 getgid";
pub const GATE_AD4_MARKER: &str = "GATE_AD4 enosys";

fn run_gate_ad2() {
    serial_println!("[user_task] Gate AD2: pwrite64 at offset without moving pos");
    let elf = crate::init::pwrite64_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "pwrite64-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AD2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_ad3() {
    serial_println!("[user_task] Gate AD3: getgid is 1000");
    let elf = crate::init::getgid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getgid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AD3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_ad4() {
    serial_println!("[user_task] Gate AD4: lookup_dcookie returns ENOSYS");
    let elf = crate::init::lookup_dcookie_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "lookup-dcookie-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AD4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_AE2_MARKER: &str = "GATE_AE2 ftruncate";
pub const GATE_AE3_MARKER: &str = "GATE_AE3 geteuid";
pub const GATE_AE4_MARKER: &str = "GATE_AE4 enosys";

fn run_gate_ae2() {
    serial_println!("[user_task] Gate AE2: ftruncate then statx size 1");
    let elf = crate::init::ftruncate_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "ftruncate-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AE2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_ae3() {
    serial_println!("[user_task] Gate AE3: geteuid is 0");
    let elf = crate::init::geteuid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "geteuid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AE3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_ae4() {
    serial_println!("[user_task] Gate AE4: memfd_secret returns ENOSYS");
    let elf = crate::init::memfd_secret_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "memfd-secret-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AE4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_AF2_MARKER: &str = "GATE_AF2 lseek";
pub const GATE_AF3_MARKER: &str = "GATE_AF3 getegid";
pub const GATE_AF4_MARKER: &str = "GATE_AF4 enosys";

fn run_gate_af2() {
    serial_println!("[user_task] Gate AF2: lseek SET 1 then read y");
    let elf = crate::init::lseek_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "lseek-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AF2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_af3() {
    serial_println!("[user_task] Gate AF3: getegid is 1000");
    let elf = crate::init::getegid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getegid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AF3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_af4() {
    serial_println!("[user_task] Gate AF4: uselib returns ENOSYS");
    let elf = crate::init::uselib_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "uselib-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AF4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_AG2_MARKER: &str = "GATE_AG2 fdatasync";
pub const GATE_AG3_MARKER: &str = "GATE_AG3 getppid";
pub const GATE_AG4_MARKER: &str = "GATE_AG4 enosys";

fn run_gate_ag2() {
    serial_println!("[user_task] Gate AG2: fdatasync after write");
    let elf = crate::init::fdatasync_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "fdatasync-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AG2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_ag3() {
    serial_println!("[user_task] Gate AG3: getppid is non-zero");
    let elf = crate::init::getppid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getppid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AG3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_ag4() {
    serial_println!("[user_task] Gate AG4: pkey_alloc returns ENOSYS");
    let elf = crate::init::pkey_alloc_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "pkey-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AG4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_AH2_MARKER: &str = "GATE_AH2 sync";
pub const GATE_AH3_MARKER: &str = "GATE_AH3 getpgid";
pub const GATE_AH4_MARKER: &str = "GATE_AH4 enosys";

fn run_gate_ah2() {
    serial_println!("[user_task] Gate AH2: sync after write");
    let elf = crate::init::sync_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "sync-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AH2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_ah3() {
    serial_println!("[user_task] Gate AH3: getpgid is non-zero");
    let elf = crate::init::getpgid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getpgid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AH3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_ah4() {
    serial_println!("[user_task] Gate AH4: pkey_mprotect returns ENOSYS");
    let elf = crate::init::pkey_mprotect_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "pkey-mprotect-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AH4 parent pid={} reaped={}", pid, reaped);
}

pub const GATE_AI2_MARKER: &str = "GATE_AI2 fchown";
pub const GATE_AI3_MARKER: &str = "GATE_AI3 getsid";
pub const GATE_AI4_MARKER: &str = "GATE_AI4 enosys";

fn run_gate_ai2() {
    serial_println!("[user_task] Gate AI2: fchown then statx uid");
    let elf = crate::init::fchown_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "fchown-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AI2 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_ai3() {
    serial_println!("[user_task] Gate AI3: getsid is non-zero");
    let elf = crate::init::getsid_userspace_elf_data();
    let Some(pid) = spawn_or_log(&elf, "getsid-demo") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AI3 parent pid={} reaped={}", pid, reaped);
}

fn run_gate_ai4() {
    serial_println!("[user_task] Gate AI4: pkey_free returns ENOSYS");
    let elf = crate::init::pkey_free_enosys_elf_data();
    let Some(pid) = spawn_or_log(&elf, "pkey-free-enosys") else {
        return;
    };
    unsafe {
        run_until_desktop(pid);
    }
    let reaped = reap_child(pid);
    serial_println!("[user_task] Gate AI4 parent pid={} reaped={}", pid, reaped);
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

    let snap = crate::context::snapshot(spinner);
    let gpr_ok = snap
        .as_ref()
        .map(|c| {
            c.rbx == crate::init::SPIN_RBX_MAGIC
                && c.fpu_initialized
                && c.fxsave_area.iter().any(|&b| b != 0)
        })
        .unwrap_or(false);
    if gpr_ok {
        serial_println!(
            "[user_task] {} (rbx={:#x} fxsave)",
            GATE_I2_MARKER,
            crate::init::SPIN_RBX_MAGIC
        );
    } else {
        let rbx = snap.map(|c| c.rbx).unwrap_or(0);
        serial_println!(
            "[user_task] Gate I2 FAILED: spinner rbx={:#x} want={:#x}",
            rbx,
            crate::init::SPIN_RBX_MAGIC
        );
    }

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

fn run_gate_i3() {
    serial_println!("[user_task] Gate I3: AP runs Ring 3");
    if crate::smp::online_cpus() < 2 {
        serial_println!("[user_task] Gate I3 skipped: only one CPU online");
        return;
    }

    crate::smp::clear_last_user_cpu();
    let elf = crate::init::ap_ring3_elf_data();
    let Some(pid) = spawn_or_log(&elf, "ap-ring3") else {
        return;
    };
    if crate::scheduler::set_cpu_affinity(pid, 1 << 1).is_err() {
        serial_println!("[user_task] Gate I3 FAILED: affinity pin");
        terminate(pid, -(crate::signals::Signal::SIGKILL as i32));
        let _ = reap_child(pid);
        return;
    }
    crate::smp::enqueue_on_cpu(1, pid);

    let start = crate::apic_timer::total_ticks();
    loop {
        let cpu = crate::smp::last_user_cpu();
        let idle = crate::smp::ap_in_idle(1);
        let zombie = crate::process::PROCESS_TABLE
            .lock()
            .get_process(pid)
            .is_none_or(|p| p.state == crate::process::ProcessState::Zombie);
        if cpu == 1 && idle && zombie {
            break;
        }
        if crate::apic_timer::total_ticks().saturating_sub(start) > 2000 {
            serial_println!(
                "[user_task] Gate I3 timeout: last_cpu={} idle={} zombie={}",
                cpu,
                idle,
                zombie
            );
            break;
        }
        core::hint::spin_loop();
    }

    let cpu = crate::smp::last_user_cpu();
    let reaped = reap_child(pid);
    if cpu == 1 && reaped {
        serial_println!(
            "[user_task] {} (pid={} cpu={} last_pid={})",
            GATE_I3_MARKER,
            pid,
            cpu,
            crate::smp::last_user_pid()
        );
    } else {
        serial_println!(
            "[user_task] Gate I3 FAILED: pid={} reaped={} cpu={} (want 1)",
            pid,
            reaped,
            cpu
        );
        if crate::context::has_runnable_context(pid) {
            terminate(pid, -(crate::signals::Signal::SIGKILL as i32));
            let _ = reap_child(pid);
        }
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
