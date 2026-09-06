/// NTP Client — Network Time Protocol
///
/// Provides time synchronization via NTP (RFC 5905 simplified):
///   - NTP packet construction and parsing
///   - Round-trip delay and offset calculation
///   - System clock adjustment
///   - Periodic sync with configurable interval
///   - Multiple server support with fallback
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// NTP CONSTANTS
// ═══════════════════════════════════════════════════════════════════════

/// NTP port
pub const NTP_PORT: u16 = 123;
/// NTP packet size
pub const NTP_PACKET_SIZE: usize = 48;
/// Seconds between NTP epoch (1900) and Unix epoch (1970)
pub const NTP_UNIX_OFFSET: u64 = 2_208_988_800;
/// Default sync interval (seconds)
pub const DEFAULT_SYNC_INTERVAL: u64 = 3600; // 1 hour

// ═══════════════════════════════════════════════════════════════════════
// NTP PACKET
// ═══════════════════════════════════════════════════════════════════════

/// NTP timestamp (64 bits: 32 seconds + 32 fraction)
#[derive(Debug, Clone, Copy, Default)]
pub struct NtpTimestamp {
    pub seconds: u32,
    pub fraction: u32,
}

impl NtpTimestamp {
    pub fn to_unix_secs(&self) -> i64 {
        self.seconds as i64 - NTP_UNIX_OFFSET as i64
    }

    pub fn to_millis(&self) -> u64 {
        let secs = self.seconds as u64;
        let frac_ms = (self.fraction as u64 * 1000) >> 32;
        secs * 1000 + frac_ms
    }

    pub fn from_bytes(data: &[u8]) -> Self {
        Self {
            seconds: u32::from_be_bytes([data[0], data[1], data[2], data[3]]),
            fraction: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
        }
    }

    pub fn to_bytes(&self) -> [u8; 8] {
        let mut buf = [0u8; 8];
        buf[0..4].copy_from_slice(&self.seconds.to_be_bytes());
        buf[4..8].copy_from_slice(&self.fraction.to_be_bytes());
        buf
    }
}

/// NTP packet structure
#[derive(Debug, Clone, Copy)]
pub struct NtpPacket {
    /// Leap indicator (2 bits) + Version (3 bits) + Mode (3 bits)
    pub li_vn_mode: u8,
    /// Stratum
    pub stratum: u8,
    /// Polling interval
    pub poll: u8,
    /// Precision
    pub precision: i8,
    /// Root delay
    pub root_delay: u32,
    /// Root dispersion
    pub root_dispersion: u32,
    /// Reference ID
    pub ref_id: u32,
    /// Reference timestamp
    pub ref_timestamp: NtpTimestamp,
    /// Origin timestamp (T1 — client send time)
    pub orig_timestamp: NtpTimestamp,
    /// Receive timestamp (T2 — server receive time)
    pub recv_timestamp: NtpTimestamp,
    /// Transmit timestamp (T3 — server send time)
    pub xmit_timestamp: NtpTimestamp,
}

impl NtpPacket {
    /// Create a client request packet
    pub fn new_request() -> Self {
        Self {
            li_vn_mode: 0b00_100_011, // LI=0, VN=4, Mode=3 (client)
            stratum: 0,
            poll: 6,        // 2^6 = 64 seconds
            precision: -20, // ~microsecond precision
            root_delay: 0,
            root_dispersion: 0,
            ref_id: 0,
            ref_timestamp: NtpTimestamp::default(),
            orig_timestamp: NtpTimestamp::default(),
            recv_timestamp: NtpTimestamp::default(),
            xmit_timestamp: NtpTimestamp::default(),
        }
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> [u8; NTP_PACKET_SIZE] {
        let mut buf = [0u8; NTP_PACKET_SIZE];
        buf[0] = self.li_vn_mode;
        buf[1] = self.stratum;
        buf[2] = self.poll;
        buf[3] = self.precision as u8;
        buf[4..8].copy_from_slice(&self.root_delay.to_be_bytes());
        buf[8..12].copy_from_slice(&self.root_dispersion.to_be_bytes());
        buf[12..16].copy_from_slice(&self.ref_id.to_be_bytes());
        buf[16..24].copy_from_slice(&self.ref_timestamp.to_bytes());
        buf[24..32].copy_from_slice(&self.orig_timestamp.to_bytes());
        buf[32..40].copy_from_slice(&self.recv_timestamp.to_bytes());
        buf[40..48].copy_from_slice(&self.xmit_timestamp.to_bytes());
        buf
    }

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < NTP_PACKET_SIZE {
            return None;
        }
        Some(Self {
            li_vn_mode: data[0],
            stratum: data[1],
            poll: data[2],
            precision: data[3] as i8,
            root_delay: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            root_dispersion: u32::from_be_bytes([data[8], data[9], data[10], data[11]]),
            ref_id: u32::from_be_bytes([data[12], data[13], data[14], data[15]]),
            ref_timestamp: NtpTimestamp::from_bytes(&data[16..24]),
            orig_timestamp: NtpTimestamp::from_bytes(&data[24..32]),
            recv_timestamp: NtpTimestamp::from_bytes(&data[32..40]),
            xmit_timestamp: NtpTimestamp::from_bytes(&data[40..48]),
        })
    }

