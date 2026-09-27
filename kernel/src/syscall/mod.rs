/// Syscall Interface - Linux-compatible system call layer
/// Provides COMPLETE coverage of all ~452 Linux x86_64 system calls
/// Wired to real VFS, process, signal, fd, and IPC subsystems
///
/// Split into submodules for maintainability:
///   numbers  — Linux x86_64 syscall number enum and ABI mapping
///   error    — SyscallError / SyscallResult
///   helpers  — user-pointer utilities
///   dispatch — main handle_syscall dispatcher
///   fs       — File I/O and filesystem operations
///   process  — Process lifecycle, groups, and sessions
///   fd       — File descriptor operations (pipe, dup, fcntl)
///   memory   — Memory management (brk, mmap, munmap)
///   net      — Network socket operations
///   signal   — Signal handling (sigaction, sigprocmask)
///   thread   — Threading and futex
///   user     — User/group identity management
///   time     — Time and clock operations
///   system   — System info, control, namespaces, security
///   io       — I/O multiplexing, advanced I/O, IPC
///   ai       — KnoxOS AI system calls
///   advanced — Remaining Linux syscalls, split by subsystem
///              (pidfd, memfd, io_uring, xattr, ptrace, prctl, etc.)
mod advanced;
mod ai;
mod dispatch;
mod error;
mod fd;
mod fs;
mod helpers;
mod io;
mod memory;
mod net;
mod numbers;
mod process;
mod signal;
mod system;
mod thread;
mod time;
mod user;

pub use advanced::fsync_self_test;
pub use dispatch::handle_syscall;
pub use error::{SyscallError, SyscallResult};
pub(crate) use helpers::read_user_string;
pub use numbers::SyscallNumber;
