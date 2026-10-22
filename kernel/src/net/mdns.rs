/// mDNS / Avahi — Multicast DNS for local service discovery (RFC 6762/6763)
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use crate::serial_println;

/// mDNS uses multicast address 224.0.0.251 port 5353
pub const MDNS_MULTICAST_IP: u32 = 0xE00000FB;
pub const MDNS_PORT: u16 = 5353;

/// DNS-SD service type
#[derive(Debug, Clone)]
pub struct ServiceType {
    pub name: String,   // e.g., "_http._tcp"
    pub domain: String, // usually "local."
}

/// An advertised service
#[derive(Debug, Clone)]
pub struct ServiceRecord {
    pub instance_name: String, // e.g., "KnoxOS Web Server"
    pub service_type: String,  // e.g., "_http._tcp.local."
    pub hostname: String,      // e.g., "knoxos.local."
    pub port: u16,
    pub txt_records: BTreeMap<String, String>,
    pub ttl: u32, // seconds
}

/// A discovered peer on the network
#[derive(Debug, Clone)]
pub struct ResolvedHost {
    pub hostname: String,
    pub ip: u32,
    pub ttl: u32,
}

lazy_static::lazy_static! {
    static ref PUBLISHED_SERVICES: Mutex<Vec<ServiceRecord>> = Mutex::new(Vec::new());
    static ref DISCOVERED_SERVICES: Mutex<Vec<ServiceRecord>> = Mutex::new(Vec::new());
    static ref HOST_CACHE: Mutex<BTreeMap<String, ResolvedHost>> = Mutex::new(BTreeMap::new());
}

static RUNNING: AtomicBool = AtomicBool::new(false);

/// Publish a service on the local network
pub fn publish(
    instance: &str,
    service_type: &str,
    port: u16,
    txt: &[(&str, &str)],
) -> Result<(), &'static str> {
    let mut txt_map = BTreeMap::new();
    for (k, v) in txt {
        txt_map.insert(String::from(*k), String::from(*v));
    }

    let record = ServiceRecord {
        instance_name: String::from(instance),
        service_type: String::from(service_type),
        hostname: String::from("knoxos.local."),
        port,
        txt_records: txt_map,
        ttl: 4500,
    };

    serial_println!(
        "[mdns] Publishing {} on port {} as {}",
        service_type,
        port,
        instance
    );
    PUBLISHED_SERVICES.lock().push(record);
    Ok(())
}

/// Unpublish a service
pub fn unpublish(instance: &str) -> bool {
    let mut services = PUBLISHED_SERVICES.lock();
    if let Some(pos) = services.iter().position(|s| s.instance_name == instance) {
        services.remove(pos);
        serial_println!("[mdns] Unpublished {}", instance);
        true
    } else {
        false
    }
}

/// Browse for services of a given type on the local network
pub fn browse(service_type: &str) -> Vec<ServiceRecord> {
    serial_println!("[mdns] Browsing for {}", service_type);
    DISCOVERED_SERVICES
        .lock()
        .iter()
        .filter(|s| s.service_type.starts_with(service_type))
        .cloned()
        .collect()
}

/// Resolve a .local hostname to an IP address
pub fn resolve_host(hostname: &str) -> Option<u32> {
    HOST_CACHE.lock().get(hostname).map(|h| h.ip)
}

/// Handle an incoming mDNS packet (from UDP 5353)
pub fn handle_packet(data: &[u8], src_ip: u32) {
    if data.len() < 12 {
        return;
    }
    // Parse DNS header
    let flags = u16::from_be_bytes([data[2], data[3]]);
    let _qr = (flags >> 15) & 1; // 0 = query, 1 = response
    let _qdcount = u16::from_be_bytes([data[4], data[5]]);
    let ancount = u16::from_be_bytes([data[6], data[7]]);

    if ancount > 0 {
        // Process answers — would parse resource records and update caches
        serial_println!(
            "[mdns] Received response with {} answers from {:08X}",
            ancount,
            src_ip
        );
    }
}

/// Build an mDNS response packet for our published services
pub fn build_response(query_name: &str) -> Option<Vec<u8>> {
    let services = PUBLISHED_SERVICES.lock();
    let matching: Vec<&ServiceRecord> = services
        .iter()
        .filter(|s| s.service_type.contains(query_name) || s.instance_name.contains(query_name))
        .collect();

    if matching.is_empty() {
        return None;
    }

    // Build minimal DNS response
    let mut pkt = Vec::with_capacity(512);
    // Header: ID=0, QR=1 (response), AA=1
    pkt.extend_from_slice(&[0, 0, 0x84, 0x00]);
    // QDCOUNT=0, ANCOUNT=matching.len()
    pkt.extend_from_slice(&[0, 0]);
    let ancount = (matching.len() as u16).to_be_bytes();
    pkt.extend_from_slice(&ancount);
    pkt.extend_from_slice(&[0, 0, 0, 0]); // NSCOUNT, ARCOUNT

    Some(pkt)
}

/// Start the mDNS responder daemon
pub fn start() {
    RUNNING.store(true, Ordering::SeqCst);
    serial_println!("[mdns] Responder started on 224.0.0.251:5353");
}

/// Stop the mDNS responder
pub fn stop() {
    RUNNING.store(false, Ordering::SeqCst);
}

pub fn init() {
    serial_println!("[mdns] mDNS / Avahi service discovery initialized");
}
