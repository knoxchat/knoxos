//! Scheduling and processes. Kernel mode.
//!
//! Crate-root names stay (`crate::scheduler`, `crate::process`, …) via `pub use`
//! in `lib.rs`.

pub mod capabilities;
pub mod cgroup2;
#[cfg(feature = "stub-drivers")]
pub mod cgroup_psi;
#[cfg(feature = "stub-drivers")]
pub mod cgroup_v2_controllers;
pub mod cgroups;
#[cfg(feature = "stub-drivers")]
pub mod cgroups_v2_advanced;
pub mod cred;
pub mod elf;
pub mod namespaces;
pub mod pgrp;
pub mod pidfd;
pub mod pidns;
pub mod posix_timer;
pub mod process;
pub mod rlimit;
pub mod rseq;
#[cfg(feature = "stub-drivers")]
pub mod sched_debug;
pub mod sched_ext;
pub mod scheduler;
pub mod session;
pub mod signals;
pub mod task;
pub mod threads;
pub mod timerfd;
pub mod user_task;
pub mod users;
pub mod wait_queue;
pub mod workqueue;
