// ═══════════════════════════════════════════════════════════════════════
// CONFIG PRESERVE / CONTEXT
// ═══════════════════════════════════════════════════════════════════════

use crate::vfs;

#[test_case]
fn test_package_upgrade_preserves_config() {
    vfs::init();
    vfs::ensure_directory("/etc/pkg");
    assert!(vfs::write_file_dispatch("/etc/pkg/cfg", b"keep-me"));
    assert!(vfs::write_file_dispatch("/usr/lib/pkg/bin", b"v1"));
    assert!(vfs::write_file_dispatch("/usr/lib/pkg/bin", b"v2"));
    assert_eq!(vfs::read_file_dispatch("/etc/pkg/cfg").unwrap(), b"keep-me");
    assert_eq!(vfs::read_file_dispatch("/usr/lib/pkg/bin").unwrap(), b"v2");
}

#[test_case]
fn test_package_downgrade_rollback() {
    vfs::write_file_dispatch("/usr/lib/pkg/bin", b"v2");
    vfs::write_file_dispatch("/usr/lib/pkg/bin", b"v1");
    assert_eq!(vfs::read_file_dispatch("/usr/lib/pkg/bin").unwrap(), b"v1");
}
