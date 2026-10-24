use crate::serial_println;

use super::super::mount::{loopback_put, mount};
use super::super::ops::{
    fuse_copy_file_range, fuse_fallocate, fuse_flush, fuse_fsync, fuse_getattr, fuse_lookup,
    fuse_lseek, fuse_open, fuse_read, fuse_release, fuse_write,
};
use super::super::protocol::{SEEK_END, SEEK_SET};

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
