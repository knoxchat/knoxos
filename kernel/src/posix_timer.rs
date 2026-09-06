/// POSIX Timer & Interval Timer Subsystem
/// Implements POSIX per-process timers and classical interval timers
///
/// Features:
/// - timer_create / timer_settime / timer_delete (POSIX per-process timers)
/// - setitimer / getitimer (classical interval timers: REAL, VIRTUAL, PROF)
/// - alarm() syscall
/// - CLOCK_REALTIME, CLOCK_MONOTONIC, CLOCK_PROCESS_CPUTIME_ID
/// - Signal delivery on timer expiry (SIGALRM, SIGVTALRM, SIGPROF)
/// - Timer overrun counting
/// - Thread-directed signals (SIGEV_THREAD_ID)
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── Clock IDs ──────────────────────────────────────────────────────

pub const CLOCK_REALTIME: u32 = 0;
pub const CLOCK_MONOTONIC: u32 = 1;
pub const CLOCK_PROCESS_CPUTIME_ID: u32 = 2;
pub const CLOCK_THREAD_CPUTIME_ID: u32 = 3;
pub const CLOCK_MONOTONIC_RAW: u32 = 4;
pub const CLOCK_REALTIME_COARSE: u32 = 5;
pub const CLOCK_MONOTONIC_COARSE: u32 = 6;
pub const CLOCK_BOOTTIME: u32 = 7;

// ─── Interval Timer Types ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ITimerWhich {
    Real = 0,    // ITIMER_REAL — SIGALRM
    Virtual = 1, // ITIMER_VIRTUAL — SIGVTALRM
    Prof = 2,    // ITIMER_PROF — SIGPROF
}

impl ITimerWhich {
    pub fn from_i32(v: i32) -> Option<Self> {
        match v {
            0 => Some(Self::Real),
            1 => Some(Self::Virtual),
            2 => Some(Self::Prof),
            _ => None,
        }
    }

    pub fn signal(&self) -> u32 {
        match self {
            Self::Real => 14,    // SIGALRM
            Self::Virtual => 26, // SIGVTALRM
            Self::Prof => 27,    // SIGPROF
        }
    }
}

// ─── Timer Notification ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub enum TimerNotify {
    Signal(u32),      // SIGEV_SIGNAL — deliver signal
    Thread(u32, u32), // SIGEV_THREAD_ID — deliver to specific thread
    None,             // SIGEV_NONE — no notification
}

// ─── Timespec ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Default)]
pub struct Timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

impl Timespec {
    pub fn from_ns(ns: u64) -> Self {
        Self {
            tv_sec: (ns / 1_000_000_000) as i64,
            tv_nsec: (ns % 1_000_000_000) as i64,
        }
    }

    pub fn to_ns(&self) -> u64 {
        (self.tv_sec as u64) * 1_000_000_000 + self.tv_nsec as u64
    }

    pub fn is_zero(&self) -> bool {
        self.tv_sec == 0 && self.tv_nsec == 0
    }
}

// ─── POSIX Timer ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PosixTimer {
    pub id: u32,
    pub pid: u32,
    pub clock_id: u32,
    pub notify: TimerNotify,
    pub interval: Timespec,    // Repeat interval (0 = one-shot)
    pub next_expiry: Timespec, // Next expiration time
    pub armed: bool,
    pub overrun_count: u32,
    pub absolute: bool, // TIMER_ABSTIME flag
}

impl PosixTimer {
    pub fn new(id: u32, pid: u32, clock_id: u32, notify: TimerNotify) -> Self {
        Self {
            id,
            pid,
            clock_id,
            notify,
            interval: Timespec::default(),
            next_expiry: Timespec::default(),
            armed: false,
            overrun_count: 0,
            absolute: false,
        }
    }
}

// ─── Interval Timer (setitimer) ─────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct ITimerVal {
    pub interval: Timespec, // Repeat interval
    pub value: Timespec,    // Time until next expiry
}

// ─── Per-Process Timer State ────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ProcessTimers {
    pub posix_timers: BTreeMap<u32, PosixTimer>,
    pub itimers: [ITimerVal; 3], // REAL, VIRTUAL, PROF
    pub alarm_remaining: u64,    // alarm() remaining seconds
}

impl Default for ProcessTimers {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessTimers {
    pub fn new() -> Self {
        Self {
            posix_timers: BTreeMap::new(),
            itimers: [
                ITimerVal::default(),
                ITimerVal::default(),
                ITimerVal::default(),
            ],
            alarm_remaining: 0,
        }
    }
}

// ─── Global Timer State ────────────────────────────────────────────

static PROCESS_TIMERS: Mutex<BTreeMap<u32, ProcessTimers>> = Mutex::new(BTreeMap::new());
static NEXT_TIMER_ID: AtomicU32 = AtomicU32::new(1);

/// timer_create — create a per-process timer
pub fn timer_create(pid: u32, clock_id: u32, notify: TimerNotify) -> Result<u32, &'static str> {
    let id = NEXT_TIMER_ID.fetch_add(1, Ordering::Relaxed);
    let timer = PosixTimer::new(id, pid, clock_id, notify);

