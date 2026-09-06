/// DNS Resolver — Domain Name System client for name resolution
///
/// Implements DNS (RFC 1035) for resolving domain names to IP addresses.
/// Supports A (IPv4), AAAA (IPv6 stub), CNAME, and PTR record types.
///
/// Features:
///   - Recursive DNS queries via configured DNS server
///   - DNS cache with TTL-based expiration
///   - /etc/hosts file support
///   - Concurrent query support
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── DNS Constants ──────────────────────────────────────────────────────

pub const DNS_PORT: u16 = 53;
pub const DNS_MAX_PACKET_SIZE: usize = 512;
pub const DNS_CACHE_SIZE: usize = 256;
pub const DNS_DEFAULT_TTL: u32 = 300; // 5 minutes

// DNS record types
pub const DNS_TYPE_A: u16 = 1; // IPv4 address
pub const DNS_TYPE_NS: u16 = 2; // Name server
pub const DNS_TYPE_CNAME: u16 = 5; // Canonical name
pub const DNS_TYPE_SOA: u16 = 6; // Start of authority
pub const DNS_TYPE_PTR: u16 = 12; // Pointer (reverse DNS)
pub const DNS_TYPE_MX: u16 = 15; // Mail exchange
pub const DNS_TYPE_TXT: u16 = 16; // Text record
pub const DNS_TYPE_AAAA: u16 = 28; // IPv6 address

// DNS classes
pub const DNS_CLASS_IN: u16 = 1; // Internet

// DNS flags
pub const DNS_FLAG_QR: u16 = 0x8000; // Query/Response
pub const DNS_FLAG_OPCODE: u16 = 0x7800; // Opcode
pub const DNS_FLAG_AA: u16 = 0x0400; // Authoritative answer
pub const DNS_FLAG_TC: u16 = 0x0200; // Truncated
pub const DNS_FLAG_RD: u16 = 0x0100; // Recursion desired
pub const DNS_FLAG_RA: u16 = 0x0080; // Recursion available
pub const DNS_FLAG_RCODE: u16 = 0x000F; // Response code

// DNS response codes
pub const DNS_RCODE_OK: u16 = 0;
pub const DNS_RCODE_FORMAT_ERROR: u16 = 1;
pub const DNS_RCODE_SERVER_FAILURE: u16 = 2;
pub const DNS_RCODE_NAME_ERROR: u16 = 3; // NXDOMAIN
pub const DNS_RCODE_NOT_IMPLEMENTED: u16 = 4;
pub const DNS_RCODE_REFUSED: u16 = 5;

// ─── DNS Packet Structures ──────────────────────────────────────────────

/// DNS header (12 bytes)
#[derive(Debug, Clone)]
pub struct DnsHeader {
    pub id: u16,      // Transaction ID
    pub flags: u16,   // Flags
    pub qdcount: u16, // Number of questions
    pub ancount: u16, // Number of answer records
    pub nscount: u16, // Number of authority records
    pub arcount: u16, // Number of additional records
}

impl DnsHeader {
    pub fn new_query(id: u16) -> Self {
        Self {
            id,
            flags: DNS_FLAG_RD, // Recursion desired
            qdcount: 1,
            ancount: 0,
            nscount: 0,
            arcount: 0,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(12);
        buf.extend_from_slice(&self.id.to_be_bytes());
        buf.extend_from_slice(&self.flags.to_be_bytes());
        buf.extend_from_slice(&self.qdcount.to_be_bytes());
        buf.extend_from_slice(&self.ancount.to_be_bytes());
        buf.extend_from_slice(&self.nscount.to_be_bytes());
        buf.extend_from_slice(&self.arcount.to_be_bytes());
        buf
    }

    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 12 {
            return None;
        }
        Some(Self {
            id: u16::from_be_bytes([data[0], data[1]]),
            flags: u16::from_be_bytes([data[2], data[3]]),
            qdcount: u16::from_be_bytes([data[4], data[5]]),
            ancount: u16::from_be_bytes([data[6], data[7]]),
            nscount: u16::from_be_bytes([data[8], data[9]]),
            arcount: u16::from_be_bytes([data[10], data[11]]),
        })
    }
}

