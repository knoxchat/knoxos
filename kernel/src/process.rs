/// Process Management - Basic process tracking for the OS
/// Provides Linux-compatible process IDs and state tracking
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Process identifier (Linux-compatible PID)
pub type Pid = u32;

/// Process state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Running,
    Ready,
    Sleeping,
    Stopped,
    Zombie,
}

/// A process in the system
#[derive(Debug, Clone)]
pub struct Process {
    pub pid: Pid,
    pub ppid: Pid, // Parent PID
    pub name: String,
    pub state: ProcessState,
    pub uid: u32,
    pub gid: u32,
    pub cwd: String,  // Current working directory
    pub priority: i8, // Nice value (-20 to 19)
    /// Whether this process has a per-process address space (VMM)
    pub has_address_space: bool,
    /// Entry point (for user-mode processes)
    pub entry_point: u64,
    /// User stack pointer
    pub user_stack_top: u64,
    /// Exit code (valid when state == Zombie)
    pub exit_code: i32,
}

/// Process table
pub struct ProcessTable {
    pub processes: Vec<Process>,
    next_pid: Pid,
}

lazy_static::lazy_static! {
    pub static ref PROCESS_TABLE: Mutex<ProcessTable> = Mutex::new(ProcessTable::new());
}

impl Default for ProcessTable {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessTable {
    pub fn new() -> Self {
        let mut table = Self {
            processes: Vec::new(),
            next_pid: 1,
        };

        // Create init process (PID 1) - like Linux init/systemd
        table.processes.push(Process {
            pid: 0,
            ppid: 0,
            name: String::from("[kernel]"),
            state: ProcessState::Running,
            uid: 0,
            gid: 0,
            cwd: String::from("/"),
            priority: 0,
            has_address_space: false,
            entry_point: 0,
            user_stack_top: 0,
            exit_code: 0,
        });

        table.processes.push(Process {
            pid: 1,
            ppid: 0,
            name: String::from("init"),
            state: ProcessState::Running,
            uid: 0,
            gid: 0,
            cwd: String::from("/"),
            priority: 0,
            has_address_space: false,
            entry_point: 0,
            user_stack_top: 0,
            exit_code: 0,
        });

        // Desktop environment process
        table.processes.push(Process {
            pid: 2,
            ppid: 1,
            name: String::from("knoxos-desktop"),
            state: ProcessState::Running,
            uid: 1000,
            gid: 1000,
            cwd: String::from("/home/user"),
            priority: 0,
            has_address_space: false,
            entry_point: 0,
            user_stack_top: 0,
            exit_code: 0,
        });

        table.next_pid = 3;
        table
    }

    /// Spawn a new process
    pub fn spawn(&mut self, name: &str, ppid: Pid) -> Pid {
        let pid = self.next_pid;
        self.next_pid += 1;

        self.processes.push(Process {
            pid,
            ppid,
            name: String::from(name),
            state: ProcessState::Ready,
            uid: 1000,
            gid: 1000,
            cwd: String::from("/home/user"),
            priority: 0,
            has_address_space: false,
            entry_point: 0,
            user_stack_top: 0,
            exit_code: 0,
        });

        crate::serial_println!("[KnoxOS] Process spawned: PID={} name={}", pid, name);
        pid
    }

    /// Kill a process — transitions to Zombie, sends SIGCHLD to parent
    pub fn kill(&mut self, pid: Pid) -> bool {
        self.exit_process(pid, -9) // Killed by SIGKILL
    }

    /// Graceful process exit with status code
    /// - Transitions to Zombie state (retained until parent calls waitpid)
    /// - Sends SIGCHLD to the parent process
    /// - Reparents orphaned children to init (PID 1)
    pub fn exit_process(&mut self, pid: Pid, exit_code: i32) -> bool {
        // Prevent killing PID 0 (kernel) or PID 1 (init)
        if pid <= 1 {
            return false;
        }

        let ppid = if let Some(proc) = self.processes.iter_mut().find(|p| p.pid == pid) {
            if proc.state == ProcessState::Zombie {
                return false; // Already exited
            }
            proc.state = ProcessState::Zombie;
            proc.exit_code = exit_code;
            crate::serial_println!(
                "[KnoxOS] Process exited: PID={} name={} code={}",
                pid,
                proc.name,
                exit_code
            );
            proc.ppid
        } else {
            return false;
        };

        // Reparent orphaned children to init (PID 1)
        for child in self.processes.iter_mut() {
            if child.ppid == pid && child.state != ProcessState::Zombie {
                child.ppid = 1;
            }
        }

        // Send SIGCHLD to the parent (must drop self borrow first)
        // We store ppid and deliver after returning
        let _ = crate::signals::kill(ppid, crate::signals::Signal::SIGCHLD, pid);

        true
    }

