/// xattr — Extended attributes for filesystem inodes
/// Linux-compatible xattr operations (setxattr, getxattr, listxattr, removexattr)
///
/// Namespaces: user, system, security, trusted
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Maximum xattr name length
const XATTR_NAME_MAX: usize = 255;
/// Maximum xattr value size
const XATTR_SIZE_MAX: usize = 65536;
/// Maximum total xattrs per inode
const XATTR_LIST_MAX: usize = 65536;

/// Xattr namespaces
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum XattrNamespace {
    User,
    System,
    Security,
    Trusted,
}

impl XattrNamespace {
    pub fn from_name(name: &str) -> Option<(Self, &str)> {
        if let Some(rest) = name.strip_prefix("user.") {
            Some((Self::User, rest))
        } else if let Some(rest) = name.strip_prefix("system.") {
            Some((Self::System, rest))
        } else if let Some(rest) = name.strip_prefix("security.") {
            Some((Self::Security, rest))
        } else if let Some(rest) = name.strip_prefix("trusted.") {
            Some((Self::Trusted, rest))
        } else {
            None
        }
    }
}

/// Xattr flags
pub const XATTR_CREATE: i32 = 1;
pub const XATTR_REPLACE: i32 = 2;

/// Extended attributes storage for a single inode
#[derive(Debug, Clone, Default)]
pub struct InodeXattrs {
    attrs: BTreeMap<String, Vec<u8>>,
}

impl InodeXattrs {
    pub fn new() -> Self {
        Self {
            attrs: BTreeMap::new(),
        }
    }

    /// Set an extended attribute
    pub fn set(&mut self, name: &str, value: &[u8], flags: i32) -> Result<(), i32> {
        if name.len() > XATTR_NAME_MAX {
            return Err(-34); // ERANGE
        }
        if value.len() > XATTR_SIZE_MAX {
            return Err(-28); // ENOSPC
        }

        // Validate namespace
        if XattrNamespace::from_name(name).is_none() {
            return Err(-95); // EOPNOTSUPP
        }

        let exists = self.attrs.contains_key(name);

        if flags == XATTR_CREATE && exists {
            return Err(-17); // EEXIST
        }
        if flags == XATTR_REPLACE && !exists {
            return Err(-61); // ENODATA
        }

        self.attrs.insert(String::from(name), value.to_vec());
        Ok(())
    }

    /// Get an extended attribute
    pub fn get(&self, name: &str) -> Result<&[u8], i32> {
        if XattrNamespace::from_name(name).is_none() {
            return Err(-95); // EOPNOTSUPP
        }
        self.attrs.get(name).map(|v| v.as_slice()).ok_or(-61) // ENODATA
    }

    /// List all extended attribute names
    pub fn list(&self) -> Vec<&str> {
        self.attrs.keys().map(|k| k.as_str()).collect()
    }

    /// Remove an extended attribute
    pub fn remove(&mut self, name: &str) -> Result<(), i32> {
        if XattrNamespace::from_name(name).is_none() {
            return Err(-95); // EOPNOTSUPP
        }
        self.attrs.remove(name).ok_or(-61)?; // ENODATA
        Ok(())
    }

    /// Total size of all xattr names and values
    pub fn total_size(&self) -> usize {
        self.attrs.iter().map(|(k, v)| k.len() + 1 + v.len()).sum()
    }
}

/// Global xattr storage: inode_id → InodeXattrs
lazy_static::lazy_static! {
    static ref XATTR_STORE: Mutex<BTreeMap<u64, InodeXattrs>> = Mutex::new(BTreeMap::new());
}

/// Set an extended attribute on an inode
pub fn setxattr(inode_id: u64, name: &str, value: &[u8], flags: i32) -> Result<(), i32> {
    let pid = crate::scheduler::current_pid().unwrap_or(0);

    // Permission check: security.* and trusted.* require CAP_SYS_ADMIN
    if let Some((ns @ (XattrNamespace::Security | XattrNamespace::Trusted), _)) =
        XattrNamespace::from_name(name)
    {
        // Check if caller is root/has capability
        let _ = ns;
        let uid = crate::users::get_current_uid();
        if uid != 0 {
            return Err(-1); // EPERM
        }
    }

    let mut store = XATTR_STORE.lock();
    let xattrs = store.entry(inode_id).or_default();
    xattrs.set(name, value, flags)?;

    // Audit log
    let uid = crate::users::get_current_uid();
    crate::audit::log_file_access(
        pid,
        uid,
        &alloc::format!("inode:{}", inode_id),
        "setxattr",
        true,
    );

    Ok(())
}

/// Get an extended attribute from an inode
pub fn getxattr(inode_id: u64, name: &str, buf: &mut [u8]) -> Result<usize, i32> {
    let store = XATTR_STORE.lock();
    let xattrs = store.get(&inode_id).ok_or(-61i32)?; // ENODATA
    let value = xattrs.get(name)?;

    if buf.is_empty() {
        // Return size without copying
        return Ok(value.len());
    }

    if buf.len() < value.len() {
        return Err(-34); // ERANGE
    }

    buf[..value.len()].copy_from_slice(value);
    Ok(value.len())
}

/// List extended attributes on an inode
pub fn listxattr(inode_id: u64, buf: &mut [u8]) -> Result<usize, i32> {
    let store = XATTR_STORE.lock();
    let xattrs = match store.get(&inode_id) {
        Some(x) => x,
        None => return Ok(0),
    };

    let names = xattrs.list();
    let total_size: usize = names.iter().map(|n| n.len() + 1).sum();

    if buf.is_empty() {
        return Ok(total_size);
    }

    if buf.len() < total_size {
        return Err(-34); // ERANGE
    }

    let mut offset = 0;
    for name in names {
        let bytes = name.as_bytes();
        buf[offset..offset + bytes.len()].copy_from_slice(bytes);
        buf[offset + bytes.len()] = 0; // null terminator
        offset += bytes.len() + 1;
    }

    Ok(offset)
}

/// Remove an extended attribute from an inode
pub fn removexattr(inode_id: u64, name: &str) -> Result<(), i32> {
    let pid = crate::scheduler::current_pid().unwrap_or(0);

    let mut store = XATTR_STORE.lock();
    let xattrs = store.get_mut(&inode_id).ok_or(-61i32)?;
    xattrs.remove(name)?;

    let uid = crate::users::get_current_uid();
    crate::audit::log_file_access(
        pid,
        uid,
        &alloc::format!("inode:{}", inode_id),
        "removexattr",
        true,
    );

    Ok(())
}

/// Remove all xattrs for an inode (called when inode is deleted)
pub fn remove_all(inode_id: u64) {
    let mut store = XATTR_STORE.lock();
    store.remove(&inode_id);
}

pub fn init() {
    serial_println!("[KnoxOS] Extended attributes (xattr) initialized");
}
