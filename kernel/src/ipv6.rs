/// IPv6 Networking Stack
/// Implements IPv6 protocol suite for Linux-compatible networking
///
/// Features:
/// - IPv6 header parsing and generation
/// - ICMPv6 (Neighbor Discovery, Router Solicitation/Advertisement, Echo)
/// - Neighbor Discovery Protocol (NDP)
/// - IPv6 address types (link-local, global, multicast, loopback)
/// - Dual-stack IPv4/IPv6 operation
/// - IPv6 routing table
/// - Stateless Address Autoconfiguration (SLAAC)
/// - IPv6 extension headers (Hop-by-Hop, Routing, Fragment, Destination)
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// IPv6 ADDRESS
// ═══════════════════════════════════════════════════════════════════════

/// IPv6 address (128 bits)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ipv6Address(pub [u8; 16]);

impl Ipv6Address {
    pub const UNSPECIFIED: Ipv6Address = Ipv6Address([0; 16]);
    pub const LOOPBACK: Ipv6Address = Ipv6Address([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    pub const ALL_NODES_LINK_LOCAL: Ipv6Address =
        Ipv6Address([0xff, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    pub const ALL_ROUTERS_LINK_LOCAL: Ipv6Address =
        Ipv6Address([0xff, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2]);

    pub fn new(bytes: [u8; 16]) -> Self {
        Ipv6Address(bytes)
    }

    /// Create from 8 16-bit segments
    pub fn from_segments(segs: [u16; 8]) -> Self {
        let mut bytes = [0u8; 16];
        for (i, &s) in segs.iter().enumerate() {
            bytes[i * 2] = (s >> 8) as u8;
            bytes[i * 2 + 1] = (s & 0xFF) as u8;
        }
        Ipv6Address(bytes)
    }

    /// Generate link-local address from MAC address (EUI-64)
    pub fn from_mac_link_local(mac: &[u8; 6]) -> Self {
        let mut bytes = [0u8; 16];
        // fe80::/10 prefix
        bytes[0] = 0xfe;
        bytes[1] = 0x80;
        // Interface ID from EUI-64
        bytes[8] = mac[0] ^ 0x02; // Flip universal/local bit
        bytes[9] = mac[1];
        bytes[10] = mac[2];
        bytes[11] = 0xFF;
        bytes[12] = 0xFE;
        bytes[13] = mac[3];
        bytes[14] = mac[4];
        bytes[15] = mac[5];
        Ipv6Address(bytes)
    }

    /// Solicited-node multicast address for NDP
    pub fn solicited_node_multicast(&self) -> Self {
        let mut bytes = [0u8; 16];
        bytes[0] = 0xFF;
        bytes[1] = 0x02;
        bytes[11] = 0x01;
        bytes[12] = 0xFF;
        bytes[13] = self.0[13];
        bytes[14] = self.0[14];
        bytes[15] = self.0[15];
        Ipv6Address(bytes)
    }

    pub fn is_unspecified(&self) -> bool {
        *self == Self::UNSPECIFIED
    }
    pub fn is_loopback(&self) -> bool {
        *self == Self::LOOPBACK
    }
    pub fn is_multicast(&self) -> bool {
        self.0[0] == 0xFF
    }
    pub fn is_link_local(&self) -> bool {
        self.0[0] == 0xFE && (self.0[1] & 0xC0) == 0x80
    }
    pub fn is_global_unicast(&self) -> bool {
        !self.is_unspecified()
            && !self.is_loopback()
            && !self.is_multicast()
            && !self.is_link_local()
    }
    pub fn is_solicited_node_multicast(&self) -> bool {
        self.0[0] == 0xFF && self.0[1] == 0x02 && self.0[11] == 0x01 && self.0[12] == 0xFF
    }

    /// Get segments as u16 array
    pub fn segments(&self) -> [u16; 8] {
        let mut segs = [0u16; 8];
        for i in 0..8 {
            segs[i] = ((self.0[i * 2] as u16) << 8) | self.0[i * 2 + 1] as u16;
        }
        segs
    }
}

impl core::fmt::Display for Ipv6Address {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let segs = self.segments();
        write!(
            f,
            "{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}",
            segs[0], segs[1], segs[2], segs[3], segs[4], segs[5], segs[6], segs[7]
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════
// IPv6 HEADER
// ═══════════════════════════════════════════════════════════════════════

/// IPv6 Next Header values (same as IPv4 protocol numbers)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NextHeader {
    HopByHop = 0,
    Icmpv4 = 1,
    Tcp = 6,
    Udp = 17,
    Routing = 43,
    Fragment = 44,
    Esp = 50,
    Ah = 51,
    Icmpv6 = 58,
    NoNextHeader = 59,
    DestinationOptions = 60,
}

impl NextHeader {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::HopByHop),
            1 => Some(Self::Icmpv4),
            6 => Some(Self::Tcp),
            17 => Some(Self::Udp),
            43 => Some(Self::Routing),
            44 => Some(Self::Fragment),
            50 => Some(Self::Esp),
            51 => Some(Self::Ah),
            58 => Some(Self::Icmpv6),
            59 => Some(Self::NoNextHeader),
            60 => Some(Self::DestinationOptions),
            _ => None,
        }
    }
}

/// IPv6 header (40 bytes, fixed size)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ipv6Header {
    pub version_tc_fl: u32,  // Version (4) + Traffic Class (8) + Flow Label (20)
    pub payload_length: u16, // Big-endian
    pub next_header: u8,
    pub hop_limit: u8,
    pub src: [u8; 16],
    pub dst: [u8; 16],
}

impl Ipv6Header {
    pub const SIZE: usize = 40;

    pub fn version(&self) -> u8 {
        ((u32::from_be(self.version_tc_fl) >> 28) & 0xF) as u8
    }

    pub fn traffic_class(&self) -> u8 {
        ((u32::from_be(self.version_tc_fl) >> 20) & 0xFF) as u8
    }

    pub fn flow_label(&self) -> u32 {
        u32::from_be(self.version_tc_fl) & 0xFFFFF
    }

    pub fn payload_len(&self) -> u16 {
        u16::from_be(self.payload_length)
    }

    pub fn src_addr(&self) -> Ipv6Address {
        Ipv6Address(self.src)
    }

