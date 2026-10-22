/// Session Management — User login/logout session tracking
///
/// Manages user sessions including:
///   - Session creation on login (local, remote, auto)
///   - Session tracking (seat, TTY, display)
///   - Session locking/unlocking
///   - Multi-session support (switch users)
///   - PAM-compatible authentication hooks
///   - Session scopes for cgroup resource tracking
///   - Idle detection and auto-lock
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

/// Session identifier
pub type SessionId = u32;
static NEXT_SESSION_ID: AtomicU32 = AtomicU32::new(1);

/// Session type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionType {
    /// X11 graphical session
    X11,
    /// Wayland graphical session
    Wayland,
    /// Text TTY session
    Tty,
    /// KnoxOS native GUI session
    KnoxGui,
    /// Remote SSH session
    Ssh,
    /// Unspecified
    Unspecified,
}

/// Session state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Session is active (foreground)
    Active,
    /// Session is online but not foreground
    Online,
    /// Session is locked
    Locked,
    /// Session is closing
    Closing,
}

/// Session class
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionClass {
    /// Regular user session
    User,
    /// System greeter (login screen)
    Greeter,
    /// Emergency/rescue session
    Emergency,
    /// Background session
    Background,
}

/// A user session
#[derive(Clone)]
pub struct Session {
    pub id: SessionId,
    pub uid: u32,
    pub username: String,
    pub session_type: SessionType,
    pub class: SessionClass,
    pub state: SessionState,
    pub seat: String,
    pub tty: Option<String>,
    pub display: Option<String>,
    pub remote_host: Option<String>,
    pub created_at: u64,
    pub last_activity: u64,
    pub idle_hint: bool,
    pub locked: bool,
    pub leader_pid: u32,
    pub scope: String,
}

/// Seat — a set of hardware devices (display, keyboard, mouse)
#[derive(Clone)]
pub struct Seat {
    pub name: String,
    pub sessions: Vec<SessionId>,
    pub active_session: Option<SessionId>,
    pub can_multi_session: bool,
    pub can_graphical: bool,
}

/// Session manager state
pub struct SessionManager {
    sessions: BTreeMap<SessionId, Session>,
    seats: BTreeMap<String, Seat>,
    active_session: Option<SessionId>,
    idle_timeout_sec: u64,
}

impl SessionManager {
    pub fn new() -> Self {
        let mut seats = BTreeMap::new();
        // Default seat (local console)
        seats.insert(
            String::from("seat0"),
            Seat {
                name: String::from("seat0"),
                sessions: Vec::new(),
                active_session: None,
                can_multi_session: true,
                can_graphical: true,
            },
        );

        Self {
            sessions: BTreeMap::new(),
            seats,
            active_session: None,
            idle_timeout_sec: 300, // 5 minutes
        }
    }

    /// Create a new session
    pub fn create_session(
        &mut self,
        uid: u32,
        username: &str,
        session_type: SessionType,
        class: SessionClass,
        seat: &str,
        leader_pid: u32,
    ) -> SessionId {
        let id = NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed);
        let now = crate::interrupts::get_ticks();

        let session = Session {
            id,
            uid,
            username: String::from(username),
            session_type,
            class,
            state: SessionState::Active,
            seat: String::from(seat),
            tty: None,
            display: Some(String::from(":0")),
            remote_host: None,
            created_at: now,
            last_activity: now,
            idle_hint: false,
            locked: false,
            leader_pid,
            scope: alloc::format!("session-{}.scope", id),
        };

        self.sessions.insert(id, session);

        // Add to seat
        if let Some(s) = self.seats.get_mut(seat) {
            s.sessions.push(id);
            if s.active_session.is_none() {
                s.active_session = Some(id);
            }
        }

        self.active_session = Some(id);
        serial_println!(
            "[Session] Created session {} for user {} (uid={}, type={:?})",
            id,
            username,
            uid,
            session_type
        );
        id
    }

    /// Close a session
    pub fn close_session(&mut self, id: SessionId) {
        if let Some(session) = self.sessions.get_mut(&id) {
            session.state = SessionState::Closing;
            serial_println!(
                "[Session] Closing session {} (user={})",
                id,
                session.username
            );

            let seat_name = session.seat.clone();
            if let Some(seat) = self.seats.get_mut(&seat_name) {
                seat.sessions.retain(|&s| s != id);
                if seat.active_session == Some(id) {
                    seat.active_session = seat.sessions.first().copied();
                }
            }

            if self.active_session == Some(id) {
                self.active_session = None;
            }
        }
        self.sessions.remove(&id);
    }

    /// Lock a session
    pub fn lock_session(&mut self, id: SessionId) {
        if let Some(session) = self.sessions.get_mut(&id) {
            session.locked = true;
            session.state = SessionState::Locked;
            serial_println!("[Session] Locked session {}", id);
        }
    }

    /// Unlock a session
    pub fn unlock_session(&mut self, id: SessionId) {
        if let Some(session) = self.sessions.get_mut(&id) {
            session.locked = false;
            session.state = SessionState::Active;
            session.last_activity = crate::interrupts::get_ticks();
            serial_println!("[Session] Unlocked session {}", id);
        }
    }

    /// Switch active session on a seat
    pub fn activate_session(&mut self, id: SessionId) {
        if let Some(session) = self.sessions.get(&id) {
            let seat_name = session.seat.clone();
            if let Some(seat) = self.seats.get_mut(&seat_name) {
                // Deactivate current
                if let Some(old_id) = seat.active_session {
                    if let Some(old) = self.sessions.get_mut(&old_id) {
                        old.state = SessionState::Online;
                    }
                }
                seat.active_session = Some(id);
            }
        }
        if let Some(session) = self.sessions.get_mut(&id) {
            session.state = SessionState::Active;
            self.active_session = Some(id);
        }
    }

    /// Update activity timestamp (call on input events)
    pub fn touch_activity(&mut self) {
        if let Some(id) = self.active_session {
            if let Some(session) = self.sessions.get_mut(&id) {
                session.last_activity = crate::interrupts::get_ticks();
                session.idle_hint = false;
            }
        }
    }

    /// Check for idle sessions
    pub fn check_idle(&mut self) {
        let now = crate::interrupts::get_ticks();
        let timeout_ticks = self.idle_timeout_sec * 18; // ~18.2 Hz PIT

        for session in self.sessions.values_mut() {
            if session.state == SessionState::Active {
                let idle_time = now.wrapping_sub(session.last_activity);
                if idle_time > timeout_ticks {
                    session.idle_hint = true;
                }
            }
        }
    }

    /// Get active session
    pub fn get_active(&self) -> Option<&Session> {
        self.active_session.and_then(|id| self.sessions.get(&id))
    }

    /// List all sessions
    pub fn list_sessions(&self) -> Vec<&Session> {
        self.sessions.values().collect()
    }

    /// Set idle timeout
    pub fn set_idle_timeout(&mut self, seconds: u64) {
        self.idle_timeout_sec = seconds;
    }
}

lazy_static::lazy_static! {
    pub static ref SESSION_MGR: Mutex<SessionManager> = Mutex::new(SessionManager::new());
}

/// Create the initial graphical session for the default user
pub fn create_default_session() {
    let mut mgr = SESSION_MGR.lock();
    mgr.create_session(
        1000,
        "user",
        SessionType::KnoxGui,
        SessionClass::User,
        "seat0",
        1,
    );
}

/// Initialize session management
pub fn init() {
    create_default_session();
    serial_println!("[KnoxOS] Session management initialized");
}
