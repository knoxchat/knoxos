use crate::serial_println;

use super::super::mount::{loopback_put, mount};
use super::super::ops::{
    fuse_create, fuse_link, fuse_lookup, fuse_mknod, fuse_readlink, fuse_rename, fuse_rename2,
    fuse_symlink, fuse_unlink,
};
use super::super::protocol::RENAME_NOREPLACE;

/// Initialize FUSE subsystem
pub const GATE_FUSE_UNLINK_MARKER: &str = "GATE_FUSE unlink";

/// UNLINK must drop the name so a later LOOKUP fails.
pub fn unlink_self_test() -> bool {
    const MP: &str = "/mnt/fuse-u";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE unlink FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "gone.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE unlink FAILED: put");
        return false;
    }
    if fuse_lookup(MP, 1, "gone.txt").is_err() {
        serial_println!("[fuse] GATE_FUSE unlink FAILED: lookup before");
        return false;
    }
    if let Err(e) = fuse_unlink(MP, 1, "gone.txt") {
        serial_println!("[fuse] GATE_FUSE unlink FAILED: unlink ({})", e);
        return false;
    }
    if fuse_lookup(MP, 1, "gone.txt").is_ok() {
        serial_println!("[fuse] GATE_FUSE unlink FAILED: name still present");
        return false;
    }
    serial_println!(
        "[fuse] {} (UNLINK then LOOKUP ENOENT)",
        GATE_FUSE_UNLINK_MARKER
    );
    true
}

pub const GATE_FUSE_RENAME_MARKER: &str = "GATE_FUSE rename";

/// RENAME must move the name so LOOKUP finds the new one.
pub fn rename_self_test() -> bool {
    const MP: &str = "/mnt/fuse-rn";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE rename FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "old.txt", b"rn").is_err() {
        serial_println!("[fuse] GATE_FUSE rename FAILED: put");
        return false;
    }
    let before = match fuse_lookup(MP, 1, "old.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE rename FAILED: lookup old ({})", e);
            return false;
        }
    };
    if let Err(e) = fuse_rename(MP, 1, "old.txt", "new.txt") {
        serial_println!("[fuse] GATE_FUSE rename FAILED: rename ({})", e);
        return false;
    }
    if fuse_lookup(MP, 1, "old.txt").is_ok() {
        serial_println!("[fuse] GATE_FUSE rename FAILED: old name still present");
        return false;
    }
    let after = match fuse_lookup(MP, 1, "new.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE rename FAILED: lookup new ({})", e);
            return false;
        }
    };
    if after.nodeid != before.nodeid {
        serial_println!(
            "[fuse] GATE_FUSE rename FAILED: ino {} -> {}",
            before.nodeid,
            after.nodeid
        );
        return false;
    }
    serial_println!(
        "[fuse] {} (RENAME old→new ino={})",
        GATE_FUSE_RENAME_MARKER,
        after.nodeid
    );
    true
}

pub const GATE_FUSE_SYMLINK_MARKER: &str = "GATE_FUSE symlink";

/// SYMLINK must create a link that LOOKUP returns with S_IFLNK and READLINK returns the target.
pub fn symlink_self_test() -> bool {
    const MP: &str = "/mnt/fuse-sl";
    const TARGET: &str = "/tmp/gate-target";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE symlink FAILED: mount ({})", e);
            return false;
        }
    }
    let entry = match fuse_symlink(MP, 1, "gate.link", TARGET) {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE symlink FAILED: symlink ({})", e);
            return false;
        }
    };
    if entry.attr.mode & 0o170000 != 0o120000 {
        serial_println!(
            "[fuse] GATE_FUSE symlink FAILED: mode={:#o} (want S_IFLNK)",
            entry.attr.mode
        );
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "gate.link") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE symlink FAILED: lookup ({})", e);
            return false;
        }
    };
    if looked.nodeid != entry.nodeid || looked.attr.mode & 0o170000 != 0o120000 {
        serial_println!(
            "[fuse] GATE_FUSE symlink FAILED: lookup ino={} mode={:#o}",
            looked.nodeid,
            looked.attr.mode
        );
        return false;
    }
    match fuse_readlink(MP, entry.nodeid) {
        Ok(got) if got == TARGET.as_bytes() => {
            serial_println!(
                "[fuse] {} (SYMLINK+READLINK ino={})",
                GATE_FUSE_SYMLINK_MARKER,
                entry.nodeid
            );
            true
        }
        other => {
            serial_println!(
                "[fuse] GATE_FUSE symlink FAILED: target len={}",
                other.map(|d| d.len()).unwrap_or(0)
            );
            false
        }
    }
}

pub const GATE_FUSE_LINK_MARKER: &str = "GATE_FUSE link";

