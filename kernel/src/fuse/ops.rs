//! VFS-facing FUSE operations.

use alloc::string::String;
use alloc::vec::Vec;

use super::loopback::{
    serve_loopback, serve_loopback_create, serve_loopback_getattr, serve_loopback_link,
    serve_loopback_mkdir, serve_loopback_mknod, serve_loopback_read, serve_loopback_readdir,
    serve_loopback_readlink, serve_loopback_rename, serve_loopback_rmdir, serve_loopback_setattr,
    serve_loopback_statfs, serve_loopback_symlink, serve_loopback_unlink, serve_loopback_write,
};
use super::mount::{send_request, take_response};
use super::protocol::{
    FuseAttr, FuseEntry, FuseOpcode, FuseStatfs, parse_fuse_dirents, parse_fuse_entry,
    parse_fuse_statfs,
};

/// Handle a VFS lookup through FUSE
pub fn fuse_lookup(mount_point: &str, parent: u64, name: &str) -> Result<FuseEntry, &'static str> {
    let unique = send_request(
        mount_point,
        FuseOpcode::Lookup,
        parent,
        name.as_bytes().to_vec(),
    )?;
    serve_loopback(mount_point, FuseOpcode::Lookup, unique, name.as_bytes());
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    let (ino, size, mode) = parse_fuse_entry(&resp).ok_or("short entry")?;
    Ok(FuseEntry {
        nodeid: ino,
        generation: 1,
        attr: FuseAttr {
            ino,
            size,
            blocks: size.div_ceil(512),
            mode,
            nlink: 1,
            uid: 0,
            gid: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
        },
        attr_valid_secs: 1,
        entry_valid_secs: 1,
    })
}

