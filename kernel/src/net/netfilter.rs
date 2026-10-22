/// netfilter — Enhanced network filtering framework
/// Extends the basic firewall with connection tracking,
/// NAT (Network Address Translation), and packet mangling
///
/// Hooks: PREROUTING, INPUT, FORWARD, OUTPUT, POSTROUTING
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Netfilter hook points (matching Linux nf_inet_hooks)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NfHook {
    PreRouting = 0,
    LocalIn = 1,
    Forward = 2,
    LocalOut = 3,
    PostRouting = 4,
}

/// Netfilter verdict
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NfVerdict {
    Accept,
    Drop,
    Stolen,
    Queue,
    Repeat,
}

/// Connection tracking state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConnState {
    New,
    Established,
    Related,
    Invalid,
}

/// Connection tracking entry
#[derive(Debug, Clone)]
pub struct ConnTrackEntry {
    pub src_ip: [u8; 4],
    pub dst_ip: [u8; 4],
    pub src_port: u16,
    pub dst_port: u16,
    pub protocol: u8,
    pub state: ConnState,
    pub packets: u64,
    pub bytes: u64,
    pub timeout: u64, // In ticks
    pub nat_ip: Option<[u8; 4]>,
    pub nat_port: Option<u16>,
}

/// Connection tracking key
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ConnKey {
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    src_port: u16,
    dst_port: u16,
    protocol: u8,
}

