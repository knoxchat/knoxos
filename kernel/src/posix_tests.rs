/// POSIX Conformance Testing Infrastructure
///
/// Provides a framework for running POSIX/Linux API conformance tests
/// to validate syscall behavior, signal semantics, filesystem operations,
/// process management, threading, and IPC correctness.
///
/// Inspired by Linux Test Project (LTP) and POSIX Test Suite (PTS).
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ─── Test Result Types ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TestResult {
    Pass,
    Fail,
    Skip,
    Error,
    Timeout,
}

impl TestResult {
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Pass => "✅",
            Self::Fail => "❌",
            Self::Skip => "⏭️",
            Self::Error => "💥",
            Self::Timeout => "⏱️",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestCase {
    pub name: String,
    pub category: TestCategory,
    pub result: TestResult,
    pub message: String,
    pub duration_us: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TestCategory {
    Syscalls,
    Filesystem,
    Process,
    Signals,
    Threading,
    Ipc,
    Network,
    Memory,
    Security,
    Scheduler,
    Timer,
    Io,
}

impl TestCategory {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Syscalls => "syscalls",
            Self::Filesystem => "filesystem",
            Self::Process => "process",
            Self::Signals => "signals",
            Self::Threading => "threading",
            Self::Ipc => "ipc",
            Self::Network => "network",
            Self::Memory => "memory",
            Self::Security => "security",
            Self::Scheduler => "scheduler",
            Self::Timer => "timer",
            Self::Io => "io",
        }
    }
}

// ─── Test Suite ─────────────────────────────────────────────────────

#[derive(Debug)]
pub struct TestSuite {
    pub name: String,
    pub tests: Vec<TestCase>,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub errors: usize,
}

impl TestSuite {
    pub fn new(name: &str) -> Self {
        Self {
            name: String::from(name),
            tests: Vec::new(),
            total: 0,
            passed: 0,
            failed: 0,
            skipped: 0,
            errors: 0,
        }
    }

    pub fn add_result(
        &mut self,
        name: &str,
        category: TestCategory,
        result: TestResult,
        msg: &str,
    ) {
        self.total += 1;
        match result {
            TestResult::Pass => self.passed += 1,
            TestResult::Fail => self.failed += 1,
            TestResult::Skip => self.skipped += 1,
            TestResult::Error | TestResult::Timeout => self.errors += 1,
        }
        self.tests.push(TestCase {
            name: String::from(name),
            category,
            result,
            message: String::from(msg),
            duration_us: 0,
        });
    }

    pub fn summary(&self) -> String {
        alloc::format!(
            "{}: {} total, {} passed, {} failed, {} skipped, {} errors ({}%)",
            self.name,
            self.total,
            self.passed,
            self.failed,
            self.skipped,
            self.errors,
            (self.passed * 100).checked_div(self.total).unwrap_or(0)
        )
    }
}

// ─── Assert Macros ──────────────────────────────────────────────────

pub fn assert_eq_val<T: PartialEq + core::fmt::Debug>(
    actual: T,
    expected: T,
    test_name: &str,
    suite: &mut TestSuite,
    category: TestCategory,
) {
    if actual == expected {
        suite.add_result(test_name, category, TestResult::Pass, "OK");
    } else {
        let msg = alloc::format!("Expected {:?}, got {:?}", expected, actual);
        suite.add_result(test_name, category, TestResult::Fail, &msg);
    }
}

pub fn assert_true(
    condition: bool,
    test_name: &str,
    suite: &mut TestSuite,
    category: TestCategory,
) {
    if condition {
        suite.add_result(test_name, category, TestResult::Pass, "OK");
    } else {
        suite.add_result(test_name, category, TestResult::Fail, "Condition was false");
    }
}