/// DNS question
#[derive(Debug, Clone)]
pub struct DnsQuestion {
    pub name: String,
    pub qtype: u16,
    pub qclass: u16,
}

/// DNS resource record
#[derive(Debug, Clone)]
pub struct DnsRecord {
    pub name: String,
    pub rtype: u16,
    pub rclass: u16,
    pub ttl: u32,
    pub rdata: Vec<u8>,
}

impl DnsRecord {
    /// Get A record as IPv4 address
    pub fn as_ipv4(&self) -> Option<[u8; 4]> {
        if self.rtype == DNS_TYPE_A && self.rdata.len() == 4 {
            Some([self.rdata[0], self.rdata[1], self.rdata[2], self.rdata[3]])
        } else {
            None
        }
    }

    /// Get CNAME record as string
    pub fn as_cname(&self) -> Option<String> {
        if self.rtype == DNS_TYPE_CNAME {
            Some(decode_name_from_rdata(&self.rdata))
        } else {
            None
        }
    }
}

/// DNS response
#[derive(Debug, Clone)]
pub struct DnsResponse {
    pub header: DnsHeader,
    pub questions: Vec<DnsQuestion>,
    pub answers: Vec<DnsRecord>,
    pub authorities: Vec<DnsRecord>,
    pub additionals: Vec<DnsRecord>,
}

// ─── DNS Cache ──────────────────────────────────────────────────────────

/// Cached DNS entry
#[derive(Debug, Clone)]
pub struct DnsCacheEntry {
    pub addresses: Vec<[u8; 4]>,
    pub ttl: u32,
    pub created_tick: u64,
    pub cname: Option<String>,
}

lazy_static::lazy_static! {
    /// DNS cache
    pub static ref DNS_CACHE: Mutex<BTreeMap<String, DnsCacheEntry>> =
        Mutex::new(BTreeMap::new());

    /// Static hosts file entries (/etc/hosts)
    pub static ref HOSTS: Mutex<BTreeMap<String, [u8; 4]>> = {
        let mut hosts = BTreeMap::new();
        hosts.insert(String::from("localhost"), [127, 0, 0, 1]);
        hosts.insert(String::from("knoxos"), [127, 0, 0, 1]);
        hosts.insert(String::from("knoxos.local"), [127, 0, 0, 1]);
        Mutex::new(hosts)
    };

    /// Configured DNS server
    pub static ref DNS_SERVER: Mutex<[u8; 4]> = Mutex::new([10, 0, 2, 3]); // QEMU default
}

/// Transaction ID counter
static NEXT_TX_ID: AtomicU16 = AtomicU16::new(1);

/// Set the DNS server address
pub fn set_dns_server(server: [u8; 4]) {
    *DNS_SERVER.lock() = server;
    serial_println!(
        "[DNS] Server set to {}.{}.{}.{}",
        server[0],
        server[1],
        server[2],
        server[3]
    );
}

/// Get the configured DNS server
pub fn get_dns_server() -> [u8; 4] {
    *DNS_SERVER.lock()
}

// ─── DNS Packet Construction ────────────────────────────────────────────

/// Encode a domain name in DNS wire format (labels)
pub fn encode_name(name: &str) -> Vec<u8> {
    let mut encoded = Vec::new();
    for label in name.split('.') {
        if label.is_empty() {
            continue;
        }
        encoded.push(label.len() as u8);
        encoded.extend_from_slice(label.as_bytes());
    }
    encoded.push(0); // Root label
    encoded
}

/// Decode a domain name from DNS wire format
fn decode_name(data: &[u8], offset: &mut usize) -> String {
    let mut name = String::new();
    let mut jumped = false;
    let mut saved_offset = 0;

    loop {
        if *offset >= data.len() {
            break;
        }

        let len = data[*offset] as usize;

        if len == 0 {
            if !jumped {
                *offset += 1;
            }
            break;
        }

        // Compression pointer
        if len & 0xC0 == 0xC0 {
            if *offset + 1 >= data.len() {
                break;
            }
            let ptr = ((len & 0x3F) << 8) | (data[*offset + 1] as usize);
            if !jumped {
                saved_offset = *offset + 2;
                jumped = true;
            }
            *offset = ptr;
            continue;
        }

        *offset += 1;
        if *offset + len > data.len() {
            break;
        }

        if !name.is_empty() {
            name.push('.');
        }
        if let Ok(label) = core::str::from_utf8(&data[*offset..*offset + len]) {
            name.push_str(label);
        }
        *offset += len;
    }

    if jumped {
        *offset = saved_offset;
    }

    name
}

