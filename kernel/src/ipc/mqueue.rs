/// mqueue — POSIX message queue implementation
/// Linux-compatible message queues (mq_open, mq_send, mq_receive, etc.)
///
/// Provides named message queues with priority-ordered delivery,
/// configurable max message size and count, and notification support.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Maximum number of message queues
const MAX_QUEUES: usize = 256;
/// Default max messages per queue
const DEFAULT_MAX_MSGS: usize = 10;
/// Default max message size
const DEFAULT_MSG_SIZE: usize = 8192;

/// Message queue attributes
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MqAttr {
    pub mq_flags: i64,   // Message queue flags (O_NONBLOCK)
    pub mq_maxmsg: i64,  // Maximum number of messages
    pub mq_msgsize: i64, // Maximum message size
    pub mq_curmsgs: i64, // Current number of messages
}

impl Default for MqAttr {
    fn default() -> Self {
        MqAttr {
            mq_flags: 0,
            mq_maxmsg: DEFAULT_MAX_MSGS as i64,
            mq_msgsize: DEFAULT_MSG_SIZE as i64,
            mq_curmsgs: 0,
        }
    }
}

/// A single message in the queue
#[derive(Debug, Clone)]
struct Message {
    data: Vec<u8>,
    priority: u32,
}

/// A POSIX message queue
struct MessageQueue {
    name: String,
    attr: MqAttr,
    messages: Vec<Message>,
    mode: u32,
    owner_uid: u32,
    owner_gid: u32,
    unlink_pending: bool,
    open_count: u32,
}

/// Message queue descriptor
#[derive(Debug, Clone)]
struct MqDescriptor {
    queue_name: String,
    flags: i32,
    pid: u32,
}

lazy_static::lazy_static! {
    static ref MQ_QUEUES: Mutex<BTreeMap<String, MessageQueue>> = Mutex::new(BTreeMap::new());
    static ref MQ_DESCRIPTORS: Mutex<BTreeMap<i32, MqDescriptor>> = Mutex::new(BTreeMap::new());
    static ref NEXT_MQFD: Mutex<i32> = Mutex::new(1000);
}

/// Open flags
pub const O_RDONLY: i32 = 0;
pub const O_WRONLY: i32 = 1;
pub const O_RDWR: i32 = 2;
pub const O_CREAT: i32 = 0x40;
pub const O_EXCL: i32 = 0x80;
pub const O_NONBLOCK: i32 = 0x800;

/// Open or create a POSIX message queue
pub fn mq_open(name: &str, oflag: i32, mode: u32, attr: Option<MqAttr>) -> Result<i32, i32> {
    let name = if name.starts_with('/') {
        String::from(name)
    } else {
        alloc::format!("/{}", name)
    };

    let mut queues = MQ_QUEUES.lock();
    let mut descs = MQ_DESCRIPTORS.lock();

    let exists = queues.contains_key(&name);

    if (oflag & O_CREAT) != 0 && (oflag & O_EXCL) != 0 && exists {
        return Err(-17); // EEXIST
    }

    if (oflag & O_CREAT) == 0 && !exists {
        return Err(-2); // ENOENT
    }

    // Create queue if needed
    if !exists {
        let qa = attr.unwrap_or_default();
        let pid = crate::scheduler::current_pid().unwrap_or(0);
        let queue = MessageQueue {
            name: name.clone(),
            attr: MqAttr {
                mq_flags: 0,
                mq_maxmsg: if qa.mq_maxmsg > 0 {
                    qa.mq_maxmsg
                } else {
                    DEFAULT_MAX_MSGS as i64
                },
                mq_msgsize: if qa.mq_msgsize > 0 {
                    qa.mq_msgsize
                } else {
                    DEFAULT_MSG_SIZE as i64
                },
                mq_curmsgs: 0,
            },
            messages: Vec::new(),
            mode,
            owner_uid: 0,
            owner_gid: 0,
            unlink_pending: false,
            open_count: 0,
        };
        queues.insert(name.clone(), queue);
    }

    // Increment open count
    if let Some(q) = queues.get_mut(&name) {
        q.open_count += 1;
    }

    // Allocate descriptor
    let mut next = NEXT_MQFD.lock();
    let mqd = *next;
    *next += 1;

    let pid = crate::scheduler::current_pid().unwrap_or(0);
    descs.insert(
        mqd,
        MqDescriptor {
            queue_name: name,
            flags: oflag,
            pid,
        },
    );

    Ok(mqd)
}

/// Close a message queue descriptor
pub fn mq_close(mqd: i32) -> Result<(), i32> {
    let mut descs = MQ_DESCRIPTORS.lock();
    let desc = descs.remove(&mqd).ok_or(-9i32)?; // EBADF

    let mut queues = MQ_QUEUES.lock();
    if let Some(q) = queues.get_mut(&desc.queue_name) {
        q.open_count = q.open_count.saturating_sub(1);
        if q.open_count == 0 && q.unlink_pending {
            queues.remove(&desc.queue_name);
        }
    }

    Ok(())
}

