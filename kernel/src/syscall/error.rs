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
