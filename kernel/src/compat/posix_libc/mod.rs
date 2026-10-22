// POSIX libc Compatibility Layer (Phase 24)
//
// Provides comprehensive C library function coverage for running
// unmodified Linux/Debian binaries. This extends libc_funcs.rs with:
//
//   - stdio.h: printf/fprintf/snprintf formatting engine
//   - stdlib.h: Enhanced memory allocation with size tracking
//   - unistd.h: POSIX file/process operations
//   - pthread.h: POSIX threads (via clone/futex)
//   - dirent.h: Directory operations
//   - time.h: Clock and time functions
//   - signal.h: Signal management
//   - fcntl.h: File control operations
//   - sys/stat.h: File status
//   - sys/mman.h: Memory mapping
//   - dlfcn.h: Dynamic loading (bridges to dynlink.rs)
//   - locale.h: Locale support
//   - math.h: Math functions (software float)
//
// All functions use C ABI (`extern "C"`) and are `#[unsafe(no_mangle)]` so
// they can be resolved by the dynamic linker at load time.
//
// Split into submodules for maintainability:
//   malloc   — tracked malloc/free/realloc
//   printf   — PrintfFormatter and printf_engine
//   fcntl    — open/seek/access flags
//   stdio    — FILE streams (fopen/fclose/fflush/…)
//   pthread  — POSIX threads, mutex, cond, TLS
//   time     — clock_gettime, nanosleep, sleep
//   dirent   — opendir/closedir
//   signal   — sigaction, sigset ops
//   math     — software float
//   locale   — setlocale / localeconv
//   dlfcn    — dlopen/dlsym/dlclose/dlerror C ABI
//   unistd   — getpid, getcwd, access, pipe, dup, strerror
#![allow(
    clippy::missing_safety_doc,
    clippy::not_unsafe_ptr_arg_deref,
    clippy::needless_range_loop,
    clippy::too_many_arguments,
    clippy::type_complexity,
    dead_code
)]

use core::sync::atomic::Ordering;

use crate::serial_println;

mod dirent;
mod dlfcn;
mod fcntl;
mod locale;
mod malloc;
mod math;
mod printf;
mod pthread;
mod signal;
mod stdio;
mod time;
mod unistd;

pub use dirent::*;
pub use dlfcn::*;
pub use fcntl::*;
pub use locale::*;
pub use malloc::*;
pub use math::*;
pub use printf::*;
pub use pthread::*;
pub use signal::*;
pub use stdio::*;
pub use time::*;
pub use unistd::*;

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

pub fn init() {
    time::BOOT_TSC.store(crate::clock::get_ticks(), Ordering::Relaxed);

    serial_println!("[KnoxOS] POSIX libc compatibility layer initialized (Phase 24)");
    serial_println!("[KnoxOS]   stdio.h: printf engine, fopen/fclose/fflush, FILE streams");
    serial_println!("[KnoxOS]   pthread.h: create/join/detach, mutex, cond, TLS, once");
    serial_println!("[KnoxOS]   time.h: clock_gettime, gettimeofday, nanosleep, sleep");
    serial_println!("[KnoxOS]   dirent.h: opendir/readdir/closedir");
    serial_println!("[KnoxOS]   signal.h: sigaction, signal, sigprocmask, sigset ops");
    serial_println!("[KnoxOS]   math.h: sqrt, floor, ceil, round, log, pow, exp, fmod");
    serial_println!("[KnoxOS]   locale.h: setlocale, localeconv");
    serial_println!("[KnoxOS]   dlfcn.h: dlopen, dlsym, dlclose, dlerror (→ dynlink.rs)");
    serial_println!("[KnoxOS]   unistd.h: getpid, getcwd, sleep, access, pipe, dup");
}