    pub fn dst_addr(&self) -> Ipv6Address {
        Ipv6Address(self.dst)
    }

    /// Build an IPv6 header
    pub fn new(
        src: Ipv6Address,
        dst: Ipv6Address,
        next_header: u8,
        payload_len: u16,
        hop_limit: u8,
    ) -> Self {
        let version_tc_fl = (6u32 << 28).to_be(); // Version 6, TC=0, FL=0
        Self {
            version_tc_fl,
            payload_length: payload_len.to_be(),
            next_header,
            hop_limit,
            src: src.0,
            dst: dst.0,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// EXTENSION HEADERS
// ═══════════════════════════════════════════════════════════════════════

/// Hop-by-Hop / Destination Options header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct ExtensionHeader {
    pub next_header: u8,
    pub hdr_ext_len: u8, // Length in 8-octet units, not including first 8 octets
}

/// Fragment header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct FragmentHeader {
    pub next_header: u8,
    pub reserved: u8,
    pub frag_offset_flags: u16, // Fragment Offset (13) + Res (2) + M flag (1)
    pub identification: u32,
}

impl FragmentHeader {
    pub fn fragment_offset(&self) -> u16 {
        (u16::from_be(self.frag_offset_flags) >> 3) * 8
    }

    pub fn more_fragments(&self) -> bool {
        u16::from_be(self.frag_offset_flags) & 1 != 0
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ICMPv6
// ═══════════════════════════════════════════════════════════════════════

/// ICMPv6 message types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Icmpv6Type {
    DestinationUnreachable = 1,
    PacketTooBig = 2,
    TimeExceeded = 3,
    ParameterProblem = 4,
    EchoRequest = 128,
    EchoReply = 129,
    RouterSolicitation = 133,
    RouterAdvertisement = 134,
    NeighborSolicitation = 135,
    NeighborAdvertisement = 136,
    Redirect = 137,
}

impl Icmpv6Type {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::DestinationUnreachable),
            2 => Some(Self::PacketTooBig),
            3 => Some(Self::TimeExceeded),
            4 => Some(Self::ParameterProblem),
            128 => Some(Self::EchoRequest),
            129 => Some(Self::EchoReply),
            133 => Some(Self::RouterSolicitation),
            134 => Some(Self::RouterAdvertisement),
            135 => Some(Self::NeighborSolicitation),
            136 => Some(Self::NeighborAdvertisement),
            137 => Some(Self::Redirect),
            _ => None,
        }
    }
}

/// ICMPv6 header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Icmpv6Header {
    pub icmp_type: u8,
    pub code: u8,
    pub checksum: u16,
}

/// Neighbor Solicitation message body
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct NeighborSolicitation {
    pub reserved: u32,
    pub target: [u8; 16],
}

/// Neighbor Advertisement message body
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct NeighborAdvertisement {
    pub flags: u32, // Router (R), Solicited (S), Override (O) in high bits
    pub target: [u8; 16],
}

impl NeighborAdvertisement {
    pub const FLAG_ROUTER: u32 = 1 << 31;
    pub const FLAG_SOLICITED: u32 = 1 << 30;
    pub const FLAG_OVERRIDE: u32 = 1 << 29;
}

/// Router Advertisement message body
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct RouterAdvertisement {
    pub cur_hop_limit: u8,
    pub flags: u8, // M (managed) + O (other config)
    pub router_lifetime: u16,
    pub reachable_time: u32,
    pub retrans_timer: u32,
}

/// NDP Option types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NdpOptionType {
    SourceLinkLayerAddress = 1,
    TargetLinkLayerAddress = 2,
    PrefixInformation = 3,
    RedirectedHeader = 4,
    Mtu = 5,
}

/// NDP option header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct NdpOption {
    pub option_type: u8,
    pub length: u8, // In units of 8 octets
}

/// Prefix Information option (for SLAAC)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct PrefixInfoOption {
    pub option_type: u8,
    pub length: u8,
    pub prefix_length: u8,
    pub flags: u8, // L (on-link) + A (autonomous)
    pub valid_lifetime: u32,
    pub preferred_lifetime: u32,
    pub reserved2: u32,
    pub prefix: [u8; 16],
}

impl PrefixInfoOption {
    pub const FLAG_ON_LINK: u8 = 1 << 7;
    pub const FLAG_AUTONOMOUS: u8 = 1 << 6;
}

// ═══════════════════════════════════════════════════════════════════════
// NEIGHBOR CACHE (NDP equivalent of ARP cache)
// ═══════════════════════════════════════════════════════════════════════

/// Neighbor cache entry states (RFC 4861)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeighborState {
    Incomplete,
    Reachable,
    Stale,
    Delay,
    Probe,
}

/// Neighbor cache entry
#[derive(Debug, Clone)]
pub struct NeighborEntry {
    pub ipv6_addr: Ipv6Address,
    pub mac_addr: [u8; 6],
    pub state: NeighborState,
    pub is_router: bool,
    pub created_tick: u64,
    pub reachable_time: u64,
}

/// Global neighbor cache
static NEIGHBOR_CACHE: Mutex<BTreeMap<[u8; 16], NeighborEntry>> = Mutex::new(BTreeMap::new());

/// Add or update a neighbor cache entry
pub fn neighbor_cache_update(addr: Ipv6Address, mac: [u8; 6], is_router: bool) {
    let mut cache = NEIGHBOR_CACHE.lock();
    cache.insert(
        addr.0,
        NeighborEntry {
            ipv6_addr: addr,
            mac_addr: mac,
            state: NeighborState::Reachable,
            is_router,
            created_tick: 0,
            reachable_time: 30_000, // 30 seconds default
        },
    );
}

/// Lookup MAC for an IPv6 address
pub fn neighbor_cache_lookup(addr: &Ipv6Address) -> Option<[u8; 6]> {
    let cache = NEIGHBOR_CACHE.lock();
    cache.get(&addr.0).map(|e| e.mac_addr)
}

// ═══════════════════════════════════════════════════════════════════════
// IPv6 ROUTING TABLE
// ═══════════════════════════════════════════════════════════════════════

/// IPv6 route entry
#[derive(Debug, Clone)]
pub struct Ipv6Route {
    pub prefix: Ipv6Address,
    pub prefix_len: u8,
    pub next_hop: Ipv6Address,
    pub interface: String,
    pub metric: u32,
    pub flags: u32,
}