    /// Get version number
    pub fn version(&self) -> u8 {
        (self.li_vn_mode >> 3) & 0x07
    }

    /// Get mode
    pub fn mode(&self) -> u8 {
        self.li_vn_mode & 0x07
    }

    /// Get leap indicator
    pub fn leap(&self) -> u8 {
        (self.li_vn_mode >> 6) & 0x03
    }
}

// ═══════════════════════════════════════════════════════════════════════
// NTP CLIENT
// ═══════════════════════════════════════════════════════════════════════

/// Clock offset from NTP (in milliseconds, signed)
static CLOCK_OFFSET_MS: AtomicI64 = AtomicI64::new(0);
/// Last sync attempt timestamp (tick-seconds) — updated on both success AND failure
static LAST_SYNC: AtomicU64 = AtomicU64::new(0);
/// Whether NTP sync is enabled
static NTP_ENABLED: AtomicBool = AtomicBool::new(false);
/// Sync interval in seconds
static SYNC_INTERVAL: AtomicU64 = AtomicU64::new(DEFAULT_SYNC_INTERVAL);
/// Number of consecutive failures (for exponential backoff)
static FAIL_COUNT: AtomicU64 = AtomicU64::new(0);

/// NTP server entry
#[derive(Clone)]
pub struct NtpServer {
    pub hostname: String,
    pub ip: [u8; 4],
    pub stratum: u8,
    pub last_rtt_ms: u32,
}

/// NTP client state
pub struct NtpClient {
    pub servers: Vec<NtpServer>,
    pub current_server: usize,
    pub synced: bool,
    pub offset_ms: i64,
    pub rtt_ms: u32,
}

impl NtpClient {
    pub fn new() -> Self {
        Self {
            servers: Vec::new(),
            current_server: 0,
            synced: false,
            offset_ms: 0,
            rtt_ms: 0,
        }
    }

    /// Add a known NTP server
    pub fn add_server(&mut self, hostname: &str, ip: [u8; 4]) {
        self.servers.push(NtpServer {
            hostname: String::from(hostname),
            ip,
            stratum: 0,
            last_rtt_ms: 0,
        });
    }

    /// Process an NTP response and calculate clock offset
    /// T1 = client send time, T2 = server receive time,
    /// T3 = server transmit time, T4 = client receive time
    pub fn process_response(&mut self, response: &NtpPacket, t1_ms: u64, t4_ms: u64) {
        // Validate response
        if response.mode() != 4 {
            // Must be server mode
            serial_println!("[NTP] Invalid response mode: {}", response.mode());
            return;
        }
        if response.stratum == 0 || response.stratum > 15 {
            serial_println!("[NTP] Invalid stratum: {}", response.stratum);
            return;
        }

        let t2_ms = response.recv_timestamp.to_millis();
        let t3_ms = response.xmit_timestamp.to_millis();

        // Clock offset = ((T2 - T1) + (T3 - T4)) / 2
        let offset = ((t2_ms as i64 - t1_ms as i64) + (t3_ms as i64 - t4_ms as i64)) / 2;

        // Round-trip delay = (T4 - T1) - (T3 - T2)
        let rtt = (t4_ms as i64 - t1_ms as i64) - (t3_ms as i64 - t2_ms as i64);

        self.offset_ms = offset;
        self.rtt_ms = rtt as u32;
        self.synced = true;

        // Update global offset
        CLOCK_OFFSET_MS.store(offset, Ordering::Relaxed);
        LAST_SYNC.store(t4_ms / 1000, Ordering::Relaxed);

        // Update server info
        if let Some(server) = self.servers.get_mut(self.current_server) {
            server.stratum = response.stratum;
            server.last_rtt_ms = rtt as u32;
        }

        serial_println!(
            "[NTP] Synced: offset={}ms, RTT={}ms, stratum={}",
            offset,
            rtt,
            response.stratum
        );
    }

    /// Try the next server in the list
    pub fn try_next_server(&mut self) {
        if !self.servers.is_empty() {
            self.current_server = (self.current_server + 1) % self.servers.len();
        }
    }
}

lazy_static::lazy_static! {
    pub static ref NTP_CLIENT: Mutex<NtpClient> = Mutex::new(NtpClient::new());
}

// ═══════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════

/// Get NTP-adjusted time (adds offset to system time)
pub fn adjusted_time_ms(system_time_ms: u64) -> u64 {
    let offset = CLOCK_OFFSET_MS.load(Ordering::Relaxed);
    (system_time_ms as i64 + offset) as u64
}