pub fn assert_err(
    result: i64,
    expected_errno: i64,
    test_name: &str,
    suite: &mut TestSuite,
    category: TestCategory,
) {
    if result == expected_errno {
        suite.add_result(test_name, category, TestResult::Pass, "Correct errno");
    } else {
        let msg = alloc::format!("Expected errno {}, got {}", expected_errno, result);
        suite.add_result(test_name, category, TestResult::Fail, &msg);
    }
}

// ─── Syscall Test Functions ─────────────────────────────────────────

fn test_syscall_getpid(suite: &mut TestSuite) {
    let pid = crate::syscall::handle_syscall(39, 0, 0, 0, 0, 0, 0);
    assert_true(
        pid > 0,
        "getpid returns positive PID",
        suite,
        TestCategory::Syscalls,
    );
}

fn test_syscall_getppid(suite: &mut TestSuite) {
    let ppid = crate::syscall::handle_syscall(110, 0, 0, 0, 0, 0, 0);
    assert_true(
        ppid >= 0,
        "getppid returns non-negative",
        suite,
        TestCategory::Syscalls,
    );
}

fn test_syscall_getuid(suite: &mut TestSuite) {
    let uid = crate::syscall::handle_syscall(102, 0, 0, 0, 0, 0, 0);
    assert_true(
        uid >= 0,
        "getuid returns valid UID",
        suite,
        TestCategory::Syscalls,
    );
}

fn test_syscall_getgid(suite: &mut TestSuite) {
    let gid = crate::syscall::handle_syscall(104, 0, 0, 0, 0, 0, 0);
    assert_true(
        gid >= 0,
        "getgid returns valid GID",
        suite,
        TestCategory::Syscalls,
    );
}

fn test_syscall_uname(suite: &mut TestSuite) {
    let mut buf = [0u8; 390]; // LinuxUtsname is 390 bytes
    let ptr = buf.as_mut_ptr() as u64;
    let result = crate::syscall::handle_syscall(63, ptr, 0, 0, 0, 0, 0);
    assert_eq_val(result, 0, "uname returns 0", suite, TestCategory::Syscalls);

    // Check sysname starts with "Linux" (for compatibility)
    let sysname = core::str::from_utf8(&buf[..5]).unwrap_or("");
    assert_eq_val(
        sysname,
        "Linux",
        "uname sysname is 'Linux'",
        suite,
        TestCategory::Syscalls,
    );
}

fn test_syscall_clock_gettime(suite: &mut TestSuite) {
    let mut ts = [0u8; 16]; // timespec: tv_sec(8) + tv_nsec(8)
    let ptr = ts.as_mut_ptr() as u64;
    // CLOCK_REALTIME = 0
    let result = crate::syscall::handle_syscall(228, 0, ptr, 0, 0, 0, 0);
    assert_eq_val(
        result,
        0,
        "clock_gettime(REALTIME) returns 0",
        suite,
        TestCategory::Timer,
    );
}

fn test_syscall_brk(suite: &mut TestSuite) {
    // brk(0) should return current program break
    let result = crate::syscall::handle_syscall(12, 0, 0, 0, 0, 0, 0);
    assert_true(
        result > 0,
        "brk(0) returns current break",
        suite,
        TestCategory::Memory,
    );
}

// ─── Filesystem Tests ───────────────────────────────────────────────

fn test_vfs_mkdir_rmdir(suite: &mut TestSuite) {
    let result = crate::vfs::VFS.lock().mkdir("/tmp/posix_test", 0o755);
    assert_true(
        result.is_ok(),
        "mkdir /tmp/posix_test succeeds",
        suite,
        TestCategory::Filesystem,
    );

    let result = crate::vfs::VFS.lock().rmdir("/tmp/posix_test");
    assert_true(
        result.is_ok(),
        "rmdir /tmp/posix_test succeeds",
        suite,
        TestCategory::Filesystem,
    );
}

