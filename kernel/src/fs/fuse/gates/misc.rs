use crate::serial_println;

use super::super::mount::{loopback_put, mount};
use super::super::ops::{
    fuse_batch_forget, fuse_bmap, fuse_destroy, fuse_forget, fuse_getattr, fuse_init,
    fuse_interrupt, fuse_ioctl, fuse_lookup, fuse_notify_inval_entry, fuse_notify_inval_inode,
    fuse_notify_poll, fuse_notify_reply, fuse_notify_store, fuse_poll, fuse_read, fuse_syncfs,
    fuse_tmpfile,
};
use super::super::protocol::{FUSE_KERNEL_MINOR_VERSION, FUSE_KERNEL_VERSION, POLLIN};

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

pub const GATE_FUSE_INIT_MARKER: &str = "GATE_FUSE init";

/// INIT of a known mount returns kernel major 7; unknown mount or major 0 fails.
pub fn init_self_test() -> bool {
    const MP: &str = "/mnt/fuse-init";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE init FAILED: mount ({})", e);
            return false;
        }
    }
    let (major, minor) = match fuse_init(MP, FUSE_KERNEL_VERSION, FUSE_KERNEL_MINOR_VERSION) {
        Ok(v) => v,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE init FAILED: init ({})", e);
            return false;
        }
    };
    if major != FUSE_KERNEL_VERSION || minor != FUSE_KERNEL_MINOR_VERSION {
        serial_println!(
            "[fuse] GATE_FUSE init FAILED: major={} minor={}",
            major,
            minor
        );
        return false;
    }
    if fuse_init(MP, 0, 0).is_ok() {
        serial_println!("[fuse] GATE_FUSE init FAILED: major 0 succeeded");
        return false;
    }
    if fuse_init(
        "/mnt/fuse-missing-init",
        FUSE_KERNEL_VERSION,
        FUSE_KERNEL_MINOR_VERSION,
    )
    .is_ok()
    {
        serial_println!("[fuse] GATE_FUSE init FAILED: missing mount succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (INIT major={} minor={})",
        GATE_FUSE_INIT_MARKER,
        major,
        minor
    );
    true
}

pub const GATE_FUSE_FORGET_MARKER: &str = "GATE_FUSE forget";

/// FORGET of a known nodeid succeeds; unknown nodeid or nlookup 0 fails.
pub fn forget_self_test() -> bool {
    const MP: &str = "/mnt/fuse-forget";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE forget FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "forget.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE forget FAILED: put");
        return false;
    }
    let file = match fuse_lookup(MP, 1, "forget.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE forget FAILED: lookup ({})", e);
            return false;
        }
    };
    if let Err(e) = fuse_forget(MP, file.nodeid, 1) {
        serial_println!("[fuse] GATE_FUSE forget FAILED: forget ({})", e);
        return false;
    }
    if fuse_forget(MP, file.nodeid, 0).is_ok() {
        serial_println!("[fuse] GATE_FUSE forget FAILED: nlookup 0 succeeded");
        return false;
    }
    if fuse_forget(MP, 0xFFFF_FFFF, 1).is_ok() {
        serial_println!("[fuse] GATE_FUSE forget FAILED: missing nodeid succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (FORGET ino={})",
        GATE_FUSE_FORGET_MARKER,
        file.nodeid
    );
    true
}

pub const GATE_FUSE_INTERRUPT_MARKER: &str = "GATE_FUSE interrupt";

/// INTERRUPT of an issued unique succeeds; unique 0 or never-issued fails.
pub fn interrupt_self_test() -> bool {
    const MP: &str = "/mnt/fuse-interrupt";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE interrupt FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "intr.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE interrupt FAILED: put");
        return false;
    }
    if fuse_lookup(MP, 1, "intr.txt").is_err() {
        serial_println!("[fuse] GATE_FUSE interrupt FAILED: lookup");
        return false;
    }
    if let Err(e) = fuse_interrupt(MP, 1) {
        serial_println!("[fuse] GATE_FUSE interrupt FAILED: interrupt ({})", e);
        return false;
    }
    if fuse_interrupt(MP, 0).is_ok() {
        serial_println!("[fuse] GATE_FUSE interrupt FAILED: unique 0 succeeded");
        return false;
    }
    if fuse_interrupt(MP, 0xFFFF_FFFF).is_ok() {
        serial_println!("[fuse] GATE_FUSE interrupt FAILED: never-issued unique succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (INTERRUPT unique=1 then unique 0)",
        GATE_FUSE_INTERRUPT_MARKER
    );
    true
}

pub const GATE_FUSE_BATCHFORGET_MARKER: &str = "GATE_FUSE batchforget";

