use super::{SyscallError, SyscallResult, read_user_string};
use crate::serial_println;
/// Syscall implementations — Process management
/// fork, execve, exit, wait4, waitid, kill, getpid, getppid,
/// process groups, sessions
use alloc::string::String;

pub fn sys_fork() -> SyscallResult {
    let ppid = crate::scheduler::current_pid().unwrap_or(1);
    let mut table = crate::process::PROCESS_TABLE.lock();
    if let Some(child_pid) = table.fork(ppid) {
        let parent_has_as = table
            .get_process(ppid)
            .map(|p| p.has_address_space)
            .unwrap_or(false);
        drop(table);

        // Fork the VMM address space (CoW) if the parent has one
        if parent_has_as {
            if !crate::vmm::fork_address_space(ppid, child_pid) {
                serial_println!(
                    "[fork] Failed to fork address space {} -> {}",
                    ppid,
                    child_pid
                );
                return Err(SyscallError::OutOfMemory);
            }
            // Clone parent's context with CR3 pointing to child's new page table
            let child_cr3 = crate::vmm::get_cr3(child_pid).unwrap_or(0);
            let parent_ctx = {
                let contexts = crate::context::PROCESS_CONTEXTS.lock();
                contexts
                    .iter()
                    .find(|pc| pc.pid == ppid)
                    .map(|pc| pc.context)
            };
            if let Some(mut ctx) = parent_ctx {
                ctx.cr3 = child_cr3;
                ctx.rax = 0; // child returns 0 from fork
                crate::context::create_user_process_context(child_pid, ctx.rip, ctx.rsp, child_cr3);
            } else {
                crate::context::create_process_context(child_pid);
            }
        } else {
            crate::context::create_process_context(child_pid);
        }

        crate::fd::create_fd_table(child_pid);
        crate::signals::create_process_signals(child_pid);
        crate::scheduler::SCHEDULER.lock().add_process(child_pid, 0);
        serial_println!("[KnoxOS] fork() -> PID {}", child_pid);
        Ok(child_pid as u64)
    } else {
        Err(SyscallError::OutOfMemory)
    }
}

