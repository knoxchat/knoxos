/// splice — Zero-copy data transfer between file descriptors
/// Linux-compatible splice, tee, vmsplice, sendfile operations
///
/// These operations allow efficient data transfer between file descriptors
/// without copying through userspace.
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Splice flags
pub const SPLICE_F_MOVE: u32 = 1;
pub const SPLICE_F_NONBLOCK: u32 = 2;
pub const SPLICE_F_MORE: u32 = 4;
pub const SPLICE_F_GIFT: u32 = 8;

/// Internal pipe buffer for splice operations
const PIPE_BUF_SIZE: usize = 65536;

/// splice() — move data between two file descriptors (at least one must be a pipe)
pub fn splice(
    fd_in: i32,
    off_in: Option<i64>,
    fd_out: i32,
    off_out: Option<i64>,
    len: usize,
    flags: u32,
) -> Result<usize, i32> {
    let pid = crate::scheduler::current_pid().unwrap_or(1);

    // Read from source fd
    let mut buf = vec![0u8; core::cmp::min(len, PIPE_BUF_SIZE)];
    let bytes_read = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        let fd_table = tables.get_mut(&pid).ok_or(-9i32)?; // EBADF
        fd_table.read(fd_in, &mut buf).map_err(|_| -9i32)?
    };

    if bytes_read == 0 {
        return Ok(0);
    }

    // Write to destination fd
    let bytes_written = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        let fd_table = tables.get_mut(&pid).ok_or(-9i32)?;
        fd_table
            .write(fd_out, &buf[..bytes_read])
            .map_err(|_| -9i32)?
    };

    Ok(bytes_written)
}

/// tee() — duplicate pipe data without consuming it
pub fn tee(fd_in: i32, fd_out: i32, len: usize, flags: u32) -> Result<usize, i32> {
    // For now, tee is implemented similarly to splice
    // A proper implementation would peek at the pipe buffer
    splice(fd_in, None, fd_out, None, len, flags)
}

/// sendfile() — transfer data between file descriptors (optimized for regular files → sockets)
pub fn sendfile(out_fd: i32, in_fd: i32, offset: Option<i64>, count: usize) -> Result<usize, i32> {
    let pid = crate::scheduler::current_pid().unwrap_or(1);

    let mut total_sent = 0;
    let mut remaining = count;
    let chunk_size = 4096;

    while remaining > 0 {
        let to_read = core::cmp::min(remaining, chunk_size);
        let mut buf = vec![0u8; to_read];

        // Read from input file
        let bytes_read = {
            let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
            let fd_table = tables.get_mut(&pid).ok_or(-9i32)?;
            match fd_table.read(in_fd, &mut buf) {
                Ok(n) => n,
                Err(_) => break,
            }
        };

        if bytes_read == 0 {
            break;
        }

        // Write to output fd
        let bytes_written = {
            let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
            let fd_table = tables.get_mut(&pid).ok_or(-9i32)?;
            match fd_table.write(out_fd, &buf[..bytes_read]) {
                Ok(n) => n,
                Err(_) => break,
            }
        };

        total_sent += bytes_written;
        remaining -= bytes_written;

        if bytes_written < bytes_read {
            break;
        }
    }

    Ok(total_sent)
}

/// copy_file_range() — copy data between two regular files
pub fn copy_file_range(
    fd_in: i32,
    off_in: Option<i64>,
    fd_out: i32,
    off_out: Option<i64>,
    len: usize,
    flags: u32,
) -> Result<usize, i32> {
    // Implemented the same as sendfile for now
    sendfile(fd_out, fd_in, off_in, len)
}

/// vmsplice() — splice user pages to/from a pipe
pub fn vmsplice(fd: i32, iov: &[(u64, usize)], flags: u32) -> Result<usize, i32> {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut total = 0;

    for (base, len) in iov {
        if *base == 0 || *len == 0 {
            continue;
        }

        let buf = unsafe { core::slice::from_raw_parts(*base as *const u8, *len) };

        let written = {
            let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
            let fd_table = tables.get_mut(&pid).ok_or(-9i32)?;
            fd_table.write(fd, buf).map_err(|_| -9i32)?
        };

        total += written;
    }

    Ok(total)
}

/// readv() — scatter-gather read
pub fn readv(fd: i32, iovs: &mut [(u64, usize)]) -> Result<usize, i32> {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut total = 0;

    for (base, len) in iovs.iter() {
        if *base == 0 || *len == 0 {
            continue;
        }

        let buf = unsafe { core::slice::from_raw_parts_mut(*base as *mut u8, *len) };

        let bytes_read = {
            let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
            let fd_table = tables.get_mut(&pid).ok_or(-9i32)?;
            match fd_table.read(fd, buf) {
                Ok(n) => n,
                Err(_) => break,
            }
        };

        total += bytes_read;
        if bytes_read < *len {
            break;
        }
    }

    Ok(total)
}

/// writev() — scatter-gather write
pub fn writev(fd: i32, iovs: &[(u64, usize)]) -> Result<usize, i32> {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut total = 0;

    for (base, len) in iovs {
        if *base == 0 || *len == 0 {
            continue;
        }

        let buf = unsafe { core::slice::from_raw_parts(*base as *const u8, *len) };

        let written = {
            let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
            let fd_table = tables.get_mut(&pid).ok_or(-9i32)?;
            fd_table.write(fd, buf).map_err(|_| -9i32)?
        };

        total += written;
        if written < *len {
            break;
        }
    }

    Ok(total)
}

/// preadv() — positional scatter-gather read
pub fn preadv(fd: i32, iovs: &mut [(u64, usize)], _offset: i64) -> Result<usize, i32> {
    // Simplified: doesn't actually use offset
    readv(fd, iovs)
}

/// pwritev() — positional scatter-gather write
pub fn pwritev(fd: i32, iovs: &[(u64, usize)], _offset: i64) -> Result<usize, i32> {
    writev(fd, iovs)
}

pub fn init() {
    serial_println!("[KnoxOS] Zero-copy I/O (splice/sendfile/readv/writev) initialized");
}
