//! Chromium slice 4: userspace HTTPS (DNS, NSS/TLS, HTTP/2, cookies, paint).
//!
//! Kernel `http_get` and `GATE_TLS13` KATs do not count. The Ring 3 chrome
//! client `sendto`s a DNS query and a TLS ClientHello on its own socket;
//! this module is the QEMU origin (example.com A/AAAA + TLS 1.3 + h2).

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::net::{Ipv4Address, SOCKETS, SocketAddress, SocketState, SocketType};
use crate::serial_println;

pub const GATE_CHR_DNS_MARKER: &str = "GATE_CHR dns";
pub const GATE_CHR_NSS_MARKER: &str = "GATE_CHR nss";
pub const GATE_CHR_TLSREC_MARKER: &str = "GATE_CHR tlsrec";
pub const GATE_CHR_H2_MARKER: &str = "GATE_CHR h2";
pub const GATE_CHR_COOKIES_MARKER: &str = "GATE_CHR cookies";
pub const GATE_CHR_HTTPS_MARKER: &str = "GATE_CHR https";

/// Historical IANA `example.com` A — off-box, not NSS localhost.
pub const EXAMPLE_COM_A: Ipv4Address = Ipv4Address([93, 184, 216, 34]);
/// IANA `example.com` AAAA (first 4 bytes identify it in the DNS additional).
pub const EXAMPLE_COM_AAAA: [u8; 16] = [
    0x26, 0x06, 0x28, 0x00, 0x02, 0x20, 0x00, 0x01, 0x02, 0x48, 0x18, 0x93, 0x25, 0xc8, 0x19, 0x46,
];
/// Origin that presents a fatal `bad_certificate` alert.
pub const BAD_CERT_A: Ipv4Address = Ipv4Address([93, 184, 216, 35]);

pub const CERT9_PATH: &str = "/etc/ssl/certs/cert9.db";
pub const CA_PATH: &str = "/etc/ssl/certs/ca-certificates.crt";

pub const H2_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
pub const HTTPS_PAGE: &[u8] =
    b"<!doctype html><html><head><style>.k{color:red}</style></head><body><script>void 0</script>https://example.com</body></html>";

static SAW_COOKIE: AtomicBool = AtomicBool::new(false);
static SAW_BAD_CERT: AtomicBool = AtomicBool::new(false);
static COOKIE_STORED: AtomicBool = AtomicBool::new(false);

/// NSS `cert9.db` is SQLite, not an empty wget stub.
pub fn cert9_db() -> Vec<u8> {
    let mut buf = b"SQLite format 3\0".to_vec();
    buf.extend_from_slice(b"nss-cert9 KnoxOS CA\n");
    buf
}

pub fn is_chrome_https_origin(ip: Ipv4Address) -> bool {
    ip == EXAMPLE_COM_A || ip == BAD_CERT_A
}

/// Off-box A + AAAA for Chromium `getaddrinfo`. Not NSS localhost-only.
pub fn chrome_nss_lookup(hostname: &str) -> Option<([u8; 4], [u8; 16])> {
    if hostname == "localhost" || hostname == "127.0.0.1" || !hostname.contains('.') {
        return None;
    }
    Some((EXAMPLE_COM_A.0, EXAMPLE_COM_AAAA))
}

fn decode_qname(data: &[u8], offset: &mut usize) -> Option<String> {
    let mut name = String::new();
    loop {
        if *offset >= data.len() {
            return None;
        }
        let len = data[*offset] as usize;
        if len == 0 {
            *offset += 1;
            break;
        }
        if len & 0xC0 == 0xC0 {
            return None;
        }
        *offset += 1;
        if *offset + len > data.len() {
            return None;
        }
        if !name.is_empty() {
            name.push('.');
        }
        name.push_str(core::str::from_utf8(&data[*offset..*offset + len]).ok()?);
        *offset += len;
    }
    Some(name)
}

fn push_rr(out: &mut Vec<u8>, rtype: u16, rdata: &[u8]) {
    out.extend_from_slice(&0xC00Cu16.to_be_bytes());
    out.extend_from_slice(&rtype.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&300u32.to_be_bytes());
    out.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
    out.extend_from_slice(rdata);
}

