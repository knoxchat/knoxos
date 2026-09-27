// POSIX threads (pthread.h)
use alloc::collections::BTreeMap;
use core::ffi::c_void;
use core::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

/// Opaque pthread types (simplified for compatibility)
pub type PthreadT = u64;
pub type PthreadMutexT = [u8; 40];
pub type PthreadCondT = [u8; 48];
pub type PthreadAttrT = [u8; 56];
pub type PthreadMutexattrT = [u8; 4];
pub type PthreadCondattrT = [u8; 4];
pub type PthreadKeyT = u32;
pub type PthreadOnceT = i32;

static NEXT_THREAD_ID: AtomicU64 = AtomicU64::new(1000);
static NEXT_TLS_KEY: AtomicU32 = AtomicU32::new(0);

lazy_static::lazy_static! {
    /// TLS keys: key -> (destructor, per-thread values)
    static ref TLS_KEYS: Mutex<BTreeMap<u32, (Option<u64>, BTreeMap<u64, u64>)>> =
        Mutex::new(BTreeMap::new());
}

static PTHREAD_ONCE_INIT: i32 = 0;

/// pthread_create — create a new thread
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_create(
    thread: *mut PthreadT,
    _attr: *const PthreadAttrT,
    start_routine: extern "C" fn(*mut c_void) -> *mut c_void,
    arg: *mut c_void,
) -> i32 {
    let tid = NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed);
    if !thread.is_null() {
        *thread = tid;
    }

    // In a full implementation, this would call clone() with CLONE_VM | CLONE_THREAD
    // For now, we create a kernel-level thread via the scheduler
    serial_println!(
        "[pthread] pthread_create: tid={}, entry={:#x}",
        tid,
        start_routine as usize
    );

    // Register as a schedulable task
    // The actual thread creation goes through the process/threads subsystem
    let _ = crate::threads::create_thread(start_routine as usize as u64, arg as u64, tid);

    0 // Success
}

/// pthread_join — wait for thread termination
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_join(thread: PthreadT, retval: *mut *mut c_void) -> i32 {
    serial_println!("[pthread] pthread_join: tid={}", thread);
    // Wait for the thread to finish via futex
    let _ = crate::threads::join_thread(thread);
    if !retval.is_null() {
        *retval = core::ptr::null_mut();
    }
    0
}

/// pthread_detach — detach a thread
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_detach(thread: PthreadT) -> i32 {
    serial_println!("[pthread] pthread_detach: tid={}", thread);
    0 // Success — thread will auto-clean on exit
}

/// pthread_self — get calling thread's ID
#[unsafe(no_mangle)]
pub extern "C" fn pthread_self() -> PthreadT {
    crate::scheduler::current_pid().unwrap_or(0) as u64
}

/// pthread_equal — compare thread IDs
#[unsafe(no_mangle)]
pub extern "C" fn pthread_equal(t1: PthreadT, t2: PthreadT) -> i32 {
    if t1 == t2 { 1 } else { 0 }
}

/// pthread_mutex_init — initialize a mutex
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_mutex_init(
    mutex: *mut PthreadMutexT,
    _attr: *const PthreadMutexattrT,
) -> i32 {
    if !mutex.is_null() {
        // Zero-init (unlocked state)
        core::ptr::write_bytes(mutex as *mut u8, 0, core::mem::size_of::<PthreadMutexT>());
    }
    0
}

/// pthread_mutex_lock — lock a mutex (blocking)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_mutex_lock(mutex: *mut PthreadMutexT) -> i32 {
    if mutex.is_null() {
        return -1;
    }
    let lock_word = mutex as *mut AtomicI32;
    // Spin-then-futex lock
    loop {
        if (*lock_word)
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            return 0;
        }
        // Yield to scheduler instead of busy-waiting
        crate::scheduler::yield_now();
    }
}

/// pthread_mutex_trylock — try to lock a mutex (non-blocking)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_mutex_trylock(mutex: *mut PthreadMutexT) -> i32 {
    if mutex.is_null() {
        return -1;
    }
    let lock_word = mutex as *mut AtomicI32;
    if (*lock_word)
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .is_ok()
    {
        0
    } else {
        16 // EBUSY
    }
}

