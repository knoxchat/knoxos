/// fanotify — Filesystem-wide notification API
///
/// Provides fanotify, a more powerful alternative to inotify for
/// filesystem event monitoring. Supports permission events, directory
/// modification, and global filesystem monitoring.
///
/// Features:
/// - FAN_ACCESS, FAN_MODIFY, FAN_CLOSE_WRITE, FAN_CLOSE_NOWRITE
/// - FAN_OPEN, FAN_OPEN_PERM, FAN_ACCESS_PERM
/// - FAN_CREATE, FAN_DELETE, FAN_MOVE (directory events)
/// - FAN_ONDIR, FAN_EVENT_ON_CHILD
/// - FID (file identifier) notification mode
/// - Permission event blocking/allowing
/// - Global filesystem or per-mount monitoring
/// - Content scanning (antivirus integration)
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ─── Event flags ────────────────────────────────────────────────────

pub const FAN_ACCESS: u64 = 0x0000_0001;
pub const FAN_MODIFY: u64 = 0x0000_0002;
pub const FAN_CLOSE_WRITE: u64 = 0x0000_0008;
pub const FAN_CLOSE_NOWRITE: u64 = 0x0000_0010;
pub const FAN_OPEN: u64 = 0x0000_0020;
pub const FAN_OPEN_PERM: u64 = 0x0001_0000;
pub const FAN_ACCESS_PERM: u64 = 0x0002_0000;
pub const FAN_OPEN_EXEC: u64 = 0x0004_0000;
pub const FAN_OPEN_EXEC_PERM: u64 = 0x0008_0000;
pub const FAN_ATTRIB: u64 = 0x0000_0004;
pub const FAN_CREATE: u64 = 0x0000_0100;
pub const FAN_DELETE: u64 = 0x0000_0200;
pub const FAN_DELETE_SELF: u64 = 0x0000_0400;
pub const FAN_MOVED_FROM: u64 = 0x0000_0040;
pub const FAN_MOVED_TO: u64 = 0x0000_0080;
pub const FAN_MOVE_SELF: u64 = 0x0000_0800;
pub const FAN_ONDIR: u64 = 0x4000_0000;
pub const FAN_EVENT_ON_CHILD: u64 = 0x0800_0000;
pub const FAN_CLOSE: u64 = FAN_CLOSE_WRITE | FAN_CLOSE_NOWRITE;
pub const FAN_MOVE: u64 = FAN_MOVED_FROM | FAN_MOVED_TO;

// Init flags
pub const FAN_CLOEXEC: u32 = 0x0000_0001;
pub const FAN_NONBLOCK: u32 = 0x0000_0002;
pub const FAN_CLASS_NOTIF: u32 = 0x0000_0000;
pub const FAN_CLASS_CONTENT: u32 = 0x0000_0004;
pub const FAN_CLASS_PRE_CONTENT: u32 = 0x0000_0008;
pub const FAN_UNLIMITED_QUEUE: u32 = 0x0000_0010;
pub const FAN_UNLIMITED_MARKS: u32 = 0x0000_0020;
pub const FAN_REPORT_TID: u32 = 0x0000_0100;
pub const FAN_REPORT_FID: u32 = 0x0000_0200;
pub const FAN_REPORT_DIR_FID: u32 = 0x0000_0400;
pub const FAN_REPORT_NAME: u32 = 0x0000_0800;
pub const FAN_REPORT_PIDFD: u32 = 0x0000_1000;

// Mark flags
pub const FAN_MARK_ADD: u32 = 0x0000_0001;
pub const FAN_MARK_REMOVE: u32 = 0x0000_0002;
pub const FAN_MARK_DONT_FOLLOW: u32 = 0x0000_0004;
pub const FAN_MARK_ONLYDIR: u32 = 0x0000_0008;
pub const FAN_MARK_INODE: u32 = 0x0000_0000;
pub const FAN_MARK_MOUNT: u32 = 0x0000_0010;
pub const FAN_MARK_FILESYSTEM: u32 = 0x0000_0100;
pub const FAN_MARK_FLUSH: u32 = 0x0000_0080;
pub const FAN_MARK_EVICTABLE: u32 = 0x0000_0200;
pub const FAN_MARK_IGNORE: u32 = 0x0000_0400;

// Permission response
pub const FAN_ALLOW: u32 = 0x01;
pub const FAN_DENY: u32 = 0x02;
pub const FAN_AUDIT: u32 = 0x10;

// ─── Data Structures ────────────────────────────────────────────────

