use crate::serial_println;

use super::super::mount::{loopback_put, mount};
use super::super::ops::{
    fuse_access, fuse_getattr, fuse_lookup, fuse_mkdir, fuse_read, fuse_setattr, fuse_statfs,
    fuse_statx,
};
use super::super::protocol::{FUSE_R_OK, FUSE_W_OK, FUSE_X_OK, STATX_BASIC_STATS};

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
