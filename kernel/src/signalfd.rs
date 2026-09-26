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
    owner_pid: u32,
    pending: Vec<SignalFdSiginfo>,
}

impl SignalFdInstance {
    fn new(mask: u64, flags: i32, owner_pid: u32) -> Self {
        Self {
            mask,
            flags,
            owner_pid,
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
    let owner_pid = crate::scheduler::current_pid().unwrap_or(0);
    SIGNAL_FDS
        .lock()
        .insert(fd, SignalFdInstance::new(mask, flags, owner_pid));
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

/// Copy a pending `signalfd_siginfo` into `buf`.
pub fn signalfd_read_bytes(id: i32, buf: &mut [u8]) -> Result<usize, i32> {
    let info = signalfd_read(id)?;
    let bytes = unsafe {
        core::slice::from_raw_parts(
            (&info as *const SignalFdSiginfo).cast::<u8>(),
            core::mem::size_of::<SignalFdSiginfo>(),
        )
    };
    let n = buf.len().min(bytes.len());
    buf[..n].copy_from_slice(&bytes[..n]);
    Ok(n)
}

/// Queue `signo` on signalfds owned by `target_pid`. Returns true if any matched.
pub fn deliver_to_pid(target_pid: u32, signo: u32, sender_pid: u32) -> bool {
    let mut fds = SIGNAL_FDS.lock();
    let sig_bit = 1u64 << signo;
    let mut delivered = false;
    for sfd in fds.values_mut() {
        if sfd.owner_pid == target_pid && sfd.mask & sig_bit != 0 {
            let mut info = SignalFdSiginfo::zero();
            info.ssi_signo = signo;
            info.ssi_pid = sender_pid;
            sfd.pending.push(info);
            delivered = true;
        }
    }
    delivered
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

/// Whether a signalfd has a queued signal.
pub fn signalfd_would_read(fd: i32) -> bool {
    SIGNAL_FDS
        .lock()
        .get(&fd)
        .map(|s| !s.pending.is_empty())
        .unwrap_or(false)
}

/// Create, inject SIGUSR1, and read the signo back.
pub fn signalfd_roundtrip_self_test() -> bool {
    const SIGUSR1: u32 = 10;
    let owner = crate::scheduler::current_pid().unwrap_or(0);
    let Ok(id) = signalfd_create(1u64 << SIGUSR1, 0) else {
        return false;
    };
    if !deliver_to_pid(owner, SIGUSR1, 1) {
        signalfd_close(id);
        return false;
    }
    let got = signalfd_read(id);
    signalfd_close(id);
    matches!(got, Ok(info) if info.ssi_signo == SIGUSR1)
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
