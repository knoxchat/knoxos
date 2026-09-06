// numa.rs — Non-Uniform Memory Access topology and policy
// NUMA-aware memory allocation, node distance, migration

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

/// Maximum number of NUMA nodes
pub const MAX_NUMNODES: usize = 64;

/// NUMA memory policy modes (matching Linux mbind/set_mempolicy)
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u32)]
pub enum NumaPolicy {
    Default = 0,       // MPOL_DEFAULT — system default
    Preferred = 1,     // MPOL_PREFERRED — prefer specific node
    Bind = 2,          // MPOL_BIND — restrict to specific nodes
    Interleave = 3,    // MPOL_INTERLEAVE — round-robin across nodes
    Local = 4,         // MPOL_LOCAL — allocate on local node
    PreferredMany = 5, // MPOL_PREFERRED_MANY — prefer set of nodes
}

/// NUMA policy flags
pub const MPOL_F_STATIC_NODES: u32 = 1 << 15;
pub const MPOL_F_RELATIVE_NODES: u32 = 1 << 14;
pub const MPOL_F_NUMA_BALANCING: u32 = 1 << 13;

/// mbind flags
pub const MPOL_MF_STRICT: u32 = 1;
pub const MPOL_MF_MOVE: u32 = 2;
pub const MPOL_MF_MOVE_ALL: u32 = 4;

/// move_pages flags
pub const MPOL_MF_MOVE_MT: u32 = 8;

/// NUMA node information
#[derive(Debug, Clone)]
pub struct NumaNode {
    pub id: u32,
    pub cpu_mask: u64,                 // CPUs on this node
    pub memory_total: u64,             // Total memory in bytes
    pub memory_free: u64,              // Free memory in bytes
    pub memory_used: u64,              // Used memory in bytes
    pub distance: [u32; MAX_NUMNODES], // Distance to other nodes
    pub hugepages_total: u64,
    pub hugepages_free: u64,
}

impl NumaNode {
    pub fn new(id: u32) -> Self {
        let mut distances = [255u32; MAX_NUMNODES];
        distances[id as usize] = 10; // Local distance is always 10

        NumaNode {
            id,
            cpu_mask: 0,
            memory_total: 0,
            memory_free: 0,
            memory_used: 0,
            distance: distances,
            hugepages_total: 0,
            hugepages_free: 0,
        }
    }
}

/// Per-process NUMA memory policy
#[derive(Debug, Clone)]
pub struct ProcessNumaPolicy {
    pub pid: u64,
    pub policy: NumaPolicy,
    pub nodemask: u64, // Bitmask of nodes
    pub flags: u32,
}

impl Default for ProcessNumaPolicy {
    fn default() -> Self {
        ProcessNumaPolicy {
            pid: 0,
            policy: NumaPolicy::Default,
            nodemask: 0,
            flags: 0,
        }
    }
}