/// LINK must add a name that LOOKUP finds with the same inode.
pub fn link_self_test() -> bool {
    const MP: &str = "/mnt/fuse-ln";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE link FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "old.txt", b"ln").is_err() {
        serial_println!("[fuse] GATE_FUSE link FAILED: put");
        return false;
    }
    let before = match fuse_lookup(MP, 1, "old.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE link FAILED: lookup old ({})", e);
            return false;
        }
    };
    if let Err(e) = fuse_link(MP, 1, "old.txt", "new.txt") {
        serial_println!("[fuse] GATE_FUSE link FAILED: link ({})", e);
        return false;
    }
    let after = match fuse_lookup(MP, 1, "new.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE link FAILED: lookup new ({})", e);
            return false;
        }
    };
    if after.nodeid != before.nodeid {
        serial_println!(
            "[fuse] GATE_FUSE link FAILED: ino {} -> {}",
            before.nodeid,
            after.nodeid
        );
        return false;
    }
    if fuse_lookup(MP, 1, "old.txt").is_err() {
        serial_println!("[fuse] GATE_FUSE link FAILED: old name missing");
        return false;
    }
    serial_println!(
        "[fuse] {} (LINK old+new ino={})",
        GATE_FUSE_LINK_MARKER,
        after.nodeid
    );
    true
}

pub const GATE_FUSE_CREATE_MARKER: &str = "GATE_FUSE create";

/// CREATE must make a regular file that LOOKUP returns with S_IFREG.
pub fn create_self_test() -> bool {
    const MP: &str = "/mnt/fuse-c";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE create FAILED: mount ({})", e);
            return false;
        }
    }
    let entry = match fuse_create(MP, 1, "gatec.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE create FAILED: create ({})", e);
            return false;
        }
    };
    if entry.attr.mode & 0o170000 != 0o100000 {
        serial_println!(
            "[fuse] GATE_FUSE create FAILED: mode={:#o} (want S_IFREG)",
            entry.attr.mode
        );
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "gatec.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE create FAILED: lookup ({})", e);
            return false;
        }
    };
    if looked.nodeid != entry.nodeid || looked.attr.mode & 0o170000 != 0o100000 {
        serial_println!(
            "[fuse] GATE_FUSE create FAILED: lookup ino={} mode={:#o}",
            looked.nodeid,
            looked.attr.mode
        );
        return false;
    }
    serial_println!(
        "[fuse] {} (CREATE+LOOKUP S_IFREG ino={})",
        GATE_FUSE_CREATE_MARKER,
        entry.nodeid
    );
    true
}

pub const GATE_FUSE_MKNOD_MARKER: &str = "GATE_FUSE mknod";
const GATE_FUSE_MKNOD_MODE: u32 = 0o020666;

/// MKNOD must create a special file LOOKUP returns with S_IFCHR.
pub fn mknod_self_test() -> bool {
    const MP: &str = "/mnt/fuse-mn";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE mknod FAILED: mount ({})", e);
            return false;
        }
    }
    let entry = match fuse_mknod(MP, 1, "gate.dev", GATE_FUSE_MKNOD_MODE) {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE mknod FAILED: mknod ({})", e);
            return false;
        }
    };
    if entry.attr.mode & 0o170000 != 0o020000 {
        serial_println!(
            "[fuse] GATE_FUSE mknod FAILED: mode={:#o} (want S_IFCHR)",
            entry.attr.mode
        );
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "gate.dev") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE mknod FAILED: lookup ({})", e);
            return false;
        }
    };
    if looked.nodeid != entry.nodeid || looked.attr.mode & 0o170000 != 0o020000 {
        serial_println!(
            "[fuse] GATE_FUSE mknod FAILED: lookup ino={} mode={:#o}",
            looked.nodeid,
            looked.attr.mode
        );
        return false;
    }
    serial_println!(
        "[fuse] {} (MKNOD+LOOKUP S_IFCHR ino={})",
        GATE_FUSE_MKNOD_MARKER,
        entry.nodeid
    );
    true
}

pub const GATE_FUSE_RENAME2_MARKER: &str = "GATE_FUSE rename2";

/// RENAME2 flags=0 moves the name; RENAME_NOREPLACE fails if dest exists; unknown old fails.
pub fn rename2_self_test() -> bool {
    const MP: &str = "/mnt/fuse-rename2";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE rename2 FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "old2.txt", b"knoxos-fuse-r2\n").is_err() {
        serial_println!("[fuse] GATE_FUSE rename2 FAILED: put old");
        return false;
    }
    if let Err(e) = fuse_rename2(MP, 1, "old2.txt", "new2.txt", 0) {
        serial_println!("[fuse] GATE_FUSE rename2 FAILED: rename2 ({})", e);
        return false;
    }
    if fuse_lookup(MP, 1, "old2.txt").is_ok() {
        serial_println!("[fuse] GATE_FUSE rename2 FAILED: old name still present");
        return false;
    }
    if fuse_lookup(MP, 1, "new2.txt").is_err() {
        serial_println!("[fuse] GATE_FUSE rename2 FAILED: new name missing");
        return false;
    }
    if loopback_put(MP, "src2.txt", b"src\n").is_err()
        || loopback_put(MP, "dst2.txt", b"dst\n").is_err()
    {
        serial_println!("[fuse] GATE_FUSE rename2 FAILED: put clash");
        return false;
    }
    if fuse_rename2(MP, 1, "src2.txt", "dst2.txt", RENAME_NOREPLACE).is_ok() {
        serial_println!("[fuse] GATE_FUSE rename2 FAILED: NOREPLACE dest succeeded");
        return false;
    }
    if fuse_rename2(MP, 1, "missing2.txt", "gone2.txt", 0).is_ok() {
        serial_println!("[fuse] GATE_FUSE rename2 FAILED: missing old succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (RENAME2 then NOREPLACE then missing)",
        GATE_FUSE_RENAME2_MARKER
    );
    true
}
