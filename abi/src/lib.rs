//! Syscall **contract**: numbers, errors, and shared operand structs.
//!
//! Handlers live in `kernel/src/syscall/`.
//! VFS inodes stay in the kernel; only the layouts copied to Ring 3 belong here.
#![no_std]

pub mod error;
pub mod numbers;
pub mod socket;
pub mod stat;

pub use error::{SyscallError, SyscallResult};
pub use numbers::SyscallNumber;
pub use socket::{
    AF_INET, AF_INET6, AF_LOCAL, AF_NETLINK, AF_UNIX, AddressFamily, SOCK_DGRAM, SOCK_RAW,
    SOCK_STREAM, SockAddrIn, SockAddrUn, SocketType,
};
pub use stat::{
    S_IFBLK, S_IFCHR, S_IFDIR, S_IFIFO, S_IFLNK, S_IFMT, S_IFREG, S_IFSOCK, STATX_BASIC_STATS,
    Stat, Statx, StatxTimestamp,
};

#[cfg(test)]
mod tests {
    use super::{SockAddrIn, SockAddrUn, Stat, Statx, SyscallNumber};
    use core::mem::{offset_of, size_of};

    #[test]
    fn write_is_one() {
        assert_eq!(SyscallNumber::Write as u64, 1);
    }

    #[test]
    fn linux_stat_is_144() {
        assert_eq!(size_of::<Stat>(), 144);
        assert_eq!(offset_of!(Stat, st_nlink), 16);
        assert_eq!(offset_of!(Stat, st_mode), 24);
        assert_eq!(offset_of!(Stat, st_size), 48);
    }

    #[test]
    fn linux_statx_is_256() {
        assert_eq!(size_of::<Statx>(), 256);
        assert_eq!(offset_of!(Statx, stx_size), 40);
        assert_eq!(offset_of!(Statx, stx_mtime), 112);
    }

    #[test]
    fn sockaddr_layouts() {
        assert_eq!(size_of::<SockAddrIn>(), 16);
        assert_eq!(offset_of!(SockAddrIn, sin_port), 2);
        assert_eq!(offset_of!(SockAddrUn, sun_path), 2);
        assert_eq!(size_of::<SockAddrUn>(), 110);
    }
}
