//! GATE_FUSE serial markers and known-answer self-tests.

use alloc::vec::Vec;

use crate::serial_println;

use super::mount::{loopback_put, mount};
use super::ops::{
    fuse_access, fuse_bmap, fuse_copy_file_range, fuse_create, fuse_destroy, fuse_fallocate,
    fuse_flush, fuse_fsync, fuse_fsyncdir, fuse_getattr, fuse_getlk, fuse_getxattr, fuse_ioctl,
    fuse_link, fuse_listxattr, fuse_lookup, fuse_lseek, fuse_mkdir, fuse_mknod, fuse_open,
    fuse_opendir, fuse_poll, fuse_read, fuse_readdir, fuse_readdirplus, fuse_readlink,
    fuse_release, fuse_releasedir, fuse_removexattr, fuse_rename, fuse_rename2, fuse_rmdir,
    fuse_setattr, fuse_setlk, fuse_setlkw, fuse_setxattr, fuse_statfs, fuse_statx, fuse_symlink,
    fuse_syncfs, fuse_unlink, fuse_write,
};
use super::protocol::{
    F_UNLCK, F_WRLCK, FUSE_R_OK, FUSE_W_OK, FUSE_X_OK, FuseFileLock, POLLIN, RENAME_NOREPLACE,
    SEEK_END, SEEK_SET, STATX_BASIC_STATS,
};

pub const GATE_FUSE_MARKER: &str = "GATE_FUSE lookup";
const GATE_FUSE_PAYLOAD: &[u8] = b"knoxos-fuse\n";

pub(super) fn lookup_self_test() -> bool {
    if mount("/mnt/fuse", 1).is_err() {
        serial_println!("[fuse] GATE_FUSE FAILED: mount");
        return false;
    }
    if loopback_put("/mnt/fuse", "gate.txt", GATE_FUSE_PAYLOAD).is_err() {
        serial_println!("[fuse] GATE_FUSE FAILED: put");
        return false;
    }
    let entry = match fuse_lookup("/mnt/fuse", 1, "gate.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE FAILED: lookup ({})", e);
            return false;
        }
    };
    match fuse_read("/mnt/fuse", entry.nodeid, 0, 64) {
        Ok(data)
            if data == GATE_FUSE_PAYLOAD && entry.attr.size == GATE_FUSE_PAYLOAD.len() as u64 =>
        {
            serial_println!(
                "[fuse] {} (LOOKUP+READ {} bytes ino={})",
                GATE_FUSE_MARKER,
                data.len(),
                entry.nodeid
            );
            true
        }
        other => {
            serial_println!(
                "[fuse] GATE_FUSE FAILED read_len={}",
                other.map(|d| d.len()).unwrap_or(0)
            );
            false
        }
    }
}

pub const GATE_FUSE_WRITE_MARKER: &str = "GATE_FUSE write";
const GATE_FUSE_WRITE_PAYLOAD: &[u8] = b"knoxos-fuse-w\n";

/// WRITE opcode must store bytes that a subsequent READ returns.
pub fn write_self_test() -> bool {
    const MP: &str = "/mnt/fuse-w";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE write FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "gate.txt", b"old").is_err() {
        serial_println!("[fuse] GATE_FUSE write FAILED: put");
        return false;
    }
    let entry = match fuse_lookup(MP, 1, "gate.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE write FAILED: lookup ({})", e);
            return false;
        }
    };
    let written = match fuse_write(MP, entry.nodeid, 0, GATE_FUSE_WRITE_PAYLOAD) {
        Ok(n) => n,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE write FAILED: write ({})", e);
            return false;
        }
    };
    match fuse_read(MP, entry.nodeid, 0, 64) {
        Ok(data) if data == GATE_FUSE_WRITE_PAYLOAD && written == GATE_FUSE_WRITE_PAYLOAD.len() => {
            serial_println!(
                "[fuse] {} (WRITE+READ {} bytes ino={})",
                GATE_FUSE_WRITE_MARKER,
                data.len(),
                entry.nodeid
            );
            true
        }
        other => {
            serial_println!(
                "[fuse] GATE_FUSE write FAILED written={} read_len={}",
                written,
                other.map(|d| d.len()).unwrap_or(0)
            );
            false
        }
    }
}

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

pub const GATE_FUSE_MKDIR_MARKER: &str = "GATE_FUSE mkdir";

/// MKDIR must create a directory that LOOKUP returns with S_IFDIR.
pub fn mkdir_self_test() -> bool {
    const MP: &str = "/mnt/fuse-m";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE mkdir FAILED: mount ({})", e);
            return false;
        }
    }
    let entry = match fuse_mkdir(MP, 1, "gatedir") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE mkdir FAILED: mkdir ({})", e);
            return false;
        }
    };
    if entry.attr.mode & 0o170000 != 0o040000 {
        serial_println!(
            "[fuse] GATE_FUSE mkdir FAILED: mode={:#o} (want S_IFDIR)",
            entry.attr.mode
        );
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "gatedir") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE mkdir FAILED: lookup ({})", e);
            return false;
        }
    };
    if looked.nodeid != entry.nodeid || looked.attr.mode & 0o170000 != 0o040000 {
        serial_println!(
            "[fuse] GATE_FUSE mkdir FAILED: lookup ino={} mode={:#o}",
            looked.nodeid,
            looked.attr.mode
        );
        return false;
    }
    serial_println!(
        "[fuse] {} (MKDIR+LOOKUP S_IFDIR ino={})",
        GATE_FUSE_MKDIR_MARKER,
        entry.nodeid
    );
    true
}

pub const GATE_FUSE_RMDIR_MARKER: &str = "GATE_FUSE rmdir";

