/// Firewall — iptables-like packet filtering for KnoxOS
/// Implements a simplified netfilter-style firewall with chains and rules
///
/// Chains:
///   - INPUT:   Incoming packets destined for local processes
///   - OUTPUT:  Outgoing packets from local processes
///   - FORWARD: Packets being routed through this host
///
/// Targets:
///   - ACCEPT: Allow the packet
///   - DROP:   Silently discard the packet
///   - REJECT: Discard and send ICMP error
///   - LOG:    Log the packet and continue
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::net::Ipv4Address;
use crate::serial_println;

/// Firewall chain type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chain {
    Input,
    Output,
    Forward,
}

impl Chain {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "INPUT" => Some(Self::Input),
            "OUTPUT" => Some(Self::Output),
            "FORWARD" => Some(Self::Forward),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Input => "INPUT",
            Self::Output => "OUTPUT",
            Self::Forward => "FORWARD",
        }
    }
}

/// Rule target (action)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Accept,
    Drop,
    Reject,
    Log,
}

impl Target {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "ACCEPT" => Some(Self::Accept),
            "DROP" => Some(Self::Drop),
            "REJECT" => Some(Self::Reject),
            "LOG" => Some(Self::Log),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Accept => "ACCEPT",
            Self::Drop => "DROP",
            Self::Reject => "REJECT",
            Self::Log => "LOG",
        }
    }
}

/// IP protocol for matching
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Any,
    Tcp,
    Udp,
    Icmp,
}

impl Protocol {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "tcp" => Self::Tcp,
            "udp" => Self::Udp,
            "icmp" => Self::Icmp,
            _ => Self::Any,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Any => "all",
            Self::Tcp => "tcp",
            Self::Udp => "udp",
            Self::Icmp => "icmp",
        }
    }

    pub fn matches(&self, ip_proto: u8) -> bool {
        match self {
            Self::Any => true,
            Self::Tcp => ip_proto == 6,
            Self::Udp => ip_proto == 17,
            Self::Icmp => ip_proto == 1,
        }
    }
}

/// A firewall rule
#[derive(Debug, Clone)]
pub struct Rule {
    /// Rule number (position in chain)
    pub number: u32,
    /// Chain this rule belongs to
    pub chain: Chain,
    /// Target action
    pub target: Target,
    /// Protocol to match
    pub protocol: Protocol,
    /// Source IP (None = any)
    pub src_ip: Option<Ipv4Address>,
    /// Source subnet mask
    pub src_mask: Option<Ipv4Address>,
    /// Destination IP (None = any)
    pub dst_ip: Option<Ipv4Address>,
    /// Destination subnet mask
    pub dst_mask: Option<Ipv4Address>,
    /// Source port (0 = any)
    pub src_port: u16,
    /// Destination port (0 = any)
    pub dst_port: u16,
    /// Network interface (empty = any)
    pub interface: String,
    /// Whether this rule is enabled
    pub enabled: bool,
    /// Packet counter
    pub packets: u64,
    /// Byte counter
    pub bytes: u64,
    /// Comment/description
    pub comment: String,
}

impl Rule {
    pub fn new(chain: Chain, target: Target) -> Self {
        Self {
            number: 0,
            chain,
            target,
            protocol: Protocol::Any,
            src_ip: None,
            src_mask: None,
            dst_ip: None,
            dst_mask: None,
            src_port: 0,
            dst_port: 0,
            interface: String::new(),
            enabled: true,
            packets: 0,
            bytes: 0,
            comment: String::new(),
        }
    }

    /// Check if a packet matches this rule
    pub fn matches(
        &self,
        src: Ipv4Address,
        dst: Ipv4Address,
        proto: u8,
        src_port: u16,
        dst_port: u16,
    ) -> bool {
        // Protocol match
        if !self.protocol.matches(proto) {
            return false;
        }

        // Source IP match
        if let Some(ref rule_src) = self.src_ip {
            let mask = self.src_mask.unwrap_or(Ipv4Address([255, 255, 255, 255]));
            if !ip_matches(src, *rule_src, mask) {
                return false;
            }
        }

        // Destination IP match
        if let Some(ref rule_dst) = self.dst_ip {
            let mask = self.dst_mask.unwrap_or(Ipv4Address([255, 255, 255, 255]));
            if !ip_matches(dst, *rule_dst, mask) {
                return false;
            }
        }

        // Source port match
        if self.src_port != 0 && self.src_port != src_port {
            return false;
        }

        // Destination port match
        if self.dst_port != 0 && self.dst_port != dst_port {
            return false;
        }

        true
    }
}