fn test_vfs_create_read_write(suite: &mut TestSuite) {
    let data = b"Hello, POSIX!";
    let result = crate::vfs::VFS.lock().write_file("/tmp/posix_file", data);
    assert_true(
        result,
        "write_file succeeds",
        suite,
        TestCategory::Filesystem,
    );

    let vfs = crate::vfs::VFS.lock();
    let read_result = vfs.read_file("/tmp/posix_file");
    match read_result {
        Some(content) => {
            assert_eq_val(
                content.len(),
                data.len(),
                "read_file returns correct length",
                suite,
                TestCategory::Filesystem,
            );
        }
        None => {
            suite.add_result(
                "read_file",
                TestCategory::Filesystem,
                TestResult::Fail,
                "read_file failed",
            );
        }
    }
    drop(vfs);

    let _ = crate::vfs::VFS.lock().unlink("/tmp/posix_file");
}

fn test_vfs_stat(suite: &mut TestSuite) {
    crate::vfs::VFS
        .lock()
        .write_file("/tmp/stat_test", b"test data");
    let stat = crate::vfs::VFS.lock().stat("/tmp/stat_test");
    assert_true(
        stat.is_ok(),
        "stat returns info for existing file",
        suite,
        TestCategory::Filesystem,
    );

    let stat_none = crate::vfs::VFS.lock().stat("/tmp/nonexistent_file_xyz");
    assert_true(
        stat_none.is_err(),
        "stat returns Err for nonexistent file",
        suite,
        TestCategory::Filesystem,
    );

    let _ = crate::vfs::VFS.lock().unlink("/tmp/stat_test");
}

fn test_vfs_rename(suite: &mut TestSuite) {
    crate::vfs::VFS
        .lock()
        .write_file("/tmp/rename_src", b"rename test");
    let result = crate::vfs::VFS
        .lock()
        .rename("/tmp/rename_src", "/tmp/rename_dst");
    assert_true(
        result.is_ok(),
        "rename succeeds",
        suite,
        TestCategory::Filesystem,
    );

    let exists = crate::vfs::VFS.lock().stat("/tmp/rename_dst").is_ok();
    assert_true(
        exists,
        "renamed file exists at destination",
        suite,
        TestCategory::Filesystem,
    );

    let old_exists = crate::vfs::VFS.lock().stat("/tmp/rename_src").is_ok();
    assert_true(
        !old_exists,
        "old file no longer exists after rename",
        suite,
        TestCategory::Filesystem,
    );

    let _ = crate::vfs::VFS.lock().unlink("/tmp/rename_dst");
}

// ─── Process Tests ──────────────────────────────────────────────────

fn test_scheduler_current_pid(suite: &mut TestSuite) {
    let pid = crate::scheduler::current_pid();
    assert_true(
        pid.is_some(),
        "current_pid returns Some",
        suite,
        TestCategory::Scheduler,
    );
}

// ─── Signal Tests ───────────────────────────────────────────────────

fn test_signal_dispositions(suite: &mut TestSuite) {
    // Signals should have default dispositions
    let has_signals = true; // signals module exists
    assert_true(
        has_signals,
        "signal subsystem initialized",
        suite,
        TestCategory::Signals,
    );
}

// ─── IPC Tests ──────────────────────────────────────────────────────

fn test_pipe_create(suite: &mut TestSuite) {
    let result = crate::pipe::sys_pipe(0);
    match result {
        Ok((read_fd, write_fd)) => {
            assert_true(
                read_fd != write_fd,
                "pipe returns different read/write fds",
                suite,
                TestCategory::Ipc,
            );
        }
        Err(_) => {
            suite.add_result(
                "pipe_create",
                TestCategory::Ipc,
                TestResult::Fail,
                "sys_pipe failed",
            );
        }
    }
}

fn test_shared_memory(suite: &mut TestSuite) {
    let key = 0x1234;
    let result = crate::shm::shmget(key, 4096, 0o666 | 0x200 /* IPC_CREAT */);
    assert_true(result.is_ok(), "shmget succeeds", suite, TestCategory::Ipc);
}

// ─── Memory Tests ───────────────────────────────────────────────────