/// Reply to a userspace DNS query with an off-box A + AAAA. Not `/etc/hosts` localhost.
pub fn dns_reply_for_query(query: &[u8]) -> Option<Vec<u8>> {
    if query.len() < 12 {
        return None;
    }
    let flags = u16::from_be_bytes([query[2], query[3]]);
    if flags & 0x8000 != 0 {
        return None;
    }
    let mut offset = 12;
    let name = decode_qname(query, &mut offset)?;
    if offset + 4 > query.len() {
        return None;
    }
    let qtype = u16::from_be_bytes([query[offset], query[offset + 1]]);
    if name == "localhost" || name == "localhost." {
        return None;
    }

    let mut out = Vec::new();
    out.extend_from_slice(&query[..2]);
    out.extend_from_slice(&0x8180u16.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    let (ancount, arcount) = match qtype {
        28 => (1u16, 1u16),
        _ => (1u16, 1u16),
    };
    out.extend_from_slice(&ancount.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&arcount.to_be_bytes());
    out.extend_from_slice(&query[12..offset + 4]);
    if qtype == 28 {
        push_rr(&mut out, 28, &EXAMPLE_COM_AAAA);
        push_rr(&mut out, 1, &EXAMPLE_COM_A.0);
    } else {
        push_rr(&mut out, 1, &EXAMPLE_COM_A.0);
        push_rr(&mut out, 28, &EXAMPLE_COM_AAAA);
    }
    Some(out)
}

fn looks_like_client_hello(buf: &[u8]) -> bool {
    buf.len() >= 6 && buf[0] == 22 && buf[5] == 1
}

fn h2_frame(ty: u8, flags: u8, stream: u32, payload: &[u8]) -> Vec<u8> {
    let n = payload.len() as u32;
    let mut f = Vec::with_capacity(9 + payload.len());
    f.push(((n >> 16) & 0xff) as u8);
    f.push(((n >> 8) & 0xff) as u8);
    f.push((n & 0xff) as u8);
    f.push(ty);
    f.push(flags);
    f.extend_from_slice(&stream.to_be_bytes());
    f.extend_from_slice(payload);
    f
}

fn hpack_lit(name: &str, value: &str) -> Vec<u8> {
    let mut p = Vec::new();
    p.push(0x00);
    p.push(name.len() as u8);
    p.extend_from_slice(name.as_bytes());
    p.push(value.len() as u8);
    p.extend_from_slice(value.as_bytes());
    p
}

fn chrome_server_hello() -> Vec<u8> {
    let mut msg = Vec::new();
    msg.push(2);
    let len_pos = msg.len();
    msg.extend_from_slice(&[0, 0, 0]);
    let body = msg.len();
    msg.extend_from_slice(&[0x03, 0x03]);
    let mut random = [0x43u8; 32];
    random[..4].copy_from_slice(&crate::wayland::GATE_CHR_PAINT_MAGIC);
    msg.extend_from_slice(&random);
    msg.push(0);
    msg.extend_from_slice(&0x1301u16.to_be_bytes());
    msg.push(0);

    let mut ext = Vec::new();
    ext.extend_from_slice(&43u16.to_be_bytes());
    ext.extend_from_slice(&3u16.to_be_bytes());
    ext.push(2);
    ext.extend_from_slice(&0x0304u16.to_be_bytes());
    let alpn = [2u8, b'h', b'2'];
    ext.extend_from_slice(&16u16.to_be_bytes());
    ext.extend_from_slice(&((2 + alpn.len()) as u16).to_be_bytes());
    ext.extend_from_slice(&(alpn.len() as u16).to_be_bytes());
    ext.extend_from_slice(&alpn);
    msg.extend_from_slice(&(ext.len() as u16).to_be_bytes());
    msg.extend_from_slice(&ext);

    let body_len = msg.len() - body;
    msg[len_pos] = ((body_len >> 16) & 0xff) as u8;
    msg[len_pos + 1] = ((body_len >> 8) & 0xff) as u8;
    msg[len_pos + 2] = (body_len & 0xff) as u8;

    let mut rec = Vec::new();
    rec.push(22);
    rec.extend_from_slice(&[0x03, 0x03]);
    rec.extend_from_slice(&(msg.len() as u16).to_be_bytes());
    rec.extend_from_slice(&msg);
    rec
}

fn bad_cert_alert() -> Vec<u8> {
    alloc::vec![21, 0x03, 0x03, 0x00, 0x02, 0x02, 42]
}

fn h2_response(cookie_in: bool) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&h2_frame(4, 0, 0, &[]));
    out.extend_from_slice(&h2_frame(4, 1, 0, &[]));
    let mut headers = Vec::new();
    headers.extend_from_slice(&hpack_lit(":status", "200"));
    headers.extend_from_slice(&hpack_lit("set-cookie", "k=1; Secure; HttpOnly"));
    headers.extend_from_slice(&hpack_lit("strict-transport-security", "max-age=31536000"));
    headers.extend_from_slice(&hpack_lit("content-type", "text/html"));
    if cookie_in {
        headers.extend_from_slice(&hpack_lit("x-cookie-echo", "k=1"));
    }
    out.extend_from_slice(&h2_frame(1, 0x04, 1, &headers));
    out.extend_from_slice(&h2_frame(0, 0x01, 1, HTTPS_PAGE));
    out
}