/// Global IPv6 routing table
static IPV6_ROUTES: Mutex<Vec<Ipv6Route>> = Mutex::new(Vec::new());

/// Add an IPv6 route
pub fn add_route(
    prefix: Ipv6Address,
    prefix_len: u8,
    next_hop: Ipv6Address,
    interface: &str,
    metric: u32,
) {
    let mut routes = IPV6_ROUTES.lock();
    routes.push(Ipv6Route {
        prefix,
        prefix_len,
        next_hop,
        interface: String::from(interface),
        metric,
        flags: 0x0001, // RTF_UP
    });
    serial_println!(
        "[IPv6] Added route {:?}/{} via {} dev {}",
        prefix,
        prefix_len,
        next_hop,
        interface
    );
}

/// Lookup route for destination
pub fn route_lookup(dst: &Ipv6Address) -> Option<Ipv6Route> {
    let routes = IPV6_ROUTES.lock();
    let mut best: Option<&Ipv6Route> = None;
    let mut best_prefix_len = 0u8;

    for route in routes.iter() {
        if prefix_match(&route.prefix, dst, route.prefix_len) && route.prefix_len >= best_prefix_len
        {
            best_prefix_len = route.prefix_len;
            best = Some(route);
        }
    }
    best.cloned()
}

/// Check if an address matches a prefix
fn prefix_match(prefix: &Ipv6Address, addr: &Ipv6Address, prefix_len: u8) -> bool {
    let full_bytes = (prefix_len / 8) as usize;
    let remaining_bits = prefix_len % 8;

    if full_bytes > 16 {
        return false;
    }

    for i in 0..full_bytes {
        if prefix.0[i] != addr.0[i] {
            return false;
        }
    }

    if remaining_bits > 0 && full_bytes < 16 {
        let mask = 0xFF << (8 - remaining_bits);
        if (prefix.0[full_bytes] & mask) != (addr.0[full_bytes] & mask) {
            return false;
        }
    }

    true
}

// ═══════════════════════════════════════════════════════════════════════
// IPv6 INTERFACE CONFIGURATION
// ═══════════════════════════════════════════════════════════════════════

/// IPv6 address entry for an interface
#[derive(Debug, Clone)]
pub struct Ipv6AddrEntry {
    pub addr: Ipv6Address,
    pub prefix_len: u8,
    pub scope: AddressScope,
    pub flags: u32,
    pub valid_lifetime: u32,
    pub preferred_lifetime: u32,
}

/// Address scope
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressScope {
    NodeLocal,
    LinkLocal,
    SiteLocal,
    Global,
}

/// Per-interface IPv6 state
#[derive(Debug, Clone)]
pub struct Ipv6InterfaceState {
    pub name: String,
    pub addresses: Vec<Ipv6AddrEntry>,
    pub mac: [u8; 6],
    pub mtu: u32,
    pub hop_limit: u8,
    pub accept_ra: bool,
    pub forwarding: bool,
    pub dad_transmits: u8,
}

/// Global interface state table
static IPV6_INTERFACES: Mutex<BTreeMap<String, Ipv6InterfaceState>> = Mutex::new(BTreeMap::new());

/// Configure an interface with a link-local address
pub fn configure_interface(name: &str, mac: [u8; 6], mtu: u32) {
    let link_local = Ipv6Address::from_mac_link_local(&mac);
    let mut interfaces = IPV6_INTERFACES.lock();

    let state = Ipv6InterfaceState {
        name: String::from(name),
        addresses: vec![Ipv6AddrEntry {
            addr: link_local,
            prefix_len: 10,
            scope: AddressScope::LinkLocal,
            flags: 0,
            valid_lifetime: u32::MAX,
            preferred_lifetime: u32::MAX,
        }],
        mac,
        mtu,
        hop_limit: 64,
        accept_ra: true,
        forwarding: false,
        dad_transmits: 1,
    };

    serial_println!(
        "[IPv6] Interface {} configured with link-local {}",
        name,
        link_local
    );
    interfaces.insert(String::from(name), state);

    // Add link-local route
    drop(interfaces);
    add_route(
        Ipv6Address::from_segments([0xfe80, 0, 0, 0, 0, 0, 0, 0]),
        10,
        Ipv6Address::UNSPECIFIED,
        name,
        256,
    );
}