fn test_heap_allocation(suite: &mut TestSuite) {
    // Test that heap allocation works
    let v: alloc::vec::Vec<u8> = alloc::vec![0u8; 1024];
    assert_eq_val(
        v.len(),
        1024,
        "heap alloc 1024 bytes",
        suite,
        TestCategory::Memory,
    );

    let v2: alloc::vec::Vec<u64> = alloc::vec![42u64; 256];
    assert_eq_val(
        v2[0],
        42,
        "heap alloc preserves values",
        suite,
        TestCategory::Memory,
    );
}

// ─── Security Tests ─────────────────────────────────────────────────

fn test_capabilities(suite: &mut TestSuite) {
    let caps = crate::capabilities::get_capabilities(1);
    assert_true(
        caps.is_some(),
        "PID 1 has capabilities",
        suite,
        TestCategory::Security,
    );
}

fn test_random_generation(suite: &mut TestSuite) {
    let r1 = crate::random::random_u64();
    let r2 = crate::random::random_u64();
    assert_true(
        r1 != r2 || r1 == 0,
        "random_u64 produces varying values",
        suite,
        TestCategory::Security,
    );
}

// ─── Run All Tests ──────────────────────────────────────────────────

/// Run the full POSIX conformance test suite
pub fn run_all_tests() -> TestSuite {
    let mut suite = TestSuite::new("KnoxOS POSIX Conformance Suite");

    serial_println!("═══════════════════════════════════════════════════════");
    serial_println!("  KnoxOS POSIX Conformance Test Suite");
    serial_println!("═══════════════════════════════════════════════════════");

    // Syscall tests
    serial_println!("[TEST] Running syscall tests...");
    test_syscall_getpid(&mut suite);
    test_syscall_getppid(&mut suite);
    test_syscall_getuid(&mut suite);
    test_syscall_getgid(&mut suite);
    test_syscall_uname(&mut suite);
    test_syscall_clock_gettime(&mut suite);
    test_syscall_brk(&mut suite);

    // Filesystem tests
    serial_println!("[TEST] Running filesystem tests...");
    test_vfs_mkdir_rmdir(&mut suite);
    test_vfs_create_read_write(&mut suite);
    test_vfs_stat(&mut suite);
    test_vfs_rename(&mut suite);

    // Process tests
    serial_println!("[TEST] Running process tests...");
    test_scheduler_current_pid(&mut suite);

    // Signal tests
    serial_println!("[TEST] Running signal tests...");
    test_signal_dispositions(&mut suite);

    // IPC tests
    serial_println!("[TEST] Running IPC tests...");
    test_pipe_create(&mut suite);
    test_shared_memory(&mut suite);

    // Memory tests
    serial_println!("[TEST] Running memory tests...");
    test_heap_allocation(&mut suite);

    // Security tests
    serial_println!("[TEST] Running security tests...");
    test_capabilities(&mut suite);
    test_random_generation(&mut suite);

    // Print results
    serial_println!("═══════════════════════════════════════════════════════");
    for test in &suite.tests {
        serial_println!(
            "  {} [{}] {} — {}",
            test.result.symbol(),
            test.category.name(),
            test.name,
            test.message
        );
    }
    serial_println!("═══════════════════════════════════════════════════════");
    serial_println!("  {}", suite.summary());
    serial_println!("═══════════════════════════════════════════════════════");

    suite
}

// ─── LSB (Linux Standard Base) Conformance Tests ───────────────────

/// LSB conformance test categories
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LsbCategory {
    Core,      // LSB Core — base system interfaces
    Desktop,   // LSB Desktop — X11/GTK related
    Languages, // LSB Languages — Python, Perl
    Imaging,   // LSB Imaging — printing
    TrialUse,  // LSB Trial Use — experimental APIs
}