/// RMDIR must drop the directory so a later LOOKUP fails.
pub fn rmdir_self_test() -> bool {
    const MP: &str = "/mnt/fuse-rm";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE rmdir FAILED: mount ({})", e);
            return false;
        }
    }
    if fuse_mkdir(MP, 1, "gone").is_err() {
        serial_println!("[fuse] GATE_FUSE rmdir FAILED: mkdir");
        return false;
    }
    if fuse_lookup(MP, 1, "gone").is_err() {
        serial_println!("[fuse] GATE_FUSE rmdir FAILED: lookup before");
        return false;
    }
    if let Err(e) = fuse_rmdir(MP, 1, "gone") {
        serial_println!("[fuse] GATE_FUSE rmdir FAILED: rmdir ({})", e);
        return false;
    }
    if fuse_lookup(MP, 1, "gone").is_ok() {
        serial_println!("[fuse] GATE_FUSE rmdir FAILED: name still present");
        return false;
    }
    serial_println!(
        "[fuse] {} (RMDIR then LOOKUP ENOENT)",
        GATE_FUSE_RMDIR_MARKER
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

pub const GATE_FUSE_GETATTR_MARKER: &str = "GATE_FUSE getattr";
const GATE_FUSE_GETATTR_PAYLOAD: &[u8] = b"knoxos-fuse-g\n";

/// GETATTR must return size and S_IFREG for a created file.
pub fn getattr_self_test() -> bool {
    const MP: &str = "/mnt/fuse-g";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE getattr FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "gattr.txt", GATE_FUSE_GETATTR_PAYLOAD).is_err() {
        serial_println!("[fuse] GATE_FUSE getattr FAILED: put");
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "gattr.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE getattr FAILED: lookup ({})", e);
            return false;
        }
    };
    let attr = match fuse_getattr(MP, looked.nodeid) {
        Ok(a) => a,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE getattr FAILED: getattr ({})", e);
            return false;
        }
    };
    if attr.ino != looked.nodeid
        || attr.size != GATE_FUSE_GETATTR_PAYLOAD.len() as u64
        || attr.mode & 0o170000 != 0o100000
    {
        serial_println!(
            "[fuse] GATE_FUSE getattr FAILED: ino={} size={} mode={:#o}",
            attr.ino,
            attr.size,
            attr.mode
        );
        return false;
    }
    serial_println!(
        "[fuse] {} (GETATTR size={} S_IFREG ino={})",
        GATE_FUSE_GETATTR_MARKER,
        attr.size,
        attr.ino
    );
    true
}

pub const GATE_FUSE_SETATTR_MARKER: &str = "GATE_FUSE setattr";
const GATE_FUSE_SETATTR_PAYLOAD: &[u8] = b"knoxos-fuse-sa\n";
const GATE_FUSE_SETATTR_SIZE: u64 = 5;

/// SETATTR must change size so a later GETATTR/READ sees the new length.
pub fn setattr_self_test() -> bool {
    const MP: &str = "/mnt/fuse-sa";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE setattr FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "sattr.txt", GATE_FUSE_SETATTR_PAYLOAD).is_err() {
        serial_println!("[fuse] GATE_FUSE setattr FAILED: put");
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "sattr.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE setattr FAILED: lookup ({})", e);
            return false;
        }
    };
    let attr = match fuse_setattr(MP, looked.nodeid, GATE_FUSE_SETATTR_SIZE) {
        Ok(a) => a,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE setattr FAILED: setattr ({})", e);
            return false;
        }
    };
    if attr.ino != looked.nodeid || attr.size != GATE_FUSE_SETATTR_SIZE {
        serial_println!(
            "[fuse] GATE_FUSE setattr FAILED: ino={} size={}",
            attr.ino,
            attr.size
        );
        return false;
    }
    let got = match fuse_getattr(MP, looked.nodeid) {
        Ok(a) => a,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE setattr FAILED: getattr ({})", e);
            return false;
        }
    };
    match fuse_read(MP, looked.nodeid, 0, 64) {
        Ok(data)
            if got.size == GATE_FUSE_SETATTR_SIZE
                && data == GATE_FUSE_SETATTR_PAYLOAD[..GATE_FUSE_SETATTR_SIZE as usize] =>
        {
            serial_println!(
                "[fuse] {} (SETATTR size={} ino={})",
                GATE_FUSE_SETATTR_MARKER,
                got.size,
                got.ino
            );
            true
        }
        other => {
            serial_println!(
                "[fuse] GATE_FUSE setattr FAILED: getattr_size={} read_len={}",
                got.size,
                other.map(|d| d.len()).unwrap_or(0)
            );
            false
        }
    }
}

pub const GATE_FUSE_READDIR_MARKER: &str = "GATE_FUSE readdir";

/// READDIR of the root must list created files, directories, and names.
pub fn readdir_self_test() -> bool {
    const MP: &str = "/mnt/fuse-rd";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE readdir FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "a.txt", b"a").is_err() {
        serial_println!("[fuse] GATE_FUSE readdir FAILED: put");
        return false;
    }
    if fuse_mkdir(MP, 1, "bdir").is_err() {
        serial_println!("[fuse] GATE_FUSE readdir FAILED: mkdir");
        return false;
    }
    if fuse_create(MP, 1, "c.txt").is_err() {
        serial_println!("[fuse] GATE_FUSE readdir FAILED: create");
        return false;
    }
    let ents = match fuse_readdir(MP, 1) {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE readdir FAILED: readdir ({})", e);
            return false;
        }
    };
    let names: Vec<&str> = ents.iter().map(|(_, _, n)| n.as_str()).collect();
    if names.contains(&"a.txt") && names.contains(&"bdir") && names.contains(&"c.txt") {
        serial_println!(
            "[fuse] {} (READDIR {} names)",
            GATE_FUSE_READDIR_MARKER,
            ents.len()
        );
        true
    } else {
        serial_println!("[fuse] GATE_FUSE readdir FAILED: names={}", names.len());
        false
    }
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

pub const GATE_FUSE_STATFS_MARKER: &str = "GATE_FUSE statfs";

/// STATFS must report bsize 4096 and count created names.
pub fn statfs_self_test() -> bool {
    const MP: &str = "/mnt/fuse-sf";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE statfs FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "sf.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE statfs FAILED: put");
        return false;
    }
    if fuse_mkdir(MP, 1, "sfdir").is_err() {
        serial_println!("[fuse] GATE_FUSE statfs FAILED: mkdir");
        return false;
    }
    let st = match fuse_statfs(MP) {
        Ok(s) => s,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE statfs FAILED: statfs ({})", e);
            return false;
        }
    };
    if st.bsize == 4096 && st.namelen == 255 && st.files >= 2 && st.blocks == 1024 {
        serial_println!(
            "[fuse] {} (STATFS bsize={} files={})",
            GATE_FUSE_STATFS_MARKER,
            st.bsize,
            st.files
        );
        true
    } else {
        serial_println!(
            "[fuse] GATE_FUSE statfs FAILED: bsize={} namelen={} files={} blocks={}",
            st.bsize,
            st.namelen,
            st.files,
            st.blocks
        );
        false
    }
}

pub const GATE_FUSE_SETXATTR_MARKER: &str = "GATE_FUSE setxattr";
const GATE_FUSE_SETXATTR_NAME: &str = "user.knox";
const GATE_FUSE_SETXATTR_PAYLOAD: &[u8] = b"knoxos-fuse-sx\n";