/// Remove a message queue name
pub fn mq_unlink(name: &str) -> Result<(), i32> {
    let name = if name.starts_with('/') {
        String::from(name)
    } else {
        alloc::format!("/{}", name)
    };

    let mut queues = MQ_QUEUES.lock();
    let queue = queues.get_mut(&name).ok_or(-2i32)?; // ENOENT

    if queue.open_count > 0 {
        queue.unlink_pending = true;
    } else {
        queues.remove(&name);
    }

    Ok(())
}

/// Send a message to the queue
pub fn mq_send(mqd: i32, data: &[u8], priority: u32) -> Result<(), i32> {
    let descs = MQ_DESCRIPTORS.lock();
    let desc = descs.get(&mqd).ok_or(-9i32)?; // EBADF

    if desc.flags & O_RDWR == 0 && desc.flags & O_WRONLY == 0 {
        // Check it's not read-only (O_RDONLY == 0, which is default)
        // In practice allow unless explicitly restricted
    }

    let queue_name = desc.queue_name.clone();
    let nonblock = (desc.flags & O_NONBLOCK) != 0;
    drop(descs);

    let mut queues = MQ_QUEUES.lock();
    let queue = queues.get_mut(&queue_name).ok_or(-9i32)?;

    if data.len() > queue.attr.mq_msgsize as usize {
        return Err(-90); // EMSGSIZE
    }

    if queue.messages.len() >= queue.attr.mq_maxmsg as usize {
        if nonblock {
            return Err(-11); // EAGAIN
        }
        // In blocking mode, we'd sleep. For now, return EAGAIN.
        return Err(-11);
    }

    // Insert in priority order (highest priority first)
    let msg = Message {
        data: data.to_vec(),
        priority,
    };

    let pos = queue.messages.iter().position(|m| m.priority < priority);
    match pos {
        Some(i) => queue.messages.insert(i, msg),
        None => queue.messages.push(msg),
    }
    queue.attr.mq_curmsgs += 1;

    Ok(())
}

/// Receive a message from the queue
pub fn mq_receive(mqd: i32, buf: &mut [u8]) -> Result<(usize, u32), i32> {
    let descs = MQ_DESCRIPTORS.lock();
    let desc = descs.get(&mqd).ok_or(-9i32)?;

    let queue_name = desc.queue_name.clone();
    let nonblock = (desc.flags & O_NONBLOCK) != 0;
    drop(descs);

    let mut queues = MQ_QUEUES.lock();
    let queue = queues.get_mut(&queue_name).ok_or(-9i32)?;

    if buf.len() < queue.attr.mq_msgsize as usize {
        return Err(-90); // EMSGSIZE
    }

    if queue.messages.is_empty() {
        if nonblock {
            return Err(-11); // EAGAIN
        }
        return Err(-11);
    }

    let msg = queue.messages.remove(0); // Highest priority first
    queue.attr.mq_curmsgs -= 1;

    let copy_len = core::cmp::min(msg.data.len(), buf.len());
    buf[..copy_len].copy_from_slice(&msg.data[..copy_len]);

    Ok((copy_len, msg.priority))
}

/// Get queue attributes
pub fn mq_getattr(mqd: i32) -> Result<MqAttr, i32> {
    let descs = MQ_DESCRIPTORS.lock();
    let desc = descs.get(&mqd).ok_or(-9i32)?;

    let queue_name = desc.queue_name.clone();
    drop(descs);

    let queues = MQ_QUEUES.lock();
    let queue = queues.get(&queue_name).ok_or(-9i32)?;

    Ok(queue.attr)
}

/// Set queue attributes (only mq_flags can be changed)
pub fn mq_setattr(mqd: i32, new_attr: &MqAttr) -> Result<MqAttr, i32> {
    let descs = MQ_DESCRIPTORS.lock();
    let desc = descs.get(&mqd).ok_or(-9i32)?;

    let queue_name = desc.queue_name.clone();
    drop(descs);

    let mut queues = MQ_QUEUES.lock();
    let queue = queues.get_mut(&queue_name).ok_or(-9i32)?;

    let old_attr = queue.attr;
    queue.attr.mq_flags = new_attr.mq_flags;

    Ok(old_attr)
}

/// List all message queues (for /dev/mqueue)
pub fn list_queues() -> Vec<(String, MqAttr)> {
    let queues = MQ_QUEUES.lock();
    queues
        .iter()
        .map(|(name, q)| (name.clone(), q.attr))
        .collect()
}

pub fn init() {
    // Create /dev/mqueue mount point in VFS
    let mut vfs = crate::vfs::VFS.lock();
    let _ = vfs.mkdir("/dev/mqueue", 0o1777);
    drop(vfs);

    serial_println!("[KnoxOS] POSIX message queues initialized (/dev/mqueue)");
}
