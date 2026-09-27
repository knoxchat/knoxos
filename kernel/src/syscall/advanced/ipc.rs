use crate::serial_println;
/// SysV sem/msg, POSIX mq timed ops, keyring
use crate::syscall::{SyscallError, SyscallResult, read_user_string};
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

// ── SysV IPC State ──────────────────────────────────────────────────

/// SysV Semaphore set
struct SysVSemSet {
    key: u32,
    id: i32,
    values: Vec<i32>,
    mode: i32,
}

/// SysV Message queue
struct SysVMsgQueue {
    key: u32,
    id: i32,
    messages: alloc::collections::VecDeque<SysVMsg>,
    mode: i32,
    max_bytes: usize,
    current_bytes: usize,
}

struct SysVMsg {
    mtype: i64,
    data: Vec<u8>,
}

lazy_static::lazy_static! {
    static ref SYSV_SEMS: Mutex<BTreeMap<i32, SysVSemSet>> = Mutex::new(BTreeMap::new());
    static ref SYSV_MSGS: Mutex<BTreeMap<i32, SysVMsgQueue>> = Mutex::new(BTreeMap::new());
}

static NEXT_SEM_ID: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(1);

static NEXT_MSG_ID: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(1);

lazy_static::lazy_static! {
    static ref KEYRING: Mutex<BTreeMap<i32, (alloc::string::String, Vec<u8>)>> = Mutex::new(BTreeMap::new());
}

static NEXT_KEY_ID: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(1);

pub fn sys_request_key(
    type_ptr: u64,
    desc_ptr: u64,
    _callout: u64,
    _keyring: i32,
) -> SyscallResult {
    let desc = unsafe { read_user_string(desc_ptr) }.unwrap_or_default();
    let _ = type_ptr;
    // Search for key by description
    let keys = KEYRING.lock();
    for (id, (name, _)) in keys.iter() {
        if *name == desc {
            return Ok(*id as u64);
        }
    }
    Err(SyscallError::NoKey)
}