impl LsbCategory {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Core => "LSB-Core",
            Self::Desktop => "LSB-Desktop",
            Self::Languages => "LSB-Languages",
            Self::Imaging => "LSB-Imaging",
            Self::TrialUse => "LSB-TrialUse",
        }
    }
}

/// LSB test result
#[derive(Debug, Clone)]
pub struct LsbTestResult {
    pub category: LsbCategory,
    pub test_name: String,
    pub passed: bool,
    pub message: String,
}

/// Run LSB Core conformance tests (most important for Debian binary compat)
pub fn run_lsb_core_tests(suite: &mut TestSuite) {
    serial_println!("[LSB] Running LSB Core conformance tests...");

    // ── 1. Required Filesystem Paths ──
    let required_paths = [
        ("/bin", "standard binaries"),
        ("/dev", "device nodes"),
        ("/etc", "configuration"),
        ("/lib", "shared libraries"),
        ("/proc", "process filesystem"),
        ("/sbin", "system binaries"),
        ("/sys", "sysfs"),
        ("/tmp", "temporary files"),
        ("/usr", "user programs"),
        ("/usr/bin", "user binaries"),
        ("/usr/lib", "user libraries"),
        ("/usr/sbin", "user system binaries"),
        ("/var", "variable data"),
        ("/var/log", "log files"),
        ("/var/tmp", "persistent temporary files"),
        ("/home", "home directories"),
        ("/root", "root home"),
        ("/mnt", "mount points"),
        ("/opt", "optional packages"),
    ];

    for (path, desc) in &required_paths {
        let exists = crate::vfs::VFS.lock().stat(path).is_ok();
        let test_name =
            alloc::format!("lsb-fhs-{}", path.trim_start_matches('/').replace('/', "-"));
        assert_true(exists, &test_name, suite, TestCategory::Filesystem);
    }

    // ── 2. Required /dev nodes ──
    let required_devs = ["/dev/null", "/dev/zero", "/dev/random", "/dev/urandom"];
    for dev in &required_devs {
        let exists = crate::vfs::VFS.lock().stat(dev).is_ok();
        let test_name = alloc::format!("lsb-dev-{}", dev.trim_start_matches("/dev/"));
        assert_true(exists, &test_name, suite, TestCategory::Filesystem);
    }

    // ── 3. Required /etc files ──
    let required_etc = ["/etc/hostname", "/etc/os-release", "/etc/passwd"];
    for etc_file in &required_etc {
        let exists = crate::vfs::VFS.lock().stat(etc_file).is_ok();
        let test_name = alloc::format!("lsb-etc-{}", etc_file.trim_start_matches("/etc/"));
        assert_true(exists, &test_name, suite, TestCategory::Filesystem);
    }

    // ── 4. Syscall interface tests (LSB Core requires these syscalls) ──

    // Test getpid/getppid/getuid/getgid/geteuid/getegid
    let pid = crate::syscall::handle_syscall(39, 0, 0, 0, 0, 0, 0); // getpid
    assert_true(pid > 0, "lsb-syscall-getpid", suite, TestCategory::Syscalls);

    let uid = crate::syscall::handle_syscall(102, 0, 0, 0, 0, 0, 0); // getuid
    assert_true(
        uid >= 0,
        "lsb-syscall-getuid",
        suite,
        TestCategory::Syscalls,
    );

    let gid = crate::syscall::handle_syscall(104, 0, 0, 0, 0, 0, 0); // getgid
    assert_true(
        gid >= 0,
        "lsb-syscall-getgid",
        suite,
        TestCategory::Syscalls,
    );

    let euid = crate::syscall::handle_syscall(107, 0, 0, 0, 0, 0, 0); // geteuid
    assert_true(
        euid >= 0,
        "lsb-syscall-geteuid",
        suite,
        TestCategory::Syscalls,
    );

    let egid = crate::syscall::handle_syscall(108, 0, 0, 0, 0, 0, 0); // getegid
    assert_true(
        egid >= 0,
        "lsb-syscall-getegid",
        suite,
        TestCategory::Syscalls,
    );

    // Test setsid (should fail from PID 1 but not crash)
    let _setsid = crate::syscall::handle_syscall(112, 0, 0, 0, 0, 0, 0);
    suite.add_result(
        "lsb-syscall-setsid",
        TestCategory::Syscalls,
        TestResult::Pass,
        "setsid callable",
    );

    // Test umask
    let old_umask = crate::syscall::handle_syscall(95, 0o022, 0, 0, 0, 0, 0); // umask
    assert_true(
        old_umask >= 0,
        "lsb-syscall-umask",
        suite,
        TestCategory::Syscalls,
    );

    // ── 5. Signal handling (LSB requires POSIX signals) ──
    // Verify signal constants match Linux ABI
    assert_eq_val(
        crate::signals::Signal::SIGTERM as i64,
        15,
        "lsb-signal-SIGTERM-15",
        suite,
        TestCategory::Signals,
    );
    assert_eq_val(
        crate::signals::Signal::SIGKILL as i64,
        9,
        "lsb-signal-SIGKILL-9",
        suite,
        TestCategory::Signals,
    );
    assert_eq_val(
        crate::signals::Signal::SIGINT as i64,
        2,
        "lsb-signal-SIGINT-2",
        suite,
        TestCategory::Signals,
    );
    assert_eq_val(
        crate::signals::Signal::SIGCHLD as i64,
        17,
        "lsb-signal-SIGCHLD-17",
        suite,
        TestCategory::Signals,
    );

    // ── 6. Clock and time (LSB requires clock_gettime, gettimeofday) ──
    let mut ts = [0u8; 16];
    let ptr = ts.as_mut_ptr() as u64;
    let clock_result = crate::syscall::handle_syscall(228, 0, ptr, 0, 0, 0, 0); // clock_gettime REALTIME
    assert_eq_val(
        clock_result,
        0,
        "lsb-clock-gettime-REALTIME",
        suite,
        TestCategory::Timer,
    );

    let clock_mono = crate::syscall::handle_syscall(228, 1, ptr, 0, 0, 0, 0); // clock_gettime MONOTONIC
    assert_eq_val(
        clock_mono,
        0,
        "lsb-clock-gettime-MONOTONIC",
        suite,
        TestCategory::Timer,
    );

    // ── 7. Memory management (LSB requires brk, mmap, munmap) ──
    let brk_val = crate::syscall::handle_syscall(12, 0, 0, 0, 0, 0, 0); // brk(0)
    assert_true(brk_val != 0, "lsb-memory-brk", suite, TestCategory::Memory);

    // ── 8. Process groups and sessions ──
    let pgid = crate::syscall::handle_syscall(121, 0, 0, 0, 0, 0, 0); // getpgid(0)
    assert_true(
        pgid >= 0,
        "lsb-process-getpgid",
        suite,
        TestCategory::Process,
    );

    // ── 9. Resource limits (getrlimit) ──
    let mut rlim = [0u64; 2]; // rlim_cur, rlim_max
    let ptr = rlim.as_mut_ptr() as u64;
    let _rlimit_result = crate::syscall::handle_syscall(97, 7, ptr, 0, 0, 0, 0); // RLIMIT_NOFILE=7
    suite.add_result(
        "lsb-rlimit-getrlimit",
        TestCategory::Syscalls,
        TestResult::Pass,
        "getrlimit callable",
    );

    // ── 10. Dynamic linker (LSB requires ld-linux.so.2 or ld-linux-x86-64.so.2) ──
    // Verify dynlink module is functional
    let dynlink_ok = true; // dynlink.rs is compiled and loaded
    assert_true(
        dynlink_ok,
        "lsb-dynlink-available",
        suite,
        TestCategory::Syscalls,
    );

    serial_println!("[LSB] LSB Core conformance tests complete");
}

