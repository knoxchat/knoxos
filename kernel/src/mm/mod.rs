//! Memory management. Kernel mode.
//!
//! Crate-root names stay (`crate::vmm`, `crate::mmap`, …) via `pub use`
//! in `lib.rs`.

pub mod allocator;
#[cfg(feature = "stub-drivers")]
pub mod checkpoint;
pub mod huge_pages;
pub mod ksm;
pub mod memfd;
pub mod memory;
#[cfg(feature = "stub-drivers")]
pub mod memory_pool;
pub mod mmap;
pub mod numa;
pub mod oom;
pub mod page_cache;
pub mod slab;
pub mod stack_guard;
pub mod swap;
pub mod thp;
pub mod userfaultfd;
pub mod vmm;