    /// Get process by PID
    pub fn get_process(&self, pid: Pid) -> Option<&Process> {
        self.processes.iter().find(|p| p.pid == pid)
    }

    /// Get mutable process by PID
    pub fn get_process_mut(&mut self, pid: Pid) -> Option<&mut Process> {
        self.processes.iter_mut().find(|p| p.pid == pid)
    }

    /// Get process count
    pub fn count(&self) -> usize {
        self.processes
            .iter()
            .filter(|p| p.state != ProcessState::Zombie)
            .count()
    }

    /// List all PIDs
    pub fn list_pids(&self) -> Vec<Pid> {
        self.processes
            .iter()
            .filter(|p| p.state != ProcessState::Zombie)
            .map(|p| p.pid)
            .collect()
    }

    /// List all active processes
    pub fn list_processes(&self) -> &[Process] {
        &self.processes
    }

    /// Wait for a child process to change state (waitpid)
    /// Supports WNOHANG behavior: if target_pid is negative, waits for any child.
    /// Returns (child_pid, exit_status) if a matching zombie child is found.
    pub fn waitpid(&mut self, pid: Pid) -> Option<(Pid, i32)> {
        // Find zombie child
        if let Some(pos) = self
            .processes
            .iter()
            .position(|p| (pid == 0 || p.pid == pid) && p.state == ProcessState::Zombie)
        {
            let child_pid = self.processes[pos].pid;
            let exit_code = self.processes[pos].exit_code;
            self.processes.remove(pos);
            Some((child_pid, exit_code))
        } else {
            None
        }
    }

    /// waitpid with options — WNOHANG support
    /// Returns Ok(Some((pid, status))) on success, Ok(None) for WNOHANG with no ready child,
    /// Err(-10) if no children exist (ECHILD)
    pub fn waitpid_options(
        &mut self,
        target_pid: i32,
        parent_pid: Pid,
        wnohang: bool,
    ) -> Result<Option<(Pid, i32)>, i32> {
        // Check if any matching children exist
        let has_children = self
            .processes
            .iter()
            .any(|p| p.ppid == parent_pid && (target_pid <= 0 || p.pid == target_pid as u32));

        if !has_children {
            return Err(-10); // ECHILD — no matching children
        }

        // Look for zombie (exited) children
        let zombie_pos = self.processes.iter().position(|p| {
            p.ppid == parent_pid
                && (target_pid <= 0 || p.pid == target_pid as u32)
                && p.state == ProcessState::Zombie
        });

        if let Some(pos) = zombie_pos {
            let child_pid = self.processes[pos].pid;
            let exit_code = self.processes[pos].exit_code;
            self.processes.remove(pos);
            return Ok(Some((child_pid, exit_code)));
        }

        // Look for stopped children (for WUNTRACED)
        let stopped_pos = self.processes.iter().position(|p| {
            p.ppid == parent_pid
                && (target_pid <= 0 || p.pid == target_pid as u32)
                && p.state == ProcessState::Stopped
        });

        if let Some(pos) = stopped_pos {
            let child_pid = self.processes[pos].pid;
            // Report stopped status: 0x7F for stopped (signal in upper byte)
            return Ok(Some((child_pid, 0x137F))); // SIGTSTP (19) << 8 | 0x7F
        }

        if wnohang {
            return Ok(None); // No child ready, don't block
        }

        // Otherwise would block (in a real implementation)
        Ok(None)
    }

