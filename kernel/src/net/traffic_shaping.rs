/// Traffic Shaping / QoS — tc-equivalent traffic control
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Queueing discipline types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdiscType {
    Pfifo,   // Simple FIFO
    Sfq,     // Stochastic Fair Queuing
    Tbf,     // Token Bucket Filter
    Htb,     // Hierarchical Token Bucket
    Fq,      // Fair Queuing
    FqCodel, // Fair Queuing + CoDel AQM
    Prio,    // Priority queuing (3 bands)
    Cake,    // Common Applications Kept Enhanced
}

/// Traffic class for classification
#[derive(Debug, Clone)]
pub struct TrafficClass {
    pub class_id: u32,
    pub parent: u32,
    pub rate_bps: u64, // guaranteed rate
    pub ceil_bps: u64, // max rate
    pub burst_bytes: u32,
    pub priority: u8,
}

/// A filter rule for classifying packets
#[derive(Debug, Clone)]
pub struct TcFilter {
    pub priority: u16,
    pub protocol: u16, // ETH_P_IP = 0x0800
    pub match_src_ip: Option<u32>,
    pub match_dst_ip: Option<u32>,
    pub match_src_port: Option<u16>,
    pub match_dst_port: Option<u16>,
    pub match_dscp: Option<u8>,
    pub target_class: u32,
}

/// Per-interface qdisc configuration
#[derive(Debug, Clone)]
pub struct InterfaceQdisc {
    pub iface: String,
    pub root_qdisc: QdiscType,
    pub classes: Vec<TrafficClass>,
    pub filters: Vec<TcFilter>,
    /// Token bucket state
    pub tokens: u64,
    pub last_tick: u64,
}

lazy_static::lazy_static! {
    static ref QDISCS: Mutex<BTreeMap<String, InterfaceQdisc>> = Mutex::new(BTreeMap::new());
}

/// Attach a root qdisc to an interface
pub fn set_qdisc(iface: &str, qdisc: QdiscType) -> Result<(), &'static str> {
    let entry = InterfaceQdisc {
        iface: String::from(iface),
        root_qdisc: qdisc,
        classes: Vec::new(),
        filters: Vec::new(),
        tokens: 0,
        last_tick: 0,
    };
    serial_println!("[tc] Set {:?} qdisc on {}", qdisc, iface);
    QDISCS.lock().insert(String::from(iface), entry);
    Ok(())
}

/// Add a traffic class (for HTB)
pub fn add_class(iface: &str, class: TrafficClass) -> Result<(), &'static str> {
    let mut qdiscs = QDISCS.lock();
    let q = qdiscs.get_mut(iface).ok_or("No qdisc on interface")?;
    serial_println!(
        "[tc] Class {} on {} rate={}bps ceil={}bps",
        class.class_id,
        iface,
        class.rate_bps,
        class.ceil_bps
    );
    q.classes.push(class);
    Ok(())
}

/// Add a filter rule
pub fn add_filter(iface: &str, filter: TcFilter) -> Result<(), &'static str> {
    let mut qdiscs = QDISCS.lock();
    let q = qdiscs.get_mut(iface).ok_or("No qdisc on interface")?;
    q.filters.push(filter);
    Ok(())
}

/// Classify a packet and return target class ID
pub fn classify(
    iface: &str,
    src_ip: u32,
    dst_ip: u32,
    src_port: u16,
    dst_port: u16,
    dscp: u8,
) -> Option<u32> {
    let qdiscs = QDISCS.lock();
    let q = qdiscs.get(iface)?;

    for filter in &q.filters {
        let mut matches = true;
        if let Some(sip) = filter.match_src_ip {
            if sip != src_ip {
                matches = false;
            }
        }
        if let Some(dip) = filter.match_dst_ip {
            if dip != dst_ip {
                matches = false;
            }
        }
        if let Some(sp) = filter.match_src_port {
            if sp != src_port {
                matches = false;
            }
        }
        if let Some(dp) = filter.match_dst_port {
            if dp != dst_port {
                matches = false;
            }
        }
        if let Some(d) = filter.match_dscp {
            if d != dscp {
                matches = false;
            }
        }
        if matches {
            return Some(filter.target_class);
        }
    }
    None
}

/// Token bucket rate limiting check — returns true if packet should be sent
pub fn token_bucket_allow(iface: &str, packet_bytes: u32, now_ns: u64) -> bool {
    let mut qdiscs = QDISCS.lock();
    if let Some(q) = qdiscs.get_mut(iface) {
        // Replenish tokens based on elapsed time
        if q.last_tick > 0 {
            let elapsed_ns = now_ns.saturating_sub(q.last_tick);
            // Find first class rate as the limit
            if let Some(class) = q.classes.first() {
                let new_tokens = class.rate_bps * elapsed_ns / 8_000_000_000;
                q.tokens = (q.tokens + new_tokens).min(class.burst_bytes as u64);
            }
        }
        q.last_tick = now_ns;

        if q.tokens >= packet_bytes as u64 {
            q.tokens -= packet_bytes as u64;
            return true;
        }
        false
    } else {
        true // no qdisc → allow all
    }
}

/// Remove qdisc from interface
pub fn del_qdisc(iface: &str) -> bool {
    QDISCS.lock().remove(iface).is_some()
}

pub fn init() {
    serial_println!("[tc] Traffic shaping / QoS initialized");
}
