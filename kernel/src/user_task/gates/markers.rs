// ─── Gate B3/B4/B5 boot demonstration ───────────────────────────────────

/// Serial marker the integration test waits for once a child has been spawned,
/// run in Ring 3 via `execve`, exited, and reaped.
pub const GATE_B3_MARKER: &str = "GATE_B3 wait complete";
/// Fork child ran and parent reaped it.
pub const GATE_B4_MARKER: &str = "GATE_B4 fork complete";
/// SIGKILL / SIGSEGV / PTY SIGINT all took down a user task.
pub const GATE_B5_MARKER: &str = "GATE_B5 signals complete";
/// `/bin/sh` ran in Ring 3 with a kernel PTY as its controlling terminal.
pub const GATE_B6_MARKER: &str = "GATE_B6 sh complete";
/// Loopback UDP send/recv from a Ring 3 program.
pub const GATE_D1_MARKER: &str = "GATE_D1 loopback complete";
/// Custom SIGINT handler returned via rt_sigreturn.
pub const GATE_B7_MARKER: &str = "GATE_B7 sigreturn complete";
/// Timer IRQ switched a spinning Ring 3 task that never syscalled.
pub const GATE_B8_MARKER: &str = "GATE_B8 timer preempt complete";
/// Timer IRQ saved user GPRs + FXSAVE (spinner RBX magic survived).
pub const GATE_I2_MARKER: &str = "GATE_I2 irq gprs";
/// Application Processor ran a Ring 3 task (`getcpu` reported CPU 1).
pub const GATE_I3_MARKER: &str = "GATE_I3 ap ring3";
/// Ring 3 client presented a buffer; not an in-kernel WindowContentType app.
pub const GATE_F1_MARKER: &str = "GATE_F1 client isolated";

pub const GATE_K1_MARKER: &str = "GATE_K1 thread join";
pub const GATE_K3_MARKER: &str = "GATE_K3 init userspace";

pub const GATE_L1_MARKER: &str = "GATE_L1 tls fs";
pub const GATE_L4_MARKER: &str = "GATE_L4 enosys";

pub const GATE_M2_MARKER: &str = "GATE_M2 pipe";
pub const GATE_M3_MARKER: &str = "GATE_M3 futex";
pub const GATE_M4_MARKER: &str = "GATE_M4 enosys";

pub const GATE_N2_MARKER: &str = "GATE_N2 socketpair";
pub const GATE_N3_MARKER: &str = "GATE_N3 eventfd";
pub const GATE_N4_MARKER: &str = "GATE_N4 enosys";

pub const GATE_O2_MARKER: &str = "GATE_O2 epoll";
pub const GATE_O3_MARKER: &str = "GATE_O3 memfd";
pub const GATE_O4_MARKER: &str = "GATE_O4 enosys";

pub const GATE_P2_MARKER: &str = "GATE_P2 timerfd";
pub const GATE_P3_MARKER: &str = "GATE_P3 signalfd";
pub const GATE_P4_MARKER: &str = "GATE_P4 enosys";

pub const GATE_Q2_MARKER: &str = "GATE_Q2 poll";
pub const GATE_Q3_MARKER: &str = "GATE_Q3 inotify";
pub const GATE_Q4_MARKER: &str = "GATE_Q4 enosys";

pub const GATE_R2_MARKER: &str = "GATE_R2 splice";
pub const GATE_R3_MARKER: &str = "GATE_R3 flock";
pub const GATE_R4_MARKER: &str = "GATE_R4 enosys";

pub const GATE_S2_MARKER: &str = "GATE_S2 sendfile";
pub const GATE_S3_MARKER: &str = "GATE_S3 tee";
pub const GATE_S4_MARKER: &str = "GATE_S4 enosys";

pub const GATE_T2_MARKER: &str = "GATE_T2 copy_file_range";
pub const GATE_T3_MARKER: &str = "GATE_T3 vmsplice";
pub const GATE_T4_MARKER: &str = "GATE_T4 enosys";

pub const GATE_U2_MARKER: &str = "GATE_U2 xattr";
pub const GATE_U3_MARKER: &str = "GATE_U3 statx";
pub const GATE_U4_MARKER: &str = "GATE_U4 enosys";

pub const GATE_V2_MARKER: &str = "GATE_V2 fallocate";
pub const GATE_V3_MARKER: &str = "GATE_V3 utimensat";
pub const GATE_V4_MARKER: &str = "GATE_V4 enosys";

pub const GATE_W2_MARKER: &str = "GATE_W2 umask";
pub const GATE_W3_MARKER: &str = "GATE_W3 symlink";
pub const GATE_W4_MARKER: &str = "GATE_W4 enosys";

