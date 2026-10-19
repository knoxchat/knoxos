use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

static NEXT_SERIAL: AtomicU32 = AtomicU32::new(1);

// ═══════════════════════════════════════════════════════════════════════
// D-BUS WIRE PROTOCOL
// ═══════════════════════════════════════════════════════════════════════

/// D-Bus message endianness
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endianness {
    Little, // 'l'
    Big,    // 'B'
}

/// D-Bus message type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    Invalid = 0,
    MethodCall = 1,
    MethodReturn = 2,
    Error = 3,
    Signal = 4,
}

impl MessageType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::MethodCall,
            2 => Self::MethodReturn,
            3 => Self::Error,
            4 => Self::Signal,
            _ => Self::Invalid,
        }
    }
}

/// D-Bus message flags
pub const NO_REPLY_EXPECTED: u8 = 0x01;
pub const NO_AUTO_START: u8 = 0x02;
pub const ALLOW_INTERACTIVE_AUTH: u8 = 0x04;

/// D-Bus header field codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderField {
    Invalid = 0,
    Path = 1,
    Interface = 2,
    Member = 3,
    ErrorName = 4,
    ReplySerial = 5,
    Destination = 6,
    Sender = 7,
    Signature = 8,
    UnixFds = 9,
}

impl HeaderField {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Path,
            2 => Self::Interface,
            3 => Self::Member,
            4 => Self::ErrorName,
            5 => Self::ReplySerial,
            6 => Self::Destination,
            7 => Self::Sender,
            8 => Self::Signature,
            9 => Self::UnixFds,
            _ => Self::Invalid,
        }
    }
}

/// D-Bus type signature characters
pub const DBUS_TYPE_BYTE: char = 'y';
pub const DBUS_TYPE_BOOLEAN: char = 'b';
pub const DBUS_TYPE_INT16: char = 'n';
pub const DBUS_TYPE_UINT16: char = 'q';
pub const DBUS_TYPE_INT32: char = 'i';
pub const DBUS_TYPE_UINT32: char = 'u';
pub const DBUS_TYPE_INT64: char = 'x';
pub const DBUS_TYPE_UINT64: char = 't';
pub const DBUS_TYPE_DOUBLE: char = 'd';
pub const DBUS_TYPE_STRING: char = 's';
pub const DBUS_TYPE_OBJECT_PATH: char = 'o';
pub const DBUS_TYPE_SIGNATURE: char = 'g';
pub const DBUS_TYPE_ARRAY: char = 'a';
pub const DBUS_TYPE_VARIANT: char = 'v';
pub const DBUS_TYPE_STRUCT_BEGIN: char = '(';
pub const DBUS_TYPE_STRUCT_END: char = ')';
pub const DBUS_TYPE_DICT_ENTRY_BEGIN: char = '{';
pub const DBUS_TYPE_DICT_ENTRY_END: char = '}';

/// D-Bus message value
#[derive(Debug, Clone)]
pub enum DbusValue {
    Byte(u8),
    Boolean(bool),
    Int16(i16),
    Uint16(u16),
    Int32(i32),
    Uint32(u32),
    Int64(i64),
    Uint64(u64),
    Double(u64), // stored as bits since no_std
    String(String),
    ObjectPath(String),
    Signature(String),
    Array(Vec<DbusValue>),
    Variant(alloc::boxed::Box<DbusValue>),
    Struct(Vec<DbusValue>),
    DictEntry(alloc::boxed::Box<DbusValue>, alloc::boxed::Box<DbusValue>),
}

/// D-Bus message header
#[derive(Debug, Clone)]
pub struct MessageHeader {
    pub endianness: Endianness,
    pub message_type: MessageType,
    pub flags: u8,
    pub protocol_version: u8,
    pub body_length: u32,
    pub serial: u32,
    pub fields: Vec<(HeaderField, DbusValue)>,
}

/// Complete D-Bus message
#[derive(Debug, Clone)]
pub struct Message {
    pub header: MessageHeader,
    pub body: Vec<DbusValue>,
    // Convenience accessors from header fields
    pub path: Option<String>,
    pub interface: Option<String>,
    pub member: Option<String>,
    pub error_name: Option<String>,
    pub reply_serial: Option<u32>,
    pub destination: Option<String>,
    pub sender: Option<String>,
    pub signature: Option<String>,
}

impl Message {
    /// Create a new method call message
    pub fn method_call(destination: &str, path: &str, interface: &str, member: &str) -> Self {
        let serial = NEXT_SERIAL.fetch_add(1, Ordering::Relaxed);
        Self {
            header: MessageHeader {
                endianness: Endianness::Little,
                message_type: MessageType::MethodCall,
                flags: 0,
                protocol_version: 1,
                body_length: 0,
                serial,
                fields: Vec::new(),
            },
            body: Vec::new(),
            path: Some(path.to_string()),
            interface: Some(interface.to_string()),
            member: Some(member.to_string()),
            error_name: None,
            reply_serial: None,
            destination: Some(destination.to_string()),
            sender: None,
            signature: None,
        }
    }

    /// Create a method return message
    pub fn method_return(reply_to: u32) -> Self {
        let serial = NEXT_SERIAL.fetch_add(1, Ordering::Relaxed);
        Self {
            header: MessageHeader {
                endianness: Endianness::Little,
                message_type: MessageType::MethodReturn,
                flags: 0,
                protocol_version: 1,
                body_length: 0,
                serial,
                fields: Vec::new(),
            },
            body: Vec::new(),
            path: None,
            interface: None,
            member: None,
            error_name: None,
            reply_serial: Some(reply_to),
            destination: None,
            sender: None,
            signature: None,
        }
    }

    /// Create a signal message
    pub fn signal(path: &str, interface: &str, member: &str) -> Self {
        let serial = NEXT_SERIAL.fetch_add(1, Ordering::Relaxed);
        Self {
            header: MessageHeader {
                endianness: Endianness::Little,
                message_type: MessageType::Signal,
                flags: NO_REPLY_EXPECTED,
                protocol_version: 1,
                body_length: 0,
                serial,
                fields: Vec::new(),
            },
            body: Vec::new(),
            path: Some(path.to_string()),
            interface: Some(interface.to_string()),
            member: Some(member.to_string()),
            error_name: None,
            reply_serial: None,
            destination: None,
            sender: None,
            signature: None,
        }
    }

    /// Create an error reply message
    pub fn error(reply_to: u32, error_name: &str, message: &str) -> Self {
        let serial = NEXT_SERIAL.fetch_add(1, Ordering::Relaxed);
        Self {
            header: MessageHeader {
                endianness: Endianness::Little,
                message_type: MessageType::Error,
                flags: 0,
                protocol_version: 1,
                body_length: 0,
                serial,
                fields: Vec::new(),
            },
            body: vec![DbusValue::String(message.to_string())],
            path: None,
            interface: None,
            member: None,
            error_name: Some(error_name.to_string()),
            reply_serial: Some(reply_to),
            destination: None,
            sender: None,
            signature: Some(String::from("s")),
        }
    }
}