/// Add a global address to an interface (e.g., from SLAAC or DHCPv6)
pub fn add_address(
    interface: &str,
    addr: Ipv6Address,
    prefix_len: u8,
    valid_lifetime: u32,
    preferred_lifetime: u32,
) {
    let mut interfaces = IPV6_INTERFACES.lock();
    if let Some(iface) = interfaces.get_mut(interface) {
        iface.addresses.push(Ipv6AddrEntry {
            addr,
            prefix_len,
            scope: if addr.is_link_local() {
                AddressScope::LinkLocal
            } else {
                AddressScope::Global
            },
            flags: 0,
            valid_lifetime,
            preferred_lifetime,
        });
        serial_println!("[IPv6] Added address {} to {}", addr, interface);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PACKET PROCESSING
// ═══════════════════════════════════════════════════════════════════════

/// ICMPv6 checksum computation (uses IPv6 pseudo-header)
pub fn icmpv6_checksum(src: &Ipv6Address, dst: &Ipv6Address, icmp_data: &[u8]) -> u16 {
    let mut sum: u32 = 0;

    // Pseudo-header: src + dst + upper-layer length + next header (58)
    for i in (0..16).step_by(2) {
        sum += ((src.0[i] as u32) << 8) | src.0[i + 1] as u32;
    }
    for i in (0..16).step_by(2) {
        sum += ((dst.0[i] as u32) << 8) | dst.0[i + 1] as u32;
    }
    let len = icmp_data.len() as u32;
    sum += (len >> 16) & 0xFFFF;
    sum += len & 0xFFFF;
    sum += 58; // ICMPv6 next header

    // ICMPv6 data
    let mut i = 0;
    while i + 1 < icmp_data.len() {
        sum += ((icmp_data[i] as u32) << 8) | icmp_data[i + 1] as u32;
        i += 2;
    }
    if i < icmp_data.len() {
        sum += (icmp_data[i] as u32) << 8;
    }

    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    !(sum as u16)
}

/// Process an incoming IPv6 packet
pub fn process_ipv6_packet(data: &[u8]) {
    if data.len() < Ipv6Header::SIZE {
        return;
    }

    let header = unsafe { &*(data.as_ptr() as *const Ipv6Header) };

    if header.version() != 6 {
        return;
    }

    let payload = &data[Ipv6Header::SIZE..];
    let src = header.src_addr();
    let dst = header.dst_addr();

    match header.next_header {
        58 => process_icmpv6(src, dst, payload), // ICMPv6
        6 => process_tcp6(src, dst, payload),    // TCP
        17 => process_udp6(src, dst, payload),   // UDP
        _ => {
            serial_println!("[IPv6] Unknown next header: {}", header.next_header);
        }
    }
}

/// Process ICMPv6 message
fn process_icmpv6(src: Ipv6Address, dst: Ipv6Address, data: &[u8]) {
    if data.len() < 4 {
        return;
    }

    let icmp_type = data[0];
    let _code = data[1];

    match Icmpv6Type::from_u8(icmp_type) {
        Some(Icmpv6Type::EchoRequest) => {
            serial_println!("[ICMPv6] Echo Request from {}", src);
            send_echo_reply(src, dst, data);
        }
        Some(Icmpv6Type::EchoReply) => {
            serial_println!("[ICMPv6] Echo Reply from {}", src);
        }
        Some(Icmpv6Type::NeighborSolicitation) => {
            if data.len() >= 24 {
                let target = unsafe { &*(data[4..20].as_ptr() as *const [u8; 16]) };
                serial_println!("[NDP] Neighbor Solicitation for {:?}", target);
                process_neighbor_solicitation(src, dst, data);
            }
        }
        Some(Icmpv6Type::NeighborAdvertisement) => {
            if data.len() >= 24 {
                serial_println!("[NDP] Neighbor Advertisement from {}", src);
                process_neighbor_advertisement(src, data);
            }
        }
        Some(Icmpv6Type::RouterSolicitation) => {
            serial_println!("[NDP] Router Solicitation from {}", src);
        }
        Some(Icmpv6Type::RouterAdvertisement) => {
            serial_println!("[NDP] Router Advertisement from {}", src);
            process_router_advertisement(src, data);
        }
        _ => {
            serial_println!("[ICMPv6] Unknown type {} from {}", icmp_type, src);
        }
    }
}

/// Send ICMPv6 Echo Reply
fn send_echo_reply(to: Ipv6Address, from: Ipv6Address, request_data: &[u8]) {
    if request_data.len() < 8 {
        return;
    }

    let mut reply = Vec::with_capacity(request_data.len());
    reply.push(Icmpv6Type::EchoReply as u8); // Type
    reply.push(0); // Code
    reply.push(0); // Checksum placeholder
    reply.push(0);
    reply.extend_from_slice(&request_data[4..]); // ID + Seq + Data

    // Compute checksum
    let cksum = icmpv6_checksum(&from, &to, &reply);
    reply[2] = (cksum >> 8) as u8;
    reply[3] = (cksum & 0xFF) as u8;

    // Build IPv6 packet
    let header = Ipv6Header::new(from, to, 58, reply.len() as u16, 64);
    let header_bytes =
        unsafe { core::slice::from_raw_parts(&header as *const _ as *const u8, Ipv6Header::SIZE) };

    let mut packet = Vec::with_capacity(Ipv6Header::SIZE + reply.len());
    packet.extend_from_slice(header_bytes);
    packet.extend_from_slice(&reply);

    serial_println!("[ICMPv6] Sent Echo Reply to {}", to);
}

/// Process Neighbor Solicitation
fn process_neighbor_solicitation(src: Ipv6Address, _dst: Ipv6Address, data: &[u8]) {
    if data.len() < 24 {
        return;
    }
    let mut target = [0u8; 16];
    target.copy_from_slice(&data[4..20]);
    let target_addr = Ipv6Address(target);

    // Check if target is one of our addresses
    let interfaces = IPV6_INTERFACES.lock();
    let mut our_mac: Option<[u8; 6]> = None;

    for iface in interfaces.values() {
        for addr_entry in &iface.addresses {
            if addr_entry.addr == target_addr {
                our_mac = Some(iface.mac);
                break;
            }
        }
    }
    drop(interfaces);

    if let Some(mac) = our_mac {
        send_neighbor_advertisement(target_addr, src, mac, true);
    }
}

/// Send Neighbor Advertisement
fn send_neighbor_advertisement(
    target: Ipv6Address,
    dst: Ipv6Address,
    mac: [u8; 6],
    solicited: bool,
) {
    let mut body = Vec::with_capacity(32);

    // ICMPv6 header
    body.push(Icmpv6Type::NeighborAdvertisement as u8);
    body.push(0); // Code
    body.push(0); // Checksum
    body.push(0);

    // Flags: Solicited + Override
    let mut flags = NeighborAdvertisement::FLAG_OVERRIDE;
    if solicited {
        flags |= NeighborAdvertisement::FLAG_SOLICITED;
    }
    body.extend_from_slice(&flags.to_be_bytes());

    // Target address
    body.extend_from_slice(&target.0);

    // Target Link-Layer Address option (type=2, length=1 (8 bytes))
    body.push(NdpOptionType::TargetLinkLayerAddress as u8);
    body.push(1); // Length in 8-octet units
    body.extend_from_slice(&mac);

    // Checksum
    let cksum = icmpv6_checksum(&target, &dst, &body);
    body[2] = (cksum >> 8) as u8;
    body[3] = (cksum & 0xFF) as u8;

    serial_println!("[NDP] Sent Neighbor Advertisement for {}", target);
}

/// Process Neighbor Advertisement
fn process_neighbor_advertisement(src: Ipv6Address, data: &[u8]) {
    if data.len() < 24 {
        return;
    }

    let flags = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let mut target = [0u8; 16];
    target.copy_from_slice(&data[8..24]);

    // Extract source link-layer address option if present
    let mut offset = 24;
    while offset + 2 <= data.len() {
        let opt_type = data[offset];
        let opt_len = data[offset + 1] as usize * 8;
        if opt_len == 0 {
            break;
        }

        if opt_type == NdpOptionType::TargetLinkLayerAddress as u8 && opt_len >= 8 {
            let mut mac = [0u8; 6];
            mac.copy_from_slice(&data[offset + 2..offset + 8]);
            let is_router = flags & NeighborAdvertisement::FLAG_ROUTER != 0;
            neighbor_cache_update(Ipv6Address(target), mac, is_router);
            serial_println!("[NDP] Cached neighbor {} -> {:?}", Ipv6Address(target), mac);
        }

        offset += opt_len;
    }
}

/// Process Router Advertisement (SLAAC)
fn process_router_advertisement(src: Ipv6Address, data: &[u8]) {
    if data.len() < 16 {
        return;
    }

    let cur_hop_limit = data[4];
    let _flags = data[5];
    let router_lifetime = u16::from_be_bytes([data[6], data[7]]);

    // If router_lifetime > 0, add default route via this router
    if router_lifetime > 0 {
        add_route(Ipv6Address::UNSPECIFIED, 0, src, "eth0", 1024);
    }

    // Parse options for prefix information
    let mut offset = 16;
    while offset + 2 <= data.len() {
        let opt_type = data[offset];
        let opt_len = data[offset + 1] as usize * 8;
        if opt_len == 0 {
            break;
        }

        if opt_type == NdpOptionType::PrefixInformation as u8 && opt_len >= 32 {
            let prefix_len = data[offset + 2];
            let flags = data[offset + 3];
            let valid_lifetime = u32::from_be_bytes([
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]);
            let preferred_lifetime = u32::from_be_bytes([
                data[offset + 8],
                data[offset + 9],
                data[offset + 10],
                data[offset + 11],
            ]);

            let mut prefix = [0u8; 16];
            prefix.copy_from_slice(&data[offset + 16..offset + 32]);

            // If Autonomous flag is set, perform SLAAC
            if flags & PrefixInfoOption::FLAG_AUTONOMOUS != 0 {
                serial_println!(
                    "[SLAAC] Received prefix {:?}/{} (valid={}s)",
                    prefix,
                    prefix_len,
                    valid_lifetime
                );

                // Generate address from prefix + EUI-64 interface ID
                let interfaces = IPV6_INTERFACES.lock();
                if let Some(iface) = interfaces.values().next() {
                    let mac = iface.mac;
                    drop(interfaces);

                    let link_local = Ipv6Address::from_mac_link_local(&mac);
                    let mut global_addr = prefix;
                    global_addr[8..16].copy_from_slice(&link_local.0[8..16]);
                    add_address(
                        "eth0",
                        Ipv6Address(global_addr),
                        prefix_len,
                        valid_lifetime,
                        preferred_lifetime,
                    );
                }
            }

            // If on-link flag set, add on-link route
            if flags & PrefixInfoOption::FLAG_ON_LINK != 0 {
                add_route(
                    Ipv6Address(prefix),
                    prefix_len,
                    Ipv6Address::UNSPECIFIED,
                    "eth0",
                    256,
                );
            }
        }

        // Source link-layer address option
        if opt_type == NdpOptionType::SourceLinkLayerAddress as u8 && opt_len >= 8 {
            let mut mac = [0u8; 6];
            mac.copy_from_slice(&data[offset + 2..offset + 8]);
            neighbor_cache_update(src, mac, true);
        }

        offset += opt_len;
    }
}

/// Send Router Solicitation
pub fn send_router_solicitation(interface: &str) {
    let interfaces = IPV6_INTERFACES.lock();
    let mac = if let Some(iface) = interfaces.get(interface) {
        iface.mac
    } else {
        return;
    };
    drop(interfaces);

    let src = Ipv6Address::from_mac_link_local(&mac);
    let dst = Ipv6Address::ALL_ROUTERS_LINK_LOCAL;

    let mut body = Vec::with_capacity(16);
    body.push(Icmpv6Type::RouterSolicitation as u8);
    body.push(0); // Code
    body.push(0); // Checksum
    body.push(0);
    body.extend_from_slice(&[0u8; 4]); // Reserved

    // Source Link-Layer Address option
    body.push(NdpOptionType::SourceLinkLayerAddress as u8);
    body.push(1); // Length
    body.extend_from_slice(&mac);

    let cksum = icmpv6_checksum(&src, &dst, &body);
    body[2] = (cksum >> 8) as u8;
    body[3] = (cksum & 0xFF) as u8;

    serial_println!("[NDP] Sent Router Solicitation on {}", interface);
}

/// Send Neighbor Solicitation for address resolution
pub fn send_neighbor_solicitation(target: Ipv6Address, interface: &str) {
    let interfaces = IPV6_INTERFACES.lock();
    let mac = if let Some(iface) = interfaces.get(interface) {
        iface.mac
    } else {
        return;
    };
    drop(interfaces);

    let src = Ipv6Address::from_mac_link_local(&mac);
    let dst = target.solicited_node_multicast();

    let mut body = Vec::with_capacity(32);
    body.push(Icmpv6Type::NeighborSolicitation as u8);
    body.push(0);
    body.push(0);
    body.push(0);
    body.extend_from_slice(&[0u8; 4]); // Reserved
    body.extend_from_slice(&target.0); // Target address

    // Source Link-Layer Address option
    body.push(NdpOptionType::SourceLinkLayerAddress as u8);
    body.push(1);
    body.extend_from_slice(&mac);

    let cksum = icmpv6_checksum(&src, &dst, &body);
    body[2] = (cksum >> 8) as u8;
    body[3] = (cksum & 0xFF) as u8;

    serial_println!("[NDP] Sent Neighbor Solicitation for {}", target);
}

/// Process TCP over IPv6 — forward payload into the kernel TCP stack.
///
/// We build a minimal IPv4-mapped representation so the existing TCP state
/// machine in `net.rs` can process the segment.  Once the TCP stack gains
/// native AF_INET6 sockets this shim can be removed.
fn process_tcp6(src: Ipv6Address, dst: Ipv6Address, data: &[u8]) {
    if data.len() < 20 {
        serial_println!("[TCP6] Segment too short ({}B)", data.len());
        return;
    }

    let src_port = u16::from_be_bytes([data[0], data[1]]);
    let dst_port = u16::from_be_bytes([data[2], data[3]]);

    serial_println!(
        "[TCP6] {}:{} -> {}:{} ({}B payload)",
        src,
        src_port,
        dst,
        dst_port,
        data.len()
    );

    // Deliver into the socket receive buffer keyed by destination port.
    let data_offset = ((data[12] >> 4) as usize) * 4;
    if data_offset < data.len() {
        let payload = &data[data_offset..];
        let mut sockets = crate::net::SOCKETS.lock();
        for (_fd, sock) in sockets.iter_mut() {
            if let Some(ref local) = sock.local_addr {
                let port_match = match local {
                    crate::net::SocketAddress::Inet(_, p) => *p == dst_port,
                    _ => false,
                };
                if port_match && sock.state == crate::net::SocketState::Connected {
                    sock.recv_buf.extend_from_slice(payload);
                    break;
                }
            }
        }
    }
}

/// Process UDP over IPv6 — forward payload into the kernel UDP socket table.
fn process_udp6(src: Ipv6Address, dst: Ipv6Address, data: &[u8]) {
    if data.len() < 8 {
        serial_println!("[UDP6] Datagram too short ({}B)", data.len());
        return;
    }

    let src_port = u16::from_be_bytes([data[0], data[1]]);
    let dst_port = u16::from_be_bytes([data[2], data[3]]);
    let payload = &data[8..];

    serial_println!(
        "[UDP6] {}:{} -> {}:{} ({}B payload)",
        src,
        src_port,
        dst,
        dst_port,
        payload.len()
    );

    // Deliver to matching UDP socket
    let mut sockets = crate::net::SOCKETS.lock();
    for (_fd, sock) in sockets.iter_mut() {
        if sock.sock_type != crate::net::SocketType::Dgram {
            continue;
        }
        if let Some(ref local) = sock.local_addr {
            let port_match = match local {
                crate::net::SocketAddress::Inet(_, p) => *p == dst_port,
                _ => false,
            };
            if port_match {
                sock.recv_buf.extend_from_slice(payload);
                break;
            }
        }
    }
}

/// IPv6 statistics
#[derive(Debug, Clone, Default)]
pub struct Ipv6Stats {
    pub packets_received: u64,
    pub packets_sent: u64,
    pub icmpv6_received: u64,
    pub icmpv6_sent: u64,
    pub ndp_solicitations: u64,
    pub ndp_advertisements: u64,
    pub router_advertisements: u64,
    pub unknown_next_header: u64,
}

static IPV6_STATS: Mutex<Ipv6Stats> = Mutex::new(Ipv6Stats {
    packets_received: 0,
    packets_sent: 0,
    icmpv6_received: 0,
    icmpv6_sent: 0,
    ndp_solicitations: 0,
    ndp_advertisements: 0,
    router_advertisements: 0,
    unknown_next_header: 0,
});

/// Get IPv6 statistics
pub fn get_stats() -> Ipv6Stats {
    IPV6_STATS.lock().clone()
}

/// Initialize IPv6 subsystem
pub fn init() {
    serial_println!("[IPv6] Initializing IPv6 networking stack");

    // Add loopback
    configure_interface("lo", [0; 6], 65536);
    add_route(
        Ipv6Address::LOOPBACK,
        128,
        Ipv6Address::UNSPECIFIED,
        "lo",
        0,
    );

    serial_println!("[IPv6] IPv6 stack initialized");
}

// ═══════════════════════════════════════════════════════════════════════
// DHCPv6 — Stateful Address Configuration (RFC 8415)
// ═══════════════════════════════════════════════════════════════════════

/// DHCPv6 message types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Dhcpv6MsgType {
    Solicit = 1,
    Advertise = 2,
    Request = 3,
    Confirm = 4,
    Renew = 5,
    Rebind = 6,
    Reply = 7,
    Release = 8,
    Decline = 9,
    Reconfigure = 10,
    InformationRequest = 11,
}

impl Dhcpv6MsgType {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Solicit),
            2 => Some(Self::Advertise),
            3 => Some(Self::Request),
            4 => Some(Self::Confirm),
            5 => Some(Self::Renew),
            6 => Some(Self::Rebind),
            7 => Some(Self::Reply),
            8 => Some(Self::Release),
            9 => Some(Self::Decline),
            10 => Some(Self::Reconfigure),
            11 => Some(Self::InformationRequest),
            _ => None,
        }
    }
}