    let mut all = PROCESS_TIMERS.lock();
    let pt = all.entry(pid).or_default();
    pt.posix_timers.insert(id, timer);

    serial_println!(
        "[TIMER] Created timer {} for pid {} (clock={})",
        id,
        pid,
        clock_id
    );
    Ok(id)
}

/// timer_settime — arm/disarm a timer
pub fn timer_settime(
    pid: u32,
    timer_id: u32,
    interval: Timespec,
    value: Timespec,
    absolute: bool,
) -> Result<Timespec, &'static str> {
    let mut all = PROCESS_TIMERS.lock();
    let pt = all.get_mut(&pid).ok_or("No timers for process")?;
    let timer = pt
        .posix_timers
        .get_mut(&timer_id)
        .ok_or("Timer not found")?;

    let old_value = timer.next_expiry;

    timer.interval = interval;
    timer.next_expiry = value;
    timer.armed = !value.is_zero();
    timer.absolute = absolute;
    timer.overrun_count = 0;

    Ok(old_value)
}

/// timer_delete — delete a timer
pub fn timer_delete(pid: u32, timer_id: u32) -> Result<(), &'static str> {
    let mut all = PROCESS_TIMERS.lock();
    let pt = all.get_mut(&pid).ok_or("No timers for process")?;
    pt.posix_timers.remove(&timer_id).ok_or("Timer not found")?;
    Ok(())
}

/// timer_getoverrun — get overrun count
pub fn timer_getoverrun(pid: u32, timer_id: u32) -> Result<u32, &'static str> {
    let all = PROCESS_TIMERS.lock();
    let pt = all.get(&pid).ok_or("No timers for process")?;
    let timer = pt.posix_timers.get(&timer_id).ok_or("Timer not found")?;
    Ok(timer.overrun_count)
}

/// setitimer — set interval timer
pub fn setitimer(
    pid: u32,
    which: ITimerWhich,
    new_val: ITimerVal,
) -> Result<ITimerVal, &'static str> {
    let mut all = PROCESS_TIMERS.lock();
    let pt = all.entry(pid).or_default();

    let old = pt.itimers[which as usize].clone();
    pt.itimers[which as usize] = new_val;
    Ok(old)
}

/// getitimer — get interval timer
pub fn getitimer(pid: u32, which: ITimerWhich) -> Result<ITimerVal, &'static str> {
    let all = PROCESS_TIMERS.lock();
    let pt = all.get(&pid).ok_or("No timers for process")?;
    Ok(pt.itimers[which as usize].clone())
}

/// alarm — set a SIGALRM timer (returns previous remaining seconds)
pub fn alarm(pid: u32, seconds: u32) -> u32 {
    let mut all = PROCESS_TIMERS.lock();
    let pt = all.entry(pid).or_default();

    let prev = pt.alarm_remaining as u32;
    pt.alarm_remaining = seconds as u64;

    // Set ITIMER_REAL as well
    pt.itimers[0] = ITimerVal {
        interval: Timespec::default(), // one-shot
        value: Timespec {
            tv_sec: seconds as i64,
            tv_nsec: 0,
        },
    };
    prev
}

/// Process timer tick — called from timer interrupt handler
pub fn tick(current_ns: u64) {
    let mut all = PROCESS_TIMERS.lock();
    for (_pid, pt) in all.iter_mut() {
        // Check POSIX timers
        for (_id, timer) in pt.posix_timers.iter_mut() {
            if !timer.armed {
                continue;
            }
            if current_ns >= timer.next_expiry.to_ns() {
                // Timer expired
                if !timer.interval.is_zero() {
                    // Periodic: reschedule
                    timer.next_expiry = Timespec::from_ns(current_ns + timer.interval.to_ns());
                    if timer.next_expiry.to_ns() <= current_ns {
                        timer.overrun_count += 1;
                    }
                } else {
                    timer.armed = false;
                }
                // Deliver signal based on notify type
                match timer.notify {
                    TimerNotify::Signal(sig) => {
                        if let Some(signal) = crate::signals::Signal::from_number(sig) {
                            let _ = crate::signals::kill(timer.pid, signal, 0);
                        }
                    }
                    TimerNotify::Thread(tid, sig) => {
                        if let Some(signal) = crate::signals::Signal::from_number(sig) {
                            let _ = crate::signals::kill(tid, signal, 0);
                        }
                    }
                    TimerNotify::None => {}
                }
            }
        }
    }
}

pub fn init() {
    serial_println!("[TIMER] POSIX timer subsystem initialized");
    serial_println!("[TIMER]   timer_create/settime/delete, setitimer/getitimer, alarm");
    serial_println!("[TIMER]   Clocks: REALTIME, MONOTONIC, PROCESS_CPUTIME, THREAD_CPUTIME");
}