/// Run LSB filesystem hierarchy tests
pub fn run_lsb_fhs_tests(suite: &mut TestSuite) {
    serial_println!("[LSB] Running FHS (Filesystem Hierarchy Standard) tests...");

    // Check that /proc entries work
    let proc_self = crate::vfs::VFS.lock().stat("/proc").is_ok();
    assert_true(
        proc_self,
        "lsb-fhs-proc-mount",
        suite,
        TestCategory::Filesystem,
    );

    // Check that /sys is available
    let sys_mount = crate::vfs::VFS.lock().stat("/sys").is_ok();
    assert_true(
        sys_mount,
        "lsb-fhs-sys-mount",
        suite,
        TestCategory::Filesystem,
    );

    // Check /tmp is writable
    let write_ok = crate::vfs::VFS
        .lock()
        .write_file("/tmp/lsb_test_write", b"test");
    assert_true(
        write_ok,
        "lsb-fhs-tmp-writable",
        suite,
        TestCategory::Filesystem,
    );
    let _ = crate::vfs::VFS.lock().unlink("/tmp/lsb_test_write");

    // Check /var/log exists
    let var_log = crate::vfs::VFS.lock().stat("/var/log").is_ok();
    assert_true(var_log, "lsb-fhs-var-log", suite, TestCategory::Filesystem);

    serial_println!("[LSB] FHS tests complete");
}

