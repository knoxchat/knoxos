// ═══════════════════════════════════════════════════════════════════════
// SOCKET ACTIVATION TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::socket_activation::{SocketAddress, SocketProtocol, SocketState, SocketUnit};

#[test_case]
fn test_socket_unit_creation() {
    let unit = SocketUnit::new("test.socket", "test.service", SocketAddress::tcp_port(8080));
    assert_eq!(unit.name, "test.socket");
    assert_eq!(unit.service, "test.service");
    assert_eq!(unit.state, SocketState::Inactive);
    assert_eq!(unit.backlog, 128);
    assert!(unit.reuse_addr);
}

#[test_case]
fn test_socket_address_display() {
    let addr = SocketAddress::tcp_addr(127, 0, 0, 1, 443);
    let port = addr.port();
    assert_eq!(port, Some(443));
}

#[test_case]
fn test_socket_unix_address() {
    let addr = SocketAddress::unix("/run/test.sock");
    assert_eq!(addr.port(), None);
}

#[test_case]
fn test_socket_unit_builder() {
    let unit = SocketUnit::new("http.socket", "httpd.service", SocketAddress::tcp_port(80))
        .with_description("HTTP Socket")
        .with_accept(true)
        .with_protocol(SocketProtocol::Tcp);
    assert!(unit.accept);
    assert_eq!(unit.protocol, SocketProtocol::Tcp);
    assert_eq!(unit.description, "HTTP Socket");
}
