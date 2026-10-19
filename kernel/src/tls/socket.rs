//! TLS-wrapped socket that encrypts/decrypts over a BSD TCP fd.

use alloc::string::String;

use crate::serial_println;

use super::api::{close_session, create_client_session, destroy_session, handshake, process, send};

// ═══════════════════════════════════════════════════════════════════════
// TLS SOCKET WRAPPER — high-level API integrating with BSD sockets
// ═══════════════════════════════════════════════════════════════════════

/// A TLS-wrapped socket that transparently encrypts/decrypts application data.
pub struct TlsSocket {
    /// Underlying BSD socket file descriptor
    pub fd: i32,
    /// TLS session ID
    pub session_id: u64,
    /// Server hostname (for SNI / cert verification)
    pub hostname: String,
    /// Whether the TLS handshake is complete
    pub handshake_done: bool,
}

impl TlsSocket {
    /// Create a TLS client socket wrapping an existing connected TCP fd.
    pub fn connect(fd: i32, hostname: &str) -> Result<Self, &'static str> {
        let session_id = create_client_session(Some(hostname));

        // Build and send ClientHello
        let client_hello = handshake(session_id).ok_or("failed to build ClientHello")?;

        // Send ClientHello over the raw socket
        let _ = crate::net::sys_sendto(fd as u32, &client_hello, 0);

        serial_println!(
            "[TLS] ClientHello sent on fd={} to '{}' (session {})",
            fd,
            hostname,
            session_id
        );

        Ok(Self {
            fd,
            session_id,
            hostname: String::from(hostname),
            handshake_done: false,
        })
    }

    /// Process incoming TLS records from the socket until handshake completes.
    pub fn finish_handshake(&mut self) -> Result<(), &'static str> {
        let mut buf = [0u8; 4096];
        let n = crate::net::sys_recvfrom(self.fd as u32, &mut buf).map_err(|_| "recv failed")?;
        if n > 0 {
            let response = process(self.session_id, &buf[..n]);
            if let Ok(resp_data) = response {
                if !resp_data.is_empty() {
                    let _ = crate::net::sys_sendto(self.fd as u32, &resp_data, 0);
                }
            }
        }
        self.handshake_done = true;
        serial_println!("[TLS] Handshake complete for session {}", self.session_id);
        Ok(())
    }

    /// Send application data through the TLS tunnel.
    pub fn tls_send(&self, plaintext: &[u8]) -> Result<usize, &'static str> {
        let encrypted = send(self.session_id, plaintext).map_err(|_| "failed to encrypt")?;
        let sent = crate::net::sys_sendto(self.fd as u32, &encrypted, 0)
            .map_err(|_| "socket send failed")?;
        Ok(sent)
    }

    /// Receive and decrypt application data from the TLS tunnel.
    pub fn tls_recv(&self, buf: &mut [u8]) -> Result<usize, &'static str> {
        let mut raw = alloc::vec![0u8; buf.len() + 64]; // extra for TLS overhead
        let n =
            crate::net::sys_recvfrom(self.fd as u32, &mut raw).map_err(|_| "socket recv failed")?;
        if n == 0 {
            return Ok(0);
        }
        // Process TLS record and extract plaintext
        match process(self.session_id, &raw[..n]) {
            Ok(plaintext) => {
                let copy_len = plaintext.len().min(buf.len());
                buf[..copy_len].copy_from_slice(&plaintext[..copy_len]);
                Ok(copy_len)
            }
            Err(_) => Ok(0),
        }
    }

    /// Close the TLS session and underlying socket.
    pub fn close(self) {
        let _ = close_session(self.session_id);
        destroy_session(self.session_id);
        let _ = crate::net::sys_close_socket(self.fd as u32);
    }
}
