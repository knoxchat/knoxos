//! GATE_FUSE serial markers and known-answer self-tests.

use alloc::vec::Vec;

use crate::serial_println;

use super::mount::{loopback_put, mount};
use super::ops::{
    fuse_create, fuse_getattr, fuse_getxattr, fuse_link, fuse_listxattr, fuse_lookup, fuse_mkdir,
    fuse_mknod, fuse_read, fuse_readdir, fuse_readlink, fuse_removexattr, fuse_rename, fuse_rmdir,
    fuse_setattr, fuse_setxattr, fuse_statfs, fuse_symlink, fuse_unlink, fuse_write,
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