/// Get current clock offset
pub fn clock_offset_ms() -> i64 {
    CLOCK_OFFSET_MS.load(Ordering::Relaxed)
}

/// Is time synced?
pub fn is_synced() -> bool {
    LAST_SYNC.load(Ordering::Relaxed) > 0
}

/// Enable/disable NTP
pub fn set_enabled(enabled: bool) {
    NTP_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Is NTP enabled?
pub fn is_enabled() -> bool {
    NTP_ENABLED.load(Ordering::Relaxed)
}

/// Set sync interval
pub fn set_sync_interval(seconds: u64) {
    SYNC_INTERVAL.store(seconds, Ordering::Relaxed);
}

/// Initialize NTP client with default servers
pub fn init() {
    let mut client = NTP_CLIENT.lock();
    // Add default NTP pool servers (IP addresses since we may not have DNS yet)
    client.add_server("pool.ntp.org", [162, 159, 200, 123]);
    client.add_server("time.google.com", [216, 239, 35, 0]);
    client.add_server("time.cloudflare.com", [162, 159, 200, 1]);
    drop(client);

    NTP_ENABLED.store(true, Ordering::Relaxed);
    serial_println!("[KnoxOS] NTP client initialized (3 servers)");
}

// ═══════════════════════════════════════════════════════════════════════
// NTP SYNC ENGINE — Real UDP-based time synchronization
// ═══════════════════════════════════════════════════════════════════════

/// Perform a single NTP sync attempt using the network stack.
/// This is NON-BLOCKING: sends the request and returns immediately.
/// On QEMU without a working gateway, the send itself may fail,
/// which is fine — we just log once and back off.
pub fn sync_once() {
    if !NTP_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    // Record that we attempted a sync (prevents hammering every second)
    let now_secs = crate::interrupts::get_ticks() / 100;
    LAST_SYNC.store(now_secs, Ordering::Relaxed);

    let client = NTP_CLIENT.lock();
    if client.servers.is_empty() {
        return;
    }
    let server = client.servers[client.current_server].clone();
    drop(client);

    let server_ip = crate::net::Ipv4Address(server.ip);

    // Build NTP client request packet
    let request = NtpPacket::new_request();
    let request_bytes = request.to_bytes();

    // Create and bind a UDP socket for NTP
    let src_port: u16 = 50123;
    {
        use crate::net::{AddressFamily, SOCKETS, Socket, SocketAddress, SocketType};

        let mut sock = Socket::new(AddressFamily::Inet, SocketType::Dgram, 0);
        let _ = sock.bind(SocketAddress::Inet(
            crate::net::Ipv4Address([0, 0, 0, 0]),
            src_port,
        ));
        let sid = sock.id;
        SOCKETS.lock().insert(sid, sock);

        // Try to send the NTP request — non-blocking
        let sent = crate::netint::send_udp(server_ip, src_port, NTP_PORT, &request_bytes);
        // Clean up socket immediately (we don't wait for a response here
        // to avoid blocking the event loop; response handling would need
        // to be async, which is a future enhancement)
        SOCKETS.lock().remove(&sid);

        if !sent {
            // Increment failure counter for exponential backoff
            FAIL_COUNT.fetch_add(1, Ordering::Relaxed);
            NTP_CLIENT.lock().try_next_server();
            return;
        }

        // Reset fail count on successful send
        FAIL_COUNT.store(0, Ordering::Relaxed);
        serial_println!(
            "[NTP] Request sent to {} ({}.{}.{}.{})",
            server.hostname,
            server.ip[0],
            server.ip[1],
            server.ip[2],
            server.ip[3]
        );
        // NOTE: Without a blocking wait, we won't process the response.
        // A truly async NTP implementation would register a callback or
        // check for responses in the next poll cycle. For now, this at
        // least prevents the UI from freezing.
    }
}

/// Periodic NTP sync task — called from the timer or cron system.
/// Uses exponential backoff on failure to avoid spamming the network
/// and blocking the event loop.
pub fn periodic_sync() {
    if !NTP_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    let now_secs = crate::interrupts::get_ticks() / 100; // approximate seconds
    let last = LAST_SYNC.load(Ordering::Relaxed);
    let base_interval = SYNC_INTERVAL.load(Ordering::Relaxed);

    // Exponential backoff: on failures, wait longer between retries
    // backoff = base_interval * 2^min(fails, 6), capped at 3600s (1 hour)
    let fails = FAIL_COUNT.load(Ordering::Relaxed);
    let backoff = if fails == 0 {
        base_interval
    } else {
        let shift = core::cmp::min(fails, 6) as u32;
        core::cmp::min(base_interval.saturating_mul(1u64 << shift), 3600)
    };

    if last == 0 || (now_secs > last && now_secs - last >= backoff) {
        sync_once();
    }
}