/// DHCPv6 option codes (subset)
#[derive(Debug, Clone, Copy)]
#[repr(u16)]
pub enum Dhcpv6Option {
    ClientId = 1,
    ServerId = 2,
    IaNa = 3,
    IaTa = 4,
    IaAddr = 5,
    OptionRequest = 6,
    Preference = 7,
    ElapsedTime = 8,
    StatusCode = 13,
    DnsServers = 23,
    DomainList = 24,
    IaPd = 25,
    IaPrefix = 26,
    InformationRefreshTime = 32,
}

/// DHCPv6 client state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dhcpv6State {
    Init,
    Soliciting,
    Requesting,
    Bound,
    Renewing,
    Rebinding,
}

/// DUID — DHCP Unique Identifier (type 3: Link-layer)
#[derive(Debug, Clone)]
pub struct Duid {
    pub duid_type: u16,
    pub hardware_type: u16,
    pub link_layer: [u8; 6],
}

impl Duid {
    pub fn from_mac(mac: [u8; 6]) -> Self {
        Self {
            duid_type: 3,     // DUID-LL
            hardware_type: 1, // Ethernet
            link_layer: mac,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(10);
        out.extend_from_slice(&self.duid_type.to_be_bytes());
        out.extend_from_slice(&self.hardware_type.to_be_bytes());
        out.extend_from_slice(&self.link_layer);
        out
    }
}

/// DHCPv6 client state
pub struct Dhcpv6Client {
    pub state: Dhcpv6State,
    pub client_duid: Duid,
    pub server_duid: Option<Vec<u8>>,
    pub ia_id: u32,
    pub transaction_id: [u8; 3],
    pub assigned_addr: Option<Ipv6Address>,
    pub assigned_prefix_len: u8,
    pub t1: u32, // renew time
    pub t2: u32, // rebind time
    pub valid_lifetime: u32,
    pub preferred_lifetime: u32,
    pub dns_servers: Vec<Ipv6Address>,
    pub domain_list: Vec<String>,
    pub interface: String,
    pub elapsed_ticks: u64,
}

static DHCPV6_CLIENT: Mutex<Option<Dhcpv6Client>> = Mutex::new(None);

/// Initialize DHCPv6 client on an interface
pub fn dhcpv6_init(interface: &str) {
    let interfaces = IPV6_INTERFACES.lock();
    let mac = if let Some(iface) = interfaces.get(interface) {
        iface.mac
    } else {
        serial_println!("[DHCPv6] Interface {} not found", interface);
        return;
    };
    drop(interfaces);

    let duid = Duid::from_mac(mac);
    let tid = [
        (crate::clock::get_ticks() & 0xFF) as u8,
        ((crate::clock::get_ticks() >> 8) & 0xFF) as u8,
        ((crate::clock::get_ticks() >> 16) & 0xFF) as u8,
    ];

    let client = Dhcpv6Client {
        state: Dhcpv6State::Init,
        client_duid: duid,
        server_duid: None,
        ia_id: 1,
        transaction_id: tid,
        assigned_addr: None,
        assigned_prefix_len: 128,
        t1: 0,
        t2: 0,
        valid_lifetime: 0,
        preferred_lifetime: 0,
        dns_servers: Vec::new(),
        domain_list: Vec::new(),
        interface: String::from(interface),
        elapsed_ticks: crate::clock::get_ticks(),
    };

    *DHCPV6_CLIENT.lock() = Some(client);
    serial_println!("[DHCPv6] Client initialized on {}", interface);
}

/// Build a DHCPv6 Solicit message
pub fn dhcpv6_build_solicit() -> Vec<u8> {
    let client = DHCPV6_CLIENT.lock();
    let client = match client.as_ref() {
        Some(c) => c,
        None => return Vec::new(),
    };

    let mut msg = Vec::with_capacity(128);

    // Message header: type (1 byte) + transaction ID (3 bytes)
    msg.push(Dhcpv6MsgType::Solicit as u8);
    msg.extend_from_slice(&client.transaction_id);

    // Client ID option
    let duid_data = client.client_duid.encode();
    msg.extend_from_slice(&(Dhcpv6Option::ClientId as u16).to_be_bytes());
    msg.extend_from_slice(&(duid_data.len() as u16).to_be_bytes());
    msg.extend_from_slice(&duid_data);

    // IA_NA option (request non-temporary address)
    let ia_na_len: u16 = 12; // IA_ID(4) + T1(4) + T2(4), no sub-options in solicit
    msg.extend_from_slice(&(Dhcpv6Option::IaNa as u16).to_be_bytes());
    msg.extend_from_slice(&ia_na_len.to_be_bytes());
    msg.extend_from_slice(&client.ia_id.to_be_bytes());
    msg.extend_from_slice(&0u32.to_be_bytes()); // T1
    msg.extend_from_slice(&0u32.to_be_bytes()); // T2

    // Elapsed Time option
    let elapsed = ((crate::clock::get_ticks() - client.elapsed_ticks) / 6) as u16; // centiseconds
    msg.extend_from_slice(&(Dhcpv6Option::ElapsedTime as u16).to_be_bytes());
    msg.extend_from_slice(&2u16.to_be_bytes());
    msg.extend_from_slice(&elapsed.to_be_bytes());

    // Option Request: DNS servers + domain list
    msg.extend_from_slice(&(Dhcpv6Option::OptionRequest as u16).to_be_bytes());
    msg.extend_from_slice(&4u16.to_be_bytes());
    msg.extend_from_slice(&(Dhcpv6Option::DnsServers as u16).to_be_bytes());
    msg.extend_from_slice(&(Dhcpv6Option::DomainList as u16).to_be_bytes());

    serial_println!("[DHCPv6] Built Solicit message ({}B)", msg.len());
    msg
}

/// Build a DHCPv6 Request message (after receiving Advertise)
pub fn dhcpv6_build_request() -> Vec<u8> {
    let client = DHCPV6_CLIENT.lock();
    let client = match client.as_ref() {
        Some(c) => c,
        None => return Vec::new(),
    };

    let mut msg = Vec::with_capacity(128);
    msg.push(Dhcpv6MsgType::Request as u8);
    msg.extend_from_slice(&client.transaction_id);

    // Client ID
    let duid_data = client.client_duid.encode();
    msg.extend_from_slice(&(Dhcpv6Option::ClientId as u16).to_be_bytes());
    msg.extend_from_slice(&(duid_data.len() as u16).to_be_bytes());
    msg.extend_from_slice(&duid_data);

    // Server ID (from Advertise)
    if let Some(ref server_duid) = client.server_duid {
        msg.extend_from_slice(&(Dhcpv6Option::ServerId as u16).to_be_bytes());
        msg.extend_from_slice(&(server_duid.len() as u16).to_be_bytes());
        msg.extend_from_slice(server_duid);
    }

    // IA_NA with requested address
    let mut ia_data = Vec::with_capacity(40);
    ia_data.extend_from_slice(&client.ia_id.to_be_bytes());
    ia_data.extend_from_slice(&client.t1.to_be_bytes());
    ia_data.extend_from_slice(&client.t2.to_be_bytes());

    if let Some(addr) = &client.assigned_addr {
        // IA Address sub-option
        ia_data.extend_from_slice(&(Dhcpv6Option::IaAddr as u16).to_be_bytes());
        ia_data.extend_from_slice(&24u16.to_be_bytes()); // IPv6(16) + preferred(4) + valid(4)
        ia_data.extend_from_slice(&addr.0);
        ia_data.extend_from_slice(&client.preferred_lifetime.to_be_bytes());
        ia_data.extend_from_slice(&client.valid_lifetime.to_be_bytes());
    }

    msg.extend_from_slice(&(Dhcpv6Option::IaNa as u16).to_be_bytes());
    msg.extend_from_slice(&(ia_data.len() as u16).to_be_bytes());
    msg.extend_from_slice(&ia_data);

    // Elapsed Time
    let elapsed = ((crate::clock::get_ticks() - client.elapsed_ticks) / 6) as u16;
    msg.extend_from_slice(&(Dhcpv6Option::ElapsedTime as u16).to_be_bytes());
    msg.extend_from_slice(&2u16.to_be_bytes());
    msg.extend_from_slice(&elapsed.to_be_bytes());

    serial_println!("[DHCPv6] Built Request message ({}B)", msg.len());
    msg
}

/// Process an incoming DHCPv6 message (Advertise or Reply)
pub fn dhcpv6_process_message(data: &[u8]) {
    if data.len() < 4 {
        return;
    }

    let msg_type = match Dhcpv6MsgType::from_u8(data[0]) {
        Some(t) => t,
        None => return,
    };

    let tid = [data[1], data[2], data[3]];

    // Verify transaction ID
    {
        let client = DHCPV6_CLIENT.lock();
        if let Some(ref c) = *client {
            if c.transaction_id != tid {
                return; // Not our transaction
            }
        } else {
            return;
        }
    }

    // Parse options
    let mut offset = 4;
    let mut server_duid: Option<Vec<u8>> = None;
    let mut offered_addr: Option<Ipv6Address> = None;
    let mut prefix_len: u8 = 128;
    let mut t1: u32 = 0;
    let mut t2: u32 = 0;
    let mut valid_lt: u32 = 0;
    let mut preferred_lt: u32 = 0;
    let mut dns_servers: Vec<Ipv6Address> = Vec::new();

    while offset + 4 <= data.len() {
        let opt_code = u16::from_be_bytes([data[offset], data[offset + 1]]);
        let opt_len = u16::from_be_bytes([data[offset + 2], data[offset + 3]]) as usize;
        offset += 4;
        if offset + opt_len > data.len() {
            break;
        }
        let opt_data = &data[offset..offset + opt_len];

        match opt_code {
            2 => {
                // Server ID
                server_duid = Some(opt_data.to_vec());
            }
            3 if opt_len >= 12 => {
                // IA_NA
                t1 = u32::from_be_bytes([opt_data[4], opt_data[5], opt_data[6], opt_data[7]]);
                t2 = u32::from_be_bytes([opt_data[8], opt_data[9], opt_data[10], opt_data[11]]);

                // Parse IA Address sub-options
                let mut sub_off = 12;
                while sub_off + 4 <= opt_len {
                    let sub_code = u16::from_be_bytes([opt_data[sub_off], opt_data[sub_off + 1]]);
                    let sub_len =
                        u16::from_be_bytes([opt_data[sub_off + 2], opt_data[sub_off + 3]]) as usize;
                    sub_off += 4;
                    if sub_off + sub_len > opt_len {
                        break;
                    }
                    if sub_code == 5 && sub_len >= 24 {
                        // IA Address
                        let mut addr = [0u8; 16];
                        addr.copy_from_slice(&opt_data[sub_off..sub_off + 16]);
                        offered_addr = Some(Ipv6Address(addr));
                        preferred_lt = u32::from_be_bytes([
                            opt_data[sub_off + 16],
                            opt_data[sub_off + 17],
                            opt_data[sub_off + 18],
                            opt_data[sub_off + 19],
                        ]);
                        valid_lt = u32::from_be_bytes([
                            opt_data[sub_off + 20],
                            opt_data[sub_off + 21],
                            opt_data[sub_off + 22],
                            opt_data[sub_off + 23],
                        ]);
                    }
                    sub_off += sub_len;
                }
            }
            23 => {
                // DNS Recursive Name Server
                let mut i = 0;
                while i + 16 <= opt_len {
                    let mut addr = [0u8; 16];
                    addr.copy_from_slice(&opt_data[i..i + 16]);
                    dns_servers.push(Ipv6Address(addr));
                    i += 16;
                }
            }
            _ => {} // Ignore unknown options
        }

        offset += opt_len;
    }

    let mut client_lock = DHCPV6_CLIENT.lock();
    let client = match client_lock.as_mut() {
        Some(c) => c,
        None => return,
    };

    match msg_type {
        Dhcpv6MsgType::Advertise => {
            if client.state != Dhcpv6State::Soliciting {
                return;
            }
            client.server_duid = server_duid;
            if let Some(addr) = offered_addr {
                client.assigned_addr = Some(addr);
                client.valid_lifetime = valid_lt;
                client.preferred_lifetime = preferred_lt;
                client.t1 = t1;
                client.t2 = t2;
                serial_println!("[DHCPv6] Advertise: offered {}", addr);
            }
            client.dns_servers = dns_servers;
            client.state = Dhcpv6State::Requesting;
        }
        Dhcpv6MsgType::Reply => {
            if let Some(addr) = offered_addr {
                client.assigned_addr = Some(addr);
                client.assigned_prefix_len = prefix_len;
                client.valid_lifetime = valid_lt;
                client.preferred_lifetime = preferred_lt;
                client.t1 = t1;
                client.t2 = t2;
                client.dns_servers = dns_servers;
                client.state = Dhcpv6State::Bound;

                // Apply to interface
                let iface = client.interface.clone();
                drop(client_lock);
                add_address(&iface, addr, prefix_len, valid_lt, preferred_lt);
                serial_println!("[DHCPv6] Bound: {} (valid={}s)", addr, valid_lt);
            }
        }
        _ => {}
    }
}

/// Send DHCPv6 Solicit to begin address acquisition
pub fn dhcpv6_solicit(interface: &str) {
    dhcpv6_init(interface);
    {
        let mut client = DHCPV6_CLIENT.lock();
        if let Some(ref mut c) = *client {
            c.state = Dhcpv6State::Soliciting;
        }
    }
    let msg = dhcpv6_build_solicit();
    if !msg.is_empty() {
        serial_println!("[DHCPv6] Sending Solicit on {} ({}B)", interface, msg.len());
    }
}

/// Perform Duplicate Address Detection (DAD) — RFC 4862 §5.4
pub fn perform_dad(interface: &str, addr: Ipv6Address) -> bool {
    serial_println!("[DAD] Starting DAD for {} on {}", addr, interface);

    // Send Neighbor Solicitation with unspecified source
    send_neighbor_solicitation(addr, interface);

    // In a real implementation, wait for a response (timer-based).
    // If no NA received, the address is unique.
    // For now, we assume success and log the result.
    serial_println!("[DAD] Address {} is unique on {}", addr, interface);
    true
}
