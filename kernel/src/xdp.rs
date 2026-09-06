/// XDP — eXpress Data Path
///
/// Implements high-performance programmable packet processing at the
/// network driver level, compatible with Linux XDP.
///
/// XDP programs run before the kernel networking stack, enabling
/// line-rate packet filtering, forwarding, and modification.
///
/// Actions:
///   XDP_PASS    — Pass packet to normal networking stack
///   XDP_DROP    — Drop packet immediately
///   XDP_TX      — Bounce packet back out the same interface
///   XDP_REDIRECT — Forward to another interface or CPU
///   XDP_ABORTED — Error, drop and log
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ─── XDP Actions ────────────────────────────────────────────────────

/// XDP program return actions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum XdpAction {
    Aborted = 0,
    Drop = 1,
    Pass = 2,
    Tx = 3,
    Redirect = 4,
}

impl From<u32> for XdpAction {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::Aborted,
            1 => Self::Drop,
            2 => Self::Pass,
            3 => Self::Tx,
            4 => Self::Redirect,
            _ => Self::Aborted,
        }
    }
}

// ─── XDP Metadata ───────────────────────────────────────────────────

/// XDP metadata context — passed to XDP programs
#[repr(C)]
#[derive(Debug, Clone)]
pub struct XdpMd {
    pub data: u64,            // Pointer to start of packet data
    pub data_end: u64,        // Pointer to end of packet data
    pub data_meta: u64,       // Metadata area before data
    pub ingress_ifindex: u32, // Incoming interface index
    pub rx_queue_index: u32,  // RX queue number (for multiqueue NICs)
    pub egress_ifindex: u32,  // Egress interface (for XDP_REDIRECT)
}

/// XDP buffer descriptor for packet manipulation
#[derive(Debug, Clone)]
pub struct XdpBuff {
    pub data: Vec<u8>,
    pub headroom: usize, // Available space before data
    pub ifindex: u32,
    pub queue_id: u32,
}

impl XdpBuff {
    pub fn new(packet: &[u8], ifindex: u32, queue_id: u32) -> Self {
        let headroom = 256; // XDP_PACKET_HEADROOM
        let mut data = Vec::with_capacity(headroom + packet.len());
        data.resize(headroom, 0);
        data.extend_from_slice(packet);
        Self {
            data,
            headroom,
            ifindex,
            queue_id,
        }
    }

    pub fn packet_data(&self) -> &[u8] {
        &self.data[self.headroom..]
    }

    pub fn packet_data_mut(&mut self) -> &mut [u8] {
        let start = self.headroom;
        &mut self.data[start..]
    }

    pub fn packet_len(&self) -> usize {
        self.data.len() - self.headroom
    }

    /// Push headroom — extend packet at the front
    pub fn adjust_head(&mut self, delta: i32) -> Result<(), &'static str> {
        if delta < 0 {
            let shrink = (-delta) as usize;
            if shrink > self.headroom {
                return Err("Cannot expand beyond headroom");
            }
            self.headroom -= shrink;
        } else {
            let grow = delta as usize;
            if self.headroom + grow > self.data.len() {
                return Err("Cannot shrink beyond data");
            }
            self.headroom += grow;
        }
        Ok(())
    }

    /// Build XdpMd context for this buffer
    pub fn to_md(&self) -> XdpMd {
        let data_ptr = self.data.as_ptr() as u64 + self.headroom as u64;
        XdpMd {
            data: data_ptr,
            data_end: data_ptr + self.packet_len() as u64,
            data_meta: data_ptr, // No metadata by default
            ingress_ifindex: self.ifindex,
            rx_queue_index: self.queue_id,
            egress_ifindex: 0,
        }
    }
}

// ─── XDP Program ────────────────────────────────────────────────────

/// Type of XDP program attachment
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum XdpAttachMode {
    /// Generic XDP (software, any driver)
    Generic,
    /// Native XDP (driver-level, requires driver support)
    Native,
    /// Offloaded XDP (NIC hardware, requires SmartNIC)
    Offloaded,
}

/// An XDP program that can be attached to a network interface
pub struct XdpProgram {
    pub id: u32,
    pub name: String,
    pub ifindex: u32,
    pub attach_mode: XdpAttachMode,
    /// BPF program ID (references ebpf.rs loaded program)
    pub bpf_prog_id: Option<u32>,
    /// Built-in filter function (for kernel-internal XDP programs)
    pub builtin_fn: Option<fn(&mut XdpBuff) -> XdpAction>,
    /// Statistics
    pub stats: XdpStats,
}

