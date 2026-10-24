use crate::serial_println;

use super::super::mount::{loopback_put, mount};
use super::super::ops::{
    fuse_getxattr, fuse_listxattr, fuse_lookup, fuse_removexattr, fuse_setxattr,
};

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
