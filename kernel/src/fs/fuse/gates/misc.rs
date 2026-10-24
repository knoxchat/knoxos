use crate::serial_println;

use super::super::mount::{loopback_put, mount};
use super::super::ops::{fuse_bmap, fuse_destroy, fuse_ioctl, fuse_lookup, fuse_poll, fuse_syncfs};
use super::super::protocol::POLLIN;

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