/// VMA-level NUMA policy (from mbind)
#[derive(Debug, Clone)]
pub struct VmaNumaPolicy {
    pub start: u64,
    pub len: u64,
    pub policy: NumaPolicy,
    pub nodemask: u64,
    pub flags: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumaError {
    InvalidNode,
    InvalidPolicy,
    InvalidNodemask,
    PermDenied,
    NoMemory,
}

lazy_static! {
    static ref TOPOLOGY: Mutex<NumaTopology> = Mutex::new(NumaTopology::new());
    static ref PROCESS_POLICIES: Mutex<BTreeMap<u64, ProcessNumaPolicy>> =
        Mutex::new(BTreeMap::new());
}

struct NumaTopology {
    nodes: Vec<NumaNode>,
    num_nodes: usize,
    online_nodes: u64, // Bitmask
}

impl NumaTopology {
    fn new() -> Self {
        // Default single-node topology (most systems)
        let mut node0 = NumaNode::new(0);
        node0.cpu_mask = 0xFF; // Assume 8 CPUs on node 0
        node0.memory_total = 8 * 1024 * 1024 * 1024; // 8 GiB
        node0.memory_free = 6 * 1024 * 1024 * 1024;
        node0.memory_used = 2 * 1024 * 1024 * 1024;

        NumaTopology {
            nodes: alloc::vec![node0],
            num_nodes: 1,
            online_nodes: 1, // Node 0 is online
        }
    }
}

/// Get number of NUMA nodes
pub fn num_nodes() -> usize {
    TOPOLOGY.lock().num_nodes
}

/// Get online node bitmask
pub fn online_nodes() -> u64 {
    TOPOLOGY.lock().online_nodes
}

/// Get NUMA node info
pub fn get_node_info(node: u32) -> Option<NumaNodeInfo> {
    let topo = TOPOLOGY.lock();
    topo.nodes.get(node as usize).map(|n| NumaNodeInfo {
        id: n.id,
        cpu_mask: n.cpu_mask,
        memory_total: n.memory_total,
        memory_free: n.memory_free,
        memory_used: n.memory_used,
    })
}

#[derive(Debug, Clone)]
pub struct NumaNodeInfo {
    pub id: u32,
    pub cpu_mask: u64,
    pub memory_total: u64,
    pub memory_free: u64,
    pub memory_used: u64,
}

/// Get distance between two NUMA nodes
pub fn node_distance(from: u32, to: u32) -> u32 {
    let topo = TOPOLOGY.lock();
    if let Some(node) = topo.nodes.get(from as usize) {
        if (to as usize) < MAX_NUMNODES {
            return node.distance[to as usize];
        }
    }
    255 // Max distance if unknown
}

/// Get the local NUMA node for a CPU
pub fn cpu_to_node(cpu: u32) -> u32 {
    let topo = TOPOLOGY.lock();
    for node in &topo.nodes {
        if node.cpu_mask & (1 << cpu) != 0 {
            return node.id;
        }
    }
    0 // Default to node 0
}

/// set_mempolicy — set process NUMA memory policy
pub fn sys_set_mempolicy(
    pid: u64,
    policy: NumaPolicy,
    nodemask: u64,
    flags: u32,
) -> Result<(), NumaError> {
    // Validate nodemask
    let topo = TOPOLOGY.lock();
    let valid_mask = (1u64 << topo.num_nodes) - 1;
    if nodemask & !valid_mask != 0 {
        return Err(NumaError::InvalidNodemask);
    }
    drop(topo);

    let proc_policy = ProcessNumaPolicy {
        pid,
        policy,
        nodemask,
        flags,
    };

    PROCESS_POLICIES.lock().insert(pid, proc_policy);
    Ok(())
}

/// get_mempolicy — get process NUMA memory policy
pub fn sys_get_mempolicy(pid: u64) -> ProcessNumaPolicy {
    PROCESS_POLICIES
        .lock()
        .get(&pid)
        .cloned()
        .unwrap_or_default()
}

/// mbind — bind a memory range to NUMA nodes
pub fn sys_mbind(
    _pid: u64,
    addr: u64,
    len: u64,
    policy: NumaPolicy,
    nodemask: u64,
    _flags: u32,
) -> Result<(), NumaError> {
    // In real implementation, would update VMA policies
    let _ = addr;
    let _ = len;
    let _ = policy;
    let _ = nodemask;
    Ok(())
}

/// migrate_pages — migrate pages of a process to another node
pub fn sys_migrate_pages(pid: u64, _old_nodes: u64, new_nodes: u64) -> Result<u64, NumaError> {
    // In real implementation, would move physical pages between nodes
    let _ = pid;
    let _ = new_nodes;
    Ok(0) // Number of pages that could not be moved
}

/// move_pages — move specific pages to specific nodes
pub fn sys_move_pages(
    _pid: u64,
    _count: usize,
    _pages: &[u64],
    _nodes: &[i32],
    _status: &mut [i32],
    _flags: u32,
) -> Result<(), NumaError> {
    // In real implementation, would move individual pages
    Ok(())
}

/// Determine which NUMA node to allocate from based on policy
pub fn allocate_node(pid: u64, preferred_node: u32) -> u32 {
    let policies = PROCESS_POLICIES.lock();
    if let Some(policy) = policies.get(&pid) {
        match policy.policy {
            NumaPolicy::Default | NumaPolicy::Local => preferred_node,
            NumaPolicy::Preferred => {
                // Return first node in nodemask
                for i in 0..MAX_NUMNODES {
                    if policy.nodemask & (1 << i) != 0 {
                        return i as u32;
                    }
                }
                preferred_node
            }
            NumaPolicy::Bind => {
                // Must allocate from nodemask nodes
                for i in 0..MAX_NUMNODES {
                    if policy.nodemask & (1 << i) != 0 {
                        return i as u32;
                    }
                }
                preferred_node
            }
            NumaPolicy::Interleave => {
                // Round-robin (simplified — real impl uses page counter)
                let tsc = unsafe {
                    let mut lo: u32 = 0;
                    let mut hi: u32 = 0;
                    #[cfg(target_arch = "x86_64")]
                    core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi);
                    (hi as u64) << 32 | lo as u64
                };
                let mut nodes = Vec::new();
                for i in 0..MAX_NUMNODES {
                    if policy.nodemask & (1 << i) != 0 {
                        nodes.push(i as u32);
                    }
                }
                if nodes.is_empty() {
                    preferred_node
                } else {
                    nodes[(tsc as usize) % nodes.len()]
                }
            }
            NumaPolicy::PreferredMany => {
                // Try nodes in order of preference
                for i in 0..MAX_NUMNODES {
                    if policy.nodemask & (1 << i) != 0 {
                        let topo = TOPOLOGY.lock();
                        if let Some(node) = topo.nodes.get(i) {
                            if node.memory_free > 0 {
                                return i as u32;
                            }
                        }
                    }
                }
                preferred_node
            }
        }
    } else {
        preferred_node
    }
}