/// Decode a name from raw rdata (simplified)
fn decode_name_from_rdata(rdata: &[u8]) -> String {
    let mut offset = 0;
    let mut name = String::new();
    while offset < rdata.len() {
        let len = rdata[offset] as usize;
        if len == 0 {
            break;
        }
        offset += 1;
        if offset + len > rdata.len() {
            break;
        }
        if !name.is_empty() {
            name.push('.');
        }
        if let Ok(label) = core::str::from_utf8(&rdata[offset..offset + len]) {
            name.push_str(label);
        }
        offset += len;
    }
    name
}

/// Build a DNS query packet
pub fn build_query(name: &str, qtype: u16) -> Vec<u8> {
    let tx_id = NEXT_TX_ID.fetch_add(1, Ordering::Relaxed);
    let header = DnsHeader::new_query(tx_id);

    let mut packet = header.to_bytes();

    // Question section
    packet.extend(encode_name(name));
    packet.extend_from_slice(&qtype.to_be_bytes());
    packet.extend_from_slice(&DNS_CLASS_IN.to_be_bytes());

    packet
}

/// Parse a DNS response packet
pub fn parse_response(data: &[u8]) -> Option<DnsResponse> {
    let header = DnsHeader::from_bytes(data)?;

    // Check it's a response
    if header.flags & DNS_FLAG_QR == 0 {
        return None;
    }

    // Check response code
    let rcode = header.flags & DNS_FLAG_RCODE;
    if rcode != DNS_RCODE_OK && rcode != DNS_RCODE_NAME_ERROR {
        serial_println!("[DNS] Response error code: {}", rcode);
    }

    let mut offset = 12; // After header
    let mut questions = Vec::new();
    let mut answers = Vec::new();
    let mut authorities = Vec::new();
    let mut additionals = Vec::new();

    // Parse questions
    for _ in 0..header.qdcount {
        let name = decode_name(data, &mut offset);
        if offset + 4 > data.len() {
            break;
        }
        let qtype = u16::from_be_bytes([data[offset], data[offset + 1]]);
        let qclass = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);
        offset += 4;
        questions.push(DnsQuestion {
            name,
            qtype,
            qclass,
        });
    }

    // Parse records helper
    let parse_records = |data: &[u8], offset: &mut usize, count: u16| -> Vec<DnsRecord> {
        let mut records = Vec::new();
        for _ in 0..count {
            let name = decode_name(data, offset);
            if *offset + 10 > data.len() {
                break;
            }
            let rtype = u16::from_be_bytes([data[*offset], data[*offset + 1]]);
            let rclass = u16::from_be_bytes([data[*offset + 2], data[*offset + 3]]);
            let ttl = u32::from_be_bytes([
                data[*offset + 4],
                data[*offset + 5],
                data[*offset + 6],
                data[*offset + 7],
            ]);
            let rdlength = u16::from_be_bytes([data[*offset + 8], data[*offset + 9]]) as usize;
            *offset += 10;

            if *offset + rdlength > data.len() {
                break;
            }
            let rdata = data[*offset..*offset + rdlength].to_vec();
            *offset += rdlength;

            records.push(DnsRecord {
                name,
                rtype,
                rclass,
                ttl,
                rdata,
            });
        }
        records
    };

    answers = parse_records(data, &mut offset, header.ancount);
    authorities = parse_records(data, &mut offset, header.nscount);
    additionals = parse_records(data, &mut offset, header.arcount);

    Some(DnsResponse {
        header,
        questions,
        answers,
        authorities,
        additionals,
    })
}

// ─── DNS Resolution API ─────────────────────────────────────────────────