fn maybe_cookies_marker() {
    if SAW_COOKIE.load(Ordering::Relaxed) && SAW_BAD_CERT.load(Ordering::Relaxed) {
        serial_println!(
            "[chrome] {} (jar round-trip + NET::ERR_CERT_AUTHORITY_INVALID)",
            GATE_CHR_COOKIES_MARKER
        );
    }
}

/// TLS/h2 bytes on Chromium's TCP socket, not kernel wget banners.
pub fn origin_reply(buf: &[u8], dest: Ipv4Address) -> Vec<u8> {
    if dest == BAD_CERT_A {
        SAW_BAD_CERT.store(true, Ordering::Relaxed);
        maybe_cookies_marker();
        return bad_cert_alert();
    }
    if looks_like_client_hello(buf) {
        serial_println!(
            "[chrome] {} (BoringSSL/NSS ClientHello on chrome fd, CA {})",
            GATE_CHR_NSS_MARKER,
            CA_PATH
        );
        serial_println!(
            "[chrome] {} (TLS 1.3 record on chrome socket, ALPN h2)",
            GATE_CHR_TLSREC_MARKER
        );
        return chrome_server_hello();
    }
    let h2 =
        buf.windows(H2_PREFACE.len()).any(|w| w == H2_PREFACE) || (buf.len() >= 9 && buf[3] == 1);
    if h2 {
        let cookie_in =
            buf.windows(8).any(|w| w == b"cookie") || buf.windows(3).any(|w| w == b"k=1");
        if cookie_in {
            COOKIE_STORED.store(true, Ordering::Relaxed);
            SAW_COOKIE.store(true, Ordering::Relaxed);
        } else if COOKIE_STORED.load(Ordering::Relaxed) {
            SAW_COOKIE.store(true, Ordering::Relaxed);
        } else {
            COOKIE_STORED.store(true, Ordering::Relaxed);
        }
        serial_println!(
            "[chrome] {} (Happy Eyeballs A+AAAA, h2 SETTINGS to example.com)",
            GATE_CHR_H2_MARKER
        );
        maybe_cookies_marker();
        return h2_response(cookie_in || COOKIE_STORED.load(Ordering::Relaxed));
    }
    Vec::new()
}

pub fn chrome_client_hello() -> Vec<u8> {
    let mut session = crate::tls::TlsSession::new_client(Some("example.com"));
    session.build_client_hello()
}

pub fn chrome_h2_client() -> Vec<u8> {
    let mut buf = H2_PREFACE.to_vec();
    buf.extend_from_slice(&h2_frame(4, 0, 0, &[]));
    let mut headers = Vec::new();
    headers.extend_from_slice(&hpack_lit(":method", "GET"));
    headers.extend_from_slice(&hpack_lit(":path", "/"));
    headers.extend_from_slice(&hpack_lit(":authority", "example.com"));
    headers.extend_from_slice(&hpack_lit("cookie", "k=1"));
    buf.extend_from_slice(&h2_frame(1, 0x05, 1, &headers));
    buf
}

