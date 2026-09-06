/// DHCP Client — Dynamic Host Configuration Protocol for automatic network configuration
///
/// Implements DHCP (RFC 2131) to automatically obtain:
///   - IP address
///   - Subnet mask
///   - Default gateway
///   - DNS server addresses
///   - Lease time
///
/// DHCP flow: DISCOVER -> OFFER -> REQUEST -> ACK
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── DHCP Constants ─────────────────────────────────────────────────────

/// DHCP ports
pub const DHCP_CLIENT_PORT: u16 = 68;
pub const DHCP_SERVER_PORT: u16 = 67;

/// DHCP message types
pub const DHCP_DISCOVER: u8 = 1;
pub const DHCP_OFFER: u8 = 2;
pub const DHCP_REQUEST: u8 = 3;
pub const DHCP_DECLINE: u8 = 4;
pub const DHCP_ACK: u8 = 5;
pub const DHCP_NAK: u8 = 6;
pub const DHCP_RELEASE: u8 = 7;
pub const DHCP_INFORM: u8 = 8;

/// DHCP opcodes
pub const BOOTREQUEST: u8 = 1;
pub const BOOTREPLY: u8 = 2;

/// DHCP option codes
pub const DHCP_OPT_SUBNET_MASK: u8 = 1;
pub const DHCP_OPT_ROUTER: u8 = 3;
pub const DHCP_OPT_DNS: u8 = 6;
pub const DHCP_OPT_HOSTNAME: u8 = 12;
pub const DHCP_OPT_DOMAIN: u8 = 15;
pub const DHCP_OPT_REQUESTED_IP: u8 = 50;
pub const DHCP_OPT_LEASE_TIME: u8 = 51;
pub const DHCP_OPT_MESSAGE_TYPE: u8 = 53;
pub const DHCP_OPT_SERVER_ID: u8 = 54;
pub const DHCP_OPT_PARAM_REQUEST: u8 = 55;
pub const DHCP_OPT_END: u8 = 255;
pub const DHCP_OPT_PAD: u8 = 0;

/// DHCP magic cookie
pub const DHCP_MAGIC_COOKIE: [u8; 4] = [99, 130, 83, 99];

/// Broadcast MAC and IP
pub const BROADCAST_MAC: [u8; 6] = [0xFF; 6];
pub const BROADCAST_IP: [u8; 4] = [255, 255, 255, 255];
pub const ZERO_IP: [u8; 4] = [0, 0, 0, 0];

// ─── DHCP Packet ────────────────────────────────────────────────────────

/// DHCP packet structure (RFC 2131)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct DhcpPacket {
    pub op: u8,           // Message op code (1=request, 2=reply)
    pub htype: u8,        // Hardware type (1=Ethernet)
    pub hlen: u8,         // Hardware address length (6 for Ethernet)
    pub hops: u8,         // Hops
    pub xid: u32,         // Transaction ID
    pub secs: u16,        // Seconds elapsed
    pub flags: u16,       // Flags (0x8000 = broadcast)
    pub ciaddr: [u8; 4],  // Client IP address (if already has one)
    pub yiaddr: [u8; 4],  // 'Your' IP address (server assigns this)
    pub siaddr: [u8; 4],  // Server IP address
    pub giaddr: [u8; 4],  // Gateway IP address
    pub chaddr: [u8; 16], // Client hardware address
    pub sname: [u8; 64],  // Server host name
    pub file: [u8; 128],  // Boot file name
}

impl DhcpPacket {
    pub fn new(mac: [u8; 6], xid: u32) -> Self {
        let mut pkt = Self {
            op: BOOTREQUEST,
            htype: 1,
            hlen: 6,
            hops: 0,
            xid,
            secs: 0,
            flags: 0x0080, // Broadcast flag (big-endian 0x8000)
            ciaddr: ZERO_IP,
            yiaddr: ZERO_IP,
            siaddr: ZERO_IP,
            giaddr: ZERO_IP,
            chaddr: [0; 16],
            sname: [0; 64],
            file: [0; 128],
        };
        pkt.chaddr[..6].copy_from_slice(&mac);
        pkt
    }

