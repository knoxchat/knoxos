/// signalfd - Signal file descriptors
/// Compatible with Linux signalfd(2)
/// Receives signals via file descriptor reads
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

/// signalfd flags
pub const SFD_CLOEXEC: i32 = 0x00080000;
pub const SFD_NONBLOCK: i32 = 0x00000800;

/// signalfd_siginfo structure (matches Linux layout, 128 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SignalFdSiginfo {
    pub ssi_signo: u32,
    pub ssi_errno: i32,
    pub ssi_code: i32,
    pub ssi_pid: u32,
    pub ssi_uid: u32,
    pub ssi_fd: i32,
    pub ssi_tid: u32,
    pub ssi_band: u32,
    pub ssi_overrun: u32,
    pub ssi_trapno: u32,
    pub ssi_status: i32,
    pub ssi_int: i32,
    pub ssi_ptr: u64,
    pub ssi_utime: u64,
    pub ssi_stime: u64,
    pub ssi_addr: u64,
    pub _pad: [u8; 48],
}

impl SignalFdSiginfo {
    pub fn zero() -> Self {
        Self {
            ssi_signo: 0,
            ssi_errno: 0,
            ssi_code: 0,
            ssi_pid: 0,
            ssi_uid: 0,
            ssi_fd: 0,
            ssi_tid: 0,
            ssi_band: 0,
            ssi_overrun: 0,
            ssi_trapno: 0,
            ssi_status: 0,
            ssi_int: 0,
            ssi_ptr: 0,
            ssi_utime: 0,
            ssi_stime: 0,
            ssi_addr: 0,
            _pad: [0; 48],
        }
    }
}

/// A signalfd instance
struct SignalFdInstance {
    mask: u64,
    flags: i32,
    pending: Vec<SignalFdSiginfo>,
}

impl SignalFdInstance {
    fn new(mask: u64, flags: i32) -> Self {
        Self {
            mask,
            flags,
            pending: Vec::new(),
        }
    }

    fn read(&mut self) -> Result<SignalFdSiginfo, i32> {
        if let Some(info) = self.pending.pop() {
            Ok(info)
        } else {
            Err(-11) // EAGAIN
        }
    }
}

/// Global signalfd table
lazy_static::lazy_static! {
    static ref SIGNAL_FDS: Mutex<BTreeMap<i32, SignalFdInstance>> = Mutex::new(BTreeMap::new());
}

static NEXT_SIGNAL_FD: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(5000);

/// Create a new signalfd
pub fn signalfd_create(mask: u64, flags: i32) -> Result<i32, i32> {
    let fd = NEXT_SIGNAL_FD.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    SIGNAL_FDS
        .lock()
        .insert(fd, SignalFdInstance::new(mask, flags));
    crate::serial_println!("[KnoxOS] signalfd(mask={:#x}) = {}", mask, fd);
    Ok(fd)
}

/// Update the signal mask for an existing signalfd
pub fn signalfd_update(fd: i32, mask: u64) -> Result<(), i32> {
    let mut fds = SIGNAL_FDS.lock();
    let sfd = fds.get_mut(&fd).ok_or(-9i32)?;
    sfd.mask = mask;
    Ok(())
}

/// Read a signal from a signalfd
pub fn signalfd_read(fd: i32) -> Result<SignalFdSiginfo, i32> {
    let mut fds = SIGNAL_FDS.lock();
    let sfd = fds.get_mut(&fd).ok_or(-9i32)?;
    sfd.read()
}

/// Deliver a signal to matching signalfds
pub fn deliver_to_signalfds(signo: u32, sender_pid: u32) {
    let mut fds = SIGNAL_FDS.lock();
    let sig_bit = 1u64 << signo;
    for sfd in fds.values_mut() {
        if sfd.mask & sig_bit != 0 {
            let mut info = SignalFdSiginfo::zero();
            info.ssi_signo = signo;
            info.ssi_pid = sender_pid;
            sfd.pending.push(info);
        }
    }
}

/// Close a signalfd
pub fn signalfd_close(fd: i32) {
    SIGNAL_FDS.lock().remove(&fd);
}

/// Check if fd is a signalfd
pub fn is_signalfd(fd: i32) -> bool {
    SIGNAL_FDS.lock().contains_key(&fd)
}

/// Initialize signalfd subsystem
pub fn init() {
    crate::serial_println!("[KnoxOS] signalfd signal file descriptors initialized");
}
