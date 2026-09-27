use alloc::collections::BTreeMap;
use alloc::string::String;
use spin::Mutex;

use super::ipv4::Ipv4Address;

/// Simple DNS cache
lazy_static::lazy_static! {
    pub static ref DNS_CACHE: Mutex<BTreeMap<String, Ipv4Address>> = {
        let mut cache = BTreeMap::new();
        cache.insert(String::from("localhost"), Ipv4Address::LOOPBACK);
        cache.insert(String::from("knoxos"), Ipv4Address::LOOPBACK);
        Mutex::new(cache)
    };
}

/// Resolve a hostname to an IP address
pub fn dns_resolve(hostname: &str) -> Option<Ipv4Address> {
    DNS_CACHE.lock().get(hostname).copied()
}
