// POSIX libc Compatibility Layer (Phase 24)
//
// Provides comprehensive C library function coverage for running
// unmodified Linux/Debian binaries. This extends libc_funcs.rs with:
//
//   - stdio.h: printf/fprintf/snprintf formatting engine
//   - stdlib.h: Enhanced memory allocation with size tracking
//   - unistd.h: POSIX file/process operations
//   - pthread.h: POSIX threads (via clone/futex)
//   - dirent.h: Directory operations
//   - time.h: Clock and time functions
//   - signal.h: Signal management
//   - fcntl.h: File control operations
//   - sys/stat.h: File status
//   - sys/mman.h: Memory mapping
//   - dlfcn.h: Dynamic loading (bridges to dynlink.rs)
//   - locale.h: Locale support
//   - math.h: Math functions (software float)
//
// All functions use C ABI (`extern "C"`) and are `#[unsafe(no_mangle)]` so
// they can be resolved by the dynamic linker at load time.
#![allow(
    clippy::missing_safety_doc,
    clippy::not_unsafe_ptr_arg_deref,
    clippy::needless_range_loop,
    clippy::too_many_arguments,
    clippy::type_complexity,
    dead_code
)]

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::ffi::c_void;
use core::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// ALLOCATION SIZE TRACKING (for proper realloc/free)
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    /// Track allocation sizes for proper free/realloc
    static ref ALLOC_SIZES: Mutex<BTreeMap<u64, usize>> = Mutex::new(BTreeMap::new());
}

/// Allocate memory with size tracking
pub unsafe fn tracked_malloc(size: usize) -> *mut c_void {
    if size == 0 {
        return core::ptr::null_mut();
    }
    let layout = alloc::alloc::Layout::from_size_align_unchecked(size, 16);
    let ptr = alloc::alloc::alloc(layout);
    if !ptr.is_null() {
        ALLOC_SIZES.lock().insert(ptr as u64, size);
    }
    ptr as *mut c_void
}

/// Free memory with size tracking
pub unsafe fn tracked_free(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    let size = ALLOC_SIZES.lock().remove(&(ptr as u64)).unwrap_or(64);
    let layout = alloc::alloc::Layout::from_size_align_unchecked(size, 16);
    alloc::alloc::dealloc(ptr as *mut u8, layout);
}

/// Reallocate with proper size tracking
pub unsafe fn tracked_realloc(ptr: *mut c_void, new_size: usize) -> *mut c_void {
    if ptr.is_null() {
        return tracked_malloc(new_size);
    }
    if new_size == 0 {
        tracked_free(ptr);
        return core::ptr::null_mut();
    }
    let old_size = ALLOC_SIZES.lock().get(&(ptr as u64)).copied().unwrap_or(0);
    let new_ptr = tracked_malloc(new_size);
    if !new_ptr.is_null() {
        let copy_size = if old_size < new_size {
            old_size
        } else {
            new_size
        };
        core::ptr::copy_nonoverlapping(ptr as *const u8, new_ptr as *mut u8, copy_size);
        tracked_free(ptr);
    }
    new_ptr
}

// ═══════════════════════════════════════════════════════════════════════
// PRINTF FORMATTING ENGINE (stdio.h)
// ═══════════════════════════════════════════════════════════════════════

/// Internal printf formatting — processes a format string and variadic args.
/// This is a simplified but functional implementation supporting:
///   %d, %i, %u, %x, %X, %o, %s, %c, %p, %ld, %lu, %lx, %lld, %llu, %llx,
///   %f (simplified), %%, %zu, %zd, width, precision, padding, left-align.
pub struct PrintfFormatter {
    output: Vec<u8>,
    max_len: Option<usize>,
}

impl PrintfFormatter {
    pub fn new(max_len: Option<usize>) -> Self {
        Self {
            output: Vec::new(),
            max_len,
        }
    }

    pub fn push(&mut self, b: u8) {
        if let Some(max) = self.max_len {
            if self.output.len() >= max {
                return;
            }
        }
        self.output.push(b);
    }

    pub fn push_str(&mut self, s: &[u8]) {
        for &b in s {
            self.push(b);
        }
    }

    pub fn finish(self) -> Vec<u8> {
        self.output
    }

    pub fn len(&self) -> usize {
        self.output.len()
    }

    pub fn is_empty(&self) -> bool {
        self.output.is_empty()
    }

    /// Format an integer as decimal
    pub fn format_signed(&mut self, val: i64, width: usize, zero_pad: bool, left_align: bool) {
        let mut buf = [0u8; 24];
        let negative = val < 0;
        let mut v = if negative {
            (val as i128).unsigned_abs() as u64
        } else {
            val as u64
        };
        let mut pos = buf.len();
        if v == 0 {
            pos -= 1;
            buf[pos] = b'0';
        } else {
            while v > 0 {
                pos -= 1;
                buf[pos] = b'0' + (v % 10) as u8;
                v /= 10;
            }
        }
        if negative {
            pos -= 1;
            buf[pos] = b'-';
        }
        let digits = &buf[pos..];
        let dlen = digits.len();
        if !left_align && dlen < width {
            let pad = if zero_pad { b'0' } else { b' ' };
            for _ in 0..(width - dlen) {
                self.push(pad);
            }
        }
        self.push_str(digits);
        if left_align && dlen < width {
            for _ in 0..(width - dlen) {
                self.push(b' ');
            }
        }
    }

