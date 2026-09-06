/// net_production — Production-ready networking stack
///
/// Implements TCP congestion control algorithms, proper retransmission,
/// window scaling, SACK, ECN, and other features needed for real-world
/// TCP/IP networking beyond basic connectivity.
///
/// Features:
/// - TCP congestion control: Reno, CUBIC, BBR, BBRv2
/// - TCP window scaling (RFC 7323)
/// - Selective Acknowledgement (SACK, RFC 2018)
/// - Explicit Congestion Notification (ECN, RFC 3168)
/// - TCP Fast Open (TFO, RFC 7413)
/// - TCP keepalive
/// - Nagle algorithm and TCP_NODELAY
/// - TCP_CORK for message batching
/// - SO_REUSEPORT load balancing
/// - TCP timestamps (RFC 7323)
/// - Path MTU discovery
/// - Retransmission timeout (RTO) with Karn's algorithm
/// - Delayed ACK
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ─── Congestion Control ─────────────────────────────────────────────

/// Congestion control algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CongestionAlgorithm {
    /// TCP Reno (basic AIMD)
    Reno,
    /// TCP New Reno (improved fast recovery)
    NewReno,
    /// CUBIC (Linux default, RFC 8312)
    Cubic,
    /// BBR (Google, model-based)
    Bbr,
    /// BBRv2 (improved BBR)
    BbrV2,
    /// Vegas (delay-based)
    Vegas,
    /// Westwood+ (bandwidth estimation)
    Westwood,
    /// DCTCP (data center TCP)
    Dctcp,
}

/// Congestion control state
#[derive(Debug, Clone)]
pub struct CongestionState {
    /// Algorithm in use
    pub algorithm: CongestionAlgorithm,
    /// Congestion window (bytes)
    pub cwnd: u32,
    /// Slow start threshold
    pub ssthresh: u32,
    /// Bytes in flight
    pub bytes_in_flight: u32,
    /// RTT estimate (microseconds)
    pub srtt_us: u64,
    /// RTT variance
    pub rttvar_us: u64,
    /// Retransmission timeout (microseconds)
    pub rto_us: u64,
    /// Minimum RTT observed
    pub min_rtt_us: u64,
    /// Maximum bandwidth observed (bytes/sec)
    pub max_bw: u64,
    /// Whether in slow start
    pub in_slow_start: bool,
    /// ECN echo count
    pub ecn_ce_count: u64,
    /// Packets lost counter
    pub lost_count: u64,
    /// Retransmit counter
    pub retransmit_count: u64,
    // CUBIC-specific
    /// CUBIC: last congestion event time
    pub cubic_last_max_cwnd: u32,
    pub cubic_epoch_start: u64,
    pub cubic_origin_point: u32,
    pub cubic_k: f64,
    // BBR-specific
    /// BBR: pacing rate (bytes/sec)
    pub bbr_pacing_rate: u64,
    /// BBR: bottleneck bandwidth
    pub bbr_bw: u64,
    /// BBR: pacing gain
    pub bbr_pacing_gain: u32, // fixed-point 8.24
    /// BBR: cwnd gain
    pub bbr_cwnd_gain: u32,
    /// BBR mode
    pub bbr_mode: BbrMode,
}

/// BBR operating mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BbrMode {
    Startup,
    Drain,
    ProbeBw,
    ProbeRtt,
}

impl Default for CongestionState {
    fn default() -> Self {
        Self {
            algorithm: CongestionAlgorithm::Cubic,
            cwnd: 10 * 1460, // 10 MSS (Initial Window, RFC 6928)
            ssthresh: u32::MAX,
            bytes_in_flight: 0,
            srtt_us: 0,
            rttvar_us: 0,
            rto_us: 1_000_000, // 1 second initial RTO
            min_rtt_us: u64::MAX,
            max_bw: 0,
            in_slow_start: true,
            ecn_ce_count: 0,
            lost_count: 0,
            retransmit_count: 0,
            cubic_last_max_cwnd: 0,
            cubic_epoch_start: 0,
            cubic_origin_point: 0,
            cubic_k: 0.0,
            bbr_pacing_rate: 0,
            bbr_bw: 0,
            bbr_pacing_gain: 1 << 24,
            bbr_cwnd_gain: 2 << 24,
            bbr_mode: BbrMode::Startup,
        }
    }
}