/// SETXATTR must store bytes that a subsequent GETXATTR returns.
pub fn setxattr_self_test() -> bool {
    const MP: &str = "/mnt/fuse-sx";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE setxattr FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "sx.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE setxattr FAILED: put");
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "sx.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE setxattr FAILED: lookup ({})", e);
            return false;
        }
    };
    if let Err(e) = fuse_setxattr(
        MP,
        looked.nodeid,
        GATE_FUSE_SETXATTR_NAME,
        GATE_FUSE_SETXATTR_PAYLOAD,
    ) {
        serial_println!("[fuse] GATE_FUSE setxattr FAILED: setxattr ({})", e);
        return false;
    }
    match fuse_getxattr(MP, looked.nodeid, GATE_FUSE_SETXATTR_NAME) {
        Ok(data) if data == GATE_FUSE_SETXATTR_PAYLOAD => {
            serial_println!(
                "[fuse] {} (SETXATTR+GETXATTR {} bytes ino={})",
                GATE_FUSE_SETXATTR_MARKER,
                data.len(),
                looked.nodeid
            );
            true
        }
        other => {
            serial_println!(
                "[fuse] GATE_FUSE setxattr FAILED: getxattr len={}",
                other.map(|d| d.len()).unwrap_or(0)
            );
            false
        }
    }
}

pub const GATE_FUSE_GETXATTR_MARKER: &str = "GATE_FUSE getxattr";
const GATE_FUSE_GETXATTR_NAME: &str = "user.knox";
const GATE_FUSE_GETXATTR_PAYLOAD: &[u8] = b"knoxos-fuse-gx\n";

/// GETXATTR must return a stored value and fail for a missing name.
pub fn getxattr_self_test() -> bool {
    const MP: &str = "/mnt/fuse-gx";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE getxattr FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "gx.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE getxattr FAILED: put");
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "gx.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE getxattr FAILED: lookup ({})", e);
            return false;
        }
    };
    if let Err(e) = fuse_setxattr(
        MP,
        looked.nodeid,
        GATE_FUSE_GETXATTR_NAME,
        GATE_FUSE_GETXATTR_PAYLOAD,
    ) {
        serial_println!("[fuse] GATE_FUSE getxattr FAILED: setxattr ({})", e);
        return false;
    }
    let got = match fuse_getxattr(MP, looked.nodeid, GATE_FUSE_GETXATTR_NAME) {
        Ok(data) => data,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE getxattr FAILED: getxattr ({})", e);
            return false;
        }
    };
    if got != GATE_FUSE_GETXATTR_PAYLOAD {
        serial_println!("[fuse] GATE_FUSE getxattr FAILED: value len={}", got.len());
        return false;
    }
    if fuse_getxattr(MP, looked.nodeid, "user.missing").is_ok() {
        serial_println!("[fuse] GATE_FUSE getxattr FAILED: missing name present");
        return false;
    }
    serial_println!(
        "[fuse] {} (GETXATTR {} bytes, missing ENOENT)",
        GATE_FUSE_GETXATTR_MARKER,
        got.len()
    );
    true
}

pub const GATE_FUSE_LISTXATTR_MARKER: &str = "GATE_FUSE listxattr";
const GATE_FUSE_LISTXATTR_NAME: &str = "user.knox";
const GATE_FUSE_LISTXATTR_PAYLOAD: &[u8] = b"knoxos-fuse-lx\n";

fn xattr_list_has(list: &[u8], name: &str) -> bool {
    list.split(|&b| b == 0)
        .filter(|n| !n.is_empty())
        .any(|n| n == name.as_bytes())
}

/// LISTXATTR must include a name stored by SETXATTR.
pub fn listxattr_self_test() -> bool {
    const MP: &str = "/mnt/fuse-lx";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE listxattr FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "lx.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE listxattr FAILED: put");
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "lx.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE listxattr FAILED: lookup ({})", e);
            return false;
        }
    };
    let before = match fuse_listxattr(MP, looked.nodeid) {
        Ok(data) => data,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE listxattr FAILED: list before ({})", e);
            return false;
        }
    };
    if xattr_list_has(&before, GATE_FUSE_LISTXATTR_NAME) {
        serial_println!("[fuse] GATE_FUSE listxattr FAILED: name present before set");
        return false;
    }
    if let Err(e) = fuse_setxattr(
        MP,
        looked.nodeid,
        GATE_FUSE_LISTXATTR_NAME,
        GATE_FUSE_LISTXATTR_PAYLOAD,
    ) {
        serial_println!("[fuse] GATE_FUSE listxattr FAILED: setxattr ({})", e);
        return false;
    }
    match fuse_listxattr(MP, looked.nodeid) {
        Ok(data) if xattr_list_has(&data, GATE_FUSE_LISTXATTR_NAME) => {
            serial_println!(
                "[fuse] {} (LISTXATTR contains {} ino={})",
                GATE_FUSE_LISTXATTR_MARKER,
                GATE_FUSE_LISTXATTR_NAME,
                looked.nodeid
            );
            true
        }
        other => {
            serial_println!(
                "[fuse] GATE_FUSE listxattr FAILED: list len={}",
                other.map(|d| d.len()).unwrap_or(0)
            );
            false
        }
    }
}

pub const GATE_FUSE_REMOVEXATTR_MARKER: &str = "GATE_FUSE removexattr";
const GATE_FUSE_REMOVEXATTR_NAME: &str = "user.knox";
const GATE_FUSE_REMOVEXATTR_PAYLOAD: &[u8] = b"knoxos-fuse-rx\n";

/// REMOVEXATTR must drop a stored name so a later GETXATTR fails.
pub fn removexattr_self_test() -> bool {
    const MP: &str = "/mnt/fuse-rx";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE removexattr FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "rx.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE removexattr FAILED: put");
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "rx.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE removexattr FAILED: lookup ({})", e);
            return false;
        }
    };
    if let Err(e) = fuse_setxattr(
        MP,
        looked.nodeid,
        GATE_FUSE_REMOVEXATTR_NAME,
        GATE_FUSE_REMOVEXATTR_PAYLOAD,
    ) {
        serial_println!("[fuse] GATE_FUSE removexattr FAILED: setxattr ({})", e);
        return false;
    }
    if fuse_getxattr(MP, looked.nodeid, GATE_FUSE_REMOVEXATTR_NAME).is_err() {
        serial_println!("[fuse] GATE_FUSE removexattr FAILED: get before remove");
        return false;
    }
    if let Err(e) = fuse_removexattr(MP, looked.nodeid, GATE_FUSE_REMOVEXATTR_NAME) {
        serial_println!("[fuse] GATE_FUSE removexattr FAILED: removexattr ({})", e);
        return false;
    }
    if fuse_getxattr(MP, looked.nodeid, GATE_FUSE_REMOVEXATTR_NAME).is_ok() {
        serial_println!("[fuse] GATE_FUSE removexattr FAILED: value still present");
        return false;
    }
    match fuse_listxattr(MP, looked.nodeid) {
        Ok(data) if !xattr_list_has(&data, GATE_FUSE_REMOVEXATTR_NAME) => {
            serial_println!(
                "[fuse] {} (REMOVEXATTR dropped {} ino={})",
                GATE_FUSE_REMOVEXATTR_MARKER,
                GATE_FUSE_REMOVEXATTR_NAME,
                looked.nodeid
            );
            true
        }
        other => {
            serial_println!(
                "[fuse] GATE_FUSE removexattr FAILED: list after len={}",
                other.map(|d| d.len()).unwrap_or(0)
            );
            false
        }
    }
}