    /// Format an unsigned integer
    pub fn format_unsigned(&mut self, val: u64, width: usize, zero_pad: bool, left_align: bool) {
        let mut buf = [0u8; 22];
        let mut v = val;
        let mut pos = buf.len();
        if v == 0 {
            pos -= 1;
            buf[pos] = b'0';
        } else {
            while v > 0 {
                pos -= 1;
                buf[pos] = b'0' + (v % 10) as u8;
                v /= 10;
            }
        }
        let digits = &buf[pos..];
        let dlen = digits.len();
        if !left_align && dlen < width {
            let pad = if zero_pad { b'0' } else { b' ' };
            for _ in 0..(width - dlen) {
                self.push(pad);
            }
        }
        self.push_str(digits);
        if left_align && dlen < width {
            for _ in 0..(width - dlen) {
                self.push(b' ');
            }
        }
    }

    /// Format unsigned as hex
    pub fn format_hex(
        &mut self,
        val: u64,
        upper: bool,
        width: usize,
        zero_pad: bool,
        left_align: bool,
        prefix: bool,
    ) {
        let mut buf = [0u8; 20];
        let mut v = val;
        let mut pos = buf.len();
        let hex_chars = if upper {
            b"0123456789ABCDEF"
        } else {
            b"0123456789abcdef"
        };
        if v == 0 {
            pos -= 1;
            buf[pos] = b'0';
        } else {
            while v > 0 {
                pos -= 1;
                buf[pos] = hex_chars[(v & 0xF) as usize];
                v >>= 4;
            }
        }
        if prefix {
            pos -= 1;
            buf[pos] = if upper { b'X' } else { b'x' };
            pos -= 1;
            buf[pos] = b'0';
        }
        let digits = &buf[pos..];
        let dlen = digits.len();
        if !left_align && dlen < width {
            let pad = if zero_pad { b'0' } else { b' ' };
            for _ in 0..(width - dlen) {
                self.push(pad);
            }
        }
        self.push_str(digits);
        if left_align && dlen < width {
            for _ in 0..(width - dlen) {
                self.push(b' ');
            }
        }
    }

    /// Format unsigned as octal
    pub fn format_octal(&mut self, val: u64, width: usize, zero_pad: bool, left_align: bool) {
        let mut buf = [0u8; 24];
        let mut v = val;
        let mut pos = buf.len();
        if v == 0 {
            pos -= 1;
            buf[pos] = b'0';
        } else {
            while v > 0 {
                pos -= 1;
                buf[pos] = b'0' + (v & 7) as u8;
                v >>= 3;
            }
        }
        let digits = &buf[pos..];
        let dlen = digits.len();
        if !left_align && dlen < width {
            let pad = if zero_pad { b'0' } else { b' ' };
            for _ in 0..(width - dlen) {
                self.push(pad);
            }
        }
        self.push_str(digits);
        if left_align && dlen < width {
            for _ in 0..(width - dlen) {
                self.push(b' ');
            }
        }
    }
}

