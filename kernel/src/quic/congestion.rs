use super::constants::{INITIAL_CWND, INITIAL_RTT, MAX_QUIC_PACKET, MIN_CWND};

/// Congestion control algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CongestionAlgorithm {
    /// New Reno (RFC 9002)
    NewReno,
    /// Cubic
    Cubic,
    /// BBR
    Bbr,
}

/// Congestion control state
#[derive(Debug, Clone)]
pub struct CongestionController {
    pub algorithm: CongestionAlgorithm,
    /// Congestion window (bytes)
    pub cwnd: u64,
    /// Slow start threshold
    pub ssthresh: u64,
    /// Bytes in flight
    pub bytes_in_flight: u64,
    /// Smoothed RTT (microseconds)
    pub smoothed_rtt: u64,
    /// RTT variance
    pub rttvar: u64,
    /// Minimum RTT observed
    pub min_rtt: u64,
    /// Latest RTT sample
    pub latest_rtt: u64,
    /// In slow start phase
    pub in_slow_start: bool,
    /// Recovery start packet number
    pub recovery_start_pn: u64,
    /// ECN CE counter
    pub ecn_ce_count: u64,
}

impl CongestionController {
    pub fn new(algorithm: CongestionAlgorithm) -> Self {
        Self {
            algorithm,
            cwnd: INITIAL_CWND,
            ssthresh: u64::MAX,
            bytes_in_flight: 0,
            smoothed_rtt: INITIAL_RTT * 1000, // Convert to microseconds
            rttvar: INITIAL_RTT * 500,
            min_rtt: u64::MAX,
            latest_rtt: 0,
            in_slow_start: true,
            recovery_start_pn: 0,
            ecn_ce_count: 0,
        }
    }

    /// Update RTT estimates
    pub fn update_rtt(&mut self, rtt_sample: u64) {
        self.latest_rtt = rtt_sample;
        if self.min_rtt > rtt_sample {
            self.min_rtt = rtt_sample;
        }
        if self.smoothed_rtt == INITIAL_RTT * 1000 {
            self.smoothed_rtt = rtt_sample;
            self.rttvar = rtt_sample / 2;
        } else {
            let abs_diff = rtt_sample.abs_diff(self.smoothed_rtt);
            self.rttvar = (3 * self.rttvar + abs_diff) / 4;
            self.smoothed_rtt = (7 * self.smoothed_rtt + rtt_sample) / 8;
        }
    }

    /// On packet acknowledged
    pub fn on_ack(&mut self, acked_bytes: u64) {
        self.bytes_in_flight = self.bytes_in_flight.saturating_sub(acked_bytes);
        if self.in_slow_start {
            self.cwnd += acked_bytes;
            if self.cwnd >= self.ssthresh {
                self.in_slow_start = false;
            }
        } else {
            // Congestion avoidance (New Reno)
            self.cwnd += (MAX_QUIC_PACKET as u64 * acked_bytes) / self.cwnd;
        }
    }

    /// On packet loss detected
    pub fn on_loss(&mut self, lost_pn: u64) {
        if lost_pn >= self.recovery_start_pn {
            self.recovery_start_pn = lost_pn + 1;
            self.ssthresh = core::cmp::max(self.cwnd / 2, MIN_CWND);
            self.cwnd = self.ssthresh;
            self.in_slow_start = false;
        }
    }

    /// Get PTO (Probe Timeout) duration in microseconds
    pub fn pto(&self) -> u64 {
        self.smoothed_rtt + core::cmp::max(4 * self.rttvar, 1000)
    }

    /// Can send data?
    pub fn can_send(&self, packet_size: u64) -> bool {
        self.bytes_in_flight + packet_size <= self.cwnd
    }
}
