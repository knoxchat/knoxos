//! Syscall implementations — Advanced Linux subsystems
//!
//! pidfd, memfd, userfaultfd, io_uring, rseq, xattr, close_range,
//! membarrier, io_prio, kcmp, fanotify, ptrace, perf, landlock,
//! mount_api, aio, capabilities, madvise, msync, mincore,
//! pread/pwrite, fallocate, sync, prctl, arch_prctl, etc.
//!
//! Split by subsystem. `dispatch` still calls `advanced::sys_*`.

mod async_io;
mod debug;
mod fds;
mod file;
mod ipc;
mod memory;
mod misc;
mod mount;
mod net;
mod prctl;
mod process;
mod security;
mod signal;
mod time;
mod xattr;

pub use async_io::*;
pub use debug::*;
pub use fds::*;
pub use file::*;
pub use ipc::*;
pub use memory::*;
pub use misc::*;
pub use mount::*;
pub use net::*;
pub use prctl::*;
pub use process::*;
pub use security::*;
pub use signal::*;
pub use time::*;
pub use xattr::*;