pub const GATE_FUSE_ACCESS_MARKER: &str = "GATE_FUSE access";

/// ACCESS must grant file bits from mode, not an echo of the request.
pub fn access_self_test() -> bool {
    const MP: &str = "/mnt/fuse-ac";
    let requested = FUSE_R_OK | FUSE_W_OK | FUSE_X_OK;
    let file_bits = FUSE_R_OK | FUSE_W_OK;
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE access FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "ac.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE access FAILED: put");
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "ac.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE access FAILED: lookup ({})", e);
            return false;
        }
    };
    let granted = match fuse_access(MP, looked.nodeid, requested) {
        Ok(g) => g,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE access FAILED: access ({})", e);
            return false;
        }
    };
    if granted != file_bits || granted & FUSE_X_OK != 0 {
        serial_println!(
            "[fuse] GATE_FUSE access FAILED: granted={:#x} want={:#x}",
            granted,
            file_bits
        );
        return false;
    }
    if fuse_access(MP, 0xFFFF_FFFF, requested).is_ok() {
        serial_println!("[fuse] GATE_FUSE access FAILED: missing nodeid succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (ACCESS granted={:#x} ino={})",
        GATE_FUSE_ACCESS_MARKER,
        granted,
        looked.nodeid
    );
    true
}

pub const GATE_FUSE_FSYNC_MARKER: &str = "GATE_FUSE fsync";
const GATE_FUSE_FSYNC_PAYLOAD: &[u8] = b"knoxos-fuse-fs\n";

/// FSYNC after WRITE must succeed; unknown nodeid must fail.
pub fn fsync_self_test() -> bool {
    const MP: &str = "/mnt/fuse-fs";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE fsync FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "fs.txt", b"old").is_err() {
        serial_println!("[fuse] GATE_FUSE fsync FAILED: put");
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "fs.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE fsync FAILED: lookup ({})", e);
            return false;
        }
    };
    let written = match fuse_write(MP, looked.nodeid, 0, GATE_FUSE_FSYNC_PAYLOAD) {
        Ok(n) => n,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE fsync FAILED: write ({})", e);
            return false;
        }
    };
    if let Err(e) = fuse_fsync(MP, looked.nodeid) {
        serial_println!("[fuse] GATE_FUSE fsync FAILED: fsync ({})", e);
        return false;
    }
    if fuse_fsync(MP, 0xFFFF_FFFF).is_ok() {
        serial_println!("[fuse] GATE_FUSE fsync FAILED: missing nodeid succeeded");
        return false;
    }
    match fuse_read(MP, looked.nodeid, 0, 64) {
        Ok(data) if data == GATE_FUSE_FSYNC_PAYLOAD && written == GATE_FUSE_FSYNC_PAYLOAD.len() => {
            serial_println!(
                "[fuse] {} (WRITE+FSYNC+READ {} bytes ino={})",
                GATE_FUSE_FSYNC_MARKER,
                data.len(),
                looked.nodeid
            );
            true
        }
        other => {
            serial_println!(
                "[fuse] GATE_FUSE fsync FAILED written={} read_len={}",
                written,
                other.map(|d| d.len()).unwrap_or(0)
            );
            false
        }
    }
}

pub const GATE_FUSE_OPEN_MARKER: &str = "GATE_FUSE open";

/// OPEN of a known inode returns fh=nodeid; unknown nodeid fails.
pub fn open_self_test() -> bool {
    const MP: &str = "/mnt/fuse-op";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE open FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "op.txt", b"open").is_err() {
        serial_println!("[fuse] GATE_FUSE open FAILED: put");
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "op.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE open FAILED: lookup ({})", e);
            return false;
        }
    };
    let (fh, flags) = match fuse_open(MP, looked.nodeid) {
        Ok(pair) => pair,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE open FAILED: open ({})", e);
            return false;
        }
    };
    if fh != looked.nodeid || flags != 0 {
        serial_println!(
            "[fuse] GATE_FUSE open FAILED: fh={} flags={} want fh={}",
            fh,
            flags,
            looked.nodeid
        );
        return false;
    }
    if fuse_open(MP, 0xFFFF_FFFF).is_ok() {
        serial_println!("[fuse] GATE_FUSE open FAILED: missing nodeid succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (OPEN fh={} flags={} ino={})",
        GATE_FUSE_OPEN_MARKER,
        fh,
        flags,
        looked.nodeid
    );
    true
}

pub const GATE_FUSE_RELEASE_MARKER: &str = "GATE_FUSE release";

/// RELEASE after OPEN must succeed; unknown nodeid or mismatched fh must fail.
pub fn release_self_test() -> bool {
    const MP: &str = "/mnt/fuse-rel";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE release FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "rel.txt", b"rel").is_err() {
        serial_println!("[fuse] GATE_FUSE release FAILED: put");
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "rel.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE release FAILED: lookup ({})", e);
            return false;
        }
    };
    let (fh, _) = match fuse_open(MP, looked.nodeid) {
        Ok(pair) => pair,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE release FAILED: open ({})", e);
            return false;
        }
    };
    if let Err(e) = fuse_release(MP, looked.nodeid, fh) {
        serial_println!("[fuse] GATE_FUSE release FAILED: release ({})", e);
        return false;
    }
    if fuse_release(MP, 0xFFFF_FFFF, fh).is_ok() {
        serial_println!("[fuse] GATE_FUSE release FAILED: missing nodeid succeeded");
        return false;
    }
    if fuse_release(MP, looked.nodeid, fh.wrapping_add(1)).is_ok() {
        serial_println!("[fuse] GATE_FUSE release FAILED: mismatched fh succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (OPEN+RELEASE fh={} ino={})",
        GATE_FUSE_RELEASE_MARKER,
        fh,
        looked.nodeid
    );
    true
}

pub const GATE_FUSE_FLUSH_MARKER: &str = "GATE_FUSE flush";
const GATE_FUSE_FLUSH_PAYLOAD: &[u8] = b"knoxos-fuse-fl\n";

