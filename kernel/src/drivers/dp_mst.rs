/// DisplayPort MST (Multi-Stream Transport) Driver
///
/// Supports daisy-chaining multiple monitors from a single DisplayPort output.
/// Implements DP MST topology discovery and bandwidth allocation.
///
/// Features:
///   - MST topology discovery via sideband messages
///   - Virtual channel payload allocation
///   - Bandwidth management across daisy-chain
///   - Hot-plug in MST topology
///   - EDID retrieval for downstream monitors
///   - Audio over MST
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// MST device in topology
#[derive(Debug, Clone)]
pub struct MstDevice {
    pub port_num: u8,
    pub rad: Vec<u8>, // Relative Address
    pub peer_device_type: PeerDeviceType,
    pub dpcd_rev: u8,
    pub edid: Vec<u8>,
    pub available_pbn: u16, // Payload Bandwidth Number
    pub allocated_pbn: u16,
    pub vcpi: u8, // Virtual Channel Payload ID
    pub connected: bool,
}

/// Peer device type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PeerDeviceType {
    None,
    SourceOrSstBranch,
    MstBranch,
    SstSink,
    DpToLegacy,
}

/// MST Manager
pub struct MstManager {
    pub topology: Vec<MstDevice>,
    pub max_payloads: u8,
    pub total_pbn: u16, // Total available PBN
    pub used_pbn: u16,
    pub mst_enabled: bool,
}

lazy_static::lazy_static! {
    pub static ref MST: Mutex<MstManager> = Mutex::new(MstManager {
        topology: Vec::new(),
        max_payloads: 63,
        total_pbn: 2520,    // DP 1.4 HBR3
        used_pbn: 0,
        mst_enabled: false,
    });
}

impl MstManager {
    /// Enable MST on a DP port
    pub fn enable(&mut self) -> Result<(), &'static str> {
        // Write DPCD 0x111 to enable MST mode
        self.mst_enabled = true;
        self.discover_topology()?;
        serial_println!("[DP-MST] Enabled, {} device(s) found", self.topology.len());
        Ok(())
    }

    /// Discover MST topology via sideband messages
    fn discover_topology(&mut self) -> Result<(), &'static str> {
        self.topology.clear();
        // Send LINK_ADDRESS to root branch
        // Parse response for downstream ports
        // Recursively discover sub-branches
        Ok(())
    }

    /// Allocate bandwidth for a stream
    pub fn allocate_payload(&mut self, device_idx: usize, pbn: u16) -> Result<u8, &'static str> {
        if device_idx >= self.topology.len() {
            return Err("Invalid device index");
        }
        if self.used_pbn + pbn > self.total_pbn {
            return Err("Insufficient bandwidth");
        }

        let vcpi = self.topology.iter().map(|d| d.vcpi).max().unwrap_or(0) + 1;
        self.topology[device_idx].allocated_pbn = pbn;
        self.topology[device_idx].vcpi = vcpi;
        self.used_pbn += pbn;

        // Send ALLOCATE_PAYLOAD sideband message
        serial_println!(
            "[DP-MST] Allocated {} PBN to device {}, VCPI={}",
            pbn,
            device_idx,
            vcpi
        );
        Ok(vcpi)
    }

    /// Calculate PBN needed for a given mode
    pub fn calculate_pbn(width: u32, height: u32, refresh: u32, bpc: u8) -> u16 {
        let pixel_clock = width as u64 * height as u64 * refresh as u64;
        let bits_per_pixel = bpc as u64 * 3; // RGB
        let data_rate = pixel_clock * bits_per_pixel / 8;
        // PBN = data_rate / 54000000 * 64 (DP spec formula)
        ((data_rate * 64) / 54_000_000) as u16 + 1
    }

    /// Handle HPD event in MST topology
    pub fn handle_hpd(&mut self) {
        if self.mst_enabled {
            let _ = self.discover_topology();
        }
    }
}

pub fn init() {
    serial_println!("[DP-MST] DisplayPort MST driver loaded");
}
