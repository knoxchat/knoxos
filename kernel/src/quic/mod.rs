/// QUIC Transport Protocol — RFC 9000 compliant
/// Modern encrypted transport protocol built on UDP
/// Provides multiplexed streams, 0-RTT connection establishment, and built-in TLS 1.3
///
/// Split into submodules for maintainability:
///   constants   — versions, flow-control and congestion defaults
///   varint      — RFC 9000 variable-length integer codec
///   cid         — connection ID
///   packet      — packet types and headers
///   frame       — frame types, encode, and payload parse
///   stream      — stream state machine
///   congestion  — New Reno / Cubic / BBR controller
///   error       — transport error codes
///   connection  — connection state, transport params, packet processing
///   listener    — server listener
///   api         — global connect / listen / send / recv
///   tls         — TLS 1.3 ClientHello / ServerHello stubs
use crate::serial_println;

mod api;
mod cid;
mod congestion;
mod connection;
mod constants;
mod error;
mod frame;
mod listener;
mod packet;
mod stream;
mod tls;
mod varint;

pub use api::*;
pub use cid::*;
pub use congestion::*;
pub use connection::*;
pub use constants::*;
pub use error::*;
pub use frame::*;
pub use listener::*;
pub use packet::*;
pub use stream::*;
pub use varint::*;

/// Initialize QUIC subsystem
pub fn init() {
    serial_println!("[KnoxOS] QUIC transport protocol initialized (RFC 9000/9001/9002)");
}