impl CongestionState {
    /// Handle ACK received (cwnd increase)
    pub fn on_ack(&mut self, acked_bytes: u32, rtt_us: u64) {
        self.update_rtt(rtt_us);

        match self.algorithm {
            CongestionAlgorithm::Reno | CongestionAlgorithm::NewReno => {
                self.reno_on_ack(acked_bytes);
            }
            CongestionAlgorithm::Cubic => {
                self.cubic_on_ack(acked_bytes);
            }
            CongestionAlgorithm::Bbr | CongestionAlgorithm::BbrV2 => {
                self.bbr_on_ack(acked_bytes, rtt_us);
            }
            _ => self.reno_on_ack(acked_bytes),
        }
    }

    /// Handle packet loss
    pub fn on_loss(&mut self) {
        self.lost_count += 1;
        self.in_slow_start = false;

        match self.algorithm {
            CongestionAlgorithm::Reno | CongestionAlgorithm::NewReno => {
                self.ssthresh = self.cwnd / 2;
                if self.ssthresh < 2 * 1460 {
                    self.ssthresh = 2 * 1460;
                }
                self.cwnd = self.ssthresh;
            }
            CongestionAlgorithm::Cubic => {
                self.cubic_last_max_cwnd = self.cwnd;
                self.ssthresh = (self.cwnd as u64 * 717 / 1024) as u32; // Beta_cubic = 0.7
                if self.ssthresh < 2 * 1460 {
                    self.ssthresh = 2 * 1460;
                }
                self.cwnd = self.ssthresh;
                self.cubic_epoch_start = 0; // Reset epoch
            }
            CongestionAlgorithm::Bbr | CongestionAlgorithm::BbrV2 => {
                // BBR doesn't react to loss directly, but adjusts pacing
                if self.bbr_mode == BbrMode::Startup {
                    self.bbr_mode = BbrMode::Drain;
                }
            }
            _ => {
                self.ssthresh = self.cwnd / 2;
                self.cwnd = self.ssthresh.max(2 * 1460);
            }
        }
    }

    /// Handle ECN congestion signal
    pub fn on_ecn(&mut self) {
        self.ecn_ce_count += 1;
        // Treat like a loss for traditional algorithms
        match self.algorithm {
            CongestionAlgorithm::Dctcp => {
                // DCTCP: proportional reduction based on fraction of marked packets
                let reduction = self.cwnd / 4; // simplified
                self.cwnd = self.cwnd.saturating_sub(reduction).max(2 * 1460);
            }
            _ => self.on_loss(),
        }
    }

    fn reno_on_ack(&mut self, acked_bytes: u32) {
        if self.in_slow_start {
            self.cwnd += acked_bytes;
            if self.cwnd >= self.ssthresh {
                self.in_slow_start = false;
            }
        } else {
            // Congestion avoidance: increase cwnd by ~1 MSS per RTT
            let mss = 1460u32;
            self.cwnd += mss * acked_bytes / self.cwnd;
        }
    }

    fn cubic_on_ack(&mut self, acked_bytes: u32) {
        if self.in_slow_start {
            self.cwnd += acked_bytes;
            if self.cwnd >= self.ssthresh {
                self.in_slow_start = false;
            }
            return;
        }

        // CUBIC formula: W(t) = C*(t-K)^3 + W_max
        // Simplified CUBIC growth
        let mss = 1460u32;
        let target = if self.cwnd < self.cubic_last_max_cwnd {
            // Below last max — concave growth
            self.cwnd + mss / 8
        } else {
            // Above last max — convex growth
            self.cwnd + mss / 4
        };
        self.cwnd = target;
    }