/// BATCH_FORGET of a known nodeid succeeds; unknown nodeid or nlookup 0 fails.
pub fn batchforget_self_test() -> bool {
    const MP: &str = "/mnt/fuse-batchforget";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE batchforget FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "batch.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE batchforget FAILED: put");
        return false;
    }
    let file = match fuse_lookup(MP, 1, "batch.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE batchforget FAILED: lookup ({})", e);
            return false;
        }
    };
    if let Err(e) = fuse_batch_forget(MP, file.nodeid, 1) {
        serial_println!("[fuse] GATE_FUSE batchforget FAILED: batch_forget ({})", e);
        return false;
    }
    if fuse_batch_forget(MP, file.nodeid, 0).is_ok() {
        serial_println!("[fuse] GATE_FUSE batchforget FAILED: nlookup 0 succeeded");
        return false;
    }
    if fuse_batch_forget(MP, 0xFFFF_FFFF, 1).is_ok() {
        serial_println!("[fuse] GATE_FUSE batchforget FAILED: missing nodeid succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (BATCH_FORGET ino={})",
        GATE_FUSE_BATCHFORGET_MARKER,
        file.nodeid
    );
    true
}

pub const GATE_FUSE_NOTIFYREPLY_MARKER: &str = "GATE_FUSE notifyreply";

/// NOTIFY_REPLY of a known file stores bytes a later READ returns; unknown nodeid or empty fails.
pub fn notifyreply_self_test() -> bool {
    const MP: &str = "/mnt/fuse-notifyreply";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE notifyreply FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "retrieve.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE notifyreply FAILED: put");
        return false;
    }
    let file = match fuse_lookup(MP, 1, "retrieve.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE notifyreply FAILED: lookup ({})", e);
            return false;
        }
    };
    if let Err(e) = fuse_notify_reply(MP, file.nodeid, 0, b"hello") {
        serial_println!("[fuse] GATE_FUSE notifyreply FAILED: notify_reply ({})", e);
        return false;
    }
    match fuse_read(MP, file.nodeid, 0, 5) {
        Ok(data) if data == b"hello" => {}
        other => {
            serial_println!("[fuse] GATE_FUSE notifyreply FAILED: read {:?}", other);
            return false;
        }
    }
    if fuse_notify_reply(MP, file.nodeid, 0, b"").is_ok() {
        serial_println!("[fuse] GATE_FUSE notifyreply FAILED: empty data succeeded");
        return false;
    }
    if fuse_notify_reply(MP, 0xFFFF_FFFF, 0, b"x").is_ok() {
        serial_println!("[fuse] GATE_FUSE notifyreply FAILED: missing nodeid succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (NOTIFY_REPLY ino={})",
        GATE_FUSE_NOTIFYREPLY_MARKER,
        file.nodeid
    );
    true
}

pub const GATE_FUSE_TMPFILE_MARKER: &str = "GATE_FUSE tmpfile";

/// TMPFILE of a directory returns an unnamed S_IFREG inode; a file parent fails.
pub fn tmpfile_self_test() -> bool {
    const MP: &str = "/mnt/fuse-tmpfile";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE tmpfile FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "not-a-dir.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE tmpfile FAILED: put");
        return false;
    }
    let file = match fuse_lookup(MP, 1, "not-a-dir.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE tmpfile FAILED: lookup ({})", e);
            return false;
        }
    };
    let tmp = match fuse_tmpfile(MP, 1) {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE tmpfile FAILED: tmpfile ({})", e);
            return false;
        }
    };
    if tmp.attr.mode & 0o170000 != 0o100000 {
        serial_println!("[fuse] GATE_FUSE tmpfile FAILED: mode={:#o}", tmp.attr.mode);
        return false;
    }
    match fuse_getattr(MP, tmp.nodeid) {
        Ok(attr) if attr.ino == tmp.nodeid && attr.mode & 0o170000 == 0o100000 => {}
        other => {
            serial_println!("[fuse] GATE_FUSE tmpfile FAILED: getattr {:?}", other);
            return false;
        }
    }
    if fuse_lookup(MP, 1, "not-a-dir.txt")
        .map(|e| e.nodeid == tmp.nodeid)
        .unwrap_or(false)
    {
        serial_println!("[fuse] GATE_FUSE tmpfile FAILED: named lookup found tmpfile");
        return false;
    }
    if fuse_tmpfile(MP, file.nodeid).is_ok() {
        serial_println!("[fuse] GATE_FUSE tmpfile FAILED: file parent succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (TMPFILE ino={})",
        GATE_FUSE_TMPFILE_MARKER,
        tmp.nodeid
    );
    true
}

pub const GATE_FUSE_INVALINODE_MARKER: &str = "GATE_FUSE invalinode";

/// NOTIFY_INVAL_INODE of a known nodeid ACKs; GETATTR still works; unknown fails.
pub fn invalinode_self_test() -> bool {
    const MP: &str = "/mnt/fuse-invalinode";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE invalinode FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "inval.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE invalinode FAILED: put");
        return false;
    }
    let file = match fuse_lookup(MP, 1, "inval.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE invalinode FAILED: lookup ({})", e);
            return false;
        }
    };
    if let Err(e) = fuse_notify_inval_inode(MP, file.nodeid) {
        serial_println!("[fuse] GATE_FUSE invalinode FAILED: inval ({})", e);
        return false;
    }
    match fuse_getattr(MP, file.nodeid) {
        Ok(attr) if attr.ino == file.nodeid => {}
        other => {
            serial_println!("[fuse] GATE_FUSE invalinode FAILED: getattr {:?}", other);
            return false;
        }
    }
    if fuse_notify_inval_inode(MP, 0xFFFF_FFFF).is_ok() {
        serial_println!("[fuse] GATE_FUSE invalinode FAILED: missing nodeid succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (NOTIFY_INVAL_INODE ino={})",
        GATE_FUSE_INVALINODE_MARKER,
        file.nodeid
    );
    true
}