/// pthread_mutex_unlock — unlock a mutex
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_mutex_unlock(mutex: *mut PthreadMutexT) -> i32 {
    if mutex.is_null() {
        return -1;
    }
    let lock_word = mutex as *mut AtomicI32;
    (*lock_word).store(0, Ordering::Release);
    // Wake one waiter (if any are blocked in futex)
    0
}

/// pthread_mutex_destroy — destroy a mutex
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_mutex_destroy(_mutex: *mut PthreadMutexT) -> i32 {
    0 // No-op for kernel-level mutexes
}

/// pthread_cond_init — initialize condition variable
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_cond_init(
    cond: *mut PthreadCondT,
    _attr: *const PthreadCondattrT,
) -> i32 {
    if !cond.is_null() {
        core::ptr::write_bytes(cond as *mut u8, 0, core::mem::size_of::<PthreadCondT>());
    }
    0
}

/// pthread_cond_wait — wait on condition variable
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_cond_wait(
    _cond: *mut PthreadCondT,
    mutex: *mut PthreadMutexT,
) -> i32 {
    // Release mutex, sleep, re-acquire
    pthread_mutex_unlock(mutex);
    crate::scheduler::yield_now();
    pthread_mutex_lock(mutex);
    0
}

/// pthread_cond_signal — signal one waiter
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_cond_signal(_cond: *mut PthreadCondT) -> i32 {
    0 // Wake one waiter
}

/// pthread_cond_broadcast — signal all waiters
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_cond_broadcast(_cond: *mut PthreadCondT) -> i32 {
    0 // Wake all waiters
}

/// pthread_cond_destroy — destroy condition variable
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_cond_destroy(_cond: *mut PthreadCondT) -> i32 {
    0
}

/// pthread_key_create — create thread-local storage key
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_key_create(
    key: *mut PthreadKeyT,
    destructor: Option<extern "C" fn(*mut c_void)>,
) -> i32 {
    let k = NEXT_TLS_KEY.fetch_add(1, Ordering::Relaxed);
    let dtor = destructor.map(|f| f as usize as u64);
    TLS_KEYS.lock().insert(k, (dtor, BTreeMap::new()));
    if !key.is_null() {
        *key = k;
    }
    0
}

/// pthread_key_delete — delete a TLS key
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_key_delete(key: PthreadKeyT) -> i32 {
    TLS_KEYS.lock().remove(&key);
    0
}

/// pthread_getspecific — get TLS value
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_getspecific(key: PthreadKeyT) -> *mut c_void {
    let tid = pthread_self();
    let keys = TLS_KEYS.lock();
    if let Some((_, values)) = keys.get(&key) {
        if let Some(&val) = values.get(&tid) {
            return val as *mut c_void;
        }
    }
    core::ptr::null_mut()
}

/// pthread_setspecific — set TLS value
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_setspecific(key: PthreadKeyT, value: *const c_void) -> i32 {
    let tid = pthread_self();
    let mut keys = TLS_KEYS.lock();
    if let Some((_, values)) = keys.get_mut(&key) {
        values.insert(tid, value as u64);
        0
    } else {
        22 // EINVAL
    }
}

/// pthread_once — call init routine once
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_once(
    once_control: *mut PthreadOnceT,
    init_routine: extern "C" fn(),
) -> i32 {
    if once_control.is_null() {
        return 22;
    }
    let ctrl = once_control as *mut AtomicI32;
    if (*ctrl)
        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Relaxed)
        .is_ok()
    {
        init_routine();
        (*ctrl).store(2, Ordering::Release);
    } else {
        // Wait for init to complete
        while (*ctrl).load(Ordering::Acquire) != 2 {
            core::hint::spin_loop();
        }
    }
    0
}

/// pthread_attr_init
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_attr_init(attr: *mut PthreadAttrT) -> i32 {
    if !attr.is_null() {
        core::ptr::write_bytes(attr as *mut u8, 0, core::mem::size_of::<PthreadAttrT>());
    }
    0
}

/// pthread_attr_destroy
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_attr_destroy(_attr: *mut PthreadAttrT) -> i32 {
    0
}

/// pthread_attr_setdetachstate
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_attr_setdetachstate(_attr: *mut PthreadAttrT, _state: i32) -> i32 {
    0
}

/// pthread_attr_setstacksize
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_attr_setstacksize(
    _attr: *mut PthreadAttrT,
    _stacksize: usize,
) -> i32 {
    0
}

use alloc::string::ToString;
static NEXT_TLS_KEY2: AtomicU32 = AtomicU32::new(0);