    /// Fork a process (create a copy)
    /// Creates a new child process that is a copy of the parent.
    /// In a full POSIX implementation, this would also copy:
    ///   - File descriptor table (with shared file descriptions)
    ///   - Signal dispositions and mask
    ///   - Address space (COW)
    ///   - Process group and session membership
    pub fn fork(&mut self, parent_pid: Pid) -> Option<Pid> {
        let parent = self.get_process(parent_pid)?.clone();
        let child_pid = self.next_pid;
        self.next_pid += 1;

        self.processes.push(Process {
            pid: child_pid,
            ppid: parent_pid,
            name: parent.name.clone(),
            state: ProcessState::Ready,
            uid: parent.uid,
            gid: parent.gid,
            cwd: parent.cwd.clone(),
            priority: parent.priority,
            has_address_space: parent.has_address_space,
            entry_point: parent.entry_point,
            user_stack_top: parent.user_stack_top,
            exit_code: 0,
        });

        // Clone FD table from parent
        crate::fd::fork_fd_table(parent_pid, child_pid);
        // Clone signal dispositions from parent
        crate::signals::fork_process_signals(parent_pid, child_pid);
        // Inherit process group from parent
        if let Ok(parent_pgid) = crate::pgrp::getpgid(parent_pid) {
            let _ = crate::pgrp::setpgid(child_pid, parent_pgid);
        }

        crate::serial_println!("[KnoxOS] fork: PID {} -> PID {}", parent_pid, child_pid);
        Some(child_pid)
    }

    /// Execute a new program in a process (exec)
    /// Replaces the process image with a new program.
    /// In POSIX, this also:
    ///   - Closes O_CLOEXEC file descriptors
    ///   - Resets signal dispositions to SIG_DFL (except ignored)
    ///   - Resets the address space
    pub fn exec(&mut self, pid: Pid, name: &str, _args: &[&str]) -> bool {
        if let Some(proc) = self.get_process_mut(pid) {
            proc.name = String::from(name);
            proc.state = ProcessState::Running;

            // Close O_CLOEXEC file descriptors
            crate::fd::close_cloexec_fds(pid);
            // Reset signal dispositions (SIG_DFL, except SIG_IGN stays)
            crate::signals::exec_reset_signals(pid);

            crate::serial_println!("[KnoxOS] exec: PID {} now running '{}'", pid, name);
            true
        } else {
            false
        }
    }

    /// Change process working directory
    pub fn chdir(&mut self, pid: Pid, path: &str) -> bool {
        if let Some(proc) = self.get_process_mut(pid) {
            proc.cwd = String::from(path);
            true
        } else {
            false
        }
    }

    /// Set process state
    pub fn set_state(&mut self, pid: Pid, state: ProcessState) {
        if let Some(proc) = self.get_process_mut(pid) {
            proc.state = state;
        }
    }

    /// Get children of a process
    pub fn children(&self, ppid: Pid) -> Vec<Pid> {
        self.processes
            .iter()
            .filter(|p| p.ppid == ppid && p.state != ProcessState::Zombie)
            .map(|p| p.pid)
            .collect()
    }

