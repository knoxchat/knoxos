//! VFS-facing FUSE operations.

use alloc::string::String;
use alloc::vec::Vec;

use super::loopback::{
    serve_loopback, serve_loopback_access, serve_loopback_bmap, serve_loopback_create,
    serve_loopback_destroy, serve_loopback_fallocate, serve_loopback_flush, serve_loopback_fsync,
    serve_loopback_fsyncdir, serve_loopback_getattr, serve_loopback_getlk, serve_loopback_getxattr,
    serve_loopback_ioctl, serve_loopback_link, serve_loopback_listxattr, serve_loopback_mkdir,
    serve_loopback_mknod, serve_loopback_open, serve_loopback_opendir, serve_loopback_poll,
    serve_loopback_read, serve_loopback_readdir, serve_loopback_readlink, serve_loopback_release,
    serve_loopback_releasedir, serve_loopback_removexattr, serve_loopback_rename,
    serve_loopback_rmdir, serve_loopback_setattr, serve_loopback_setlk, serve_loopback_setxattr,
    serve_loopback_statfs, serve_loopback_symlink, serve_loopback_unlink, serve_loopback_write,
};
use super::mount::{send_request, take_response};
use super::protocol::{
    FuseAttr, FuseEntry, FuseFileLock, FuseOpcode, FuseStatfs, encode_lk, parse_fuse_bmap,
    parse_fuse_dirents, parse_fuse_entry, parse_fuse_ioctl, parse_fuse_lk, parse_fuse_open,
    parse_fuse_poll, parse_fuse_statfs,
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
/// Handle a VFS setxattr through FUSE. Request body is `name\0value`.
pub fn fuse_setxattr(
    mount_point: &str,
    nodeid: u64,
    name: &str,
    value: &[u8],
) -> Result<(), &'static str> {
    let mut data = Vec::with_capacity(name.len() + 1 + value.len());
    data.extend_from_slice(name.as_bytes());
    data.push(0);
    data.extend_from_slice(value);
    let unique = send_request(mount_point, FuseOpcode::Setxattr, nodeid, data.clone())?;
    serve_loopback_setxattr(mount_point, unique, nodeid, &data);
    take_response(mount_point, unique).ok_or("would block")?;
    Ok(())
}
/// Handle a VFS getxattr through FUSE. Request body is the attribute name.
pub fn fuse_getxattr(mount_point: &str, nodeid: u64, name: &str) -> Result<Vec<u8>, &'static str> {
    let unique = send_request(
        mount_point,
        FuseOpcode::Getxattr,
        nodeid,
        name.as_bytes().to_vec(),
    )?;
    serve_loopback_getxattr(mount_point, unique, nodeid, name.as_bytes());
    take_response(mount_point, unique).ok_or("would block")
}
/// Handle a VFS listxattr through FUSE. Response is concatenated `name\0` bytes.
pub fn fuse_listxattr(mount_point: &str, nodeid: u64) -> Result<Vec<u8>, &'static str> {
    let unique = send_request(mount_point, FuseOpcode::Listxattr, nodeid, Vec::new())?;
    serve_loopback_listxattr(mount_point, unique, nodeid);
    take_response(mount_point, unique).ok_or("would block")
}
/// Handle a VFS removexattr through FUSE. Request body is the attribute name.
pub fn fuse_removexattr(mount_point: &str, nodeid: u64, name: &str) -> Result<(), &'static str> {
    let unique = send_request(
        mount_point,
        FuseOpcode::Removexattr,
        nodeid,
        name.as_bytes().to_vec(),
    )?;
    serve_loopback_removexattr(mount_point, unique, nodeid, name.as_bytes());
    take_response(mount_point, unique).ok_or("would block")?;
    Ok(())
}
/// Handle a VFS access through FUSE. Request body is the POSIX mask (u32 LE).
/// Reply is the granted bits (u32 LE), from the inode mode, not an echo of the mask.
pub fn fuse_access(mount_point: &str, nodeid: u64, mask: u32) -> Result<u32, &'static str> {
    let mut data = Vec::with_capacity(4);
    data.extend_from_slice(&mask.to_le_bytes());
    let unique = send_request(mount_point, FuseOpcode::Access, nodeid, data.clone())?;
    serve_loopback_access(mount_point, unique, nodeid, &data);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    if resp.len() < 4 {
        return Err("short access");
    }
    Ok(u32::from_le_bytes(resp[0..4].try_into().unwrap_or([0; 4])))
}
/// Handle a VFS fsync through FUSE. Succeeds only for a known inode.
pub fn fuse_fsync(mount_point: &str, nodeid: u64) -> Result<(), &'static str> {
    let unique = send_request(mount_point, FuseOpcode::Fsync, nodeid, Vec::new())?;
    serve_loopback_fsync(mount_point, unique, nodeid);
    take_response(mount_point, unique).ok_or("would block")?;
    Ok(())
}
/// Handle a VFS open through FUSE. Reply is fh u64 + flags u32; unknown nodeid fails.
pub fn fuse_open(mount_point: &str, nodeid: u64) -> Result<(u64, u32), &'static str> {
    let unique = send_request(mount_point, FuseOpcode::Open, nodeid, Vec::new())?;
    serve_loopback_open(mount_point, unique, nodeid);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    parse_fuse_open(&resp).ok_or("short open")
}
/// Handle a VFS release through FUSE. Request body is the fh (u64 LE).
pub fn fuse_release(mount_point: &str, nodeid: u64, fh: u64) -> Result<(), &'static str> {
    let unique = send_request(
        mount_point,
        FuseOpcode::Release,
        nodeid,
        fh.to_le_bytes().to_vec(),
    )?;
    serve_loopback_release(mount_point, unique, nodeid, fh);
    take_response(mount_point, unique).ok_or("would block")?;
    Ok(())
}
/// Handle a VFS flush through FUSE. Succeeds only for a known inode.
pub fn fuse_flush(mount_point: &str, nodeid: u64) -> Result<(), &'static str> {
    let unique = send_request(mount_point, FuseOpcode::Flush, nodeid, Vec::new())?;
    serve_loopback_flush(mount_point, unique, nodeid);
    take_response(mount_point, unique).ok_or("would block")?;
    Ok(())
}
/// Handle a VFS fsyncdir through FUSE. Succeeds only for a known directory.
pub fn fuse_fsyncdir(mount_point: &str, nodeid: u64) -> Result<(), &'static str> {
    let unique = send_request(mount_point, FuseOpcode::Fsyncdir, nodeid, Vec::new())?;
    serve_loopback_fsyncdir(mount_point, unique, nodeid);
    take_response(mount_point, unique).ok_or("would block")?;
    Ok(())
}
/// Handle a VFS opendir through FUSE. Reply is fh u64 + flags u32; a file or unknown nodeid fails.
pub fn fuse_opendir(mount_point: &str, nodeid: u64) -> Result<(u64, u32), &'static str> {
    let unique = send_request(mount_point, FuseOpcode::Opendir, nodeid, Vec::new())?;
    serve_loopback_opendir(mount_point, unique, nodeid);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    parse_fuse_open(&resp).ok_or("short opendir")
}
/// Handle a VFS releasedir through FUSE. Request body is the fh (u64 LE).
pub fn fuse_releasedir(mount_point: &str, nodeid: u64, fh: u64) -> Result<(), &'static str> {
    let unique = send_request(
        mount_point,
        FuseOpcode::Releasedir,
        nodeid,
        fh.to_le_bytes().to_vec(),
    )?;
    serve_loopback_releasedir(mount_point, unique, nodeid, fh);
    take_response(mount_point, unique).ok_or("would block")?;
    Ok(())
}
/// Handle a VFS GETLK through FUSE. Unknown nodeid fails.
pub fn fuse_getlk(
    mount_point: &str,
    nodeid: u64,
    want: FuseFileLock,
) -> Result<FuseFileLock, &'static str> {
    let unique = send_request(mount_point, FuseOpcode::Getlk, nodeid, encode_lk(&want))?;
    serve_loopback_getlk(mount_point, unique, nodeid, &encode_lk(&want));
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    parse_fuse_lk(&resp).ok_or("short getlk")
}
/// Handle a VFS SETLK through FUSE. Conflicts and unknown nodeid fail.
pub fn fuse_setlk(mount_point: &str, nodeid: u64, lk: FuseFileLock) -> Result<(), &'static str> {
    let unique = send_request(mount_point, FuseOpcode::Setlk, nodeid, encode_lk(&lk))?;
    serve_loopback_setlk(mount_point, unique, nodeid, &encode_lk(&lk));
    take_response(mount_point, unique).ok_or("would block")?;
    Ok(())
}
/// Handle a VFS SETLKW through FUSE. Same lock table as SETLK; unknown nodeid fails.
pub fn fuse_setlkw(mount_point: &str, nodeid: u64, lk: FuseFileLock) -> Result<(), &'static str> {
    let unique = send_request(mount_point, FuseOpcode::Setlkw, nodeid, encode_lk(&lk))?;
    serve_loopback_setlk(mount_point, unique, nodeid, &encode_lk(&lk));
    take_response(mount_point, unique).ok_or("would block")?;
    Ok(())
}
/// Handle a VFS BMAP through FUSE. Known files map logical block 0 to nodeid; unknown nodeid fails.
pub fn fuse_bmap(
    mount_point: &str,
    nodeid: u64,
    block: u64,
    blocksize: u32,
) -> Result<u64, &'static str> {
    let mut data = Vec::with_capacity(12);
    data.extend_from_slice(&block.to_le_bytes());
    data.extend_from_slice(&blocksize.to_le_bytes());
    let unique = send_request(mount_point, FuseOpcode::Bmap, nodeid, data.clone())?;
    serve_loopback_bmap(mount_point, unique, nodeid, &data);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    parse_fuse_bmap(&resp).ok_or("short bmap")
}
/// Handle a VFS DESTROY through FUSE. Known mounts succeed; unknown mounts fail.
pub fn fuse_destroy(mount_point: &str) -> Result<(), &'static str> {
    let unique = send_request(mount_point, FuseOpcode::Destroy, 0, Vec::new())?;
    serve_loopback_destroy(mount_point, unique);
    take_response(mount_point, unique).ok_or("would block")?;
    Ok(())
}
/// Handle a VFS IOCTL through FUSE. Known files echo `cmd` as result; unknown nodeid fails.
pub fn fuse_ioctl(mount_point: &str, nodeid: u64, cmd: u32) -> Result<i32, &'static str> {
    let mut data = Vec::with_capacity(32);
    data.extend_from_slice(&0u64.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&cmd.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    let unique = send_request(mount_point, FuseOpcode::Ioctl, nodeid, data.clone())?;
    serve_loopback_ioctl(mount_point, unique, nodeid, &data);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    parse_fuse_ioctl(&resp).ok_or("short ioctl")
}
/// Handle a VFS POLL through FUSE. Known files report POLLIN/POLLOUT; unknown nodeid fails.
pub fn fuse_poll(mount_point: &str, nodeid: u64) -> Result<u32, &'static str> {
    let unique = send_request(mount_point, FuseOpcode::Poll, nodeid, Vec::new())?;
    serve_loopback_poll(mount_point, unique, nodeid);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    parse_fuse_poll(&resp).ok_or("short poll")
}
/// Handle a VFS FALLOCATE through FUSE. Mode 0 extends a known file; unknown nodeid fails.
pub fn fuse_fallocate(
    mount_point: &str,
    nodeid: u64,
    offset: u64,
    length: u64,
    mode: u32,
) -> Result<(), &'static str> {
    let mut data = Vec::with_capacity(20);
    data.extend_from_slice(&offset.to_le_bytes());
    data.extend_from_slice(&length.to_le_bytes());
    data.extend_from_slice(&mode.to_le_bytes());
    let unique = send_request(mount_point, FuseOpcode::Fallocate, nodeid, data.clone())?;
    serve_loopback_fallocate(mount_point, unique, nodeid, &data);
    take_response(mount_point, unique).ok_or("would block")?;
    Ok(())
}