/// FLUSH after WRITE must succeed; unknown nodeid must fail.
pub fn flush_self_test() -> bool {
    const MP: &str = "/mnt/fuse-fl";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE flush FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "fl.txt", b"old").is_err() {
        serial_println!("[fuse] GATE_FUSE flush FAILED: put");
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "fl.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE flush FAILED: lookup ({})", e);
            return false;
        }
    };
    let written = match fuse_write(MP, looked.nodeid, 0, GATE_FUSE_FLUSH_PAYLOAD) {
        Ok(n) => n,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE flush FAILED: write ({})", e);
            return false;
        }
    };
    if let Err(e) = fuse_flush(MP, looked.nodeid) {
        serial_println!("[fuse] GATE_FUSE flush FAILED: flush ({})", e);
        return false;
    }
    if fuse_flush(MP, 0xFFFF_FFFF).is_ok() {
        serial_println!("[fuse] GATE_FUSE flush FAILED: missing nodeid succeeded");
        return false;
    }
    match fuse_read(MP, looked.nodeid, 0, 64) {
        Ok(data) if data == GATE_FUSE_FLUSH_PAYLOAD && written == GATE_FUSE_FLUSH_PAYLOAD.len() => {
            serial_println!(
                "[fuse] {} (WRITE+FLUSH+READ {} bytes ino={})",
                GATE_FUSE_FLUSH_MARKER,
                data.len(),
                looked.nodeid
            );
            true
        }
        other => {
            serial_println!(
                "[fuse] GATE_FUSE flush FAILED written={} read_len={}",
                written,
                other.map(|d| d.len()).unwrap_or(0)
            );
            false
        }
    }
}

pub const GATE_FUSE_FSYNCDIR_MARKER: &str = "GATE_FUSE fsyncdir";

/// FSYNCDIR of a directory must succeed; a file or unknown nodeid must fail.
pub fn fsyncdir_self_test() -> bool {
    const MP: &str = "/mnt/fuse-fd";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE fsyncdir FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "fd.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE fsyncdir FAILED: put");
        return false;
    }
    let file = match fuse_lookup(MP, 1, "fd.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE fsyncdir FAILED: lookup ({})", e);
            return false;
        }
    };
    let dir = match fuse_mkdir(MP, 1, "fd.dir") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE fsyncdir FAILED: mkdir ({})", e);
            return false;
        }
    };
    if let Err(e) = fuse_fsyncdir(MP, dir.nodeid) {
        serial_println!("[fuse] GATE_FUSE fsyncdir FAILED: fsyncdir ({})", e);
        return false;
    }
    if fuse_fsyncdir(MP, file.nodeid).is_ok() {
        serial_println!("[fuse] GATE_FUSE fsyncdir FAILED: file nodeid succeeded");
        return false;
    }
    if fuse_fsyncdir(MP, 0xFFFF_FFFF).is_ok() {
        serial_println!("[fuse] GATE_FUSE fsyncdir FAILED: missing nodeid succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (FSYNCDIR dir={} file rejected)",
        GATE_FUSE_FSYNCDIR_MARKER,
        dir.nodeid
    );
    true
}

pub const GATE_FUSE_OPENDIR_MARKER: &str = "GATE_FUSE opendir";

/// OPENDIR of a directory returns fh=nodeid; a file or unknown nodeid fails.
pub fn opendir_self_test() -> bool {
    const MP: &str = "/mnt/fuse-od";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE opendir FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "od.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE opendir FAILED: put");
        return false;
    }
    let file = match fuse_lookup(MP, 1, "od.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE opendir FAILED: lookup ({})", e);
            return false;
        }
    };
    let dir = match fuse_mkdir(MP, 1, "od.dir") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE opendir FAILED: mkdir ({})", e);
            return false;
        }
    };
    let (fh, flags) = match fuse_opendir(MP, dir.nodeid) {
        Ok(pair) => pair,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE opendir FAILED: opendir ({})", e);
            return false;
        }
    };
    if fh != dir.nodeid || flags != 0 {
        serial_println!(
            "[fuse] GATE_FUSE opendir FAILED: fh={} flags={} want fh={}",
            fh,
            flags,
            dir.nodeid
        );
        return false;
    }
    if fuse_opendir(MP, file.nodeid).is_ok() {
        serial_println!("[fuse] GATE_FUSE opendir FAILED: file nodeid succeeded");
        return false;
    }
    if fuse_opendir(MP, 0xFFFF_FFFF).is_ok() {
        serial_println!("[fuse] GATE_FUSE opendir FAILED: missing nodeid succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (OPENDIR fh={} dir={} file rejected)",
        GATE_FUSE_OPENDIR_MARKER,
        fh,
        dir.nodeid
    );
    true
}

pub const GATE_FUSE_RELEASEDIR_MARKER: &str = "GATE_FUSE releasedir";

/// RELEASEDIR after OPENDIR must succeed; a file, unknown nodeid, or mismatched fh must fail.
pub fn releasedir_self_test() -> bool {
    const MP: &str = "/mnt/fuse-rdl";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE releasedir FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "rdl.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE releasedir FAILED: put");
        return false;
    }
    let file = match fuse_lookup(MP, 1, "rdl.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE releasedir FAILED: lookup ({})", e);
            return false;
        }
    };
    let dir = match fuse_mkdir(MP, 1, "rdl.dir") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE releasedir FAILED: mkdir ({})", e);
            return false;
        }
    };
    let (fh, _) = match fuse_opendir(MP, dir.nodeid) {
        Ok(pair) => pair,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE releasedir FAILED: opendir ({})", e);
            return false;
        }
    };
    if let Err(e) = fuse_releasedir(MP, dir.nodeid, fh) {
        serial_println!("[fuse] GATE_FUSE releasedir FAILED: releasedir ({})", e);
        return false;
    }
    if fuse_releasedir(MP, file.nodeid, file.nodeid).is_ok() {
        serial_println!("[fuse] GATE_FUSE releasedir FAILED: file nodeid succeeded");
        return false;
    }
    if fuse_releasedir(MP, 0xFFFF_FFFF, fh).is_ok() {
        serial_println!("[fuse] GATE_FUSE releasedir FAILED: missing nodeid succeeded");
        return false;
    }
    if fuse_releasedir(MP, dir.nodeid, fh.wrapping_add(1)).is_ok() {
        serial_println!("[fuse] GATE_FUSE releasedir FAILED: mismatched fh succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (OPENDIR+RELEASEDIR fh={} dir={})",
        GATE_FUSE_RELEASEDIR_MARKER,
        fh,
        dir.nodeid
    );
    true
}

pub const GATE_FUSE_GETLK_MARKER: &str = "GATE_FUSE getlk";

