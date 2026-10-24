//! Chromium leftover: Ring 3 `io_uring_setup` (`GATE_CHR iouring`).
//!
//! Kernel SQE processing is already live (`GATE_IOURING sqe`). This row
//! exports that ring through the Linux syscall so Chromium can create a
//! ring fd. `io_uring_setup(0, 0)` stays `-ENOSYS` (Gate M4). Ring 3
//! `userfaultfd` stays `-ENOSYS` (Gate N4).

use crate::io_uring::{
    IORING_ENTER_GETEVENTS, IoUringOp, IoUringSqe, reap_cqes, ring_id_for_current_fd, submit_sqe,
};
use crate::serial_println;

pub const GATE_CHR_IOURING_MARKER: &str = "GATE_CHR iouring";

/// Leftover KAT: Ring 3 `io_uring_setup(8)` returns a ring fd; `(0,0)` is ENOSYS.
pub fn iouring_self_test() -> bool {
    let zero = crate::syscall::handle_syscall(425, 0, 0, 0, 0, 0, 0);
    if zero != -38 {
        serial_println!(
            "[chrome] GATE_CHR iouring FAILED: setup(0) rax={} (want ENOSYS)",
            zero
        );
        return false;
    }
    let uffd = crate::syscall::handle_syscall(323, 0, 0, 0, 0, 0, 0);
    if uffd != -38 {
        serial_println!(
            "[chrome] GATE_CHR iouring FAILED: userfaultfd rax={} (want ENOSYS)",
            uffd
        );
        return false;
    }
    let r = crate::syscall::handle_syscall(425, 8, 0, 0, 0, 0, 0);
    if r < 0 {
        serial_println!("[chrome] GATE_CHR iouring FAILED: setup rax={}", r);
        return false;
    }
    let fd = r as i32;
    let Some(ring) = ring_id_for_current_fd(fd) else {
        serial_println!("[chrome] GATE_CHR iouring FAILED: fd {} is not a ring", fd);
        let _ = crate::syscall::handle_syscall(3, fd as u64, 0, 0, 0, 0, 0);
        return false;
    };
    if submit_sqe(ring, IoUringSqe::new(IoUringOp::Nop, -1, 1)).is_err() {
        serial_println!("[chrome] GATE_CHR iouring FAILED: nop sqe");
        let _ = crate::syscall::handle_syscall(3, fd as u64, 0, 0, 0, 0, 0);
        return false;
    }
    let entered =
        crate::syscall::handle_syscall(426, fd as u64, 1, 1, IORING_ENTER_GETEVENTS as u64, 0, 0);
    if entered < 1 {
        serial_println!("[chrome] GATE_CHR iouring FAILED: enter rax={}", entered);
        let _ = crate::syscall::handle_syscall(3, fd as u64, 0, 0, 0, 0, 0);
        return false;
    }
    let Ok(cqes) = reap_cqes(ring, 8) else {
        serial_println!("[chrome] GATE_CHR iouring FAILED: reap");
        let _ = crate::syscall::handle_syscall(3, fd as u64, 0, 0, 0, 0, 0);
        return false;
    };
    let nop_ok = cqes.iter().any(|c| c.user_data == 1 && c.res == 0);
    let _ = crate::syscall::handle_syscall(3, fd as u64, 0, 0, 0, 0, 0);
    if !nop_ok {
        serial_println!("[chrome] GATE_CHR iouring FAILED: nop CQE");
        return false;
    }
    serial_println!(
        "[chrome] {} (setup fd={} NOP CQE, userfaultfd ENOSYS)",
        GATE_CHR_IOURING_MARKER,
        fd
    );
    true
}