/// Generate /proc/buddyinfo-like NUMA memory info
pub fn proc_numa_info() -> String {
    let topo = TOPOLOGY.lock();
    let mut output = String::new();
    for node in &topo.nodes {
        output.push_str(&alloc::format!(
            "Node {} MemTotal: {} kB\nNode {} MemFree: {} kB\nNode {} MemUsed: {} kB\n",
            node.id,
            node.memory_total / 1024,
            node.id,
            node.memory_free / 1024,
            node.id,
            node.memory_used / 1024,
        ));
    }
    output
}

/// Generate /sys/devices/system/node info
pub fn sys_node_info() -> String {
    let topo = TOPOLOGY.lock();
    let mut output = String::new();
    output.push_str(&alloc::format!("node_count: {}\n", topo.num_nodes));
    output.push_str(&alloc::format!("online_nodes: {:#x}\n", topo.online_nodes));
    for i in 0..topo.num_nodes {
        for j in 0..topo.num_nodes {
            output.push_str(&alloc::format!(
                "node_distance[{}][{}] = {}\n",
                i,
                j,
                topo.nodes[i].distance[j]
            ));
        }
    }
    output
}

/// Initialize NUMA subsystem
pub fn init() {
    // Detect real NUMA topology from ACPI SRAT table
    detect_topology_from_acpi();

    let topo = TOPOLOGY.lock();
    crate::serial_println!(
        "  NUMA subsystem initialized ({} node(s), {}MB total memory)",
        topo.num_nodes,
        topo.nodes.iter().map(|n| n.memory_total).sum::<u64>() / (1024 * 1024),
    );
}

/// Parse ACPI SRAT (System Resource Affinity Table) for NUMA topology
fn detect_topology_from_acpi() {
    // Read SRAT from ACPI tables if available
    if let Some(srat_data) = crate::vfs::read_file_dispatch("/sys/firmware/acpi/tables/SRAT") {
        parse_srat_table(&srat_data);
    } else {
        // Fall back to CPUID-based detection
        detect_topology_from_cpuid();
    }
}