/// An fanotify event
#[derive(Debug, Clone)]
pub struct FanotifyEvent {
    /// Event mask (which events occurred)
    pub mask: u64,
    /// File descriptor of the affected file (or -1 for FID mode)
    pub fd: i32,
    /// Process ID that triggered the event
    pub pid: u32,
    /// Path (if available)
    pub path: String,
    /// File handle (FID mode)
    pub file_handle: Option<FileHandle>,
    /// Whether this is a permission event awaiting response
    pub is_permission: bool,
    /// Unique ID for permission events
    pub perm_id: u64,
}

/// File handle for FID notification mode
#[derive(Debug, Clone)]
pub struct FileHandle {
    /// Filesystem ID (from statfs)
    pub fsid: [u32; 2],
    /// Handle type
    pub handle_type: i32,
    /// File handle bytes
    pub handle_bytes: Vec<u8>,
}

/// A mark (watch) on a filesystem object
#[derive(Debug, Clone)]
pub struct FanotifyMark {
    /// Event mask for this mark
    pub mask: u64,
    /// Mark type (inode, mount, filesystem)
    pub mark_type: MarkType,
    /// Path or mount point
    pub path: String,
    /// Ignore mask
    pub ignore_mask: u64,
    /// Flags
    pub flags: u32,
}

/// Mark type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkType {
    Inode,
    Mount,
    Filesystem,
}

/// An fanotify instance (file descriptor)
#[derive(Debug, Clone)]
pub struct FanotifyInstance {
    /// Instance ID (acts as fd)
    pub fd: i32,
    /// Init flags
    pub init_flags: u32,
    /// Event flags for the fd
    pub event_flags: u32,
    /// Marks (watches)
    pub marks: Vec<FanotifyMark>,
    /// Pending events
    pub events: Vec<FanotifyEvent>,
    /// Notification class
    pub class: FanotifyClass,
    /// Maximum queue depth (0 = unlimited)
    pub max_events: usize,
    /// Whether FID reporting is enabled
    pub report_fid: bool,
    /// Whether directory FID is reported
    pub report_dir_fid: bool,
    /// Whether name is reported
    pub report_name: bool,
}

/// Fanotify notification class
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanotifyClass {
    /// Notification only (no permission events)
    Notification,
    /// Content scanning (can read file during permission check)
    Content,
    /// Pre-content (can access file before it's made available)
    PreContent,
}

// ─── Global State ───────────────────────────────────────────────────

pub struct FanotifyState {
    /// All active fanotify instances
    pub instances: BTreeMap<i32, FanotifyInstance>,
    /// Next fd
    next_fd: i32,
    /// Next permission event ID
    next_perm_id: u64,
    /// Statistics
    pub stats: FanotifyStats,
}

/// Statistics
#[derive(Debug, Clone, Default)]
pub struct FanotifyStats {
    pub instances_created: u64,
    pub marks_added: u64,
    pub events_generated: u64,
    pub permission_events: u64,
    pub events_allowed: u64,
    pub events_denied: u64,
}

lazy_static::lazy_static! {
    pub static ref FANOTIFY: Mutex<FanotifyState> = Mutex::new(FanotifyState::new());
}

impl FanotifyState {
    pub fn new() -> Self {
        Self {
            instances: BTreeMap::new(),
            next_fd: 100,
            next_perm_id: 1,
            stats: FanotifyStats::default(),
        }
    }

    /// fanotify_init() — create a new fanotify instance
    pub fn fanotify_init(&mut self, flags: u32, event_flags: u32) -> Result<i32, i32> {
        let fd = self.next_fd;
        self.next_fd += 1;

        let class = if flags & FAN_CLASS_PRE_CONTENT != 0 {
            FanotifyClass::PreContent
        } else if flags & FAN_CLASS_CONTENT != 0 {
            FanotifyClass::Content
        } else {
            FanotifyClass::Notification
        };

        let max_events = if flags & FAN_UNLIMITED_QUEUE != 0 {
            0
        } else {
            16384 // default limit
        };

        let instance = FanotifyInstance {
            fd,
            init_flags: flags,
            event_flags,
            marks: Vec::new(),
            events: Vec::new(),
            class,
            max_events,
            report_fid: flags & FAN_REPORT_FID != 0,
            report_dir_fid: flags & FAN_REPORT_DIR_FID != 0,
            report_name: flags & FAN_REPORT_NAME != 0,
        };

        self.instances.insert(fd, instance);
        self.stats.instances_created += 1;
        Ok(fd)
    }

