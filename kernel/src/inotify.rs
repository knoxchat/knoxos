/// inotify - Filesystem event notification
/// Compatible with Linux inotify(7) interface
/// Watches files/directories for changes
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// inotify event mask flags (Linux-compatible)
pub const IN_ACCESS: u32 = 0x00000001;
pub const IN_MODIFY: u32 = 0x00000002;
pub const IN_ATTRIB: u32 = 0x00000004;
pub const IN_CLOSE_WRITE: u32 = 0x00000008;
pub const IN_CLOSE_NOWRITE: u32 = 0x00000010;
pub const IN_OPEN: u32 = 0x00000020;
pub const IN_MOVED_FROM: u32 = 0x00000040;
pub const IN_MOVED_TO: u32 = 0x00000080;
pub const IN_CREATE: u32 = 0x00000100;
pub const IN_DELETE: u32 = 0x00000200;
pub const IN_DELETE_SELF: u32 = 0x00000400;
pub const IN_MOVE_SELF: u32 = 0x00000800;
pub const IN_ISDIR: u32 = 0x40000000;
pub const IN_ONESHOT: u32 = 0x80000000;

/// Combined masks
pub const IN_CLOSE: u32 = IN_CLOSE_WRITE | IN_CLOSE_NOWRITE;
pub const IN_MOVE: u32 = IN_MOVED_FROM | IN_MOVED_TO;
pub const IN_ALL_EVENTS: u32 = IN_ACCESS
    | IN_MODIFY
    | IN_ATTRIB
    | IN_CLOSE
    | IN_OPEN
    | IN_MOVE
    | IN_CREATE
    | IN_DELETE
    | IN_DELETE_SELF
    | IN_MOVE_SELF;

/// An inotify event (matches Linux struct inotify_event)
#[repr(C)]
#[derive(Debug, Clone)]
pub struct InotifyEvent {
    pub wd: i32,      // Watch descriptor
    pub mask: u32,    // Event mask
    pub cookie: u32,  // Cookie for rename tracking
    pub len: u32,     // Length of name
    pub name: String, // Filename (only for directory watches)
}

/// A watch entry
#[derive(Debug, Clone)]
struct WatchEntry {
    wd: i32,
    path: String,
    mask: u32,
    oneshot: bool,
}

/// An inotify instance
struct InotifyInstance {
    watches: BTreeMap<i32, WatchEntry>,
    events: Vec<InotifyEvent>,
    next_wd: i32,
}

impl InotifyInstance {
    fn new() -> Self {
        Self {
            watches: BTreeMap::new(),
            events: Vec::new(),
            next_wd: 1,
        }
    }

    fn add_watch(&mut self, path: &str, mask: u32) -> i32 {
        // Check if already watching this path
        for (wd, watch) in &self.watches {
            if watch.path == path {
                return *wd;
            }
        }

        let wd = self.next_wd;
        self.next_wd += 1;
        self.watches.insert(
            wd,
            WatchEntry {
                wd,
                path: String::from(path),
                mask: mask & !IN_ONESHOT,
                oneshot: mask & IN_ONESHOT != 0,
            },
        );
        wd
    }

    fn remove_watch(&mut self, wd: i32) -> Result<(), i32> {
        self.watches.remove(&wd).ok_or(-22)?; // EINVAL
        Ok(())
    }

    fn read_events(&mut self) -> Vec<InotifyEvent> {
        let events = self.events.clone();
        self.events.clear();

        // Remove oneshot watches that have fired
        let oneshot_wds: Vec<i32> = events
            .iter()
            .filter_map(|e| self.watches.get(&e.wd).filter(|w| w.oneshot).map(|_| e.wd))
            .collect();
        for wd in oneshot_wds {
            self.watches.remove(&wd);
        }

        events
    }
}

/// Global inotify instances
lazy_static::lazy_static! {
    static ref INOTIFY_INSTANCES: Mutex<BTreeMap<i32, InotifyInstance>> = Mutex::new(BTreeMap::new());
}

static NEXT_INOTIFY_FD: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(2000);

/// Create a new inotify instance
pub fn inotify_init() -> Result<i32, i32> {
    let fd = NEXT_INOTIFY_FD.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    INOTIFY_INSTANCES.lock().insert(fd, InotifyInstance::new());
    crate::serial_println!("[KnoxOS] inotify_init() = {}", fd);
    Ok(fd)
}

/// Add a watch to an inotify instance
pub fn inotify_add_watch(fd: i32, path: &str, mask: u32) -> Result<i32, i32> {
    let mut instances = INOTIFY_INSTANCES.lock();
    let instance = instances.get_mut(&fd).ok_or(-9i32)?; // EBADF
    Ok(instance.add_watch(path, mask))
}

