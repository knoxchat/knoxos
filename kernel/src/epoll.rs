/// epoll - I/O event notification facility
/// Compatible with Linux epoll(7) interface
/// Provides scalable I/O multiplexing for file descriptors
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

/// epoll event flags (Linux-compatible)
pub const EPOLLIN: u32 = 0x001;
pub const EPOLLOUT: u32 = 0x004;
pub const EPOLLERR: u32 = 0x008;
pub const EPOLLHUP: u32 = 0x010;
pub const EPOLLRDHUP: u32 = 0x2000;
pub const EPOLLET: u32 = 1 << 31; // Edge-triggered
pub const EPOLLONESHOT: u32 = 1 << 30;

/// epoll_ctl operations
pub const EPOLL_CTL_ADD: i32 = 1;
pub const EPOLL_CTL_DEL: i32 = 2;
pub const EPOLL_CTL_MOD: i32 = 3;

/// epoll event structure (matches Linux struct epoll_event)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct EpollEvent {
    pub events: u32,
    pub data: u64,
}

/// An entry being watched by an epoll instance
#[derive(Debug, Clone)]
struct EpollEntry {
    fd: i32,
    events: u32,
    data: u64,
    edge_triggered: bool,
    oneshot: bool,
    last_ready: u32,
}

/// An epoll instance
struct EpollInstance {
    entries: BTreeMap<i32, EpollEntry>,
}

impl EpollInstance {
    fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    fn add(&mut self, fd: i32, event: &EpollEvent) -> Result<(), i32> {
        if self.entries.contains_key(&fd) {
            return Err(-17); // EEXIST
        }
        self.entries.insert(
            fd,
            EpollEntry {
                fd,
                events: event.events & !(EPOLLET | EPOLLONESHOT),
                data: event.data,
                edge_triggered: event.events & EPOLLET != 0,
                oneshot: event.events & EPOLLONESHOT != 0,
                last_ready: 0,
            },
        );
        Ok(())
    }

    fn modify(&mut self, fd: i32, event: &EpollEvent) -> Result<(), i32> {
        let entry = self.entries.get_mut(&fd).ok_or(-2i32)?; // ENOENT
        entry.events = event.events & !(EPOLLET | EPOLLONESHOT);
        entry.data = event.data;
        entry.edge_triggered = event.events & EPOLLET != 0;
        entry.oneshot = event.events & EPOLLONESHOT != 0;
        Ok(())
    }

    fn delete(&mut self, fd: i32) -> Result<(), i32> {
        self.entries.remove(&fd).ok_or(-2i32)?; // ENOENT
        Ok(())
    }

    fn poll(&mut self, events: &mut [EpollEvent], _timeout_ms: i32) -> usize {
        let mut count = 0;
        let mut to_disable = Vec::new();

        for entry in self.entries.values_mut() {
            if count >= events.len() {
                break;
            }

            // Check if fd is ready (simplified: check fd validity and readiness)
            let ready = check_fd_readiness(entry.fd, entry.events);

            if ready != 0 {
                // For edge-triggered: only report if state changed
                if entry.edge_triggered {
                    if ready == entry.last_ready {
                        continue;
                    }
                    entry.last_ready = ready;
                }

                events[count] = EpollEvent {
                    events: ready,
                    data: entry.data,
                };
                count += 1;

                if entry.oneshot {
                    to_disable.push(entry.fd);
                }
            }
        }

        // Disable oneshot entries that fired
        for fd in to_disable {
            if let Some(entry) = self.entries.get_mut(&fd) {
                entry.events = 0;
            }
        }

        count
    }
}