    /// Serialize packet with options into a byte buffer
    pub fn to_bytes(&self, options: &[u8]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(300);

        // Fixed header
        buf.push(self.op);
        buf.push(self.htype);
        buf.push(self.hlen);
        buf.push(self.hops);
        buf.extend_from_slice(&self.xid.to_be_bytes());
        buf.extend_from_slice(&self.secs.to_be_bytes());
        buf.extend_from_slice(&self.flags.to_be_bytes());
        buf.extend_from_slice(&self.ciaddr);
        buf.extend_from_slice(&self.yiaddr);
        buf.extend_from_slice(&self.siaddr);
        buf.extend_from_slice(&self.giaddr);
        buf.extend_from_slice(&self.chaddr);
        buf.extend_from_slice(&self.sname);
        buf.extend_from_slice(&self.file);

        // Magic cookie
        buf.extend_from_slice(&DHCP_MAGIC_COOKIE);

        // Options
        buf.extend_from_slice(options);

        // End option
        buf.push(DHCP_OPT_END);

        // Pad to minimum DHCP packet size (300 bytes)
        while buf.len() < 300 {
            buf.push(0);
        }

        buf
    }
}

// ─── DHCP Lease ─────────────────────────────────────────────────────────

/// Network configuration obtained via DHCP
#[derive(Debug, Clone)]
pub struct DhcpLease {
    pub ip_address: [u8; 4],
    pub subnet_mask: [u8; 4],
    pub gateway: [u8; 4],
    pub dns_servers: Vec<[u8; 4]>,
    pub dhcp_server: [u8; 4],
    pub lease_time: u32, // Seconds
    pub domain_name: String,
    pub obtained_at: u64, // Tick count when obtained
    pub state: DhcpState,
    pub xid: u32, // Transaction ID
}

impl Default for DhcpLease {
    fn default() -> Self {
        Self::new()
    }
}

impl DhcpLease {
    pub fn new() -> Self {
        Self {
            ip_address: ZERO_IP,
            subnet_mask: [255, 255, 255, 0],
            gateway: ZERO_IP,
            dns_servers: Vec::new(),
            dhcp_server: ZERO_IP,
            lease_time: 0,
            domain_name: String::new(),
            obtained_at: 0,
            state: DhcpState::Init,
            xid: 0x12345678, // Random-ish transaction ID
        }
    }

    /// Format IP address as string
    pub fn ip_string(&self) -> String {
        alloc::format!(
            "{}.{}.{}.{}",
            self.ip_address[0],
            self.ip_address[1],
            self.ip_address[2],
            self.ip_address[3]
        )
    }

    /// Check if lease is valid
    pub fn is_valid(&self) -> bool {
        self.state == DhcpState::Bound && self.ip_address != ZERO_IP
    }
}

/// DHCP client state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DhcpState {
    Init,
    Selecting,  // Sent DISCOVER, waiting for OFFER
    Requesting, // Sent REQUEST, waiting for ACK
    Bound,      // Have a valid lease
    Renewing,   // Trying to renew
    Rebinding,  // Broadcast renewal
    Released,   // Explicitly released
}

// ─── DHCP Client ────────────────────────────────────────────────────────

lazy_static::lazy_static! {
    pub static ref DHCP_LEASE: Mutex<DhcpLease> = Mutex::new(DhcpLease::new());
}

static DHCP_CONFIGURED: AtomicBool = AtomicBool::new(false);

/// Check if DHCP has configured the network
pub fn is_configured() -> bool {
    DHCP_CONFIGURED.load(Ordering::Relaxed)
}

/// Build DHCP DISCOVER packet
pub fn build_discover(mac: [u8; 6]) -> Vec<u8> {
    let lease = DHCP_LEASE.lock();
    let pkt = DhcpPacket::new(mac, lease.xid);

    let mut options = Vec::new();
    // Message type: DISCOVER
    options.extend_from_slice(&[DHCP_OPT_MESSAGE_TYPE, 1, DHCP_DISCOVER]);
    // Parameter request list
    options.extend_from_slice(&[
        DHCP_OPT_PARAM_REQUEST,
        4,
        DHCP_OPT_SUBNET_MASK,
        DHCP_OPT_ROUTER,
        DHCP_OPT_DNS,
        DHCP_OPT_DOMAIN,
    ]);
    // Hostname
    let hostname = b"knoxos";
    options.push(DHCP_OPT_HOSTNAME);
    options.push(hostname.len() as u8);
    options.extend_from_slice(hostname);

    pkt.to_bytes(&options)
}