pub const GATE_FUSE_NOTIFYSTORE_MARKER: &str = "GATE_FUSE notifystore";

/// NOTIFY_STORE of a known file stores bytes a later READ returns; unknown nodeid or empty fails.
pub fn notifystore_self_test() -> bool {
    const MP: &str = "/mnt/fuse-notifystore";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE notifystore FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "store.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE notifystore FAILED: put");
        return false;
    }
    let file = match fuse_lookup(MP, 1, "store.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE notifystore FAILED: lookup ({})", e);
            return false;
        }
    };
    if let Err(e) = fuse_notify_store(MP, file.nodeid, 0, b"store") {
        serial_println!("[fuse] GATE_FUSE notifystore FAILED: store ({})", e);
        return false;
    }
    match fuse_read(MP, file.nodeid, 0, 5) {
        Ok(data) if data == b"store" => {}
        other => {
            serial_println!("[fuse] GATE_FUSE notifystore FAILED: read {:?}", other);
            return false;
        }
    }
    if fuse_notify_store(MP, file.nodeid, 0, b"").is_ok() {
        serial_println!("[fuse] GATE_FUSE notifystore FAILED: empty data succeeded");
        return false;
    }
    if fuse_notify_store(MP, 0xFFFF_FFFF, 0, b"x").is_ok() {
        serial_println!("[fuse] GATE_FUSE notifystore FAILED: missing nodeid succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (NOTIFY_STORE ino={})",
        GATE_FUSE_NOTIFYSTORE_MARKER,
        file.nodeid
    );
    true
}

pub const GATE_FUSE_NOTIFYPOLL_MARKER: &str = "GATE_FUSE notifypoll";

/// NOTIFY_POLL of a known nodeid ACKs; POLL still works; unknown fails.
pub fn notifypoll_self_test() -> bool {
    const MP: &str = "/mnt/fuse-notifypoll";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE notifypoll FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "poll.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE notifypoll FAILED: put");
        return false;
    }
    let file = match fuse_lookup(MP, 1, "poll.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE notifypoll FAILED: lookup ({})", e);
            return false;
        }
    };
    if let Err(e) = fuse_notify_poll(MP, file.nodeid) {
        serial_println!("[fuse] GATE_FUSE notifypoll FAILED: poll notify ({})", e);
        return false;
    }
    match fuse_poll(MP, file.nodeid) {
        Ok(revents) if revents & POLLIN != 0 => {}
        other => {
            serial_println!("[fuse] GATE_FUSE notifypoll FAILED: poll {:?}", other);
            return false;
        }
    }
    if fuse_notify_poll(MP, 0xFFFF_FFFF).is_ok() {
        serial_println!("[fuse] GATE_FUSE notifypoll FAILED: missing nodeid succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (NOTIFY_POLL ino={})",
        GATE_FUSE_NOTIFYPOLL_MARKER,
        file.nodeid
    );
    true
}

pub const GATE_FUSE_INVALENTRY_MARKER: &str = "GATE_FUSE invalentry";

/// NOTIFY_INVAL_ENTRY of a known parent+name ACKs; LOOKUP still works; unknown or empty fails.
pub fn invalentry_self_test() -> bool {
    const MP: &str = "/mnt/fuse-invalentry";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE invalentry FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "entry.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE invalentry FAILED: put");
        return false;
    }
    if let Err(e) = fuse_notify_inval_entry(MP, 1, "entry.txt") {
        serial_println!("[fuse] GATE_FUSE invalentry FAILED: inval ({})", e);
        return false;
    }
    match fuse_lookup(MP, 1, "entry.txt") {
        Ok(e) if e.attr.size == 1 => {}
        other => {
            serial_println!("[fuse] GATE_FUSE invalentry FAILED: lookup {:?}", other);
            return false;
        }
    }
    if fuse_notify_inval_entry(MP, 1, "").is_ok() {
        serial_println!("[fuse] GATE_FUSE invalentry FAILED: empty name succeeded");
        return false;
    }
    if fuse_notify_inval_entry(MP, 0xFFFF_FFFF, "entry.txt").is_ok() {
        serial_println!("[fuse] GATE_FUSE invalentry FAILED: missing parent succeeded");
        return false;
    }
    serial_println!(
        "[fuse] {} (NOTIFY_INVAL_ENTRY parent=1)",
        GATE_FUSE_INVALENTRY_MARKER
    );
    true
}