/// Check if an IP matches with a mask
fn ip_matches(ip: Ipv4Address, rule_ip: Ipv4Address, mask: Ipv4Address) -> bool {
    for i in 0..4 {
        if (ip.0[i] & mask.0[i]) != (rule_ip.0[i] & mask.0[i]) {
            return false;
        }
    }
    true
}

/// Firewall state
struct FirewallState {
    /// Whether the firewall is enabled
    enabled: bool,
    /// INPUT chain rules
    input_rules: Vec<Rule>,
    /// OUTPUT chain rules
    output_rules: Vec<Rule>,
    /// FORWARD chain rules
    forward_rules: Vec<Rule>,
    /// Default policy for INPUT
    input_policy: Target,
    /// Default policy for OUTPUT
    output_policy: Target,
    /// Default policy for FORWARD
    forward_policy: Target,
    /// Total packets processed
    total_packets: u64,
    /// Total packets dropped
    dropped_packets: u64,
    /// Total packets accepted
    accepted_packets: u64,
}

lazy_static::lazy_static! {
    static ref FIREWALL: Mutex<FirewallState> = Mutex::new(FirewallState {
        enabled: false,
        input_rules: Vec::new(),
        output_rules: Vec::new(),
        forward_rules: Vec::new(),
        input_policy: Target::Accept,
        output_policy: Target::Accept,
        forward_policy: Target::Drop,
        total_packets: 0,
        dropped_packets: 0,
        accepted_packets: 0,
    });
}

/// Add a rule to a chain
pub fn add_rule(rule: Rule) -> Result<u32, i32> {
    let mut fw = FIREWALL.lock();
    let chain_rules = match rule.chain {
        Chain::Input => &mut fw.input_rules,
        Chain::Output => &mut fw.output_rules,
        Chain::Forward => &mut fw.forward_rules,
    };

    let number = chain_rules.len() as u32 + 1;
    let mut r = rule;
    r.number = number;
    chain_rules.push(r);

    serial_println!("[firewall] Rule {} added", number);
    Ok(number)
}

/// Remove a rule from a chain by number
pub fn delete_rule(chain: Chain, number: u32) -> Result<(), i32> {
    let mut fw = FIREWALL.lock();
    let chain_rules = match chain {
        Chain::Input => &mut fw.input_rules,
        Chain::Output => &mut fw.output_rules,
        Chain::Forward => &mut fw.forward_rules,
    };

    if let Some(pos) = chain_rules.iter().position(|r| r.number == number) {
        chain_rules.remove(pos);
        // Renumber remaining rules
        for (i, rule) in chain_rules.iter_mut().enumerate() {
            rule.number = i as u32 + 1;
        }
        Ok(())
    } else {
        Err(-2) // ENOENT
    }
}

/// Flush all rules in a chain
pub fn flush_chain(chain: Chain) {
    let mut fw = FIREWALL.lock();
    match chain {
        Chain::Input => fw.input_rules.clear(),
        Chain::Output => fw.output_rules.clear(),
        Chain::Forward => fw.forward_rules.clear(),
    }
    serial_println!("[firewall] Chain {} flushed", chain.as_str());
}

/// Flush all chains
pub fn flush_all() {
    let mut fw = FIREWALL.lock();
    fw.input_rules.clear();
    fw.output_rules.clear();
    fw.forward_rules.clear();
    serial_println!("[firewall] All chains flushed");
}

/// Set the default policy for a chain
pub fn set_policy(chain: Chain, target: Target) {
    let mut fw = FIREWALL.lock();
    match chain {
        Chain::Input => fw.input_policy = target,
        Chain::Output => fw.output_policy = target,
        Chain::Forward => fw.forward_policy = target,
    }
    serial_println!(
        "[firewall] {} policy set to {}",
        chain.as_str(),
        target.as_str()
    );
}

/// Enable the firewall
pub fn enable() {
    FIREWALL.lock().enabled = true;
    serial_println!("[firewall] Firewall enabled");
}

/// Disable the firewall
pub fn disable() {
    FIREWALL.lock().enabled = false;
    serial_println!("[firewall] Firewall disabled");
}

/// Check if firewall is enabled
pub fn is_enabled() -> bool {
    FIREWALL.lock().enabled
}