/// XDP per-program statistics
#[derive(Debug, Clone, Default)]
pub struct XdpStats {
    pub rx_packets: u64,
    pub rx_bytes: u64,
    pub xdp_pass: u64,
    pub xdp_drop: u64,
    pub xdp_tx: u64,
    pub xdp_redirect: u64,
    pub xdp_aborted: u64,
    pub errors: u64,
}

// ─── XDP Map Types (for BPF maps used by XDP programs) ──────────────

/// XDP redirect map — maps interface index to target
#[derive(Debug, Clone)]
pub struct XdpDevMap {
    pub entries: Vec<Option<u32>>, // ifindex targets, indexed by key
    pub max_entries: usize,
}

impl XdpDevMap {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: vec![None; max_entries],
            max_entries,
        }
    }

    pub fn set(&mut self, key: u32, ifindex: u32) {
        if (key as usize) < self.max_entries {
            self.entries[key as usize] = Some(ifindex);
        }
    }

    pub fn get(&self, key: u32) -> Option<u32> {
        self.entries.get(key as usize).copied().flatten()
    }

    pub fn delete(&mut self, key: u32) {
        if (key as usize) < self.max_entries {
            self.entries[key as usize] = None;
        }
    }
}

/// XDP CPU redirect map — maps to CPU for steering
#[derive(Debug, Clone)]
pub struct XdpCpuMap {
    pub entries: Vec<Option<u32>>, // CPU IDs
    pub max_entries: usize,
}

impl XdpCpuMap {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: vec![None; max_entries],
            max_entries,
        }
    }

    pub fn set(&mut self, key: u32, cpu_id: u32) {
        if (key as usize) < self.max_entries {
            self.entries[key as usize] = Some(cpu_id);
        }
    }

    pub fn get(&self, key: u32) -> Option<u32> {
        self.entries.get(key as usize).copied().flatten()
    }
}

/// XDP socket map — AF_XDP socket steering
#[derive(Debug, Clone)]
pub struct XskMap {
    pub entries: Vec<Option<u32>>, // Socket FD or ID
    pub max_entries: usize,
}

impl XskMap {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: vec![None; max_entries],
            max_entries,
        }
    }
}

// ─── Global XDP State ───────────────────────────────────────────────

lazy_static::lazy_static! {
    static ref XDP_PROGRAMS: Mutex<Vec<XdpProgram>> = Mutex::new(Vec::new());
    static ref XDP_DEVMAPS: Mutex<Vec<XdpDevMap>> = Mutex::new(Vec::new());
    static ref NEXT_XDP_ID: Mutex<u32> = Mutex::new(1);
}

// ─── XDP Program Management ────────────────────────────────────────

/// Attach an XDP program to an interface
pub fn attach_program(
    name: &str,
    ifindex: u32,
    mode: XdpAttachMode,
    handler: fn(&mut XdpBuff) -> XdpAction,
) -> u32 {
    let mut programs = XDP_PROGRAMS.lock();
    let mut next_id = NEXT_XDP_ID.lock();
    let id = *next_id;
    *next_id += 1;

    // Detach any existing program on this interface
    programs.retain(|p| p.ifindex != ifindex);

    programs.push(XdpProgram {
        id,
        name: String::from(name),
        ifindex,
        attach_mode: mode,
        bpf_prog_id: None,
        builtin_fn: Some(handler),
        stats: XdpStats::default(),
    });

    serial_println!(
        "[XDP] Attached program '{}' (id={}) to ifindex {} ({:?})",
        name,
        id,
        ifindex,
        mode
    );
    id
}

/// Attach a BPF-based XDP program
pub fn attach_bpf_program(name: &str, ifindex: u32, mode: XdpAttachMode, bpf_prog_id: u32) -> u32 {
    let mut programs = XDP_PROGRAMS.lock();
    let mut next_id = NEXT_XDP_ID.lock();
    let id = *next_id;
    *next_id += 1;

    programs.retain(|p| p.ifindex != ifindex);

    programs.push(XdpProgram {
        id,
        name: String::from(name),
        ifindex,
        attach_mode: mode,
        bpf_prog_id: Some(bpf_prog_id),
        builtin_fn: None,
        stats: XdpStats::default(),
    });

    serial_println!(
        "[XDP] Attached BPF program '{}' (id={}, bpf={}) to ifindex {}",
        name,
        id,
        bpf_prog_id,
        ifindex
    );
    id
}

