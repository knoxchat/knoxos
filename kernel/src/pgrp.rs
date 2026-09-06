/// Process Groups and Sessions - POSIX job control
/// Compatible with Linux process group and session management
/// Enables job control (Ctrl+Z, fg, bg, etc.)
use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::process::Pid;

/// A process group
#[derive(Debug, Clone)]
pub struct ProcessGroup {
    pub pgid: Pid,
    pub members: Vec<Pid>,
    pub session_id: Pid,
}

/// A session
#[derive(Debug, Clone)]
pub struct Session {
    pub sid: Pid,         // Session leader PID
    pub groups: Vec<Pid>, // Process group IDs in this session
    pub controlling_tty: Option<u32>,
    pub foreground_pg: Option<Pid>,
}

/// Global process group and session tables
lazy_static::lazy_static! {
    static ref PROCESS_GROUPS: Mutex<BTreeMap<Pid, ProcessGroup>> = Mutex::new(BTreeMap::new());
    static ref SESSIONS: Mutex<BTreeMap<Pid, Session>> = Mutex::new(BTreeMap::new());
    /// PID -> PGID mapping
    static ref PID_TO_PGID: Mutex<BTreeMap<Pid, Pid>> = Mutex::new(BTreeMap::new());
    /// PID -> SID mapping
    static ref PID_TO_SID: Mutex<BTreeMap<Pid, Pid>> = Mutex::new(BTreeMap::new());
}

/// Create a new session (setsid)
pub fn setsid(pid: Pid) -> Result<Pid, i32> {
    let mut sessions = SESSIONS.lock();
    let mut groups = PROCESS_GROUPS.lock();
    let mut pid_pgid = PID_TO_PGID.lock();
    let mut pid_sid = PID_TO_SID.lock();

    // Process must not already be a process group leader
    if groups.contains_key(&pid) {
        return Err(-1); // EPERM
    }

    // Create new session with pid as session leader
    let session = Session {
        sid: pid,
        groups: vec![pid],
        controlling_tty: None,
        foreground_pg: Some(pid),
    };
    sessions.insert(pid, session);

    // Create new process group with same ID
    let group = ProcessGroup {
        pgid: pid,
        members: vec![pid],
        session_id: pid,
    };
    groups.insert(pid, group);

    pid_pgid.insert(pid, pid);
    pid_sid.insert(pid, pid);

    crate::serial_println!(
        "[KnoxOS] setsid: PID {} is now session leader (SID={})",
        pid,
        pid
    );
    Ok(pid)
}

/// Set process group (setpgid)
pub fn setpgid(pid: Pid, pgid: Pid) -> Result<(), i32> {
    let target_pid = if pid == 0 {
        crate::scheduler::current_pid().unwrap_or(1)
    } else {
        pid
    };
    let target_pgid = if pgid == 0 { target_pid } else { pgid };

    let mut groups = PROCESS_GROUPS.lock();
    let mut pid_pgid = PID_TO_PGID.lock();

    // Remove from current group if in one
    if let Some(&old_pgid) = pid_pgid.get(&target_pid) {
        if let Some(old_group) = groups.get_mut(&old_pgid) {
            old_group.members.retain(|&p| p != target_pid);
            if old_group.members.is_empty() {
                groups.remove(&old_pgid);
            }
        }
    }

    // Add to new group (create if needed)
    let sid = PID_TO_SID.lock().get(&target_pid).copied().unwrap_or(1);
    groups
        .entry(target_pgid)
        .or_insert_with(|| ProcessGroup {
            pgid: target_pgid,
            members: Vec::new(),
            session_id: sid,
        })
        .members
        .push(target_pid);

    pid_pgid.insert(target_pid, target_pgid);

    crate::serial_println!(
        "[KnoxOS] setpgid: PID {} -> PGID {}",
        target_pid,
        target_pgid
    );
    Ok(())
}

/// Get process group ID (getpgid)
pub fn getpgid(pid: Pid) -> Result<Pid, i32> {
    let target_pid = if pid == 0 {
        crate::scheduler::current_pid().unwrap_or(1)
    } else {
        pid
    };

    PID_TO_PGID.lock().get(&target_pid).copied().ok_or(-3) // ESRCH
}

/// Get session ID (getsid)
pub fn getsid(pid: Pid) -> Result<Pid, i32> {
    let target_pid = if pid == 0 {
        crate::scheduler::current_pid().unwrap_or(1)
    } else {
        pid
    };

    PID_TO_SID.lock().get(&target_pid).copied().ok_or(-3) // ESRCH
}

/// Get process group ID of calling process (getpgrp)
pub fn getpgrp() -> Pid {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    PID_TO_PGID.lock().get(&pid).copied().unwrap_or(pid)
}

