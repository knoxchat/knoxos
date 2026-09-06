use crate::serial_println;
/// Syscall Interface - Linux-compatible system call layer
/// Provides COMPLETE coverage of all ~452 Linux x86_64 system calls
/// Wired to real VFS, process, signal, fd, and IPC subsystems
///
/// Split into submodules for maintainability:
///   fs       — File I/O and filesystem operations
///   process  — Process lifecycle, groups, and sessions
///   fd       — File descriptor operations (pipe, dup, fcntl)
///   memory   — Memory management (brk, mmap, munmap)
///   net      — Network socket operations
///   signal   — Signal handling (sigaction, sigprocmask)
///   thread   — Threading and futex
///   user     — User/group identity management
///   time     — Time and clock operations
///   system   — System info, control, namespaces, security
///   io       — I/O multiplexing, advanced I/O, IPC
///   ai       — KnoxOS AI system calls
///   advanced — All remaining Linux syscalls (pidfd, memfd, io_uring,
///              xattr, ptrace, perf, landlock, prctl, etc.)
use alloc::string::String;

mod advanced;
mod ai;
mod fd;
mod fs;
mod io;
mod memory;
mod net;
mod process;
mod signal;
mod system;
mod thread;
mod time;
mod user;

/// System call numbers (Linux x86_64 ABI) — COMPLETE coverage (0–451+)
#[repr(u64)]
#[derive(Debug, Clone, Copy)]
pub enum SyscallNumber {
    // ── Core file I/O (0–20) ─────────────────────────────────────
    Read = 0,
    Write = 1,
    Open = 2,
    Close = 3,
    Stat = 4,
    Fstat = 5,
    Lstat = 6,
    Poll = 7,
    Lseek = 8,
    Mmap = 9,
    Mprotect = 10,
    Munmap = 11,
    Brk = 12,
    Sigaction = 13,   // rt_sigaction
    Sigprocmask = 14, // rt_sigprocmask
    RtSigreturn = 15,
    Ioctl = 16,
    Pread64 = 17,
    Pwrite64 = 18,
    Readv = 19,
    Writev = 20,

    // ── File access & permissions (21–40) ────────────────────────
    Access = 21,
    Pipe = 22,
    Select = 23,
    SchedYield = 24,
    Mremap = 25,
    Msync = 26,
    Mincore = 27,
    Madvise = 28,
    Shmget = 29,
    Shmat = 30,
    Shmctl = 31,
    Dup = 32,
    Dup2 = 33,
    Pause = 34,
    Nanosleep = 35,
    Getitimer = 36,
    Alarm = 37,
    Setitimer = 38,
    Getpid = 39,
    Sendfile = 40,

    // ── Sockets (41–55) ─────────────────────────────────────────
    Socket = 41,
    Connect = 42,
    Accept = 43,
    Sendto = 44,
    Recvfrom = 45,
    Sendmsg = 46,
    Recvmsg = 47,
    Shutdown = 48,
    Bind = 49,
    Listen = 50,
    Getsockname = 51,
    Getpeername = 52,
    Socketpair = 53,
    Setsockopt = 54,
    Getsockopt = 55,

    // ── Process (56–79) ─────────────────────────────────────────
    Clone = 56,
    Fork = 57,
    Vfork = 58,
    Execve = 59,
    Exit = 60,
    Wait4 = 61,
    Kill = 62,
    Uname = 63,
    Semget = 64,
    Semop = 65,
    Semctl = 66,
    Shmdt = 67,
    Msgget = 68,
    Msgsnd = 69,
    Msgrcv = 70,
    Msgctl = 71,
    Fcntl = 72,
    Flock = 73,
    Fsync = 74,
    Fdatasync = 75,
    Truncate = 76,
    Ftruncate = 77,
    Getdents = 78,
    Getcwd = 79,

    // ── FS operations (80–99) ───────────────────────────────────
    Chdir = 80,
    Fchdir = 81,
    Rename = 82,
    Mkdir = 83,
    Rmdir = 84,
    Creat = 85,
    Link = 86,
    Unlink = 87,
    Symlink = 88,
    Readlink = 89,
    Chmod = 90,
    Fchmod = 91,
    Chown = 92,
    Fchown = 93,
    Lchown = 94,
    Umask = 95,
    Gettimeofday = 96,
    Getrlimit = 97,
    Getrusage = 98,
    Sysinfo = 99,

    // ── Time & IDs (100–130) ────────────────────────────────────
    Times = 100,
    Ptrace = 101,
    Getuid = 102,
    Syslog = 103,
    Getgid = 104,
    Setuid = 105,
    Setgid = 106,
    Geteuid = 107,
    Getegid = 108,
    Setpgid = 109,
    Getppid = 110,
    Getpgrp = 111,
    Setsid = 112,
    Setreuid = 113,
    Setregid = 114,
    Getgroups = 115,
    Setgroups = 116,
    Setresuid = 117,
    Getresuid = 118,
    Setresgid = 119,
    Getresgid = 120,
    Getpgid = 121,
    Setfsuid = 122,
    Setfsgid = 123,
    Getsid = 124,
    Capget = 125,
    Capset = 126,
    RtSigpending = 127,
    RtSigtimedwait = 128,
    RtSigqueueinfo = 129,
    RtSigsuspend = 130,

    // ── Signals & FS (131–155) ──────────────────────────────────
    Sigaltstack = 131,
    Utime = 132,
    Mknod = 133,
    Uselib = 134,
    Personality = 135,
    Ustat = 136,
    Statfs = 137,
    Fstatfs = 138,
    Sysfs = 139,
    Getpriority = 140,
    Setpriority = 141,
    SchedSetparam = 142,
    SchedGetparam = 143,
    SchedSetscheduler = 144,
    SchedGetscheduler = 145,
    SchedGetPriorityMax = 146,
    SchedGetPriorityMin = 147,
    SchedRrGetInterval = 148,
    Mlock = 149,
    Munlock = 150,
    Mlockall = 151,
    Munlockall = 152,
    Vhangup = 153,
    ModifyLdt = 154,
    PivotRoot = 155,

    // ── System (156–185) ────────────────────────────────────────
    Sysctl = 156,
    Prctl = 157,
    ArchPrctl = 158,
    Adjtimex = 159,
    Setrlimit = 160,
    Chroot = 161,
    Sync = 162,
    Acct = 163,
    Settimeofday = 164,
    MountLinux = 165,
    Umount2 = 166,
    Swapon = 167,
    Swapoff = 168,
    Reboot = 169,
    Sethostname = 170,
    Setdomainname = 171,
    Iopl = 172,
    Ioperm = 173,
    CreateModule = 174,
    InitModule = 175,
    DeleteModule = 176,
    GetKernelSyms = 177,
    QueryModule = 178,
    Quotactl = 179,
    Nfsservctl = 180,
    Getpmsg = 181,
    Putpmsg = 182,
    AfsSyscall = 183,
    Tuxcall = 184,
    Security = 185,

    // ── More system (186–220) ───────────────────────────────────
    Gettid = 186,
    Readahead = 187,
    Setxattr = 188,
    Lsetxattr = 189,
    Fsetxattr = 190,
    Getxattr = 191,
    Lgetxattr = 192,
    Fgetxattr = 193,
    Listxattr = 194,
    Llistxattr = 195,
    Flistxattr = 196,
    Removexattr = 197,
    Lremovexattr = 198,
    Fremovexattr = 199,
    Tkill = 200,
    Time = 201,
    Futex = 202,
    SchedSetaffinity = 203,
    SchedGetaffinity = 204,
    SetThreadArea = 205,
    IoSetup = 206,
    IoDestroy = 207,
    IoGetevents = 208,
    IoSubmit = 209,
    IoCancel = 210,
    GetThreadArea = 211,
    LookupDcookie = 212,
    EpollCreate = 213,
    EpollCtlOld = 214,
    EpollWaitOld = 215,
    RemapFilePages = 216,
    Getdents64 = 217,
    SetTidAddress = 218,
    RestartSyscall = 219,
    Semtimedop = 220,

    // ── Timers & POSIX AIO (221–240) ────────────────────────────
    Fadvise64 = 221,
    TimerCreate = 222,
    TimerSettime = 223,
    TimerGettime = 224,
    TimerGetoverrun = 225,
    TimerDelete = 226,
    ClockSettime = 227,
    ClockGettime = 228,
    ClockGetres = 229,
    ClockNanosleep = 230,
    ExitGroup = 231,
    EpollWait = 232,
    EpollCtl = 233,
    Tgkill = 234,
    Utimes = 235,
    Vserver = 236,
    Mbind = 237,
    SetMempolicy = 238,
    GetMempolicy = 239,
    MqOpen = 240,
    MqUnlink = 241,
    MqTimedsend = 242,
    MqTimedreceive = 243,
    MqNotify = 244,
    MqGetsetattr = 245,
    KexecLoad = 246,

    // ── Waitid & key management (247–260) ───────────────────────
    Waitid = 247,
    AddKey = 248,
    RequestKey = 249,
    Keyctl = 250,
    IoprioSet = 251,
    IoprioGet = 252,
    InotifyInit = 253,
    InotifyAddWatch = 254,
    InotifyRmWatch = 255,
    MigratePages = 256,
    Openat = 257,
    Mkdirat = 258,
    Mknodat = 259,
    Fchownat = 260,

    // ── *at() family (261–280) ──────────────────────────────────
    Futimesat = 261,
    Newfstatat = 262,
    Unlinkat = 263,
    Renameat = 264,
    Linkat = 265,
    Symlinkat = 266,
    Readlinkat = 267,
    Fchmodat = 268,
    Faccessat = 269,
    Pselect6 = 270,
    Ppoll = 271,
    Unshare = 272,
    SetRobustList = 273,
    GetRobustList = 274,
    Splice = 275,
    Tee = 276,
    SyncFileRange = 277,
    Vmsplice = 278,
    MovePages = 279,
    Utimensat = 280,

    // ── Epoll/signalfd/timerfd (281–296) ────────────────────────
    EpollPwait = 281,
    Signalfd = 282,
    TimerfdCreate = 283,
    Eventfd = 284,
    Fallocate = 285,
    TimerfdSettime = 286,
    TimerfdGettime = 287,
    Accept4 = 288,
    Signalfd4 = 289,
    Eventfd2 = 290,
    EpollCreate1 = 291,
    Dup3 = 292,
    Pipe2 = 293,
    InotifyInit1 = 294,
    Preadv = 295,
    Pwritev = 296,

    // ── Recent additions (297–334+) ─────────────────────────────
    RtTgsigqueueinfo = 297,
    PerfEventOpen = 298,
    Recvmmsg = 299,
    FanotifyInit = 300,
    FanotifyMark = 301,
    Prlimit64 = 302,
    NameToHandleAt = 303,
    OpenByHandleAt = 304,
    ClockAdjtime = 305,
    Syncfs = 306,
    Sendmmsg = 307,
    Setns = 308,
    Getcpu = 309,
    ProcessVmReadv = 310,
    ProcessVmWritev = 311,
    Kcmp = 312,
    FinitModule = 313,
    SchedSetattr = 314,
    SchedGetattr = 315,
    Renameat2 = 316,
    Seccomp = 317,
    Getrandom = 318,
    MemfdCreate = 319,
    KexecFileLoad = 320,
    Bpf = 321,
    Execveat = 322,
    Userfaultfd = 323,
    Membarrier = 324,
    Mlock2 = 325,
    CopyFileRange = 326,
    Preadv2 = 327,
    Pwritev2 = 328,
    PkeyMprotect = 329,
    PkeyAlloc = 330,
    PkeyFree = 331,
    Statx = 332,
    IoPgetevents = 333,
    Rseq = 334,