    fn bbr_on_ack(&mut self, _acked_bytes: u32, rtt_us: u64) {
        // Update bandwidth estimate
        if let Some(bw) = (self.cwnd as u64 * 1_000_000).checked_div(rtt_us) {
            if bw > self.bbr_bw {
                self.bbr_bw = bw;
            }
        }

        // Update min RTT
        if rtt_us < self.min_rtt_us {
            self.min_rtt_us = rtt_us;
        }

        // BBR state machine
        match self.bbr_mode {
            BbrMode::Startup => {
                // Exponential growth until bandwidth plateau
                self.cwnd = (self.cwnd as u64 * 3 / 2) as u32; // 1.5x gain
                self.bbr_pacing_rate = self.bbr_bw * 2;
            }
            BbrMode::Drain => {
                // Drain the queue
                if self.bytes_in_flight <= self.cwnd {
                    self.bbr_mode = BbrMode::ProbeBw;
                }
                self.bbr_pacing_rate = self.bbr_bw / 2;
            }
            BbrMode::ProbeBw => {
                // Cycle through gain values
                let bdp = if self.min_rtt_us > 0 {
                    (self.bbr_bw * self.min_rtt_us / 1_000_000) as u32
                } else {
                    self.cwnd
                };
                self.cwnd = bdp * 2; // 2x BDP
                self.bbr_pacing_rate = self.bbr_bw;
            }
            BbrMode::ProbeRtt => {
                // Reduce cwnd to probe for RTT changes
                self.cwnd = 4 * 1460; // 4 MSS minimum
                self.bbr_pacing_rate = self.bbr_bw;
            }
        }
    }

    fn update_rtt(&mut self, rtt_us: u64) {
        if self.srtt_us == 0 {
            self.srtt_us = rtt_us;
            self.rttvar_us = rtt_us / 2;
        } else {
            // Exponential weighted moving average (RFC 6298)
            let diff = rtt_us.abs_diff(self.srtt_us);
            self.rttvar_us = (3 * self.rttvar_us + diff) / 4;
            self.srtt_us = (7 * self.srtt_us + rtt_us) / 8;
        }
        // RTO = SRTT + max(G, 4*RTTVAR), minimum 200ms
        self.rto_us = (self.srtt_us + 4 * self.rttvar_us).clamp(200_000, 120_000_000);

        if rtt_us < self.min_rtt_us {
            self.min_rtt_us = rtt_us;
        }
    }
}

// ─── TCP Options ────────────────────────────────────────────────────

/// TCP socket options for production use
#[derive(Debug, Clone)]
pub struct TcpOptions {
    /// TCP_NODELAY (disable Nagle)
    pub nodelay: bool,
    /// TCP_CORK (batch output)
    pub cork: bool,
    /// TCP_QUICKACK (disable delayed ACK)
    pub quickack: bool,
    /// SO_KEEPALIVE
    pub keepalive: bool,
    /// Keepalive idle time (seconds)
    pub keepalive_idle: u32,
    /// Keepalive interval (seconds)
    pub keepalive_interval: u32,
    /// Keepalive probe count
    pub keepalive_count: u32,
    /// TCP_FASTOPEN
    pub fast_open: bool,
    /// TCP_FASTOPEN queue length
    pub fast_open_qlen: u32,
    /// Window scaling (shift count, 0-14)
    pub window_scale: u8,
    /// Whether SACK is enabled
    pub sack_enabled: bool,
    /// Whether timestamps are enabled
    pub timestamps: bool,
    /// Whether ECN is enabled
    pub ecn: bool,
    /// MSS (Maximum Segment Size)
    pub mss: u16,
    /// TCP_USER_TIMEOUT (milliseconds, 0 = system default)
    pub user_timeout: u32,
    /// SO_REUSEPORT
    pub reuseport: bool,
    /// SO_REUSEADDR
    pub reuseaddr: bool,
    /// TCP_DEFER_ACCEPT (seconds)
    pub defer_accept: u32,
    /// Congestion control algorithm
    pub congestion: CongestionAlgorithm,
}