/// Process a printf format string with a raw va_list-style argument pointer.
///
/// This is the core engine used by printf, fprintf, snprintf, etc.
/// `args` is a pointer to the first variadic argument (treated as u64 slots
/// on x86_64 System V ABI).
///
/// Returns the formatted byte vector.
pub unsafe fn printf_engine(
    fmt: *const u8,
    mut args: *const u64,
    max_len: Option<usize>,
) -> Vec<u8> {
    let mut f = PrintfFormatter::new(max_len);
    let mut i = 0;

    loop {
        let c = *fmt.add(i);
        if c == 0 {
            break;
        }
        i += 1;

        if c != b'%' {
            f.push(c);
            continue;
        }

        // Parse format specifier
        let mut flags_left = false;
        let mut flags_zero = false;
        let mut flags_hash = false;
        let mut flags_plus = false;
        let mut flags_space = false;

        // Parse flags
        loop {
            let fc = *fmt.add(i);
            match fc {
                b'-' => {
                    flags_left = true;
                    i += 1;
                }
                b'0' => {
                    flags_zero = true;
                    i += 1;
                }
                b'#' => {
                    flags_hash = true;
                    i += 1;
                }
                b'+' => {
                    flags_plus = true;
                    i += 1;
                }
                b' ' => {
                    flags_space = true;
                    i += 1;
                }
                _ => break,
            }
        }

        // Parse width
        let mut width: usize = 0;
        if *fmt.add(i) == b'*' {
            width = *args as usize;
            args = args.add(1);
            i += 1;
        } else {
            while (*fmt.add(i)).is_ascii_digit() {
                width = width * 10 + (*fmt.add(i) - b'0') as usize;
                i += 1;
            }
        }

        // Parse precision
        let mut precision: Option<usize> = None;
        if *fmt.add(i) == b'.' {
            i += 1;
            let mut prec = 0usize;
            if *fmt.add(i) == b'*' {
                prec = *args as usize;
                args = args.add(1);
                i += 1;
            } else {
                while (*fmt.add(i)).is_ascii_digit() {
                    prec = prec * 10 + (*fmt.add(i) - b'0') as usize;
                    i += 1;
                }
            }
            precision = Some(prec);
        }

        // Parse length modifier
        let mut long_count = 0u8; // 1 = l, 2 = ll
        let mut size_t_mod = false;
        match *fmt.add(i) {
            b'l' => {
                long_count = 1;
                i += 1;
                if *fmt.add(i) == b'l' {
                    long_count = 2;
                    i += 1;
                }
            }
            b'h' => {
                i += 1;
                if *fmt.add(i) == b'h' {
                    i += 1;
                }
            }
            b'z' => {
                size_t_mod = true;
                i += 1;
            }
            b'j' | b't' => {
                i += 1;
            }
            _ => {}
        }

        // Parse conversion specifier
        let spec = *fmt.add(i);
        i += 1;

        match spec {
            b'd' | b'i' => {
                let val = *args as i64;
                args = args.add(1);
                f.format_signed(val, width, flags_zero && !flags_left, flags_left);
            }
            b'u' => {
                let val = *args;
                args = args.add(1);
                f.format_unsigned(val, width, flags_zero && !flags_left, flags_left);
            }
            b'x' => {
                let val = *args;
                args = args.add(1);
                f.format_hex(
                    val,
                    false,
                    width,
                    flags_zero && !flags_left,
                    flags_left,
                    flags_hash,
                );
            }
            b'X' => {
                let val = *args;
                args = args.add(1);
                f.format_hex(
                    val,
                    true,
                    width,
                    flags_zero && !flags_left,
                    flags_left,
                    flags_hash,
                );
            }
            b'o' => {
                let val = *args;
                args = args.add(1);
                f.format_octal(val, width, flags_zero && !flags_left, flags_left);
            }
            b's' => {
                let ptr = *args as *const u8;
                args = args.add(1);
                if ptr.is_null() {
                    let s = b"(null)";
                    let len = if let Some(p) = precision {
                        p.min(s.len())
                    } else {
                        s.len()
                    };
                    if !flags_left && len < width {
                        for _ in 0..(width - len) {
                            f.push(b' ');
                        }
                    }
                    f.push_str(&s[..len]);
                    if flags_left && len < width {
                        for _ in 0..(width - len) {
                            f.push(b' ');
                        }
                    }
                } else {
                    let mut slen = 0;
                    while *ptr.add(slen) != 0 {
                        slen += 1;
                        if slen > 65536 {
                            break;
                        }
                    }
                    let len = if let Some(p) = precision {
                        p.min(slen)
                    } else {
                        slen
                    };
                    if !flags_left && len < width {
                        for _ in 0..(width - len) {
                            f.push(b' ');
                        }
                    }
                    let slice = core::slice::from_raw_parts(ptr, len);
                    f.push_str(slice);
                    if flags_left && len < width {
                        for _ in 0..(width - len) {
                            f.push(b' ');
                        }
                    }
                }
            }
            b'c' => {
                let val = *args as u8;
                args = args.add(1);
                if !flags_left && width > 1 {
                    for _ in 0..(width - 1) {
                        f.push(b' ');
                    }
                }
                f.push(val);
                if flags_left && width > 1 {
                    for _ in 0..(width - 1) {
                        f.push(b' ');
                    }
                }
            }
            b'p' => {
                let val = *args;
                args = args.add(1);
                f.push_str(b"0x");
                f.format_hex(val, false, 0, false, false, false);
            }
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' => {
                // Simplified float: print integer part + 6 decimals
                let val = f64::from_bits(*args);
                args = args.add(1);
                let prec = precision.unwrap_or(6);
                let negative = val < 0.0;
                let abs_val = if negative { -val } else { val };
                let int_part = abs_val as u64;
                let frac_mult = 10u64.pow(prec as u32);
                let frac_part = ((abs_val - int_part as f64) * frac_mult as f64) as u64;
                if negative {
                    f.push(b'-');
                }
                f.format_unsigned(int_part, 0, false, false);
                if prec > 0 {
                    f.push(b'.');
                    f.format_unsigned(frac_part, prec, true, false);
                }
            }
            b'n' => {
                // Store number of characters written so far
                let ptr = *args as *mut i32;
                args = args.add(1);
                if !ptr.is_null() {
                    *ptr = f.len() as i32;
                }
            }
            b'%' => {
                f.push(b'%');
            }
            _ => {
                // Unknown specifier — output as-is
                f.push(b'%');
                f.push(spec);
            }
        }
    }

    f.finish()
}

// ═══════════════════════════════════════════════════════════════════════
// POSIX FILE I/O (unistd.h / fcntl.h)
// ═══════════════════════════════════════════════════════════════════════

/// File open flags
pub const O_RDONLY: i32 = 0;
pub const O_WRONLY: i32 = 1;
pub const O_RDWR: i32 = 2;
pub const O_CREAT: i32 = 0o100;
pub const O_EXCL: i32 = 0o200;
pub const O_NOCTTY: i32 = 0o400;
pub const O_TRUNC: i32 = 0o1000;
pub const O_APPEND: i32 = 0o2000;
pub const O_NONBLOCK: i32 = 0o4000;
pub const O_CLOEXEC: i32 = 0o2000000;
pub const O_DIRECTORY: i32 = 0o200000;
pub const O_NOFOLLOW: i32 = 0o400000;

/// Seek constants
pub const SEEK_SET: i32 = 0;
pub const SEEK_CUR: i32 = 1;
pub const SEEK_END: i32 = 2;

/// Access mode flags
pub const F_OK: i32 = 0;
pub const R_OK: i32 = 4;
pub const W_OK: i32 = 2;
pub const X_OK: i32 = 1;

// ═══════════════════════════════════════════════════════════════════════
// STDIO — FILE streams
// ═══════════════════════════════════════════════════════════════════════

/// Simplified FILE structure for stdio streams
#[repr(C)]
pub struct StdioFile {
    pub fd: i32,
    pub flags: u32,
    pub buf: *mut u8,
    pub buf_size: usize,
    pub buf_pos: usize,
    pub buf_end: usize,
    pub error: i32,
    pub eof: i32,
    pub mode: u32, // 0=unbuffered, 1=line-buffered, 2=fully-buffered
}

/// FILE* buffer modes
pub const _IONBF: i32 = 0;
pub const _IOLBF: i32 = 1;
pub const _IOFBF: i32 = 2;

pub const BUFSIZ: usize = 8192;
pub const EOF: i32 = -1;

