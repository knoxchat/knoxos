// FILE streams (stdio.h)
use core::ffi::c_void;

use super::fcntl::{O_APPEND, O_CREAT, O_RDONLY, O_TRUNC, O_WRONLY};
use super::malloc::{tracked_free, tracked_malloc};

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
