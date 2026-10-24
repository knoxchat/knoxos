use crate::serial_println;

use super::super::mount::{loopback_put, mount};
use super::super::ops::{fuse_getlk, fuse_lookup, fuse_setlk, fuse_setlkw};
use super::super::protocol::{F_UNLCK, F_WRLCK, FuseFileLock};

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