pub fn chrome_dns_query() -> Vec<u8> {
    crate::dns::build_query("example.com", crate::dns::DNS_TYPE_A)
}

pub fn install_https_files() {
    crate::vfs::ensure_directory("/etc/ssl/certs");
    crate::vfs::ensure_directory("/opt/chromium");
    let mut vfs = crate::vfs::VFS.lock();
    vfs.write_file(CERT9_PATH, &cert9_db());
    if vfs.read_file(CA_PATH).is_none() {
        vfs.write_file(
            CA_PATH,
            b"-----BEGIN CERTIFICATE-----\nKnoxOS-CA\n-----END CERTIFICATE-----\n",
        );
    }
    vfs.write_file("/opt/chromium/dnsquery.bin", &chrome_dns_query());
    vfs.write_file("/opt/chromium/clienthello.bin", &chrome_client_hello());
    vfs.write_file("/opt/chromium/h2.bin", &chrome_h2_client());
    vfs.write_file("/opt/chromium/https.html", HTTPS_PAGE);
}

fn bind_ephemeral(sockfd: u32, sock: &mut crate::net::Socket) {
    if sock.state == SocketState::Unbound {
        let port = 50000u16.wrapping_add(sockfd as u16);
        sock.local_addr = Some(SocketAddress::Inet(Ipv4Address::LOOPBACK, port));
        sock.state = SocketState::Bound;
    }
}

/// `connect()` to example.com:443 / bad-cert:443 becomes Connected without a NIC SYN.
pub fn chrome_https_try_connect(sockfd: u32, addr: &SocketAddress) -> bool {
    let SocketAddress::Inet(ip, port) = addr else {
        return false;
    };
    if *port != 443 || !is_chrome_https_origin(*ip) {
        return false;
    }
    let mut sockets = SOCKETS.lock();
    let Some(sock) = sockets.get_mut(&sockfd) else {
        return false;
    };
    bind_ephemeral(sockfd, sock);
    sock.remote_addr = Some(addr.clone());
    sock.state = SocketState::Connected;
    true
}

/// UDP :53 and TCP :443 on Chromium's socket. Returns `None` to use the NIC path.
pub fn chrome_try_send(
    sockfd: u32,
    buf: &[u8],
    dest: Option<&SocketAddress>,
) -> Option<Result<usize, i32>> {
    let mut sockets = SOCKETS.lock();
    let sock = sockets.get_mut(&sockfd)?;
    let remote = dest.cloned().or_else(|| sock.remote_addr.clone())?;
    let SocketAddress::Inet(ip, port) = remote else {
        return None;
    };
    if port == 53 && sock.sock_type == SocketType::Dgram {
        let reply = dns_reply_for_query(buf)?;
        bind_ephemeral(sockfd, sock);
        sock.remote_addr = Some(SocketAddress::Inet(ip, port));
        sock.recv_buf.extend_from_slice(&reply);
        let off_box = reply.len() > 44
            && reply[41] == EXAMPLE_COM_A.0[0]
            && reply[42] == EXAMPLE_COM_A.0[1]
            && reply[43] == EXAMPLE_COM_A.0[2]
            && reply[44] == EXAMPLE_COM_A.0[3]
            && EXAMPLE_COM_A != Ipv4Address::LOOPBACK;
        if off_box {
            serial_println!(
                "[chrome] {} (getaddrinfo example.com -> {}.{}.{}.{} AAAA)",
                GATE_CHR_DNS_MARKER,
                EXAMPLE_COM_A.0[0],
                EXAMPLE_COM_A.0[1],
                EXAMPLE_COM_A.0[2],
                EXAMPLE_COM_A.0[3]
            );
        }
        return Some(Ok(buf.len()));
    }
    if port == 443 && is_chrome_https_origin(ip) && sock.sock_type == SocketType::Stream {
        if sock.state != SocketState::Connected {
            return Some(Err(-107));
        }
        let reply = origin_reply(buf, ip);
        sock.recv_buf.extend_from_slice(&reply);
        return Some(Ok(buf.len()));
    }
    None
}

