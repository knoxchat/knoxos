← [Status index](../README.md) · [Do this first](../next.md)

# 5. Networking Stack

**Grade: Wired (88%)** · `net.rs`, `netint.rs`, `virtio_net.rs`, `ipv6.rs`, `e1000.rs`, `rtl8139.rs`, `dhcp.rs`, `dns.rs`, `net_production.rs`

### Wired (builders / closest-to-real NICs)
- [x] Ethernet / ARP / IPv4 / UDP / TCP **header** construction
- [x] `e1000`: reset, MAC, RX/TX rings; `Socket::send` TX + loopback RX (`GATE_E1000 send`)
- [x] `rtl8139`: PIO TX, RX ring poll; software TX→RX with a Realtek ROK header (`GATE_RTL8139 send`)
- [x] DHCP DISCOVER/REQUEST **can be sent**; OFFER/ACK parse
- [x] DNS A-query **can be sent**; response parse
- [x] NTP packet build + UDP send helper. Fixture Server-mode apply (`GATE_NTP apply`)
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
- [x] TLS 1.3 HKDF — RFC 5869 extract/expand + Expand-Label (`GATE_TLS13 hkdf`). ClientHello+ServerHello matching `c hs traffic` (`GATE_TLS13 hs`). DER + RSA PKCS#1 over TBS (`GATE_TLS13 x509`). NFS4 WRITE then READ (`GATE_NFS write`). NFS4 REMOVE then LOOKUP `NFS4ERR_NOENT` (`GATE_NFS remove`). NFS4 RENAME then LOOKUP (`GATE_NFS rename`). NFS4 CREATE then LOOKUP (`GATE_NFS create`). NFS4 LINK then LOOKUP (`GATE_NFS link`). NFS4 CREATE NF4LNK then READLINK (`GATE_NFS readlink`). NFS4 SETATTR then GETATTR/READ (`GATE_NFS setattr`). NFS4 READDIR lists names (`GATE_NFS readdir`). NFS4 ACCESS grants file bits from mode (`GATE_NFS access`). NFS4 COMMIT persists an unstable WRITE (`GATE_NFS commit`). NFS4 GETFH after LOOKUP is a child handle (`GATE_NFS getfh`). NFS4 PUTFH restores it; unknown is STALE (`GATE_NFS putfh`). NFS4 LOOKUPP of a child is the export root (`GATE_NFS lookupp`). NFS4 RESTOREFH restores SAVEFH; missing is NOFILEHANDLE (`GATE_NFS restorefh`). NFS4 PUTPUBFH after LOOKUP is the export root (`GATE_NFS putpubfh`). NFS4 VERIFY matching size is OK; mismatch is NOT_SAME (`GATE_NFS verify`). NFS4 NVERIFY mismatch is OK; match is SAME (`GATE_NFS nverify`). NFS4 SETCLIENTID_CONFIRM of the issued client_id (`GATE_NFS confirm`). NFS4 CLOSE of the issued OPEN (`GATE_NFS close`). NFS4 RENEW of a confirmed client_id (`GATE_NFS renew`). NFS4 LOCK of a free range is OK; overlap is DENIED (`GATE_NFS lock`). NFS4 LOCKU of a held range is OK; missing is DENIED (`GATE_NFS unlock`). NFS4 LOCKT of a free range is OK; overlap is DENIED (`GATE_NFS lockt`). NFS4 RELEASE_LOCKOWNER drops the owner's locks (`GATE_NFS lockowner`). NFS4 OPEN_CONFIRM of the issued OPEN (`GATE_NFS openconfirm`). NFS4 OPEN_DOWNGRADE to a subset (`GATE_NFS downgrade`). NFS4 SECINFO of a known name is AUTH_SYS (`GATE_NFS secinfo`). NFS4 OPENATTR createdir (`GATE_NFS openattr`). NFS4 DELEGPURGE of a confirmed client_id (`GATE_NFS delegpurge`). NFS4 DELEGRETURN of an OPEN state_id (`GATE_NFS delegreturn`).
- [x] NAT — IPv4 SNAT/DNAT rewrite with header checksum (`GATE_NAT masquerade`)
- [x] VLAN — 802.1Q insert/strip (`GATE_VLAN tag`). Wi-Fi Beacon + WPA2 PMK (`GATE_WIFI pmk`). WPA3-SAE matching PMK (`GATE_WPA3 sae`); bridge FDB (`GATE_BRIDGE fdb`).

### Perfect-OS next steps
1. ~~Loopback: `send` → peer `recv_buf`.~~ **D1 done** — kernel self-test + Ring 3 `sendto`/`recvfrom`.
2. ~~Fix VirtIO-net avail/used rings; TX one UDP ping.~~ **D2 done** — DHCP DISCOVER TX + UDP reply RX vs QEMU user-net.
3. ~~Wire `Socket` → `netint::send_*`; ARP then DHCP that **writes the interface IP**.~~ **D3 done.**
4. ~~TCP: SYN/ACK, seq/ack, RTO, then CUBIC.~~ **SYN/ACK + RTO + HTTP GET done (D4). CUBIC cwnd done (D5).**

---