/// GETLK of a known file with no lock is F_UNLCK; unknown nodeid fails.
pub fn getlk_self_test() -> bool {
    const MP: &str = "/mnt/fuse-glk";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE getlk FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "glk.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE getlk FAILED: put");
        return false;
    }
    let file = match fuse_lookup(MP, 1, "glk.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE getlk FAILED: lookup ({})", e);
            return false;
        }
    };
    let want = FuseFileLock {
        start: 0,
        end: 64,
        typ: F_WRLCK,
        pid: 2,
    };
    let got = match fuse_getlk(MP, file.nodeid, want) {
        Ok(lk) => lk,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE getlk FAILED: getlk ({})", e);
            return false;
        }
    };
    if got.typ != F_UNLCK {
        serial_println!("[fuse] GATE_FUSE getlk FAILED: typ={} want UNLCK", got.typ);
        return false;
    }
    if fuse_getlk(MP, 0xFFFF_FFFF, want).is_ok() {
        serial_println!("[fuse] GATE_FUSE getlk FAILED: missing nodeid succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (GETLK UNLCK ino={})",
        GATE_FUSE_GETLK_MARKER,
        file.nodeid
    );
    true
}

pub const GATE_FUSE_SETLK_MARKER: &str = "GATE_FUSE setlk";

/// SETLK F_WRLCK is visible to GETLK from another pid; unknown nodeid fails; F_UNLCK drops it.
pub fn setlk_self_test() -> bool {
    const MP: &str = "/mnt/fuse-slk";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE setlk FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "slk.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE setlk FAILED: put");
        return false;
    }
    let file = match fuse_lookup(MP, 1, "slk.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE setlk FAILED: lookup ({})", e);
            return false;
        }
    };
    let held = FuseFileLock {
        start: 0,
        end: 64,
        typ: F_WRLCK,
        pid: 1,
    };
    if let Err(e) = fuse_setlk(MP, file.nodeid, held) {
        serial_println!("[fuse] GATE_FUSE setlk FAILED: setlk ({})", e);
        return false;
    }
    let probe = FuseFileLock {
        start: 0,
        end: 64,
        typ: F_WRLCK,
        pid: 2,
    };
    let got = match fuse_getlk(MP, file.nodeid, probe) {
        Ok(lk) => lk,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE setlk FAILED: getlk ({})", e);
            return false;
        }
    };
    if got.typ != F_WRLCK || got.pid != 1 {
        serial_println!(
            "[fuse] GATE_FUSE setlk FAILED: getlk typ={} pid={}",
            got.typ,
            got.pid
        );
        return false;
    }
    if fuse_setlk(MP, 0xFFFF_FFFF, held).is_ok() {
        serial_println!("[fuse] GATE_FUSE setlk FAILED: missing nodeid succeeded");
        return false;
    }
    let unlock = FuseFileLock {
        start: 0,
        end: 64,
        typ: F_UNLCK,
        pid: 1,
    };
    if let Err(e) = fuse_setlk(MP, file.nodeid, unlock) {
        serial_println!("[fuse] GATE_FUSE setlk FAILED: unlock ({})", e);
        return false;
    }
    let after = match fuse_getlk(MP, file.nodeid, probe) {
        Ok(lk) => lk,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE setlk FAILED: getlk after unlock ({})", e);
            return false;
        }
    };
    if after.typ != F_UNLCK {
        serial_println!(
            "[fuse] GATE_FUSE setlk FAILED: still locked typ={}",
            after.typ
        );
        return false;
    }
    serial_println!(
        "[fuse] {} (SETLK WRLCK pid=1 then UNLCK ino={})",
        GATE_FUSE_SETLK_MARKER,
        file.nodeid
    );
    true
}

pub const GATE_FUSE_SETLKW_MARKER: &str = "GATE_FUSE setlkw";

/// SETLKW F_WRLCK is visible to GETLK from another pid; unknown nodeid fails; F_UNLCK drops it.
pub fn setlkw_self_test() -> bool {
    const MP: &str = "/mnt/fuse-slkw";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE setlkw FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "slkw.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE setlkw FAILED: put");
        return false;
    }
    let file = match fuse_lookup(MP, 1, "slkw.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE setlkw FAILED: lookup ({})", e);
            return false;
        }
    };
    let held = FuseFileLock {
        start: 0,
        end: 64,
        typ: F_WRLCK,
        pid: 1,
    };
    if let Err(e) = fuse_setlkw(MP, file.nodeid, held) {
        serial_println!("[fuse] GATE_FUSE setlkw FAILED: setlkw ({})", e);
        return false;
    }
    let probe = FuseFileLock {
        start: 0,
        end: 64,
        typ: F_WRLCK,
        pid: 2,
    };
    let got = match fuse_getlk(MP, file.nodeid, probe) {
        Ok(lk) => lk,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE setlkw FAILED: getlk ({})", e);
            return false;
        }
    };
    if got.typ != F_WRLCK || got.pid != 1 {
        serial_println!(
            "[fuse] GATE_FUSE setlkw FAILED: getlk typ={} pid={}",
            got.typ,
            got.pid
        );
        return false;
    }
    if fuse_setlkw(MP, 0xFFFF_FFFF, held).is_ok() {
        serial_println!("[fuse] GATE_FUSE setlkw FAILED: missing nodeid succeeded");
        return false;
    }
    let unlock = FuseFileLock {
        start: 0,
        end: 64,
        typ: F_UNLCK,
        pid: 1,
    };
    if let Err(e) = fuse_setlkw(MP, file.nodeid, unlock) {
        serial_println!("[fuse] GATE_FUSE setlkw FAILED: unlock ({})", e);
        return false;
    }
    let after = match fuse_getlk(MP, file.nodeid, probe) {
        Ok(lk) => lk,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE setlkw FAILED: getlk after unlock ({})", e);
            return false;
        }
    };
    if after.typ != F_UNLCK {
        serial_println!(
            "[fuse] GATE_FUSE setlkw FAILED: still locked typ={}",
            after.typ
        );
        return false;
    }
    serial_println!(
        "[fuse] {} (SETLKW WRLCK pid=1 then UNLCK ino={})",
        GATE_FUSE_SETLKW_MARKER,
        file.nodeid
    );
    true
}

pub const GATE_FUSE_BMAP_MARKER: &str = "GATE_FUSE bmap";

