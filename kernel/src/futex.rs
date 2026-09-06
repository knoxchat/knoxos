/// futex — Enhanced futex (Fast Userspace muTEX) implementation
/// Linux-compatible futex operations beyond the basic stub
///
/// Operations: FUTEX_WAIT, FUTEX_WAKE, FUTEX_REQUEUE, FUTEX_CMP_REQUEUE,
///             FUTEX_WAKE_OP, FUTEX_WAIT_BITSET, FUTEX_WAKE_BITSET
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Futex operation constants (matching Linux)
pub const FUTEX_WAIT: i32 = 0;
pub const FUTEX_WAKE: i32 = 1;
pub const FUTEX_FD: i32 = 2;
pub const FUTEX_REQUEUE: i32 = 3;
pub const FUTEX_CMP_REQUEUE: i32 = 4;
pub const FUTEX_WAKE_OP: i32 = 5;
pub const FUTEX_LOCK_PI: i32 = 6;
pub const FUTEX_UNLOCK_PI: i32 = 7;
pub const FUTEX_TRYLOCK_PI: i32 = 8;
pub const FUTEX_WAIT_BITSET: i32 = 9;
pub const FUTEX_WAKE_BITSET: i32 = 10;
pub const FUTEX_PRIVATE_FLAG: i32 = 128;
pub const FUTEX_CLOCK_REALTIME: i32 = 256;
pub const FUTEX_CMD_MASK: i32 = !(FUTEX_PRIVATE_FLAG | FUTEX_CLOCK_REALTIME);

/// Represents a thread waiting on a futex
#[derive(Debug, Clone)]
struct FutexWaiter {
    pid: u32,
    tid: u32,
    bitset: u32,
}

/// Global futex wait queues keyed by physical address
lazy_static::lazy_static! {
    static ref FUTEX_QUEUES: Mutex<BTreeMap<u64, Vec<FutexWaiter>>> = Mutex::new(BTreeMap::new());
}

/// Perform a futex operation
pub fn futex_op(
    addr: u64,
    op: i32,
    val: u32,
    timeout: u64,
    addr2: u64,
    val3: u32,
) -> Result<i32, i32> {
    let cmd = op & FUTEX_CMD_MASK;

    match cmd {
        FUTEX_WAIT => futex_wait(addr, val, timeout, !0u32),
        FUTEX_WAKE => futex_wake(addr, val as i32, !0u32),
        FUTEX_REQUEUE => futex_requeue(addr, val as i32, addr2, i32::MAX),
        FUTEX_CMP_REQUEUE => futex_cmp_requeue(addr, val as i32, addr2, val3, i32::MAX),
        FUTEX_WAIT_BITSET => futex_wait(addr, val, timeout, val3),
        FUTEX_WAKE_BITSET => futex_wake(addr, val as i32, val3),
        FUTEX_WAKE_OP => futex_wake_op(addr, val as i32, addr2, val3),
        FUTEX_LOCK_PI => futex_lock_pi(addr, timeout),
        FUTEX_UNLOCK_PI => futex_unlock_pi(addr),
        _ => Err(-38), // ENOSYS
    }
}

/// FUTEX_WAIT — atomically check *addr == val, then sleep
fn futex_wait(addr: u64, expected: u32, timeout: u64, bitset: u32) -> Result<i32, i32> {
    if bitset == 0 {
        return Err(-22); // EINVAL
    }

    // Read the current value
    let current = unsafe { *(addr as *const u32) };
    if current != expected {
        return Err(-11); // EAGAIN — value changed
    }

    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let tid = pid; // In single-threaded, tid == pid

    let waiter = FutexWaiter { pid, tid, bitset };

    // Add to wait queue
    let mut queues = FUTEX_QUEUES.lock();
    queues.entry(addr).or_default().push(waiter);
    drop(queues);

    // Put the current process to sleep
    crate::scheduler::sleep_current();

    Ok(0)
}

/// FUTEX_WAKE — wake at most `count` waiters
fn futex_wake(addr: u64, count: i32, bitset: u32) -> Result<i32, i32> {
    if bitset == 0 {
        return Err(-22); // EINVAL
    }

    let mut queues = FUTEX_QUEUES.lock();
    let waiters = match queues.get_mut(&addr) {
        Some(w) => w,
        None => return Ok(0),
    };

    let mut woken = 0;
    let mut i = 0;
    while i < waiters.len() && woken < count {
        if waiters[i].bitset & bitset != 0 {
            let waiter = waiters.remove(i);
            crate::scheduler::wake_process(waiter.pid);
            woken += 1;
        } else {
            i += 1;
        }
    }

    if waiters.is_empty() {
        queues.remove(&addr);
    }

    Ok(woken)
}

