//! In-kernel FUSE loopback daemon.

use alloc::string::String;
use alloc::vec::Vec;

use super::mount::{LOOPBACK, write_response};
use super::protocol::{
    F_UNLCK, FUSE_KERNEL_MINOR_VERSION, FUSE_KERNEL_VERSION, FuseFileLock, FuseOpcode, FuseStatfs,
    POLLIN, POLLOUT, RENAME_NOREPLACE, SEEK_END, SEEK_SET, encode_bmap, encode_copy_file_range,
    encode_dirent, encode_direntplus, encode_entry, encode_init, encode_ioctl, encode_lk,
    encode_lseek, encode_open, encode_poll, encode_statfs, encode_statx, lock_conflicts,
    parse_fuse_lk,
};

pub(super) fn serve_loopback(mount_point: &str, opcode: FuseOpcode, unique: u64, req: &[u8]) {
    if opcode != FuseOpcode::Lookup {
        return;
    }
    let name = core::str::from_utf8(req).unwrap_or("");
    let (ino, size, mode) = {
        let lbs = LOOPBACK.lock();
        let Some(lb) = lbs.get(mount_point) else {
            return;
        };
        if let Some(&ino) = lb.dirs.get(name) {
            (ino, 0u64, 0o040755)
        } else if let Some(&(ino, ref data)) = lb.files.get(name) {
            (ino, data.len() as u64, 0o100644)
        } else if let Some(&(ino, ref target)) = lb.links.get(name) {
            (ino, target.len() as u64, 0o120777)
        } else if let Some(&(ino, mode)) = lb.nodes.get(name) {
            (ino, 0u64, mode)
        } else {
            return;
        }
    };
    let _ = write_response(mount_point, unique, encode_entry(ino, size, mode));
}