/// Handle a VFS read through FUSE
pub fn fuse_read(
    mount_point: &str,
    nodeid: u64,
    offset: u64,
    size: u32,
) -> Result<Vec<u8>, &'static str> {
    let mut data = Vec::with_capacity(16);
    data.extend_from_slice(&offset.to_le_bytes());
    data.extend_from_slice(&(size as u64).to_le_bytes());
    let unique = send_request(mount_point, FuseOpcode::Read, nodeid, data)?;
    serve_loopback_read(mount_point, unique, nodeid);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    let start = offset as usize;
    let end = start.saturating_add(size as usize).min(resp.len());
    if start >= resp.len() {
        Ok(Vec::new())
    } else {
        Ok(resp[start..end].to_vec())
    }
}
/// Handle a VFS write through FUSE
pub fn fuse_write(
    mount_point: &str,
    nodeid: u64,
    offset: u64,
    data: &[u8],
) -> Result<usize, &'static str> {
    let mut req_data = Vec::with_capacity(8 + data.len());
    req_data.extend_from_slice(&offset.to_le_bytes());
    req_data.extend_from_slice(data);
    let unique = send_request(mount_point, FuseOpcode::Write, nodeid, req_data.clone())?;
    serve_loopback_write(mount_point, unique, nodeid, &req_data);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    if resp.len() < 4 {
        return Err("short write");
    }
    Ok(u32::from_le_bytes(resp[0..4].try_into().map_err(|_| "short write")?) as usize)
}
/// Handle a VFS unlink through FUSE.
pub fn fuse_unlink(mount_point: &str, parent: u64, name: &str) -> Result<(), &'static str> {
    let unique = send_request(
        mount_point,
        FuseOpcode::Unlink,
        parent,
        name.as_bytes().to_vec(),
    )?;
    serve_loopback_unlink(mount_point, unique, name);
    take_response(mount_point, unique).ok_or("would block")?;
    Ok(())
}
/// Handle a VFS mkdir through FUSE.
pub fn fuse_mkdir(mount_point: &str, parent: u64, name: &str) -> Result<FuseEntry, &'static str> {
    let unique = send_request(
        mount_point,
        FuseOpcode::Mkdir,
        parent,
        name.as_bytes().to_vec(),
    )?;
    serve_loopback_mkdir(mount_point, unique, name);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    let (ino, size, mode) = parse_fuse_entry(&resp).ok_or("short entry")?;
    Ok(FuseEntry {
        nodeid: ino,
        generation: 1,
        attr: FuseAttr {
            ino,
            size,
            blocks: 0,
            mode,
            nlink: 2,
            uid: 0,
            gid: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
        },
        attr_valid_secs: 1,
        entry_valid_secs: 1,
    })
}
/// Handle a VFS rmdir through FUSE.
pub fn fuse_rmdir(mount_point: &str, parent: u64, name: &str) -> Result<(), &'static str> {
    let unique = send_request(
        mount_point,
        FuseOpcode::Rmdir,
        parent,
        name.as_bytes().to_vec(),
    )?;
    serve_loopback_rmdir(mount_point, unique, name);
    take_response(mount_point, unique).ok_or("would block")?;
    Ok(())
}
/// Handle a VFS rename through FUSE. Request body is `old\0new`.
pub fn fuse_rename(
    mount_point: &str,
    parent: u64,
    old: &str,
    new: &str,
) -> Result<(), &'static str> {
    let mut data = Vec::with_capacity(old.len() + new.len() + 1);
    data.extend_from_slice(old.as_bytes());
    data.push(0);
    data.extend_from_slice(new.as_bytes());
    let unique = send_request(mount_point, FuseOpcode::Rename, parent, data.clone())?;
    serve_loopback_rename(mount_point, unique, &data);
    take_response(mount_point, unique).ok_or("would block")?;
    Ok(())
}
/// Handle a VFS symlink through FUSE. Request body is `name\0target`.
pub fn fuse_symlink(
    mount_point: &str,
    parent: u64,
    name: &str,
    target: &str,
) -> Result<FuseEntry, &'static str> {
    let mut data = Vec::with_capacity(name.len() + target.len() + 1);
    data.extend_from_slice(name.as_bytes());
    data.push(0);
    data.extend_from_slice(target.as_bytes());
    let unique = send_request(mount_point, FuseOpcode::Symlink, parent, data.clone())?;
    serve_loopback_symlink(mount_point, unique, &data);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    let (ino, size, mode) = parse_fuse_entry(&resp).ok_or("short entry")?;
    Ok(FuseEntry {
        nodeid: ino,
        generation: 1,
        attr: FuseAttr {
            ino,
            size,
            blocks: 0,
            mode,
            nlink: 1,
            uid: 0,
            gid: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
        },
        attr_valid_secs: 1,
        entry_valid_secs: 1,
    })
}
/// Handle a VFS readlink through FUSE.
pub fn fuse_readlink(mount_point: &str, nodeid: u64) -> Result<Vec<u8>, &'static str> {
    let unique = send_request(mount_point, FuseOpcode::Readlink, nodeid, Vec::new())?;
    serve_loopback_readlink(mount_point, unique, nodeid);
    take_response(mount_point, unique).ok_or("would block")
}
/// Handle a VFS hard link through FUSE. Request body is `old\0new`.
pub fn fuse_link(mount_point: &str, parent: u64, old: &str, new: &str) -> Result<(), &'static str> {
    let mut data = Vec::with_capacity(old.len() + new.len() + 1);
    data.extend_from_slice(old.as_bytes());
    data.push(0);
    data.extend_from_slice(new.as_bytes());
    let unique = send_request(mount_point, FuseOpcode::Link, parent, data.clone())?;
    serve_loopback_link(mount_point, unique, &data);
    take_response(mount_point, unique).ok_or("would block")?;
    Ok(())
}
/// Handle a VFS create through FUSE.
pub fn fuse_create(mount_point: &str, parent: u64, name: &str) -> Result<FuseEntry, &'static str> {
    let unique = send_request(
        mount_point,
        FuseOpcode::Create,
        parent,
        name.as_bytes().to_vec(),
    )?;
    serve_loopback_create(mount_point, unique, name);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    let (ino, size, mode) = parse_fuse_entry(&resp).ok_or("short entry")?;
    Ok(FuseEntry {
        nodeid: ino,
        generation: 1,
        attr: FuseAttr {
            ino,
            size,
            blocks: 0,
            mode,
            nlink: 1,
            uid: 0,
            gid: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
        },
        attr_valid_secs: 1,
        entry_valid_secs: 1,
    })
}
/// Handle a VFS getattr through FUSE.
pub fn fuse_getattr(mount_point: &str, nodeid: u64) -> Result<FuseAttr, &'static str> {
    let unique = send_request(mount_point, FuseOpcode::Getattr, nodeid, Vec::new())?;
    serve_loopback_getattr(mount_point, unique, nodeid);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    let (ino, size, mode) = parse_fuse_entry(&resp).ok_or("short entry")?;
    Ok(FuseAttr {
        ino,
        size,
        blocks: size.div_ceil(512),
        mode,
        nlink: 1,
        uid: 0,
        gid: 0,
        atime: 0,
        mtime: 0,
        ctime: 0,
    })
}
/// Handle a VFS setattr through FUSE. Request body is the new size (u64 LE).
pub fn fuse_setattr(mount_point: &str, nodeid: u64, size: u64) -> Result<FuseAttr, &'static str> {
    let mut data = Vec::with_capacity(8);
    data.extend_from_slice(&size.to_le_bytes());
    let unique = send_request(mount_point, FuseOpcode::Setattr, nodeid, data.clone())?;
    serve_loopback_setattr(mount_point, unique, nodeid, &data);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    let (ino, size, mode) = parse_fuse_entry(&resp).ok_or("short entry")?;
    Ok(FuseAttr {
        ino,
        size,
        blocks: size.div_ceil(512),
        mode,
        nlink: 1,
        uid: 0,
        gid: 0,
        atime: 0,
        mtime: 0,
        ctime: 0,
    })
}
/// Handle a VFS readdir through FUSE. Root nodeid 1 lists loopback names.
pub fn fuse_readdir(
    mount_point: &str,
    nodeid: u64,
) -> Result<Vec<(u64, u32, String)>, &'static str> {
    let unique = send_request(mount_point, FuseOpcode::Readdir, nodeid, Vec::new())?;
    serve_loopback_readdir(mount_point, unique, nodeid);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    Ok(parse_fuse_dirents(&resp))
}
/// Handle a VFS mknod through FUSE. Request body is mode (u32 LE) + name.
pub fn fuse_mknod(
    mount_point: &str,
    parent: u64,
    name: &str,
    mode: u32,
) -> Result<FuseEntry, &'static str> {
    let mut data = Vec::with_capacity(4 + name.len());
    data.extend_from_slice(&mode.to_le_bytes());
    data.extend_from_slice(name.as_bytes());
    let unique = send_request(mount_point, FuseOpcode::Mknod, parent, data.clone())?;
    serve_loopback_mknod(mount_point, unique, &data);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    let (ino, size, mode) = parse_fuse_entry(&resp).ok_or("short entry")?;
    Ok(FuseEntry {
        nodeid: ino,
        generation: 1,
        attr: FuseAttr {
            ino,
            size,
            blocks: 0,
            mode,
            nlink: 1,
            uid: 0,
            gid: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
        },
        attr_valid_secs: 1,
        entry_valid_secs: 1,
    })
}
/// Handle a VFS statfs through FUSE.
pub fn fuse_statfs(mount_point: &str) -> Result<FuseStatfs, &'static str> {
    let unique = send_request(mount_point, FuseOpcode::Statfs, 1, Vec::new())?;
    serve_loopback_statfs(mount_point, unique);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    parse_fuse_statfs(&resp).ok_or("short statfs")
}