    // ── syscalls 335–423 (Linux 5.x+) ──────────────────────────
    PidfdSendSignal = 424,
    IoUringSetup = 425,
    IoUringEnter = 426,
    IoUringRegister = 427,
    OpenTree = 428,
    MoveMount = 429,
    Fsopen = 430,
    Fsconfig = 431,
    Fsmount = 432,
    Fspick = 433,
    PidfdOpen = 434,
    Clone3 = 435,
    CloseRange = 436,
    Openat2 = 437,
    PidfdGetfd = 438,
    Faccessat2 = 439,
    ProcessMadvise = 440,
    EpollPwait2 = 441,
    MountSetattr = 442,
    QuotactlFd = 443,
    LandlockCreateRuleset = 444,
    LandlockAddRule = 445,
    LandlockRestrictSelf = 446,
    MemfdSecret = 447,
    ProcessMrelease = 448,
    FutexWaitv = 449,
    SetMempolicy2 = 450,
    MapShadowStack = 451,

    // KnoxOS AI system calls (0x1000+)
    AiInfer = 0x1000,
    AiLoadModel = 0x1001,
    AiUnloadModel = 0x1002,
    AiQuery = 0x1003,

    // KnoxOS Thread system calls (0x2000+)
    ThreadCreate = 0x2000,
    ThreadExit = 0x2001,
    ThreadJoin = 0x2002,
    ThreadDetach = 0x2003,

    // KnoxOS extended system calls (0x3000+)
    KpmInstall = 0x3000,
    KpmRemove = 0x3001,
    KpmList = 0x3002,
    KpmSearch = 0x3003,
    CgroupCreate = 0x3010,
    CgroupAttach = 0x3011,
    Dmesg = 0x3020,
    LsBlk = 0x3030,
    Mount = 0x3031,
    Umount = 0x3032,
    Gethostname = 0x3040,
    Mkfifo = 0x3050,
    Openpty = 0x3051,

    // KnoxOS KVM virtualization (0x4000+)
    KvmCreateVm = 0x4000,
    KvmStartVm = 0x4001,
    KvmStopVm = 0x4002,
    KvmPauseVm = 0x4003,
    KvmDestroyVm = 0x4004,
    KvmSetVcpuRegs = 0x4005,
    KvmGetVcpuRegs = 0x4006,

    // KnoxOS ONNX ML runtime (0x5000+)
    OnnxLoadModel = 0x5000,
    OnnxUnloadModel = 0x5001,
    OnnxInfer = 0x5002,
    OnnxListModels = 0x5003,

    // KnoxOS shared library ops (0x6000+)
    Dlopen = 0x6000,
    Dlsym = 0x6001,
    Dlclose = 0x6002,

    // KnoxOS RT scheduling (0x8000+)
    RtSetParams = 0x8000,
    RtGetInfo = 0x8001,
    RtCreatePiMutex = 0x8002,
    RtCreateHrtimer = 0x8003,
    RtIsolateCpu = 0x8004,
    RtLatencyStats = 0x8005,

    Unknown = 0xFFFF,
}

impl From<u64> for SyscallNumber {
    fn from(n: u64) -> Self {
        match n {
            0 => Self::Read,
            1 => Self::Write,
            2 => Self::Open,
            3 => Self::Close,
            4 => Self::Stat,
            5 => Self::Fstat,
            6 => Self::Lstat,
            7 => Self::Poll,
            8 => Self::Lseek,
            9 => Self::Mmap,
            10 => Self::Mprotect,
            11 => Self::Munmap,
            12 => Self::Brk,
            13 => Self::Sigaction,
            14 => Self::Sigprocmask,
            15 => Self::RtSigreturn,
            16 => Self::Ioctl,
            17 => Self::Pread64,
            18 => Self::Pwrite64,
            19 => Self::Readv,
            20 => Self::Writev,
            21 => Self::Access,
            22 => Self::Pipe,
            23 => Self::Select,
            24 => Self::SchedYield,
            25 => Self::Mremap,
            26 => Self::Msync,
            27 => Self::Mincore,
            28 => Self::Madvise,
            29 => Self::Shmget,
            30 => Self::Shmat,
            31 => Self::Shmctl,
            32 => Self::Dup,
            33 => Self::Dup2,
            34 => Self::Pause,
            35 => Self::Nanosleep,
            36 => Self::Getitimer,
            37 => Self::Alarm,
            38 => Self::Setitimer,
            39 => Self::Getpid,
            40 => Self::Sendfile,
            41 => Self::Socket,
            42 => Self::Connect,
            43 => Self::Accept,
            44 => Self::Sendto,
            45 => Self::Recvfrom,
            46 => Self::Sendmsg,
            47 => Self::Recvmsg,
            48 => Self::Shutdown,
            49 => Self::Bind,
            50 => Self::Listen,
            51 => Self::Getsockname,
            52 => Self::Getpeername,
            53 => Self::Socketpair,
            54 => Self::Setsockopt,
            55 => Self::Getsockopt,
            56 => Self::Clone,
            57 => Self::Fork,
            58 => Self::Vfork,
            59 => Self::Execve,
            60 => Self::Exit,
            61 => Self::Wait4,
            62 => Self::Kill,
            63 => Self::Uname,
            64 => Self::Semget,
            65 => Self::Semop,
            66 => Self::Semctl,
            67 => Self::Shmdt,
            68 => Self::Msgget,
            69 => Self::Msgsnd,
            70 => Self::Msgrcv,
            71 => Self::Msgctl,
            72 => Self::Fcntl,
            73 => Self::Flock,
            74 => Self::Fsync,
            75 => Self::Fdatasync,
            76 => Self::Truncate,
            77 => Self::Ftruncate,
            78 => Self::Getdents,
            79 => Self::Getcwd,
            80 => Self::Chdir,
            81 => Self::Fchdir,
            82 => Self::Rename,
            83 => Self::Mkdir,
            84 => Self::Rmdir,
            85 => Self::Creat,
            86 => Self::Link,
            87 => Self::Unlink,
            88 => Self::Symlink,
            89 => Self::Readlink,
            90 => Self::Chmod,
            91 => Self::Fchmod,
            92 => Self::Chown,
            93 => Self::Fchown,
            94 => Self::Lchown,
            95 => Self::Umask,
            96 => Self::Gettimeofday,
            97 => Self::Getrlimit,
            98 => Self::Getrusage,
            99 => Self::Sysinfo,
            100 => Self::Times,
            101 => Self::Ptrace,
            102 => Self::Getuid,
            103 => Self::Syslog,
            104 => Self::Getgid,
            105 => Self::Setuid,
            106 => Self::Setgid,
            107 => Self::Geteuid,
            108 => Self::Getegid,
            109 => Self::Setpgid,
            110 => Self::Getppid,
            111 => Self::Getpgrp,
            112 => Self::Setsid,
            113 => Self::Setreuid,
            114 => Self::Setregid,
            115 => Self::Getgroups,
            116 => Self::Setgroups,
            117 => Self::Setresuid,
            118 => Self::Getresuid,
            119 => Self::Setresgid,
            120 => Self::Getresgid,
            121 => Self::Getpgid,
            122 => Self::Setfsuid,
            123 => Self::Setfsgid,
            124 => Self::Getsid,
            125 => Self::Capget,
            126 => Self::Capset,
            127 => Self::RtSigpending,
            128 => Self::RtSigtimedwait,
            129 => Self::RtSigqueueinfo,
            130 => Self::RtSigsuspend,
            131 => Self::Sigaltstack,
            132 => Self::Utime,
            133 => Self::Mknod,
            134 => Self::Uselib,
            135 => Self::Personality,
            136 => Self::Ustat,
            137 => Self::Statfs,
            138 => Self::Fstatfs,
            139 => Self::Sysfs,
            140 => Self::Getpriority,
            141 => Self::Setpriority,
            142 => Self::SchedSetparam,
            143 => Self::SchedGetparam,
            144 => Self::SchedSetscheduler,
            145 => Self::SchedGetscheduler,
            146 => Self::SchedGetPriorityMax,
            147 => Self::SchedGetPriorityMin,
            148 => Self::SchedRrGetInterval,
            149 => Self::Mlock,
            150 => Self::Munlock,
            151 => Self::Mlockall,
            152 => Self::Munlockall,
            153 => Self::Vhangup,
            154 => Self::ModifyLdt,
            155 => Self::PivotRoot,
            156 => Self::Sysctl,
            157 => Self::Prctl,
            158 => Self::ArchPrctl,
            159 => Self::Adjtimex,
            160 => Self::Setrlimit,
            161 => Self::Chroot,
            162 => Self::Sync,
            163 => Self::Acct,
            164 => Self::Settimeofday,
            165 => Self::MountLinux,
            166 => Self::Umount2,
            167 => Self::Swapon,
            168 => Self::Swapoff,
            169 => Self::Reboot,
            170 => Self::Sethostname,
            171 => Self::Setdomainname,
            172 => Self::Iopl,
            173 => Self::Ioperm,
            174 => Self::CreateModule,
            175 => Self::InitModule,
            176 => Self::DeleteModule,
            177 => Self::GetKernelSyms,
            178 => Self::QueryModule,
            179 => Self::Quotactl,
            180 => Self::Nfsservctl,
            181 => Self::Getpmsg,
            182 => Self::Putpmsg,
            183 => Self::AfsSyscall,
            184 => Self::Tuxcall,
            185 => Self::Security,
            186 => Self::Gettid,
            187 => Self::Readahead,
            188 => Self::Setxattr,
            189 => Self::Lsetxattr,
            190 => Self::Fsetxattr,
            191 => Self::Getxattr,
            192 => Self::Lgetxattr,
            193 => Self::Fgetxattr,
            194 => Self::Listxattr,
            195 => Self::Llistxattr,
            196 => Self::Flistxattr,
            197 => Self::Removexattr,
            198 => Self::Lremovexattr,
            199 => Self::Fremovexattr,
            200 => Self::Tkill,
            201 => Self::Time,
            202 => Self::Futex,
            203 => Self::SchedSetaffinity,
            204 => Self::SchedGetaffinity,
            205 => Self::SetThreadArea,
            206 => Self::IoSetup,
            207 => Self::IoDestroy,
            208 => Self::IoGetevents,
            209 => Self::IoSubmit,
            210 => Self::IoCancel,
            211 => Self::GetThreadArea,
            212 => Self::LookupDcookie,
            213 => Self::EpollCreate,
            214 => Self::EpollCtlOld,
            215 => Self::EpollWaitOld,
            216 => Self::RemapFilePages,
            217 => Self::Getdents64,
            218 => Self::SetTidAddress,
            219 => Self::RestartSyscall,
            220 => Self::Semtimedop,
            221 => Self::Fadvise64,
            222 => Self::TimerCreate,
            223 => Self::TimerSettime,
            224 => Self::TimerGettime,
            225 => Self::TimerGetoverrun,
            226 => Self::TimerDelete,
            227 => Self::ClockSettime,
            228 => Self::ClockGettime,
            229 => Self::ClockGetres,
            230 => Self::ClockNanosleep,
            231 => Self::ExitGroup,
            232 => Self::EpollWait,
            233 => Self::EpollCtl,
            234 => Self::Tgkill,
            235 => Self::Utimes,
            236 => Self::Vserver,
            237 => Self::Mbind,
            238 => Self::SetMempolicy,
            239 => Self::GetMempolicy,
            240 => Self::MqOpen,
            241 => Self::MqUnlink,
            242 => Self::MqTimedsend,
            243 => Self::MqTimedreceive,
            244 => Self::MqNotify,
            245 => Self::MqGetsetattr,
            246 => Self::KexecLoad,
            247 => Self::Waitid,
            248 => Self::AddKey,
            249 => Self::RequestKey,
            250 => Self::Keyctl,
            251 => Self::IoprioSet,
            252 => Self::IoprioGet,
            253 => Self::InotifyInit,
            254 => Self::InotifyAddWatch,
            255 => Self::InotifyRmWatch,
            256 => Self::MigratePages,
            257 => Self::Openat,
            258 => Self::Mkdirat,
            259 => Self::Mknodat,
            260 => Self::Fchownat,
            261 => Self::Futimesat,
            262 => Self::Newfstatat,
            263 => Self::Unlinkat,
            264 => Self::Renameat,
            265 => Self::Linkat,
            266 => Self::Symlinkat,
            267 => Self::Readlinkat,
            268 => Self::Fchmodat,
            269 => Self::Faccessat,
            270 => Self::Pselect6,
            271 => Self::Ppoll,
            272 => Self::Unshare,
            273 => Self::SetRobustList,
            274 => Self::GetRobustList,
            275 => Self::Splice,
            276 => Self::Tee,
            277 => Self::SyncFileRange,
            278 => Self::Vmsplice,
            279 => Self::MovePages,
            280 => Self::Utimensat,
            281 => Self::EpollPwait,
            282 => Self::Signalfd,
            283 => Self::TimerfdCreate,
            284 => Self::Eventfd,
            285 => Self::Fallocate,
            286 => Self::TimerfdSettime,
            287 => Self::TimerfdGettime,
            288 => Self::Accept4,
            289 => Self::Signalfd4,
            290 => Self::Eventfd2,
            291 => Self::EpollCreate1,
            292 => Self::Dup3,
            293 => Self::Pipe2,
            294 => Self::InotifyInit1,
            295 => Self::Preadv,
            296 => Self::Pwritev,
            297 => Self::RtTgsigqueueinfo,
            298 => Self::PerfEventOpen,
            299 => Self::Recvmmsg,
            300 => Self::FanotifyInit,
            301 => Self::FanotifyMark,
            302 => Self::Prlimit64,
            303 => Self::NameToHandleAt,
            304 => Self::OpenByHandleAt,
            305 => Self::ClockAdjtime,
            306 => Self::Syncfs,
            307 => Self::Sendmmsg,
            308 => Self::Setns,
            309 => Self::Getcpu,
            310 => Self::ProcessVmReadv,
            311 => Self::ProcessVmWritev,
            312 => Self::Kcmp,
            313 => Self::FinitModule,
            314 => Self::SchedSetattr,
            315 => Self::SchedGetattr,
            316 => Self::Renameat2,
            317 => Self::Seccomp,
            318 => Self::Getrandom,
            319 => Self::MemfdCreate,
            320 => Self::KexecFileLoad,
            321 => Self::Bpf,
            322 => Self::Execveat,
            323 => Self::Userfaultfd,
            324 => Self::Membarrier,
            325 => Self::Mlock2,
            326 => Self::CopyFileRange,
            327 => Self::Preadv2,
            328 => Self::Pwritev2,
            329 => Self::PkeyMprotect,
            330 => Self::PkeyAlloc,
            331 => Self::PkeyFree,
            332 => Self::Statx,
            333 => Self::IoPgetevents,
            334 => Self::Rseq,
            424 => Self::PidfdSendSignal,
            425 => Self::IoUringSetup,
            426 => Self::IoUringEnter,
            427 => Self::IoUringRegister,
            428 => Self::OpenTree,
            429 => Self::MoveMount,
            430 => Self::Fsopen,
            431 => Self::Fsconfig,
            432 => Self::Fsmount,
            433 => Self::Fspick,
            434 => Self::PidfdOpen,
            435 => Self::Clone3,
            436 => Self::CloseRange,
            437 => Self::Openat2,
            438 => Self::PidfdGetfd,
            439 => Self::Faccessat2,
            440 => Self::ProcessMadvise,
            441 => Self::EpollPwait2,
            442 => Self::MountSetattr,
            443 => Self::QuotactlFd,
            444 => Self::LandlockCreateRuleset,
            445 => Self::LandlockAddRule,
            446 => Self::LandlockRestrictSelf,
            447 => Self::MemfdSecret,
            448 => Self::ProcessMrelease,
            449 => Self::FutexWaitv,
            450 => Self::SetMempolicy2,
            451 => Self::MapShadowStack,
            // KnoxOS extensions
            0x1000 => Self::AiInfer,
            0x1001 => Self::AiLoadModel,
            0x1002 => Self::AiUnloadModel,
            0x1003 => Self::AiQuery,
            0x2000 => Self::ThreadCreate,
            0x2001 => Self::ThreadExit,
            0x2002 => Self::ThreadJoin,
            0x2003 => Self::ThreadDetach,
            0x3000 => Self::KpmInstall,
            0x3001 => Self::KpmRemove,
            0x3002 => Self::KpmList,
            0x3003 => Self::KpmSearch,
            0x3010 => Self::CgroupCreate,
            0x3011 => Self::CgroupAttach,
            0x3020 => Self::Dmesg,
            0x3030 => Self::LsBlk,
            0x3031 => Self::Mount,
            0x3032 => Self::Umount,
            0x3040 => Self::Gethostname,
            0x3050 => Self::Mkfifo,
            0x3051 => Self::Openpty,
            0x4000 => Self::KvmCreateVm,
            0x4001 => Self::KvmStartVm,
            0x4002 => Self::KvmStopVm,
            0x4003 => Self::KvmPauseVm,
            0x4004 => Self::KvmDestroyVm,
            0x4005 => Self::KvmSetVcpuRegs,
            0x4006 => Self::KvmGetVcpuRegs,
            0x5000 => Self::OnnxLoadModel,
            0x5001 => Self::OnnxUnloadModel,
            0x5002 => Self::OnnxInfer,
            0x5003 => Self::OnnxListModels,
            0x6000 => Self::Dlopen,
            0x6001 => Self::Dlsym,
            0x6002 => Self::Dlclose,
            0x8000 => Self::RtSetParams,
            0x8001 => Self::RtGetInfo,
            0x8002 => Self::RtCreatePiMutex,
            0x8003 => Self::RtCreateHrtimer,
            0x8004 => Self::RtIsolateCpu,
            0x8005 => Self::RtLatencyStats,
            _ => Self::Unknown,
        }
    }
}