/// Build DHCP REQUEST packet
pub fn build_request(mac: [u8; 6], offered_ip: [u8; 4], server_ip: [u8; 4]) -> Vec<u8> {
    let lease = DHCP_LEASE.lock();
    let pkt = DhcpPacket::new(mac, lease.xid);

    let mut options = Vec::new();
    // Message type: REQUEST
    options.extend_from_slice(&[DHCP_OPT_MESSAGE_TYPE, 1, DHCP_REQUEST]);
    // Requested IP
    options.push(DHCP_OPT_REQUESTED_IP);
    options.push(4);
    options.extend_from_slice(&offered_ip);
    // Server ID
    options.push(DHCP_OPT_SERVER_ID);
    options.push(4);
    options.extend_from_slice(&server_ip);

    pkt.to_bytes(&options)
}

/// Build DHCP RELEASE packet
pub fn build_release(mac: [u8; 6], ip: [u8; 4], server_ip: [u8; 4]) -> Vec<u8> {
    let lease = DHCP_LEASE.lock();
    let mut pkt = DhcpPacket::new(mac, lease.xid);
    pkt.ciaddr = ip;

    let mut options = Vec::new();
    options.extend_from_slice(&[DHCP_OPT_MESSAGE_TYPE, 1, DHCP_RELEASE]);
    options.push(DHCP_OPT_SERVER_ID);
    options.push(4);
    options.extend_from_slice(&server_ip);

    pkt.to_bytes(&options)
}

/// Wrap DHCP payload in UDP/IP/Ethernet headers
pub fn wrap_dhcp_packet(dhcp_payload: &[u8], src_mac: [u8; 6]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(14 + 20 + 8 + dhcp_payload.len());

    // Ethernet header
    frame.extend_from_slice(&BROADCAST_MAC); // Destination MAC
    frame.extend_from_slice(&src_mac); // Source MAC
    frame.extend_from_slice(&[0x08, 0x00]); // EtherType: IPv4

    // IPv4 header (20 bytes)
    let total_len = 20 + 8 + dhcp_payload.len();
    frame.push(0x45); // Version + IHL
    frame.push(0x00); // DSCP/ECN
    frame.extend_from_slice(&(total_len as u16).to_be_bytes()); // Total length
    frame.extend_from_slice(&[0x00, 0x00]); // Identification
    frame.extend_from_slice(&[0x00, 0x00]); // Flags + Fragment offset
    frame.push(64); // TTL
    frame.push(17); // Protocol: UDP
    frame.extend_from_slice(&[0x00, 0x00]); // Header checksum (placeholder)
    frame.extend_from_slice(&ZERO_IP); // Source IP: 0.0.0.0
    frame.extend_from_slice(&BROADCAST_IP); // Dest IP: 255.255.255.255

    // Calculate IP header checksum
    let ip_start = 14;
    let checksum = ip_checksum(&frame[ip_start..ip_start + 20]);
    frame[ip_start + 10] = (checksum >> 8) as u8;
    frame[ip_start + 11] = (checksum & 0xFF) as u8;

    // UDP header (8 bytes)
    frame.extend_from_slice(&DHCP_CLIENT_PORT.to_be_bytes()); // Source port
    frame.extend_from_slice(&DHCP_SERVER_PORT.to_be_bytes()); // Dest port
    let udp_len = 8 + dhcp_payload.len();
    frame.extend_from_slice(&(udp_len as u16).to_be_bytes()); // UDP length
    frame.extend_from_slice(&[0x00, 0x00]); // UDP checksum (optional for IPv4)

    // DHCP payload
    frame.extend_from_slice(dhcp_payload);

    frame
}

/// Calculate IP header checksum
fn ip_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        sum += ((data[i] as u32) << 8) | (data[i + 1] as u32);
        i += 2;
    }
    if i < data.len() {
        sum += (data[i] as u32) << 8;
    }
    while sum > 0xFFFF {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !sum as u16
}