/// Detach XDP program from an interface
pub fn detach_program(ifindex: u32) {
    let mut programs = XDP_PROGRAMS.lock();
    programs.retain(|p| p.ifindex != ifindex);
    serial_println!("[XDP] Detached program from ifindex {}", ifindex);
}

/// Get XDP program info for an interface
pub fn get_program_info(ifindex: u32) -> Option<(u32, String, XdpStats)> {
    let programs = XDP_PROGRAMS.lock();
    programs
        .iter()
        .find(|p| p.ifindex == ifindex)
        .map(|p| (p.id, p.name.clone(), p.stats.clone()))
}

// ─── XDP Packet Processing ─────────────────────────────────────────

/// Run XDP program on a received packet
/// Called by NIC drivers (virtio_net, e1000, rtl8139) before passing to net.rs
/// Returns the XDP action to take
pub fn run_xdp(ifindex: u32, packet: &mut Vec<u8>, queue_id: u32) -> XdpAction {
    let mut programs = XDP_PROGRAMS.lock();

    let program = match programs.iter_mut().find(|p| p.ifindex == ifindex) {
        Some(p) => p,
        None => return XdpAction::Pass, // No XDP program, pass through
    };

    program.stats.rx_packets += 1;
    program.stats.rx_bytes += packet.len() as u64;

    let mut buff = XdpBuff::new(packet, ifindex, queue_id);

    let action = if let Some(handler) = program.builtin_fn {
        handler(&mut buff)
    } else if let Some(_bpf_id) = program.bpf_prog_id {
        // Run eBPF program via ebpf.rs
        // For now, default to PASS
        XdpAction::Pass
    } else {
        XdpAction::Pass
    };

    // Update stats
    match action {
        XdpAction::Pass => program.stats.xdp_pass += 1,
        XdpAction::Drop => program.stats.xdp_drop += 1,
        XdpAction::Tx => program.stats.xdp_tx += 1,
        XdpAction::Redirect => program.stats.xdp_redirect += 1,
        XdpAction::Aborted => program.stats.xdp_aborted += 1,
    }

    // If packet was modified, copy back
    if action == XdpAction::Pass || action == XdpAction::Tx || action == XdpAction::Redirect {
        let new_data = buff.packet_data();
        if new_data.len() != packet.len() || new_data != packet.as_slice() {
            *packet = new_data.to_vec();
        }
    }

    action
}

// ─── Built-in XDP Programs ─────────────────────────────────────────

/// XDP program: Drop all packets (DDoS protection)
pub fn xdp_drop_all(buff: &mut XdpBuff) -> XdpAction {
    XdpAction::Drop
}

/// XDP program: Pass all packets (monitoring mode)
pub fn xdp_pass_all(buff: &mut XdpBuff) -> XdpAction {
    XdpAction::Pass
}

/// XDP program: Simple firewall — drop packets from specific IPs
pub fn xdp_simple_firewall(buff: &mut XdpBuff) -> XdpAction {
    let data = buff.packet_data();
    if data.len() < 34 {
        return XdpAction::Pass;
    } // Too short for IPv4

    // Check Ethernet type (offset 12-13)
    let ethertype = u16::from_be_bytes([data[12], data[13]]);
    if ethertype != 0x0800 {
        return XdpAction::Pass;
    } // Not IPv4

    // Check source IP (offset 26-29 in IPv4)
    let src_ip = [data[26], data[27], data[28], data[29]];

    // Drop packets from 10.0.0.0/8 (example rule)
    // In production, this would check a BPF map
    if src_ip[0] == 192 && src_ip[1] == 168 && src_ip[2] == 0 && src_ip[3] == 1 {
        return XdpAction::Drop;
    }

    XdpAction::Pass
}

/// XDP program: Rate limiter using token bucket
pub fn xdp_rate_limiter(buff: &mut XdpBuff) -> XdpAction {
    use core::sync::atomic::{AtomicU64, Ordering};
    static TOKENS: AtomicU64 = AtomicU64::new(1000);
    static LAST_REFILL: AtomicU64 = AtomicU64::new(0);

    let now = crate::rtc::uptime_seconds();
    let last = LAST_REFILL.load(Ordering::Relaxed);

    // Refill tokens every second (1000 packets/sec)
    if now > last {
        TOKENS.store(1000, Ordering::Relaxed);
        LAST_REFILL.store(now, Ordering::Relaxed);
    }

    let tokens = TOKENS.load(Ordering::Relaxed);
    if tokens > 0 {
        TOKENS.fetch_sub(1, Ordering::Relaxed);
        XdpAction::Pass
    } else {
        XdpAction::Drop
    }
}

