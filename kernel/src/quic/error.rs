/// QUIC error codes (RFC 9000 Section 20)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuicError {
    NoError,
    InternalError,
    ConnectionRefused,
    FlowControlError,
    StreamLimitError,
    StreamStateError,
    FinalSizeError,
    FrameEncodingError,
    TransportParameterError,
    ConnectionIdLimitError,
    ProtocolViolation,
    InvalidToken,
    ApplicationError,
    CryptoBufferExceeded,
    KeyUpdateError,
    AeadLimitReached,
    NoViablePath,
    /// Non-standard: would block
    WouldBlock,
    /// Non-standard: invalid state
    InvalidState,
}

impl QuicError {
    pub fn to_code(self) -> u64 {
        match self {
            QuicError::NoError => 0x00,
            QuicError::InternalError => 0x01,
            QuicError::ConnectionRefused => 0x02,
            QuicError::FlowControlError => 0x03,
            QuicError::StreamLimitError => 0x04,
            QuicError::StreamStateError => 0x05,
            QuicError::FinalSizeError => 0x06,
            QuicError::FrameEncodingError => 0x07,
            QuicError::TransportParameterError => 0x08,
            QuicError::ConnectionIdLimitError => 0x09,
            QuicError::ProtocolViolation => 0x0a,
            QuicError::InvalidToken => 0x0b,
            QuicError::ApplicationError => 0x0c,
            QuicError::CryptoBufferExceeded => 0x0d,
            QuicError::KeyUpdateError => 0x0e,
            QuicError::AeadLimitReached => 0x0f,
            QuicError::NoViablePath => 0x10,
            QuicError::WouldBlock => 0xff00,
            QuicError::InvalidState => 0xff01,
        }
    }
}