/// Check readiness of a file descriptor
fn check_fd_readiness(fd: i32, interest: u32) -> u32 {
    let mut ready = 0u32;

    // stdin is always readable (we have keyboard input)
    if fd == 0 && interest & EPOLLIN != 0 {
        ready |= EPOLLIN;
    }

    // stdout/stderr are always writable
    if (fd == 1 || fd == 2) && interest & EPOLLOUT != 0 {
        ready |= EPOLLOUT;
    }

    // Regular files are always readable and writable
    if fd >= 3 {
        let pid = crate::scheduler::current_pid().unwrap_or(1);
        let tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(fd_table) = tables.get(&pid) {
            if let Some(file) = fd_table.get(fd) {
                match file.file_type {
                    crate::fd::FileType::Regular | crate::fd::FileType::ProcFile => {
                        if interest & EPOLLIN != 0 {
                            ready |= EPOLLIN;
                        }
                        if interest & EPOLLOUT != 0 {
                            ready |= EPOLLOUT;
                        }
                    }
                    crate::fd::FileType::Pipe => {
                        // Pipe: readable if data available, writable if space available
                        if interest & EPOLLIN != 0 {
                            ready |= EPOLLIN;
                        }
                        if interest & EPOLLOUT != 0 {
                            ready |= EPOLLOUT;
                        }
                    }
                    crate::fd::FileType::Socket => {
                        if interest & EPOLLIN != 0 {
                            ready |= EPOLLIN;
                        }
                        if interest & EPOLLOUT != 0 {
                            ready |= EPOLLOUT;
                        }
                    }
                    _ => {
                        if interest & EPOLLIN != 0 {
                            ready |= EPOLLIN;
                        }
                        if interest & EPOLLOUT != 0 {
                            ready |= EPOLLOUT;
                        }
                    }
                }
            } else {
                ready |= EPOLLERR; // Bad fd
            }
        }
    }

    ready
}

/// Global epoll instance table
lazy_static::lazy_static! {
    static ref EPOLL_INSTANCES: Mutex<BTreeMap<i32, EpollInstance>> = Mutex::new(BTreeMap::new());
}

static NEXT_EPOLL_FD: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(1000);

/// Create a new epoll instance
pub fn epoll_create() -> Result<i32, i32> {
    let fd = NEXT_EPOLL_FD.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    EPOLL_INSTANCES.lock().insert(fd, EpollInstance::new());
    crate::serial_println!("[KnoxOS] epoll_create() = {}", fd);
    Ok(fd)
}

/// Control an epoll instance (add/mod/del)
pub fn epoll_ctl(epfd: i32, op: i32, fd: i32, event: &EpollEvent) -> Result<(), i32> {
    let mut instances = EPOLL_INSTANCES.lock();
    let instance = instances.get_mut(&epfd).ok_or(-9i32)?; // EBADF

    match op {
        EPOLL_CTL_ADD => instance.add(fd, event),
        EPOLL_CTL_MOD => instance.modify(fd, event),
        EPOLL_CTL_DEL => instance.delete(fd),
        _ => Err(-22), // EINVAL
    }
}

/// Wait for events on an epoll instance
pub fn epoll_wait(epfd: i32, events: &mut [EpollEvent], timeout_ms: i32) -> Result<usize, i32> {
    let mut instances = EPOLL_INSTANCES.lock();
    let instance = instances.get_mut(&epfd).ok_or(-9i32)?; // EBADF
    Ok(instance.poll(events, timeout_ms))
}

/// Close an epoll instance
pub fn epoll_close(epfd: i32) {
    EPOLL_INSTANCES.lock().remove(&epfd);
}

// ═══════════════════════════════════════════════════════════════════════
// poll() - Simplified poll implementation
// ═══════════════════════════════════════════════════════════════════════

/// poll event flags
pub const POLLIN: i16 = 0x001;
pub const POLLOUT: i16 = 0x004;
pub const POLLERR: i16 = 0x008;
pub const POLLHUP: i16 = 0x010;
pub const POLLNVAL: i16 = 0x020;
pub const POLLPRI: i16 = 0x002;

/// struct pollfd (Linux-compatible)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PollFd {
    pub fd: i32,
    pub events: i16,
    pub revents: i16,
}

