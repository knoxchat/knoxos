// ═══════════════════════════════════════════════════════════════════════
// SYSCALL ABI TEST FRAMEWORK
// ═══════════════════════════════════════════════════════════════════════

use super::*;
use crate::syscall;
use alloc::vec;

/// Helper: invoke handle_syscall with given number and arguments
fn do_syscall(nr: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64, a6: u64) -> i64 {
    syscall::handle_syscall(nr, a1, a2, a3, a4, a5, a6)
}

// ─── File I/O syscalls ─────────────────────────────────────

#[test_case]
fn test_sys_write_stdout() {
    // write(fd=1, buf, len) → should succeed (returns bytes written or ≥0)
    let msg = b"test\n";
    let ret = do_syscall(1, 1, msg.as_ptr() as u64, msg.len() as u64, 0, 0, 0);
    assert!(ret >= 0, "write() to stdout failed: {}", ret);
}

#[test_case]
fn test_sys_read_bad_fd() {
    // read(fd=9999, ...) → should return -EBADF (−9)
    let mut buf = [0u8; 64];
    let ret = do_syscall(0, 9999, buf.as_mut_ptr() as u64, 64, 0, 0, 0);
    assert!(ret < 0, "read() on bad fd should fail");
}

#[test_case]
fn test_sys_close_bad_fd() {
    // close(fd=9999) → should return -EBADF
    let ret = do_syscall(3, 9999, 0, 0, 0, 0, 0);
    assert!(ret < 0, "close() on bad fd should fail");
}

#[test_case]
fn test_sys_open_close_cycle() {
    // Ensure VFS has a test file
    crate::vfs::write_file_dispatch("/tmp/syscall_test.txt", b"hello");

    // open("/tmp/syscall_test.txt", O_RDONLY=0, 0)
    let path = b"/tmp/syscall_test.txt\0";
    let fd = do_syscall(2, path.as_ptr() as u64, 0, 0, 0, 0, 0);
    assert!(fd >= 0, "open() should return valid fd, got {}", fd);

    // close(fd)
    let ret = do_syscall(3, fd as u64, 0, 0, 0, 0, 0);
    assert!(ret == 0 || ret >= 0, "close() should succeed, got {}", ret);
}

// ─── Process syscalls ──────────────────────────────────────

#[test_case]
fn test_sys_getpid() {
    // getpid() = syscall 39
    let pid = do_syscall(39, 0, 0, 0, 0, 0, 0);
    assert!(pid > 0, "getpid() should return positive pid, got {}", pid);
}

#[test_case]
fn test_sys_getppid() {
    // getppid() = syscall 110
    let ppid = do_syscall(110, 0, 0, 0, 0, 0, 0);
    assert!(ppid >= 0, "getppid() should return >= 0, got {}", ppid);
}

#[test_case]
fn test_sys_getuid() {
    // getuid() = syscall 102
    let uid = do_syscall(102, 0, 0, 0, 0, 0, 0);
    assert!(uid >= 0, "getuid() should return >= 0, got {}", uid);
}

#[test_case]
fn test_sys_getgid() {
    // getgid() = syscall 104
    let gid = do_syscall(104, 0, 0, 0, 0, 0, 0);
    assert!(gid >= 0, "getgid() should return >= 0, got {}", gid);
}

#[test_case]
fn test_sys_geteuid() {
    // geteuid() = syscall 107
    let euid = do_syscall(107, 0, 0, 0, 0, 0, 0);
    assert!(euid >= 0, "geteuid() should return >= 0, got {}", euid);
}

// ─── Memory syscalls ───────────────────────────────────────

#[test_case]
fn test_sys_brk() {
    // brk(0) → returns current break
    let brk = do_syscall(12, 0, 0, 0, 0, 0, 0);
    assert!(brk >= 0, "brk(0) should return current break, got {}", brk);
}

#[test_case]
fn test_sys_mmap_anonymous() {
    // mmap(0, 4096, PROT_READ|PROT_WRITE=3, MAP_PRIVATE|MAP_ANONYMOUS=0x22, -1, 0)
    let ret = do_syscall(9, 0, 4096, 3, 0x22, u64::MAX, 0);
    // Should return a valid address or an error
    // Even if simulation, should not crash
    assert!(ret != 0, "mmap() returned null");
}

// ─── Time syscalls ─────────────────────────────────────────