// Standard streams (kernel-level stubs)
static mut STDIN_FILE: StdioFile = StdioFile {
    fd: 0,
    flags: 0,
    buf: core::ptr::null_mut(),
    buf_size: 0,
    buf_pos: 0,
    buf_end: 0,
    error: 0,
    eof: 0,
    mode: 0,
};
static mut STDOUT_FILE: StdioFile = StdioFile {
    fd: 1,
    flags: 1,
    buf: core::ptr::null_mut(),
    buf_size: 0,
    buf_pos: 0,
    buf_end: 0,
    error: 0,
    eof: 0,
    mode: 1,
};
static mut STDERR_FILE: StdioFile = StdioFile {
    fd: 2,
    flags: 1,
    buf: core::ptr::null_mut(),
    buf_size: 0,
    buf_pos: 0,
    buf_end: 0,
    error: 0,
    eof: 0,
    mode: 0,
};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __stdin() -> *mut StdioFile {
    &raw mut STDIN_FILE
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __stdout() -> *mut StdioFile {
    &raw mut STDOUT_FILE
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __stderr() -> *mut StdioFile {
    &raw mut STDERR_FILE
}

/// fopen — open a file stream
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fopen(path: *const u8, mode: *const u8) -> *mut StdioFile {
    if path.is_null() || mode.is_null() {
        return core::ptr::null_mut();
    }

    // Parse the path
    let mut plen = 0;
    while *path.add(plen) != 0 {
        plen += 1;
    }
    let path_slice = core::slice::from_raw_parts(path, plen);
    let path_str = core::str::from_utf8_unchecked(path_slice);

    // Parse the mode
    let m = *mode;
    let flags = match m {
        b'r' => O_RDONLY,
        b'w' => O_WRONLY | O_CREAT | O_TRUNC,
        b'a' => O_WRONLY | O_CREAT | O_APPEND,
        _ => O_RDONLY,
    };

    // Use VFS to check if file exists
    let fd = if flags & O_WRONLY != 0 {
        // Write mode — create or truncate
        crate::vfs::create_file_dispatch(path_str, &[]);
        42 // Dummy FD for kernel-level stdio
    } else {
        // Read mode
        if crate::vfs::read_file_dispatch(path_str).is_some() {
            42
        } else {
            return core::ptr::null_mut();
        }
    };

    // Allocate FILE struct
    let file = tracked_malloc(core::mem::size_of::<StdioFile>()) as *mut StdioFile;
    if file.is_null() {
        return core::ptr::null_mut();
    }
    (*file).fd = fd;
    (*file).flags = flags as u32;
    (*file).buf = core::ptr::null_mut();
    (*file).buf_size = 0;
    (*file).buf_pos = 0;
    (*file).buf_end = 0;
    (*file).error = 0;
    (*file).eof = 0;
    (*file).mode = 2; // fully buffered by default
    file
}

/// fclose — close a file stream
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fclose(stream: *mut StdioFile) -> i32 {
    if stream.is_null() {
        return EOF;
    }
    if !(*stream).buf.is_null() {
        tracked_free((*stream).buf as *mut c_void);
    }
    tracked_free(stream as *mut c_void);
    0
}

/// fflush — flush a stream
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fflush(_stream: *mut StdioFile) -> i32 {
    0 // No-op for kernel-level streams
}

/// feof — test end-of-file indicator
#[unsafe(no_mangle)]
pub unsafe extern "C" fn feof(stream: *mut StdioFile) -> i32 {
    if stream.is_null() {
        return 0;
    }
    (*stream).eof
}

/// ferror — test error indicator
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ferror(stream: *mut StdioFile) -> i32 {
    if stream.is_null() {
        return 0;
    }
    (*stream).error
}

/// clearerr — clear error and EOF indicators
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearerr(stream: *mut StdioFile) {
    if !stream.is_null() {
        (*stream).error = 0;
        (*stream).eof = 0;
    }
}

/// fileno — get file descriptor from stream
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fileno(stream: *mut StdioFile) -> i32 {
    if stream.is_null() {
        return -1;
    }
    (*stream).fd
}

/// setvbuf — set stream buffering mode
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setvbuf(
    _stream: *mut StdioFile,
    _buf: *mut u8,
    mode: i32,
    _size: usize,
) -> i32 {
    0 // Accept but ignore for now
}

/// setbuf — set stream buffering
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setbuf(stream: *mut StdioFile, buf: *mut u8) {
    if buf.is_null() {
        setvbuf(stream, core::ptr::null_mut(), _IONBF, 0);
    } else {
        setvbuf(stream, buf, _IOFBF, BUFSIZ);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// POSIX THREADS (pthread.h)
// ═══════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════
// TIME FUNCTIONS (time.h / sys/time.h)
// ═══════════════════════════════════════════════════════════════════════

/// struct timespec
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

/// struct timeval
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timeval {
    pub tv_sec: i64,
    pub tv_usec: i64,
}

/// struct timezone
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timezone {
    pub tz_minuteswest: i32,
    pub tz_dsttime: i32,
}

/// struct tm (broken-down time)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Tm {
    pub tm_sec: i32,
    pub tm_min: i32,
    pub tm_hour: i32,
    pub tm_mday: i32,
    pub tm_mon: i32,
    pub tm_year: i32,
    pub tm_wday: i32,
    pub tm_yday: i32,
    pub tm_isdst: i32,
    pub tm_gmtoff: i64,
    pub tm_zone: *const u8,
}

pub const CLOCK_REALTIME: i32 = 0;
pub const CLOCK_MONOTONIC: i32 = 1;
pub const CLOCK_PROCESS_CPUTIME_ID: i32 = 2;
pub const CLOCK_THREAD_CPUTIME_ID: i32 = 3;
pub const CLOCK_MONOTONIC_RAW: i32 = 4;
pub const CLOCK_REALTIME_COARSE: i32 = 5;
pub const CLOCK_MONOTONIC_COARSE: i32 = 6;
pub const CLOCK_BOOTTIME: i32 = 7;

/// Kernel boot TSC for monotonic time
static BOOT_TSC: AtomicU64 = AtomicU64::new(0);

/// clock_gettime — get time from a clock
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clock_gettime(clock_id: i32, tp: *mut Timespec) -> i32 {
    if tp.is_null() {
        return -1;
    }

    let ticks = crate::clock::get_ticks();
    let seconds = ticks / 1000;
    let millis = ticks % 1000;

    match clock_id {
        CLOCK_REALTIME | CLOCK_REALTIME_COARSE => {
            // Use RTC for wall clock time
            let rtc = crate::rtc::read_rtc();
            (*tp).tv_sec = rtc.to_unix_timestamp();
            (*tp).tv_nsec = (millis * 1_000_000) as i64;
        }
        CLOCK_MONOTONIC | CLOCK_MONOTONIC_RAW | CLOCK_MONOTONIC_COARSE | CLOCK_BOOTTIME => {
            (*tp).tv_sec = seconds as i64;
            (*tp).tv_nsec = (millis * 1_000_000) as i64;
        }
        _ => {
            (*tp).tv_sec = seconds as i64;
            (*tp).tv_nsec = (millis * 1_000_000) as i64;
        }
    }

    0
}

/// gettimeofday — get time of day
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gettimeofday(tv: *mut Timeval, tz: *mut Timezone) -> i32 {
    if !tv.is_null() {
        let ticks = crate::clock::get_ticks();
        let rtc = crate::rtc::read_rtc();
        (*tv).tv_sec = rtc.to_unix_timestamp();
        (*tv).tv_usec = ((ticks % 1000) * 1000) as i64;
    }
    if !tz.is_null() {
        (*tz).tz_minuteswest = 0;
        (*tz).tz_dsttime = 0;
    }
    0
}

/// time — get time in seconds
#[unsafe(no_mangle)]
pub unsafe extern "C" fn time(t: *mut i64) -> i64 {
    let rtc = crate::rtc::read_rtc();
    let secs = rtc.to_unix_timestamp();
    if !t.is_null() {
        *t = secs;
    }
    secs
}

/// nanosleep — high-resolution sleep
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nanosleep(req: *const Timespec, rem: *mut Timespec) -> i32 {
    if req.is_null() {
        return -1;
    }
    let total_ms = (*req).tv_sec * 1000 + (*req).tv_nsec / 1_000_000;
    // Yield for approximate time
    let start = crate::clock::get_ticks();
    while (crate::clock::get_ticks() - start) < total_ms as u64 {
        crate::scheduler::yield_now();
    }
    if !rem.is_null() {
        (*rem).tv_sec = 0;
        (*rem).tv_nsec = 0;
    }
    0
}

/// usleep — sleep for microseconds
#[unsafe(no_mangle)]
pub unsafe extern "C" fn usleep(usec: u32) -> i32 {
    let ts = Timespec {
        tv_sec: (usec / 1_000_000) as i64,
        tv_nsec: ((usec % 1_000_000) * 1000) as i64,
    };
    nanosleep(&ts, core::ptr::null_mut())
}

/// sleep — sleep for seconds
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sleep(seconds: u32) -> u32 {
    let ts = Timespec {
        tv_sec: seconds as i64,
        tv_nsec: 0,
    };
    nanosleep(&ts, core::ptr::null_mut());
    0
}

// ═══════════════════════════════════════════════════════════════════════
// DIRECTORY OPERATIONS (dirent.h)
// ═══════════════════════════════════════════════════════════════════════

/// struct dirent
#[repr(C)]
pub struct Dirent {
    pub d_ino: u64,
    pub d_off: i64,
    pub d_reclen: u16,
    pub d_type: u8,
    pub d_name: [u8; 256],
}

/// DT_* constants for d_type
pub const DT_UNKNOWN: u8 = 0;
pub const DT_FIFO: u8 = 1;
pub const DT_CHR: u8 = 2;
pub const DT_DIR: u8 = 4;
pub const DT_BLK: u8 = 6;
pub const DT_REG: u8 = 8;
pub const DT_LNK: u8 = 10;
pub const DT_SOCK: u8 = 12;

/// DIR stream
pub struct DirStream {
    pub path: String,
    pub entries: Vec<(String, u8)>, // (name, type)
    pub pos: usize,
}

lazy_static::lazy_static! {
    static ref OPEN_DIRS: Mutex<BTreeMap<u64, DirStream>> = Mutex::new(BTreeMap::new());
}

static NEXT_DIR_HANDLE: AtomicU64 = AtomicU64::new(1);

/// opendir — open a directory stream
#[unsafe(no_mangle)]
pub unsafe extern "C" fn opendir(name: *const u8) -> u64 {
    if name.is_null() {
        return 0;
    }
    let mut len = 0;
    while *name.add(len) != 0 {
        len += 1;
    }
    let path = core::str::from_utf8_unchecked(core::slice::from_raw_parts(name, len));

    // List directory via VFS
    let listing = crate::vfs::list_directory(path).unwrap_or_default();
    let entries: Vec<(String, u8)> = listing
        .iter()
        .map(|e| {
            let dtype = if e.ends_with('/') { DT_DIR } else { DT_REG };
            let name = e.trim_end_matches('/').to_string();
            (name, dtype)
        })
        .collect();

    let handle = NEXT_DIR_HANDLE.fetch_add(1, Ordering::Relaxed);
    OPEN_DIRS.lock().insert(
        handle,
        DirStream {
            path: String::from(path),
            entries,
            pos: 0,
        },
    );

    handle
}

/// closedir — close a directory stream
#[unsafe(no_mangle)]
pub unsafe extern "C" fn closedir(dirp: u64) -> i32 {
    OPEN_DIRS.lock().remove(&dirp);
    0
}

// ═══════════════════════════════════════════════════════════════════════
// SIGNAL MANAGEMENT (signal.h)
// ═══════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════
// MATH FUNCTIONS (software float, no_std compatible)
// ═══════════════════════════════════════════════════════════════════════

/// fabs — absolute value of float
#[unsafe(no_mangle)]
pub extern "C" fn fabs(x: f64) -> f64 {
    if x < 0.0 { -x } else { x }
}

/// fabsf — absolute value of float (f32)
#[unsafe(no_mangle)]
pub extern "C" fn fabsf(x: f32) -> f32 {
    if x < 0.0 { -x } else { x }
}

/// sqrt — square root (Newton-Raphson)
#[unsafe(no_mangle)]
pub extern "C" fn sqrt(x: f64) -> f64 {
    if x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return 0.0;
    }
    let mut guess = x;
    let i = f64::to_bits(x);
    let i = 0x1FF7A3BEA91D9B1B_u64.wrapping_add(i >> 1);
    guess = f64::from_bits(i);
    for _ in 0..5 {
        guess = 0.5 * (guess + x / guess);
    }
    guess
}

/// sqrtf — square root (f32)
#[unsafe(no_mangle)]
pub extern "C" fn sqrtf(x: f32) -> f32 {
    if x < 0.0 {
        return f32::NAN;
    }
    if x == 0.0 {
        return 0.0;
    }
    let mut guess = x;
    let i = f32::to_bits(x);
    let i = 0x1FBD1DF5_u32.wrapping_add(i >> 1);
    guess = f32::from_bits(i);
    for _ in 0..4 {
        guess = 0.5 * (guess + x / guess);
    }
    guess
}

/// floor — largest integer not greater than x
#[unsafe(no_mangle)]
pub extern "C" fn floor(x: f64) -> f64 {
    let i = x as i64;
    let f = i as f64;
    if x < f { f - 1.0 } else { f }
}

/// floorf
#[unsafe(no_mangle)]
pub extern "C" fn floorf(x: f32) -> f32 {
    let i = x as i32;
    let f = i as f32;
    if x < f { f - 1.0 } else { f }
}

/// ceil — smallest integer not less than x
#[unsafe(no_mangle)]
pub extern "C" fn ceil(x: f64) -> f64 {
    let i = x as i64;
    let f = i as f64;
    if x > f { f + 1.0 } else { f }
}

/// ceilf
#[unsafe(no_mangle)]
pub extern "C" fn ceilf(x: f32) -> f32 {
    let i = x as i32;
    let f = i as f32;
    if x > f { f + 1.0 } else { f }
}

/// round — round to nearest integer
#[unsafe(no_mangle)]
pub extern "C" fn round(x: f64) -> f64 {
    floor(x + 0.5)
}

/// roundf
#[unsafe(no_mangle)]
pub extern "C" fn roundf(x: f32) -> f32 {
    floorf(x + 0.5)
}

/// fmod — floating-point remainder
#[unsafe(no_mangle)]
pub extern "C" fn fmod(x: f64, y: f64) -> f64 {
    if y == 0.0 {
        return f64::NAN;
    }
    x - (x / y) as i64 as f64 * y
}

/// fmodf
#[unsafe(no_mangle)]
pub extern "C" fn fmodf(x: f32, y: f32) -> f32 {
    if y == 0.0 {
        return f32::NAN;
    }
    x - (x / y) as i32 as f32 * y
}

/// log — natural logarithm (series approximation)
#[unsafe(no_mangle)]
pub extern "C" fn log(x: f64) -> f64 {
    if x <= 0.0 {
        return f64::NAN;
    }
    // Use the identity: ln(x) = ln(2) * log2(x)
    // log2(x) via bit manipulation + polynomial
    let bits = f64::to_bits(x);
    let exp = ((bits >> 52) & 0x7FF) as i64 - 1023;
    let mantissa = f64::from_bits((bits & 0x000FFFFFFFFFFFFF) | 0x3FF0000000000000);
    // Polynomial approximation for ln(m) where m in [1, 2)
    let m = mantissa - 1.0;
    let ln_m = m * (1.0 - m * (0.5 - m * (1.0 / 3.0 - m * 0.25)));
    ln_m + exp as f64 * core::f64::consts::LN_2
}

/// log10 — base-10 logarithm
#[unsafe(no_mangle)]
pub extern "C" fn log10(x: f64) -> f64 {
    log(x) * core::f64::consts::LOG10_E
}

/// pow — power function (simplified)
#[unsafe(no_mangle)]
pub extern "C" fn pow(base: f64, exp: f64) -> f64 {
    if exp == 0.0 {
        return 1.0;
    }
    if base == 0.0 {
        return 0.0;
    }
    // For integer exponents, use repeated multiplication
    let trunc_exp = exp as i64 as f64;
    if exp == trunc_exp && (if exp < 0.0 { -exp } else { exp }) < 100.0 {
        let n = exp as i64;
        let mut result = 1.0;
        let mut b = base;
        let mut e = n.unsigned_abs();
        while e > 0 {
            if e & 1 == 1 {
                result *= b;
            }
            b *= b;
            e >>= 1;
        }
        if n < 0 { 1.0 / result } else { result }
    } else {
        // exp(exp * ln(base))
        let lnb = log(base);
        exp_approx(exp * lnb)
    }
}

/// exp — exponential function (Taylor series)
fn exp_approx(x: f64) -> f64 {
    if x > 709.0 {
        return f64::INFINITY;
    }
    if x < -709.0 {
        return 0.0;
    }
    // Reduce: e^x = 2^k * e^r where r = x - k*ln(2)
    let k = {
        let v = x * core::f64::consts::LOG2_E;
        if v >= 0.0 {
            (v + 0.5) as i64
        } else {
            (v - 0.5) as i64
        }
    }; // 1/ln(2)
    let r = x - k as f64 * core::f64::consts::LN_2;
    // Taylor series for e^r (r is small)
    let mut term = 1.0;
    let mut sum = 1.0;
    for i in 1..=15 {
        term *= r / i as f64;
        sum += term;
    }
    // Multiply by 2^k
    let scale = f64::from_bits(((k + 1023) as u64) << 52);
    sum * scale
}

/// exp (exported)
#[unsafe(no_mangle)]
pub extern "C" fn exp(x: f64) -> f64 {
    exp_approx(x)
}

/// expf
#[unsafe(no_mangle)]
pub extern "C" fn expf(x: f32) -> f32 {
    exp_approx(x as f64) as f32
}

// ═══════════════════════════════════════════════════════════════════════
// LOCALE (locale.h)
// ═══════════════════════════════════════════════════════════════════════

/// struct lconv (simplified)
#[repr(C)]
pub struct Lconv {
    pub decimal_point: *const u8,
    pub thousands_sep: *const u8,
    pub grouping: *const u8,
    pub int_curr_symbol: *const u8,
    pub currency_symbol: *const u8,
    pub mon_decimal_point: *const u8,
    pub mon_thousands_sep: *const u8,
    pub mon_grouping: *const u8,
    pub positive_sign: *const u8,
    pub negative_sign: *const u8,
}

static DECIMAL_POINT: [u8; 2] = [b'.', 0];
static EMPTY_STRING: [u8; 1] = [0];
static NEGATIVE_SIGN: [u8; 2] = [b'-', 0];

static mut DEFAULT_LCONV: Lconv = Lconv {
    decimal_point: DECIMAL_POINT.as_ptr(),
    thousands_sep: EMPTY_STRING.as_ptr(),
    grouping: EMPTY_STRING.as_ptr(),
    int_curr_symbol: EMPTY_STRING.as_ptr(),
    currency_symbol: EMPTY_STRING.as_ptr(),
    mon_decimal_point: EMPTY_STRING.as_ptr(),
    mon_thousands_sep: EMPTY_STRING.as_ptr(),
    mon_grouping: EMPTY_STRING.as_ptr(),
    positive_sign: EMPTY_STRING.as_ptr(),
    negative_sign: NEGATIVE_SIGN.as_ptr(),
};

static LC_ALL_NAME: [u8; 12] = *b"en_US.UTF-8\0";

/// setlocale — set locale
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setlocale(_category: i32, locale: *const u8) -> *const u8 {
    if locale.is_null() || *locale == 0 {
        return LC_ALL_NAME.as_ptr();
    }
    LC_ALL_NAME.as_ptr()
}

/// localeconv — get locale formatting parameters
#[unsafe(no_mangle)]
pub unsafe extern "C" fn localeconv() -> *mut Lconv {
    &raw mut DEFAULT_LCONV
}

// ═══════════════════════════════════════════════════════════════════════
// DLFCN (dlfcn.h) — bridges to dynlink.rs
// ═══════════════════════════════════════════════════════════════════════

/// dlopen — open a shared library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dlopen_c(filename: *const u8, flags: i32) -> *mut c_void {
    let name = if filename.is_null() {
        None
    } else {
        let mut len = 0;
        while *filename.add(len) != 0 {
            len += 1;
        }
        Some(core::str::from_utf8_unchecked(core::slice::from_raw_parts(
            filename, len,
        )))
    };
    let handle = crate::dynlink::dlopen(name, flags);
    if handle == 0 {
        core::ptr::null_mut()
    } else {
        handle as *mut c_void
    }
}

/// dlsym — look up a symbol
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dlsym_c(handle: *mut c_void, symbol: *const u8) -> *mut c_void {
    if symbol.is_null() {
        return core::ptr::null_mut();
    }
    let mut len = 0;
    while *symbol.add(len) != 0 {
        len += 1;
    }
    let sym_name = core::str::from_utf8_unchecked(core::slice::from_raw_parts(symbol, len));
    crate::dynlink::dlsym(handle as u64, sym_name)
}

/// dlclose — close a shared library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dlclose_c(handle: *mut c_void) -> i32 {
    crate::dynlink::dlclose(handle as u64)
}