pub(super) fn serve_loopback_read(mount_point: &str, unique: u64, nodeid: u64) {
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    if let Some((_, data)) = lb.files.values().find(|(ino, _)| *ino == nodeid) {
        let payload = data.clone();
        drop(lbs);
        let _ = write_response(mount_point, unique, payload);
    }
}
pub(super) fn serve_loopback_write(mount_point: &str, unique: u64, nodeid: u64, req: &[u8]) {
    if req.len() < 8 {
        return;
    }
    let offset = u64::from_le_bytes(req[0..8].try_into().unwrap_or([0; 8])) as usize;
    let payload = &req[8..];
    let written = {
        let mut lbs = LOOPBACK.lock();
        let Some(lb) = lbs.get_mut(mount_point) else {
            return;
        };
        let Some((_, data)) = lb.files.values_mut().find(|(ino, _)| *ino == nodeid) else {
            return;
        };
        let end = offset.saturating_add(payload.len());
        if end > data.len() {
            data.resize(end, 0);
        }
        if offset < end {
            data[offset..end].copy_from_slice(payload);
        }
        payload.len() as u32
    };
    let mut resp = Vec::with_capacity(8);
    resp.extend_from_slice(&written.to_le_bytes());
    resp.extend_from_slice(&0u32.to_le_bytes());
    let _ = write_response(mount_point, unique, resp);
}
pub(super) fn serve_loopback_unlink(mount_point: &str, unique: u64, name: &str) {
    let mut lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get_mut(mount_point) else {
        return;
    };
    lb.files.remove(name);
    lb.links.remove(name);
    lb.nodes.remove(name);
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}
pub(super) fn serve_loopback_mkdir(mount_point: &str, unique: u64, name: &str) {
    let entry = {
        let mut lbs = LOOPBACK.lock();
        let Some(lb) = lbs.get_mut(mount_point) else {
            return;
        };
        let ino = lb.next_ino;
        lb.next_ino += 1;
        lb.dirs.insert(String::from(name), ino);
        encode_entry(ino, 0, 0o040755)
    };
    let _ = write_response(mount_point, unique, entry);
}
pub(super) fn serve_loopback_rmdir(mount_point: &str, unique: u64, name: &str) {
    let mut lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get_mut(mount_point) else {
        return;
    };
    lb.dirs.remove(name);
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}
pub(super) fn serve_loopback_rename(mount_point: &str, unique: u64, req: &[u8]) {
    let Ok(s) = core::str::from_utf8(req) else {
        return;
    };
    let Some((old, new)) = s.split_once('\0') else {
        return;
    };
    let mut lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get_mut(mount_point) else {
        return;
    };
    if let Some(file) = lb.files.remove(old) {
        lb.files.insert(String::from(new), file);
    } else if let Some(ino) = lb.dirs.remove(old) {
        lb.dirs.insert(String::from(new), ino);
    } else if let Some(link) = lb.links.remove(old) {
        lb.links.insert(String::from(new), link);
    } else if let Some(node) = lb.nodes.remove(old) {
        lb.nodes.insert(String::from(new), node);
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}
pub(super) fn serve_loopback_symlink(mount_point: &str, unique: u64, req: &[u8]) {
    let Ok(s) = core::str::from_utf8(req) else {
        return;
    };
    let Some((name, target)) = s.split_once('\0') else {
        return;
    };
    let entry = {
        let mut lbs = LOOPBACK.lock();
        let Some(lb) = lbs.get_mut(mount_point) else {
            return;
        };
        let ino = lb.next_ino;
        lb.next_ino += 1;
        lb.links
            .insert(String::from(name), (ino, String::from(target)));
        encode_entry(ino, target.len() as u64, 0o120777)
    };
    let _ = write_response(mount_point, unique, entry);
}
pub(super) fn serve_loopback_readlink(mount_point: &str, unique: u64, nodeid: u64) {
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    if let Some((_, target)) = lb.links.values().find(|(ino, _)| *ino == nodeid) {
        let payload = target.as_bytes().to_vec();
        drop(lbs);
        let _ = write_response(mount_point, unique, payload);
    }
}
pub(super) fn serve_loopback_link(mount_point: &str, unique: u64, req: &[u8]) {
    let Ok(s) = core::str::from_utf8(req) else {
        return;
    };
    let Some((old, new)) = s.split_once('\0') else {
        return;
    };
    let mut lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get_mut(mount_point) else {
        return;
    };
    if let Some(file) = lb.files.get(old).cloned() {
        lb.files.insert(String::from(new), file);
    } else if let Some(&ino) = lb.dirs.get(old) {
        lb.dirs.insert(String::from(new), ino);
    } else if let Some(link) = lb.links.get(old).cloned() {
        lb.links.insert(String::from(new), link);
    } else if let Some(&node) = lb.nodes.get(old) {
        lb.nodes.insert(String::from(new), node);
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}
pub(super) fn serve_loopback_create(mount_point: &str, unique: u64, name: &str) {
    let entry = {
        let mut lbs = LOOPBACK.lock();
        let Some(lb) = lbs.get_mut(mount_point) else {
            return;
        };
        let ino = lb.next_ino;
        lb.next_ino += 1;
        lb.files.insert(String::from(name), (ino, Vec::new()));
        encode_entry(ino, 0, 0o100644)
    };
    let _ = write_response(mount_point, unique, entry);
}
pub(super) fn serve_loopback_getattr(mount_point: &str, unique: u64, nodeid: u64) {
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    let entry = if let Some((_, data)) = lb.files.values().find(|(ino, _)| *ino == nodeid) {
        encode_entry(nodeid, data.len() as u64, 0o100644)
    } else if lb.dirs.values().any(|&ino| ino == nodeid) {
        encode_entry(nodeid, 0, 0o040755)
    } else if let Some((_, target)) = lb.links.values().find(|(ino, _)| *ino == nodeid) {
        encode_entry(nodeid, target.len() as u64, 0o120777)
    } else if let Some(&(_, mode)) = lb.nodes.values().find(|(ino, _)| *ino == nodeid) {
        encode_entry(nodeid, 0, mode)
    } else {
        return;
    };
    drop(lbs);
    let _ = write_response(mount_point, unique, entry);
}
pub(super) fn serve_loopback_setattr(mount_point: &str, unique: u64, nodeid: u64, req: &[u8]) {
    if req.len() < 8 {
        return;
    }
    let size = u64::from_le_bytes(req[0..8].try_into().unwrap_or([0; 8]));
    let mut lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get_mut(mount_point) else {
        return;
    };
    let Some((_, data)) = lb.files.values_mut().find(|(ino, _)| *ino == nodeid) else {
        return;
    };
    data.resize(size as usize, 0);
    drop(lbs);
    let _ = write_response(mount_point, unique, encode_entry(nodeid, size, 0o100644));
}
pub(super) fn serve_loopback_readdir(mount_point: &str, unique: u64, nodeid: u64) {
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    let is_root = nodeid == 1;
    let is_dir = lb.dirs.values().any(|&ino| ino == nodeid);
    if !is_root && !is_dir {
        return;
    }
    let mut out = Vec::new();
    if is_root {
        for (name, &(ino, _)) in &lb.files {
            out.extend_from_slice(&encode_dirent(ino, 0o100644, name));
        }
        for (name, &ino) in &lb.dirs {
            out.extend_from_slice(&encode_dirent(ino, 0o040755, name));
        }
        for (name, &(ino, _)) in &lb.links {
            out.extend_from_slice(&encode_dirent(ino, 0o120777, name));
        }
        for (name, &(ino, mode)) in &lb.nodes {
            out.extend_from_slice(&encode_dirent(ino, mode, name));
        }
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, out);
}
pub(super) fn serve_loopback_mknod(mount_point: &str, unique: u64, req: &[u8]) {
    if req.len() < 4 {
        return;
    }
    let mode = u32::from_le_bytes(req[0..4].try_into().unwrap_or([0; 4]));
    let Ok(name) = core::str::from_utf8(&req[4..]) else {
        return;
    };
    let entry = {
        let mut lbs = LOOPBACK.lock();
        let Some(lb) = lbs.get_mut(mount_point) else {
            return;
        };
        let ino = lb.next_ino;
        lb.next_ino += 1;
        lb.nodes.insert(String::from(name), (ino, mode));
        encode_entry(ino, 0, mode)
    };
    let _ = write_response(mount_point, unique, entry);
}
pub(super) fn serve_loopback_statfs(mount_point: &str, unique: u64) {
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    let files = (lb.files.len() + lb.dirs.len() + lb.links.len() + lb.nodes.len()) as u64;
    let st = FuseStatfs {
        bsize: 4096,
        blocks: 1024,
        bfree: 1024,
        files,
        namelen: 255,
    };
    drop(lbs);
    let _ = write_response(mount_point, unique, encode_statfs(&st));
}
fn loopback_has_ino(lb: &super::mount::LoopbackFs, nodeid: u64) -> bool {
    nodeid == 1
        || lb.files.values().any(|(ino, _)| *ino == nodeid)
        || lb.dirs.values().any(|&ino| ino == nodeid)
        || lb.links.values().any(|(ino, _)| *ino == nodeid)
        || lb.nodes.values().any(|(ino, _)| *ino == nodeid)
}
fn loopback_is_dir(lb: &super::mount::LoopbackFs, nodeid: u64) -> bool {
    nodeid == 1 || lb.dirs.values().any(|&ino| ino == nodeid)
}
pub(super) fn serve_loopback_setxattr(mount_point: &str, unique: u64, nodeid: u64, req: &[u8]) {
    let Some(z) = req.iter().position(|&b| b == 0) else {
        return;
    };
    let Ok(name) = core::str::from_utf8(&req[..z]) else {
        return;
    };
    let value = req[z + 1..].to_vec();
    let mut lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get_mut(mount_point) else {
        return;
    };
    if !loopback_has_ino(lb, nodeid) {
        return;
    }
    lb.xattrs
        .entry(nodeid)
        .or_default()
        .insert(String::from(name), value);
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}
pub(super) fn serve_loopback_getxattr(mount_point: &str, unique: u64, nodeid: u64, req: &[u8]) {
    let Ok(name) = core::str::from_utf8(req) else {
        return;
    };
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    let Some(value) = lb.xattrs.get(&nodeid).and_then(|m| m.get(name)).cloned() else {
        return;
    };
    drop(lbs);
    let _ = write_response(mount_point, unique, value);
}
pub(super) fn serve_loopback_listxattr(mount_point: &str, unique: u64, nodeid: u64) {
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    if !loopback_has_ino(lb, nodeid) {
        return;
    }
    let mut out = Vec::new();
    if let Some(attrs) = lb.xattrs.get(&nodeid) {
        for name in attrs.keys() {
            out.extend_from_slice(name.as_bytes());
            out.push(0);
        }
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, out);
}
pub(super) fn serve_loopback_removexattr(mount_point: &str, unique: u64, nodeid: u64, req: &[u8]) {
    let Ok(name) = core::str::from_utf8(req) else {
        return;
    };
    let mut lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get_mut(mount_point) else {
        return;
    };
    if !loopback_has_ino(lb, nodeid) {
        return;
    }
    let removed = lb
        .xattrs
        .get_mut(&nodeid)
        .map(|m| m.remove(name).is_some())
        .unwrap_or(false);
    if !removed {
        return;
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}
pub(super) fn serve_loopback_access(mount_point: &str, unique: u64, nodeid: u64, req: &[u8]) {
    if req.len() < 4 {
        return;
    }
    let requested = u32::from_le_bytes(req[0..4].try_into().unwrap_or([0; 4]));
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    let mode = if nodeid == 1 {
        0o040755
    } else if lb.files.values().any(|(ino, _)| *ino == nodeid) {
        0o100644
    } else if lb.dirs.values().any(|&ino| ino == nodeid) {
        0o040755
    } else if lb.links.values().any(|(ino, _)| *ino == nodeid) {
        0o120777
    } else if let Some(&(_, mode)) = lb.nodes.values().find(|(ino, _)| *ino == nodeid) {
        mode
    } else {
        return;
    };
    drop(lbs);
    let granted = super::protocol::mode_to_access(mode) & requested;
    let _ = write_response(mount_point, unique, granted.to_le_bytes().to_vec());
}
pub(super) fn serve_loopback_fsync(mount_point: &str, unique: u64, nodeid: u64) {
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    if !loopback_has_ino(lb, nodeid) {
        return;
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}
pub(super) fn serve_loopback_open(mount_point: &str, unique: u64, nodeid: u64) {
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    if !loopback_has_ino(lb, nodeid) {
        return;
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, encode_open(nodeid, 0));
}
pub(super) fn serve_loopback_release(mount_point: &str, unique: u64, nodeid: u64, fh: u64) {
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    if !loopback_has_ino(lb, nodeid) || fh != nodeid {
        return;
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}
pub(super) fn serve_loopback_flush(mount_point: &str, unique: u64, nodeid: u64) {
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    if !loopback_has_ino(lb, nodeid) {
        return;
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}
pub(super) fn serve_loopback_fsyncdir(mount_point: &str, unique: u64, nodeid: u64) {
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    if !loopback_is_dir(lb, nodeid) {
        return;
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}
pub(super) fn serve_loopback_opendir(mount_point: &str, unique: u64, nodeid: u64) {
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    if !loopback_is_dir(lb, nodeid) {
        return;
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, encode_open(nodeid, 0));
}
pub(super) fn serve_loopback_releasedir(mount_point: &str, unique: u64, nodeid: u64, fh: u64) {
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    if !loopback_is_dir(lb, nodeid) || fh != nodeid {
        return;
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}
pub(super) fn serve_loopback_getlk(mount_point: &str, unique: u64, nodeid: u64, req: &[u8]) {
    let Some(want) = parse_fuse_lk(req) else {
        return;
    };
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    if !loopback_has_ino(lb, nodeid) {
        return;
    }
    let reply = lb
        .locks
        .get(&nodeid)
        .and_then(|held| held.iter().find(|h| lock_conflicts(h, &want)).copied())
        .unwrap_or(FuseFileLock {
            start: want.start,
            end: want.end,
            typ: F_UNLCK,
            pid: 0,
        });
    drop(lbs);
    let _ = write_response(mount_point, unique, encode_lk(&reply));
}
pub(super) fn serve_loopback_setlk(mount_point: &str, unique: u64, nodeid: u64, req: &[u8]) {
    let Some(want) = parse_fuse_lk(req) else {
        return;
    };
    let mut lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get_mut(mount_point) else {
        return;
    };
    if !loopback_has_ino(lb, nodeid) {
        return;
    }
    let held = lb.locks.entry(nodeid).or_default();
    if want.typ == F_UNLCK {
        held.retain(|h| h.pid != want.pid || !super::protocol::locks_overlap(h, &want));
        drop(lbs);
        let _ = write_response(mount_point, unique, Vec::new());
        return;
    }
    if held.iter().any(|h| lock_conflicts(h, &want)) {
        return;
    }
    held.retain(|h| h.pid != want.pid || !super::protocol::locks_overlap(h, &want));
    held.push(want);
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}
pub(super) fn serve_loopback_bmap(mount_point: &str, unique: u64, nodeid: u64, req: &[u8]) {
    if req.len() < 12 {
        return;
    }
    let logical = u64::from_le_bytes(req[0..8].try_into().unwrap_or([0; 8]));
    let bsize = u32::from_le_bytes(req[8..12].try_into().unwrap_or([0; 4]));
    if bsize == 0 {
        return;
    }
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    if !lb.files.values().any(|(ino, _)| *ino == nodeid) {
        return;
    }
    drop(lbs);
    let _ = write_response(
        mount_point,
        unique,
        encode_bmap(nodeid.saturating_add(logical)),
    );
}
pub(super) fn serve_loopback_destroy(mount_point: &str, unique: u64) {
    let lbs = LOOPBACK.lock();
    if lbs.get(mount_point).is_none() {
        return;
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}
pub(super) fn serve_loopback_ioctl(mount_point: &str, unique: u64, nodeid: u64, req: &[u8]) {
    if req.len() < 16 {
        return;
    }
    let cmd = u32::from_le_bytes(req[12..16].try_into().unwrap_or([0; 4]));
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    if !loopback_has_ino(lb, nodeid) {
        return;
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, encode_ioctl(cmd as i32));
}
pub(super) fn serve_loopback_poll(mount_point: &str, unique: u64, nodeid: u64) {
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    if !loopback_has_ino(lb, nodeid) {
        return;
    }
    let nonempty = lb
        .files
        .values()
        .any(|(ino, data)| *ino == nodeid && !data.is_empty());
    drop(lbs);
    let mut revents = POLLOUT;
    if nonempty || nodeid == 1 {
        revents |= POLLIN;
    }
    let _ = write_response(mount_point, unique, encode_poll(revents));
}
fn loopback_has_name(lb: &super::mount::LoopbackFs, name: &str) -> bool {
    lb.files.contains_key(name)
        || lb.dirs.contains_key(name)
        || lb.links.contains_key(name)
        || lb.nodes.contains_key(name)
}
pub(super) fn serve_loopback_readdirplus(mount_point: &str, unique: u64, nodeid: u64) {
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    let is_root = nodeid == 1;
    let is_dir = lb.dirs.values().any(|&ino| ino == nodeid);
    if !is_root && !is_dir {
        return;
    }
    let mut out = Vec::new();
    if is_root {
        for (name, &(ino, ref data)) in &lb.files {
            out.extend_from_slice(&encode_direntplus(ino, data.len() as u64, 0o100644, name));
        }
        for (name, &ino) in &lb.dirs {
            out.extend_from_slice(&encode_direntplus(ino, 0, 0o040755, name));
        }
        for (name, &(ino, ref target)) in &lb.links {
            out.extend_from_slice(&encode_direntplus(ino, target.len() as u64, 0o120777, name));
        }
        for (name, &(ino, mode)) in &lb.nodes {
            out.extend_from_slice(&encode_direntplus(ino, 0, mode, name));
        }
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, out);
}
pub(super) fn serve_loopback_rename2(mount_point: &str, unique: u64, req: &[u8]) {
    if req.len() < 4 {
        return;
    }
    let flags = u32::from_le_bytes(req[0..4].try_into().unwrap_or([0; 4]));
    let Ok(s) = core::str::from_utf8(&req[4..]) else {
        return;
    };
    let Some((old, new)) = s.split_once('\0') else {
        return;
    };
    let mut lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get_mut(mount_point) else {
        return;
    };
    if !loopback_has_name(lb, old) {
        return;
    }
    if flags & RENAME_NOREPLACE != 0 && loopback_has_name(lb, new) {
        return;
    }
    if let Some(file) = lb.files.remove(old) {
        lb.files.insert(String::from(new), file);
    } else if let Some(ino) = lb.dirs.remove(old) {
        lb.dirs.insert(String::from(new), ino);
    } else if let Some(link) = lb.links.remove(old) {
        lb.links.insert(String::from(new), link);
    } else if let Some(node) = lb.nodes.remove(old) {
        lb.nodes.insert(String::from(new), node);
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}
pub(super) fn serve_loopback_lseek(mount_point: &str, unique: u64, nodeid: u64, req: &[u8]) {
    if req.len() < 12 {
        return;
    }
    let offset = u64::from_le_bytes(req[0..8].try_into().unwrap_or([0; 8]));
    let whence = u32::from_le_bytes(req[8..12].try_into().unwrap_or([0; 4]));
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    let Some((_, data)) = lb.files.values().find(|(ino, _)| *ino == nodeid) else {
        return;
    };
    let size = data.len() as u64;
    let out = match whence {
        SEEK_SET if offset <= size => offset,
        SEEK_END => size,
        _ => return,
    };
    drop(lbs);
    let _ = write_response(mount_point, unique, encode_lseek(out));
}
pub(super) fn serve_loopback_copy_file_range(
    mount_point: &str,
    unique: u64,
    nodeid: u64,
    req: &[u8],
) {
    if req.len() < 32 {
        return;
    }
    let off_in = u64::from_le_bytes(req[0..8].try_into().unwrap_or([0; 8])) as usize;
    let nodeid_out = u64::from_le_bytes(req[8..16].try_into().unwrap_or([0; 8]));
    let off_out = u64::from_le_bytes(req[16..24].try_into().unwrap_or([0; 8])) as usize;
    let len = u64::from_le_bytes(req[24..32].try_into().unwrap_or([0; 8])) as usize;
    let mut lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get_mut(mount_point) else {
        return;
    };
    let Some(src) = lb
        .files
        .values()
        .find(|(ino, _)| *ino == nodeid)
        .map(|(_, data)| data.clone())
    else {
        return;
    };
    if !lb.files.values().any(|(ino, _)| *ino == nodeid_out) {
        return;
    }
    let end_in = off_in.saturating_add(len).min(src.len());
    if off_in >= src.len() {
        drop(lbs);
        let _ = write_response(mount_point, unique, encode_copy_file_range(0));
        return;
    }
    let payload = src[off_in..end_in].to_vec();
    let copied = payload.len() as u64;
    let Some((_, dst)) = lb.files.values_mut().find(|(ino, _)| *ino == nodeid_out) else {
        return;
    };
    let end_out = off_out.saturating_add(payload.len());
    if end_out > dst.len() {
        dst.resize(end_out, 0);
    }
    if off_out < end_out {
        dst[off_out..end_out].copy_from_slice(&payload);
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, encode_copy_file_range(copied));
}
pub(super) fn serve_loopback_syncfs(mount_point: &str, unique: u64) {
    let lbs = LOOPBACK.lock();
    if lbs.get(mount_point).is_none() {
        return;
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}
pub(super) fn serve_loopback_statx(mount_point: &str, unique: u64, nodeid: u64, req: &[u8]) {
    if req.len() < 4 {
        return;
    }
    let mask = u32::from_le_bytes(req[0..4].try_into().unwrap_or([0; 4]));
    if mask == 0 {
        return;
    }
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    let entry = if let Some((_, data)) = lb.files.values().find(|(ino, _)| *ino == nodeid) {
        encode_statx(nodeid, data.len() as u64, 0o100644, mask)
    } else if lb.dirs.values().any(|&ino| ino == nodeid) {
        encode_statx(nodeid, 0, 0o040755, mask)
    } else if let Some((_, target)) = lb.links.values().find(|(ino, _)| *ino == nodeid) {
        encode_statx(nodeid, target.len() as u64, 0o120777, mask)
    } else if let Some(&(_, mode)) = lb.nodes.values().find(|(ino, _)| *ino == nodeid) {
        encode_statx(nodeid, 0, mode, mask)
    } else {
        return;
    };
    drop(lbs);
    let _ = write_response(mount_point, unique, entry);
}
pub(super) fn serve_loopback_init(mount_point: &str, unique: u64, req: &[u8]) {
    if req.len() < 8 {
        return;
    }
    let major = u32::from_le_bytes(req[0..4].try_into().unwrap_or([0; 4]));
    if major == 0 {
        return;
    }
    let lbs = LOOPBACK.lock();
    if lbs.get(mount_point).is_none() {
        return;
    }
    drop(lbs);
    let _ = write_response(
        mount_point,
        unique,
        encode_init(FUSE_KERNEL_VERSION, FUSE_KERNEL_MINOR_VERSION),
    );
}
pub(super) fn serve_loopback_forget(mount_point: &str, unique: u64, nodeid: u64, req: &[u8]) {
    if req.len() < 8 {
        return;
    }
    let nlookup = u64::from_le_bytes(req[0..8].try_into().unwrap_or([0; 8]));
    if nlookup == 0 {
        return;
    }
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    let known = nodeid == 1
        || lb.files.values().any(|(ino, _)| *ino == nodeid)
        || lb.dirs.values().any(|&ino| ino == nodeid)
        || lb.links.values().any(|(ino, _)| *ino == nodeid)
        || lb.nodes.values().any(|(ino, _)| *ino == nodeid);
    if !known {
        return;
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}
pub(super) fn serve_loopback_fallocate(mount_point: &str, unique: u64, nodeid: u64, req: &[u8]) {
    if req.len() < 20 {
        return;
    }
    let offset = u64::from_le_bytes(req[0..8].try_into().unwrap_or([0; 8]));
    let length = u64::from_le_bytes(req[8..16].try_into().unwrap_or([0; 8]));
    let mode = u32::from_le_bytes(req[16..20].try_into().unwrap_or([0; 4]));
    let mut lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get_mut(mount_point) else {
        return;
    };
    let Some((_, data)) = lb.files.values_mut().find(|(ino, _)| *ino == nodeid) else {
        return;
    };
    if mode == 0 {
        let end = offset.saturating_add(length) as usize;
        if end > data.len() {
            data.resize(end, 0);
        }
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}
