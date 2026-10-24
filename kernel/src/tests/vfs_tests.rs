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
fn test_xhci_get_interface() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::getiface_self_test(),
        "XHCI GET_INTERFACE must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_clear_feature() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::clear_self_test(),
        "XHCI CLEAR_FEATURE must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_bulk_out() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::bulk_self_test(),
        "XHCI bulk OUT must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_set_feature() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::set_self_test(),
        "XHCI SET_FEATURE must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_bulk_in() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::bulk_in_self_test(),
        "XHCI bulk IN must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_set_descriptor() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::set_descriptor_self_test(),
        "XHCI SET_DESCRIPTOR must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_isoch_in() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::isoch_in_self_test(),
        "XHCI isochronous IN must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_get_string_descriptor() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::string_self_test(),
        "XHCI GET_DESCRIPTOR(String) must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_interrupt_in() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::interrupt_in_self_test(),
        "XHCI interrupt IN must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_noop_command() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::noop_self_test(),
        "XHCI No-Op Command must complete with cc=1"
    );
}

#[test_case]
fn test_xhci_disable_slot() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::disable_self_test(),
        "XHCI Disable Slot must complete with cc=1"
    );
}

#[test_case]
fn test_xhci_evaluate_context() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::eval_self_test(),
        "XHCI Evaluate Context must complete with cc=1"
    );
}

#[test_case]
fn test_xhci_stop_endpoint() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::stop_self_test(),
        "XHCI Stop Endpoint must complete with cc=1"
    );
}

#[test_case]
fn test_xhci_reset_endpoint() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::resetep_self_test(),
        "XHCI Reset Endpoint must complete with cc=1"
    );
}

#[test_case]
fn test_xhci_set_tr_dequeue() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::settr_self_test(),
        "XHCI Set TR Dequeue Pointer must complete with cc=1"
    );
}

#[test_case]
fn test_xhci_transfer_noop() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::trnoop_self_test(),
        "XHCI Transfer No-Op TRB must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_bos_descriptor() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::bos_self_test(),
        "XHCI GET_DESCRIPTOR(BOS) must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_device_qualifier() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::qual_self_test(),
        "XHCI GET_DESCRIPTOR(Device Qualifier) must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_other_speed_config() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::oscfg_self_test(),
        "XHCI GET_DESCRIPTOR(Other Speed Configuration) must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_synch_frame() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::synch_self_test(),
        "XHCI SYNCH_FRAME must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_set_sel() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::setsel_self_test(),
        "XHCI SET_SEL must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_set_isoch_delay() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::isochdelay_self_test(),
        "XHCI SET_ISOCH_DELAY must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_hid_descriptor() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::hid_self_test(),
        "XHCI GET_DESCRIPTOR(HID) must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_hid_report_descriptor() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::hidreport_self_test(),
        "XHCI GET_DESCRIPTOR(HID Report) must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_hid_get_report() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::getreport_self_test(),
        "XHCI HID GET_REPORT must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_hid_set_report() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::setreport_self_test(),
        "XHCI HID SET_REPORT must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_hid_get_idle() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::getidle_self_test(),
        "XHCI HID GET_IDLE must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_hid_set_idle() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::setidle_self_test(),
        "XHCI HID SET_IDLE must complete with a Transfer Event"
    );
}