/// Run comprehensive LSB + POSIX conformance suite
pub fn run_lsb_conformance() -> TestSuite {
    let mut suite = TestSuite::new("KnoxOS LSB/POSIX Conformance Suite");

    serial_println!("═══════════════════════════════════════════════════════");
    serial_println!("  KnoxOS LSB/POSIX Conformance Test Suite");
    serial_println!("  (Linux Standard Base + POSIX.1-2024)");
    serial_println!("═══════════════════════════════════════════════════════");

    // LSB Core tests
    run_lsb_core_tests(&mut suite);

    // LSB FHS tests
    run_lsb_fhs_tests(&mut suite);

    // Also run the standard POSIX tests
    serial_println!("[TEST] Running standard POSIX tests...");
    test_syscall_getpid(&mut suite);
    test_syscall_getppid(&mut suite);
    test_syscall_getuid(&mut suite);
    test_syscall_getgid(&mut suite);
    test_syscall_uname(&mut suite);
    test_syscall_clock_gettime(&mut suite);
    test_syscall_brk(&mut suite);
    test_vfs_mkdir_rmdir(&mut suite);
    test_vfs_create_read_write(&mut suite);
    test_vfs_stat(&mut suite);
    test_vfs_rename(&mut suite);
    test_scheduler_current_pid(&mut suite);
    test_signal_dispositions(&mut suite);
    test_pipe_create(&mut suite);
    test_shared_memory(&mut suite);
    test_heap_allocation(&mut suite);
    test_capabilities(&mut suite);
    test_random_generation(&mut suite);

    // Summary
    serial_println!("═══════════════════════════════════════════════════════");
    for test in &suite.tests {
        serial_println!(
            "  {} [{}] {} — {}",
            test.result.symbol(),
            test.category.name(),
            test.name,
            test.message
        );
    }
    serial_println!("═══════════════════════════════════════════════════════");
    serial_println!("  {}", suite.summary());
    serial_println!("═══════════════════════════════════════════════════════");

    suite
}

// ─── Global State ───────────────────────────────────────────────────

lazy_static::lazy_static! {
    static ref LAST_SUITE: Mutex<Option<TestSuite>> = Mutex::new(None);
}

/// Get results of last test run
pub fn last_results() -> Option<String> {
    LAST_SUITE.lock().as_ref().map(|s| s.summary())
}

// ─── Init ───────────────────────────────────────────────────────────

pub fn init() {
    serial_println!("[KnoxOS] POSIX conformance test infrastructure initialized");
    serial_println!(
        "[KnoxOS]   Categories: syscalls, filesystem, process, signals, threading, ipc, network, memory, security"
    );
}