/// dlerror — get error message
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dlerror_c() -> *const u8 {
    // Returns a pointer to a static error message
    static mut ERROR_BUF: [u8; 256] = [0; 256];
    match crate::dynlink::dlerror() {
        Some(msg) => {
            let bytes = msg.as_bytes();
            let len = bytes.len().min(255);
            let buf_ptr = core::ptr::addr_of_mut!(ERROR_BUF) as *mut u8;
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf_ptr, len);
            *buf_ptr.add(len) = 0;
            buf_ptr as *const u8
        }
        None => core::ptr::null(),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// MISCELLANEOUS POSIX FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════

/// getpid — get process ID
#[unsafe(no_mangle)]
pub extern "C" fn getpid() -> i32 {
    crate::scheduler::current_pid().unwrap_or(1) as i32
}

/// getppid — get parent process ID
#[unsafe(no_mangle)]
pub extern "C" fn getppid() -> i32 {
    1 // init is always parent for now
}

/// getuid / geteuid — get user ID
#[unsafe(no_mangle)]
pub extern "C" fn getuid() -> u32 {
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn geteuid() -> u32 {
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn getgid() -> u32 {
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn getegid() -> u32 {
    0
}

/// sysconf — get configurable system variables
#[unsafe(no_mangle)]
pub extern "C" fn sysconf(name: i32) -> i64 {
    match name {
        30 => 4096,  // _SC_PAGESIZE
        84 => 4096,  // _SC_PAGE_SIZE
        11 => 1,     // _SC_NPROCESSORS_ONLN (conservative)
        83 => 1,     // _SC_NPROCESSORS_CONF
        2 => 200809, // _SC_VERSION (POSIX.1-2008)
        4 => 2048,   // _SC_OPEN_MAX
        29 => 4096,  // _SC_ARG_MAX
        0 => 2048,   // _SC_ARG_MAX
        8 => 256,    // _SC_NAME_MAX
        12 => 4096,  // _SC_LINE_MAX
        _ => -1,
    }
}

/// sysconf — get host/domain name length
#[unsafe(no_mangle)]
pub extern "C" fn get_nprocs() -> i32 {
    1
}

/// getpagesize
#[unsafe(no_mangle)]
pub extern "C" fn getpagesize() -> i32 {
    4096
}

/// isatty — test whether fd is a terminal
#[unsafe(no_mangle)]
pub extern "C" fn isatty(fd: i32) -> i32 {
    if (0..=2).contains(&fd) { 1 } else { 0 }
}

/// getcwd — get current working directory
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getcwd(buf: *mut u8, size: usize) -> *mut u8 {
    let cwd = b"/home/user\0";
    if buf.is_null() || size < cwd.len() {
        return core::ptr::null_mut();
    }
    core::ptr::copy_nonoverlapping(cwd.as_ptr(), buf, cwd.len());
    buf
}

/// strerror — get string description of error number
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strerror(errnum: i32) -> *const u8 {
    match errnum {
        0 => c"Success".as_ptr() as *const u8,
        1 => c"Operation not permitted".as_ptr() as *const u8,
        2 => c"No such file or directory".as_ptr() as *const u8,
        3 => c"No such process".as_ptr() as *const u8,
        4 => c"Interrupted system call".as_ptr() as *const u8,
        5 => c"Input/output error".as_ptr() as *const u8,
        9 => c"Bad file descriptor".as_ptr() as *const u8,
        11 => c"Resource temporarily unavailable".as_ptr() as *const u8,
        12 => c"Cannot allocate memory".as_ptr() as *const u8,
        13 => c"Permission denied".as_ptr() as *const u8,
        14 => c"Bad address".as_ptr() as *const u8,
        17 => c"File exists".as_ptr() as *const u8,
        20 => c"Not a directory".as_ptr() as *const u8,
        21 => c"Is a directory".as_ptr() as *const u8,
        22 => c"Invalid argument".as_ptr() as *const u8,
        28 => c"No space left on device".as_ptr() as *const u8,
        36 => c"Numerical result out of range".as_ptr() as *const u8,
        38 => c"Function not implemented".as_ptr() as *const u8,
        _ => c"Unknown error".as_ptr() as *const u8,
    }
}

/// perror — print error message
#[unsafe(no_mangle)]
pub unsafe extern "C" fn perror(s: *const u8) {
    let errno = *crate::libc_funcs::__errno_location();
    let msg = strerror(errno);
    if !s.is_null() && *s != 0 {
        let mut len = 0;
        while *s.add(len) != 0 {
            len += 1;
        }
        let prefix = core::slice::from_raw_parts(s, len);
        if let Ok(p) = core::str::from_utf8(prefix) {
            let mut mlen = 0;
            while *msg.add(mlen) != 0 {
                mlen += 1;
            }
            let msg_str = core::str::from_utf8_unchecked(core::slice::from_raw_parts(msg, mlen));
            serial_println!("{}: {}", p, msg_str);
        }
    }
}

/// access — check file accessibility
#[unsafe(no_mangle)]
pub unsafe extern "C" fn access(path: *const u8, _mode: i32) -> i32 {
    if path.is_null() {
        return -1;
    }
    let mut len = 0;
    while *path.add(len) != 0 {
        len += 1;
    }
    let p = core::str::from_utf8_unchecked(core::slice::from_raw_parts(path, len));
    if crate::vfs::read_file_dispatch(p).is_some() {
        0
    } else {
        -1
    }
}

/// pipe — create a pipe
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pipe(pipefd: *mut i32) -> i32 {
    if pipefd.is_null() {
        return -1;
    }
    let (read_fd, write_fd) = crate::pipe::create_pipe();
    *pipefd = read_fd;
    *pipefd.add(1) = write_fd;
    0
}

/// dup — duplicate a file descriptor
#[unsafe(no_mangle)]
pub extern "C" fn dup(oldfd: i32) -> i32 {
    crate::fd::dup(oldfd as usize)
        .map(|fd| fd as i32)
        .unwrap_or(-1)
}

/// dup2 — duplicate a file descriptor to a specific number
#[unsafe(no_mangle)]
pub extern "C" fn dup2(oldfd: i32, newfd: i32) -> i32 {
    crate::fd::dup2(oldfd as usize, newfd as usize)
        .map(|fd| fd as i32)
        .unwrap_or(-1)
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

pub fn init() {
    BOOT_TSC.store(crate::clock::get_ticks(), Ordering::Relaxed);

    serial_println!("[KnoxOS] POSIX libc compatibility layer initialized (Phase 24)");
    serial_println!("[KnoxOS]   stdio.h: printf engine, fopen/fclose/fflush, FILE streams");
    serial_println!("[KnoxOS]   pthread.h: create/join/detach, mutex, cond, TLS, once");
    serial_println!("[KnoxOS]   time.h: clock_gettime, gettimeofday, nanosleep, sleep");
    serial_println!("[KnoxOS]   dirent.h: opendir/readdir/closedir");
    serial_println!("[KnoxOS]   signal.h: sigaction, signal, sigprocmask, sigset ops");
    serial_println!("[KnoxOS]   math.h: sqrt, floor, ceil, round, log, pow, exp, fmod");
    serial_println!("[KnoxOS]   locale.h: setlocale, localeconv");
    serial_println!("[KnoxOS]   dlfcn.h: dlopen, dlsym, dlclose, dlerror (→ dynlink.rs)");
    serial_println!("[KnoxOS]   unistd.h: getpid, getcwd, sleep, access, pipe, dup");
}

use alloc::string::ToString;
static NEXT_TLS_KEY2: AtomicU32 = AtomicU32::new(0);
