use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use super::types::NfsFileHandle;

// ═══════════════════════════════════════════════════════════════════════
// RPC / XDR
// ═══════════════════════════════════════════════════════════════════════

/// RPC message type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcMsgType {
    Call = 0,
    Reply = 1,
}

/// RPC authentication flavor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFlavor {
    None = 0,
    Sys = 1, // AUTH_SYS (Unix)
    Gss = 6, // RPCSEC_GSS
}

/// AUTH_SYS credentials
#[derive(Debug, Clone)]
pub struct AuthSys {
    pub stamp: u32,
    pub machinename: String,
    pub uid: u32,
    pub gid: u32,
    pub gids: Vec<u32>,
}

impl AuthSys {
    pub fn root() -> Self {
        Self {
            stamp: 0,
            machinename: String::from("knoxos"),
            uid: 0,
            gid: 0,
            gids: vec![0],
        }
    }
}

/// XDR encoder
pub struct XdrEncoder {
    buf: Vec<u8>,
}

impl XdrEncoder {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn encode_u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn encode_u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn encode_i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn encode_bool(&mut self, v: bool) {
        self.encode_u32(if v { 1 } else { 0 });
    }

    pub fn encode_opaque(&mut self, data: &[u8]) {
        self.encode_u32(data.len() as u32);
        self.buf.extend_from_slice(data);
        // Pad to 4-byte boundary
        let pad = (4 - (data.len() % 4)) % 4;
        for _ in 0..pad {
            self.buf.push(0);
        }
    }

    pub fn encode_string(&mut self, s: &str) {
        self.encode_opaque(s.as_bytes());
    }

    pub fn encode_fh(&mut self, fh: &NfsFileHandle) {
        self.encode_opaque(&fh.0);
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

/// XDR decoder
pub struct XdrDecoder<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> XdrDecoder<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    pub fn decode_u32(&mut self) -> Option<u32> {
        if self.offset + 4 > self.data.len() {
            return None;
        }
        let v = u32::from_be_bytes([
            self.data[self.offset],
            self.data[self.offset + 1],
            self.data[self.offset + 2],
            self.data[self.offset + 3],
        ]);
        self.offset += 4;
        Some(v)
    }

    pub fn decode_u64(&mut self) -> Option<u64> {
        let hi = self.decode_u32()? as u64;
        let lo = self.decode_u32()? as u64;
        Some((hi << 32) | lo)
    }

    pub fn decode_opaque(&mut self) -> Option<Vec<u8>> {
        let len = self.decode_u32()? as usize;
        if self.offset + len > self.data.len() {
            return None;
        }
        let data = self.data[self.offset..self.offset + len].to_vec();
        self.offset += len;
        let pad = (4 - (len % 4)) % 4;
        self.offset += pad;
        Some(data)
    }

    pub fn decode_string(&mut self) -> Option<String> {
        let bytes = self.decode_opaque()?;
        String::from_utf8(bytes).ok()
    }

    pub fn decode_fh(&mut self) -> Option<NfsFileHandle> {
        let data = self.decode_opaque()?;
        Some(NfsFileHandle::new(data))
    }
}