/// Parse a DHCP response (OFFER or ACK)
pub fn parse_dhcp_response(data: &[u8]) -> Option<(u8, DhcpLease)> {
    // Minimum DHCP packet: 240 bytes header + 4 magic cookie + options
    if data.len() < 244 {
        return None;
    }

    // Check it's a BOOTREPLY
    if data[0] != BOOTREPLY {
        return None;
    }

    let mut lease = DhcpLease::new();

    // Parse fixed fields
    lease.xid = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    lease.ip_address.copy_from_slice(&data[16..20]); // yiaddr
    let siaddr = [data[20], data[21], data[22], data[23]];
    lease.dhcp_server = siaddr;

    // Check magic cookie
    if data[236..240] != DHCP_MAGIC_COOKIE {
        return None;
    }

    // Parse options
    let mut i = 240;
    let mut msg_type = 0u8;

    while i < data.len() {
        let opt = data[i];
        if opt == DHCP_OPT_END {
            break;
        }
        if opt == DHCP_OPT_PAD {
            i += 1;
            continue;
        }

        if i + 1 >= data.len() {
            break;
        }
        let len = data[i + 1] as usize;
        let opt_data = if i + 2 + len <= data.len() {
            &data[i + 2..i + 2 + len]
        } else {
            break;
        };

        match opt {
            DHCP_OPT_MESSAGE_TYPE if len >= 1 => {
                msg_type = opt_data[0];
            }
            DHCP_OPT_SUBNET_MASK if len >= 4 => {
                lease.subnet_mask.copy_from_slice(&opt_data[..4]);
            }
            DHCP_OPT_ROUTER if len >= 4 => {
                lease.gateway.copy_from_slice(&opt_data[..4]);
            }
            DHCP_OPT_DNS => {
                let mut j = 0;
                while j + 4 <= len {
                    let mut dns = [0u8; 4];
                    dns.copy_from_slice(&opt_data[j..j + 4]);
                    lease.dns_servers.push(dns);
                    j += 4;
                }
            }
            DHCP_OPT_LEASE_TIME if len >= 4 => {
                lease.lease_time =
                    u32::from_be_bytes([opt_data[0], opt_data[1], opt_data[2], opt_data[3]]);
            }
            DHCP_OPT_SERVER_ID if len >= 4 => {
                lease.dhcp_server.copy_from_slice(&opt_data[..4]);
            }
            DHCP_OPT_DOMAIN => {
                if let Ok(s) = core::str::from_utf8(opt_data) {
                    lease.domain_name = String::from(s);
                }
            }
            _ => {}
        }

        i += 2 + len;
    }

    if msg_type == 0 {
        return None;
    }

    Some((msg_type, lease))
}

/// Process DHCP response (called by network stack when a DHCP reply arrives)
pub fn process_dhcp_response(data: &[u8]) {
    if let Some((msg_type, response_lease)) = parse_dhcp_response(data) {
        let mut lease = DHCP_LEASE.lock();

        match msg_type {
            DHCP_OFFER if lease.state == DhcpState::Selecting => {
                serial_println!(
                    "[DHCP] Received OFFER: IP={}.{}.{}.{}",
                    response_lease.ip_address[0],
                    response_lease.ip_address[1],
                    response_lease.ip_address[2],
                    response_lease.ip_address[3]
                );

                lease.ip_address = response_lease.ip_address;
                lease.subnet_mask = response_lease.subnet_mask;
                lease.gateway = response_lease.gateway;
                lease.dns_servers = response_lease.dns_servers;
                lease.dhcp_server = response_lease.dhcp_server;
                lease.state = DhcpState::Requesting;

                // Send REQUEST
                drop(lease);
                send_request();
            }
            DHCP_ACK
                if (lease.state == DhcpState::Requesting || lease.state == DhcpState::Renewing) =>
            {
                lease.ip_address = response_lease.ip_address;
                lease.subnet_mask = response_lease.subnet_mask;
                lease.gateway = response_lease.gateway;
                lease.dns_servers = response_lease.dns_servers.clone();
                lease.lease_time = response_lease.lease_time;
                lease.domain_name = response_lease.domain_name.clone();
                lease.state = DhcpState::Bound;

                DHCP_CONFIGURED.store(true, Ordering::Relaxed);

                serial_println!("[DHCP] Lease obtained:");
                serial_println!(
                    "[DHCP]   IP:      {}.{}.{}.{}",
                    lease.ip_address[0],
                    lease.ip_address[1],
                    lease.ip_address[2],
                    lease.ip_address[3]
                );
                serial_println!(
                    "[DHCP]   Mask:    {}.{}.{}.{}",
                    lease.subnet_mask[0],
                    lease.subnet_mask[1],
                    lease.subnet_mask[2],
                    lease.subnet_mask[3]
                );
                serial_println!(
                    "[DHCP]   Gateway: {}.{}.{}.{}",
                    lease.gateway[0],
                    lease.gateway[1],
                    lease.gateway[2],
                    lease.gateway[3]
                );
                for dns in &lease.dns_servers {
                    serial_println!(
                        "[DHCP]   DNS:     {}.{}.{}.{}",
                        dns[0],
                        dns[1],
                        dns[2],
                        dns[3]
                    );
                }
                serial_println!("[DHCP]   Lease:   {}s", lease.lease_time);

                // Configure the network interface with obtained settings
                drop(lease);
                apply_configuration();
            }
            DHCP_NAK => {
                serial_println!("[DHCP] Received NAK - restarting discovery");
                lease.state = DhcpState::Init;
            }
            _ => {}
        }
    }
}

