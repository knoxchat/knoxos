// ═══════════════════════════════════════════════════════════════════════
// FILESYSTEM STRESS TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::vfs;
use alloc::format;
use alloc::vec;

#[test_case]
fn test_concurrent_file_create() {
    vfs::init();
    vfs::ensure_directory("/tmp/stress");
    for i in 0..32u32 {
        let path = format!("/tmp/stress/f{}.txt", i);
        let payload = format!("file-{}", i);
        assert!(vfs::write_file_dispatch(&path, payload.as_bytes()));
    }
    for i in 0..32u32 {
        let path = format!("/tmp/stress/f{}.txt", i);
        let got = vfs::read_file_dispatch(&path).expect("missing");
        assert_eq!(got, format!("file-{}", i).as_bytes());
    }
}

#[test_case]
fn test_deep_directory_nesting() {
    vfs::ensure_directory("/tmp/deep");
    let mut path = alloc::string::String::from("/tmp/deep");
    for i in 0..16 {
        path.push_str("/d");
        path.push_str(&format!("{}", i));
        vfs::ensure_directory(&path);
    }
    let file = format!("{}/leaf", path);
    assert!(vfs::write_file_dispatch(&file, b"ok"));
    assert_eq!(vfs::read_file_dispatch(&file).unwrap(), b"ok");
}

#[test_case]
fn test_large_file_write() {
    let data = vec![0x5Au8; 64 * 1024];
    assert!(vfs::write_file_dispatch("/tmp/large.bin", &data));
    let read = vfs::read_file_dispatch("/tmp/large.bin").unwrap();
    assert_eq!(read.len(), data.len());
    assert_eq!(read, data);
}