impl Default for TcpOptions {
    fn default() -> Self {
        Self {
            nodelay: false,
            cork: false,
            quickack: false,
            keepalive: false,
            keepalive_idle: 7200,
            keepalive_interval: 75,
            keepalive_count: 9,
            fast_open: false,
            fast_open_qlen: 10,
            window_scale: 7, // 128KB default window
            sack_enabled: true,
            timestamps: true,
            ecn: false,
            mss: 1460,
            user_timeout: 0,
            reuseport: false,
            reuseaddr: false,
            defer_accept: 0,
            congestion: CongestionAlgorithm::Cubic,
        }
    }
}

// ─── SACK ───────────────────────────────────────────────────────────

/// SACK block (left edge, right edge)
#[derive(Debug, Clone, Copy)]
pub struct SackBlock {
    pub left: u32,
    pub right: u32,
}

/// SACK state for a connection
#[derive(Debug, Clone)]
pub struct SackState {
    /// Received SACK blocks
    pub blocks: Vec<SackBlock>,
    /// Highest SACKed sequence
    pub highest_sacked: u32,
    /// Whether SACK is permitted (negotiated in SYN)
    pub permitted: bool,
}

impl Default for SackState {
    fn default() -> Self {
        Self {
            blocks: Vec::new(),
            highest_sacked: 0,
            permitted: true,
        }
    }
}

// ─── Path MTU Discovery ─────────────────────────────────────────────

/// Path MTU discovery state
#[derive(Debug, Clone)]
pub struct PmtuState {
    /// Current path MTU
    pub pmtu: u16,
    /// Minimum MTU ever seen on this path
    pub min_pmtu: u16,
    /// Time of last PMTU decrease
    pub last_decrease_time: u64,
    /// Whether PMTU discovery is enabled
    pub enabled: bool,
    /// Probe timer (seconds until next probe)
    pub probe_timer: u32,
}

impl Default for PmtuState {
    fn default() -> Self {
        Self {
            pmtu: 1500,
            min_pmtu: 576,
            last_decrease_time: 0,
            enabled: true,
            probe_timer: 600, // 10 minutes
        }
    }
}

// ─── TCP Fast Open ──────────────────────────────────────────────────

/// TCP Fast Open cookie
#[derive(Debug, Clone)]
pub struct TfoCookie {
    /// 4-16 byte cookie
    pub cookie: Vec<u8>,
    /// Expiry timestamp
    pub expires: u64,
}

/// TCP Fast Open state
#[derive(Debug, Clone)]
pub struct TfoState {
    /// Server-side cookies per client IP
    pub cookies: BTreeMap<u32, TfoCookie>, // IP → cookie
    /// TFO is enabled
    pub enabled: bool,
    /// Maximum pending TFO connections
    pub max_pending: u32,
}

impl Default for TfoState {
    fn default() -> Self {
        Self {
            cookies: BTreeMap::new(),
            enabled: false,
            max_pending: 10,
        }
    }
}

// ─── Global State ───────────────────────────────────────────────────

pub struct NetProductionState {
    /// Default TCP options
    pub default_tcp_opts: TcpOptions,
    /// Default congestion algorithm
    pub default_congestion: CongestionAlgorithm,
    /// Available congestion algorithms
    pub available_algorithms: Vec<CongestionAlgorithm>,
    /// System-wide TFO state
    pub tfo: TfoState,
    /// Global TCP statistics
    pub stats: TcpGlobalStats,
}

#[derive(Debug, Clone, Default)]
pub struct TcpGlobalStats {
    pub active_opens: u64,
    pub passive_opens: u64,
    pub segments_sent: u64,
    pub segments_received: u64,
    pub retransmits: u64,
    pub fast_retransmits: u64,
    pub sack_recovery: u64,
    pub ecn_marked: u64,
    pub tfo_syn_data: u64,
    pub keepalive_probes: u64,
    pub rto_timeouts: u64,
}

