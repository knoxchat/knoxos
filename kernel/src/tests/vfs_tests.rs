// ═══════════════════════════════════════════════════════════════════════
// VFS TESTS
// ═══════════════════════════════════════════════════════════════════════

use super::*;
use crate::vfs;
use alloc::string::String;
use alloc::vec;

#[test_case]
fn test_vfs_write_read() {
    vfs::init();
    let data = b"Hello, KnoxOS!";
    assert!(vfs::write_file_dispatch("/tmp/test.txt", data));
    let read = vfs::read_file_dispatch("/tmp/test.txt").expect("read failed");
    assert_eq!(&read[..], data);
}

#[test_case]
fn test_vfs_overwrite() {
    let data1 = b"first";
    let data2 = b"second";
    vfs::write_file_dispatch("/tmp/overwrite.txt", data1);
    vfs::write_file_dispatch("/tmp/overwrite.txt", data2);
    let read = vfs::read_file_dispatch("/tmp/overwrite.txt").unwrap();
    assert_eq!(&read[..], data2);
}

#[test_case]
fn test_vfs_nonexistent_read() {
    let result = vfs::read_file_dispatch("/nonexistent/path/file.txt");
    assert!(result.is_none());
}

#[test_case]
fn test_vfs_ensure_directory() {
    vfs::ensure_directory("/etc/knoxos/test");
    assert!(vfs::write_file_dispatch("/etc/knoxos/test/conf", b"ok"));
}

#[test_case]
fn test_persist_roundtrip_if_virtio() {
    if !crate::virtio_blk::is_available() {
        return;
    }
    assert!(
        crate::persist::roundtrip_self_test(),
        "virtio persist must restore a file after it is unlinked from RAM"
    );
}

#[test_case]
fn test_persist_journal_replay_if_virtio() {
    if !crate::virtio_blk::is_available() {
        return;
    }
    assert!(
        crate::persist::journal_recovery_self_test(),
        "committed journal must recover after simulated crash; uncommitted must not"
    );
}

#[test_case]
fn test_persist_root_if_virtio() {
    if !crate::virtio_blk::is_available() {
        return;
    }
    assert!(
        crate::persist::root_persist_self_test(),
        "files outside old persist prefixes must round-trip through VirtIO-blk"
    );
}

#[test_case]
fn test_persist_bin_if_virtio() {
    if !crate::virtio_blk::is_available() {
        return;
    }
    assert!(
        crate::persist::bin_persist_self_test(),
        "/bin must round-trip through VirtIO-blk"
    );
}

#[test_case]
fn test_persist_virtfs_dirs_if_virtio() {
    if !crate::virtio_blk::is_available() {
        return;
    }
    assert!(
        crate::persist::virtfs_persist_self_test(),
        "/dev /proc /sys /run directories must round-trip; files under them must not"
    );
}

#[test_case]
fn test_jbd2_replay_if_ramdisk() {
    if crate::block::ramdisk_index().is_none() {
        return;
    }
    assert!(
        crate::journal_replay::journal_recovery_self_test(),
        "committed JBD2 transaction must apply; torn descriptor must not"
    );
}

#[test_case]
fn test_usb_hid_urb_complete() {
    assert!(
        crate::usb_hid::urb_completion_self_test(),
        "HID interrupt URB must complete into evdev KEY_A"
    );
}

#[test_case]
fn test_usb_hid_no_ps2_cursor() {
    assert!(
        crate::usb_hid::cursor_self_test(),
        "USB HID mouse URB must move the GUI cursor without PS/2"
    );
}

#[test_case]
fn test_xhci_mmio_mapped() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::mmio_self_test(),
        "XHCI BAR0 must be mapped uncached with a valid CapLength"
    );
}

#[test_case]
fn test_xhci_enable_slot() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::slot_self_test(),
        "XHCI Enable Slot must complete with cc=1"
    );
}

#[test_case]
fn test_xhci_get_descriptor() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::descriptor_self_test(),
        "XHCI GET_DESCRIPTOR must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_set_configuration() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::config_self_test(),
        "XHCI SET_CONFIGURATION must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_address_device() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::addr_self_test(),
        "XHCI Address Device must complete with cc=1"
    );
}

#[test_case]
fn test_xhci_enum_no_fallback() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::enum_self_test(),
        "XHCI enumeration must complete every step from a live event"
    );
}

#[test_case]
fn test_xhci_configure_endpoint() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::ep_self_test(),
        "XHCI Configure Endpoint must complete with cc=1"
    );
}