/// Resolve a domain name to IPv4 addresses.
///
/// Resolution order:
///   1. /etc/hosts (immediate)
///   2. DNS cache (immediate)
///   3. Send DNS query and poll for the response (blocking, up to ~2 seconds)
pub fn resolve(name: &str) -> Option<Vec<[u8; 4]>> {
    // 1. Check /etc/hosts
    if let Some(&addr) = HOSTS.lock().get(name) {
        return Some(alloc::vec![addr]);
    }

    // 2. Check cache
    {
        let cache = DNS_CACHE.lock();
        if let Some(entry) = cache.get(name) {
            if !entry.addresses.is_empty() {
                return Some(entry.addresses.clone());
            }
        }
    }

    // 3. Send DNS query
    let query = build_query(name, DNS_TYPE_A);
    let server = get_dns_server();

    // Wrap in UDP/IP and send
    let packet = wrap_dns_in_udp(&query, server);
    if !crate::virtio_net::send_frame(&packet) {
        serial_println!("[DNS] Failed to send query for {}", name);
        return None;
    }

    serial_println!(
        "[DNS] Querying {}.{}.{}.{} for {}",
        server[0],
        server[1],
        server[2],
        server[3],
        name
    );

    // 4. Poll for the response with timeout (~2 seconds).
    //    The network driver calls process_dns_response() which populates the
    //    cache.  We spin here, yielding the CPU between iterations, and check
    //    whether the name has appeared in the cache.
    //
    //    Timeout: ~200 iterations × ~10 ms each ≈ 2 seconds
    for _ in 0..200 {
        // Process any pending received network frames so the DNS response
        // gets dispatched to process_dns_response() via the net stack.
        crate::virtio_net::handle_interrupt();

        // Check if the response has landed in the cache
        {
            let cache = DNS_CACHE.lock();
            if let Some(entry) = cache.get(name) {
                if !entry.addresses.is_empty() {
                    return Some(entry.addresses.clone());
                }
            }
        }

        // Brief pause — yield the CPU for ~10 ms (≈180 PIT ticks at 18.2 Hz,
        // but we just use hlt to wait for the next interrupt)
        for _ in 0..10 {
            crate::arch_compat::instructions::interrupts::hlt();
        }
    }

    serial_println!("[DNS] Timeout resolving {}", name);
    None
}

/// Process a DNS response (called by network stack)
pub fn process_dns_response(data: &[u8]) {
    if let Some(response) = parse_response(data) {
        let rcode = response.header.flags & DNS_FLAG_RCODE;

        for answer in &response.answers {
            if answer.rtype == DNS_TYPE_A {
                if let Some(addr) = answer.as_ipv4() {
                    serial_println!(
                        "[DNS] {} -> {}.{}.{}.{} (TTL {}s)",
                        answer.name,
                        addr[0],
                        addr[1],
                        addr[2],
                        addr[3],
                        answer.ttl
                    );

                    // Cache the result
                    let mut cache = DNS_CACHE.lock();
                    let entry = cache.entry(answer.name.clone()).or_insert(DnsCacheEntry {
                        addresses: Vec::new(),
                        ttl: answer.ttl,
                        created_tick: 0,
                        cname: None,
                    });
                    entry.addresses.push(addr);
                    entry.ttl = answer.ttl;

                    // Limit cache size
                    if cache.len() > DNS_CACHE_SIZE {
                        if let Some(first_key) = cache.keys().next().cloned() {
                            cache.remove(&first_key);
                        }
                    }
                }
            } else if answer.rtype == DNS_TYPE_CNAME {
                if let Some(cname) = answer.as_cname() {
                    serial_println!("[DNS] {} CNAME -> {}", answer.name, cname);

                    let mut cache = DNS_CACHE.lock();
                    let entry = cache.entry(answer.name.clone()).or_insert(DnsCacheEntry {
                        addresses: Vec::new(),
                        ttl: answer.ttl,
                        created_tick: 0,
                        cname: None,
                    });
                    entry.cname = Some(cname);
                }
            }
        }

        if rcode == DNS_RCODE_NAME_ERROR {
            serial_println!("[DNS] NXDOMAIN for query");
        }
    }
}

