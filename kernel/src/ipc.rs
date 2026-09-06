/// Pipe & IPC - Inter-process communication primitives
/// Implements Linux-compatible pipes and message passing
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

/// Maximum pipe buffer size (Linux default is 65536)
const PIPE_BUF_SIZE: usize = 65536;

/// Pipe state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeState {
    Open,
    ReadClosed,
    WriteClosed,
    Closed,
}

/// A pipe - unidirectional byte stream
pub struct Pipe {
    pub id: u32,
    pub buffer: VecDeque<u8>,
    pub state: PipeState,
    pub readers: u32,
    pub writers: u32,
    pub bytes_written: u64,
    pub bytes_read: u64,
}

impl Pipe {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            buffer: VecDeque::with_capacity(4096),
            state: PipeState::Open,
            readers: 1,
            writers: 1,
            bytes_written: 0,
            bytes_read: 0,
        }
    }

    /// Write bytes into the pipe
    pub fn write(&mut self, data: &[u8]) -> Result<usize, i32> {
        if self.state == PipeState::ReadClosed || self.state == PipeState::Closed {
            return Err(-32); // EPIPE
        }

        let available = PIPE_BUF_SIZE - self.buffer.len();
        let to_write = data.len().min(available);

        if to_write == 0 {
            return Err(-11); // EAGAIN (pipe full)
        }

        for &byte in &data[..to_write] {
            self.buffer.push_back(byte);
        }
        self.bytes_written += to_write as u64;
        Ok(to_write)
    }

    /// Read bytes from the pipe
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, i32> {
        if self.buffer.is_empty() {
            if self.state == PipeState::WriteClosed || self.state == PipeState::Closed {
                return Ok(0); // EOF
            }
            return Err(-11); // EAGAIN (no data)
        }

        let to_read = buf.len().min(self.buffer.len());
        for item in buf.iter_mut().take(to_read) {
            *item = self.buffer.pop_front().unwrap();
        }
        self.bytes_read += to_read as u64;
        Ok(to_read)
    }

    /// Close the read end
    pub fn close_read(&mut self) {
        self.readers = self.readers.saturating_sub(1);
        if self.readers == 0 {
            match self.state {
                PipeState::WriteClosed => self.state = PipeState::Closed,
                _ => self.state = PipeState::ReadClosed,
            }
        }
    }

    /// Close the write end
    pub fn close_write(&mut self) {
        self.writers = self.writers.saturating_sub(1);
        if self.writers == 0 {
            match self.state {
                PipeState::ReadClosed => self.state = PipeState::Closed,
                _ => self.state = PipeState::WriteClosed,
            }
        }
    }

    /// Check if pipe is fully closed
    pub fn is_closed(&self) -> bool {
        self.state == PipeState::Closed
    }

    /// Available bytes to read
    pub fn available(&self) -> usize {
        self.buffer.len()
    }

    /// Space available for writing
    pub fn space(&self) -> usize {
        PIPE_BUF_SIZE - self.buffer.len()
    }
}

/// Global pipe table
static NEXT_PIPE_ID: AtomicU32 = AtomicU32::new(1);

lazy_static::lazy_static! {
    pub static ref PIPES: Mutex<Vec<Pipe>> = Mutex::new(Vec::new());
}

/// Create a new pipe, returns (read_pipe_id, write_pipe_id)
/// Both share the same pipe, but are distinguished by the fd layer
pub fn create_pipe() -> Result<u32, i32> {
    let id = NEXT_PIPE_ID.fetch_add(1, Ordering::Relaxed);
    let pipe = Pipe::new(id);
    PIPES.lock().push(pipe);
    serial_println!("[KnoxOS] Created pipe {}", id);
    Ok(id)
}

/// Write to a pipe by ID
pub fn pipe_write(id: u32, data: &[u8]) -> Result<usize, i32> {
    let mut pipes = PIPES.lock();
    let pipe = pipes.iter_mut().find(|p| p.id == id).ok_or(-9i32)?; // EBADF
    pipe.write(data)
}

/// Read from a pipe by ID
pub fn pipe_read(id: u32, buf: &mut [u8]) -> Result<usize, i32> {
    let mut pipes = PIPES.lock();
    let pipe = pipes.iter_mut().find(|p| p.id == id).ok_or(-9i32)?; // EBADF
    pipe.read(buf)
}

