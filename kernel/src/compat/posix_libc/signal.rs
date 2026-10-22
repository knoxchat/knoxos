// Signal management (signal.h)
use crate::serial_println;

/// Signal set type
pub type SigsetT = u64;

/// struct sigaction
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Sigaction {
    pub sa_handler: u64,
    pub sa_flags: u64,
    pub sa_restorer: u64,
    pub sa_mask: SigsetT,
}

pub const SIG_DFL: u64 = 0;
pub const SIG_IGN: u64 = 1;
pub const SIG_ERR: u64 = u64::MAX;

pub const SA_RESTART: u64 = 0x10000000;
pub const SA_NODEFER: u64 = 0x40000000;
pub const SA_SIGINFO: u64 = 0x00000004;

/// sigaction — examine and change a signal action
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigaction(
    signum: i32,
    act: *const Sigaction,
    oldact: *mut Sigaction,
) -> i32 {
    // Bridge to kernel signal subsystem
    if !oldact.is_null() {
        // Return default action
        (*oldact).sa_handler = SIG_DFL;
        (*oldact).sa_flags = 0;
        (*oldact).sa_restorer = 0;
        (*oldact).sa_mask = 0;
    }
    if !act.is_null() {
        serial_println!(
            "[signal] sigaction: sig={}, handler={:#x}",
            signum,
            (*act).sa_handler
        );
    }
    0
}

/// signal — simplified signal handling
#[unsafe(no_mangle)]
pub unsafe extern "C" fn signal(signum: i32, handler: u64) -> u64 {
    serial_println!("[signal] signal: sig={}, handler={:#x}", signum, handler);
    SIG_DFL // Return previous handler
}

/// sigprocmask — examine and change blocked signals
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigprocmask(_how: i32, _set: *const SigsetT, oldset: *mut SigsetT) -> i32 {
    if !oldset.is_null() {
        *oldset = 0;
    }
    0
}

/// sigemptyset — initialize empty signal set
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigemptyset(set: *mut SigsetT) -> i32 {
    if !set.is_null() {
        *set = 0;
    }
    0
}

/// sigfillset — initialize full signal set
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigfillset(set: *mut SigsetT) -> i32 {
    if !set.is_null() {
        *set = u64::MAX;
    }
    0
}

/// sigaddset — add signal to set
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigaddset(set: *mut SigsetT, signum: i32) -> i32 {
    if !set.is_null() && signum > 0 && signum < 64 {
        *set |= 1u64 << (signum - 1);
    }
    0
}

/// sigdelset — remove signal from set
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigdelset(set: *mut SigsetT, signum: i32) -> i32 {
    if !set.is_null() && signum > 0 && signum < 64 {
        *set &= !(1u64 << (signum - 1));
    }
    0
}