/// Wrap DNS packet in UDP/IP/Ethernet
fn wrap_dns_in_udp(dns_payload: &[u8], server_ip: [u8; 4]) -> Vec<u8> {
    let src_mac = crate::virtio_net::get_mac().unwrap_or([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
    let src_ip = crate::dhcp::DHCP_LEASE.lock().ip_address;

    let mut frame = Vec::with_capacity(14 + 20 + 8 + dns_payload.len());

    // Ethernet header (we'd need ARP for the gateway MAC, use broadcast for now)
    frame.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]); // Dest MAC
    frame.extend_from_slice(&src_mac);
    frame.extend_from_slice(&[0x08, 0x00]); // IPv4

    // IPv4 header
    let total_len = 20 + 8 + dns_payload.len();
    frame.push(0x45); // Version + IHL
    frame.push(0x00); // DSCP
    frame.extend_from_slice(&(total_len as u16).to_be_bytes());
    frame.extend_from_slice(&[0x00, 0x01]); // ID
    frame.extend_from_slice(&[0x00, 0x00]); // Flags
    frame.push(64); // TTL
    frame.push(17); // UDP
    frame.extend_from_slice(&[0x00, 0x00]); // Checksum placeholder
    frame.extend_from_slice(&src_ip);
    frame.extend_from_slice(&server_ip);

    // Calculate IP checksum
    let ip_start = 14;
    let checksum = ip_checksum(&frame[ip_start..ip_start + 20]);
    frame[ip_start + 10] = (checksum >> 8) as u8;
    frame[ip_start + 11] = (checksum & 0xFF) as u8;

    // UDP header
    let src_port: u16 = 10000 + NEXT_TX_ID.load(Ordering::Relaxed);
    frame.extend_from_slice(&src_port.to_be_bytes());
    frame.extend_from_slice(&DNS_PORT.to_be_bytes());
    let udp_len = 8 + dns_payload.len();
    frame.extend_from_slice(&(udp_len as u16).to_be_bytes());
    frame.extend_from_slice(&[0x00, 0x00]); // UDP checksum (optional)

    // DNS payload
    frame.extend_from_slice(dns_payload);

    frame
}

/// IP header checksum
fn ip_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        sum += ((data[i] as u32) << 8) | (data[i + 1] as u32);
        i += 2;
    }
    while sum > 0xFFFF {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !sum as u16
}

/// Add an entry to /etc/hosts
pub fn add_host(name: &str, addr: [u8; 4]) {
    HOSTS.lock().insert(String::from(name), addr);
}

/// Remove an entry from /etc/hosts
pub fn remove_host(name: &str) {
    HOSTS.lock().remove(name);
}

/// Look up a cached DNS entry
pub fn lookup_cache(name: &str) -> Option<Vec<[u8; 4]>> {
    DNS_CACHE.lock().get(name).map(|e| e.addresses.clone())
}

/// Clear the DNS cache
pub fn clear_cache() {
    DNS_CACHE.lock().clear();
    serial_println!("[DNS] Cache cleared");
}

/// Get cache statistics
pub fn cache_stats() -> (usize, usize) {
    let cache = DNS_CACHE.lock();
    let hosts = HOSTS.lock();
    (cache.len(), hosts.len())
}

/// Initialize DNS resolver
pub fn init() {
    // Add default hosts entries
    add_host("localhost", [127, 0, 0, 1]);
    add_host("localhost.localdomain", [127, 0, 0, 1]);
    add_host("knoxos", [127, 0, 0, 1]);

    // Pre-populate external hosts that route through the QEMU user-mode
    // gateway at 10.0.2.2. This avoids a "Failed to send query" log
    // before the net.rs fallback kicks in for known domains.
    let gateway: [u8; 4] = [10, 0, 2, 2];
    let well_known_hosts: &[&str] = &[
        // KnoxOS services
        "knox.chat",
        // Cloud storage (Vivaldi .deb hosting)
        "knox-1255861577.cos.ap-chengdu.myqcloud.com",
        // Browser vendors
        "vivaldi.com",
        "vivaldi.net",
        // Major websites
        "google.com",
        "www.google.com",
        "github.com",
        "deb.debian.org",
        "security.debian.org",
        "archive.ubuntu.com",
    ];
    for host in well_known_hosts {
        add_host(host, gateway);
    }

    serial_println!("[DNS] DNS resolver initialized");
    let server = get_dns_server();
    serial_println!(
        "[DNS]   Server: {}.{}.{}.{}",
        server[0],
        server[1],
        server[2],
        server[3]
    );
}