/// FUTEX_REQUEUE — wake `wake_count` waiters, move rest to addr2
fn futex_requeue(addr: u64, wake_count: i32, addr2: u64, requeue_count: i32) -> Result<i32, i32> {
    let mut queues = FUTEX_QUEUES.lock();
    let waiters = match queues.get_mut(&addr) {
        Some(w) => w,
        None => return Ok(0),
    };

    let mut woken = 0;
    let mut requeued = 0;
    let mut to_requeue = Vec::new();

    let mut i = 0;
    while i < waiters.len() {
        if woken < wake_count {
            let waiter = waiters.remove(i);
            crate::scheduler::wake_process(waiter.pid);
            woken += 1;
        } else if requeued < requeue_count {
            to_requeue.push(waiters.remove(i));
            requeued += 1;
        } else {
            i += 1;
        }
    }

    if waiters.is_empty() {
        queues.remove(&addr);
    }

    // Move requeued waiters to addr2
    if !to_requeue.is_empty() {
        let queue2 = queues.entry(addr2).or_default();
        queue2.extend(to_requeue);
    }

    Ok(woken + requeued)
}

/// FUTEX_CMP_REQUEUE — like REQUEUE but check *addr == expected first
fn futex_cmp_requeue(
    addr: u64,
    wake_count: i32,
    addr2: u64,
    expected: u32,
    requeue_count: i32,
) -> Result<i32, i32> {
    let current = unsafe { *(addr as *const u32) };
    if current != expected {
        return Err(-11); // EAGAIN
    }
    futex_requeue(addr, wake_count, addr2, requeue_count)
}

/// FUTEX_WAKE_OP — wake on two addresses with atomic op
fn futex_wake_op(addr1: u64, wake1_count: i32, addr2: u64, op_arg: u32) -> Result<i32, i32> {
    // Decode op: oparg encodes (op, cmp, oparg, cmparg)
    let _op = (op_arg >> 28) & 0xf;
    let _cmp = (op_arg >> 24) & 0xf;
    let _oparg_val = (op_arg >> 12) & 0xfff;
    let _cmparg = op_arg & 0xfff;

    // Perform atomic operation on *addr2
    let oldval = unsafe { *(addr2 as *const u32) };
    // Simplified: just set to oparg value
    unsafe { *(addr2 as *mut u32) = _oparg_val };

    // Wake on addr1
    let woken1 = futex_wake(addr1, wake1_count, !0u32)?;

    // Check condition and maybe wake on addr2
    let cond_met = match _cmp {
        0 => oldval == _cmparg, // FUTEX_OP_CMP_EQ
        1 => oldval != _cmparg, // FUTEX_OP_CMP_NE
        2 => oldval < _cmparg,  // FUTEX_OP_CMP_LT
        3 => oldval <= _cmparg, // FUTEX_OP_CMP_LE
        4 => oldval > _cmparg,  // FUTEX_OP_CMP_GT
        5 => oldval >= _cmparg, // FUTEX_OP_CMP_GE
        _ => false,
    };

    let woken2 = if cond_met {
        futex_wake(addr2, 1, !0u32)?
    } else {
        0
    };

    Ok(woken1 + woken2)
}

/// FUTEX_LOCK_PI — priority-inheritance lock
fn futex_lock_pi(addr: u64, _timeout: u64) -> Result<i32, i32> {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let current = unsafe { *(addr as *const u32) };

    if current == 0 {
        // Lock is free, take it
        unsafe { *(addr as *mut u32) = pid };
        Ok(0)
    } else {
        // Lock is held, wait
        futex_wait(addr, current, _timeout, !0u32)
    }
}

/// FUTEX_UNLOCK_PI — release priority-inheritance lock
fn futex_unlock_pi(addr: u64) -> Result<i32, i32> {
    unsafe { *(addr as *mut u32) = 0 };
    futex_wake(addr, 1, !0u32)
}

/// Get number of waiters on a given address (for debugging)
pub fn waiter_count(addr: u64) -> usize {
    let queues = FUTEX_QUEUES.lock();
    queues.get(&addr).map(|w| w.len()).unwrap_or(0)
}

pub fn init() {
    serial_println!("[KnoxOS] Enhanced futex subsystem initialized");
}
