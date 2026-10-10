use super::{SyscallError, SyscallNumber, read_user_string};
use super::{advanced, ai, fd, fs, io, memory, net, process, signal, system, thread, time, user};
use crate::serial_println;

/// Main syscall dispatcher — handles ALL ~452 Linux x86_64 syscalls + KnoxOS extensions
pub fn handle_syscall(
    number: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> i64 {
    // ── Seccomp filter check ────────────────────────────────────────
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let args = [arg1, arg2, arg3, arg4, arg5, arg6];
    match crate::seccomp::check_syscall(pid, number, args) {
        Ok(()) => {}
        Err(errno) => {
            serial_println!(
                "[KnoxOS] seccomp: blocked pid {} syscall {} (errno={})",
                pid,
                number,
                errno
            );
            let uid = crate::users::get_current_uid();
            crate::audit::log_syscall(pid, uid, number, args, errno as i64, false);
            return errno as i64;
        }
    }
    // ── Audit syscall logging ───────────────────────────────────────
    let uid = crate::users::get_current_uid();
    crate::audit::log_syscall(pid, uid, number, args, 0, true);

    let syscall = SyscallNumber::from(number);
    let result = match syscall {
        // ════════════════════════════════════════════════════════════
        // ── Core File I/O (0–20) ───────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Read => fs::sys_read(arg1, arg2, arg3),
        SyscallNumber::Write => fs::sys_write(arg1, arg2, arg3),
        SyscallNumber::Open | SyscallNumber::Creat => fs::sys_open(arg1, arg2 as u32, arg3 as u16),
        SyscallNumber::Close => fs::sys_close(arg1 as i32),
        SyscallNumber::Stat | SyscallNumber::Lstat => fs::sys_stat(arg1, arg2),
        SyscallNumber::Fstat => fs::sys_fstat(arg1 as i32, arg2),
        SyscallNumber::Poll => io::sys_poll(arg1, arg2 as u32, arg3 as i32),
        SyscallNumber::Ppoll => io::sys_ppoll(arg1, arg2 as u32, arg3, arg4),
        SyscallNumber::Lseek => fs::sys_lseek(arg1 as i32, arg2 as i64, arg3 as u32),
        SyscallNumber::Mmap => {
            memory::sys_mmap(arg1, arg2, arg3 as u32, arg4 as u32, arg5 as i32, arg6)
        }
        SyscallNumber::PkeyMprotect => {
            advanced::sys_pkey_mprotect(arg1, arg2, arg3 as i32, arg4 as i32)
        }
        SyscallNumber::Mprotect => {
            let pid = crate::scheduler::current_pid().unwrap_or(1);
            let r = crate::mmap::sys_mprotect(pid, arg1, arg2, arg3);
            if r == 0 {
                Ok(0)
            } else if r == -13 {
                Err(SyscallError::PermissionDenied)
            } else if r == -22 {
                Err(SyscallError::InvalidArgument)
            } else {
                Err(SyscallError::OutOfMemory)
            }
        }
        SyscallNumber::Munmap => memory::sys_munmap(arg1, arg2),
        SyscallNumber::Brk => memory::sys_brk(arg1),
        SyscallNumber::Sigaction => signal::sys_sigaction(arg1 as u32, arg2, arg3),
        SyscallNumber::Sigprocmask => signal::sys_sigprocmask(arg1 as i32, arg2, arg3),
        SyscallNumber::RtSigreturn => advanced::sys_rt_sigreturn(),
        SyscallNumber::Ioctl => fd::sys_ioctl(arg1 as i32, arg2 as u32, arg3),
        SyscallNumber::Pread64 => advanced::sys_pread64(arg1, arg2, arg3, arg4 as i64),
        SyscallNumber::Pwrite64 => advanced::sys_pwrite64(arg1, arg2, arg3, arg4 as i64),
        SyscallNumber::Readv => io::sys_readv(arg1 as i32, arg2, arg3 as usize),
        SyscallNumber::Writev => io::sys_writev(arg1 as i32, arg2, arg3 as usize),

        // ════════════════════════════════════════════════════════════
        // ── File access & memory (21–40) ───────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Access => fs::sys_access(arg1, arg2 as u32),
        SyscallNumber::Pipe => fd::sys_pipe(arg1),
        SyscallNumber::Select => io::sys_select(arg1 as i32, arg2, arg3, arg4, arg5),
        SyscallNumber::Pselect6 => io::sys_pselect6(arg1 as i32, arg2, arg3, arg4, arg5, arg6),
        SyscallNumber::SchedYield => crate::sched_ext::sched_yield()
            .map(|_| 0u64)
            .map_err(|_| SyscallError::NotImplemented),
        SyscallNumber::Mremap => advanced::sys_mremap(arg1, arg2, arg3, arg4 as i32, arg5),
        SyscallNumber::Msync => advanced::sys_msync(arg1, arg2, arg3 as i32),
        SyscallNumber::Mincore => advanced::sys_mincore(arg1, arg2, arg3),
        SyscallNumber::Madvise | SyscallNumber::ProcessMadvise => {
            advanced::sys_madvise(arg1, arg2, arg3 as i32)
        }
        SyscallNumber::Shmget => io::sys_shmget(arg1 as u32, arg2 as usize, arg3 as i32),
        SyscallNumber::Shmat => io::sys_shmat(arg1 as u32, arg2, arg3 as i32),
        SyscallNumber::Shmctl => io::sys_shmctl(arg1 as u32, arg2 as i32, arg3),
        SyscallNumber::Dup => fd::sys_dup(arg1 as i32),
        SyscallNumber::Dup2 => fd::sys_dup2(arg1 as i32, arg2 as i32),
        SyscallNumber::Pause => {
            let pid = crate::context::current_pid();
            if pid > crate::context::DESKTOP_PID {
                crate::user_task::park_current(pid);
                Ok(0)
            } else {
                Err(SyscallError::Interrupted)
            }
        }
        SyscallNumber::Nanosleep => time::sys_nanosleep(arg1),
        SyscallNumber::ClockNanosleep => {
            time::sys_clock_nanosleep(arg1 as i32, arg2 as i32, arg3, arg4)
        }
        SyscallNumber::Getitimer => advanced::sys_getitimer_linux(arg1 as i32, arg2),
        SyscallNumber::Alarm => advanced::sys_alarm_linux(arg1 as u32),
        SyscallNumber::Setitimer => advanced::sys_setitimer_linux(arg1 as i32, arg2, arg3),
        SyscallNumber::Getpid => process::sys_getpid(),
        SyscallNumber::Sendfile => io::sys_sendfile(arg1 as i32, arg2 as i32, arg3, arg4 as usize),

        // ════════════════════════════════════════════════════════════
        // ── Sockets (41–55) ────────────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Socket => net::sys_socket(arg1 as i32, arg2 as i32, arg3 as i32),
        SyscallNumber::Connect => net::sys_connect(arg1 as i32, arg2, arg3 as u32),
        SyscallNumber::Accept => net::sys_accept(arg1 as i32, arg2, arg3),
        SyscallNumber::Sendto => net::sys_sendto(
            arg1 as i32,
            arg2,
            arg3 as usize,
            arg4 as i32,
            arg5,
            arg6 as u32,
        ),
        SyscallNumber::Recvfrom => {
            net::sys_recvfrom(arg1 as i32, arg2, arg3 as usize, arg4 as i32, arg5, arg6)
        }
        SyscallNumber::Sendmsg => advanced::sys_sendmsg(arg1 as i32, arg2, arg3 as i32),
        SyscallNumber::Sendmmsg => {
            advanced::sys_sendmmsg(arg1 as i32, arg2, arg3 as u32, arg4 as i32)
        }
        SyscallNumber::Recvmsg => advanced::sys_recvmsg(arg1 as i32, arg2, arg3 as i32),
        SyscallNumber::Recvmmsg => {
            advanced::sys_recvmmsg(arg1 as i32, arg2, arg3 as u32, arg4 as i32, arg5)
        }
        SyscallNumber::Shutdown => net::sys_shutdown(arg1 as i32, arg2 as i32),
        SyscallNumber::Bind => net::sys_bind(arg1 as i32, arg2, arg3 as u32),
        SyscallNumber::Listen => net::sys_listen(arg1 as i32, arg2 as i32),
        SyscallNumber::Getsockname => advanced::sys_getsockname(arg1 as i32, arg2, arg3),
        SyscallNumber::Getpeername => advanced::sys_getpeername(arg1 as i32, arg2, arg3),
        SyscallNumber::Socketpair => {
            net::sys_socketpair(arg1 as i32, arg2 as i32, arg3 as i32, arg4)
        }
        SyscallNumber::Setsockopt => {
            advanced::sys_setsockopt(arg1 as i32, arg2 as i32, arg3 as i32, arg4, arg5 as u32)
        }
        SyscallNumber::Getsockopt => {
            advanced::sys_getsockopt(arg1 as i32, arg2 as i32, arg3 as i32, arg4, arg5)
        }

        // ════════════════════════════════════════════════════════════
        // ── Process management (56–71) ─────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Clone => {
            // clone(flags, stack, ptid, ctid, tls)
            process::sys_clone(arg1, arg2, arg3, arg4, arg5)
        }
        SyscallNumber::Fork | SyscallNumber::Vfork => process::sys_fork(),
        SyscallNumber::Execve => process::sys_execve(arg1, arg2, arg3),
        SyscallNumber::Execveat => {
            process::sys_execveat(arg1 as i32, arg2, arg3, arg4, arg5 as i32)
        }
        SyscallNumber::Exit | SyscallNumber::ExitGroup => process::sys_exit(arg1 as i32),
        SyscallNumber::Wait4 => process::sys_wait4(arg1 as i32, arg2, arg3 as i32),
        SyscallNumber::Kill => process::sys_kill(arg1 as u32, arg2 as u32),
        SyscallNumber::Uname => system::sys_uname(arg1),
        SyscallNumber::Semget => advanced::sys_semget(arg1 as u32, arg2 as i32, arg3 as i32),
        SyscallNumber::Semop => advanced::sys_semop(arg1 as i32, arg2, arg3 as usize),
        SyscallNumber::Semctl => advanced::sys_semctl(arg1 as i32, arg2 as i32, arg3 as i32, arg4),
        SyscallNumber::Shmdt => io::sys_shmdt(arg1),
        SyscallNumber::Msgget => advanced::sys_msgget(arg1 as u32, arg2 as i32),
        SyscallNumber::Msgsnd => {
            advanced::sys_msgsnd(arg1 as i32, arg2, arg3 as usize, arg4 as i32)
        }
        SyscallNumber::Msgrcv => {
            advanced::sys_msgrcv(arg1 as i32, arg2, arg3 as usize, arg4 as i64, arg5 as i32)
        }
        SyscallNumber::Msgctl => advanced::sys_msgctl(arg1 as i32, arg2 as i32, arg3),

        // ════════════════════════════════════════════════════════════
        // ── FD & FS ops (72–99) ────────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Fcntl => fd::sys_fcntl(arg1 as i32, arg2 as i32, arg3),
        SyscallNumber::Flock => io::sys_flock(arg1 as i32, arg2 as i32),
        SyscallNumber::Fsync => advanced::sys_fsync(arg1 as i32),
        SyscallNumber::Fdatasync => advanced::sys_fdatasync(arg1 as i32),
        SyscallNumber::Truncate => fs::sys_truncate(arg1, arg2 as usize),
        SyscallNumber::Ftruncate => fs::sys_ftruncate(arg1 as i32, arg2 as usize),
        SyscallNumber::Getdents => advanced::sys_getdents(arg1 as i32, arg2, arg3 as u32),
        SyscallNumber::Getcwd => fs::sys_getcwd(arg1, arg2 as usize),
        SyscallNumber::Chdir => fs::sys_chdir(arg1),
        SyscallNumber::Fchdir => {
            // fchdir(fd) — change directory using file descriptor
            let pid = crate::scheduler::current_pid().unwrap_or(1);
            let tables = crate::fd::PROCESS_FD_TABLES.lock();
            if let Some(fd_table) = tables.get(&pid) {
                if let Some(file) = fd_table.get(arg1 as i32) {
                    if file.file_type != crate::fd::FileType::Directory {
                        Err(SyscallError::NotDirectory)
                    } else {
                        let path = file.path.clone();
                        drop(tables);
                        crate::process::PROCESS_TABLE.lock().chdir(pid, &path);
                        Ok(0)
                    }
                } else {
                    Err(SyscallError::BadFileDescriptor)
                }
            } else {
                Err(SyscallError::BadFileDescriptor)
            }
        }
        SyscallNumber::Rename => fs::sys_rename(arg1, arg2),
        SyscallNumber::Mkdir => fs::sys_mkdir(arg1, arg2 as u16),
        SyscallNumber::Rmdir => fs::sys_rmdir(arg1),
        SyscallNumber::Link => fs::sys_link(arg1, arg2),
        SyscallNumber::Unlink => fs::sys_unlink(arg1),
        SyscallNumber::Symlink => fs::sys_symlink(arg1, arg2),
        SyscallNumber::Readlink => fs::sys_readlink(arg1, arg2, arg3),
        SyscallNumber::Chmod => fs::sys_chmod(arg1, arg2 as u16),
        SyscallNumber::Fchmod => fs::sys_fchmod(arg1 as i32, arg2 as u16),
        SyscallNumber::Chown | SyscallNumber::Lchown => {
            fs::sys_chown(arg1, arg2 as u32, arg3 as u32)
        }
        SyscallNumber::Fchown => fs::sys_fchown(arg1 as i32, arg2 as u32, arg3 as u32),
        SyscallNumber::Umask => fs::sys_umask(arg1 as u16),
        SyscallNumber::Gettimeofday => time::sys_gettimeofday(arg1),
        SyscallNumber::Getrlimit => system::sys_getrlimit(arg1 as i32, arg2),
        SyscallNumber::Setrlimit => system::sys_setrlimit(arg1 as i32, arg2),
        SyscallNumber::Getrusage => fs::sys_getrusage(arg1 as i32, arg2),
        SyscallNumber::Sysinfo => system::sys_sysinfo(arg1),

        // ════════════════════════════════════════════════════════════
        // ── Time, IDs, signals (100–130) ───────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Times => {
            if arg1 != 0 {
                unsafe {
                    core::ptr::write_bytes(arg1 as *mut u8, 0, 32);
                }
            }
            Ok(crate::interrupts::get_ticks())
        }
        SyscallNumber::Ptrace => advanced::sys_ptrace(arg1, arg2, arg3, arg4),
        SyscallNumber::Getuid | SyscallNumber::Geteuid => user::sys_getuid(),
        SyscallNumber::Syslog => system::sys_syslog(arg1 as i32, arg2, arg3 as usize),
        SyscallNumber::Getgid | SyscallNumber::Getegid => user::sys_getgid(),
        SyscallNumber::Setuid => user::sys_setuid(arg1 as u32),
        SyscallNumber::Setgid => user::sys_setgid(arg1 as u32),
        SyscallNumber::Setpgid => process::sys_setpgid(arg1 as u32, arg2 as u32),
        SyscallNumber::Getppid => process::sys_getppid(),
        SyscallNumber::Getpgrp => process::sys_getpgrp(),
        SyscallNumber::Setsid => process::sys_setsid(),
        SyscallNumber::Setreuid => user::sys_setreuid(arg1 as u32, arg2 as u32),
        SyscallNumber::Setregid => user::sys_setregid(arg1 as u32, arg2 as u32),
        SyscallNumber::Getgroups => user::sys_getgroups(arg1 as i32, arg2),
        SyscallNumber::Setgroups => user::sys_setgroups(arg1 as usize, arg2),
        SyscallNumber::Setresuid => advanced::sys_setresuid(arg1 as u32, arg2 as u32, arg3 as u32),
        SyscallNumber::Getresuid => advanced::sys_getresuid(arg1, arg2, arg3),
        SyscallNumber::Setresgid => advanced::sys_setresgid(arg1 as u32, arg2 as u32, arg3 as u32),
        SyscallNumber::Getresgid => advanced::sys_getresgid(arg1, arg2, arg3),
        SyscallNumber::Getpgid => process::sys_getpgid(arg1 as u32),
        SyscallNumber::Setfsuid => {
            // setfsuid returns the previous fsuid (= uid)
            let pid = crate::scheduler::current_pid().unwrap_or(0);
            let uid = crate::process::PROCESS_TABLE
                .lock()
                .get_process(pid)
                .map(|p| p.uid)
                .unwrap_or(0);
            Ok(uid as u64)
        }
        SyscallNumber::Setfsgid => {
            let pid = crate::scheduler::current_pid().unwrap_or(0);
            let gid = crate::process::PROCESS_TABLE
                .lock()
                .get_process(pid)
                .map(|p| p.gid)
                .unwrap_or(0);
            Ok(gid as u64)
        }
        SyscallNumber::Getsid => process::sys_getsid(arg1 as u32),
        SyscallNumber::Capget => advanced::sys_capget(arg1, arg2),
        SyscallNumber::Capset => advanced::sys_capset(arg1, arg2),
        SyscallNumber::RtSigpending => advanced::sys_rt_sigpending(arg1, arg2 as usize),
        SyscallNumber::RtSigtimedwait => {
            advanced::sys_rt_sigtimedwait(arg1, arg2, arg3, arg4 as usize)
        }
        SyscallNumber::RtSigqueueinfo | SyscallNumber::RtTgsigqueueinfo => {
            advanced::sys_rt_sigqueueinfo(arg1 as u32, arg2 as u32, arg3)
        }
        SyscallNumber::RtSigsuspend => advanced::sys_rt_sigsuspend(arg1, arg2 as usize),

        // ════════════════════════════════════════════════════════════
        // ── FS & scheduling (131–155) ──────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Sigaltstack => signal::sys_sigaltstack(arg1, arg2),
        SyscallNumber::Utime | SyscallNumber::Utimes => {
            // Validate path exists (timestamps not tracked by in-memory VFS)
            if arg1 != 0 {
                if let Some(path) = unsafe { read_user_string(arg1) } {
                    let vfs = crate::vfs::VFS.lock();
                    if vfs.resolve_path(&path).is_none() {
                        Err(SyscallError::FileNotFound)
                    } else {
                        Ok(0)
                    }
                } else {
                    Ok(0)
                }
            } else {
                Ok(0)
            }
        }
        SyscallNumber::Futimesat => advanced::sys_futimesat(arg1 as i32, arg2, arg3),
        SyscallNumber::Mknodat => advanced::sys_mknodat(arg1 as i32, arg2, arg3 as u32, arg4),
        SyscallNumber::Mknod => {
            // mknod(path, mode, dev) — create special file
            let path = unsafe { read_user_string(arg1) };
            if let Some(p) = path {
                let pid = crate::scheduler::current_pid().unwrap_or(1);
                let p = crate::process::translate_path(pid, &p);
                let mode = arg2 as u32;
                let file_type = mode & 0o170000;
                match file_type {
                    0o010000 => {
                        // S_IFIFO — named pipe with a live FIFO buffer
                        let perms = (mode as u16) & 0o7777 & !crate::syscall::fs::current_umask();
                        crate::fifo::mkfifo(&p, perms)
                            .map(|_| 0u64)
                            .map_err(|e| match e {
                                -17 => SyscallError::FileExists,
                                -2 => SyscallError::FileNotFound,
                                _ => SyscallError::IoError,
                            })
                    }
                    0o100000 | 0 => {
                        // S_IFREG or default — create regular file
                        let mut vfs = crate::vfs::VFS.lock();
                        vfs.write_file(&p, &[]);
                        Ok(0)
                    }
                    0o020000 | 0o060000 => {
                        // S_IFCHR / S_IFBLK — create device node
                        let mut vfs = crate::vfs::VFS.lock();
                        vfs.write_file(&p, &[]);
                        if let Some(ino) = vfs.resolve_path(&p) {
                            if let Some(inode) = vfs.get_inode_mut(ino) {
                                inode.file_type = if file_type == 0o020000 {
                                    crate::vfs::FileType::CharDevice
                                } else {
                                    crate::vfs::FileType::BlockDevice
                                };
                                inode.permissions = (mode & 0o7777) as u16;
                            }
                        }
                        Ok(0)
                    }
                    0o140000 => {
                        // S_IFSOCK — create socket node
                        let mut vfs = crate::vfs::VFS.lock();
                        vfs.write_file(&p, &[]);
                        if let Some(ino) = vfs.resolve_path(&p) {
                            if let Some(inode) = vfs.get_inode_mut(ino) {
                                inode.file_type = crate::vfs::FileType::Socket;
                            }
                        }
                        Ok(0)
                    }
                    _ => Err(SyscallError::InvalidArgument),
                }
            } else {
                Err(SyscallError::InvalidArgument)
            }
        }
        SyscallNumber::Uselib => {
            serial_println!("[KnoxOS] uselib denied (ENOSYS)");
            Err(SyscallError::NotImplemented)
        }
        SyscallNumber::Personality => advanced::sys_personality(arg1),
        SyscallNumber::Ustat => {
            serial_println!("[KnoxOS] ustat denied (ENOSYS)");
            Err(SyscallError::NotImplemented)
        }
        SyscallNumber::Statfs => fs::sys_statfs(arg1, arg2),
        SyscallNumber::Fstatfs => fs::sys_fstatfs(arg1 as i32, arg2),
        SyscallNumber::Sysfs => advanced::sys_sysfs(arg1 as i32, arg2, arg3),
        SyscallNumber::Getpriority => io::sys_getpriority(arg1 as i32, arg2 as u32),
        SyscallNumber::Setpriority => io::sys_setpriority(arg1 as i32, arg2 as u32, arg3 as i32),
        SyscallNumber::SchedSetparam => {
            // sched_setparam(pid, param) — set scheduling parameters
            if arg2 != 0 {
                let _priority = unsafe { *(arg2 as *const i32) };
            }
            Ok(0)
        }
        SyscallNumber::SchedGetparam => {
            if arg2 != 0 {
                let pid = crate::scheduler::current_pid().unwrap_or(1);
                let buf = [0u8; 4];
                unsafe {
                    core::ptr::copy_nonoverlapping(buf.as_ptr(), arg2 as *mut u8, 4);
                }
                crate::vmm::write_user_memory(pid, arg2, &buf);
            }
            Ok(0)
        }
        SyscallNumber::SchedSetscheduler => {
            io::sys_sched_setscheduler(arg1 as u32, arg2 as i32, arg3)
        }
        SyscallNumber::SchedGetscheduler => io::sys_sched_getscheduler(arg1 as u32),
        SyscallNumber::SchedGetPriorityMax => Ok(99),
        SyscallNumber::SchedGetPriorityMin => Ok(0),
        SyscallNumber::SchedRrGetInterval => advanced::sys_sched_rr_get_interval(arg1 as u32, arg2),
        SyscallNumber::Mlock => advanced::sys_mlock(arg1, arg2),
        SyscallNumber::Munlock => advanced::sys_munlock(arg1, arg2),
        SyscallNumber::Mlockall => advanced::sys_mlockall(arg1 as i32),
        SyscallNumber::Munlockall => advanced::sys_munlockall(),
        SyscallNumber::Vhangup => advanced::sys_vhangup(),
        SyscallNumber::ModifyLdt => advanced::sys_modify_ldt(arg1 as i32, arg2, arg3),
        SyscallNumber::PivotRoot => advanced::sys_pivot_root(arg1, arg2),

        // ════════════════════════════════════════════════════════════
        // ── System control (156–185) ───────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Sysctl => advanced::sys_sysctl_old(arg1),
        SyscallNumber::Prctl => advanced::sys_prctl(arg1 as i32, arg2, arg3, arg4, arg5),
        SyscallNumber::ArchPrctl => advanced::sys_arch_prctl(arg1 as i32, arg2),
        SyscallNumber::Adjtimex | SyscallNumber::ClockAdjtime => advanced::sys_adjtimex(arg1),
        SyscallNumber::Chroot => {
            if let Some(path) = unsafe { read_user_string(arg1) } {
                let pid = crate::scheduler::current_pid().unwrap_or(1);
                match crate::process::chroot(pid, &path) {
                    Ok(()) => {
                        serial_println!("[KnoxOS] chroot({}) for PID {}", path, pid);
                        Ok(0)
                    }
                    Err(-2) => Err(SyscallError::FileNotFound),
                    Err(-20) => Err(SyscallError::NotDirectory),
                    _ => Err(SyscallError::InvalidArgument),
                }
            } else {
                Err(SyscallError::InvalidArgument)
            }
        }
        SyscallNumber::Sync => advanced::sys_sync(),
        SyscallNumber::Acct => advanced::sys_acct(arg1),
        SyscallNumber::Settimeofday => advanced::sys_settimeofday(arg1, arg2),
        SyscallNumber::MountLinux => advanced::sys_mount_linux(arg1, arg2, arg3, arg4, arg5),
        SyscallNumber::Umount2 => advanced::sys_umount2(arg1, arg2 as i32),
        SyscallNumber::Swapon => advanced::sys_swapon(arg1, arg2 as i32),
        SyscallNumber::Swapoff => advanced::sys_swapoff(arg1),
        SyscallNumber::Reboot => system::sys_reboot(arg1 as u32, arg2 as u32),
        SyscallNumber::Sethostname => system::sys_sethostname(arg1, arg2 as usize),
        SyscallNumber::Setdomainname => system::sys_sethostname(arg1, arg2 as usize),
        SyscallNumber::Iopl => advanced::sys_iopl(arg1 as i32),
        SyscallNumber::Ioperm => advanced::sys_ioperm(arg1, arg2, arg3 as i32),
        SyscallNumber::CreateModule | SyscallNumber::GetKernelSyms | SyscallNumber::QueryModule => {
            Err(SyscallError::NotImplemented)
        }
        SyscallNumber::InitModule | SyscallNumber::FinitModule | SyscallNumber::DeleteModule => {
            serial_println!("[KnoxOS] init_module denied (ENOSYS)");
            Err(SyscallError::NotImplemented)
        }
        SyscallNumber::Quotactl | SyscallNumber::QuotactlFd => {
            serial_println!("[KnoxOS] quotactl denied (ENOSYS)");
            Err(SyscallError::NotImplemented)
        }
        SyscallNumber::Nfsservctl
        | SyscallNumber::Getpmsg
        | SyscallNumber::Putpmsg
        | SyscallNumber::AfsSyscall
        | SyscallNumber::Tuxcall
        | SyscallNumber::Security
        | SyscallNumber::Vserver => Err(SyscallError::NotImplemented),

        // ════════════════════════════════════════════════════════════
        // ── Thread ID, xattr, signals (186–211) ────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Gettid => advanced::sys_gettid(),
        SyscallNumber::Readahead => {
            // readahead(fd, offset, count) — advisory prefetch
            // In-memory FS: data is already "cached"
            Ok(0)
        }
        SyscallNumber::Setxattr | SyscallNumber::Lsetxattr => {
            advanced::sys_setxattr(arg1, arg2, arg3, arg4 as usize, arg5 as i32)
        }
        SyscallNumber::Fsetxattr => {
            advanced::sys_fsetxattr(arg1 as i32, arg2, arg3, arg4 as usize, arg5 as i32)
        }
        SyscallNumber::Getxattr | SyscallNumber::Lgetxattr => {
            advanced::sys_getxattr(arg1, arg2, arg3, arg4 as usize)
        }
        SyscallNumber::Fgetxattr => advanced::sys_fgetxattr(arg1 as i32, arg2, arg3, arg4 as usize),
        SyscallNumber::Listxattr | SyscallNumber::Llistxattr => {
            advanced::sys_listxattr(arg1, arg2, arg3 as usize)
        }
        SyscallNumber::Flistxattr => advanced::sys_flistxattr(arg1 as i32, arg2, arg3 as usize),
        SyscallNumber::Removexattr | SyscallNumber::Lremovexattr => {
            advanced::sys_removexattr(arg1, arg2)
        }
        SyscallNumber::Fremovexattr => advanced::sys_fremovexattr(arg1 as i32, arg2),
        SyscallNumber::Tkill => advanced::sys_tkill(arg1 as u32, arg2 as u32),
        SyscallNumber::Time => {
            let t = crate::rtc::unix_time();
            if arg1 != 0 {
                unsafe {
                    *(arg1 as *mut i64) = t;
                }
            }
            Ok(t as u64)
        }
        SyscallNumber::Futex => thread::sys_futex(arg1, arg2 as i32, arg3 as u32),
        SyscallNumber::SchedSetaffinity | SyscallNumber::SchedGetaffinity => Ok(0),
        SyscallNumber::SchedSetattr | SyscallNumber::SchedGetattr => {
            serial_println!("[KnoxOS] sched_setattr denied (ENOSYS)");
            Err(SyscallError::NotImplemented)
        }
        SyscallNumber::SetThreadArea | SyscallNumber::GetThreadArea => {
            serial_println!("[KnoxOS] set_thread_area denied (ENOSYS)");
            Err(SyscallError::NotImplemented)
        }
        SyscallNumber::IoSetup => advanced::sys_io_setup(arg1 as u32, arg2),
        SyscallNumber::IoDestroy => advanced::sys_io_destroy(arg1),
        SyscallNumber::IoGetevents => {
            advanced::sys_io_getevents(arg1, arg2 as i64, arg3 as i64, arg4, arg5)
        }
        SyscallNumber::IoPgetevents => {
            advanced::sys_io_pgetevents(arg1, arg2 as i64, arg3 as i64, arg4, arg5, arg6)
        }
        SyscallNumber::IoSubmit => advanced::sys_io_submit(arg1, arg2 as i64, arg3),
        SyscallNumber::IoCancel => advanced::sys_io_cancel(arg1, arg2, arg3),
        SyscallNumber::LookupDcookie => advanced::sys_lookup_dcookie(arg1, arg2, arg3 as usize),

        // ════════════════════════════════════════════════════════════
        // ── Epoll, dentries (213–220) ──────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::EpollCreate | SyscallNumber::EpollCreate1 => {
            io::sys_epoll_create(arg1 as i32)
        }
        SyscallNumber::EpollCtl | SyscallNumber::EpollCtlOld => {
            io::sys_epoll_ctl(arg1 as i32, arg2 as i32, arg3 as i32, arg4)
        }
        SyscallNumber::EpollWait | SyscallNumber::EpollWaitOld | SyscallNumber::EpollPwait2 => {
            io::sys_epoll_wait(arg1 as i32, arg2, arg3 as i32, arg4 as i32)
        }
        SyscallNumber::EpollPwait => {
            io::sys_epoll_pwait(arg1 as i32, arg2, arg3 as i32, arg4 as i32, arg5)
        }
        SyscallNumber::RemapFilePages => {
            serial_println!("[KnoxOS] remap_file_pages denied (ENOSYS)");
            Err(SyscallError::NotImplemented)
        }
        SyscallNumber::Getdents64 => fs::sys_getdents64(arg1 as i32, arg2, arg3 as u32),
        SyscallNumber::SetTidAddress => thread::sys_set_tid_address(arg1),
        SyscallNumber::RestartSyscall => Ok(0),
        SyscallNumber::Semtimedop => {
            advanced::sys_semtimedop(arg1 as i32, arg2, arg3 as usize, arg4)
        }

        // ════════════════════════════════════════════════════════════
        // ── Timers (221–235) ───────────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Fadvise64 => Ok(0),
        SyscallNumber::TimerCreate => advanced::sys_timer_create_linux(arg1 as u32, arg2, arg3),
        SyscallNumber::TimerSettime => {
            advanced::sys_timer_settime_linux(arg1 as u32, arg2 as i32, arg3, arg4)
        }
        SyscallNumber::TimerGettime => advanced::sys_timer_gettime_linux(arg1 as u32, arg2),
        SyscallNumber::TimerGetoverrun => advanced::sys_timer_getoverrun_linux(arg1 as u32),
        SyscallNumber::TimerDelete => advanced::sys_timer_delete_linux(arg1 as u32),
        SyscallNumber::ClockSettime => advanced::sys_clock_settime(arg1 as u32, arg2),
        SyscallNumber::ClockGettime => time::sys_clock_gettime(arg1 as u32, arg2),
        SyscallNumber::ClockGetres => advanced::sys_clock_getres(arg1 as u32, arg2),
        SyscallNumber::Tgkill => advanced::sys_tgkill(arg1 as u32, arg2 as u32, arg3 as u32),

        // ════════════════════════════════════════════════════════════
        // ── NUMA, message queues (237–246) ─────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Mbind => {
            advanced::sys_mbind(arg1, arg2, arg3 as i32, arg4, arg5, arg6 as u32)
        }
        SyscallNumber::SetMempolicy | SyscallNumber::SetMempolicy2 => {
            advanced::sys_set_mempolicy(arg1 as i32, arg2, arg3)
        }
        SyscallNumber::GetMempolicy => advanced::sys_get_mempolicy(arg1, arg2, arg3, arg4, arg5),
        SyscallNumber::MqOpen => io::sys_mq_open(arg1, arg2 as i32, arg3 as u32),
        SyscallNumber::MqUnlink => {
            if let Some(name) = unsafe { read_user_string(arg1) } {
                let mut queues = crate::ipc::MESSAGE_QUEUES.lock();
                let before = queues.len();
                queues.retain(|q| q.name != name);
                if queues.len() < before {
                    Ok(0)
                } else {
                    Err(SyscallError::FileNotFound)
                }
            } else {
                Err(SyscallError::InvalidArgument)
            }
        }
        SyscallNumber::MqTimedsend => {
            advanced::sys_mq_timedsend(arg1 as i32, arg2, arg3 as usize, arg4 as u32, arg5)
        }
        SyscallNumber::MqTimedreceive => {
            advanced::sys_mq_timedreceive(arg1 as i32, arg2, arg3 as usize, arg4, arg5)
        }
        SyscallNumber::MqNotify => advanced::sys_mq_notify(arg1 as i32, arg2),
        SyscallNumber::MqGetsetattr => advanced::sys_mq_getsetattr(arg1 as i32, arg2, arg3),
        SyscallNumber::KexecLoad | SyscallNumber::KexecFileLoad => {
            serial_println!("[KnoxOS] kexec denied (ENOSYS)");
            Err(SyscallError::NotImplemented)
        }

        // ════════════════════════════════════════════════════════════
        // ── Waitid, keys, ioprio (247–260) ─────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Waitid => process::sys_waitid(arg1 as i32, arg2 as u32, arg3, arg4 as i32),
        SyscallNumber::AddKey | SyscallNumber::RequestKey | SyscallNumber::Keyctl => {
            serial_println!("[KnoxOS] keyctl denied (ENOSYS)");
            Err(SyscallError::NotImplemented)
        }
        SyscallNumber::IoprioSet => advanced::sys_ioprio_set(arg1 as u32, arg2 as u32, arg3 as u32),
        SyscallNumber::IoprioGet => advanced::sys_ioprio_get(arg1 as u32, arg2 as u32),
        SyscallNumber::InotifyInit | SyscallNumber::InotifyInit1 => {
            io::sys_inotify_init(arg1 as i32)
        }
        SyscallNumber::InotifyAddWatch => io::sys_inotify_add_watch(arg1 as i32, arg2, arg3 as u32),
        SyscallNumber::InotifyRmWatch => io::sys_inotify_rm_watch(arg1 as i32, arg2 as i32),
        SyscallNumber::MigratePages => advanced::sys_migrate_pages(arg1 as u32, arg2, arg3, arg4),

        // ════════════════════════════════════════════════════════════
        // ── *at() family (257–280) ─────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Openat | SyscallNumber::Openat2 => {
            advanced::sys_openat(arg1 as i32, arg2, arg3 as u32, arg4 as u16)
        }
        SyscallNumber::Mkdirat => advanced::sys_mkdirat(arg1 as i32, arg2, arg3 as u16),
        SyscallNumber::Fchownat => {
            advanced::sys_fchownat(arg1 as i32, arg2, arg3 as u32, arg4 as u32, arg5 as i32)
        }
        SyscallNumber::Newfstatat => advanced::sys_newfstatat(arg1 as i32, arg2, arg3, arg4 as i32),
        SyscallNumber::Unlinkat => advanced::sys_unlinkat(arg1 as i32, arg2, arg3 as i32),
        SyscallNumber::Renameat => advanced::sys_renameat(arg1 as i32, arg2, arg3 as i32, arg4),
        SyscallNumber::Linkat => {
            advanced::sys_linkat(arg1 as i32, arg2, arg3 as i32, arg4, arg5 as i32)
        }
        SyscallNumber::Symlinkat => advanced::sys_symlinkat(arg1, arg2 as i32, arg3),
        SyscallNumber::Readlinkat => advanced::sys_readlinkat(arg1 as i32, arg2, arg3, arg4),
        SyscallNumber::Fchmodat => {
            advanced::sys_fchmodat(arg1 as i32, arg2, arg3 as u32, arg4 as i32)
        }
        SyscallNumber::Faccessat => advanced::sys_faccessat(arg1 as i32, arg2, arg3 as u32, 0),
        SyscallNumber::Faccessat2 => {
            advanced::sys_faccessat2(arg1 as i32, arg2, arg3 as u32, arg4 as i32)
        }
        SyscallNumber::Unshare => system::sys_unshare(arg1 as u32),
        SyscallNumber::SetRobustList => advanced::sys_set_robust_list(arg1, arg2 as usize),
        SyscallNumber::GetRobustList => advanced::sys_get_robust_list(arg1 as i32, arg2, arg3),
        SyscallNumber::Splice => io::sys_splice(
            arg1 as i32,
            arg2,
            arg3 as i32,
            arg4,
            arg5 as usize,
            arg6 as u32,
        ),
        SyscallNumber::Tee => io::sys_tee(arg1 as i32, arg2 as i32, arg3 as usize, arg4 as u32),
        SyscallNumber::SyncFileRange => {
            advanced::sys_sync_file_range(arg1 as i32, arg2 as i64, arg3 as i64, arg4 as u32)
        }
        SyscallNumber::Vmsplice => io::sys_vmsplice(arg1 as i32, arg2, arg3 as usize, arg4 as u32),
        SyscallNumber::MovePages => {
            advanced::sys_move_pages(arg1 as u32, arg2, arg3, arg4, arg5, arg6 as i32)
        }
        SyscallNumber::Utimensat => advanced::sys_utimensat(arg1 as i32, arg2, arg3, arg4 as i32),

        // ════════════════════════════════════════════════════════════
        // ── Epoll/signalfd/timerfd/accept4 (281–296) ───────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Signalfd | SyscallNumber::Signalfd4 => {
            advanced::sys_signalfd(arg1 as i32, arg2, arg3 as i32)
        }
        SyscallNumber::TimerfdCreate => io::sys_timerfd_create(arg1 as i32, arg2 as i32),
        SyscallNumber::Eventfd | SyscallNumber::Eventfd2 => {
            io::sys_eventfd(arg1 as u32, arg2 as i32)
        }
        SyscallNumber::Fallocate => {
            advanced::sys_fallocate(arg1 as i32, arg2 as i32, arg3 as i64, arg4 as i64)
        }
        SyscallNumber::TimerfdSettime => {
            io::sys_timerfd_settime(arg1 as i32, arg2 as i32, arg3, arg4)
        }
        SyscallNumber::TimerfdGettime => io::sys_timerfd_gettime(arg1 as i32, arg2),
        SyscallNumber::Accept4 => advanced::sys_accept4(arg1 as i32, arg2, arg3, arg4 as i32),
        SyscallNumber::Dup3 => fd::sys_dup3(arg1 as i32, arg2 as i32, arg3 as i32),
        SyscallNumber::Pipe2 => fd::sys_pipe2(arg1, arg2 as i32),
        SyscallNumber::Preadv => {
            advanced::sys_preadv2(arg1 as i32, arg2, arg3 as i32, arg4 as i64, 0)
        }
        SyscallNumber::Pwritev => {
            advanced::sys_pwritev2(arg1 as i32, arg2, arg3 as i32, arg4 as i64, 0)
        }

        // ════════════════════════════════════════════════════════════
        // ── Recent syscalls (298–334) ──────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::PerfEventOpen => {
            serial_println!("[KnoxOS] perf_event_open denied (ENOSYS)");
            Err(SyscallError::NotImplemented)
        }
        SyscallNumber::FanotifyInit | SyscallNumber::FanotifyMark => {
            serial_println!("[KnoxOS] fanotify denied (ENOSYS)");
            Err(SyscallError::NotImplemented)
        }
        SyscallNumber::Prlimit64 => system::sys_prlimit64(arg1 as u32, arg2 as i32, arg3, arg4),
        SyscallNumber::NameToHandleAt => {
            advanced::sys_name_to_handle_at(arg1 as i32, arg2, arg3, arg4, arg5 as i32)
        }
        SyscallNumber::OpenByHandleAt => {
            advanced::sys_open_by_handle_at(arg1 as i32, arg2, arg3 as i32)
        }
        SyscallNumber::Syncfs => advanced::sys_syncfs(arg1 as i32),
        SyscallNumber::Setns => advanced::sys_setns(arg1 as i32, arg2 as i32),
        SyscallNumber::Getcpu => advanced::sys_getcpu(arg1, arg2, arg3),
        SyscallNumber::ProcessVmReadv => {
            // process_vm_readv(pid, local_iov, liovcnt, remote_iov, riovcnt, flags)
            let target_pid = arg1 as u32;
            // Verify target process exists
            let table = crate::process::PROCESS_TABLE.lock();
            if table.get_process(target_pid).is_none() {
                Err(SyscallError::NoSuchProcess)
            } else {
                drop(table);
                // In KnoxOS, all kernel processes share address space,
                // so we can directly copy between iov arrays
                let local_iov = arg2;
                let liovcnt = arg3 as usize;
                let remote_iov = arg4;
                let riovcnt = arg5 as usize;
                let mut total = 0usize;
                let mut li = 0;
                let mut ri = 0;
                while li < liovcnt && ri < riovcnt {
                    let l_base = unsafe { *((local_iov + (li * 16) as u64) as *const u64) };
                    let l_len =
                        unsafe { *((local_iov + (li * 16 + 8) as u64) as *const u64) } as usize;
                    let r_base = unsafe { *((remote_iov + (ri * 16) as u64) as *const u64) };
                    let r_len =
                        unsafe { *((remote_iov + (ri * 16 + 8) as u64) as *const u64) } as usize;
                    let copy_len = l_len.min(r_len);
                    if l_base != 0 && r_base != 0 && copy_len > 0 {
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                r_base as *const u8,
                                l_base as *mut u8,
                                copy_len,
                            );
                        }
                    }
                    total += copy_len;
                    li += 1;
                    ri += 1;
                }
                Ok(total as u64)
            }
        }
        SyscallNumber::ProcessVmWritev => {
            let target_pid = arg1 as u32;
            let table = crate::process::PROCESS_TABLE.lock();
            if table.get_process(target_pid).is_none() {
                Err(SyscallError::NoSuchProcess)
            } else {
                drop(table);
                let local_iov = arg2;
                let liovcnt = arg3 as usize;
                let remote_iov = arg4;
                let riovcnt = arg5 as usize;
                let mut total = 0usize;
                let mut li = 0;
                let mut ri = 0;
                while li < liovcnt && ri < riovcnt {
                    let l_base = unsafe { *((local_iov + (li * 16) as u64) as *const u64) };
                    let l_len =
                        unsafe { *((local_iov + (li * 16 + 8) as u64) as *const u64) } as usize;
                    let r_base = unsafe { *((remote_iov + (ri * 16) as u64) as *const u64) };
                    let r_len =
                        unsafe { *((remote_iov + (ri * 16 + 8) as u64) as *const u64) } as usize;
                    let copy_len = l_len.min(r_len);
                    if l_base != 0 && r_base != 0 && copy_len > 0 {
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                l_base as *const u8,
                                r_base as *mut u8,
                                copy_len,
                            );
                        }
                    }
                    total += copy_len;
                    li += 1;
                    ri += 1;
                }
                Ok(total as u64)
            }
        }
        SyscallNumber::Kcmp => {
            advanced::sys_kcmp(arg1 as u32, arg2 as u32, arg3 as u32, arg4, arg5)
        }
        SyscallNumber::Renameat2 => {
            advanced::sys_renameat2(arg1 as i32, arg2, arg3 as i32, arg4, arg5 as u32)
        }
        SyscallNumber::Seccomp => system::sys_seccomp(arg1 as u32, arg2 as u32, arg3),
        SyscallNumber::Getrandom => system::sys_getrandom(arg1, arg2 as usize, arg3 as u32),
        SyscallNumber::MemfdCreate => advanced::sys_memfd_create(arg1, arg2 as u32),
        SyscallNumber::Bpf => {
            serial_println!("[KnoxOS] bpf(cmd={}) denied (ENOSYS)", arg1);
            Err(SyscallError::NotImplemented)
        }
        SyscallNumber::Userfaultfd => {
            serial_println!("[KnoxOS] userfaultfd denied (ENOSYS)");
            Err(SyscallError::NotImplemented)
        }
        SyscallNumber::Membarrier => advanced::sys_membarrier(arg1 as u32, arg2 as u32),
        SyscallNumber::Mlock2 => advanced::sys_mlock2(arg1, arg2, arg3 as i32),
        SyscallNumber::CopyFileRange => {
            io::sys_copy_file_range(arg1 as i32, arg2, arg3 as i32, arg4, arg5 as usize)
        }
        SyscallNumber::Preadv2 => {
            advanced::sys_preadv2(arg1 as i32, arg2, arg3 as i32, arg4 as i64, arg5 as i32)
        }
        SyscallNumber::Pwritev2 => {
            advanced::sys_pwritev2(arg1 as i32, arg2, arg3 as i32, arg4 as i64, arg5 as i32)
        }
        SyscallNumber::PkeyAlloc => advanced::sys_pkey_alloc(arg1 as u32, arg2 as u32),
        SyscallNumber::PkeyFree => advanced::sys_pkey_free(arg1 as i32),
        SyscallNumber::Statx => {
            advanced::sys_statx(arg1 as i32, arg2, arg3 as i32, arg4 as u32, arg5)
        }
        SyscallNumber::Rseq => advanced::sys_rseq(arg1, arg2 as u32, arg3 as i32, arg4 as u32),

        // ════════════════════════════════════════════════════════════
        // ── Newest syscalls (424+) ─────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::PidfdSendSignal => {
            advanced::sys_pidfd_send_signal(arg1, arg2 as i32, arg3, arg4 as u32)
        }
        SyscallNumber::IoUringSetup
        | SyscallNumber::IoUringEnter
        | SyscallNumber::IoUringRegister => {
            serial_println!("[KnoxOS] io_uring denied (ENOSYS)");
            Err(SyscallError::NotImplemented)
        }
        SyscallNumber::OpenTree
        | SyscallNumber::MoveMount
        | SyscallNumber::Fsopen
        | SyscallNumber::Fsconfig
        | SyscallNumber::Fsmount
        | SyscallNumber::Fspick => {
            serial_println!("[KnoxOS] fsopen/mount_api denied (ENOSYS)");
            Err(SyscallError::NotImplemented)
        }
        SyscallNumber::PidfdOpen => advanced::sys_pidfd_open(arg1, arg2 as u32),
        SyscallNumber::Clone3 => advanced::sys_clone3(arg1, arg2),
        SyscallNumber::CloseRange => {
            advanced::sys_close_range(arg1 as u32, arg2 as u32, arg3 as u32)
        }
        SyscallNumber::PidfdGetfd => advanced::sys_pidfd_getfd(arg1, arg2 as i32, arg3 as u32),
        SyscallNumber::MountSetattr => {
            serial_println!("[KnoxOS] mount_setattr denied (ENOSYS)");
            Err(SyscallError::NotImplemented)
        }
        SyscallNumber::LandlockCreateRuleset => {
            advanced::sys_landlock_create_ruleset(arg1, arg2 as usize, arg3 as u32)
        }
        SyscallNumber::LandlockAddRule => {
            advanced::sys_landlock_add_rule(arg1 as i32, arg2 as u32, arg3, arg4 as u32)
        }
        SyscallNumber::LandlockRestrictSelf => {
            advanced::sys_landlock_restrict_self(arg1 as i32, arg2 as u32)
        }
        SyscallNumber::MemfdSecret => {
            serial_println!("[KnoxOS] memfd_secret denied (ENOSYS)");
            Err(SyscallError::NotImplemented)
        }
        SyscallNumber::ProcessMrelease => {
            serial_println!("[KnoxOS] process_mrelease denied (ENOSYS)");
            Err(SyscallError::NotImplemented)
        }
        SyscallNumber::FutexWaitv => {
            advanced::sys_futex_waitv(arg1, arg2 as u32, arg3 as u32, arg4, arg5 as u32)
        }
        SyscallNumber::MapShadowStack => {
            serial_println!("[KnoxOS] map_shadow_stack denied (ENOSYS)");
            Err(SyscallError::NotImplemented)
        }

        // ════════════════════════════════════════════════════════════
        // ── KnoxOS AI system calls ─────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::AiQuery => ai::sys_ai_query(arg1, arg2, arg3),
        SyscallNumber::AiLoadModel => ai::sys_ai_load_model(arg1, arg2),
        SyscallNumber::AiInfer => ai::sys_ai_infer(arg1, arg2, arg3),
        SyscallNumber::AiUnloadModel => {
            let success = crate::ai::unload_model(arg1);
            if success {
                Ok(0)
            } else {
                Err(SyscallError::InvalidArgument)
            }
        }

        // ════════════════════════════════════════════════════════════
        // ── KnoxOS Threading ───────────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::ThreadCreate => thread::sys_thread_create(arg1, arg2),
        SyscallNumber::ThreadExit => thread::sys_thread_exit(arg1 as i32),
        SyscallNumber::ThreadJoin => thread::sys_thread_join(arg1 as u32),
        SyscallNumber::ThreadDetach => thread::sys_thread_detach(arg1 as u32),

        // ════════════════════════════════════════════════════════════
        // ── KnoxOS System control ──────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::KpmInstall => system::sys_kpm_install(arg1),
        SyscallNumber::KpmRemove => system::sys_kpm_remove(arg1),
        SyscallNumber::KpmList => system::sys_kpm_list(arg1, arg2 as usize),
        SyscallNumber::KpmSearch => system::sys_kpm_search(arg1, arg2, arg3 as usize),
        SyscallNumber::CgroupCreate => system::sys_cgroup_create(arg1),
        SyscallNumber::CgroupAttach => system::sys_cgroup_attach(arg1, arg2 as u32),
        SyscallNumber::Dmesg => system::sys_dmesg(arg1, arg2 as usize),
        SyscallNumber::LsBlk => system::sys_lsblk(arg1, arg2 as usize),
        SyscallNumber::Mount => system::sys_mount(arg1, arg2),
        SyscallNumber::Umount => system::sys_umount(arg1),
        SyscallNumber::Gethostname => system::sys_gethostname(arg1, arg2 as usize),

        // ════════════════════════════════════════════════════════════
        // ── KnoxOS FIFO / PTY ──────────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Mkfifo => io::sys_mkfifo(arg1, arg2 as u32),
        SyscallNumber::Openpty => io::sys_openpty(arg1, arg2),

        // ════════════════════════════════════════════════════════════
        // ── KnoxOS KVM Virtualization ──────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::KvmCreateVm => match crate::kvm::create_vm("vm", arg2 as u32, arg1) {
            Ok(vm_id) => Ok(vm_id as u64),
            Err(_) => Err(SyscallError::InvalidArgument),
        },
        SyscallNumber::KvmStartVm => {
            let _ = crate::kvm::start_vm(arg1 as u32);
            Ok(0)
        }
        SyscallNumber::KvmStopVm => {
            let _ = crate::kvm::stop_vm(arg1 as u32);
            Ok(0)
        }
        SyscallNumber::KvmPauseVm => {
            let _ = crate::kvm::pause_vm(arg1 as u32);
            Ok(0)
        }
        SyscallNumber::KvmDestroyVm => {
            let _ = crate::kvm::destroy_vm(arg1 as u32);
            Ok(0)
        }
        SyscallNumber::KvmSetVcpuRegs | SyscallNumber::KvmGetVcpuRegs => {
            Err(SyscallError::NotImplemented)
        }

        // ════════════════════════════════════════════════════════════
        // ── KnoxOS ONNX ML Runtime ─────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::OnnxLoadModel => {
            let name = unsafe { read_user_string(arg1) }.unwrap_or_default();
            match crate::onnx::load_model(&name) {
                Ok(id) => Ok(id as u64),
                Err(_) => Err(SyscallError::InvalidArgument),
            }
        }
        SyscallNumber::OnnxUnloadModel => {
            let _ = crate::onnx::unload_model(arg1 as u32);
            Ok(0)
        }
        SyscallNumber::OnnxInfer | SyscallNumber::OnnxListModels => {
            Err(SyscallError::NotImplemented)
        }

        // ════════════════════════════════════════════════════════════
        // ── KnoxOS Shared Library Loader ───────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Dlopen => {
            let name = unsafe { read_user_string(arg1) }.unwrap_or_default();
            match crate::soloader::dlopen(&name, arg2 as i32) {
                Ok(handle) => Ok(handle as u64),
                Err(_) => Err(SyscallError::FileNotFound),
            }
        }
        SyscallNumber::Dlsym => {
            let symbol = unsafe { read_user_string(arg2) }.unwrap_or_default();
            match crate::soloader::dlsym(arg1 as u32, &symbol) {
                Some(addr) => Ok(addr),
                None => Err(SyscallError::FileNotFound),
            }
        }
        SyscallNumber::Dlclose => match crate::soloader::dlclose(arg1 as u32) {
            Ok(_) => Ok(0),
            Err(_) => Err(SyscallError::InvalidArgument),
        },

        // ════════════════════════════════════════════════════════════
        // ── KnoxOS RT Scheduling ───────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::RtSetParams => {
            let policy = match arg2 {
                1 => crate::preempt_rt::RtPolicy::Fifo,
                2 => crate::preempt_rt::RtPolicy::RoundRobin,
                5 => crate::preempt_rt::RtPolicy::Deadline,
                _ => crate::preempt_rt::RtPolicy::Normal,
            };
            match crate::preempt_rt::set_rt_params(arg1 as u32, policy, arg3 as u32) {
                Ok(_) => Ok(0),
                Err(_) => Err(SyscallError::InvalidArgument),
            }
        }
        SyscallNumber::RtGetInfo => Ok(0),
        SyscallNumber::RtCreatePiMutex => {
            let name = unsafe { read_user_string(arg1) }.unwrap_or_default();
            Ok(crate::preempt_rt::create_pi_mutex(&name) as u64)
        }
        SyscallNumber::RtCreateHrtimer => {
            let id = crate::preempt_rt::create_hrtimer(arg1, arg2, pid, arg3 as u32);
            Ok(id as u64)
        }
        SyscallNumber::RtIsolateCpu => {
            crate::preempt_rt::isolate_cpu(arg1 as u32);
            Ok(0)
        }
        SyscallNumber::RtLatencyStats => {
            let (_min, _max, _avg, samples) = crate::preempt_rt::latency_stats();
            Ok(samples)
        }

        // ════════════════════════════════════════════════════════════
        // ── Unknown / unimplemented ────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Unknown => {
            serial_println!("[KnoxOS] Unknown syscall: {}", number);
            Err(SyscallError::NotImplemented)
        }
    };
    match result {
        Ok(val) => val as i64,
        Err(err) => err.errno(),
    }
}