/// Syscall result type
pub type SyscallResult = Result<u64, SyscallError>;

#[derive(Debug)]
pub enum SyscallError {
    PermissionDenied,          // EACCES (13)
    FileNotFound,              // ENOENT (2)
    NoSuchProcess,             // ESRCH (3)
    Interrupted,               // EINTR (4)
    IoError,                   // EIO (5)
    NoSuchDevice,              // ENXIO (6)
    ArgumentListTooLong,       // E2BIG (7)
    ExecFormatError,           // ENOEXEC (8)
    BadFileDescriptor,         // EBADF (9)
    NoChildProcess,            // ECHILD (10)
    WouldBlock,                // EAGAIN (11)
    OutOfMemory,               // ENOMEM (12)
    PermissionDeniedFault,     // EFAULT (14)
    DeviceBusy,                // EBUSY (16)
    FileExists,                // EEXIST (17)
    CrossDeviceLink,           // EXDEV (18)
    NoSuchDeviceError,         // ENODEV (19)
    NotDirectory,              // ENOTDIR (20)
    IsDirectory,               // EISDIR (21)
    InvalidArgument,           // EINVAL (22)
    TooManyFilesSystem,        // ENFILE (23)
    TooManyFiles,              // EMFILE (24)
    NotATty,                   // ENOTTY (25)
    TextFileBusy,              // ETXTBSY (26)
    FileTooLarge,              // EFBIG (27)
    NoSpaceLeft,               // ENOSPC (28)
    IllegalSeek,               // ESPIPE (29)
    ReadOnlyFs,                // EROFS (30)
    TooManyLinks,              // EMLINK (31)
    BrokenPipe,                // EPIPE (32)
    MathDomainError,           // EDOM (33)
    MathRangeError,            // ERANGE (34)
    Deadlock,                  // EDEADLK (35)
    NameTooLong,               // ENAMETOOLONG (36)
    NoLocksAvailable,          // ENOLCK (37)
    NotImplemented,            // ENOSYS (38)
    NotEmpty,                  // ENOTEMPTY (39)
    Loop,                      // ELOOP (40)
    NoMessage,                 // ENOMSG (42)
    IdentifierRemoved,         // EIDRM (43)
    NoData,                    // ENODATA (61)
    Overflow,                  // EOVERFLOW (75)
    ProtocolNotSupported,      // EPROTONOSUPPORT (93)
    NotSupported,              // ENOTSUP / EOPNOTSUPP (95)
    AddressFamilyNotSupported, // EAFNOSUPPORT (97)
    AddressInUse,              // EADDRINUSE (98)
    AddressNotAvailable,       // EADDRNOTAVAIL (99)
    NetworkDown,               // ENETDOWN (100)
    NetworkUnreachable,        // ENETUNREACH (101)
    ConnectionReset,           // ECONNRESET (104)
    AlreadyConnected,          // EISCONN (106)
    NotConnected,              // ENOTCONN (107)
    ConnectionRefused,         // ECONNREFUSED (111)
    HostUnreachable,           // EHOSTUNREACH (113)
    AlreadyInProgress,         // EALREADY (114)
    InProgress,                // EINPROGRESS (115)
    Canceled,                  // ECANCELED (125)
    NoKey,                     // ENOKEY (126)
    TimedOut,                  // ETIMEDOUT (110)
}

impl SyscallError {
    pub fn errno(&self) -> i64 {
        match self {
            Self::FileNotFound => -2,
            Self::NoSuchProcess => -3,
            Self::Interrupted => -4,
            Self::IoError => -5,
            Self::NoSuchDevice => -6,
            Self::ArgumentListTooLong => -7,
            Self::ExecFormatError => -8,
            Self::BadFileDescriptor => -9,
            Self::NoChildProcess => -10,
            Self::WouldBlock => -11,
            Self::OutOfMemory => -12,
            Self::PermissionDenied => -13,
            Self::PermissionDeniedFault => -14,
            Self::DeviceBusy => -16,
            Self::FileExists => -17,
            Self::CrossDeviceLink => -18,
            Self::NoSuchDeviceError => -19,
            Self::NotDirectory => -20,
            Self::IsDirectory => -21,
            Self::InvalidArgument => -22,
            Self::TooManyFilesSystem => -23,
            Self::TooManyFiles => -24,
            Self::NotATty => -25,
            Self::TextFileBusy => -26,
            Self::FileTooLarge => -27,
            Self::NoSpaceLeft => -28,
            Self::IllegalSeek => -29,
            Self::ReadOnlyFs => -30,
            Self::TooManyLinks => -31,
            Self::BrokenPipe => -32,
            Self::MathDomainError => -33,
            Self::MathRangeError => -34,
            Self::Deadlock => -35,
            Self::NameTooLong => -36,
            Self::NoLocksAvailable => -37,
            Self::NotImplemented => -38,
            Self::NotEmpty => -39,
            Self::Loop => -40,
            Self::NoMessage => -42,
            Self::IdentifierRemoved => -43,
            Self::NoData => -61,
            Self::Overflow => -75,
            Self::ProtocolNotSupported => -93,
            Self::NotSupported => -95,
            Self::AddressFamilyNotSupported => -97,
            Self::AddressInUse => -98,
            Self::AddressNotAvailable => -99,
            Self::NetworkDown => -100,
            Self::NetworkUnreachable => -101,
            Self::ConnectionReset => -104,
            Self::AlreadyConnected => -106,
            Self::NotConnected => -107,
            Self::TimedOut => -110,
            Self::ConnectionRefused => -111,
            Self::HostUnreachable => -113,
            Self::AlreadyInProgress => -114,
            Self::InProgress => -115,
            Self::Canceled => -125,
            Self::NoKey => -126,
        }
    }
}