#[test_case]
fn test_xhci_reset_device() {
    if !crate::usb::is_available() {
        return;
    }
    assert!(
        crate::usb::resetdev_self_test(),
        "XHCI Reset Device must complete with cc=1"
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
fn test_fuse_create_s_ifreg() {
    assert!(
        crate::fuse::create_self_test(),
        "FUSE CREATE must create a file LOOKUP returns with S_IFREG"
    );
}

#[test_case]
fn test_fuse_getattr_size() {
    assert!(
        crate::fuse::getattr_self_test(),
        "FUSE GETATTR must return size and S_IFREG for a created file"
    );
}

#[test_case]
fn test_fuse_setattr_size() {
    assert!(
        crate::fuse::setattr_self_test(),
        "FUSE SETATTR must change size so GETATTR/READ see the new length"
    );
}

#[test_case]
fn test_fuse_readdir_names() {
    assert!(
        crate::fuse::readdir_self_test(),
        "FUSE READDIR must list created files and directories"
    );
}

#[test_case]
fn test_fuse_mknod_s_ifchr() {
    assert!(
        crate::fuse::mknod_self_test(),
        "FUSE MKNOD must create a special file LOOKUP returns with S_IFCHR"
    );
}

#[test_case]
fn test_fuse_statfs_bsize() {
    assert!(
        crate::fuse::statfs_self_test(),
        "FUSE STATFS must report bsize 4096 and count created names"
    );
}

#[test_case]
fn test_fuse_setxattr_roundtrip() {
    assert!(
        crate::fuse::setxattr_self_test(),
        "FUSE SETXATTR must store bytes that a later GETXATTR returns"
    );
}

#[test_case]
fn test_fuse_getxattr_missing() {
    assert!(
        crate::fuse::getxattr_self_test(),
        "FUSE GETXATTR must return a stored value and fail for a missing name"
    );
}

#[test_case]
fn test_fuse_listxattr_names() {
    assert!(
        crate::fuse::listxattr_self_test(),
        "FUSE LISTXATTR must include a name stored by SETXATTR"
    );
}

#[test_case]
fn test_fuse_removexattr_drops_name() {
    assert!(
        crate::fuse::removexattr_self_test(),
        "FUSE REMOVEXATTR must drop a stored name so a later GETXATTR fails"
    );
}

#[test_case]
fn test_fuse_access_bits() {
    assert!(
        crate::fuse::access_self_test(),
        "FUSE ACCESS must grant file bits from mode, not echo the request"
    );
}

#[test_case]
fn test_fuse_fsync_after_write() {
    assert!(
        crate::fuse::fsync_self_test(),
        "FUSE FSYNC after WRITE must succeed and unknown nodeid must fail"
    );
}

#[test_case]
fn test_fuse_open_fh() {
    assert!(
        crate::fuse::open_self_test(),
        "FUSE OPEN must return fh=nodeid and unknown nodeid must fail"
    );
}

#[test_case]
fn test_fuse_release_after_open() {
    assert!(
        crate::fuse::release_self_test(),
        "FUSE RELEASE after OPEN must succeed; missing nodeid or mismatched fh must fail"
    );
}

#[test_case]
fn test_fuse_flush_after_write() {
    assert!(
        crate::fuse::flush_self_test(),
        "FUSE FLUSH after WRITE must succeed and unknown nodeid must fail"
    );
}

#[test_case]
fn test_fuse_fsyncdir() {
    assert!(
        crate::fuse::fsyncdir_self_test(),
        "FUSE FSYNCDIR of a directory must succeed; a file or unknown nodeid must fail"
    );
}

#[test_case]
fn test_fuse_opendir_fh() {
    assert!(
        crate::fuse::opendir_self_test(),
        "FUSE OPENDIR must return fh=nodeid; a file or unknown nodeid must fail"
    );
}

#[test_case]
fn test_fuse_releasedir_after_opendir() {
    assert!(
        crate::fuse::releasedir_self_test(),
        "FUSE RELEASEDIR after OPENDIR must succeed; a file, unknown nodeid, or mismatched fh must fail"
    );
}

#[test_case]
fn test_fuse_getlk_unlck() {
    assert!(
        crate::fuse::getlk_self_test(),
        "FUSE GETLK of a known file with no lock must be F_UNLCK; unknown nodeid must fail"
    );
}

#[test_case]
fn test_fuse_setlk_then_getlk() {
    assert!(
        crate::fuse::setlk_self_test(),
        "FUSE SETLK F_WRLCK must be visible to GETLK from another pid; unknown nodeid must fail"
    );
}

#[test_case]
fn test_fuse_setlkw_then_getlk() {
    assert!(
        crate::fuse::setlkw_self_test(),
        "FUSE SETLKW F_WRLCK must be visible to GETLK from another pid; unknown nodeid must fail"
    );
}

#[test_case]
fn test_fuse_bmap_block() {
    assert!(
        crate::fuse::bmap_self_test(),
        "FUSE BMAP of a known file must map logical block 0 to nodeid; unknown nodeid must fail"
    );
}

#[test_case]
fn test_fuse_destroy_mount() {
    assert!(
        crate::fuse::destroy_self_test(),
        "FUSE DESTROY of a known mount must succeed; unknown mount must fail"
    );
}

#[test_case]
fn test_fuse_ioctl_cmd() {
    assert!(
        crate::fuse::ioctl_self_test(),
        "FUSE IOCTL of a known file must echo cmd; unknown nodeid must fail"
    );
}

#[test_case]
fn test_fuse_poll_events() {
    assert!(
        crate::fuse::poll_self_test(),
        "FUSE POLL of a known file with data must set POLLIN; unknown nodeid must fail"
    );
}

#[test_case]
fn test_fuse_fallocate_size() {
    assert!(
        crate::fuse::fallocate_self_test(),
        "FUSE FALLOCATE of a known file must extend size; unknown nodeid must fail"
    );
}

#[test_case]
fn test_fuse_readdirplus_attrs() {
    assert!(
        crate::fuse::readdirplus_self_test(),
        "FUSE READDIRPLUS of the root must list names with size; unknown nodeid must fail"
    );
}

#[test_case]
fn test_fuse_rename2_noreplace() {
    assert!(
        crate::fuse::rename2_self_test(),
        "FUSE RENAME2 must move a name, fail NOREPLACE if dest exists, and fail if old is missing"
    );
}

#[test_case]
fn test_fuse_lseek() {
    assert!(
        crate::fuse::lseek_self_test(),
        "FUSE LSEEK SEEK_SET 0 must be 0, SEEK_END the size, and unknown nodeid must fail"
    );
}

#[test_case]
fn test_fuse_copy_file_range() {
    assert!(
        crate::fuse::copy_file_range_self_test(),
        "FUSE COPY_FILE_RANGE must copy bytes between known files; unknown nodeid must fail"
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
fn test_xfs_mount() {
    assert!(
        crate::xfs::mount_self_test(),
        "XFS mount must read GATE.TXT through the block layer"
    );
}

#[test_case]
fn test_zfs_mount() {
    assert!(
        crate::zfs::mount_self_test(),
        "ZFS mount must read GATE.TXT through the block layer"
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

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_create_then_lookup() {
    assert!(
        crate::nfs::create_self_test(),
        "NFS4 CREATE must add a name so LOOKUP finds it"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_link_then_lookup() {
    assert!(
        crate::nfs::link_self_test(),
        "NFS4 LINK must add a name so LOOKUP finds it while the old name remains"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_readlink_target() {
    assert!(
        crate::nfs::readlink_self_test(),
        "NFS4 CREATE NF4LNK then READLINK must return the target"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_setattr_size() {
    assert!(
        crate::nfs::setattr_self_test(),
        "NFS4 SETATTR must change size so GETATTR/READ see the new length"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_readdir_names() {
    assert!(
        crate::nfs::readdir_self_test(),
        "NFS4 READDIR must list created files and directories"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_access_bits() {
    assert!(
        crate::nfs::access_self_test(),
        "NFS4 ACCESS must grant file bits from mode, not echo the request"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_commit_persist() {
    assert!(
        crate::nfs::commit_self_test(),
        "NFS4 COMMIT must persist an unstable WRITE so a later READ returns it"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_getfh_child() {
    assert!(
        crate::nfs::getfh_self_test(),
        "NFS4 GETFH after LOOKUP must return a child handle distinct from the root"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_putfh_restore() {
    assert!(
        crate::nfs::putfh_self_test(),
        "NFS4 PUTFH must restore a GETFH handle and reject an unknown handle as STALE"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_lookupp_parent() {
    assert!(
        crate::nfs::lookupp_self_test(),
        "NFS4 LOOKUPP of a child must return the export root; LOOKUPP of root is NOENT"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_restorefh() {
    assert!(
        crate::nfs::restorefh_self_test(),
        "NFS4 RESTOREFH must restore SAVEFH and reject a missing SAVEFH as NOFILEHANDLE"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_putpubfh() {
    assert!(
        crate::nfs::putpubfh_self_test(),
        "NFS4 PUTPUBFH after LOOKUP must set the current handle to the export root"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_verify() {
    assert!(
        crate::nfs::verify_self_test(),
        "NFS4 VERIFY must accept a matching size, reject a mismatch as NOT_SAME, and missing FH as NOFILEHANDLE"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_nverify() {
    assert!(
        crate::nfs::nverify_self_test(),
        "NFS4 NVERIFY must accept a mismatch, reject a match as SAME, and missing FH as NOFILEHANDLE"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_setclientid_confirm() {
    assert!(
        crate::nfs::confirm_self_test(),
        "NFS4 SETCLIENTID_CONFIRM must accept the issued client_id and reject a wrong id as STALE_CLIENTID"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_close() {
    assert!(
        crate::nfs::close_self_test(),
        "NFS4 CLOSE must accept the issued OPEN state_id, reject a wrong id as BAD_STATEID, and missing FH as NOFILEHANDLE"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_renew() {
    assert!(
        crate::nfs::renew_self_test(),
        "NFS4 RENEW must accept a confirmed client_id and reject a wrong id as STALE_CLIENTID"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_lock() {
    assert!(
        crate::nfs::lock_self_test(),
        "NFS4 LOCK must accept a free range, deny overlap, and missing FH as NOFILEHANDLE"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_unlock() {
    assert!(
        crate::nfs::unlock_self_test(),
        "NFS4 LOCKU must drop a held range, deny a missing range, and missing FH as NOFILEHANDLE"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_lockt() {
    assert!(
        crate::nfs::lockt_self_test(),
        "NFS4 LOCKT must accept a free range, deny overlap, and missing FH as NOFILEHANDLE"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_release_lockowner() {
    assert!(
        crate::nfs::lockowner_self_test(),
        "NFS4 RELEASE_LOCKOWNER must drop that owner's locks; missing FH is NOFILEHANDLE"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_open_confirm() {
    assert!(
        crate::nfs::openconfirm_self_test(),
        "NFS4 OPEN_CONFIRM must accept the issued OPEN, deny a wrong id, and missing FH as NOFILEHANDLE"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_open_downgrade() {
    assert!(
        crate::nfs::downgrade_self_test(),
        "NFS4 OPEN_DOWNGRADE must accept a subset, reject a superset, and missing FH as NOFILEHANDLE"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_secinfo() {
    assert!(
        crate::nfs::secinfo_self_test(),
        "NFS4 SECINFO must return AUTH_SYS for a known name, NOENT for missing, and NOFILEHANDLE without FH"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_openattr() {
    assert!(
        crate::nfs::openattr_self_test(),
        "NFS4 OPENATTR must create a named-attr directory, NOENT without createdir, and NOFILEHANDLE without FH"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_delegpurge() {
    assert!(
        crate::nfs::delegpurge_self_test(),
        "NFS4 DELEGPURGE must accept a confirmed client_id and return STALE_CLIENTID for a wrong id"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_delegreturn() {
    assert!(
        crate::nfs::delegreturn_self_test(),
        "NFS4 DELEGRETURN must accept an OPEN state_id, BAD_STATEID a second time, and NOFILEHANDLE without FH"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_exchange_id() {
    assert!(
        crate::nfs::exchangeid_self_test(),
        "NFS4 EXCHANGE_ID must return a client_id and INVAL for an empty name"
    );
}

#[cfg(feature = "net")]
#[test_case]
fn test_nfs_create_session() {
    assert!(
        crate::nfs::session_self_test(),
        "NFS4 CREATE_SESSION must accept an EXCHANGE_ID client_id and return STALE_CLIENTID for a wrong id"
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
