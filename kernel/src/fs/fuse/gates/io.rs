use crate::serial_println;

use super::super::mount::{loopback_put, mount};
use super::super::ops::{fuse_lookup, fuse_read, fuse_write};

pub const GATE_FUSE_MARKER: &str = "GATE_FUSE lookup";
const GATE_FUSE_PAYLOAD: &[u8] = b"knoxos-fuse\n";

pub fn lookup_self_test() -> bool {
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