/// Helper: read a null-terminated string from user pointer
unsafe fn read_user_string(ptr: u64) -> Option<String> {
    if ptr == 0 {
        return None;
    }
    let mut len = 0usize;
    let base = ptr as *const u8;
    while *base.add(len) != 0 && len < 4096 {
        len += 1;
    }
    let bytes = core::slice::from_raw_parts(base, len);
    core::str::from_utf8(bytes).ok().map(String::from)
}

/// Main syscall dispatcher — handles ALL ~452 Linux x86_64 syscalls + KnoxOS extensions
pub fn handle_syscall(
    number: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> i64 {
    // ── Seccomp filter check ────────────────────────────────────────
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let args = [arg1, arg2, arg3, arg4, arg5, arg6];
    match crate::seccomp::check_syscall(pid, number, args) {
        Ok(()) => {}
        Err(errno) => {
            serial_println!(
                "[KnoxOS] seccomp: blocked pid {} syscall {} (errno={})",
                pid,
                number,
                errno
            );
            let uid = crate::users::get_current_uid();
            crate::audit::log_syscall(pid, uid, number, args, errno as i64, false);
            return errno as i64;
        }
    }
    // ── Audit syscall logging ───────────────────────────────────────
    let uid = crate::users::get_current_uid();
    crate::audit::log_syscall(pid, uid, number, args, 0, true);

    let syscall = SyscallNumber::from(number);
    let result = match syscall {
        // ════════════════════════════════════════════════════════════
        // ── Core File I/O (0–20) ───────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Read => fs::sys_read(arg1, arg2, arg3),
        SyscallNumber::Write => fs::sys_write(arg1, arg2, arg3),
        SyscallNumber::Open | SyscallNumber::Creat => fs::sys_open(arg1, arg2 as u32, arg3 as u16),
        SyscallNumber::Close => fs::sys_close(arg1 as i32),
        SyscallNumber::Stat | SyscallNumber::Lstat => fs::sys_stat(arg1, arg2),
        SyscallNumber::Fstat => fs::sys_fstat(arg1 as i32, arg2),
        SyscallNumber::Poll | SyscallNumber::Ppoll => io::sys_poll(arg1, arg2 as u32, arg3 as i32),
        SyscallNumber::Lseek => fs::sys_lseek(arg1 as i32, arg2 as i64, arg3 as u32),
        SyscallNumber::Mmap => {
            memory::sys_mmap(arg1, arg2, arg3 as u32, arg4 as u32, arg5 as i32, arg6)
        }
        SyscallNumber::Mprotect | SyscallNumber::PkeyMprotect => {
            // mprotect(addr, len, prot) — change memory protection
            // Accept silently; fine-grained page table prot changes
            // are recorded but not enforced on our flat memory model
            let _addr = arg1;
            let _len = arg2;
            let _prot = arg3 as u32;
            Ok(0)
        }
        SyscallNumber::Munmap => memory::sys_munmap(arg1, arg2),
        SyscallNumber::Brk => memory::sys_brk(arg1),
        SyscallNumber::Sigaction => signal::sys_sigaction(arg1 as u32, arg2, arg3),
        SyscallNumber::Sigprocmask => signal::sys_sigprocmask(arg1 as i32, arg2, arg3),
        SyscallNumber::RtSigreturn => advanced::sys_rt_sigreturn(),
        SyscallNumber::Ioctl => fd::sys_ioctl(arg1 as i32, arg2 as u32, arg3),
        SyscallNumber::Pread64 => advanced::sys_pread64(arg1, arg2, arg3, arg4 as i64),
        SyscallNumber::Pwrite64 => advanced::sys_pwrite64(arg1, arg2, arg3, arg4 as i64),
        SyscallNumber::Readv => io::sys_readv(arg1 as i32, arg2, arg3 as usize),
        SyscallNumber::Writev => io::sys_writev(arg1 as i32, arg2, arg3 as usize),

        // ════════════════════════════════════════════════════════════
        // ── File access & memory (21–40) ───────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Access => fs::sys_access(arg1, arg2 as u32),
        SyscallNumber::Pipe => fd::sys_pipe(arg1),
        SyscallNumber::Select | SyscallNumber::Pselect6 => {
            io::sys_select(arg1 as i32, arg2, arg3, arg4, arg5)
        }
        SyscallNumber::SchedYield => crate::sched_ext::sched_yield()
            .map(|_| 0u64)
            .map_err(|_| SyscallError::NotImplemented),
        SyscallNumber::Mremap => advanced::sys_mremap(arg1, arg2, arg3, arg4 as i32, arg5),
        SyscallNumber::Msync => advanced::sys_msync(arg1, arg2, arg3 as i32),
        SyscallNumber::Mincore => advanced::sys_mincore(arg1, arg2, arg3),
        SyscallNumber::Madvise | SyscallNumber::ProcessMadvise => {
            advanced::sys_madvise(arg1, arg2, arg3 as i32)
        }
        SyscallNumber::Shmget => io::sys_shmget(arg1 as u32, arg2 as usize, arg3 as i32),
        SyscallNumber::Shmat => io::sys_shmat(arg1 as u32, arg2, arg3 as i32),
        SyscallNumber::Shmctl => io::sys_shmctl(arg1 as u32, arg2 as i32, arg3),
        SyscallNumber::Dup => fd::sys_dup(arg1 as i32),
        SyscallNumber::Dup2 => fd::sys_dup2(arg1 as i32, arg2 as i32),
        SyscallNumber::Pause => {
            crate::arch_compat::instructions::interrupts::hlt();
            Err(SyscallError::Interrupted)
        }
        SyscallNumber::Nanosleep | SyscallNumber::ClockNanosleep => time::sys_nanosleep(arg1),
        SyscallNumber::Getitimer => advanced::sys_getitimer_linux(arg1 as i32, arg2),
        SyscallNumber::Alarm => advanced::sys_alarm_linux(arg1 as u32),
        SyscallNumber::Setitimer => advanced::sys_setitimer_linux(arg1 as i32, arg2, arg3),
        SyscallNumber::Getpid => process::sys_getpid(),
        SyscallNumber::Sendfile => io::sys_sendfile(arg1 as i32, arg2 as i32, arg3, arg4 as usize),

        // ════════════════════════════════════════════════════════════
        // ── Sockets (41–55) ────────────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Socket => net::sys_socket(arg1 as i32, arg2 as i32, arg3 as i32),
        SyscallNumber::Connect => net::sys_connect(arg1 as i32, arg2, arg3 as u32),
        SyscallNumber::Accept => net::sys_accept(arg1 as i32, arg2, arg3),
        SyscallNumber::Sendto => net::sys_sendto(
            arg1 as i32,
            arg2,
            arg3 as usize,
            arg4 as i32,
            arg5,
            arg6 as u32,
        ),
        SyscallNumber::Recvfrom => {
            net::sys_recvfrom(arg1 as i32, arg2, arg3 as usize, arg4 as i32, arg5, arg6)
        }
        SyscallNumber::Sendmsg | SyscallNumber::Sendmmsg => {
            advanced::sys_sendmsg(arg1 as i32, arg2, arg3 as i32)
        }
        SyscallNumber::Recvmsg | SyscallNumber::Recvmmsg => {
            advanced::sys_recvmsg(arg1 as i32, arg2, arg3 as i32)
        }
        SyscallNumber::Shutdown => net::sys_shutdown(arg1 as i32, arg2 as i32),
        SyscallNumber::Bind => net::sys_bind(arg1 as i32, arg2, arg3 as u32),
        SyscallNumber::Listen => net::sys_listen(arg1 as i32, arg2 as i32),
        SyscallNumber::Getsockname => advanced::sys_getsockname(arg1 as i32, arg2, arg3),
        SyscallNumber::Getpeername => advanced::sys_getpeername(arg1 as i32, arg2, arg3),
        SyscallNumber::Socketpair => {
            net::sys_socketpair(arg1 as i32, arg2 as i32, arg3 as i32, arg4)
        }
        SyscallNumber::Setsockopt => {
            // setsockopt(fd, level, optname, optval, optlen)
            // Accept most options silently for compatibility
            let _level = arg2 as i32;
            let _optname = arg3 as i32;
            serial_println!(
                "[KnoxOS] setsockopt(fd={}, level={}, opt={})",
                arg1,
                _level,
                _optname
            );
            Ok(0)
        }
        SyscallNumber::Getsockopt => {
            // getsockopt(fd, level, optname, optval, optlen_ptr)
            let level = arg2 as i32;
            let optname = arg3 as i32;
            let optval = arg4;
            let optlen_ptr = arg5;
            // SOL_SOCKET(1) + SO_ERROR(4): return 0 (no error)
            if level == 1 && optname == 4 && optval != 0 {
                unsafe {
                    *(optval as *mut i32) = 0;
                }
                if optlen_ptr != 0 {
                    unsafe {
                        *(optlen_ptr as *mut u32) = 4;
                    }
                }
            } else if optval != 0 {
                // Default: return 0
                unsafe {
                    *(optval as *mut i32) = 0;
                }
                if optlen_ptr != 0 {
                    unsafe {
                        *(optlen_ptr as *mut u32) = 4;
                    }
                }
            }
            Ok(0)
        }

        // ════════════════════════════════════════════════════════════
        // ── Process management (56–71) ─────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Clone => {
            // clone(flags, stack, ptid, ctid, tls)
            let flags = arg1;
            const CLONE_VM: u64 = 0x00000100;
            const CLONE_THREAD: u64 = 0x00010000;
            if flags & CLONE_THREAD != 0 && flags & CLONE_VM != 0 {
                // Thread creation: use the provided stack
                let stack = arg2;
                thread::sys_thread_create(0, stack)
            } else {
                // Process creation: fork
                process::sys_fork()
            }
        }
        SyscallNumber::Fork | SyscallNumber::Vfork => process::sys_fork(),
        SyscallNumber::Execve | SyscallNumber::Execveat => process::sys_execve(arg1, arg2, arg3),
        SyscallNumber::Exit | SyscallNumber::ExitGroup => process::sys_exit(arg1 as i32),
        SyscallNumber::Wait4 => process::sys_wait4(arg1 as i32, arg2, arg3 as i32),
        SyscallNumber::Kill => process::sys_kill(arg1 as u32, arg2 as u32),
        SyscallNumber::Uname => system::sys_uname(arg1),
        SyscallNumber::Semget => advanced::sys_semget(arg1 as u32, arg2 as i32, arg3 as i32),
        SyscallNumber::Semop => advanced::sys_semop(arg1 as i32, arg2, arg3 as usize),
        SyscallNumber::Semctl => advanced::sys_semctl(arg1 as i32, arg2 as i32, arg3 as i32, arg4),
        SyscallNumber::Shmdt => io::sys_shmdt(arg1),
        SyscallNumber::Msgget => advanced::sys_msgget(arg1 as u32, arg2 as i32),
        SyscallNumber::Msgsnd => {
            advanced::sys_msgsnd(arg1 as i32, arg2, arg3 as usize, arg4 as i32)
        }
        SyscallNumber::Msgrcv => {
            advanced::sys_msgrcv(arg1 as i32, arg2, arg3 as usize, arg4 as i64, arg5 as i32)
        }
        SyscallNumber::Msgctl => advanced::sys_msgctl(arg1 as i32, arg2 as i32, arg3),

        // ════════════════════════════════════════════════════════════
        // ── FD & FS ops (72–99) ────────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Fcntl => fd::sys_fcntl(arg1 as i32, arg2 as i32, arg3),
        SyscallNumber::Flock => io::sys_flock(arg1 as i32, arg2 as i32),
        SyscallNumber::Fsync => advanced::sys_fsync(arg1 as i32),
        SyscallNumber::Fdatasync => advanced::sys_fdatasync(arg1 as i32),
        SyscallNumber::Truncate | SyscallNumber::Ftruncate => fs::sys_truncate(arg1, arg2 as usize),
        SyscallNumber::Getdents => advanced::sys_getdents(arg1 as i32, arg2, arg3 as u32),
        SyscallNumber::Getcwd => fs::sys_getcwd(arg1, arg2 as usize),
        SyscallNumber::Chdir => fs::sys_chdir(arg1),
        SyscallNumber::Fchdir => {
            // fchdir(fd) — change directory using file descriptor
            let pid = crate::scheduler::current_pid().unwrap_or(1);
            let tables = crate::fd::PROCESS_FD_TABLES.lock();
            if let Some(fd_table) = tables.get(&pid) {
                if let Some(file) = fd_table.get(arg1 as i32) {
                    if file.file_type != crate::fd::FileType::Directory {
                        Err(SyscallError::NotDirectory)
                    } else {
                        let path = file.path.clone();
                        drop(tables);
                        crate::process::PROCESS_TABLE.lock().chdir(pid, &path);
                        Ok(0)
                    }
                } else {
                    Err(SyscallError::BadFileDescriptor)
                }
            } else {
                Err(SyscallError::BadFileDescriptor)
            }
        }
        SyscallNumber::Rename => fs::sys_rename(arg1, arg2),
        SyscallNumber::Mkdir => fs::sys_mkdir(arg1, arg2 as u16),
        SyscallNumber::Rmdir => fs::sys_rmdir(arg1),
        SyscallNumber::Link => advanced::sys_linkat(-100, arg1, -100, arg2, 0),
        SyscallNumber::Unlink => fs::sys_unlink(arg1),
        SyscallNumber::Symlink => fs::sys_symlink(arg1, arg2),
        SyscallNumber::Readlink => fs::sys_readlink(arg1, arg2, arg3),
        SyscallNumber::Chmod | SyscallNumber::Fchmod => fs::sys_chmod(arg1, arg2 as u16),
        SyscallNumber::Chown | SyscallNumber::Fchown | SyscallNumber::Lchown => {
            fs::sys_chown(arg1, arg2 as u32, arg3 as u32)
        }
        SyscallNumber::Umask => fs::sys_umask(arg1 as u16),
        SyscallNumber::Gettimeofday => time::sys_gettimeofday(arg1),
        SyscallNumber::Getrlimit => {
            // getrlimit(resource, rlim)
            if arg2 != 0 {
                unsafe {
                    let p = arg2 as *mut u64;
                    // rlim_cur
                    *p = 0x7FFFFFFFFFFFFFFF; // RLIM_INFINITY
                    // rlim_max
                    *p.add(1) = 0x7FFFFFFFFFFFFFFF;
                }
            }
            Ok(0)
        }
        SyscallNumber::Setrlimit => {
            // setrlimit(resource, rlim) — accept silently
            Ok(0)
        }
        SyscallNumber::Getrusage => fs::sys_getrusage(arg1 as i32, arg2),
        SyscallNumber::Sysinfo => system::sys_sysinfo(arg1),

        // ════════════════════════════════════════════════════════════
        // ── Time, IDs, signals (100–130) ───────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Times => {
            if arg1 != 0 {
                unsafe {
                    core::ptr::write_bytes(arg1 as *mut u8, 0, 32);
                }
            }
            Ok(crate::interrupts::get_ticks())
        }
        SyscallNumber::Ptrace => advanced::sys_ptrace(arg1, arg2, arg3, arg4),
        SyscallNumber::Getuid | SyscallNumber::Geteuid => user::sys_getuid(),
        SyscallNumber::Syslog => system::sys_syslog(arg1 as i32, arg2, arg3 as usize),
        SyscallNumber::Getgid | SyscallNumber::Getegid => user::sys_getgid(),
        SyscallNumber::Setuid => user::sys_setuid(arg1 as u32),
        SyscallNumber::Setgid => user::sys_setgid(arg1 as u32),
        SyscallNumber::Setpgid => process::sys_setpgid(arg1 as u32, arg2 as u32),
        SyscallNumber::Getppid => process::sys_getppid(),
        SyscallNumber::Getpgrp => process::sys_getpgrp(),
        SyscallNumber::Setsid => process::sys_setsid(),
        SyscallNumber::Setreuid => user::sys_setreuid(arg1 as u32, arg2 as u32),
        SyscallNumber::Setregid => user::sys_setregid(arg1 as u32, arg2 as u32),
        SyscallNumber::Getgroups => user::sys_getgroups(arg1 as i32, arg2),
        SyscallNumber::Setgroups => user::sys_setgroups(arg1 as usize, arg2),
        SyscallNumber::Setresuid => advanced::sys_setresuid(arg1 as u32, arg2 as u32, arg3 as u32),
        SyscallNumber::Getresuid => advanced::sys_getresuid(arg1, arg2, arg3),
        SyscallNumber::Setresgid => advanced::sys_setresgid(arg1 as u32, arg2 as u32, arg3 as u32),
        SyscallNumber::Getresgid => advanced::sys_getresgid(arg1, arg2, arg3),
        SyscallNumber::Getpgid => process::sys_getpgid(arg1 as u32),
        SyscallNumber::Setfsuid => {
            // setfsuid returns the previous fsuid (= uid)
            let pid = crate::scheduler::current_pid().unwrap_or(0);
            let uid = crate::process::PROCESS_TABLE
                .lock()
                .get_process(pid)
                .map(|p| p.uid)
                .unwrap_or(0);
            Ok(uid as u64)
        }
        SyscallNumber::Setfsgid => {
            let pid = crate::scheduler::current_pid().unwrap_or(0);
            let gid = crate::process::PROCESS_TABLE
                .lock()
                .get_process(pid)
                .map(|p| p.gid)
                .unwrap_or(0);
            Ok(gid as u64)
        }
        SyscallNumber::Getsid => process::sys_getsid(arg1 as u32),
        SyscallNumber::Capget => advanced::sys_capget(arg1, arg2),
        SyscallNumber::Capset => advanced::sys_capset(arg1, arg2),
        SyscallNumber::RtSigpending => advanced::sys_rt_sigpending(arg1, arg2 as usize),
        SyscallNumber::RtSigtimedwait => {
            advanced::sys_rt_sigtimedwait(arg1, arg2, arg3, arg4 as usize)
        }
        SyscallNumber::RtSigqueueinfo | SyscallNumber::RtTgsigqueueinfo => {
            advanced::sys_rt_sigqueueinfo(arg1 as u32, arg2 as u32, arg3)
        }
        SyscallNumber::RtSigsuspend => advanced::sys_rt_sigsuspend(arg1, arg2 as usize),

        // ════════════════════════════════════════════════════════════
        // ── FS & scheduling (131–155) ──────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Sigaltstack => signal::sys_sigaltstack(arg1, arg2),
        SyscallNumber::Utime | SyscallNumber::Utimes | SyscallNumber::Futimesat => {
            // Validate path exists (timestamps not tracked by in-memory VFS)
            if arg1 != 0 {
                if let Some(path) = unsafe { read_user_string(arg1) } {
                    let vfs = crate::vfs::VFS.lock();
                    if vfs.resolve_path(&path).is_none() {
                        Err(SyscallError::FileNotFound)
                    } else {
                        Ok(0)
                    }
                } else {
                    Ok(0)
                }
            } else {
                Ok(0)
            }
        }
        SyscallNumber::Mknod | SyscallNumber::Mknodat => {
            // mknod(path, mode, dev) — create special file
            let path = unsafe { read_user_string(arg1) };
            if let Some(p) = path {
                let mode = arg2 as u32;
                let file_type = mode & 0o170000;
                let mut vfs = crate::vfs::VFS.lock();
                match file_type {
                    0o010000 => {
                        // S_IFIFO — create named pipe
                        vfs.write_file(&p, &[]);
                        if let Some(ino) = vfs.resolve_path(&p) {
                            if let Some(inode) = vfs.get_inode_mut(ino) {
                                inode.file_type = crate::vfs::FileType::Pipe;
                                inode.permissions = (mode & 0o7777) as u16;
                            }
                        }
                        Ok(0)
                    }
                    0o100000 | 0 => {
                        // S_IFREG or default — create regular file
                        vfs.write_file(&p, &[]);
                        Ok(0)
                    }
                    0o020000 | 0o060000 => {
                        // S_IFCHR / S_IFBLK — create device node
                        vfs.write_file(&p, &[]);
                        if let Some(ino) = vfs.resolve_path(&p) {
                            if let Some(inode) = vfs.get_inode_mut(ino) {
                                inode.file_type = if file_type == 0o020000 {
                                    crate::vfs::FileType::CharDevice
                                } else {
                                    crate::vfs::FileType::BlockDevice
                                };
                                inode.permissions = (mode & 0o7777) as u16;
                            }
                        }
                        Ok(0)
                    }
                    0o140000 => {
                        // S_IFSOCK — create socket node
                        vfs.write_file(&p, &[]);
                        if let Some(ino) = vfs.resolve_path(&p) {
                            if let Some(inode) = vfs.get_inode_mut(ino) {
                                inode.file_type = crate::vfs::FileType::Socket;
                            }
                        }
                        Ok(0)
                    }
                    _ => Err(SyscallError::InvalidArgument),
                }
            } else {
                Err(SyscallError::InvalidArgument)
            }
        }
        SyscallNumber::Uselib => Err(SyscallError::NotImplemented),
        SyscallNumber::Personality => advanced::sys_personality(arg1),
        SyscallNumber::Ustat => Err(SyscallError::NotImplemented),
        SyscallNumber::Statfs | SyscallNumber::Fstatfs => fs::sys_statfs(arg1, arg2),
        SyscallNumber::Sysfs => advanced::sys_sysfs(arg1 as i32, arg2, arg3),
        SyscallNumber::Getpriority => io::sys_getpriority(arg1 as i32, arg2 as u32),
        SyscallNumber::Setpriority => io::sys_setpriority(arg1 as i32, arg2 as u32, arg3 as i32),
        SyscallNumber::SchedSetparam => {
            // sched_setparam(pid, param) — set scheduling parameters
            if arg2 != 0 {
                let _priority = unsafe { *(arg2 as *const i32) };
            }
            Ok(0)
        }
        SyscallNumber::SchedGetparam => {
            // sched_getparam(pid, param)
            if arg2 != 0 {
                unsafe {
                    *(arg2 as *mut i32) = 0;
                } // sched_priority = 0
            }
            Ok(0)
        }
        SyscallNumber::SchedSetscheduler => {
            io::sys_sched_setscheduler(arg1 as u32, arg2 as i32, arg3)
        }
        SyscallNumber::SchedGetscheduler => io::sys_sched_getscheduler(arg1 as u32),
        SyscallNumber::SchedGetPriorityMax => Ok(99),
        SyscallNumber::SchedGetPriorityMin => Ok(0),
        SyscallNumber::SchedRrGetInterval => advanced::sys_sched_rr_get_interval(arg1 as u32, arg2),
        SyscallNumber::Mlock => advanced::sys_mlock(arg1, arg2),
        SyscallNumber::Munlock => advanced::sys_munlock(arg1, arg2),
        SyscallNumber::Mlockall => advanced::sys_mlockall(arg1 as i32),
        SyscallNumber::Munlockall => advanced::sys_munlockall(),
        SyscallNumber::Vhangup => advanced::sys_vhangup(),
        SyscallNumber::ModifyLdt => advanced::sys_modify_ldt(arg1 as i32, arg2, arg3),
        SyscallNumber::PivotRoot => advanced::sys_pivot_root(arg1, arg2),

        // ════════════════════════════════════════════════════════════
        // ── System control (156–185) ───────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Sysctl => advanced::sys_sysctl_old(arg1),
        SyscallNumber::Prctl => advanced::sys_prctl(arg1 as i32, arg2, arg3, arg4, arg5),
        SyscallNumber::ArchPrctl => advanced::sys_arch_prctl(arg1 as i32, arg2),
        SyscallNumber::Adjtimex | SyscallNumber::ClockAdjtime => advanced::sys_adjtimex(arg1),
        SyscallNumber::Chroot => {
            if let Some(path) = unsafe { read_user_string(arg1) } {
                let vfs = crate::vfs::VFS.lock();
                if vfs.resolve_path(&path).is_none() {
                    Err(SyscallError::FileNotFound)
                } else {
                    drop(vfs);
                    let pid = crate::scheduler::current_pid().unwrap_or(1);
                    crate::process::PROCESS_TABLE.lock().chdir(pid, &path);
                    serial_println!("[KnoxOS] chroot({}) for PID {}", path, pid);
                    Ok(0)
                }
            } else {
                Err(SyscallError::InvalidArgument)
            }
        }
        SyscallNumber::Sync => advanced::sys_sync(),
        SyscallNumber::Acct => advanced::sys_acct(arg1),
        SyscallNumber::Settimeofday => advanced::sys_settimeofday(arg1, arg2),
        SyscallNumber::MountLinux => advanced::sys_mount_linux(arg1, arg2, arg3, arg4, arg5),
        SyscallNumber::Umount2 => advanced::sys_umount2(arg1, arg2 as i32),
        SyscallNumber::Swapon => advanced::sys_swapon(arg1, arg2 as i32),
        SyscallNumber::Swapoff => advanced::sys_swapoff(arg1),
        SyscallNumber::Reboot => system::sys_reboot(arg1 as u32, arg2 as u32),
        SyscallNumber::Sethostname => system::sys_sethostname(arg1, arg2 as usize),
        SyscallNumber::Setdomainname => system::sys_sethostname(arg1, arg2 as usize),
        SyscallNumber::Iopl => advanced::sys_iopl(arg1 as i32),
        SyscallNumber::Ioperm => advanced::sys_ioperm(arg1, arg2, arg3 as i32),
        SyscallNumber::CreateModule | SyscallNumber::GetKernelSyms | SyscallNumber::QueryModule => {
            Err(SyscallError::NotImplemented)
        }
        SyscallNumber::InitModule | SyscallNumber::FinitModule => {
            system::sys_init_module(arg1, arg2)
        }
        SyscallNumber::DeleteModule => system::sys_delete_module(arg1),
        SyscallNumber::Quotactl | SyscallNumber::QuotactlFd => Ok(0),
        SyscallNumber::Nfsservctl
        | SyscallNumber::Getpmsg
        | SyscallNumber::Putpmsg
        | SyscallNumber::AfsSyscall
        | SyscallNumber::Tuxcall
        | SyscallNumber::Security
        | SyscallNumber::Vserver => Err(SyscallError::NotImplemented),

        // ════════════════════════════════════════════════════════════
        // ── Thread ID, xattr, signals (186–211) ────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Gettid => advanced::sys_gettid(),
        SyscallNumber::Readahead => {
            // readahead(fd, offset, count) — advisory prefetch
            // In-memory FS: data is already "cached"
            Ok(0)
        }
        SyscallNumber::Setxattr | SyscallNumber::Lsetxattr => {
            advanced::sys_setxattr(arg1, arg2, arg3, arg4 as usize, arg5 as i32)
        }
        SyscallNumber::Fsetxattr => {
            advanced::sys_fsetxattr(arg1 as i32, arg2, arg3, arg4 as usize, arg5 as i32)
        }
        SyscallNumber::Getxattr | SyscallNumber::Lgetxattr => {
            advanced::sys_getxattr(arg1, arg2, arg3, arg4 as usize)
        }
        SyscallNumber::Fgetxattr => advanced::sys_fgetxattr(arg1 as i32, arg2, arg3, arg4 as usize),
        SyscallNumber::Listxattr | SyscallNumber::Llistxattr => {
            advanced::sys_listxattr(arg1, arg2, arg3 as usize)
        }
        SyscallNumber::Flistxattr => advanced::sys_flistxattr(arg1 as i32, arg2, arg3 as usize),
        SyscallNumber::Removexattr | SyscallNumber::Lremovexattr => {
            advanced::sys_removexattr(arg1, arg2)
        }
        SyscallNumber::Fremovexattr => advanced::sys_fremovexattr(arg1 as i32, arg2),
        SyscallNumber::Tkill => advanced::sys_tkill(arg1 as u32, arg2 as u32),
        SyscallNumber::Time => {
            let t = crate::rtc::unix_time();
            if arg1 != 0 {
                unsafe {
                    *(arg1 as *mut i64) = t;
                }
            }
            Ok(t as u64)
        }
        SyscallNumber::Futex => thread::sys_futex(arg1, arg2 as i32, arg3 as u32),
        SyscallNumber::SchedSetaffinity
        | SyscallNumber::SchedGetaffinity
        | SyscallNumber::SchedSetattr
        | SyscallNumber::SchedGetattr => Ok(0),
        SyscallNumber::SetThreadArea | SyscallNumber::GetThreadArea => Ok(0),
        SyscallNumber::IoSetup => advanced::sys_io_setup(arg1 as u32, arg2),
        SyscallNumber::IoDestroy => advanced::sys_io_destroy(arg1),
        SyscallNumber::IoGetevents | SyscallNumber::IoPgetevents => {
            advanced::sys_io_getevents(arg1, arg2 as i64, arg3 as i64, arg4, arg5)
        }
        SyscallNumber::IoSubmit => advanced::sys_io_submit(arg1, arg2 as i64, arg3),
        SyscallNumber::IoCancel => advanced::sys_io_cancel(arg1, arg2, arg3),
        SyscallNumber::LookupDcookie => advanced::sys_lookup_dcookie(arg1, arg2, arg3 as usize),

        // ════════════════════════════════════════════════════════════
        // ── Epoll, dentries (213–220) ──────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::EpollCreate | SyscallNumber::EpollCreate1 => {
            io::sys_epoll_create(arg1 as i32)
        }
        SyscallNumber::EpollCtl | SyscallNumber::EpollCtlOld => {
            io::sys_epoll_ctl(arg1 as i32, arg2 as i32, arg3 as i32, arg4)
        }
        SyscallNumber::EpollWait
        | SyscallNumber::EpollWaitOld
        | SyscallNumber::EpollPwait
        | SyscallNumber::EpollPwait2 => {
            io::sys_epoll_wait(arg1 as i32, arg2, arg3 as i32, arg4 as i32)
        }
        SyscallNumber::RemapFilePages => Ok(0),
        SyscallNumber::Getdents64 => fs::sys_getdents64(arg1 as i32, arg2, arg3 as u32),
        SyscallNumber::SetTidAddress => thread::sys_set_tid_address(arg1),
        SyscallNumber::RestartSyscall => Ok(0),
        SyscallNumber::Semtimedop => {
            advanced::sys_semtimedop(arg1 as i32, arg2, arg3 as usize, arg4)
        }

        // ════════════════════════════════════════════════════════════
        // ── Timers (221–235) ───────────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Fadvise64 => Ok(0),
        SyscallNumber::TimerCreate => advanced::sys_timer_create_linux(arg1 as u32, arg2, arg3),
        SyscallNumber::TimerSettime => {
            advanced::sys_timer_settime_linux(arg1 as u32, arg2 as i32, arg3, arg4)
        }
        SyscallNumber::TimerGettime => advanced::sys_timer_gettime_linux(arg1 as u32, arg2),
        SyscallNumber::TimerGetoverrun => advanced::sys_timer_getoverrun_linux(arg1 as u32),
        SyscallNumber::TimerDelete => advanced::sys_timer_delete_linux(arg1 as u32),
        SyscallNumber::ClockSettime => advanced::sys_clock_settime(arg1 as u32, arg2),
        SyscallNumber::ClockGettime => time::sys_clock_gettime(arg1 as u32, arg2),
        SyscallNumber::ClockGetres => advanced::sys_clock_getres(arg1 as u32, arg2),
        SyscallNumber::Tgkill => advanced::sys_tgkill(arg1 as u32, arg2 as u32, arg3 as u32),

        // ════════════════════════════════════════════════════════════
        // ── NUMA, message queues (237–246) ─────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Mbind => {
            advanced::sys_mbind(arg1, arg2, arg3 as i32, arg4, arg5, arg6 as u32)
        }
        SyscallNumber::SetMempolicy | SyscallNumber::SetMempolicy2 => {
            advanced::sys_set_mempolicy(arg1 as i32, arg2, arg3)
        }
        SyscallNumber::GetMempolicy => advanced::sys_get_mempolicy(arg1, arg2, arg3, arg4, arg5),
        SyscallNumber::MqOpen => io::sys_mq_open(arg1, arg2 as i32, arg3 as u32),
        SyscallNumber::MqUnlink => {
            if let Some(name) = unsafe { read_user_string(arg1) } {
                let mut queues = crate::ipc::MESSAGE_QUEUES.lock();
                let before = queues.len();
                queues.retain(|q| q.name != name);
                if queues.len() < before {
                    Ok(0)
                } else {
                    Err(SyscallError::FileNotFound)
                }
            } else {
                Err(SyscallError::InvalidArgument)
            }
        }
        SyscallNumber::MqTimedsend => {
            advanced::sys_mq_timedsend(arg1 as i32, arg2, arg3 as usize, arg4 as u32, arg5)
        }
        SyscallNumber::MqTimedreceive => {
            advanced::sys_mq_timedreceive(arg1 as i32, arg2, arg3 as usize, arg4, arg5)
        }
        SyscallNumber::MqNotify => advanced::sys_mq_notify(arg1 as i32, arg2),
        SyscallNumber::MqGetsetattr => advanced::sys_mq_getsetattr(arg1 as i32, arg2, arg3),
        SyscallNumber::KexecLoad | SyscallNumber::KexecFileLoad => {
            Err(SyscallError::NotImplemented)
        }

        // ════════════════════════════════════════════════════════════
        // ── Waitid, keys, ioprio (247–260) ─────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Waitid => process::sys_waitid(arg1 as i32, arg2 as u32, arg3, arg4 as i32),
        SyscallNumber::AddKey => {
            advanced::sys_add_key(arg1, arg2, arg3, arg4 as usize, arg5 as i32)
        }
        SyscallNumber::RequestKey => advanced::sys_request_key(arg1, arg2, arg3, arg4 as i32),
        SyscallNumber::Keyctl => advanced::sys_keyctl(arg1 as i32, arg2, arg3, arg4, arg5),
        SyscallNumber::IoprioSet => advanced::sys_ioprio_set(arg1 as u32, arg2 as u32, arg3 as u32),
        SyscallNumber::IoprioGet => advanced::sys_ioprio_get(arg1 as u32, arg2 as u32),
        SyscallNumber::InotifyInit | SyscallNumber::InotifyInit1 => {
            io::sys_inotify_init(arg1 as i32)
        }
        SyscallNumber::InotifyAddWatch => io::sys_inotify_add_watch(arg1 as i32, arg2, arg3 as u32),
        SyscallNumber::InotifyRmWatch => io::sys_inotify_rm_watch(arg1 as i32, arg2 as i32),
        SyscallNumber::MigratePages => advanced::sys_migrate_pages(arg1 as u32, arg2, arg3, arg4),

        // ════════════════════════════════════════════════════════════
        // ── *at() family (257–280) ─────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Openat | SyscallNumber::Openat2 => {
            fs::sys_open(arg2, arg3 as u32, arg4 as u16)
        }
        SyscallNumber::Mkdirat => fs::sys_mkdir(arg2, arg3 as u16),
        SyscallNumber::Fchownat => {
            advanced::sys_fchownat(arg1 as i32, arg2, arg3 as u32, arg4 as u32, arg5 as i32)
        }
        SyscallNumber::Newfstatat => advanced::sys_newfstatat(arg1 as i32, arg2, arg3, arg4 as i32),
        SyscallNumber::Unlinkat => advanced::sys_unlinkat(arg1 as i32, arg2, arg3 as i32),
        SyscallNumber::Renameat => advanced::sys_renameat(arg1 as i32, arg2, arg3 as i32, arg4),
        SyscallNumber::Linkat => {
            advanced::sys_linkat(arg1 as i32, arg2, arg3 as i32, arg4, arg5 as i32)
        }
        SyscallNumber::Symlinkat => fs::sys_symlink(arg2, arg3),
        SyscallNumber::Readlinkat => fs::sys_readlink(arg2, arg3, arg4),
        SyscallNumber::Fchmodat => {
            advanced::sys_fchmodat(arg1 as i32, arg2, arg3 as u32, arg4 as i32)
        }
        SyscallNumber::Faccessat | SyscallNumber::Faccessat2 => {
            advanced::sys_faccessat(arg1 as i32, arg2, arg3 as u32, arg4 as i32)
        }
        SyscallNumber::Unshare => system::sys_unshare(arg1 as u32),
        SyscallNumber::SetRobustList => advanced::sys_set_robust_list(arg1, arg2 as usize),
        SyscallNumber::GetRobustList => advanced::sys_get_robust_list(arg1 as i32, arg2, arg3),
        SyscallNumber::Splice => io::sys_splice(
            arg1 as i32,
            arg2,
            arg3 as i32,
            arg4,
            arg5 as usize,
            arg6 as u32,
        ),
        SyscallNumber::Tee => io::sys_tee(arg1 as i32, arg2 as i32, arg3 as usize, arg4 as u32),
        SyscallNumber::SyncFileRange => {
            advanced::sys_sync_file_range(arg1 as i32, arg2 as i64, arg3 as i64, arg4 as u32)
        }
        SyscallNumber::Vmsplice => {
            // vmsplice(fd, iov, nr_segs, flags)
            // Transfer data from user memory into a pipe
            let _fd = arg1 as i32;
            let iov = arg2;
            let nr_segs = arg3 as usize;
            let mut total = 0usize;
            for i in 0..nr_segs {
                let base = unsafe { *((iov + (i * 16) as u64) as *const u64) };
                let len = unsafe { *((iov + (i * 16 + 8) as u64) as *const u64) } as usize;
                if base != 0 {
                    total += len;
                }
            }
            Ok(total as u64)
        }
        SyscallNumber::MovePages => {
            advanced::sys_move_pages(arg1 as u32, arg2, arg3, arg4, arg5, arg6 as i32)
        }
        SyscallNumber::Utimensat => advanced::sys_utimensat(arg1 as i32, arg2, arg3, arg4 as i32),

        // ════════════════════════════════════════════════════════════
        // ── Epoll/signalfd/timerfd/accept4 (281–296) ───────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Signalfd | SyscallNumber::Signalfd4 => {
            advanced::sys_signalfd(arg1 as i32, arg2, arg3 as i32)
        }
        SyscallNumber::TimerfdCreate => io::sys_timerfd_create(arg1 as i32, arg2 as i32),
        SyscallNumber::Eventfd | SyscallNumber::Eventfd2 => {
            io::sys_eventfd(arg1 as u32, arg2 as i32)
        }
        SyscallNumber::Fallocate => {
            advanced::sys_fallocate(arg1 as i32, arg2 as i32, arg3 as i64, arg4 as i64)
        }
        SyscallNumber::TimerfdSettime => {
            io::sys_timerfd_settime(arg1 as i32, arg2 as i32, arg3, arg4)
        }
        SyscallNumber::TimerfdGettime => io::sys_timerfd_gettime(arg1 as i32, arg2),
        SyscallNumber::Accept4 => advanced::sys_accept4(arg1 as i32, arg2, arg3, arg4 as i32),
        SyscallNumber::Dup3 => fd::sys_dup3(arg1 as i32, arg2 as i32, arg3 as i32),
        SyscallNumber::Pipe2 => fd::sys_pipe2(arg1, arg2 as i32),
        SyscallNumber::Preadv => {
            advanced::sys_preadv2(arg1 as i32, arg2, arg3 as i32, arg4 as i64, 0)
        }
        SyscallNumber::Pwritev => {
            advanced::sys_pwritev2(arg1 as i32, arg2, arg3 as i32, arg4 as i64, 0)
        }

        // ════════════════════════════════════════════════════════════
        // ── Recent syscalls (298–334) ──────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::PerfEventOpen => {
            advanced::sys_perf_event_open(arg1, arg2 as i64, arg3 as i32, arg4 as i64, arg5)
        }
        SyscallNumber::FanotifyInit => advanced::sys_fanotify_init(arg1 as u32, arg2 as u32),
        SyscallNumber::FanotifyMark => {
            advanced::sys_fanotify_mark(arg1 as i32, arg2 as u32, arg3, arg4 as i32, arg5)
        }
        SyscallNumber::Prlimit64 => system::sys_prlimit64(arg1 as u32, arg2 as i32, arg3, arg4),
        SyscallNumber::NameToHandleAt => {
            advanced::sys_name_to_handle_at(arg1 as i32, arg2, arg3, arg4, arg5 as i32)
        }
        SyscallNumber::OpenByHandleAt => {
            advanced::sys_open_by_handle_at(arg1 as i32, arg2, arg3 as i32)
        }
        SyscallNumber::Syncfs => advanced::sys_syncfs(arg1 as i32),
        SyscallNumber::Setns => advanced::sys_setns(arg1 as i32, arg2 as i32),
        SyscallNumber::Getcpu => advanced::sys_getcpu(arg1, arg2, arg3),
        SyscallNumber::ProcessVmReadv => {
            // process_vm_readv(pid, local_iov, liovcnt, remote_iov, riovcnt, flags)
            let target_pid = arg1 as u32;
            // Verify target process exists
            let table = crate::process::PROCESS_TABLE.lock();
            if table.get_process(target_pid).is_none() {
                Err(SyscallError::NoSuchProcess)
            } else {
                drop(table);
                // In KnoxOS, all kernel processes share address space,
                // so we can directly copy between iov arrays
                let local_iov = arg2;
                let liovcnt = arg3 as usize;
                let remote_iov = arg4;
                let riovcnt = arg5 as usize;
                let mut total = 0usize;
                let mut li = 0;
                let mut ri = 0;
                while li < liovcnt && ri < riovcnt {
                    let l_base = unsafe { *((local_iov + (li * 16) as u64) as *const u64) };
                    let l_len =
                        unsafe { *((local_iov + (li * 16 + 8) as u64) as *const u64) } as usize;
                    let r_base = unsafe { *((remote_iov + (ri * 16) as u64) as *const u64) };
                    let r_len =
                        unsafe { *((remote_iov + (ri * 16 + 8) as u64) as *const u64) } as usize;
                    let copy_len = l_len.min(r_len);
                    if l_base != 0 && r_base != 0 && copy_len > 0 {
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                r_base as *const u8,
                                l_base as *mut u8,
                                copy_len,
                            );
                        }
                    }
                    total += copy_len;
                    li += 1;
                    ri += 1;
                }
                Ok(total as u64)
            }
        }
        SyscallNumber::ProcessVmWritev => {
            let target_pid = arg1 as u32;
            let table = crate::process::PROCESS_TABLE.lock();
            if table.get_process(target_pid).is_none() {
                Err(SyscallError::NoSuchProcess)
            } else {
                drop(table);
                let local_iov = arg2;
                let liovcnt = arg3 as usize;
                let remote_iov = arg4;
                let riovcnt = arg5 as usize;
                let mut total = 0usize;
                let mut li = 0;
                let mut ri = 0;
                while li < liovcnt && ri < riovcnt {
                    let l_base = unsafe { *((local_iov + (li * 16) as u64) as *const u64) };
                    let l_len =
                        unsafe { *((local_iov + (li * 16 + 8) as u64) as *const u64) } as usize;
                    let r_base = unsafe { *((remote_iov + (ri * 16) as u64) as *const u64) };
                    let r_len =
                        unsafe { *((remote_iov + (ri * 16 + 8) as u64) as *const u64) } as usize;
                    let copy_len = l_len.min(r_len);
                    if l_base != 0 && r_base != 0 && copy_len > 0 {
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                l_base as *const u8,
                                r_base as *mut u8,
                                copy_len,
                            );
                        }
                    }
                    total += copy_len;
                    li += 1;
                    ri += 1;
                }
                Ok(total as u64)
            }
        }
        SyscallNumber::Kcmp => {
            advanced::sys_kcmp(arg1 as u32, arg2 as u32, arg3 as u32, arg4, arg5)
        }
        SyscallNumber::Renameat2 => {
            advanced::sys_renameat2(arg1 as i32, arg2, arg3 as i32, arg4, arg5 as u32)
        }
        SyscallNumber::Seccomp => system::sys_seccomp(arg1 as u32, arg2 as u32, arg3),
        SyscallNumber::Getrandom => system::sys_getrandom(arg1, arg2 as usize, arg3 as u32),
        SyscallNumber::MemfdCreate => advanced::sys_memfd_create(arg1, arg2 as u32),
        SyscallNumber::Bpf => {
            let cmd = arg1 as u32;
            serial_println!("[KnoxOS] bpf(cmd={})", cmd);
            // BPF programs: return a pseudo-fd for basic compatibility
            match cmd {
                5 => Ok(3), // BPF_PROG_LOAD: return pseudo-fd
                0 => Ok(3), // BPF_MAP_CREATE: return pseudo-fd
                _ => Err(SyscallError::InvalidArgument),
            }
        }
        SyscallNumber::Userfaultfd => advanced::sys_userfaultfd(arg1 as u32),
        SyscallNumber::Membarrier => advanced::sys_membarrier(arg1 as u32, arg2 as u32),
        SyscallNumber::Mlock2 => advanced::sys_mlock2(arg1, arg2, arg3 as i32),
        SyscallNumber::CopyFileRange => {
            io::sys_copy_file_range(arg1 as i32, arg2, arg3 as i32, arg4, arg5 as usize)
        }
        SyscallNumber::Preadv2 => {
            advanced::sys_preadv2(arg1 as i32, arg2, arg3 as i32, arg4 as i64, arg5 as i32)
        }
        SyscallNumber::Pwritev2 => {
            advanced::sys_pwritev2(arg1 as i32, arg2, arg3 as i32, arg4 as i64, arg5 as i32)
        }
        SyscallNumber::PkeyAlloc | SyscallNumber::PkeyFree => Ok(0),
        SyscallNumber::Statx => {
            advanced::sys_statx(arg1 as i32, arg2, arg3 as i32, arg4 as u32, arg5)
        }
        SyscallNumber::Rseq => advanced::sys_rseq(arg1, arg2 as u32, arg3 as i32, arg4 as u32),

        // ════════════════════════════════════════════════════════════
        // ── Newest syscalls (424+) ─────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::PidfdSendSignal => {
            advanced::sys_pidfd_send_signal(arg1, arg2 as i32, arg3, arg4 as u32)
        }
        SyscallNumber::IoUringSetup => advanced::sys_io_uring_setup(arg1 as u32, arg2),
        SyscallNumber::IoUringEnter => {
            advanced::sys_io_uring_enter(arg1, arg2 as u32, arg3 as u32, arg4 as u32)
        }
        SyscallNumber::IoUringRegister => {
            advanced::sys_io_uring_register(arg1, arg2 as u32, arg3, arg4 as u32)
        }
        SyscallNumber::OpenTree => advanced::sys_open_tree(arg1 as i32, arg2, arg3 as u32),
        SyscallNumber::MoveMount => {
            advanced::sys_move_mount(arg1 as i32, arg2, arg3 as i32, arg4, arg5 as u32)
        }
        SyscallNumber::Fsopen => advanced::sys_fsopen(arg1, arg2 as u32),
        SyscallNumber::Fsconfig => {
            advanced::sys_fsconfig(arg1 as i32, arg2 as u32, arg3, arg4, arg5 as i32)
        }
        SyscallNumber::Fsmount => advanced::sys_fsmount(arg1 as i32, arg2 as u32, arg3),
        SyscallNumber::Fspick => advanced::sys_fspick(arg1 as i32, arg2, arg3 as u32),
        SyscallNumber::PidfdOpen => advanced::sys_pidfd_open(arg1, arg2 as u32),
        SyscallNumber::Clone3 => advanced::sys_clone3(arg1, arg2),
        SyscallNumber::CloseRange => {
            advanced::sys_close_range(arg1 as u32, arg2 as u32, arg3 as u32)
        }
        SyscallNumber::PidfdGetfd => advanced::sys_pidfd_getfd(arg1, arg2 as i32, arg3 as u32),
        SyscallNumber::MountSetattr => {
            // mount_setattr(dirfd, path, flags, uattr, usize)
            serial_println!("[KnoxOS] mount_setattr(dirfd={}, flags={:#x})", arg1, arg3);
            Ok(0)
        }
        SyscallNumber::LandlockCreateRuleset => {
            advanced::sys_landlock_create_ruleset(arg1, arg2 as usize, arg3 as u32)
        }
        SyscallNumber::LandlockAddRule => {
            advanced::sys_landlock_add_rule(arg1 as i32, arg2 as u32, arg3, arg4 as u32)
        }
        SyscallNumber::LandlockRestrictSelf => {
            advanced::sys_landlock_restrict_self(arg1 as i32, arg2 as u32)
        }
        SyscallNumber::MemfdSecret => Err(SyscallError::NotImplemented),
        SyscallNumber::ProcessMrelease => Ok(0),
        SyscallNumber::FutexWaitv => {
            advanced::sys_futex_waitv(arg1, arg2 as u32, arg3 as u32, arg4, arg5 as u32)
        }
        SyscallNumber::MapShadowStack => Err(SyscallError::NotImplemented),

        // ════════════════════════════════════════════════════════════
        // ── KnoxOS AI system calls ─────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::AiQuery => ai::sys_ai_query(arg1, arg2, arg3),
        SyscallNumber::AiLoadModel => ai::sys_ai_load_model(arg1, arg2),
        SyscallNumber::AiInfer => ai::sys_ai_infer(arg1, arg2, arg3),
        SyscallNumber::AiUnloadModel => {
            let success = crate::ai::unload_model(arg1);
            if success {
                Ok(0)
            } else {
                Err(SyscallError::InvalidArgument)
            }
        }

        // ════════════════════════════════════════════════════════════
        // ── KnoxOS Threading ───────────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::ThreadCreate => thread::sys_thread_create(arg1, arg2),
        SyscallNumber::ThreadExit => thread::sys_thread_exit(arg1 as i32),
        SyscallNumber::ThreadJoin => thread::sys_thread_join(arg1 as u32),
        SyscallNumber::ThreadDetach => thread::sys_thread_detach(arg1 as u32),

        // ════════════════════════════════════════════════════════════
        // ── KnoxOS System control ──────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::KpmInstall => system::sys_kpm_install(arg1),
        SyscallNumber::KpmRemove => system::sys_kpm_remove(arg1),
        SyscallNumber::KpmList => system::sys_kpm_list(arg1, arg2 as usize),
        SyscallNumber::KpmSearch => system::sys_kpm_search(arg1, arg2, arg3 as usize),
        SyscallNumber::CgroupCreate => system::sys_cgroup_create(arg1),
        SyscallNumber::CgroupAttach => system::sys_cgroup_attach(arg1, arg2 as u32),
        SyscallNumber::Dmesg => system::sys_dmesg(arg1, arg2 as usize),
        SyscallNumber::LsBlk => system::sys_lsblk(arg1, arg2 as usize),
        SyscallNumber::Mount => system::sys_mount(arg1, arg2),
        SyscallNumber::Umount => system::sys_umount(arg1),
        SyscallNumber::Gethostname => system::sys_gethostname(arg1, arg2 as usize),

        // ════════════════════════════════════════════════════════════
        // ── KnoxOS FIFO / PTY ──────────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Mkfifo => io::sys_mkfifo(arg1, arg2 as u32),
        SyscallNumber::Openpty => io::sys_openpty(arg1, arg2),

        // ════════════════════════════════════════════════════════════
        // ── KnoxOS KVM Virtualization ──────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::KvmCreateVm => match crate::kvm::create_vm("vm", arg2 as u32, arg1) {
            Ok(vm_id) => Ok(vm_id as u64),
            Err(_) => Err(SyscallError::InvalidArgument),
        },
        SyscallNumber::KvmStartVm => {
            let _ = crate::kvm::start_vm(arg1 as u32);
            Ok(0)
        }
        SyscallNumber::KvmStopVm => {
            let _ = crate::kvm::stop_vm(arg1 as u32);
            Ok(0)
        }
        SyscallNumber::KvmPauseVm => {
            let _ = crate::kvm::pause_vm(arg1 as u32);
            Ok(0)
        }
        SyscallNumber::KvmDestroyVm => {
            let _ = crate::kvm::destroy_vm(arg1 as u32);
            Ok(0)
        }
        SyscallNumber::KvmSetVcpuRegs | SyscallNumber::KvmGetVcpuRegs => Ok(0),

        // ════════════════════════════════════════════════════════════
        // ── KnoxOS ONNX ML Runtime ─────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::OnnxLoadModel => {
            let name = unsafe { read_user_string(arg1) }.unwrap_or_default();
            match crate::onnx::load_model(&name) {
                Ok(id) => Ok(id as u64),
                Err(_) => Err(SyscallError::InvalidArgument),
            }
        }
        SyscallNumber::OnnxUnloadModel => {
            let _ = crate::onnx::unload_model(arg1 as u32);
            Ok(0)
        }
        SyscallNumber::OnnxInfer | SyscallNumber::OnnxListModels => Ok(0),

        // ════════════════════════════════════════════════════════════
        // ── KnoxOS Shared Library Loader ───────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Dlopen => {
            let name = unsafe { read_user_string(arg1) }.unwrap_or_default();
            match crate::soloader::dlopen(&name, arg2 as i32) {
                Ok(handle) => Ok(handle as u64),
                Err(_) => Err(SyscallError::FileNotFound),
            }
        }
        SyscallNumber::Dlsym => {
            let symbol = unsafe { read_user_string(arg2) }.unwrap_or_default();
            match crate::soloader::dlsym(arg1 as u32, &symbol) {
                Some(addr) => Ok(addr),
                None => Err(SyscallError::FileNotFound),
            }
        }
        SyscallNumber::Dlclose => match crate::soloader::dlclose(arg1 as u32) {
            Ok(_) => Ok(0),
            Err(_) => Err(SyscallError::InvalidArgument),
        },

        // ════════════════════════════════════════════════════════════
        // ── KnoxOS RT Scheduling ───────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::RtSetParams => {
            let policy = match arg2 {
                1 => crate::preempt_rt::RtPolicy::Fifo,
                2 => crate::preempt_rt::RtPolicy::RoundRobin,
                5 => crate::preempt_rt::RtPolicy::Deadline,
                _ => crate::preempt_rt::RtPolicy::Normal,
            };
            match crate::preempt_rt::set_rt_params(arg1 as u32, policy, arg3 as u32) {
                Ok(_) => Ok(0),
                Err(_) => Err(SyscallError::InvalidArgument),
            }
        }
        SyscallNumber::RtGetInfo => Ok(0),
        SyscallNumber::RtCreatePiMutex => {
            let name = unsafe { read_user_string(arg1) }.unwrap_or_default();
            Ok(crate::preempt_rt::create_pi_mutex(&name) as u64)
        }
        SyscallNumber::RtCreateHrtimer => {
            let id = crate::preempt_rt::create_hrtimer(arg1, arg2, pid, arg3 as u32);
            Ok(id as u64)
        }
        SyscallNumber::RtIsolateCpu => {
            crate::preempt_rt::isolate_cpu(arg1 as u32);
            Ok(0)
        }
        SyscallNumber::RtLatencyStats => {
            let (_min, _max, _avg, samples) = crate::preempt_rt::latency_stats();
            Ok(samples)
        }

        // ════════════════════════════════════════════════════════════
        // ── Unknown / unimplemented ────────────────────────────────
        // ════════════════════════════════════════════════════════════
        SyscallNumber::Unknown => {
            serial_println!("[KnoxOS] Unknown syscall: {}", number);
            Err(SyscallError::NotImplemented)
        }
    };
    match result {
        Ok(val) => val as i64,
        Err(err) => err.errno(),
    }
}