pub fn sys_execve(filename_ptr: u64, _argv: u64, _envp: u64) -> SyscallResult {
    let filename =
        unsafe { read_user_string(filename_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    serial_println!("[KnoxOS] execve({}) PID={}", filename, pid);

    // Read ELF data from VFS
    let data_copy = {
        let vfs = crate::vfs::VFS.lock();
        let data = vfs.read_file(&filename).ok_or(SyscallError::FileNotFound)?;
        if !crate::elf::is_elf(data) {
            // Not an ELF — fall back to old behavior (name change only)
            let name = filename.rsplit('/').next().unwrap_or(&filename);
            drop(vfs);
            crate::process::PROCESS_TABLE.lock().exec(pid, name, &[]);
            return Ok(0);
        }
        data.to_vec()
    };

    // Destroy old address space if any
    crate::vmm::destroy_address_space(pid);

    // Create fresh address space
    if !crate::vmm::create_address_space(pid) {
        serial_println!("[execve] Failed to create address space for PID {}", pid);
        return Err(SyscallError::OutOfMemory);
    }

    // Load ELF segments into new address space
    let (entry_point, _brk) = match crate::vmm::load_elf_into_address_space(pid, &data_copy) {
        Ok(r) => r,
        Err(e) => {
            serial_println!("[execve] ELF load failed: {}", e);
            crate::vmm::destroy_address_space(pid);
            return Err(SyscallError::InvalidArgument);
        }
    };

    // Map user stack
    let stack_top = crate::vmm::STACK_TOP;
    let stack_size = crate::vmm::STACK_SIZE;
    if crate::vmm::setup_user_stack(pid, stack_top, stack_size).is_none() {
        serial_println!("[execve] Stack setup failed for PID {}", pid);
        crate::vmm::destroy_address_space(pid);
        return Err(SyscallError::OutOfMemory);
    }

    // Set up initial stack layout (argc, argv, envp, auxv)
    let argv_strs: &[&str] = &[&filename];
    let envp_strs: &[&str] = &[];
    let initial_rsp =
        crate::vmm::setup_initial_stack(pid, stack_top, argv_strs, envp_strs, entry_point, 0, 0)
            .unwrap_or(stack_top - 8);

    // Update process table entry
    let name = filename.rsplit('/').next().unwrap_or(&filename);
    {
        let mut table = crate::process::PROCESS_TABLE.lock();
        if let Some(proc) = table.get_process_mut(pid) {
            proc.name = String::from(name);
            proc.has_address_space = true;
            proc.entry_point = entry_point;
            proc.user_stack_top = initial_rsp;
        }
    }

    // Replace process context with new user-mode context
    let cr3 = crate::vmm::get_cr3(pid).unwrap_or(0);
    crate::context::destroy_process_context(pid);
    crate::context::create_user_process_context(pid, entry_point, initial_rsp, cr3);

    serial_println!(
        "[execve] PID {} ready: entry={:#x} rsp={:#x} cr3={:#x}",
        pid,
        entry_point,
        initial_rsp,
        cr3
    );
    Ok(0)
}

pub fn sys_exit(status: i32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    serial_println!("[KnoxOS] exit({}): PID {}", status, pid);
    // Gate B2: one-shot hello never sysretq's into a dead RIP.
    crate::usermode::exit_oneshot_userspace();
    crate::process::PROCESS_TABLE
        .lock()
        .set_state(pid, crate::process::ProcessState::Zombie);
    crate::scheduler::SCHEDULER.lock().remove_process(pid);
    let ppid = crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .map(|p| p.ppid)
        .unwrap_or(0);
    if ppid > 0 {
        let _ = crate::signals::kill(ppid, crate::signals::Signal::SIGCHLD, pid);
    }
    // Clean up VMM address space and context
    crate::vmm::destroy_address_space(pid);
    crate::context::destroy_process_context(pid);
    crate::fd::destroy_fd_table(pid);
    Ok(0)
}

pub fn sys_wait4(pid: i32, wstatus_ptr: u64, _options: i32) -> SyscallResult {
    let wait_pid = if pid == -1 { 0 } else { pid as u32 };
    let mut table = crate::process::PROCESS_TABLE.lock();
    if let Some((child_pid, status)) = table.waitpid(wait_pid) {
        if wstatus_ptr != 0 {
            unsafe {
                *(wstatus_ptr as *mut i32) = status;
            }
        }
        crate::signals::destroy_process_signals(child_pid);
        Ok(child_pid as u64)
    } else {
        if _options & 1 != 0 {
            Ok(0)
        } else {
            Err(SyscallError::Interrupted)
        }
    }
}

/// waitid(idtype, id, infop, options) — wait for process state changes
pub fn sys_waitid(idtype: i32, id: u32, infop: u64, options: i32) -> SyscallResult {
    const P_PID: i32 = 1;
    const P_ALL: i32 = 0;
    const WNOHANG: i32 = 1;

    let target_pid = match idtype {
        P_PID => id as i32,
        P_ALL => -1,
        _ => return Err(SyscallError::InvalidArgument),
    };

    let current_pid = crate::scheduler::current_pid().unwrap_or(1);

    if let Some((child_pid, exit_code)) = crate::process::wait_child(current_pid, target_pid) {
        if infop != 0 {
            unsafe {
                core::ptr::write_bytes(infop as *mut u8, 0, 128);
                *(infop as *mut i32) = 17; // SIGCHLD
                *((infop + 8) as *mut i32) = 1; // CLD_EXITED
                *((infop + 16) as *mut i32) = child_pid as i32;
                *((infop + 24) as *mut i32) = exit_code;
            }
        }
        Ok(0)
    } else if options & WNOHANG != 0 {
        if infop != 0 {
            unsafe {
                core::ptr::write_bytes(infop as *mut u8, 0, 128);
            }
        }
        Ok(0)
    } else {
        Err(SyscallError::NoSuchProcess)
    }
}

pub fn sys_kill(pid: u32, sig: u32) -> SyscallResult {
    let signal = crate::signals::Signal::from_number(sig).ok_or(SyscallError::InvalidArgument)?;
    let sender = crate::scheduler::current_pid().unwrap_or(0);
    crate::signals::kill(pid, signal, sender).map_err(|_| SyscallError::NoSuchProcess)?;
    Ok(0)
}

pub fn sys_getpid() -> SyscallResult {
    Ok(crate::scheduler::current_pid().unwrap_or(1) as u64)
}

pub fn sys_getppid() -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let table = crate::process::PROCESS_TABLE.lock();
    Ok(table.get_process(pid).map(|p| p.ppid).unwrap_or(0) as u64)
}

// ── Process groups & sessions ───────────────────────────────────────

pub fn sys_setpgid(pid: u32, pgid: u32) -> SyscallResult {
    let real_pid = if pid == 0 {
        crate::scheduler::current_pid().unwrap_or(1)
    } else {
        pid
    };
    crate::pgrp::setpgid(real_pid, pgid)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::PermissionDenied)
}

pub fn sys_getpgid(pid: u32) -> SyscallResult {
    let real_pid = if pid == 0 {
        crate::scheduler::current_pid().unwrap_or(1)
    } else {
        pid
    };
    crate::pgrp::getpgid(real_pid)
        .map(|pgid| pgid as u64)
        .map_err(|_| SyscallError::NoSuchProcess)
}

pub fn sys_getpgrp() -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    crate::pgrp::getpgid(pid)
        .map(|pgid| pgid as u64)
        .map_err(|_| SyscallError::NoSuchProcess)
}

pub fn sys_setsid() -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    crate::pgrp::setsid(pid)
        .map(|sid| sid as u64)
        .map_err(|_| SyscallError::PermissionDenied)
}

pub fn sys_getsid(pid: u32) -> SyscallResult {
    let real_pid = if pid == 0 {
        crate::scheduler::current_pid().unwrap_or(1)
    } else {
        pid
    };
    crate::pgrp::getsid(real_pid)
        .map(|sid| sid as u64)
        .map_err(|_| SyscallError::NoSuchProcess)
}
