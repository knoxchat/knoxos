//! Filesystems and VFS. Kernel mode.
//!
//! Crate-root names stay (`crate::vfs`, `crate::ext4`, …) via `pub use`
//! in `lib.rs`.

pub mod bcache;
pub mod btrfs;
pub mod cifs;
pub mod cpio;
pub mod devfs;
#[cfg(feature = "stub-drivers")]
pub mod dm_crypt;
pub mod dm_verity;
pub mod exfat;
pub mod ext2;
#[cfg(feature = "fs-ext4")]
pub mod ext4;
pub mod fat32;
pub mod fd;
pub mod fsck;
pub mod fuse;
pub mod iso9660;
pub mod journal_replay;
pub mod mount;
pub mod mount_api;
#[cfg(feature = "net")]
pub mod nfs;
pub mod ntfs;
pub mod overlayfs;
#[cfg(feature = "stub-drivers")]
pub mod p9fs;
pub mod path;
pub mod persist;
pub mod procfs;
pub mod procfs_extended;
pub mod procfs_full;
pub mod quota;
pub mod sysfs;
pub mod tmpfs;
pub mod vfs;
pub mod xattr;
pub mod xfs;
pub mod zfs;