pub const GATE_X2_MARKER: &str = "GATE_X2 rename";
pub const GATE_X3_MARKER: &str = "GATE_X3 truncate";
pub const GATE_X4_MARKER: &str = "GATE_X4 enosys";

pub const GATE_Y2_MARKER: &str = "GATE_Y2 chown";
pub const GATE_Y3_MARKER: &str = "GATE_Y3 mkdir";
pub const GATE_Y4_MARKER: &str = "GATE_Y4 enosys";

pub const GATE_Z2_MARKER: &str = "GATE_Z2 unlink";
pub const GATE_Z3_MARKER: &str = "GATE_Z3 chdir";
pub const GATE_Z4_MARKER: &str = "GATE_Z4 enosys";

pub const GATE_AA2_MARKER: &str = "GATE_AA2 fchdir";
pub const GATE_AA3_MARKER: &str = "GATE_AA3 access";
pub const GATE_AA4_MARKER: &str = "GATE_AA4 enosys";

pub const GATE_AB2_MARKER: &str = "GATE_AB2 dup2";
pub const GATE_AB3_MARKER: &str = "GATE_AB3 uname";
pub const GATE_AB4_MARKER: &str = "GATE_AB4 enosys";

pub const GATE_AC2_MARKER: &str = "GATE_AC2 pread64";
pub const GATE_AC3_MARKER: &str = "GATE_AC3 getuid";
pub const GATE_AC4_MARKER: &str = "GATE_AC4 enosys";

pub const GATE_AD2_MARKER: &str = "GATE_AD2 pwrite64";
pub const GATE_AD3_MARKER: &str = "GATE_AD3 getgid";
pub const GATE_AD4_MARKER: &str = "GATE_AD4 enosys";

pub const GATE_AE2_MARKER: &str = "GATE_AE2 ftruncate";
pub const GATE_AE3_MARKER: &str = "GATE_AE3 geteuid";
pub const GATE_AE4_MARKER: &str = "GATE_AE4 enosys";

pub const GATE_AF2_MARKER: &str = "GATE_AF2 lseek";
pub const GATE_AF3_MARKER: &str = "GATE_AF3 getegid";
pub const GATE_AF4_MARKER: &str = "GATE_AF4 enosys";

pub const GATE_AG2_MARKER: &str = "GATE_AG2 fdatasync";
pub const GATE_AG3_MARKER: &str = "GATE_AG3 getppid";
pub const GATE_AG4_MARKER: &str = "GATE_AG4 enosys";

pub const GATE_AH2_MARKER: &str = "GATE_AH2 sync";
pub const GATE_AH3_MARKER: &str = "GATE_AH3 getpgid";
pub const GATE_AH4_MARKER: &str = "GATE_AH4 enosys";

pub const GATE_AI2_MARKER: &str = "GATE_AI2 fchown";
pub const GATE_AI3_MARKER: &str = "GATE_AI3 getsid";
pub const GATE_AI4_MARKER: &str = "GATE_AI4 enosys";

pub const GATE_AJ2_MARKER: &str = "GATE_AJ2 statfs";
pub const GATE_AJ3_MARKER: &str = "GATE_AJ3 setuid";
pub const GATE_AJ4_MARKER: &str = "GATE_AJ4 enosys";

pub const GATE_AK2_MARKER: &str = "GATE_AK2 getrlimit";
pub const GATE_AK3_MARKER: &str = "GATE_AK3 setgid";
pub const GATE_AK4_MARKER: &str = "GATE_AK4 enosys";

pub const GATE_AL2_MARKER: &str = "GATE_AL2 setrlimit";
pub const GATE_AL3_MARKER: &str = "GATE_AL3 setresuid";
pub const GATE_AL4_MARKER: &str = "GATE_AL4 enosys";

pub const GATE_AM2_MARKER: &str = "GATE_AM2 prlimit64";
pub const GATE_AM3_MARKER: &str = "GATE_AM3 setresgid";
pub const GATE_AM4_MARKER: &str = "GATE_AM4 enosys";

pub const GATE_AN2_MARKER: &str = "GATE_AN2 setreuid";
pub const GATE_AN3_MARKER: &str = "GATE_AN3 getgroups";
pub const GATE_AN4_MARKER: &str = "GATE_AN4 enosys";

pub const GATE_AO2_MARKER: &str = "GATE_AO2 setregid";
pub const GATE_AO3_MARKER: &str = "GATE_AO3 getresuid";
pub const GATE_AO4_MARKER: &str = "GATE_AO4 enosys";

pub const GATE_AP2_MARKER: &str = "GATE_AP2 getresgid";
pub const GATE_AP3_MARKER: &str = "GATE_AP3 setpgid";
pub const GATE_AP4_MARKER: &str = "GATE_AP4 enosys";
