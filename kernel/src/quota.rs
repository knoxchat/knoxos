/// quota — Filesystem disk quota support
/// Linux-compatible quota management (quotactl)
///
/// Per-user and per-group quotas for block and inode usage
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Quota types
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum QuotaType {
    User = 0,
    Group = 1,
    Project = 2,
}

/// Quota limits and usage for one entity (user or group)
#[derive(Debug, Clone, Copy, Default)]
pub struct DqBlk {
    /// Hard limit for disk blocks (in KiB)
    pub dqb_bhardlimit: u64,
    /// Soft limit for disk blocks (in KiB)
    pub dqb_bsoftlimit: u64,
    /// Current block usage (in KiB)
    pub dqb_curspace: u64,
    /// Hard limit for inodes
    pub dqb_ihardlimit: u64,
    /// Soft limit for inodes
    pub dqb_isoftlimit: u64,
    /// Current inode count
    pub dqb_curinodes: u64,
    /// Time soft block limit is exceeded
    pub dqb_btime: u64,
    /// Time soft inode limit is exceeded
    pub dqb_itime: u64,
}

/// Quota info for a filesystem
#[derive(Debug, Clone, Copy)]
pub struct DqInfo {
    /// Grace period for soft block limit (seconds)
    pub dqi_bgrace: u64,
    /// Grace period for soft inode limit (seconds)
    pub dqi_igrace: u64,
    /// Flags
    pub dqi_flags: u32,
}

impl Default for DqInfo {
    fn default() -> Self {
        DqInfo {
            dqi_bgrace: 7 * 86400, // 7 days
            dqi_igrace: 7 * 86400,
            dqi_flags: 0,
        }
    }
}

/// Quota subsystem state
struct QuotaState {
    /// Whether quotas are enabled per filesystem
    enabled: BTreeMap<String, bool>,
    /// User quotas: (filesystem, uid) → DqBlk
    user_quotas: BTreeMap<(String, u32), DqBlk>,
    /// Group quotas: (filesystem, gid) → DqBlk
    group_quotas: BTreeMap<(String, u32), DqBlk>,
    /// Quota info per filesystem
    info: BTreeMap<(String, QuotaType), DqInfo>,
}

lazy_static::lazy_static! {
    static ref QUOTA_STATE: Mutex<QuotaState> = Mutex::new(QuotaState {
        enabled: BTreeMap::new(),
        user_quotas: BTreeMap::new(),
        group_quotas: BTreeMap::new(),
        info: BTreeMap::new(),
    });
}

/// quotactl commands (matching Linux Q_* constants)
pub const Q_QUOTAON: i32 = 0x800002;
pub const Q_QUOTAOFF: i32 = 0x800003;
pub const Q_GETQUOTA: i32 = 0x800007;
pub const Q_SETQUOTA: i32 = 0x800008;
pub const Q_GETINFO: i32 = 0x800005;
pub const Q_SETINFO: i32 = 0x800006;
pub const Q_SYNC: i32 = 0x800001;

/// Enable quotas for a filesystem
pub fn quota_on(device: &str, quota_type: QuotaType) -> Result<(), i32> {
    let uid = crate::users::get_current_uid();
    if uid != 0 {
        return Err(-1); // EPERM
    }

    let mut state = QUOTA_STATE.lock();
    state.enabled.insert(String::from(device), true);
    state
        .info
        .entry((String::from(device), quota_type))
        .or_default();

    serial_println!(
        "[KnoxOS] Quotas enabled for {} (type {:?})",
        device,
        quota_type
    );
    Ok(())
}

/// Disable quotas for a filesystem
pub fn quota_off(device: &str, _quota_type: QuotaType) -> Result<(), i32> {
    let uid = crate::users::get_current_uid();
    if uid != 0 {
        return Err(-1); // EPERM
    }

    let mut state = QUOTA_STATE.lock();
    state.enabled.insert(String::from(device), false);
    Ok(())
}

/// Get quota for a user/group
pub fn get_quota(device: &str, quota_type: QuotaType, id: u32) -> Result<DqBlk, i32> {
    let state = QUOTA_STATE.lock();

    let quota = match quota_type {
        QuotaType::User => state.user_quotas.get(&(String::from(device), id)),
        QuotaType::Group => state.group_quotas.get(&(String::from(device), id)),
        QuotaType::Project => None,
    };

    Ok(quota.copied().unwrap_or_default())
}