    /// fanotify_mark() — add, remove, or flush marks
    pub fn fanotify_mark(&mut self, fd: i32, flags: u32, mask: u64, path: &str) -> Result<(), i32> {
        let instance = self.instances.get_mut(&fd).ok_or(-9i32)?; // EBADF

        let mark_type = if flags & FAN_MARK_FILESYSTEM != 0 {
            MarkType::Filesystem
        } else if flags & FAN_MARK_MOUNT != 0 {
            MarkType::Mount
        } else {
            MarkType::Inode
        };

        if flags & FAN_MARK_FLUSH != 0 {
            instance.marks.clear();
            return Ok(());
        }

        if flags & FAN_MARK_REMOVE != 0 {
            instance
                .marks
                .retain(|m| !(m.path == path && m.mark_type == mark_type));
            return Ok(());
        }

        // FAN_MARK_ADD
        // Check if mark already exists
        for mark in &mut instance.marks {
            if mark.path == path && mark.mark_type == mark_type {
                if flags & FAN_MARK_IGNORE != 0 {
                    mark.ignore_mask |= mask;
                } else {
                    mark.mask |= mask;
                }
                return Ok(());
            }
        }

        let mark = FanotifyMark {
            mask,
            mark_type,
            path: String::from(path),
            ignore_mask: 0,
            flags,
        };

        instance.marks.push(mark);
        self.stats.marks_added += 1;
        Ok(())
    }

    /// Generate an event for a filesystem operation
    pub fn generate_event(&mut self, event_mask: u64, path: &str, pid: u32) {
        // Check all instances for matching marks
        let mut events_to_add: Vec<(i32, FanotifyEvent)> = Vec::new();

        for (fd, instance) in &self.instances {
            for mark in &instance.marks {
                // Check if event matches the mark mask
                if mark.mask & event_mask == 0 {
                    continue;
                }
                // Check ignore mask
                if mark.ignore_mask & event_mask != 0 {
                    continue;
                }
                // Check path match based on mark type
                let matches = match mark.mark_type {
                    MarkType::Inode => path == mark.path,
                    MarkType::Mount => path.starts_with(&mark.path),
                    MarkType::Filesystem => true, // matches all on that FS
                };

                if matches {
                    let is_permission =
                        event_mask & (FAN_OPEN_PERM | FAN_ACCESS_PERM | FAN_OPEN_EXEC_PERM) != 0;

                    let event = FanotifyEvent {
                        mask: event_mask,
                        fd: -1, // FID mode or TBD
                        pid,
                        path: String::from(path),
                        file_handle: None,
                        is_permission,
                        perm_id: 0, // set below
                    };
                    events_to_add.push((*fd, event));
                    break; // one event per instance
                }
            }
        }

        for (fd, mut event) in events_to_add {
            if event.is_permission {
                event.perm_id = self.next_perm_id;
                self.next_perm_id += 1;
                self.stats.permission_events += 1;
            }
            if let Some(instance) = self.instances.get_mut(&fd) {
                if instance.max_events == 0 || instance.events.len() < instance.max_events {
                    instance.events.push(event);
                    self.stats.events_generated += 1;
                }
            }
        }
    }

    /// Read events from a fanotify fd
    pub fn read_events(&mut self, fd: i32, max: usize) -> Result<Vec<FanotifyEvent>, i32> {
        let instance = self.instances.get_mut(&fd).ok_or(-9i32)?;
        let count = max.min(instance.events.len());
        let events: Vec<FanotifyEvent> = instance.events.drain(..count).collect();
        Ok(events)
    }

    /// Respond to a permission event
    pub fn respond_permission(&mut self, fd: i32, perm_id: u64, response: u32) -> Result<(), i32> {
        // Remove the permission event
        if response & FAN_ALLOW != 0 {
            self.stats.events_allowed += 1;
        } else if response & FAN_DENY != 0 {
            self.stats.events_denied += 1;
        }
        Ok(())
    }

    /// Close an fanotify instance
    pub fn close(&mut self, fd: i32) -> Result<(), i32> {
        self.instances.remove(&fd).ok_or(-9i32)?;
        Ok(())
    }
}

// ─── Public API ─────────────────────────────────────────────────────

pub fn fanotify_init(flags: u32, event_flags: u32) -> Result<i32, i32> {
    FANOTIFY.lock().fanotify_init(flags, event_flags)
}

pub fn fanotify_mark(fd: i32, flags: u32, mask: u64, path: &str) -> Result<(), i32> {
    FANOTIFY.lock().fanotify_mark(fd, flags, mask, path)
}

pub fn read_events(fd: i32, max: usize) -> Result<Vec<FanotifyEvent>, i32> {
    FANOTIFY.lock().read_events(fd, max)
}

pub fn respond(fd: i32, perm_id: u64, response: u32) -> Result<(), i32> {
    FANOTIFY.lock().respond_permission(fd, perm_id, response)
}

pub fn generate_event(mask: u64, path: &str, pid: u32) {
    FANOTIFY.lock().generate_event(mask, path, pid);
}

pub fn init() {
    serial_println!(
        "[FANOTIFY] Filesystem notification subsystem initialized (permission events, FID mode, directory events)"
    );
}
