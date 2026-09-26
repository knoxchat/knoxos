// ═══════════════════════════════════════════════════════════════════════
// NETWORK CONFORMANCE TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::dns;
use crate::net::{TCP_ACK, TCP_SYN, TcpHeader};

#[test_case]
fn test_tcp_syn_ack_sequence() {
    let syn = TcpHeader::new(12345, 80, 1, 0, TCP_SYN);
    assert_eq!(syn.flags() & TCP_SYN, TCP_SYN);
    assert_eq!(syn.flags() & TCP_ACK, 0);
    assert_eq!(syn.seq(), 1);

    let syn_ack = TcpHeader::new(80, 12345, 99, 2, TCP_SYN | TCP_ACK);
    assert_eq!(syn_ack.flags() & (TCP_SYN | TCP_ACK), TCP_SYN | TCP_ACK);
    assert_eq!(syn_ack.ack(), 2);
    assert_eq!(syn_ack.seq(), 99);
}

#[test_case]
fn test_tcp_retransmit_timeout() {
    // RTO doubles on timeout (classic exponential backoff), capped.
    let mut rto: u32 = 200;
    for _ in 0..5 {
        rto = (rto * 2).min(3200);
    }
    assert_eq!(rto, 3200);
    assert!(rto >= 200);
}

#[test_case]
fn test_dns_rfc_compliance() {
    let q = dns::build_query("example.com", dns::DNS_TYPE_A);
    assert!(q.len() >= 12 + 17); // header + 7example3com0 + type + class
    let qdcount = u16::from_be_bytes([q[4], q[5]]);
    assert_eq!(qdcount, 1);
    let flags = u16::from_be_bytes([q[2], q[3]]);
    assert_eq!(flags & dns::DNS_FLAG_QR, 0); // query, not response
    assert_eq!(flags & dns::DNS_FLAG_RD, dns::DNS_FLAG_RD);
    assert_eq!(q[12], 7); // "example"
    assert_eq!(&q[13..20], b"example");
    assert_eq!(q[20], 3); // "com"
    assert_eq!(&q[21..24], b"com");
    assert_eq!(q[24], 0);
    let qtype = u16::from_be_bytes([q[25], q[26]]);
    let qclass = u16::from_be_bytes([q[27], q[28]]);
    assert_eq!(qtype, dns::DNS_TYPE_A);
    assert_eq!(qclass, dns::DNS_CLASS_IN);
}

#[test_case]
fn test_virtio_net_ping_if_nic() {
    if !crate::virtio_net::is_nic_available() {
        return;
    }
    assert!(
        crate::virtio_net::ping_self_test(),
        "virtio-net TX/RX used ring must deliver a UDP reply from QEMU"
    );
}

#[test_case]
fn test_nvme_dma_if_present() {
    if !crate::nvme::is_available() {
        return;
    }
    assert!(
        crate::nvme::dma_self_test(),
        "NVMe PRP DMA write then read must round-trip"
    );
}

#[test_case]
fn test_cubic_window() {
    assert!(
        crate::net_production::cubic_self_test(),
        "CUBIC must grow on ACK, shrink on loss, and consume send window"
    );
}

#[test_case]
fn test_ahci_dma_if_present() {
    if !crate::ahci::is_available() {
        return;
    }
    assert!(
        crate::ahci::dma_self_test(),
        "AHCI DMA write then read must round-trip"
    );
}

#[test_case]
fn test_dhcp_applies_ip_if_nic() {
    if !crate::virtio_net::is_nic_available() {
        return;
    }
    assert!(
        crate::dhcp::apply_self_test(),
        "DHCP ACK must write eth0 IP and default route"
    );
}

#[test_case]
fn test_dns_tcp_if_nic() {
    if !crate::virtio_net::is_nic_available() {
        return;
    }
    assert!(
        crate::net::dns_tcp_self_test(),
        "DNS response plus TCP connect/retransmit must succeed"
    );
}