/// Remove a watch from an inotify instance
pub fn inotify_rm_watch(fd: i32, wd: i32) -> Result<(), i32> {
    let mut instances = INOTIFY_INSTANCES.lock();
    let instance = instances.get_mut(&fd).ok_or(-9i32)?; // EBADF
    instance.remove_watch(wd)
}

/// Read events from an inotify instance
pub fn inotify_read(fd: i32) -> Result<Vec<InotifyEvent>, i32> {
    let mut instances = INOTIFY_INSTANCES.lock();
    let instance = instances.get_mut(&fd).ok_or(-9i32)?; // EBADF
    Ok(instance.read_events())
}

/// Close an inotify instance
pub fn inotify_close(fd: i32) {
    INOTIFY_INSTANCES.lock().remove(&fd);
}

/// Emit a filesystem event (called by VFS operations)
pub fn emit_event(path: &str, mask: u32, name: Option<&str>) {
    static COOKIE: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(1);

    let mut instances = INOTIFY_INSTANCES.lock();
    for instance in instances.values_mut() {
        let watches: Vec<(i32, String, u32)> = instance
            .watches
            .values()
            .map(|w| (w.wd, w.path.clone(), w.mask))
            .collect();
        for (wd, watch_path, watch_mask) in watches {
            if watch_mask & mask == 0 {
                continue;
            }
            if !path_matches_watch(&watch_path, path) {
                continue;
            }
            let event = InotifyEvent {
                wd,
                mask,
                cookie: if mask & IN_MOVE != 0 {
                    COOKIE.fetch_add(1, core::sync::atomic::Ordering::Relaxed)
                } else {
                    0
                },
                len: name.map_or(0, |n| n.len() as u32),
                name: name.map_or(String::new(), String::from),
            };
            instance.events.push(event);
        }
    }
}

fn parent_dir(path: &str) -> &str {
    let path = path.trim_end_matches('/');
    match path.rfind('/') {
        Some(0) => "/",
        Some(i) => &path[..i],
        None => "",
    }
}

fn path_matches_watch(watch_path: &str, event_path: &str) -> bool {
    watch_path == event_path || parent_dir(event_path) == watch_path
}

fn basename(path: &str) -> Option<&str> {
    path.rsplit('/').next().filter(|s| !s.is_empty())
}

/// Initialize inotify subsystem
pub fn init() {
    crate::serial_println!("[KnoxOS] inotify filesystem notification initialized");
}

/// Serial marker once a VFS write/unlink is visible to an inotify watch.
pub const GATE_H3_MARKER: &str = "GATE_H3 inotify";

const GATE_H3_DIR: &str = "/var/lib/knoxos";
const GATE_H3_PATH: &str = "/var/lib/knoxos/gate_h3";
const GATE_H3_PAYLOAD: &[u8] = b"knoxos-h3\n";

/// Watch a directory, write and unlink a file, and prove events were queued.
pub fn vfs_watch_self_test() -> bool {
    let Ok(fd) = inotify_init() else {
        crate::serial_println!("[inotify] Gate H3 FAILED: inotify_init");
        return false;
    };
    let Ok(_wd) = inotify_add_watch(fd, GATE_H3_DIR, IN_CREATE | IN_MODIFY | IN_DELETE) else {
        crate::serial_println!("[inotify] Gate H3 FAILED: add_watch");
        inotify_close(fd);
        return false;
    };

    if !crate::vfs::write_file_dispatch(GATE_H3_PATH, GATE_H3_PAYLOAD) {
        crate::serial_println!("[inotify] Gate H3 FAILED: write");
        inotify_close(fd);
        return false;
    }
    let created = inotify_read(fd).unwrap_or_default();
    let saw_create = created
        .iter()
        .any(|e| e.mask & (IN_CREATE | IN_MODIFY) != 0);
    if !saw_create {
        crate::serial_println!("[inotify] Gate H3 FAILED: no create/modify event");
        inotify_close(fd);
        return false;
    }

    if crate::vfs::remove_dispatch(GATE_H3_PATH).is_err() {
        crate::serial_println!("[inotify] Gate H3 FAILED: unlink");
        inotify_close(fd);
        return false;
    }
    let deleted = inotify_read(fd).unwrap_or_default();
    inotify_close(fd);
    if !deleted.iter().any(|e| e.mask & IN_DELETE != 0) {
        crate::serial_println!("[inotify] Gate H3 FAILED: no delete event");
        return false;
    }

    crate::serial_println!("[inotify] {}", GATE_H3_MARKER);
    true
}