pub fn sys_keyctl(operation: i32, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> SyscallResult {
    const KEYCTL_GET_KEYRING_ID: i32 = 0;
    const KEYCTL_REVOKE: i32 = 3;
    const KEYCTL_READ: i32 = 11;
    const KEYCTL_DESCRIBE: i32 = 6;

    match operation {
        KEYCTL_GET_KEYRING_ID => Ok(arg2), // Return the keyring serial
        KEYCTL_REVOKE => {
            KEYRING.lock().remove(&(arg2 as i32));
            Ok(0)
        }
        KEYCTL_READ => {
            let keys = KEYRING.lock();
            if let Some((_, payload)) = keys.get(&(arg2 as i32)) {
                let copy_len = payload.len().min(arg4 as usize);
                if arg3 != 0 && copy_len > 0 {
                    unsafe {
                        core::ptr::copy_nonoverlapping(payload.as_ptr(), arg3 as *mut u8, copy_len);
                    }
                }
                Ok(payload.len() as u64)
            } else {
                Err(SyscallError::NoKey)
            }
        }
        KEYCTL_DESCRIBE => {
            let keys = KEYRING.lock();
            if let Some((name, _)) = keys.get(&(arg2 as i32)) {
                let bytes = name.as_bytes();
                let copy_len = bytes.len().min(arg4 as usize);
                if arg3 != 0 && copy_len > 0 {
                    unsafe {
                        core::ptr::copy_nonoverlapping(bytes.as_ptr(), arg3 as *mut u8, copy_len);
                    }
                }
                Ok(bytes.len() as u64)
            } else {
                Err(SyscallError::NoKey)
            }
        }
        _ => {
            let _ = arg5;
            Ok(0)
        }
    }
}

pub fn sys_add_key(
    type_ptr: u64,
    desc_ptr: u64,
    payload: u64,
    plen: usize,
    _keyring: i32,
) -> SyscallResult {
    let _ = type_ptr;
    let desc = unsafe { read_user_string(desc_ptr) }.unwrap_or_default();
    let data = if payload != 0 && plen > 0 {
        unsafe { core::slice::from_raw_parts(payload as *const u8, plen) }.to_vec()
    } else {
        Vec::new()
    };
    let id = NEXT_KEY_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    KEYRING.lock().insert(id, (desc, data));
    Ok(id as u64)
}

pub fn sys_mq_timedreceive(
    mqd: i32,
    msg_ptr: u64,
    msg_len: usize,
    prio_ptr: u64,
    timeout_ptr: u64,
) -> SyscallResult {
    let _ = (prio_ptr, timeout_ptr);
    crate::syscall::io::sys_mq_receive(mqd, msg_ptr, msg_len)
}

pub fn sys_mq_timedsend(
    mqd: i32,
    msg_ptr: u64,
    msg_len: usize,
    prio: u32,
    timeout_ptr: u64,
) -> SyscallResult {
    let _ = timeout_ptr;
    crate::syscall::io::sys_mq_send(mqd, msg_ptr, msg_len, prio)
}

pub fn sys_mq_notify(mqd: i32, sevp: u64) -> SyscallResult {
    let _ = (mqd, sevp);
    Ok(0)
}

pub fn sys_mq_getsetattr(mqd: i32, newattr: u64, oldattr: u64) -> SyscallResult {
    let _ = (mqd, newattr);
    if oldattr != 0 {
        unsafe {
            core::ptr::write_bytes(oldattr as *mut u8, 0, 64);
        }
    }
    Ok(0)
}

pub fn sys_semget(key: u32, nsems: i32, semflg: i32) -> SyscallResult {
    let ipc_creat = 0o1000;
    let ipc_excl = 0o2000;
    let ipc_private = 0u32;

    let mut sems = SYSV_SEMS.lock();

    // IPC_PRIVATE always creates a new set
    if key == ipc_private || semflg & ipc_creat != 0 {
        // Check for existing key (unless IPC_PRIVATE)
        if key != ipc_private {
            if let Some(existing) = sems.values().find(|s| s.key == key) {
                if semflg & ipc_excl != 0 {
                    return Err(SyscallError::FileExists);
                }
                return Ok(existing.id as u64);
            }
        }
        let id = NEXT_SEM_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        let n = if nsems > 0 { nsems as usize } else { 1 };
        sems.insert(
            id,
            SysVSemSet {
                key,
                id,
                values: alloc::vec![0; n],
                mode: semflg & 0o777,
            },
        );
        serial_println!(
            "[KnoxOS] semget: created set {} (key={}, nsems={})",
            id,
            key,
            n
        );
        Ok(id as u64)
    } else {
        // Lookup existing
        sems.values()
            .find(|s| s.key == key)
            .map(|s| s.id as u64)
            .ok_or(SyscallError::FileNotFound)
    }
}

pub fn sys_semop(semid: i32, sops: u64, nsops: usize) -> SyscallResult {
    if sops == 0 || nsops == 0 {
        return Err(SyscallError::InvalidArgument);
    }
    // struct sembuf { u16 sem_num, i16 sem_op, i16 sem_flg }
    let mut sems = SYSV_SEMS.lock();
    let set = sems.get_mut(&semid).ok_or(SyscallError::InvalidArgument)?;

    for i in 0..nsops {
        let base = (sops as usize) + i * 6;
        let sem_num = unsafe { *(base as *const u16) } as usize;
        let sem_op = unsafe { *((base + 2) as *const i16) } as i32;
        // sem_flg at base+4 (IPC_NOWAIT etc.)

        if sem_num >= set.values.len() {
            return Err(SyscallError::InvalidArgument);
        }

        if sem_op > 0 {
            // Increment (V/signal)
            set.values[sem_num] += sem_op;
        } else if sem_op < 0 {
            // Decrement (P/wait)
            let needed = -sem_op;
            if set.values[sem_num] >= needed {
                set.values[sem_num] -= needed;
            } else {
                return Err(SyscallError::WouldBlock);
            }
        }
        // sem_op == 0: wait for zero (simplified: check if already zero)
    }
    Ok(0)
}

pub fn sys_semctl(semid: i32, semnum: i32, cmd: i32, arg: u64) -> SyscallResult {
    const IPC_RMID: i32 = 0;
    const IPC_SET: i32 = 1;
    const IPC_STAT: i32 = 2;
    const GETVAL: i32 = 12;
    const SETVAL: i32 = 16;
    const GETALL: i32 = 13;
    const SETALL: i32 = 17;

    match cmd {
        IPC_RMID => {
            SYSV_SEMS.lock().remove(&semid);
            Ok(0)
        }
        GETVAL => {
            let sems = SYSV_SEMS.lock();
            let set = sems.get(&semid).ok_or(SyscallError::InvalidArgument)?;
            let idx = semnum as usize;
            if idx >= set.values.len() {
                return Err(SyscallError::InvalidArgument);
            }
            Ok(set.values[idx] as u64)
        }
        SETVAL => {
            let mut sems = SYSV_SEMS.lock();
            let set = sems.get_mut(&semid).ok_or(SyscallError::InvalidArgument)?;
            let idx = semnum as usize;
            if idx >= set.values.len() {
                return Err(SyscallError::InvalidArgument);
            }
            set.values[idx] = arg as i32;
            Ok(0)
        }
        GETALL => {
            let sems = SYSV_SEMS.lock();
            let set = sems.get(&semid).ok_or(SyscallError::InvalidArgument)?;
            if arg != 0 {
                let dst = arg as *mut u16;
                for (i, val) in set.values.iter().enumerate() {
                    unsafe {
                        *dst.add(i) = *val as u16;
                    }
                }
            }
            Ok(0)
        }
        SETALL => {
            let mut sems = SYSV_SEMS.lock();
            let set = sems.get_mut(&semid).ok_or(SyscallError::InvalidArgument)?;
            if arg != 0 {
                let src = arg as *const u16;
                for (i, val) in set.values.iter_mut().enumerate() {
                    *val = unsafe { *src.add(i) } as i32;
                }
            }
            Ok(0)
        }
        IPC_STAT | IPC_SET => {
            // Accept stat/set as no-op for compatibility
            let _ = arg;
            Ok(0)
        }
        _ => Ok(0),
    }
}

pub fn sys_semtimedop(semid: i32, sops: u64, nsops: usize, _timeout: u64) -> SyscallResult {
    // Forward to semop (timeout handling simplified)
    sys_semop(semid, sops, nsops)
}

pub fn sys_msgget(key: u32, msgflg: i32) -> SyscallResult {
    let ipc_creat = 0o1000;
    let ipc_excl = 0o2000;
    let ipc_private = 0u32;

    let mut msgs = SYSV_MSGS.lock();

    if key == ipc_private || msgflg & ipc_creat != 0 {
        if key != ipc_private {
            if let Some(existing) = msgs.values().find(|m| m.key == key) {
                if msgflg & ipc_excl != 0 {
                    return Err(SyscallError::FileExists);
                }
                return Ok(existing.id as u64);
            }
        }
        let id = NEXT_MSG_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        msgs.insert(
            id,
            SysVMsgQueue {
                key,
                id,
                messages: alloc::collections::VecDeque::new(),
                mode: msgflg & 0o777,
                max_bytes: 16384,
                current_bytes: 0,
            },
        );
        serial_println!("[KnoxOS] msgget: created queue {} (key={})", id, key);
        Ok(id as u64)
    } else {
        msgs.values()
            .find(|m| m.key == key)
            .map(|m| m.id as u64)
            .ok_or(SyscallError::FileNotFound)
    }
}

pub fn sys_msgsnd(msqid: i32, msgp: u64, msgsz: usize, msgflg: i32) -> SyscallResult {
    if msgp == 0 {
        return Err(SyscallError::InvalidArgument);
    }
    // msgbuf: { long mtype; char mtext[msgsz]; }
    let mtype = unsafe { *(msgp as *const i64) };
    if mtype < 1 {
        return Err(SyscallError::InvalidArgument);
    }
    let data = if msgsz > 0 {
        unsafe { core::slice::from_raw_parts((msgp + 8) as *const u8, msgsz) }.to_vec()
    } else {
        Vec::new()
    };

    let mut msgs = SYSV_MSGS.lock();
    let queue = msgs.get_mut(&msqid).ok_or(SyscallError::InvalidArgument)?;

    // Check capacity
    if queue.current_bytes + msgsz > queue.max_bytes {
        if msgflg & 0x800 != 0 {
            // IPC_NOWAIT
            return Err(SyscallError::WouldBlock);
        }
        return Err(SyscallError::WouldBlock); // Simplified: no blocking
    }

    queue.current_bytes += msgsz;
    queue.messages.push_back(SysVMsg { mtype, data });
    Ok(0)
}

pub fn sys_msgrcv(msqid: i32, msgp: u64, msgsz: usize, msgtyp: i64, msgflg: i32) -> SyscallResult {
    if msgp == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    let mut msgs = SYSV_MSGS.lock();
    let queue = msgs.get_mut(&msqid).ok_or(SyscallError::InvalidArgument)?;

    // Find matching message
    let pos = if msgtyp == 0 {
        // Any message type
        if queue.messages.is_empty() {
            None
        } else {
            Some(0)
        }
    } else if msgtyp > 0 {
        // Exact type match
        queue.messages.iter().position(|m| m.mtype == msgtyp)
    } else {
        // Lowest type <= |msgtyp|
        let abs_typ = -msgtyp;
        queue
            .messages
            .iter()
            .enumerate()
            .filter(|(_, m)| m.mtype <= abs_typ)
            .min_by_key(|(_, m)| m.mtype)
            .map(|(i, _)| i)
    };

    if let Some(idx) = pos {
        let msg = queue.messages.remove(idx).unwrap();
        queue.current_bytes = queue.current_bytes.saturating_sub(msg.data.len());

        let copy_len = msg.data.len().min(msgsz);
        if msg.data.len() > msgsz && msgflg & 0x1000 == 0 {
            // MSG_NOERROR
            return Err(SyscallError::InvalidArgument); // E2BIG
        }

        unsafe {
            *(msgp as *mut i64) = msg.mtype;
            if copy_len > 0 {
                core::ptr::copy_nonoverlapping(msg.data.as_ptr(), (msgp + 8) as *mut u8, copy_len);
            }
        }
        Ok(copy_len as u64)
    } else {
        if msgflg & 0x800 != 0 {
            // IPC_NOWAIT
            return Err(SyscallError::NoMessage);
        }
        Err(SyscallError::WouldBlock) // Simplified: no blocking
    }
}

pub fn sys_msgctl(msqid: i32, cmd: i32, buf: u64) -> SyscallResult {
    const IPC_RMID: i32 = 0;
    const IPC_STAT: i32 = 2;
    const IPC_SET: i32 = 1;

    match cmd {
        IPC_RMID => {
            SYSV_MSGS.lock().remove(&msqid);
            Ok(0)
        }
        IPC_STAT => {
            let msgs = SYSV_MSGS.lock();
            if let Some(queue) = msgs.get(&msqid) {
                if buf != 0 {
                    // Write msqid_ds structure (simplified: write msg count and bytes)
                    unsafe {
                        core::ptr::write_bytes(buf as *mut u8, 0, 120);
                        // msg_qnum at offset 64
                        *((buf + 64) as *mut u64) = queue.messages.len() as u64;
                        // msg_qbytes at offset 72
                        *((buf + 72) as *mut u64) = queue.max_bytes as u64;
                    }
                }
                Ok(0)
            } else {
                Err(SyscallError::InvalidArgument)
            }
        }
        IPC_SET => {
            // Accept set as no-op
            Ok(0)
        }
        _ => Ok(0),
    }
}
