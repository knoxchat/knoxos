/// User Task — scheduled Ring 3 processes
///
/// This is the difference between "a program ran once at boot" and "the OS
/// runs programs". It owns the lifecycle every Ring 3 task shares:
///
///   * `spawn` — build the task (address space, ELF, stack, context, fd
///     table, signal state, kernel stack) and put it on the run queue.
///   * `finish_current` — a task called `exit`. Retire it, wake its parent,
///     and give the CPU to whoever is runnable next.
///   * `park_current` — a task blocked in a syscall (wait4 / pause). Save a
///     Ring 3 resume point, block it, and switch away.
///   * `wake` / `terminate` — the thing a task was waiting for happened, or
///     a fatal signal arrived.
///
/// The address-space switch happens inside `context::enter_context`, so a
/// task always runs on its own CR3 and its own kernel stack.
///
/// Split into submodules for maintainability:
///   spawn     — `spawn_elf` and the kernel-stack size
///   lifecycle — exit, park, wait/join/futex, and the scheduler handoff
///   gates     — boot-time Ring 3 demonstrations and the launcher spawn
mod gates;
mod lifecycle;
mod spawn;

pub use gates::*;
pub use lifecycle::*;
pub use spawn::*;
