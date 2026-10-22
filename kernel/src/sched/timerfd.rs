/// timerfd - Timer file descriptors
/// Compatible with Linux timerfd_create(2), timerfd_settime(2), timerfd_gettime(2)
/// Provides timers that notify via file descriptors
use alloc::collections::BTreeMap;
use spin::Mutex;

/// Clock IDs (Linux-compatible)
pub const CLOCK_REALTIME: i32 = 0;
pub const CLOCK_MONOTONIC: i32 = 1;

/// timerfd flags
pub const TFD_CLOEXEC: i32 = 0x00080000;
pub const TFD_NONBLOCK: i32 = 0x00000800;
pub const TFD_TIMER_ABSTIME: i32 = 0x00000001;

/// Timer specification
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ITimerSpec {
    /// Timer interval (for repeating timers)
    pub it_interval: crate::rtc::Timespec,
    /// Initial expiration
    pub it_value: crate::rtc::Timespec,
}

impl ITimerSpec {
    pub fn zero() -> Self {
        Self {
            it_interval: crate::rtc::Timespec {
                tv_sec: 0,
                tv_nsec: 0,
            },
            it_value: crate::rtc::Timespec {
                tv_sec: 0,
                tv_nsec: 0,
            },
        }
    }
}

/// A timer file descriptor
struct TimerFd {
    clock_id: i32,
    flags: i32,
    spec: ITimerSpec,
    expirations: u64,
    start_ticks: u64,
    armed: bool,
}

impl TimerFd {
    fn new(clock_id: i32, flags: i32) -> Self {
        Self {
            clock_id,
            flags,
            spec: ITimerSpec::zero(),
            expirations: 0,
            start_ticks: crate::interrupts::get_ticks(),
            armed: false,
        }
    }

    fn settime(&mut self, flags: i32, new_value: &ITimerSpec) -> ITimerSpec {
        let old = self.spec;
        self.spec = *new_value;
        self.start_ticks = crate::interrupts::get_ticks();
        self.expirations = 0;
        self.armed = new_value.it_value.tv_sec != 0 || new_value.it_value.tv_nsec != 0;
        old
    }

    fn gettime(&self) -> ITimerSpec {
        self.spec
    }

    fn is_expired(&self) -> bool {
        if !self.armed {
            return false;
        }
        let elapsed_ticks = crate::interrupts::get_ticks() - self.start_ticks;
        let timer_ticks = (self.spec.it_value.tv_sec as u64) * 18; // ~18.2Hz PIT
        elapsed_ticks >= timer_ticks
    }

    fn read(&mut self) -> Result<u64, i32> {
        if !self.armed {
            if self.flags & TFD_NONBLOCK != 0 {
                return Err(-11); // EAGAIN
            }
            return Err(-11);
        }

        // Check if timer has expired
        let elapsed_ticks = crate::interrupts::get_ticks() - self.start_ticks;
        let timer_ticks = (self.spec.it_value.tv_sec as u64) * 18; // ~18.2Hz PIT

        if elapsed_ticks >= timer_ticks {
            let count = if self.spec.it_interval.tv_sec > 0 || self.spec.it_interval.tv_nsec > 0 {
                let interval_ticks = (self.spec.it_interval.tv_sec as u64) * 18;
                (elapsed_ticks - timer_ticks)
                    .checked_div(interval_ticks)
                    .map_or(1, |v| v + 1)
            } else {
                self.armed = false;
                1
            };
            self.expirations += count;
            let result = self.expirations;
            self.expirations = 0;
            Ok(result)
        } else {
            if self.flags & TFD_NONBLOCK != 0 {
                Err(-11) // EAGAIN
            } else {
                Ok(0)
            }
        }
    }
}

/// Global timerfd table
lazy_static::lazy_static! {
    static ref TIMER_FDS: Mutex<BTreeMap<i32, TimerFd>> = Mutex::new(BTreeMap::new());
}

static NEXT_TIMER_FD: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(4000);

/// Create a new timer file descriptor
pub fn timerfd_create(clock_id: i32, flags: i32) -> Result<i32, i32> {
    if clock_id != CLOCK_REALTIME && clock_id != CLOCK_MONOTONIC {
        return Err(-22); // EINVAL
    }
    let fd = NEXT_TIMER_FD.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    TIMER_FDS.lock().insert(fd, TimerFd::new(clock_id, flags));
    crate::serial_println!("[KnoxOS] timerfd_create({}) = {}", clock_id, fd);
    Ok(fd)
}

/// Set the timer
pub fn timerfd_settime(fd: i32, flags: i32, new_value: &ITimerSpec) -> Result<ITimerSpec, i32> {
    let mut fds = TIMER_FDS.lock();
    let tfd = fds.get_mut(&fd).ok_or(-9i32)?;
    Ok(tfd.settime(flags, new_value))
}

/// Get the current timer value
pub fn timerfd_gettime(fd: i32) -> Result<ITimerSpec, i32> {
    let fds = TIMER_FDS.lock();
    let tfd = fds.get(&fd).ok_or(-9i32)?;
    Ok(tfd.gettime())
}

/// Read from a timerfd (returns number of expirations)
pub fn timerfd_read(fd: i32) -> Result<u64, i32> {
    let mut fds = TIMER_FDS.lock();
    let tfd = fds.get_mut(&fd).ok_or(-9i32)?;
    tfd.read()
}

/// Read expirations into an 8-byte little-endian buffer (Linux timerfd ABI).
pub fn timerfd_read_bytes(id: i32, buf: &mut [u8]) -> Result<usize, i32> {
    if buf.len() < 8 {
        return Err(-22); // EINVAL
    }
    let val = timerfd_read(id)?;
    buf[..8].copy_from_slice(&val.to_le_bytes());
    Ok(8)
}

/// Whether a timerfd read would return an expiration count.
pub fn timerfd_would_read(fd: i32) -> bool {
    TIMER_FDS
        .lock()
        .get(&fd)
        .map(|tfd| tfd.is_expired())
        .unwrap_or(false)
}

/// Create, arm a 1ns one-shot, and read at least one expiration.
pub fn timerfd_expire_self_test() -> bool {
    let Ok(id) = timerfd_create(CLOCK_MONOTONIC, TFD_NONBLOCK) else {
        return false;
    };
    let spec = ITimerSpec {
        it_interval: crate::rtc::Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        },
        it_value: crate::rtc::Timespec {
            tv_sec: 0,
            tv_nsec: 1,
        },
    };
    if timerfd_settime(id, 0, &spec).is_err() {
        timerfd_close(id);
        return false;
    }
    let got = timerfd_read(id);
    timerfd_close(id);
    matches!(got, Ok(n) if n >= 1)
}

/// Close a timerfd
pub fn timerfd_close(fd: i32) {
    TIMER_FDS.lock().remove(&fd);
}

/// Check if fd is a timerfd
pub fn is_timerfd(fd: i32) -> bool {
    TIMER_FDS.lock().contains_key(&fd)
}

/// Initialize timerfd subsystem
pub fn init() {
    crate::serial_println!("[KnoxOS] timerfd timer file descriptors initialized");
}
