/// User/Group Authentication System
/// Compatible with Linux user/group management
/// Provides passwd/shadow/group file parsing and authentication
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

// ─── SHA-256 implementation (no_std) ────────────────────────────────────
// Used for password hashing. Implements FIPS 180-4.

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // Pre-processing: pad message to 512-bit boundary
    let bit_len = (data.len() as u64) * 8;
    let mut padded = Vec::from(data);
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    // Process each 512-bit (64-byte) block
    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut digest = [0u8; 32];
    for (i, val) in h.iter().enumerate() {
        digest[i * 4..i * 4 + 4].copy_from_slice(&val.to_be_bytes());
    }
    digest
}

/// Hash a password with a salt using SHA-256.
/// Format: `$5$<salt>$<hex-digest>` (compatible with crypt(3) $5$ scheme idea)
pub fn hash_password(password: &str, salt: &str) -> String {
    // Combine salt + password for the hash input
    let mut input = Vec::new();
    input.extend_from_slice(salt.as_bytes());
    input.push(b'$');
    input.extend_from_slice(password.as_bytes());

    // Multiple rounds of hashing for key stretching (5000 rounds, matching crypt $5$ default)
    let mut digest = sha256(&input);
    for _ in 0..4999 {
        let mut round_input = Vec::with_capacity(32 + salt.len() + 1);
        round_input.extend_from_slice(&digest);
        round_input.extend_from_slice(salt.as_bytes());
        digest = sha256(&round_input);
    }

    // Encode as hex string
    let mut hex = String::with_capacity(5 + salt.len() + 1 + 64);
    hex.push_str("$5$");
    hex.push_str(salt);
    hex.push('$');
    for byte in &digest {
        use core::fmt::Write;
        let _ = write!(hex, "{:02x}", byte);
    }
    hex
}

/// Verify a password against a stored hash.
/// Supports:
///   - `$5$<salt>$<hex>` — SHA-256 hashed password
///   - `*` — allows any password (service accounts)
///   - empty string — empty password accepted
///   - plain text (legacy, no `$` prefix) — compared directly (deprecated)
pub fn verify_password(password: &str, stored_hash: &str) -> bool {
    // Service accounts with wildcard password
    if stored_hash == "*" {
        return true;
    }
    // Empty stored hash matches empty password only
    if stored_hash.is_empty() {
        return password.is_empty();
    }
    // SHA-256 salted hash
    if stored_hash.starts_with("$5$") {
        // Extract salt from "$5$<salt>$<digest>"
        if let Some(rest) = stored_hash.strip_prefix("$5$") {
            if let Some(dollar_pos) = rest.find('$') {
                let salt = &rest[..dollar_pos];
                let computed = hash_password(password, salt);
                return constant_time_eq(computed.as_bytes(), stored_hash.as_bytes());
            }
        }
        return false;
    }
    // Legacy plain-text fallback (for backwards compatibility during migration)
    password == stored_hash
}

/// Constant-time byte comparison to prevent timing attacks
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// User account information (like struct passwd)
#[derive(Debug, Clone)]
pub struct UserInfo {
    pub username: String,
    pub uid: u32,
    pub gid: u32,
    pub gecos: String, // Full name/comment
    pub home_dir: String,
    pub shell: String,
    pub password_hash: String,
    pub disabled: bool,
}

/// Group information
#[derive(Debug, Clone)]
pub struct GroupInfo {
    pub name: String,
    pub gid: u32,
    pub members: Vec<String>,
}

/// Global user and group databases
lazy_static::lazy_static! {
    static ref USERS: Mutex<BTreeMap<u32, UserInfo>> = Mutex::new(BTreeMap::new());
    static ref GROUPS: Mutex<BTreeMap<u32, GroupInfo>> = Mutex::new(BTreeMap::new());
    /// Username -> UID lookup
    static ref USERNAME_MAP: Mutex<BTreeMap<String, u32>> = Mutex::new(BTreeMap::new());
    /// Group name -> GID lookup
    static ref GROUPNAME_MAP: Mutex<BTreeMap<String, u32>> = Mutex::new(BTreeMap::new());
}

/// Add a user to the system
pub fn add_user(info: UserInfo) {
    let uid = info.uid;
    let username = info.username.clone();
    USERNAME_MAP.lock().insert(username, uid);
    USERS.lock().insert(uid, info);
}

/// Add a group to the system
pub fn add_group(info: GroupInfo) {
    let gid = info.gid;
    let name = info.name.clone();
    GROUPNAME_MAP.lock().insert(name, gid);
    GROUPS.lock().insert(gid, info);
}