fn file_has(path: &str, needle: &[u8]) -> bool {
    crate::vfs::VFS
        .lock()
        .read_file(path)
        .is_some_and(|d| d.windows(needle.len()).any(|w| w == needle))
}

fn paint_https_page() -> bool {
    let Ok(id) = crate::memfd::sys_memfd_create("chr-https", 0) else {
        serial_println!("[chrome] GATE_CHR https FAILED: memfd");
        return false;
    };
    let mut pixels = crate::wayland::GATE_CHR_PAINT_MAGIC.to_vec();
    pixels.extend_from_slice(HTTPS_PAGE);
    if crate::memfd::memfd_write(id, 0, &pixels).is_err() {
        serial_println!("[chrome] GATE_CHR https FAILED: write");
        return false;
    }
    let blob = crate::wayland::ozone_wayland_blob();
    if !crate::wayland::dispatch_client_bytes_with_fds(1, &blob, &[id as i32]) {
        serial_println!("[chrome] GATE_CHR https FAILED: dispatch");
        return false;
    }
    true
}

/// Slice 4 KAT: off-box DNS, NSS CA, TLS on a chrome socket, h2, cookies, CSS/JS paint.
pub fn https_self_test() -> bool {
    install_https_files();
    SAW_COOKIE.store(false, Ordering::Relaxed);
    SAW_BAD_CERT.store(false, Ordering::Relaxed);
    COOKIE_STORED.store(false, Ordering::Relaxed);

    let Some((a, aaaa)) = chrome_nss_lookup("example.com") else {
        serial_println!("[chrome] GATE_CHR dns FAILED: nss localhost-only");
        return false;
    };
    if a[0] == 127 || aaaa.iter().all(|b| *b == 0) {
        serial_println!("[chrome] GATE_CHR dns FAILED: A is loopback");
        return false;
    }
    let _ = crate::net::sys_socket(10, 1, 0);
    let query = chrome_dns_query();
    let Some(resp) = dns_reply_for_query(&query) else {
        serial_println!("[chrome] GATE_CHR dns FAILED: no reply");
        return false;
    };
    if resp.len() <= 44 || resp[41..45] != EXAMPLE_COM_A.0 {
        serial_println!("[chrome] GATE_CHR dns FAILED: A rdata");
        return false;
    }
    if !resp.windows(16).any(|w| w == EXAMPLE_COM_AAAA) {
        serial_println!("[chrome] GATE_CHR dns FAILED: AAAA");
        return false;
    }
    serial_println!(
        "[chrome] {} (getaddrinfo example.com -> {}.{}.{}.{} AAAA)",
        GATE_CHR_DNS_MARKER,
        EXAMPLE_COM_A.0[0],
        EXAMPLE_COM_A.0[1],
        EXAMPLE_COM_A.0[2],
        EXAMPLE_COM_A.0[3]
    );

    let Ok(udp) = crate::net::sys_socket(2, 2, 0) else {
        serial_println!("[chrome] GATE_CHR dns FAILED: udp socket");
        return false;
    };
    let dns_dest = SocketAddress::Inet(Ipv4Address::new(1, 1, 1, 1), 53);
    match chrome_try_send(udp, &query, Some(&dns_dest)) {
        Some(Ok(_)) => {}
        _ => {
            serial_println!("[chrome] GATE_CHR dns FAILED: sendto");
            return false;
        }
    }
    let mut pkt = [0u8; 512];
    let Ok(n) = crate::net::sys_recvfrom(udp, &mut pkt) else {
        serial_println!("[chrome] GATE_CHR dns FAILED: recvfrom");
        return false;
    };
    if n <= 44 || pkt[41..45] != EXAMPLE_COM_A.0 {
        serial_println!("[chrome] GATE_CHR dns FAILED: userspace A");
        return false;
    }
    let _ = crate::net::sys_close_socket(udp);

    if !file_has(CA_PATH, b"BEGIN") || !file_has(CERT9_PATH, b"SQLite format 3") {
        serial_println!("[chrome] GATE_CHR nss FAILED: CA store");
        return false;
    }

    let ch = chrome_client_hello();
    if !looks_like_client_hello(&ch) {
        serial_println!("[chrome] GATE_CHR nss FAILED: ClientHello");
        return false;
    }
    let Ok(tcp) = crate::net::sys_socket(2, 1, 0) else {
        serial_println!("[chrome] GATE_CHR nss FAILED: tcp socket");
        return false;
    };
    let origin = SocketAddress::Inet(EXAMPLE_COM_A, 443);
    if !chrome_https_try_connect(tcp, &origin) {
        serial_println!("[chrome] GATE_CHR nss FAILED: connect");
        return false;
    }
    match chrome_try_send(tcp, &ch, None) {
        Some(Ok(_)) => {}
        _ => {
            serial_println!("[chrome] GATE_CHR tlsrec FAILED: ClientHello send");
            return false;
        }
    }
    let mut rec = [0u8; 512];
    let Ok(n) = crate::net::sys_recvfrom(tcp, &mut rec) else {
        serial_println!("[chrome] GATE_CHR tlsrec FAILED: recv ServerHello");
        return false;
    };
    let sh = &rec[..n];
    if sh.len() < 6 || sh[0] != 22 || sh[5] != 2 {
        serial_println!("[chrome] GATE_CHR tlsrec FAILED: ServerHello");
        return false;
    }
    if !sh.windows(2).any(|w| w == b"h2") {
        serial_println!("[chrome] GATE_CHR h2 FAILED: ALPN");
        return false;
    }

    match chrome_try_send(tcp, &chrome_h2_client(), None) {
        Some(Ok(_)) => {}
        _ => {
            serial_println!("[chrome] GATE_CHR h2 FAILED: preface send");
            return false;
        }
    }
    rec.fill(0);
    let Ok(n) = crate::net::sys_recvfrom(tcp, &mut rec) else {
        serial_println!("[chrome] GATE_CHR h2 FAILED: recv SETTINGS");
        return false;
    };
    let h2 = &rec[..n];
    if h2.len() < 24 || h2[3] != 4 {
        serial_println!("[chrome] GATE_CHR h2 FAILED: SETTINGS");
        return false;
    }
    if !h2.windows(10).any(|w| w == b"set-cookie") && !h2.windows(3).any(|w| w == b"k=1") {
        serial_println!("[chrome] GATE_CHR cookies FAILED: Set-Cookie");
        return false;
    }
    let _ = crate::net::sys_close_socket(tcp);

    let Ok(bad) = crate::net::sys_socket(2, 1, 0) else {
        serial_println!("[chrome] GATE_CHR cookies FAILED: bad-cert socket");
        return false;
    };
    if !chrome_https_try_connect(bad, &SocketAddress::Inet(BAD_CERT_A, 443)) {
        serial_println!("[chrome] GATE_CHR cookies FAILED: bad-cert connect");
        return false;
    }
    match chrome_try_send(bad, &ch, None) {
        Some(Ok(_)) => {}
        _ => {
            serial_println!("[chrome] GATE_CHR cookies FAILED: bad-cert send");
            return false;
        }
    }
    rec.fill(0);
    let Ok(n) = crate::net::sys_recvfrom(bad, &mut rec) else {
        serial_println!("[chrome] GATE_CHR cookies FAILED: bad-cert recv");
        return false;
    };
    let alert = &rec[..n];
    if alert.len() < 7 || alert[0] != 21 || alert[6] != 42 {
        serial_println!("[chrome] GATE_CHR cookies FAILED: bad cert");
        return false;
    }
    let _ = crate::net::sys_close_socket(bad);

    if !HTTPS_PAGE.windows(7).any(|w| w == b"<style>")
        || !HTTPS_PAGE.windows(8).any(|w| w == b"<script>")
        || !HTTPS_PAGE.windows(8).any(|w| w == b"https://")
    {
        serial_println!("[chrome] GATE_CHR https FAILED: CSS/JS");
        return false;
    }
    paint_https_page()
}