/// Set controlling terminal for a session
pub fn set_ctty(sid: Pid, tty_num: u32) -> Result<(), i32> {
    let mut sessions = SESSIONS.lock();
    let session = sessions.get_mut(&sid).ok_or(-3i32)?;
    session.controlling_tty = Some(tty_num);
    Ok(())
}

/// Set foreground process group
pub fn tcsetpgrp(tty_fd: i32, pgid: Pid) -> Result<(), i32> {
    // Find the session that owns this tty
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let sid = PID_TO_SID.lock().get(&pid).copied().unwrap_or(1);

    let mut sessions = SESSIONS.lock();
    if let Some(session) = sessions.get_mut(&sid) {
        session.foreground_pg = Some(pgid);
        Ok(())
    } else {
        Err(-25) // ENOTTY
    }
}

/// Get foreground process group
pub fn tcgetpgrp(tty_fd: i32) -> Result<Pid, i32> {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let sid = PID_TO_SID.lock().get(&pid).copied().unwrap_or(1);

    let sessions = SESSIONS.lock();
    if let Some(session) = sessions.get(&sid) {
        session.foreground_pg.ok_or(-25) // ENOTTY
    } else {
        Err(-25) // ENOTTY
    }
}

/// Send signal to a process group
pub fn killpg(pgid: Pid, signal: crate::signals::Signal) -> Result<(), i32> {
    let groups = PROCESS_GROUPS.lock();
    let group = groups.get(&pgid).ok_or(-3i32)?; // ESRCH
    let members = group.members.clone();
    let sender = crate::scheduler::current_pid().unwrap_or(0);
    drop(groups);

    for &pid in &members {
        let _ = crate::signals::kill(pid, signal, sender);
    }
    Ok(())
}

/// Register a new process in the session/group system
pub fn register_process(pid: Pid, ppid: Pid) {
    let mut pid_pgid = PID_TO_PGID.lock();
    let mut pid_sid = PID_TO_SID.lock();

    // Inherit parent's process group and session
    let parent_pgid = pid_pgid.get(&ppid).copied().unwrap_or(ppid);
    let parent_sid = pid_sid.get(&ppid).copied().unwrap_or(ppid);

    pid_pgid.insert(pid, parent_pgid);
    pid_sid.insert(pid, parent_sid);

    // Add to parent's process group
    let mut groups = PROCESS_GROUPS.lock();
    if let Some(group) = groups.get_mut(&parent_pgid) {
        group.members.push(pid);
    }
}

/// Unregister a process (on exit)
pub fn unregister_process(pid: Pid) {
    let mut pid_pgid = PID_TO_PGID.lock();
    let mut pid_sid = PID_TO_SID.lock();

    if let Some(pgid) = pid_pgid.remove(&pid) {
        let mut groups = PROCESS_GROUPS.lock();
        if let Some(group) = groups.get_mut(&pgid) {
            group.members.retain(|&p| p != pid);
            if group.members.is_empty() {
                groups.remove(&pgid);
            }
        }
    }

    if let Some(sid) = pid_sid.remove(&pid) {
        let mut sessions = SESSIONS.lock();
        if let Some(session) = sessions.get(&sid) {
            if session.sid == pid {
                // Session leader exited - send SIGHUP to foreground group
                if let Some(fg_pgid) = session.foreground_pg {
                    let groups = PROCESS_GROUPS.lock();
                    if let Some(group) = groups.get(&fg_pgid) {
                        for &member in &group.members {
                            let _ = crate::signals::kill(member, crate::signals::Signal::SIGHUP, 0);
                        }
                    }
                }
                drop(sessions);
                SESSIONS.lock().remove(&sid);
            }
        }
    }
}

/// Initialize process groups and sessions
pub fn init() {
    // Set up initial session for init process
    let mut sessions = SESSIONS.lock();
    sessions.insert(
        1,
        Session {
            sid: 1,
            groups: vec![1],
            controlling_tty: Some(0),
            foreground_pg: Some(1),
        },
    );

    let mut groups = PROCESS_GROUPS.lock();
    groups.insert(
        1,
        ProcessGroup {
            pgid: 1,
            members: vec![1, 2],
            session_id: 1,
        },
    );

    let mut pid_pgid = PID_TO_PGID.lock();
    pid_pgid.insert(0, 0);
    pid_pgid.insert(1, 1);
    pid_pgid.insert(2, 1);

    let mut pid_sid = PID_TO_SID.lock();
    pid_sid.insert(0, 0);
    pid_sid.insert(1, 1);
    pid_sid.insert(2, 1);

    crate::serial_println!("[KnoxOS] Process groups and sessions initialized");
}
