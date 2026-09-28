use crate::process::Pid;
use crate::serial_println;

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
    crate::rlimit::inherit_limits(parent, pid);
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