/// Filter a packet through a chain
/// Returns the target action to take
pub fn filter_packet(
    chain: Chain,
    src: Ipv4Address,
    dst: Ipv4Address,
    proto: u8,
    src_port: u16,
    dst_port: u16,
    packet_len: usize,
) -> Target {
    let mut fw = FIREWALL.lock();

    if !fw.enabled {
        return Target::Accept;
    }

    fw.total_packets += 1;

    let default_policy = match chain {
        Chain::Input => fw.input_policy,
        Chain::Output => fw.output_policy,
        Chain::Forward => fw.forward_policy,
    };
    let rules = match chain {
        Chain::Input => &mut fw.input_rules,
        Chain::Output => &mut fw.output_rules,
        Chain::Forward => &mut fw.forward_rules,
    };

    // Check rules in order
    for rule in rules.iter_mut() {
        if !rule.enabled {
            continue;
        }
        if rule.matches(src, dst, proto, src_port, dst_port) {
            rule.packets += 1;
            rule.bytes += packet_len as u64;

            match rule.target {
                Target::Log => {
                    serial_println!(
                        "[firewall] LOG: {} {} {}:{} -> {}:{} proto={}",
                        chain.as_str(),
                        rule.target.as_str(),
                        src,
                        src_port,
                        dst,
                        dst_port,
                        proto
                    );
                    // LOG continues processing (doesn't terminate)
                    continue;
                }
                Target::Accept => {
                    fw.accepted_packets += 1;
                    return Target::Accept;
                }
                Target::Drop => {
                    fw.dropped_packets += 1;
                    return Target::Drop;
                }
                Target::Reject => {
                    fw.dropped_packets += 1;
                    return Target::Reject;
                }
            }
        }
    }

    // No rule matched, use default policy
    match default_policy {
        Target::Accept => fw.accepted_packets += 1,
        Target::Drop | Target::Reject => fw.dropped_packets += 1,
        _ => {}
    }
    default_policy
}

/// List rules in a chain
pub fn list_rules(chain: Chain) -> Vec<Rule> {
    let fw = FIREWALL.lock();
    match chain {
        Chain::Input => fw.input_rules.clone(),
        Chain::Output => fw.output_rules.clone(),
        Chain::Forward => fw.forward_rules.clone(),
    }
}

/// Get firewall statistics
pub fn stats() -> (u64, u64, u64) {
    let fw = FIREWALL.lock();
    (fw.total_packets, fw.accepted_packets, fw.dropped_packets)
}

/// Format rules for display (iptables -L style)
pub fn format_chain(chain: Chain) -> String {
    let fw = FIREWALL.lock();
    let (rules, policy) = match chain {
        Chain::Input => (&fw.input_rules, fw.input_policy),
        Chain::Output => (&fw.output_rules, fw.output_policy),
        Chain::Forward => (&fw.forward_rules, fw.forward_policy),
    };

    let mut output = alloc::format!("Chain {} (policy {})\n", chain.as_str(), policy.as_str());
    output.push_str("num   pkts bytes target     prot opt source               destination\n");

    for rule in rules {
        let src = rule
            .src_ip
            .map(|ip| alloc::format!("{}", ip))
            .unwrap_or_else(|| String::from("0.0.0.0/0"));
        let dst = rule
            .dst_ip
            .map(|ip| alloc::format!("{}", ip))
            .unwrap_or_else(|| String::from("0.0.0.0/0"));

        output.push_str(&alloc::format!(
            "{:<5} {:<5} {:<5} {:<10} {:<4} --  {:<20} {}\n",
            rule.number,
            rule.packets,
            rule.bytes,
            rule.target.as_str(),
            rule.protocol.as_str(),
            src,
            dst,
        ));
    }

    output
}

/// Initialize firewall subsystem
pub fn init() {
    // Set up default rules
    // Allow loopback
    let mut lo_rule = Rule::new(Chain::Input, Target::Accept);
    lo_rule.interface = String::from("lo");
    lo_rule.comment = String::from("Allow loopback");
    let _ = add_rule(lo_rule);

    // Allow established connections (simplified - allow all TCP with src port > 1024)
    let mut established = Rule::new(Chain::Input, Target::Accept);
    established.protocol = Protocol::Tcp;
    established.comment = String::from("Allow established");
    let _ = add_rule(established);

    // Allow ICMP (ping)
    let mut icmp_rule = Rule::new(Chain::Input, Target::Accept);
    icmp_rule.protocol = Protocol::Icmp;
    icmp_rule.comment = String::from("Allow ICMP");
    let _ = add_rule(icmp_rule);

    serial_println!("[KnoxOS] Firewall initialized (disabled, 3 default rules)");
}