#[test_case]
fn test_xhci_get_status() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::status_self_test(),
        "XHCI GET_STATUS must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_set_interface() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::iface_self_test(),
        "XHCI SET_INTERFACE must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_get_configuration() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::getcfg_self_test(),
        "XHCI GET_CONFIGURATION must complete with a Transfer Event"
    );
}

#[test_case]
fn test_fuse_write_roundtrip() {
    assert!(
        crate::fuse::write_self_test(),
        "FUSE WRITE must store bytes that a later READ returns"
    );
}

#[test_case]
fn test_fuse_unlink_drops_name() {
    assert!(
        crate::fuse::unlink_self_test(),
        "FUSE UNLINK must drop the name so LOOKUP fails"
    );
}

#[test_case]
fn test_fuse_mkdir_s_ifdir() {
    assert!(
        crate::fuse::mkdir_self_test(),
        "FUSE MKDIR must create a directory LOOKUP returns with S_IFDIR"
    );
}

#[test_case]
fn test_fuse_rmdir_drops_name() {
    assert!(
        crate::fuse::rmdir_self_test(),
        "FUSE RMDIR must drop the directory so LOOKUP fails"
    );
}

#[test_case]
fn test_fuse_rename_moves_name() {
    assert!(
        crate::fuse::rename_self_test(),
        "FUSE RENAME must move the name so LOOKUP finds the new one"
    );
}

#[test_case]
fn test_fuse_symlink_readlink() {
    assert!(
        crate::fuse::symlink_self_test(),
        "FUSE SYMLINK must create a link LOOKUP returns with S_IFLNK and READLINK returns the target"
    );
}

#[test_case]
fn test_fuse_link_same_ino() {
    assert!(
        crate::fuse::link_self_test(),
        "FUSE LINK must add a name LOOKUP finds with the same inode"
    );
}

#[test_case]
fn test_fat32_ramdisk() {
    assert!(
        crate::fat32::block_self_test(),
        "FAT32 BPB + directory $DATA must round-trip through the ramdisk"
    );
}

#[test_case]
fn test_fat32_vfat_mount() {
    assert!(
        crate::fat32::mount_self_test(),
        "FAT32 mount must read GATE.TXT through the block layer"
    );
}

#[test_case]
fn test_ext2_ramdisk() {
    assert!(
        crate::ext2::block_self_test(),
        "ext2 superblock + inode $DATA must round-trip through the ramdisk"
    );
}

#[test_case]
fn test_iso9660_ramdisk() {
    assert!(
        crate::iso9660::block_self_test(),
        "ISO9660 PVD + directory $DATA must round-trip through the ramdisk"
    );
}

#[test_case]
fn test_iso9660_mount() {
    assert!(
        crate::iso9660::mount_self_test(),
        "ISO9660 mount must read GATE.TXT through the block layer"
    );
}

#[test_case]
fn test_ntfs_mount() {
    assert!(
        crate::ntfs::mount_self_test(),
        "NTFS mount must read GATE.TXT through the block layer"
    );
}

#[test_case]
fn test_exfat_mount() {
    assert!(
        crate::exfat::mount_self_test(),
        "exFAT mount must read GATE.TXT through the block layer"
    );
}

#[test_case]
fn test_btrfs_mount() {
    assert!(
        crate::btrfs::mount_self_test(),
        "Btrfs mount must read GATE.TXT through the block layer"
    );
}

#[test_case]
fn test_io_uring_nop_write_sqe() {
    assert!(
        crate::io_uring::sqe_self_test(),
        "io_uring NOP+WRITE SQEs must complete with matching CQEs"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_write_roundtrip() {
    assert!(
        crate::nfs::write_self_test(),
        "NFS4 WRITE must store bytes that a later READ returns"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_remove_drops_name() {
    assert!(
        crate::nfs::remove_self_test(),
        "NFS4 REMOVE must drop the name so LOOKUP is NFS4ERR_NOENT"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_rename_moves_name() {
    assert!(
        crate::nfs::rename_self_test(),
        "NFS4 RENAME must move the name so LOOKUP finds the new one"
    );
}

#[test_case]
fn test_page_cache_writeback() {
    assert!(
        crate::page_cache::writeback_self_test(),
        "dirty middle page must flush without replacing sibling pages"
    );
}

#[test_case]
fn test_page_cache_lru_reclaim() {
    assert!(
        crate::page_cache::lru_reclaim_self_test(),
        "shrink must drop oldest clean pages and keep dirty"
    );
}