/// BMAP of a known file maps logical block 0 to nodeid; unknown nodeid fails.
pub fn bmap_self_test() -> bool {
    const MP: &str = "/mnt/fuse-bmap";
    const BSIZE: u32 = 4096;
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE bmap FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "bmap.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE bmap FAILED: put");
        return false;
    }
    let file = match fuse_lookup(MP, 1, "bmap.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE bmap FAILED: lookup ({})", e);
            return false;
        }
    };
    let got = match fuse_bmap(MP, file.nodeid, 0, BSIZE) {
        Ok(blk) => blk,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE bmap FAILED: bmap ({})", e);
            return false;
        }
    };
    if got != file.nodeid {
        serial_println!(
            "[fuse] GATE_FUSE bmap FAILED: block={} want {}",
            got,
            file.nodeid
        );
        return false;
    }
    if fuse_bmap(MP, 0xFFFF_FFFF, 0, BSIZE).is_ok() {
        serial_println!("[fuse] GATE_FUSE bmap FAILED: missing nodeid succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (BMAP block={} ino={})",
        GATE_FUSE_BMAP_MARKER,
        got,
        file.nodeid
    );
    true
}

pub const GATE_FUSE_DESTROY_MARKER: &str = "GATE_FUSE destroy";

/// DESTROY of a known mount succeeds; unknown mount fails.
pub fn destroy_self_test() -> bool {
    const MP: &str = "/mnt/fuse-destroy";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE destroy FAILED: mount ({})", e);
            return false;
        }
    }
    if let Err(e) = fuse_destroy(MP) {
        serial_println!("[fuse] GATE_FUSE destroy FAILED: destroy ({})", e);
        return false;
    }
    if fuse_destroy("/mnt/fuse-missing-destroy").is_ok() {
        serial_println!("[fuse] GATE_FUSE destroy FAILED: missing mount succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (DESTROY then missing mount)",
        GATE_FUSE_DESTROY_MARKER
    );
    true
}

pub const GATE_FUSE_IOCTL_MARKER: &str = "GATE_FUSE ioctl";

/// IOCTL of a known file echoes `cmd`; unknown nodeid fails.
pub fn ioctl_self_test() -> bool {
    const MP: &str = "/mnt/fuse-ioctl";
    const CMD: u32 = 0x5401;
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE ioctl FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "ioctl.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE ioctl FAILED: put");
        return false;
    }
    let file = match fuse_lookup(MP, 1, "ioctl.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE ioctl FAILED: lookup ({})", e);
            return false;
        }
    };
    let got = match fuse_ioctl(MP, file.nodeid, CMD) {
        Ok(r) => r,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE ioctl FAILED: ioctl ({})", e);
            return false;
        }
    };
    if got != CMD as i32 {
        serial_println!("[fuse] GATE_FUSE ioctl FAILED: result={} want {}", got, CMD);
        return false;
    }
    if fuse_ioctl(MP, 0xFFFF_FFFF, CMD).is_ok() {
        serial_println!("[fuse] GATE_FUSE ioctl FAILED: missing nodeid succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (IOCTL cmd={:#06x} ino={})",
        GATE_FUSE_IOCTL_MARKER,
        CMD,
        file.nodeid
    );
    true
}

pub const GATE_FUSE_POLL_MARKER: &str = "GATE_FUSE poll";

/// POLL of a known file with data is POLLIN; unknown nodeid fails.
pub fn poll_self_test() -> bool {
    const MP: &str = "/mnt/fuse-poll";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE poll FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "poll.txt", b"knoxos-fuse-poll\n").is_err() {
        serial_println!("[fuse] GATE_FUSE poll FAILED: put");
        return false;
    }
    let file = match fuse_lookup(MP, 1, "poll.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE poll FAILED: lookup ({})", e);
            return false;
        }
    };
    let revents = match fuse_poll(MP, file.nodeid) {
        Ok(r) => r,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE poll FAILED: poll ({})", e);
            return false;
        }
    };
    if revents & POLLIN == 0 {
        serial_println!(
            "[fuse] GATE_FUSE poll FAILED: revents={:#x} missing POLLIN",
            revents
        );
        return false;
    }
    if fuse_poll(MP, 0xFFFF_FFFF).is_ok() {
        serial_println!("[fuse] GATE_FUSE poll FAILED: missing nodeid succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (POLL revents={:#x} ino={})",
        GATE_FUSE_POLL_MARKER,
        revents,
        file.nodeid
    );
    true
}

pub const GATE_FUSE_FALLOCATE_MARKER: &str = "GATE_FUSE fallocate";

/// FALLOCATE of a known file extends size; unknown nodeid fails.
pub fn fallocate_self_test() -> bool {
    const MP: &str = "/mnt/fuse-fallocate";
    const WANT: u64 = 32;
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE fallocate FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "fall.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE fallocate FAILED: put");
        return false;
    }
    let file = match fuse_lookup(MP, 1, "fall.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE fallocate FAILED: lookup ({})", e);
            return false;
        }
    };
    if let Err(e) = fuse_fallocate(MP, file.nodeid, 0, WANT, 0) {
        serial_println!("[fuse] GATE_FUSE fallocate FAILED: fallocate ({})", e);
        return false;
    }
    let attr = match fuse_getattr(MP, file.nodeid) {
        Ok(a) => a,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE fallocate FAILED: getattr ({})", e);
            return false;
        }
    };
    if attr.size != WANT {
        serial_println!(
            "[fuse] GATE_FUSE fallocate FAILED: size={} want {}",
            attr.size,
            WANT
        );
        return false;
    }
    if fuse_fallocate(MP, 0xFFFF_FFFF, 0, WANT, 0).is_ok() {
        serial_println!("[fuse] GATE_FUSE fallocate FAILED: missing nodeid succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (FALLOCATE size={} ino={})",
        GATE_FUSE_FALLOCATE_MARKER,
        attr.size,
        file.nodeid
    );
    true
}

pub const GATE_FUSE_READDIRPLUS_MARKER: &str = "GATE_FUSE readdirplus";

