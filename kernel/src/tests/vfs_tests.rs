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