/// Look up a user by UID
pub fn get_user(uid: u32) -> Option<UserInfo> {
    USERS.lock().get(&uid).cloned()
}

/// Look up a user by username
pub fn get_user_by_name(name: &str) -> Option<UserInfo> {
    let uid = USERNAME_MAP.lock().get(name).copied()?;
    USERS.lock().get(&uid).cloned()
}

/// Look up a group by GID
pub fn get_group(gid: u32) -> Option<GroupInfo> {
    GROUPS.lock().get(&gid).cloned()
}

/// Look up a group by name
pub fn get_group_by_name(name: &str) -> Option<GroupInfo> {
    let gid = GROUPNAME_MAP.lock().get(name).copied()?;
    GROUPS.lock().get(&gid).cloned()
}

/// List all users in the system
pub fn list_users() -> Vec<UserInfo> {
    USERS.lock().values().cloned().collect()
}

/// Authenticate a user (verify password)
pub fn authenticate(username: &str, password: &str) -> Result<u32, &'static str> {
    let uid = USERNAME_MAP
        .lock()
        .get(username)
        .copied()
        .ok_or("User not found")?;
    let user = USERS.lock().get(&uid).cloned().ok_or("User not found")?;

    if user.disabled {
        return Err("Account disabled");
    }

    if verify_password(password, &user.password_hash) {
        Ok(uid)
    } else {
        Err("Invalid password")
    }
}

/// Set UID for current process (setuid)
pub fn setuid(uid: u32) -> Result<(), i32> {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut table = crate::process::PROCESS_TABLE.lock();
    let proc = table.get_process_mut(pid).ok_or(-3i32)?;

    // Only root (uid 0) can change to arbitrary UIDs
    if proc.uid != 0 && uid != proc.uid {
        return Err(-1); // EPERM
    }

    proc.uid = uid;
    crate::serial_println!("[KnoxOS] setuid: PID {} -> UID {}", pid, uid);
    Ok(())
}

/// Set GID for current process (setgid)
pub fn setgid(gid: u32) -> Result<(), i32> {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut table = crate::process::PROCESS_TABLE.lock();
    let proc = table.get_process_mut(pid).ok_or(-3i32)?;

    if proc.uid != 0 && gid != proc.gid {
        return Err(-1); // EPERM
    }

    proc.gid = gid;
    crate::serial_println!("[KnoxOS] setgid: PID {} -> GID {}", pid, gid);
    Ok(())
}

/// Set effective UID (seteuid)
pub fn seteuid(uid: u32) -> Result<(), i32> {
    // In this simple implementation, seteuid = setuid
    setuid(uid)
}

/// Set effective GID (setegid)
pub fn setegid(gid: u32) -> Result<(), i32> {
    setgid(gid)
}

/// Set real and effective UID (setreuid)
pub fn setreuid(ruid: u32, euid: u32) -> Result<(), i32> {
    if ruid != u32::MAX {
        setuid(ruid)?;
    }
    if euid != u32::MAX {
        seteuid(euid)?;
    }
    Ok(())
}

/// Set real and effective GID (setregid)
pub fn setregid(rgid: u32, egid: u32) -> Result<(), i32> {
    if rgid != u32::MAX {
        setgid(rgid)?;
    }
    if egid != u32::MAX {
        setegid(egid)?;
    }
    Ok(())
}

/// Get supplementary group IDs (getgroups)
pub fn getgroups(pid: u32) -> Vec<u32> {
    let table = crate::process::PROCESS_TABLE.lock();
    if let Some(proc) = table.get_process(pid) {
        let username = get_user(proc.uid).map(|u| u.username).unwrap_or_default();
        let groups = GROUPS.lock();
        groups
            .values()
            .filter(|g| g.members.contains(&username) || g.gid == proc.gid)
            .map(|g| g.gid)
            .collect()
    } else {
        Vec::new()
    }
}

/// Set supplementary groups (setgroups) - requires root
pub fn setgroups(_groups: &[u32]) -> Result<(), i32> {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let table = crate::process::PROCESS_TABLE.lock();
    let proc = table.get_process(pid).ok_or(-3i32)?;
    if proc.uid != 0 {
        return Err(-1); // EPERM
    }
    Ok(())
}

/// Generate /etc/passwd content
pub fn generate_passwd() -> String {
    let users = USERS.lock();
    let mut output = String::new();
    for user in users.values() {
        use core::fmt::Write;
        let _ = writeln!(
            output,
            "{}:x:{}:{}:{}:{}:{}",
            user.username, user.uid, user.gid, user.gecos, user.home_dir, user.shell
        );
    }
    output
}