/// READDIRPLUS of the root lists created names with size; unknown nodeid fails.
pub fn readdirplus_self_test() -> bool {
    const MP: &str = "/mnt/fuse-readdirplus";
    const PAYLOAD: &[u8] = b"knoxos-fuse-plus\n";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE readdirplus FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "plus.txt", PAYLOAD).is_err() {
        serial_println!("[fuse] GATE_FUSE readdirplus FAILED: put");
        return false;
    }
    let file = match fuse_lookup(MP, 1, "plus.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE readdirplus FAILED: lookup ({})", e);
            return false;
        }
    };
    let entries = match fuse_readdirplus(MP, 1) {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE readdirplus FAILED: readdirplus ({})", e);
            return false;
        }
    };
    let found = entries.iter().any(|(ino, size, mode, name)| {
        *ino == file.nodeid
            && *size == PAYLOAD.len() as u64
            && *mode == 0o100644
            && name == "plus.txt"
    });
    if !found {
        serial_println!("[fuse] GATE_FUSE readdirplus FAILED: missing plus.txt");
        return false;
    }
    if fuse_readdirplus(MP, 0xFFFF_FFFF).is_ok() {
        serial_println!("[fuse] GATE_FUSE readdirplus FAILED: missing nodeid succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (READDIRPLUS ino={} size={})",
        GATE_FUSE_READDIRPLUS_MARKER,
        file.nodeid,
        PAYLOAD.len()
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

pub const GATE_FUSE_LSEEK_MARKER: &str = "GATE_FUSE lseek";

/// LSEEK SEEK_SET 0 is 0; SEEK_END is size; unknown nodeid fails.
pub fn lseek_self_test() -> bool {
    const MP: &str = "/mnt/fuse-lseek";
    const PAYLOAD: &[u8] = b"knoxos-fuse-ls\n";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE lseek FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "lseek.txt", PAYLOAD).is_err() {
        serial_println!("[fuse] GATE_FUSE lseek FAILED: put");
        return false;
    }
    let file = match fuse_lookup(MP, 1, "lseek.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE lseek FAILED: lookup ({})", e);
            return false;
        }
    };
    let set0 = match fuse_lseek(MP, file.nodeid, 0, SEEK_SET) {
        Ok(off) => off,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE lseek FAILED: SEEK_SET ({})", e);
            return false;
        }
    };
    let end = match fuse_lseek(MP, file.nodeid, 0, SEEK_END) {
        Ok(off) => off,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE lseek FAILED: SEEK_END ({})", e);
            return false;
        }
    };
    if set0 != 0 || end != PAYLOAD.len() as u64 {
        serial_println!(
            "[fuse] GATE_FUSE lseek FAILED: set0={} end={} size={}",
            set0,
            end,
            PAYLOAD.len()
        );
        return false;
    }
    if fuse_lseek(MP, 0xFFFF_FFFF, 0, SEEK_SET).is_ok() {
        serial_println!("[fuse] GATE_FUSE lseek FAILED: missing nodeid succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (LSEEK SEEK_SET 0 then SEEK_END {})",
        GATE_FUSE_LSEEK_MARKER,
        end
    );
    true
}

pub const GATE_FUSE_COPY_FILE_RANGE_MARKER: &str = "GATE_FUSE copy";

/// COPY_FILE_RANGE copies bytes between known files; unknown nodeid fails.
pub fn copy_file_range_self_test() -> bool {
    const MP: &str = "/mnt/fuse-copy";
    const PAYLOAD: &[u8] = b"knoxos-fuse-cf\n";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE copy FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "src-cf.txt", PAYLOAD).is_err() {
        serial_println!("[fuse] GATE_FUSE copy FAILED: put src");
        return false;
    }
    if loopback_put(MP, "dst-cf.txt", b"").is_err() {
        serial_println!("[fuse] GATE_FUSE copy FAILED: put dst");
        return false;
    }
    let src = match fuse_lookup(MP, 1, "src-cf.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE copy FAILED: lookup src ({})", e);
            return false;
        }
    };
    let dst = match fuse_lookup(MP, 1, "dst-cf.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE copy FAILED: lookup dst ({})", e);
            return false;
        }
    };
    let copied = match fuse_copy_file_range(MP, src.nodeid, 0, dst.nodeid, 0, PAYLOAD.len() as u64)
    {
        Ok(n) => n,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE copy FAILED: copy ({})", e);
            return false;
        }
    };
    let data = match fuse_read(MP, dst.nodeid, 0, PAYLOAD.len() as u32) {
        Ok(d) => d,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE copy FAILED: read ({})", e);
            return false;
        }
    };
    if copied != PAYLOAD.len() as u64 || data != PAYLOAD {
        serial_println!(
            "[fuse] GATE_FUSE copy FAILED: copied={} data={:?}",
            copied,
            data
        );
        return false;
    }
    if fuse_copy_file_range(MP, 0xFFFF_FFFF, 0, dst.nodeid, 0, 1).is_ok() {
        serial_println!("[fuse] GATE_FUSE copy FAILED: missing src succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (COPY_FILE_RANGE {} bytes)",
        GATE_FUSE_COPY_FILE_RANGE_MARKER,
        copied
    );
    true
}

pub const GATE_FUSE_SYNCFS_MARKER: &str = "GATE_FUSE syncfs";

/// SYNCFS of a known mount succeeds; unknown mount fails.
pub fn syncfs_self_test() -> bool {
    const MP: &str = "/mnt/fuse-syncfs";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE syncfs FAILED: mount ({})", e);
            return false;
        }
    }
    if let Err(e) = fuse_syncfs(MP) {
        serial_println!("[fuse] GATE_FUSE syncfs FAILED: syncfs ({})", e);
        return false;
    }
    if fuse_syncfs("/mnt/fuse-missing-syncfs").is_ok() {
        serial_println!("[fuse] GATE_FUSE syncfs FAILED: missing mount succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (SYNCFS then missing mount)",
        GATE_FUSE_SYNCFS_MARKER
    );
    true
}

pub const GATE_FUSE_STATX_MARKER: &str = "GATE_FUSE statx";
const GATE_FUSE_STATX_PAYLOAD: &[u8] = b"knoxos-fuse-sx\n";

/// STATX of a known file returns size and S_IFREG; unknown nodeid fails.
pub fn statx_self_test() -> bool {
    const MP: &str = "/mnt/fuse-statx";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE statx FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "statx.txt", GATE_FUSE_STATX_PAYLOAD).is_err() {
        serial_println!("[fuse] GATE_FUSE statx FAILED: put");
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "statx.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE statx FAILED: lookup ({})", e);
            return false;
        }
    };
    let attr = match fuse_statx(MP, looked.nodeid, STATX_BASIC_STATS) {
        Ok(a) => a,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE statx FAILED: statx ({})", e);
            return false;
        }
    };
    if attr.ino != looked.nodeid
        || attr.size != GATE_FUSE_STATX_PAYLOAD.len() as u64
        || attr.mode & 0o170000 != 0o100000
    {
        serial_println!(
            "[fuse] GATE_FUSE statx FAILED: ino={} size={} mode={:#o}",
            attr.ino,
            attr.size,
            attr.mode
        );
        return false;
    }
    if fuse_statx(MP, 0xFFFF_FFFF, STATX_BASIC_STATS).is_ok() {
        serial_println!("[fuse] GATE_FUSE statx FAILED: missing nodeid succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (STATX size={} S_IFREG ino={})",
        GATE_FUSE_STATX_MARKER,
        attr.size,
        attr.ino
    );
    true
}
