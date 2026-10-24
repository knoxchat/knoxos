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
///   elf      — static ELF64 packer; Gate B2 hello blob (source in userspace/apps/hello)
///   launch   — filesystem search, hello iretq, start_init
///   programs — Ring 3 gate fixtures (process, I/O, fs, identity, net, display, ENOSYS)
mod chromium;
mod chromium_desktop;
mod chromium_gl;
mod chromium_https;
mod chromium_iouring;
mod chromium_loader;
mod chromium_media;
mod chromium_mojo;
mod chromium_polish;
mod chromium_sandbox;
mod dynlink;
mod elf;
mod launch;
mod programs;
mod types;
mod vivaldi;

pub use chromium::*;
pub use chromium_desktop::*;
pub use chromium_gl::*;
pub use chromium_https::*;
pub use chromium_iouring::*;
pub use chromium_loader::*;
pub use chromium_media::*;
pub use chromium_mojo::*;
pub use chromium_polish::*;
pub use chromium_sandbox::*;
pub use dynlink::*;
pub use elf::*;
pub use launch::*;
pub use programs::*;
pub use types::*;
pub use vivaldi::*;