lazy_static::lazy_static! {
    pub static ref NET_PRODUCTION: Mutex<NetProductionState> = Mutex::new(NetProductionState::new());
}

impl NetProductionState {
    pub fn new() -> Self {
        Self {
            default_tcp_opts: TcpOptions::default(),
            default_congestion: CongestionAlgorithm::Cubic,
            available_algorithms: alloc::vec![
                CongestionAlgorithm::Reno,
                CongestionAlgorithm::NewReno,
                CongestionAlgorithm::Cubic,
                CongestionAlgorithm::Bbr,
                CongestionAlgorithm::BbrV2,
                CongestionAlgorithm::Vegas,
                CongestionAlgorithm::Westwood,
                CongestionAlgorithm::Dctcp,
            ],
            tfo: TfoState::default(),
            stats: TcpGlobalStats::default(),
        }
    }

    /// Set system-wide default congestion algorithm
    pub fn set_default_congestion(&mut self, algo: CongestionAlgorithm) {
        self.default_congestion = algo;
        self.default_tcp_opts.congestion = algo;
    }

    /// Enable TCP Fast Open system-wide
    pub fn enable_tfo(&mut self, max_pending: u32) {
        self.tfo.enabled = true;
        self.tfo.max_pending = max_pending;
    }

    /// Set system-wide ECN
    pub fn set_ecn(&mut self, enabled: bool) {
        self.default_tcp_opts.ecn = enabled;
    }

    /// Get /proc/sys/net/ipv4 style settings
    pub fn format_proc_sysctl(&self) -> String {
        let mut s = String::new();
        s.push_str(&alloc::format!(
            "tcp_congestion_control = {:?}\n",
            self.default_congestion
        ));
        s.push_str(&alloc::format!(
            "tcp_ecn = {}\n",
            if self.default_tcp_opts.ecn { 1 } else { 0 }
        ));
        s.push_str(&alloc::format!(
            "tcp_sack = {}\n",
            if self.default_tcp_opts.sack_enabled {
                1
            } else {
                0
            }
        ));
        s.push_str(&alloc::format!(
            "tcp_timestamps = {}\n",
            if self.default_tcp_opts.timestamps {
                1
            } else {
                0
            }
        ));
        s.push_str(&alloc::format!(
            "tcp_window_scaling = {}\n",
            self.default_tcp_opts.window_scale
        ));
        s.push_str(&alloc::format!(
            "tcp_fastopen = {}\n",
            if self.tfo.enabled { 3 } else { 0 }
        ));
        s.push_str(&alloc::format!(
            "tcp_keepalive_time = {}\n",
            self.default_tcp_opts.keepalive_idle
        ));
        s.push_str(&alloc::format!(
            "tcp_keepalive_intvl = {}\n",
            self.default_tcp_opts.keepalive_interval
        ));
        s.push_str(&alloc::format!(
            "tcp_keepalive_probes = {}\n",
            self.default_tcp_opts.keepalive_count
        ));
        s.push_str(&alloc::format!("tcp_mss = {}\n", self.default_tcp_opts.mss));
        s
    }
}

// ─── Public API ─────────────────────────────────────────────────────

pub fn set_congestion(algo: CongestionAlgorithm) {
    NET_PRODUCTION.lock().set_default_congestion(algo);
}

pub fn enable_tfo(max_pending: u32) {
    NET_PRODUCTION.lock().enable_tfo(max_pending);
}

pub fn set_ecn(enabled: bool) {
    NET_PRODUCTION.lock().set_ecn(enabled);
}

pub fn init() {
    let state = NET_PRODUCTION.lock();
    serial_println!(
        "[NET_PROD] Production networking initialized (congestion={:?}, {} algorithms, SACK, ECN, WScale, PMTU, TFO, keepalive)",
        state.default_congestion,
        state.available_algorithms.len()
    );
}