/// Send DHCP DISCOVER
pub fn send_discover() {
    let mac = crate::virtio_net::get_mac().unwrap_or([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);

    {
        let mut lease = DHCP_LEASE.lock();
        lease.state = DhcpState::Selecting;
    }

    let discover = build_discover(mac);
    let frame = wrap_dhcp_packet(&discover, mac);

    if crate::virtio_net::send_frame(&frame) {
        serial_println!("[DHCP] Sent DISCOVER");
    } else {
        serial_println!("[DHCP] Failed to send DISCOVER (no NIC?)");
    }
}

/// Send DHCP REQUEST
fn send_request() {
    let mac = crate::virtio_net::get_mac().unwrap_or([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
    let (offered_ip, server_ip) = {
        let lease = DHCP_LEASE.lock();
        (lease.ip_address, lease.dhcp_server)
    };

    let request = build_request(mac, offered_ip, server_ip);
    let frame = wrap_dhcp_packet(&request, mac);

    if crate::virtio_net::send_frame(&frame) {
        serial_println!(
            "[DHCP] Sent REQUEST for {}.{}.{}.{}",
            offered_ip[0],
            offered_ip[1],
            offered_ip[2],
            offered_ip[3]
        );
    }
}

/// Apply DHCP configuration to network stack
fn apply_configuration() {
    let lease = DHCP_LEASE.lock();
    if lease.state != DhcpState::Bound {
        return;
    }

    // Update the DNS resolver with obtained servers
    if !lease.dns_servers.is_empty() {
        let dns = lease.dns_servers[0];
        crate::dns::set_dns_server(dns);
    }

    serial_println!("[DHCP] Network configuration applied");
}

/// Release the DHCP lease
pub fn release() {
    let mac = crate::virtio_net::get_mac().unwrap_or([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
    let (ip, server) = {
        let mut lease = DHCP_LEASE.lock();
        let ip = lease.ip_address;
        let server = lease.dhcp_server;
        lease.state = DhcpState::Released;
        (ip, server)
    };

    let release_pkt = build_release(mac, ip, server);
    let frame = wrap_dhcp_packet(&release_pkt, mac);
    crate::virtio_net::send_frame(&frame);

    DHCP_CONFIGURED.store(false, Ordering::Relaxed);
    serial_println!("[DHCP] Lease released");
}

/// Get current lease info
pub fn get_lease() -> Option<(String, String, String, Vec<String>)> {
    let lease = DHCP_LEASE.lock();
    if !lease.is_valid() {
        return None;
    }

    let ip = alloc::format!(
        "{}.{}.{}.{}",
        lease.ip_address[0],
        lease.ip_address[1],
        lease.ip_address[2],
        lease.ip_address[3]
    );
    let mask = alloc::format!(
        "{}.{}.{}.{}",
        lease.subnet_mask[0],
        lease.subnet_mask[1],
        lease.subnet_mask[2],
        lease.subnet_mask[3]
    );
    let gw = alloc::format!(
        "{}.{}.{}.{}",
        lease.gateway[0],
        lease.gateway[1],
        lease.gateway[2],
        lease.gateway[3]
    );
    let dns: Vec<String> = lease
        .dns_servers
        .iter()
        .map(|d| alloc::format!("{}.{}.{}.{}", d[0], d[1], d[2], d[3]))
        .collect();

    Some((ip, mask, gw, dns))
}

/// Initialize DHCP client
pub fn init() {
    serial_println!("[DHCP] DHCP client initialized");

    // If NIC is available, start DHCP discovery
    if crate::virtio_net::is_nic_available() {
        send_discover();
    } else {
        serial_println!("[DHCP] No NIC available, using static configuration");
        // Set default QEMU user-mode networking config
        let mut lease = DHCP_LEASE.lock();
        lease.ip_address = [10, 0, 2, 15];
        lease.subnet_mask = [255, 255, 255, 0];
        lease.gateway = [10, 0, 2, 2];
        lease.dns_servers.push([10, 0, 2, 3]);
        lease.state = DhcpState::Bound;
        lease.lease_time = 86400;
        DHCP_CONFIGURED.store(true, Ordering::Relaxed);
    }
}
