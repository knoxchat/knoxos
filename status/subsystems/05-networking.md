← [Status index](../README.md) · [Do this first](../next.md)

# 5. Networking Stack

**Grade: Wired (78%)** · `net.rs`, `netint.rs`, `virtio_net.rs`, `ipv6.rs`, `e1000.rs`, `rtl8139.rs`, `dhcp.rs`, `dns.rs`, `net_production.rs`

### Wired (builders / closest-to-real NICs)
- [x] Ethernet / ARP / IPv4 / UDP / TCP **header** construction
- [x] `e1000`: reset, MAC, RX/TX rings, TDT/RDT poll (heap-as-DMA caveat)
- [x] `rtl8139`: PIO TX, RX ring poll
- [x] DHCP DISCOVER/REQUEST **can be sent**; OFFER/ACK parse
- [x] DNS A-query **can be sent**; response parse
- [x] NTP packet build + UDP send helper
- [x] **Loopback** — UDP `sendto` and TCP `connect`/`send` copy into the peer `recv_buf` on `127.0.0.1`; Gate D1 Ring 3 demo
- [x] **Gate D2 VirtIO-net rings** — guest-physical avail/used; TX DHCP DISCOVER; RX UDP reply from QEMU user-net (`GATE_D2 virtio-net complete`)
- [x] **Gate D3 DHCP apply** — ACK writes `eth0` IPv4, mask, gateway (`GATE_D3 dhcp applied`)
- [x] **Gate D4 DNS + TCP** — UDP DNS parse; SYN/ACK + RTO; HTTP GET (`GATE_D4 dns tcp complete`)
- [x] **Gate D5 TCP CUBIC** — cwnd grows on ACK, β=0.7 on loss, send limited by window (`GATE_D5 cubic window`)

### Stub (the path apps use)
- [x] **Socket send off-box** — non-loopback `send`/`sendto` goes through `netint` + VirtIO-net
- [x] **TCP connect off-box** — SYN sent; SYN-ACK completes; RTO retransmits
- [x] **TCP CUBIC / window** — `CongestionState` on each TCB; send clamped to cwnd; advertised `rcv_wnd`; loss cuts cwnd (D5)
- [x] **VirtIO-net TX** — writes avail ring with guest-physical bounce; waits for used
- [x] **VirtIO-net RX** — walks the used ring and re-arms buffers
- [x] **DHCP apply** — writes interface IP + DNS + default route
- [x] IPv6 echo — loopback TX reply (`GATE_D6 ipv6 echo`); off-box 0x86DD uses VirtIO-net
- [ ] TLS 1.3 — types; GCM/X.509 stub; handshake flagged done after one recv
- [x] NAT — IPv4 SNAT/DNAT rewrite with header checksum (`GATE_NAT masquerade`)
- [ ] Wi-Fi / WPA3 / bridge / VLAN — in-memory

### Perfect-OS next steps
1. ~~Loopback: `send` → peer `recv_buf`.~~ **D1 done** — kernel self-test + Ring 3 `sendto`/`recvfrom`.
2. ~~Fix VirtIO-net avail/used rings; TX one UDP ping.~~ **D2 done** — DHCP DISCOVER TX + UDP reply RX vs QEMU user-net.
3. ~~Wire `Socket` → `netint::send_*`; ARP then DHCP that **writes the interface IP**.~~ **D3 done.**
4. ~~TCP: SYN/ACK, seq/ack, RTO, then CUBIC.~~ **SYN/ACK + RTO + HTTP GET done (D4). CUBIC cwnd done (D5).**

---