/// Parse SRAT table entries
fn parse_srat_table(data: &[u8]) {
    if data.len() < 48 || &data[0..4] != b"SRAT" {
        return;
    }

    let table_length = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let mut offset = 48; // Skip header + reserved

    let mut topo = TOPOLOGY.lock();

    while offset + 2 <= table_length.min(data.len()) {
        let entry_type = data[offset];
        let entry_length = data[offset + 1] as usize;

        if entry_length < 2 {
            break;
        }

        match entry_type {
            0
                // Processor Local APIC Affinity (Type 0, 16 bytes)
                if offset + 16 <= data.len() => {
                    let proximity_domain_lo = data[offset + 2] as u32;
                    let apic_id = data[offset + 3];
                    let flags = u32::from_le_bytes([
                        data[offset + 4],
                        data[offset + 5],
                        data[offset + 6],
                        data[offset + 7],
                    ]);
                    let proximity_domain_hi = u32::from_le_bytes([
                        data[offset + 12],
                        data[offset + 13],
                        data[offset + 14],
                        0,
                    ]);
                    let domain = proximity_domain_lo | (proximity_domain_hi << 8);
                    let enabled = flags & 1 != 0;

                    if enabled {
                        // Ensure node exists
                        while topo.nodes.len() <= domain as usize {
                            let id = topo.nodes.len() as u32;
                            topo.nodes.push(NumaNode::new(id));
                        }
                        topo.nodes[domain as usize].cpu_mask |= 1u64 << (apic_id as u64);
                        topo.num_nodes = topo.nodes.len();
                        topo.online_nodes |= 1u64 << domain;
                    }
                }
            1
                // Memory Affinity (Type 1, 40 bytes)
                if offset + 40 <= data.len() => {
                    let domain = u32::from_le_bytes([
                        data[offset + 2],
                        data[offset + 3],
                        data[offset + 4],
                        data[offset + 5],
                    ]);
                    let base = u64::from_le_bytes([
                        data[offset + 8],
                        data[offset + 9],
                        data[offset + 10],
                        data[offset + 11],
                        data[offset + 12],
                        data[offset + 13],
                        data[offset + 14],
                        data[offset + 15],
                    ]);
                    let length = u64::from_le_bytes([
                        data[offset + 16],
                        data[offset + 17],
                        data[offset + 18],
                        data[offset + 19],
                        data[offset + 20],
                        data[offset + 21],
                        data[offset + 22],
                        data[offset + 23],
                    ]);
                    let flags = u32::from_le_bytes([
                        data[offset + 28],
                        data[offset + 29],
                        data[offset + 30],
                        data[offset + 31],
                    ]);
                    let enabled = flags & 1 != 0;

                    if enabled {
                        while topo.nodes.len() <= domain as usize {
                            let id = topo.nodes.len() as u32;
                            topo.nodes.push(NumaNode::new(id));
                        }
                        topo.nodes[domain as usize].memory_total += length;
                        topo.nodes[domain as usize].memory_free += length;
                        topo.num_nodes = topo.nodes.len();
                        topo.online_nodes |= 1u64 << domain;

                        crate::serial_println!(
                            "[NUMA] Memory affinity: node={} base={:#x} size={}MB",
                            domain,
                            base,
                            length / (1024 * 1024)
                        );
                    }
                }
            2
                // Processor Local x2APIC Affinity (Type 2, 24 bytes)
                if offset + 24 <= data.len() => {
                    let domain = u32::from_le_bytes([
                        data[offset + 4],
                        data[offset + 5],
                        data[offset + 6],
                        data[offset + 7],
                    ]);
                    let x2apic_id = u32::from_le_bytes([
                        data[offset + 8],
                        data[offset + 9],
                        data[offset + 10],
                        data[offset + 11],
                    ]);
                    let flags = u32::from_le_bytes([
                        data[offset + 12],
                        data[offset + 13],
                        data[offset + 14],
                        data[offset + 15],
                    ]);

                    if flags & 1 != 0 && x2apic_id < 64 {
                        while topo.nodes.len() <= domain as usize {
                            let id = topo.nodes.len() as u32;
                            topo.nodes.push(NumaNode::new(id));
                        }
                        topo.nodes[domain as usize].cpu_mask |= 1u64 << x2apic_id;
                        topo.num_nodes = topo.nodes.len();
                        topo.online_nodes |= 1u64 << domain;
                    }
                }
            _ => {} // Skip unknown types
        }

        offset += entry_length;
    }

    // Parse SLIT (System Locality Information Table) for distances
    if let Some(slit_data) = crate::vfs::read_file_dispatch("/sys/firmware/acpi/tables/SLIT") {
        parse_slit_table(&slit_data, &mut topo);
    }
}

/// Parse SLIT table for inter-node distances
fn parse_slit_table(data: &[u8], topo: &mut NumaTopology) {
    if data.len() < 44 || &data[0..4] != b"SLIT" {
        return;
    }

    let num_localities = u64::from_le_bytes([
        data[36], data[37], data[38], data[39], data[40], data[41], data[42], data[43],
    ]) as usize;

    let matrix_start = 44;
    for i in 0..num_localities.min(topo.num_nodes) {
        for j in 0..num_localities.min(MAX_NUMNODES) {
            let idx = matrix_start + i * num_localities + j;
            if idx < data.len() && i < topo.nodes.len() {
                topo.nodes[i].distance[j] = data[idx] as u32;
            }
        }
    }

    crate::serial_println!(
        "[NUMA] Parsed SLIT: {} × {} distance matrix",
        num_localities,
        num_localities
    );
}

/// Detect NUMA topology from CPUID (fallback when no SRAT)
fn detect_topology_from_cpuid() {
    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();

    // Check for CPUID leaf 0x0B (Extended Topology) or 0x1F (V2 Extended Topology)
    if let Some(ext_topo) = cpuid.get_extended_topology_info() {
        let mut max_pkg = 0u32;
        for level in ext_topo {
            if level.level_type() == crate::arch_compat::raw_cpuid::TopologyType::Core {
                crate::serial_println!(
                    "[NUMA] CPUID topology: {} logical processors",
                    level.processors()
                );
            }
        }
    }

    // Without SRAT, assume single-node topology
    crate::serial_println!("[NUMA] No SRAT found, using single-node topology");
}