/// Close a pipe end
pub fn pipe_close(id: u32, is_read_end: bool) {
    let mut pipes = PIPES.lock();
    if let Some(pipe) = pipes.iter_mut().find(|p| p.id == id) {
        if is_read_end {
            pipe.close_read();
        } else {
            pipe.close_write();
        }
    }
    // Garbage collect fully closed pipes
    pipes.retain(|p| !p.is_closed());
}

// ─── Message Queue IPC ────────────────────────────────────────────────

/// Maximum message size
const MAX_MSG_SIZE: usize = 8192;
/// Maximum messages in a queue
const MAX_QUEUE_SIZE: usize = 64;

/// IPC message
#[derive(Debug, Clone)]
pub struct IpcMessage {
    pub sender_pid: u32,
    pub msg_type: u32,
    pub data: Vec<u8>,
    pub timestamp: u64,
}

/// Message queue
pub struct MessageQueue {
    pub id: u32,
    pub name: String,
    pub messages: VecDeque<IpcMessage>,
    pub max_size: usize,
    pub owner_pid: u32,
}

impl MessageQueue {
    pub fn new(id: u32, name: &str, owner_pid: u32) -> Self {
        Self {
            id,
            name: String::from(name),
            messages: VecDeque::new(),
            max_size: MAX_QUEUE_SIZE,
            owner_pid,
        }
    }

    /// Send a message to the queue
    pub fn send(&mut self, msg: IpcMessage) -> Result<(), i32> {
        if self.messages.len() >= self.max_size {
            return Err(-11); // EAGAIN
        }
        if msg.data.len() > MAX_MSG_SIZE {
            return Err(-90); // EMSGSIZE
        }
        self.messages.push_back(msg);
        Ok(())
    }

    /// Receive the next message
    pub fn receive(&mut self) -> Option<IpcMessage> {
        self.messages.pop_front()
    }

    /// Receive a message of a specific type
    pub fn receive_type(&mut self, msg_type: u32) -> Option<IpcMessage> {
        let pos = self.messages.iter().position(|m| m.msg_type == msg_type)?;
        Some(self.messages.remove(pos).unwrap())
    }

    /// Number of pending messages
    pub fn pending_count(&self) -> usize {
        self.messages.len()
    }
}

/// Global message queue registry
lazy_static::lazy_static! {
    pub static ref MESSAGE_QUEUES: Mutex<Vec<MessageQueue>> = Mutex::new(Vec::new());
}

static NEXT_MQ_ID: AtomicU32 = AtomicU32::new(1);

/// Create a named message queue
pub fn mq_create(name: &str, owner_pid: u32) -> Result<u32, i32> {
    let mut queues = MESSAGE_QUEUES.lock();
    // Check if already exists
    if queues.iter().any(|q| q.name == name) {
        return Err(-17); // EEXIST
    }
    let id = NEXT_MQ_ID.fetch_add(1, Ordering::Relaxed);
    queues.push(MessageQueue::new(id, name, owner_pid));
    serial_println!("[KnoxOS] Created message queue '{}' (id={})", name, id);
    Ok(id)
}

/// Open an existing message queue by name
pub fn mq_open(name: &str) -> Result<u32, i32> {
    let queues = MESSAGE_QUEUES.lock();
    queues
        .iter()
        .find(|q| q.name == name)
        .map(|q| q.id)
        .ok_or(-2) // ENOENT
}

/// Send a message to a queue
pub fn mq_send(queue_id: u32, sender_pid: u32, msg_type: u32, data: &[u8]) -> Result<(), i32> {
    let mut queues = MESSAGE_QUEUES.lock();
    let queue = queues.iter_mut().find(|q| q.id == queue_id).ok_or(-9i32)?;
    queue.send(IpcMessage {
        sender_pid,
        msg_type,
        data: Vec::from(data),
        timestamp: 0,
    })
}

/// Receive a message from a queue
pub fn mq_receive(queue_id: u32) -> Result<IpcMessage, i32> {
    let mut queues = MESSAGE_QUEUES.lock();
    let queue = queues.iter_mut().find(|q| q.id == queue_id).ok_or(-9i32)?;
    queue.receive().ok_or(-11) // EAGAIN
}

/// Initialize IPC subsystem
pub fn init() {
    serial_println!("[KnoxOS] IPC initialized (pipes + message queues)");
}