/// NAT rule
#[derive(Debug, Clone)]
pub struct NatRule {
    pub hook: NfHook,
    pub match_src_ip: Option<[u8; 4]>,
    pub match_dst_ip: Option<[u8; 4]>,
    pub match_dst_port: Option<u16>,
    pub nat_type: NatType,
    pub nat_ip: [u8; 4],
    pub nat_port: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NatType {
    Snat, // Source NAT (MASQUERADE)
    Dnat, // Destination NAT (port forwarding)
}

/// Netfilter hook callback
pub type HookFn = fn(hook: NfHook, packet: &mut [u8]) -> NfVerdict;

/// Registered hook
#[derive(Clone)]
struct RegisteredHook {
    priority: i32,
    callback: HookFn,
}

/// Global connection tracking table
lazy_static::lazy_static! {
    static ref CONNTRACK: Mutex<BTreeMap<ConnKey, ConnTrackEntry>> = Mutex::new(BTreeMap::new());
    static ref NAT_RULES: Mutex<Vec<NatRule>> = Mutex::new(Vec::new());
    static ref HOOKS: Mutex<BTreeMap<NfHook, Vec<RegisteredHook>>> = Mutex::new(BTreeMap::new());
    static ref CONNTRACK_STATS: Mutex<ConnTrackStats> = Mutex::new(ConnTrackStats::default());
}

#[derive(Debug, Default)]
struct ConnTrackStats {
    entries: u64,
    searched: u64,
    found: u64,
    new: u64,
    invalid: u64,
    delete: u64,
    insert: u64,
}

/// Register a netfilter hook
pub fn register_hook(hook: NfHook, priority: i32, callback: HookFn) {
    let mut hooks = HOOKS.lock();
    let hook_list = hooks.entry(hook).or_default();
    hook_list.push(RegisteredHook { priority, callback });
    // Sort by priority (lower = higher priority)
    hook_list.sort_by_key(|h| h.priority);
}

/// Run all hooks for a given hook point
pub fn run_hooks(hook: NfHook, packet: &mut [u8]) -> NfVerdict {
    let hooks = HOOKS.lock();
    if let Some(hook_list) = hooks.get(&hook) {
        for registered in hook_list {
            match (registered.callback)(hook, packet) {
                NfVerdict::Accept => continue,
                verdict => return verdict,
            }
        }
    }
    NfVerdict::Accept
}

/// Look up connection tracking state for a packet
pub fn conntrack_lookup(
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    src_port: u16,
    dst_port: u16,
    protocol: u8,
) -> ConnState {
    let key = ConnKey {
        src_ip,
        dst_ip,
        src_port,
        dst_port,
        protocol,
    };

    let mut stats = CONNTRACK_STATS.lock();
    stats.searched += 1;

    let mut ct = CONNTRACK.lock();

    // Check forward direction
    if let Some(entry) = ct.get_mut(&key) {
        stats.found += 1;
        entry.packets += 1;
        return entry.state;
    }

    // Check reverse direction (reply)
    let reply_key = ConnKey {
        src_ip: dst_ip,
        dst_ip: src_ip,
        src_port: dst_port,
        dst_port: src_port,
        protocol,
    };

    if let Some(entry) = ct.get_mut(&reply_key) {
        stats.found += 1;
        entry.packets += 1;
        if entry.state == ConnState::New {
            entry.state = ConnState::Established;
        }
        return ConnState::Established;
    }

    // New connection
    stats.new += 1;
    let entry = ConnTrackEntry {
        src_ip,
        dst_ip,
        src_port,
        dst_port,
        protocol,
        state: ConnState::New,
        packets: 1,
        bytes: 0,
        timeout: 300, // 5 minute default timeout
        nat_ip: None,
        nat_port: None,
    };
    ct.insert(key, entry);
    stats.entries += 1;

    ConnState::New
}

/// Add a NAT rule
pub fn add_nat_rule(rule: NatRule) {
    NAT_RULES.lock().push(rule);
}

/// IPv4 header checksum (RFC 1071). `hdr` is the header with checksum field zeroed.
pub fn ipv4_checksum(hdr: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < hdr.len() {
        sum += u16::from_be_bytes([hdr[i], hdr[i + 1]]) as u32;
        i += 2;
    }
    if i < hdr.len() {
        sum += (hdr[i] as u32) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

fn rewrite_ipv4_checksum(packet: &mut [u8]) {
    if packet.len() < 20 {
        return;
    }
    let ihl = ((packet[0] & 0x0f) as usize) * 4;
    if ihl < 20 || packet.len() < ihl {
        return;
    }
    packet[10] = 0;
    packet[11] = 0;
    let csum = ipv4_checksum(&packet[..ihl]);
    packet[10] = (csum >> 8) as u8;
    packet[11] = (csum & 0xff) as u8;
}

/// Apply SNAT (masquerade) — rewrite source address and refresh the IPv4 checksum.
pub fn apply_snat(packet: &mut [u8], nat_ip: [u8; 4]) -> bool {
    if packet.len() < 20 {
        return false;
    }
    // Rewrite source IP in IP header (bytes 12-15)
    packet[12..16].copy_from_slice(&nat_ip);
    rewrite_ipv4_checksum(packet);
    true
}

/// Apply DNAT (port forward) — rewrite destination address
pub fn apply_dnat(packet: &mut [u8], nat_ip: [u8; 4], nat_port: Option<u16>) -> bool {
    if packet.len() < 20 {
        return false;
    }
    // Rewrite destination IP in IP header (bytes 16-19)
    packet[16..20].copy_from_slice(&nat_ip);

    // Rewrite destination port if specified (TCP/UDP header starts at byte 20+)
    if let Some(port) = nat_port {
        let ihl = ((packet[0] & 0x0f) as usize) * 4;
        if packet.len() > ihl + 4 {
            packet[ihl + 2] = (port >> 8) as u8;
            packet[ihl + 3] = (port & 0xff) as u8;
        }
    }

    true
}

/// Process packet through NAT rules
pub fn process_nat(hook: NfHook, packet: &mut [u8]) -> NfVerdict {
    if packet.len() < 20 {
        return NfVerdict::Accept;
    }

    let rules = NAT_RULES.lock();
    for rule in rules.iter() {
        if rule.hook != hook {
            continue;
        }

        // Check if packet matches rule
        let src_ip = [packet[12], packet[13], packet[14], packet[15]];
        let dst_ip = [packet[16], packet[17], packet[18], packet[19]];

        let src_match = rule.match_src_ip.is_none_or(|ip| ip == src_ip);
        let dst_match = rule.match_dst_ip.is_none_or(|ip| ip == dst_ip);

        if src_match && dst_match {
            match rule.nat_type {
                NatType::Snat => {
                    apply_snat(packet, rule.nat_ip);
                }
                NatType::Dnat => {
                    apply_dnat(packet, rule.nat_ip, rule.nat_port);
                    rewrite_ipv4_checksum(packet);
                }
            }
            return NfVerdict::Accept;
        }
    }

    NfVerdict::Accept
}

/// Get connection tracking statistics
pub fn get_conntrack_stats() -> (u64, u64, u64) {
    let stats = CONNTRACK_STATS.lock();
    (stats.entries, stats.searched, stats.found)
}

/// List all tracked connections
pub fn list_connections() -> Vec<ConnTrackEntry> {
    CONNTRACK.lock().values().cloned().collect()
}

/// Flush all connection tracking entries
pub fn flush_conntrack() {
    CONNTRACK.lock().clear();
    let mut stats = CONNTRACK_STATS.lock();
    stats.entries = 0;
}

/// Expire old connections
pub fn expire_connections() {
    let mut ct = CONNTRACK.lock();
    let mut to_remove = Vec::new();

    for (key, entry) in ct.iter_mut() {
        if entry.timeout > 0 {
            entry.timeout -= 1;
        }
        if entry.timeout == 0 {
            to_remove.push(key.clone());
        }
    }

    for key in to_remove {
        ct.remove(&key);
        CONNTRACK_STATS.lock().delete += 1;
    }
}

pub fn init() {
    // Register default hooks

    // Connection tracking in PREROUTING
    register_hook(NfHook::PreRouting, -200, |_hook, packet| {
        if packet.len() >= 20 {
            let src_ip = [packet[12], packet[13], packet[14], packet[15]];
            let dst_ip = [packet[16], packet[17], packet[18], packet[19]];
            let protocol = packet[9];
            let ihl = ((packet[0] & 0x0f) as usize) * 4;
            let (src_port, dst_port) = if packet.len() > ihl + 4 {
                (
                    u16::from_be_bytes([packet[ihl], packet[ihl + 1]]),
                    u16::from_be_bytes([packet[ihl + 2], packet[ihl + 3]]),
                )
            } else {
                (0, 0)
            };
            conntrack_lookup(src_ip, dst_ip, src_port, dst_port, protocol);
        }
        NfVerdict::Accept
    });

    // NAT in POSTROUTING
    register_hook(NfHook::PostRouting, 100, |hook, packet| {
        process_nat(hook, packet)
    });

    serial_println!("[KnoxOS] Netfilter framework initialized (conntrack + NAT)");
    let _ = nat_self_test();
}

pub const GATE_NAT_MARKER: &str = "GATE_NAT masquerade";

fn nat_self_test() -> bool {
    add_nat_rule(NatRule {
        hook: NfHook::PostRouting,
        match_src_ip: Some([10, 0, 0, 2]),
        match_dst_ip: None,
        match_dst_port: None,
        nat_type: NatType::Snat,
        nat_ip: [10, 0, 2, 15],
        nat_port: None,
    });
    let mut pkt = [
        0x45u8, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x40, 0x01, 0x00, 0x00, 10, 0, 0, 2, 1,
        1, 1, 1,
    ];
    rewrite_ipv4_checksum(&mut pkt);
    let _ = process_nat(NfHook::PostRouting, &mut pkt);
    let src = [pkt[12], pkt[13], pkt[14], pkt[15]];
    let mut chk = pkt;
    chk[10] = 0;
    chk[11] = 0;
    let expect = ipv4_checksum(&chk[..20]);
    let got = u16::from_be_bytes([pkt[10], pkt[11]]);
    if src == [10, 0, 2, 15] && got == expect {
        serial_println!("[netfilter] {} (10.0.0.2 → 10.0.2.15)", GATE_NAT_MARKER);
        true
    } else {
        serial_println!(
            "[netfilter] GATE_NAT FAILED src={:?} csum={:#06x}",
            src,
            got
        );
        false
    }
}