/// Set quota for a user/group
pub fn set_quota(device: &str, quota_type: QuotaType, id: u32, dqblk: DqBlk) -> Result<(), i32> {
    let uid = crate::users::get_current_uid();
    if uid != 0 {
        return Err(-1); // EPERM
    }

    let mut state = QUOTA_STATE.lock();

    match quota_type {
        QuotaType::User => {
            state.user_quotas.insert((String::from(device), id), dqblk);
        }
        QuotaType::Group => {
            state.group_quotas.insert((String::from(device), id), dqblk);
        }
        QuotaType::Project => return Err(-95), // EOPNOTSUPP
    }

    Ok(())
}

/// Check if a user/group can allocate blocks
pub fn check_block_quota(device: &str, uid: u32, gid: u32, blocks: u64) -> Result<(), i32> {
    let state = QUOTA_STATE.lock();

    if !state.enabled.get(device).copied().unwrap_or(false) {
        return Ok(()); // Quotas not enabled
    }

    // Check user quota
    if let Some(quota) = state.user_quotas.get(&(String::from(device), uid)) {
        if quota.dqb_bhardlimit > 0 && quota.dqb_curspace + blocks > quota.dqb_bhardlimit {
            return Err(-122); // EDQUOT
        }
    }

    // Check group quota
    if let Some(quota) = state.group_quotas.get(&(String::from(device), gid)) {
        if quota.dqb_bhardlimit > 0 && quota.dqb_curspace + blocks > quota.dqb_bhardlimit {
            return Err(-122); // EDQUOT
        }
    }

    Ok(())
}

/// Check if a user/group can create inodes
pub fn check_inode_quota(device: &str, uid: u32, gid: u32) -> Result<(), i32> {
    let state = QUOTA_STATE.lock();

    if !state.enabled.get(device).copied().unwrap_or(false) {
        return Ok(());
    }

    if let Some(quota) = state.user_quotas.get(&(String::from(device), uid)) {
        if quota.dqb_ihardlimit > 0 && quota.dqb_curinodes >= quota.dqb_ihardlimit {
            return Err(-122); // EDQUOT
        }
    }

    if let Some(quota) = state.group_quotas.get(&(String::from(device), gid)) {
        if quota.dqb_ihardlimit > 0 && quota.dqb_curinodes >= quota.dqb_ihardlimit {
            return Err(-122); // EDQUOT
        }
    }

    Ok(())
}

/// Update usage after block allocation
pub fn charge_blocks(device: &str, uid: u32, gid: u32, blocks: u64) {
    let mut state = QUOTA_STATE.lock();

    if let Some(quota) = state.user_quotas.get_mut(&(String::from(device), uid)) {
        quota.dqb_curspace += blocks;
    }
    if let Some(quota) = state.group_quotas.get_mut(&(String::from(device), gid)) {
        quota.dqb_curspace += blocks;
    }
}

/// Update usage after inode creation
pub fn charge_inode(device: &str, uid: u32, gid: u32) {
    let mut state = QUOTA_STATE.lock();

    if let Some(quota) = state.user_quotas.get_mut(&(String::from(device), uid)) {
        quota.dqb_curinodes += 1;
    }
    if let Some(quota) = state.group_quotas.get_mut(&(String::from(device), gid)) {
        quota.dqb_curinodes += 1;
    }
}

/// Unified quotactl interface
pub fn quotactl(cmd: i32, device: &str, id: u32, addr: u64) -> Result<i32, i32> {
    let quota_type = QuotaType::User; // Simplified

    match cmd {
        Q_QUOTAON => {
            quota_on(device, quota_type)?;
            Ok(0)
        }
        Q_QUOTAOFF => {
            quota_off(device, quota_type)?;
            Ok(0)
        }
        Q_GETQUOTA => {
            let dq = get_quota(device, quota_type, id)?;
            if addr != 0 {
                unsafe {
                    *(addr as *mut DqBlk) = dq;
                }
            }
            Ok(0)
        }
        Q_SETQUOTA => {
            if addr == 0 {
                return Err(-22); // EINVAL
            }
            let dq = unsafe { *(addr as *const DqBlk) };
            set_quota(device, quota_type, id, dq)?;
            Ok(0)
        }
        Q_SYNC => Ok(0), // No-op
        _ => Err(-22),   // EINVAL
    }
}

pub fn init() {
    serial_println!("[KnoxOS] Disk quota subsystem initialized");
}
