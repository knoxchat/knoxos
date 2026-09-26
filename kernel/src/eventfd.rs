/// eventfd - Event file descriptor
/// Compatible with Linux eventfd(2) interface
/// Provides a mechanism for event wait/notify
use alloc::collections::BTreeMap;
use spin::Mutex;

/// eventfd flags
pub const EFD_SEMAPHORE: i32 = 0x00000001;
pub const EFD_CLOEXEC: i32 = 0x00080000;
pub const EFD_NONBLOCK: i32 = 0x00000800;

/// An eventfd instance
struct EventFd {
    counter: u64,
    flags: i32,
    semaphore: bool,
}

impl EventFd {
    fn new(initval: u64, flags: i32) -> Self {
        Self {
            counter: initval,
            flags,
            semaphore: flags & EFD_SEMAPHORE != 0,
        }
    }

    fn read(&mut self) -> Result<u64, i32> {
        if self.counter == 0 {
            if self.flags & EFD_NONBLOCK != 0 {
                return Err(-11); // EAGAIN
            }
            // In a real implementation, we would block here
            return Err(-11); // EAGAIN for now
        }

        if self.semaphore {
            self.counter -= 1;
            Ok(1)
        } else {
            let val = self.counter;
            self.counter = 0;
            Ok(val)
        }
    }

    fn write(&mut self, val: u64) -> Result<(), i32> {
        if val == u64::MAX {
            return Err(-22); // EINVAL
        }
        let new_val = self.counter.checked_add(val).ok_or(-22i32)?;
        if new_val > u64::MAX - 1 {
            if self.flags & EFD_NONBLOCK != 0 {
                return Err(-11); // EAGAIN
            }
            return Err(-11); // EAGAIN for now
        }
        self.counter = new_val;
        Ok(())
    }
}

/// Global eventfd table
lazy_static::lazy_static! {
    static ref EVENTFDS: Mutex<BTreeMap<i32, EventFd>> = Mutex::new(BTreeMap::new());
}

static NEXT_EVENTFD: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(3000);

/// Create a new eventfd
pub fn eventfd_create(initval: u64, flags: i32) -> Result<i32, i32> {
    let fd = NEXT_EVENTFD.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    EVENTFDS.lock().insert(fd, EventFd::new(initval, flags));
    crate::serial_println!("[KnoxOS] eventfd({}, {:#x}) = {}", initval, flags, fd);
    Ok(fd)
}

/// Read from an eventfd
pub fn eventfd_read(fd: i32) -> Result<u64, i32> {
    let mut fds = EVENTFDS.lock();
    let efd = fds.get_mut(&fd).ok_or(-9i32)?; // EBADF
    efd.read()
}

/// Write to an eventfd
pub fn eventfd_write(fd: i32, val: u64) -> Result<(), i32> {
    let mut fds = EVENTFDS.lock();
    let efd = fds.get_mut(&fd).ok_or(-9i32)?; // EBADF
    efd.write(val)
}

/// Read the counter into an 8-byte little-endian buffer (Linux eventfd ABI).
pub fn eventfd_read_bytes(id: i32, buf: &mut [u8]) -> Result<usize, i32> {
    if buf.len() < 8 {
        return Err(-22); // EINVAL
    }
    let val = eventfd_read(id)?;
    buf[..8].copy_from_slice(&val.to_le_bytes());
    Ok(8)
}

/// Add an 8-byte little-endian counter write (Linux eventfd ABI).
pub fn eventfd_write_bytes(id: i32, buf: &[u8]) -> Result<usize, i32> {
    if buf.len() < 8 {
        return Err(-22); // EINVAL
    }
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&buf[..8]);
    eventfd_write(id, u64::from_le_bytes(bytes))?;
    Ok(8)
}

/// Write then read a counter through the eventfd table used by `sys_eventfd`.
pub fn eventfd_roundtrip_self_test() -> bool {
    let Ok(id) = eventfd_create(0, EFD_NONBLOCK) else {
        return false;
    };
    if eventfd_write(id, 1).is_err() {
        eventfd_close(id);
        return false;
    }
    let got = eventfd_read(id);
    eventfd_close(id);
    matches!(got, Ok(1))
}

/// Close an eventfd
pub fn eventfd_close(fd: i32) {
    EVENTFDS.lock().remove(&fd);
}

/// Check if fd is an eventfd
pub fn is_eventfd(fd: i32) -> bool {
    EVENTFDS.lock().contains_key(&fd)
}

/// Initialize eventfd subsystem
pub fn init() {
    crate::serial_println!("[KnoxOS] eventfd notification initialized");
}