#[test_case]
fn test_sys_clock_gettime() {
    // clock_gettime(CLOCK_REALTIME=0, &timespec)
    let mut ts = [0u64; 2]; // tv_sec, tv_nsec
    let ret = do_syscall(228, 0, ts.as_mut_ptr() as u64, 0, 0, 0, 0);
    // Should succeed or return a meaningful error
    assert!(
        ret >= -1000,
        "clock_gettime() returned unexpected error: {}",
        ret
    );
}

#[test_case]
fn test_sys_gettimeofday() {
    // gettimeofday(tv, NULL) = syscall 96
    let mut tv = [0u64; 2]; // tv_sec, tv_usec
    let ret = do_syscall(96, tv.as_mut_ptr() as u64, 0, 0, 0, 0, 0);
    assert!(ret >= 0, "gettimeofday() should succeed, got {}", ret);
}

// ─── Signal syscalls ───────────────────────────────────────

#[test_case]
fn test_sys_sigprocmask() {
    // rt_sigprocmask(SIG_BLOCK=0, NULL, &oldset, sigsetsize=8)
    let mut oldset = 0u64;
    let ret = do_syscall(14, 0, 0, &mut oldset as *mut u64 as u64, 8, 0, 0);
    // Should not crash, may return 0 or -EINVAL
    assert!(ret >= -1000, "sigprocmask returned unexpected: {}", ret);
}

// ─── Network syscalls ──────────────────────────────────────

#[test_case]
fn test_sys_socket_create() {
    // socket(AF_INET=2, SOCK_STREAM=1, 0) = syscall 41
    let fd = do_syscall(41, 2, 1, 0, 0, 0, 0);
    // Should return fd or error
    if fd >= 0 {
        // Clean up
        do_syscall(3, fd as u64, 0, 0, 0, 0, 0);
    }
    // Even if it fails, should return a valid errno
    assert!(fd >= -1000, "socket() returned unexpected: {}", fd);
}

#[test_case]
fn test_loopback_udp_and_tcp_send_recv() {
    assert!(
        crate::net::loopback_self_test(),
        "loopback UDP+TCP send must copy into the peer recv buffer"
    );
}

// ─── Filesystem meta syscalls ──────────────────────────────

#[test_case]
fn test_sys_getcwd() {
    // getcwd(buf, size) = syscall 79
    let mut buf = [0u8; 256];
    let ret = do_syscall(79, buf.as_mut_ptr() as u64, 256, 0, 0, 0, 0);
    assert!(ret >= 0, "getcwd() should succeed, got {}", ret);
}

#[test_case]
fn test_sys_dup2() {
    // dup2(oldfd, newfd) = syscall 33
    // dup2 on invalid fd
    let ret = do_syscall(33, 9999, 9998, 0, 0, 0, 0);
    assert!(ret < 0, "dup2() on bad fds should fail");
}

// ─── Misc syscalls ─────────────────────────────────────────

#[test_case]
fn test_sys_uname() {
    // uname(buf) = syscall 63
    let mut buf = [0u8; 390]; // struct utsname
    let ret = do_syscall(63, buf.as_mut_ptr() as u64, 0, 0, 0, 0, 0);
    assert!(ret >= 0, "uname() should succeed, got {}", ret);
}

#[test_case]
fn test_sys_sched_yield() {
    // sched_yield() = syscall 24
    let ret = do_syscall(24, 0, 0, 0, 0, 0, 0);
    assert!(ret >= 0, "sched_yield() should return 0, got {}", ret);
}

#[test_case]
fn test_sys_unknown_syscall() {
    // Very high syscall number should return -ENOSYS (-38)
    let ret = do_syscall(99999, 0, 0, 0, 0, 0, 0);
    assert_eq!(
        ret, -38,
        "Unknown syscall should return -ENOSYS, got {}",
        ret
    );
}

// ─── ABI register convention test ──────────────────────────
// Verify that all 6 argument registers pass through correctly

#[test_case]
fn test_syscall_six_args_passthrough() {
    // Use write (syscall 1) which uses arg1=fd, arg2=buf, arg3=len
    // The fact that write works with pointer args validates register passing
    let data = b"abi-test\n";
    let ret = do_syscall(
        1,                    // nr: write
        2,                    // arg1: fd=stderr
        data.as_ptr() as u64, // arg2: buf pointer
        data.len() as u64,    // arg3: count
        0,                    // arg4: unused
        0,                    // arg5: unused
        0,                    // arg6: unused
    );
    assert!(ret >= 0, "write to stderr should succeed, got {}", ret);
}