/// Generate /etc/group content
pub fn generate_group() -> String {
    let groups = GROUPS.lock();
    let mut output = String::new();
    for group in groups.values() {
        use core::fmt::Write;
        let _ = writeln!(
            output,
            "{}:x:{}:{}",
            group.name,
            group.gid,
            group.members.join(",")
        );
    }
    output
}

/// Get the current process's UID
pub fn get_current_uid() -> u32 {
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    let table = crate::process::PROCESS_TABLE.lock();
    table.get_process(pid).map(|p| p.uid).unwrap_or(0)
}

/// Get the current process's GID
pub fn get_current_gid() -> u32 {
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    let table = crate::process::PROCESS_TABLE.lock();
    table.get_process(pid).map(|p| p.gid).unwrap_or(0)
}

/// Change a user's password (requires old password or root)
pub fn change_password(
    username: &str,
    old_password: &str,
    new_password: &str,
) -> Result<(), &'static str> {
    let uid = USERNAME_MAP
        .lock()
        .get(username)
        .copied()
        .ok_or("User not found")?;

    // Verify old password (skip if caller is root)
    let caller_uid = get_current_uid();
    if caller_uid != 0 {
        authenticate(username, old_password)?;
    }

    // Generate a simple salt from the username + UID
    let salt = {
        let mut s = String::from(username);
        use core::fmt::Write;
        let _ = write!(s, "{}", uid);
        s
    };

    let new_hash = hash_password(new_password, &salt);

    let mut users = USERS.lock();
    if let Some(user) = users.get_mut(&uid) {
        user.password_hash = new_hash;
        crate::serial_println!("[KnoxOS] Password changed for '{}'", username);
        Ok(())
    } else {
        Err("User not found")
    }
}

/// Initialize user/group database with standard accounts
pub fn init() {
    // Standard system users
    add_user(UserInfo {
        username: String::from("root"),
        uid: 0,
        gid: 0,
        gecos: String::from("Root User"),
        home_dir: String::from("/root"),
        shell: String::from("/bin/sh"),
        password_hash: String::from("*"),
        disabled: false,
    });

    add_user(UserInfo {
        username: String::from("nobody"),
        uid: 65534,
        gid: 65534,
        gecos: String::from("Nobody"),
        home_dir: String::from("/nonexistent"),
        shell: String::from("/usr/sbin/nologin"),
        password_hash: String::from("*"),
        disabled: true,
    });

    add_user(UserInfo {
        username: String::from("daemon"),
        uid: 1,
        gid: 1,
        gecos: String::from("System Daemon"),
        home_dir: String::from("/usr/sbin"),
        shell: String::from("/usr/sbin/nologin"),
        password_hash: String::from("*"),
        disabled: false,
    });

    add_user(UserInfo {
        username: String::from("user"),
        uid: 1000,
        gid: 1000,
        gecos: String::from("KnoxOS User"),
        home_dir: String::from("/home/user"),
        shell: String::from("/bin/sh"),
        password_hash: hash_password("", "knoxos"), // empty password, but properly hashed
        disabled: false,
    });

    // Standard groups
    add_group(GroupInfo {
        name: String::from("root"),
        gid: 0,
        members: vec![String::from("root")],
    });
    add_group(GroupInfo {
        name: String::from("daemon"),
        gid: 1,
        members: vec![String::from("daemon")],
    });
    add_group(GroupInfo {
        name: String::from("sys"),
        gid: 3,
        members: Vec::new(),
    });
    add_group(GroupInfo {
        name: String::from("adm"),
        gid: 4,
        members: vec![String::from("user")],
    });
    add_group(GroupInfo {
        name: String::from("tty"),
        gid: 5,
        members: Vec::new(),
    });
    add_group(GroupInfo {
        name: String::from("disk"),
        gid: 6,
        members: Vec::new(),
    });
    add_group(GroupInfo {
        name: String::from("wheel"),
        gid: 10,
        members: vec![String::from("user")],
    });
    add_group(GroupInfo {
        name: String::from("audio"),
        gid: 29,
        members: vec![String::from("user")],
    });
    add_group(GroupInfo {
        name: String::from("video"),
        gid: 44,
        members: vec![String::from("user")],
    });
    add_group(GroupInfo {
        name: String::from("users"),
        gid: 100,
        members: vec![String::from("user")],
    });
    add_group(GroupInfo {
        name: String::from("user"),
        gid: 1000,
        members: vec![String::from("user")],
    });
    add_group(GroupInfo {
        name: String::from("nogroup"),
        gid: 65534,
        members: Vec::new(),
    });

    crate::serial_println!(
        "[KnoxOS] User/group authentication initialized ({} users, {} groups)",
        USERS.lock().len(),
        GROUPS.lock().len()
    );
}