    /// Reap all zombie children of a process
    pub fn reap_zombies(&mut self, ppid: Pid) -> Vec<Pid> {
        let zombies: Vec<Pid> = self
            .processes
            .iter()
            .filter(|p| p.ppid == ppid && p.state == ProcessState::Zombie)
            .map(|p| p.pid)
            .collect();
        self.processes
            .retain(|p| !(p.ppid == ppid && p.state == ProcessState::Zombie));
        zombies
    }
}

/// Execute an ELF binary: create address space, load segments, set up stack, create context
///
/// This is the full pipeline for launching a user-mode process from an ELF binary.
/// Returns the child PID on success.
pub fn exec_elf(elf_data: &[u8], name: &str, argv: &[&str], envp: &[&str]) -> Option<Pid> {
    // 1. Allocate PID and create process entry
    let pid = {
        let mut table = PROCESS_TABLE.lock();
        let pid = table.next_pid;
        table.next_pid += 1;
        table.processes.push(Process {
            pid,
            ppid: crate::scheduler::current_pid().unwrap_or(1),
            name: String::from(name),
            state: ProcessState::Ready,
            uid: 1000,
            gid: 1000,
            cwd: String::from("/"),
            priority: 0,
            has_address_space: true,
            entry_point: 0,
            user_stack_top: 0,
            exit_code: 0,
        });
        pid
    };

    crate::serial_println!("[exec] PID {} creating address space for '{}'", pid, name);

    // 2. Create per-process address space
    if !crate::vmm::create_address_space(pid) {
        crate::serial_println!("[exec] Failed to create address space for PID {}", pid);
        destroy_process(pid);
        return None;
    }

    // 3. Load ELF segments into the address space
    let (entry_point, _brk) = match crate::vmm::load_elf_into_address_space(pid, elf_data) {
        Ok(result) => result,
        Err(e) => {
            crate::serial_println!("[exec] Failed to load ELF for PID {}: {}", pid, e);
            crate::vmm::destroy_address_space(pid);
            destroy_process(pid);
            return None;
        }
    };

    // 4. Map user stack
    let stack_size = crate::vmm::STACK_SIZE;
    let stack_top = crate::vmm::STACK_TOP;
    if crate::vmm::setup_user_stack(pid, stack_top, stack_size).is_none() {
        crate::serial_println!("[exec] Failed to map stack for PID {}", pid);
        crate::vmm::destroy_address_space(pid);
        destroy_process(pid);
        return None;
    }

    // 5. Set up initial stack contents (argc, argv, envp, auxv)
    let initial_rsp =
        crate::vmm::setup_initial_stack(pid, stack_top, argv, envp, entry_point, 0, 0)
            .unwrap_or(stack_top - 8);

    // 6. Update process entry with entry point and stack
    {
        let mut table = PROCESS_TABLE.lock();
        if let Some(proc) = table.get_process_mut(pid) {
            proc.entry_point = entry_point;
            proc.user_stack_top = initial_rsp;
        }
    }

    // 7. Get CR3 for this process
    let cr3 = crate::vmm::get_cr3(pid).unwrap_or(0);

    // 8. Create user-mode context with CR3
    crate::context::create_user_process_context(pid, entry_point, initial_rsp, cr3);

    // 9. Set up fd table, signals, and scheduler entry
    crate::fd::create_fd_table(pid);
    crate::signals::create_process_signals(pid);
    crate::scheduler::SCHEDULER.lock().add_process(pid, 0);

    crate::serial_println!(
        "[exec] PID {} ready: entry={:#x} stack={:#x} cr3={:#x}",
        pid,
        entry_point,
        initial_rsp,
        cr3
    );

    Some(pid)
}

/// Clean up all resources for a process
pub fn destroy_process(pid: Pid) {
    // Remove from process table
    let mut table = PROCESS_TABLE.lock();
    table.processes.retain(|p| p.pid != pid);
    drop(table);

    // Clean up VMM address space
    crate::vmm::destroy_address_space(pid);
    // Clean up context
    crate::context::destroy_process_context(pid);
    // Clean up fd table
    crate::fd::destroy_fd_table(pid);
    // Clean up signals
    crate::signals::destroy_process_signals(pid);
    // Remove from scheduler
    crate::scheduler::SCHEDULER.lock().remove_process(pid);
}

/// Kill a process by PID (free function wrapper)
pub fn kill(pid: Pid) -> bool {
    PROCESS_TABLE.lock().kill(pid)
}

/// Exit the current process with a status code (called by sys_exit / exit_group)
pub fn sys_exit(exit_code: i32) {
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    if pid <= 1 {
        return; // Don't exit kernel or init
    }
    crate::serial_println!("[KnoxOS] sys_exit: PID {} code {}", pid, exit_code);

    // 1. Mark process as zombie, reparent children, send SIGCHLD
    PROCESS_TABLE.lock().exit_process(pid, exit_code);

    // 2. Clean up process resources (but leave zombie in table for waitpid)
    crate::vmm::destroy_address_space(pid);
    crate::context::destroy_process_context(pid);
    crate::fd::destroy_fd_table(pid);
    // Note: signal state kept until parent reaps via waitpid

    // 3. Remove from scheduler (no longer runnable)
    crate::scheduler::remove_process(pid);
}

/// Reap all zombie children of init (PID 1) — orphan cleanup
/// Should be called periodically to prevent zombie accumulation
pub fn reap_init_zombies() {
    let mut table = PROCESS_TABLE.lock();
    let reaped = table.reap_zombies(1);
    for pid in &reaped {
        // Clean up remaining signal state
        crate::signals::destroy_process_signals(*pid);
    }
    if !reaped.is_empty() {
        crate::serial_println!("[KnoxOS] Reaped {} orphan zombies", reaped.len());
    }
}

/// Initialize process management
pub fn init() {
    let pt = PROCESS_TABLE.lock();
    crate::serial_println!(
        "[KnoxOS] Process table initialized: {} processes",
        pt.count()
    );
}

/// Wait for a child process (public API for waitid/wait4 syscalls)
/// Returns (child_pid, exit_code) if a matching zombie child is found
pub fn wait_child(parent_pid: u32, target_pid: i32) -> Option<(u32, i32)> {
    let mut table = PROCESS_TABLE.lock();
    table.waitpid(if target_pid < 0 { 0 } else { target_pid as u32 })
}
