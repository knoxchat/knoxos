use alloc::vec::Vec;

use super::constants::INITIAL_MAX_STREAM_DATA;
use super::error::QuicError;

/// Stream state (RFC 9000 Section 3)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    /// Open for sending and receiving
    Ready,
    /// Sending data
    Send,
    /// All data sent, waiting for ack
    DataSent,
    /// Reset sent
    ResetSent,
    /// Reset received or all data acked
    ResetRecvd,
    /// Receiving data
    Recv,
    /// All data received
    SizeKnown,
    /// All data read by application
    DataRead,
    /// Reset received
    ResetRead,
}

/// Stream direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamDirection {
    Bidirectional,
    Unidirectional,
}

/// QUIC stream
#[derive(Debug)]
pub struct QuicStream {
    pub id: u64,
    pub state: StreamState,
    pub direction: StreamDirection,
    /// Send buffer
    pub send_buf: Vec<u8>,
    /// Receive buffer
    pub recv_buf: Vec<u8>,
    /// Send offset (next byte to send)
    pub send_offset: u64,
    /// Receive offset (next expected byte)
    pub recv_offset: u64,
    /// Max send data (flow control)
    pub max_send_data: u64,
    /// Max recv data (flow control)
    pub max_recv_data: u64,
    /// FIN flag sent
    pub fin_sent: bool,
    /// FIN flag received
    pub fin_received: bool,
    /// Final size (if known)
    pub final_size: Option<u64>,
}

impl QuicStream {
    pub fn new(id: u64, direction: StreamDirection) -> Self {
        Self {
            id,
            state: StreamState::Ready,
            direction,
            send_buf: Vec::new(),
            recv_buf: Vec::new(),
            send_offset: 0,
            recv_offset: 0,
            max_send_data: INITIAL_MAX_STREAM_DATA,
            max_recv_data: INITIAL_MAX_STREAM_DATA,
            fin_sent: false,
            fin_received: false,
            final_size: None,
        }
    }

    /// Write data to the stream's send buffer
    pub fn write(&mut self, data: &[u8]) -> Result<usize, QuicError> {
        if self.fin_sent {
            return Err(QuicError::FinalSizeError);
        }
        let available = self
            .max_send_data
            .saturating_sub(self.send_offset + self.send_buf.len() as u64);
        let to_write = core::cmp::min(data.len(), available as usize);
        if to_write == 0 {
            return Err(QuicError::FlowControlError);
        }
        self.send_buf.extend_from_slice(&data[..to_write]);
        self.state = StreamState::Send;
        Ok(to_write)
    }

    /// Read data from the stream's receive buffer
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, QuicError> {
        let to_read = core::cmp::min(buf.len(), self.recv_buf.len());
        if to_read == 0 {
            if self.fin_received {
                self.state = StreamState::DataRead;
                return Ok(0); // EOF
            }
            return Err(QuicError::WouldBlock);
        }
        buf[..to_read].copy_from_slice(&self.recv_buf[..to_read]);
        self.recv_buf.drain(..to_read);
        self.recv_offset += to_read as u64;
        Ok(to_read)
    }

    /// Receive data from a STREAM frame
    pub fn receive_data(&mut self, offset: u64, data: &[u8], fin: bool) -> Result<(), QuicError> {
        if offset != self.recv_offset + self.recv_buf.len() as u64 {
            // Out-of-order data — in a full implementation, buffer and reassemble
            // For now, only accept in-order data
            if offset < self.recv_offset {
                return Ok(()); // duplicate, ignore
            }
            return Err(QuicError::StreamStateError);
        }
        self.recv_buf.extend_from_slice(data);
        if fin {
            self.fin_received = true;
            self.final_size = Some(offset + data.len() as u64);
            self.state = StreamState::SizeKnown;
        } else {
            self.state = StreamState::Recv;
        }
        Ok(())
    }

    /// Mark stream as finished (send FIN)
    pub fn finish(&mut self) -> Result<(), QuicError> {
        if self.fin_sent {
            return Err(QuicError::FinalSizeError);
        }
        self.fin_sent = true;
        self.state = StreamState::DataSent;
        Ok(())
    }

    /// Check if stream is client-initiated
    pub fn is_client_initiated(&self) -> bool {
        self.id & 0x01 == 0
    }

    /// Check if stream is bidirectional
    pub fn is_bidirectional(&self) -> bool {
        self.id & 0x02 == 0
    }
}