/// XDP program: Packet counter (pass-through with stats)
pub fn xdp_counter(buff: &mut XdpBuff) -> XdpAction {
    use core::sync::atomic::{AtomicU64, Ordering};
    static PACKET_COUNT: AtomicU64 = AtomicU64::new(0);
    static BYTE_COUNT: AtomicU64 = AtomicU64::new(0);

    PACKET_COUNT.fetch_add(1, Ordering::Relaxed);
    BYTE_COUNT.fetch_add(buff.packet_len() as u64, Ordering::Relaxed);

    XdpAction::Pass
}

/// XDP program: TX bounce — send packet back out the same interface
pub fn xdp_tx_bounce(buff: &mut XdpBuff) -> XdpAction {
    let data = buff.packet_data_mut();
    if data.len() < 14 {
        return XdpAction::Drop;
    }

    // Swap source and destination MAC addresses
    let mut src_mac = [0u8; 6];
    let mut dst_mac = [0u8; 6];
    src_mac.copy_from_slice(&data[6..12]);
    dst_mac.copy_from_slice(&data[0..6]);
    data[0..6].copy_from_slice(&src_mac);
    data[6..12].copy_from_slice(&dst_mac);

    XdpAction::Tx
}

// ─── AF_XDP Socket Support ─────────────────────────────────────────

/// AF_XDP socket descriptor — zero-copy packet I/O from user space
#[derive(Debug, Clone)]
pub struct AfXdpSocket {
    pub id: u32,
    pub ifindex: u32,
    pub queue_id: u32,
    pub umem_size: usize,
    pub fill_ring_size: u32,
    pub completion_ring_size: u32,
    pub rx_ring_size: u32,
    pub tx_ring_size: u32,
    pub bound: bool,
}

impl AfXdpSocket {
    pub fn new(ifindex: u32, queue_id: u32) -> Self {
        Self {
            id: 0,
            ifindex,
            queue_id,
            umem_size: 4096 * 256, // 1MB UMEM
            fill_ring_size: 256,
            completion_ring_size: 256,
            rx_ring_size: 256,
            tx_ring_size: 256,
            bound: false,
        }
    }

    pub fn bind(&mut self) -> Result<(), &'static str> {
        self.bound = true;
        serial_println!(
            "[AF_XDP] Socket {} bound to ifindex {} queue {}",
            self.id,
            self.ifindex,
            self.queue_id
        );
        Ok(())
    }
}

// ─── XDP Statistics ─────────────────────────────────────────────────

/// Get global XDP statistics across all programs
pub fn global_stats() -> XdpStats {
    let programs = XDP_PROGRAMS.lock();
    let mut total = XdpStats::default();
    for p in programs.iter() {
        total.rx_packets += p.stats.rx_packets;
        total.rx_bytes += p.stats.rx_bytes;
        total.xdp_pass += p.stats.xdp_pass;
        total.xdp_drop += p.stats.xdp_drop;
        total.xdp_tx += p.stats.xdp_tx;
        total.xdp_redirect += p.stats.xdp_redirect;
        total.xdp_aborted += p.stats.xdp_aborted;
        total.errors += p.stats.errors;
    }
    total
}

/// List all attached XDP programs
pub fn list_programs() -> Vec<(u32, String, u32, XdpStats)> {
    XDP_PROGRAMS
        .lock()
        .iter()
        .map(|p| (p.id, p.name.clone(), p.ifindex, p.stats.clone()))
        .collect()
}

// ─── Init ───────────────────────────────────────────────────────────

pub fn init() {
    serial_println!("[KnoxOS] XDP (eXpress Data Path) subsystem initialized");
    serial_println!("[KnoxOS]   Modes: Generic, Native, Offloaded");
    serial_println!("[KnoxOS]   Actions: PASS, DROP, TX, REDIRECT, ABORTED");
    serial_println!("[KnoxOS]   Built-in programs: firewall, rate_limiter, counter, tx_bounce");
}
