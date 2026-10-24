use alloc::vec::Vec;

use crate::serial_println;

use super::super::mount::{loopback_put, mount};
use super::super::ops::{
    fuse_create, fuse_fsyncdir, fuse_lookup, fuse_mkdir, fuse_opendir, fuse_readdir,
    fuse_readdirplus, fuse_releasedir, fuse_rmdir,
};

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