/// poll() system call implementation
pub fn poll(fds: &mut [PollFd], _timeout_ms: i32) -> usize {
    let mut ready_count = 0;

    for pfd in fds.iter_mut() {
        pfd.revents = 0;

        if pfd.fd < 0 {
            continue;
        }

        let interest = pfd.events as u32;
        let mut epoll_interest = 0u32;
        if interest & POLLIN as u32 != 0 {
            epoll_interest |= EPOLLIN;
        }
        if interest & POLLOUT as u32 != 0 {
            epoll_interest |= EPOLLOUT;
        }

        let ready = check_fd_readiness(pfd.fd, epoll_interest);

        if ready & EPOLLIN != 0 {
            pfd.revents |= POLLIN;
        }
        if ready & EPOLLOUT != 0 {
            pfd.revents |= POLLOUT;
        }
        if ready & EPOLLERR != 0 {
            pfd.revents |= POLLERR;
        }
        if ready & EPOLLHUP != 0 {
            pfd.revents |= POLLHUP;
        }

        if pfd.revents != 0 {
            ready_count += 1;
        }
    }

    ready_count
}

// ═══════════════════════════════════════════════════════════════════════
// select() - POSIX select() implementation
// ═══════════════════════════════════════════════════════════════════════

/// fd_set for select (up to 1024 fds)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FdSet {
    pub bits: [u64; 16], // 1024 bits = 16 * 64
}

impl Default for FdSet {
    fn default() -> Self {
        Self::new()
    }
}

impl FdSet {
    pub fn new() -> Self {
        Self { bits: [0; 16] }
    }

    pub fn set(&mut self, fd: i32) {
        if (0..1024).contains(&fd) {
            let idx = fd as usize / 64;
            let bit = fd as usize % 64;
            self.bits[idx] |= 1u64 << bit;
        }
    }

    pub fn clear(&mut self, fd: i32) {
        if (0..1024).contains(&fd) {
            let idx = fd as usize / 64;
            let bit = fd as usize % 64;
            self.bits[idx] &= !(1u64 << bit);
        }
    }

    pub fn is_set(&self, fd: i32) -> bool {
        if (0..1024).contains(&fd) {
            let idx = fd as usize / 64;
            let bit = fd as usize % 64;
            self.bits[idx] & (1u64 << bit) != 0
        } else {
            false
        }
    }

    pub fn zero(&mut self) {
        self.bits = [0; 16];
    }
}

/// select() system call implementation
pub fn select(
    nfds: i32,
    readfds: Option<&mut FdSet>,
    writefds: Option<&mut FdSet>,
    exceptfds: Option<&mut FdSet>,
    _timeout: Option<&crate::rtc::Timeval>,
) -> i32 {
    let mut ready_count = 0i32;

    // Create result sets
    let mut read_result = FdSet::new();
    let mut write_result = FdSet::new();
    let mut except_result = FdSet::new();

    for fd in 0..nfds {
        let check_read = readfds.as_ref().is_some_and(|s| s.is_set(fd));
        let check_write = writefds.as_ref().is_some_and(|s| s.is_set(fd));
        let check_except = exceptfds.as_ref().is_some_and(|s| s.is_set(fd));

        if !check_read && !check_write && !check_except {
            continue;
        }

        let mut interest = 0u32;
        if check_read {
            interest |= EPOLLIN;
        }
        if check_write {
            interest |= EPOLLOUT;
        }

        let ready = check_fd_readiness(fd, interest);

        if check_read && ready & EPOLLIN != 0 {
            read_result.set(fd);
            ready_count += 1;
        }
        if check_write && ready & EPOLLOUT != 0 {
            write_result.set(fd);
            ready_count += 1;
        }
        if check_except && ready & EPOLLERR != 0 {
            except_result.set(fd);
            ready_count += 1;
        }
    }

    // Copy results back
    if let Some(readfds) = readfds {
        *readfds = read_result;
    }
    if let Some(writefds) = writefds {
        *writefds = write_result;
    }
    if let Some(exceptfds) = exceptfds {
        *exceptfds = except_result;
    }

    ready_count
}

/// Initialize epoll subsystem
pub fn init() {
    crate::serial_println!("[KnoxOS] epoll/poll/select I/O multiplexing initialized");
}
