/// QUIC protocol version 1 (RFC 9000)
pub const QUIC_VERSION_1: u32 = 0x0000_0001;
/// QUIC version 2 (RFC 9369)
pub const QUIC_VERSION_2: u32 = 0x6b33_43cf;

/// Maximum UDP payload size
pub const MAX_UDP_PAYLOAD: usize = 1472;
/// Maximum QUIC packet size (including headers)
pub const MAX_QUIC_PACKET: usize = 1350;
/// Initial max data (connection-level flow control)
pub const INITIAL_MAX_DATA: u64 = 1_048_576; // 1 MiB
/// Initial max stream data
pub const INITIAL_MAX_STREAM_DATA: u64 = 262_144; // 256 KiB
/// Initial max streams (bidi)
pub const INITIAL_MAX_STREAMS_BIDI: u64 = 100;
/// Initial max streams (uni)
pub const INITIAL_MAX_STREAMS_UNI: u64 = 100;
/// Default idle timeout (ms)
pub const DEFAULT_IDLE_TIMEOUT: u64 = 30_000;
/// Default max ack delay (ms)
pub const DEFAULT_MAX_ACK_DELAY: u64 = 25;
/// Initial RTT estimate (ms)
pub const INITIAL_RTT: u64 = 333;
/// Minimum congestion window (bytes)
pub const MIN_CWND: u64 = 2 * MAX_QUIC_PACKET as u64;
/// Initial congestion window (bytes)
pub const INITIAL_CWND: u64 = 10 * MAX_QUIC_PACKET as u64;
