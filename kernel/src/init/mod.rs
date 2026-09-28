/// Init Process — /init startup and user-space process launcher
///
/// This module implements the /init process startup flow:
///   1. Load /init (or /sbin/init) from the initramfs/filesystem
///   2. Create a user-mode address space
///   3. Map ELF segments into user pages
///   4. Set up argv/envp/auxv on the user stack
///   5. Transition to ring 3 via iretq
///
/// If no real /init ELF is available, a built-in minimal init is used
/// that sets up the system and spawns a shell.
///
/// Split into submodules for maintainability:
///   types    — INIT_PID and AuxvType
///   elf      — static ELF64 packer and the Gate B2 hello image
///   launch   — filesystem search, hello iretq, start_init
///   programs — Ring 3 gate fixtures (process, I/O, fs, identity, net, display, ENOSYS)
mod elf;
mod launch;
mod programs;
mod types;

pub use elf::*;
pub use launch::*;
pub use programs::*;
pub use types::*;
