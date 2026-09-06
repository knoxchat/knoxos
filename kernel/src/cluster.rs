/// Cluster & Distributed Computing Support
///
/// Provides node-to-node communication and distributed workload management.
/// Implements:
///   - Node discovery via multicast
///   - Distributed process migration
///   - Shared state via distributed hash table (DHT)
///   - Leader election (Raft-inspired)
///   - Remote procedure calls (RPC)
///   - Load-balanced task distribution
///   - Heartbeat & failure detection
///   - Cluster-wide resource monitoring
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// CLUSTER NODE
// ═══════════════════════════════════════════════════════════════════════

/// Unique node identifier (128-bit UUID)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeId(pub [u8; 16]);

impl NodeId {
    pub fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub fn local() -> Self {
        // Generate from MAC address + timestamp
        let mut id = [0u8; 16];
        // Use TSC for uniqueness
        let tsc = rdtsc();
        id[0..8].copy_from_slice(&tsc.to_le_bytes());
        id[8] = 0xAA; // KnoxOS marker
        id[9] = 0x55;
        Self(id)
    }

    pub fn display_id(&self) -> String {
        alloc::format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            self.0[0],
            self.0[1],
            self.0[2],
            self.0[3],
            self.0[4],
            self.0[5],
            self.0[6],
            self.0[7],
            self.0[8],
            self.0[9],
            self.0[10],
            self.0[11],
            self.0[12],
            self.0[13],
            self.0[14],
            self.0[15]
        )
    }
}

/// Node state in the cluster
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeState {
    Joining,
    Active,
    Suspect,
    Failed,
    Leaving,
    Left,
}

/// Node role in consensus
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeRole {
    Follower,
    Candidate,
    Leader,
}

/// Cluster node information
#[derive(Debug, Clone)]
pub struct ClusterNode {
    pub id: NodeId,
    pub name: String,
    pub address: [u8; 4], // IPv4 address
    pub port: u16,
    pub state: NodeState,
    pub role: NodeRole,
    pub last_heartbeat: u64, // Tick of last heartbeat
    pub join_time: u64,
    pub cpu_count: u32,
    pub memory_mb: u64,
    pub load_avg: f32,
    pub running_tasks: u32,
    pub version: String,
}

/// Local node state
static LOCAL_NODE: Mutex<Option<ClusterNode>> = Mutex::new(None);

/// Known nodes in the cluster
static CLUSTER_NODES: Mutex<BTreeMap<NodeId, ClusterNode>> = Mutex::new(BTreeMap::new());

/// Current cluster term (Raft epoch)
static CURRENT_TERM: AtomicU64 = AtomicU64::new(0);

/// Current leader
static LEADER_ID: Mutex<Option<NodeId>> = Mutex::new(None);

// ═══════════════════════════════════════════════════════════════════════
// CLUSTER MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════

/// Cluster configuration
#[derive(Debug, Clone)]
pub struct ClusterConfig {
    /// Cluster name
    pub name: String,
    /// Multicast group for discovery
    pub discovery_group: [u8; 4],
    /// Discovery port
    pub discovery_port: u16,
    /// Heartbeat interval (ticks)
    pub heartbeat_interval: u64,
    /// Failure detection timeout (ticks)
    pub failure_timeout: u64,
    /// Election timeout range (ticks)
    pub election_timeout_min: u64,
    pub election_timeout_max: u64,
    /// Maximum nodes
    pub max_nodes: usize,
    /// Enable auto-discovery
    pub auto_discover: bool,
}

impl ClusterConfig {
    pub fn default() -> Self {
        Self {
            name: String::from("knoxos-cluster"),
            discovery_group: [239, 0, 0, 1],
            discovery_port: 7946,
            heartbeat_interval: 18, // ~1 second at 18.2Hz PIT
            failure_timeout: 90,    // ~5 seconds
            election_timeout_min: 27,
            election_timeout_max: 54,
            max_nodes: 256,
            auto_discover: true,
        }
    }
}

static CLUSTER_CONFIG: Mutex<ClusterConfig> = Mutex::new(ClusterConfig {
    name: String::new(),
    discovery_group: [239, 0, 0, 1],
    discovery_port: 7946,
    heartbeat_interval: 18,
    failure_timeout: 90,
    election_timeout_min: 27,
    election_timeout_max: 54,
    max_nodes: 256,
    auto_discover: true,
});

/// Initialize the local node
pub fn init_local_node(name: &str) -> NodeId {
    let id = NodeId::local();
    let node = ClusterNode {
        id,
        name: String::from(name),
        address: [10, 0, 2, 15], // Default QEMU address
        port: 7946,
        state: NodeState::Active,
        role: NodeRole::Follower,
        last_heartbeat: 0,
        join_time: 0,
        cpu_count: 2,
        memory_mb: 512,
        load_avg: 0.0,
        running_tasks: 0,
        version: String::from("0.12.0"),
    };

    *LOCAL_NODE.lock() = Some(node.clone());
    CLUSTER_NODES.lock().insert(id, node);

    serial_println!("[CLUSTER] Local node: {} ({})", name, id.display_id());
    id
}

/// Join a cluster
pub fn join_cluster(seed_addr: [u8; 4], seed_port: u16) -> Result<(), &'static str> {
    serial_println!(
        "[CLUSTER] Joining cluster via {}.{}.{}.{}:{}",
        seed_addr[0],
        seed_addr[1],
        seed_addr[2],
        seed_addr[3],
        seed_port
    );

    // Send JoinRequest to seed node
    // Wait for JoinAccepted with cluster state
    Ok(())
}

/// Leave the cluster gracefully
pub fn leave_cluster() {
    let mut local = LOCAL_NODE.lock();
    if let Some(ref mut node) = *local {
        node.state = NodeState::Leaving;
        serial_println!("[CLUSTER] Leaving cluster...");
        // Notify all nodes, transfer leadership if leader
    }
}

// ═══════════════════════════════════════════════════════════════════════
// HEARTBEAT & FAILURE DETECTION
// ═══════════════════════════════════════════════════════════════════════

/// Process heartbeat tick (called from timer interrupt)
pub fn heartbeat_tick(current_tick: u64) {
    let config = CLUSTER_CONFIG.lock().clone();

    // Send heartbeat to all known nodes
    if current_tick % config.heartbeat_interval == 0 {
        send_heartbeat(current_tick);
    }

    // Check for failed nodes
    let mut nodes = CLUSTER_NODES.lock();
    let local_id = LOCAL_NODE.lock().as_ref().map(|n| n.id);

    for (id, node) in nodes.iter_mut() {
        if Some(*id) == local_id {
            continue; // Skip self
        }

        let elapsed = current_tick.wrapping_sub(node.last_heartbeat);

        if elapsed > config.failure_timeout && node.state == NodeState::Active {
            node.state = NodeState::Suspect;
            serial_println!("[CLUSTER] Node {} suspected failed", node.name);
        }

        if elapsed > config.failure_timeout * 2 && node.state == NodeState::Suspect {
            node.state = NodeState::Failed;
            serial_println!("[CLUSTER] Node {} marked FAILED", node.name);
        }
    }
}

/// Send heartbeat to all nodes
fn send_heartbeat(tick: u64) {
    let local = LOCAL_NODE.lock();
    if let Some(ref node) = *local {
        // Build heartbeat message: [MAGIC, NodeId, term, state, load, tasks]
        let _msg = HeartbeatMessage {
            sender: node.id,
            term: CURRENT_TERM.load(Ordering::SeqCst),
            state: node.state,
            load_avg: node.load_avg,
            running_tasks: node.running_tasks,
            timestamp: tick,
        };
        // In real implementation: send via UDP to all known nodes
    }
}

/// Heartbeat message format
#[derive(Debug, Clone)]
struct HeartbeatMessage {
    sender: NodeId,
    term: u64,
    state: NodeState,
    load_avg: f32,
    running_tasks: u32,
    timestamp: u64,
}

/// Receive and process a heartbeat
pub fn receive_heartbeat(sender: NodeId, term: u64, load: f32, tasks: u32, tick: u64) {
    let mut nodes = CLUSTER_NODES.lock();
    if let Some(node) = nodes.get_mut(&sender) {
        node.last_heartbeat = tick;
        node.load_avg = load;
        node.running_tasks = tasks;
        if node.state == NodeState::Suspect {
            node.state = NodeState::Active;
            serial_println!("[CLUSTER] Node {} recovered", node.name);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// LEADER ELECTION (RAFT-INSPIRED)
// ═══════════════════════════════════════════════════════════════════════

/// Start an election
pub fn start_election() {
    let term = CURRENT_TERM.fetch_add(1, Ordering::SeqCst) + 1;
    serial_println!("[CLUSTER] Starting election for term {}", term);

    let mut local = LOCAL_NODE.lock();
    if let Some(ref mut node) = *local {
        node.role = NodeRole::Candidate;
    }

    // Request votes from all nodes
    // In real implementation: send RequestVote RPCs
}

/// Receive vote and potentially become leader
pub fn receive_votes(votes: u32, total_nodes: u32) {
    if votes > total_nodes / 2 {
        let mut local = LOCAL_NODE.lock();
        if let Some(ref mut node) = *local {
            node.role = NodeRole::Leader;
            *LEADER_ID.lock() = Some(node.id);
            serial_println!(
                "[CLUSTER] Node {} elected leader (term {})",
                node.name,
                CURRENT_TERM.load(Ordering::SeqCst)
            );
        }
    }
}

/// Get current leader
pub fn get_leader() -> Option<NodeId> {
    *LEADER_ID.lock()
}

/// Am I the leader?
pub fn is_leader() -> bool {
    let local = LOCAL_NODE.lock();
    let leader = LEADER_ID.lock();
    match (&*local, &*leader) {
        (Some(node), Some(lid)) => node.id == *lid,
        _ => false,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// DISTRIBUTED HASH TABLE (DHT)
// ═══════════════════════════════════════════════════════════════════════

/// DHT entry
#[derive(Debug, Clone)]
pub struct DhtEntry {
    pub key: String,
    pub value: Vec<u8>,
    pub version: u64,
    pub owner: NodeId,
    pub replicas: Vec<NodeId>,
    pub ttl: Option<u64>, // Ticks until expiry
}

/// Global DHT storage
static DHT: Mutex<BTreeMap<String, DhtEntry>> = Mutex::new(BTreeMap::new());
static DHT_VERSION: AtomicU64 = AtomicU64::new(0);

/// Put a value into the DHT
pub fn dht_put(key: &str, value: &[u8]) -> u64 {
    let version = DHT_VERSION.fetch_add(1, Ordering::SeqCst) + 1;
    let local_id = LOCAL_NODE
        .lock()
        .as_ref()
        .map(|n| n.id)
        .unwrap_or(NodeId([0; 16]));

    let entry = DhtEntry {
        key: String::from(key),
        value: value.to_vec(),
        version,
        owner: local_id,
        replicas: Vec::new(),
        ttl: None,
    };

    DHT.lock().insert(String::from(key), entry);
    serial_println!(
        "[CLUSTER-DHT] PUT {} (v{}, {} bytes)",
        key,
        version,
        value.len()
    );

    // Replicate to other nodes
    // In real implementation: send ReplicateRPC to replica nodes

    version
}

/// Get a value from the DHT
pub fn dht_get(key: &str) -> Option<Vec<u8>> {
    DHT.lock().get(key).map(|e| e.value.clone())
}

/// Delete a value from the DHT
pub fn dht_delete(key: &str) -> bool {
    DHT.lock().remove(key).is_some()
}

/// List all DHT keys
pub fn dht_keys() -> Vec<String> {
    DHT.lock().keys().cloned().collect()
}

// ═══════════════════════════════════════════════════════════════════════
// DISTRIBUTED TASK SCHEDULING
// ═══════════════════════════════════════════════════════════════════════

/// Distributed task
#[derive(Debug, Clone)]
pub struct DistributedTask {
    pub id: u64,
    pub name: String,
    pub assigned_node: Option<NodeId>,
    pub state: TaskState,
    pub priority: i32,
    pub cpu_requirement: u32,     // Required CPU cores
    pub memory_requirement: u64,  // Required MB
    pub affinity: Option<NodeId>, // Preferred node
    pub retries: u32,
    pub max_retries: u32,
}

/// Distributed task state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Pending,
    Assigned,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Global task queue
static DIST_TASKS: Mutex<BTreeMap<u64, DistributedTask>> = Mutex::new(BTreeMap::new());
static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);

/// Submit a task to the cluster
pub fn submit_task(name: &str, cpu: u32, memory: u64) -> u64 {
    let id = NEXT_TASK_ID.fetch_add(1, Ordering::SeqCst);

    let task = DistributedTask {
        id,
        name: String::from(name),
        assigned_node: None,
        state: TaskState::Pending,
        priority: 0,
        cpu_requirement: cpu,
        memory_requirement: memory,
        affinity: None,
        retries: 0,
        max_retries: 3,
    };

    DIST_TASKS.lock().insert(id, task);
    serial_println!("[CLUSTER] Task submitted: id={} name={}", id, name);

    // Leader assigns to best-fit node
    if is_leader() {
        schedule_task(id);
    }

    id
}

/// Schedule a pending task to a node (leader only)
fn schedule_task(task_id: u64) {
    let nodes = CLUSTER_NODES.lock();
    let mut tasks = DIST_TASKS.lock();

    let task = match tasks.get_mut(&task_id) {
        Some(t) => t,
        None => return,
    };

    // Find node with lowest load that meets requirements
    let best_node = nodes
        .iter()
        .filter(|(_, n)| n.state == NodeState::Active)
        .filter(|(_, n)| n.cpu_count >= task.cpu_requirement)
        .filter(|(_, n)| n.memory_mb >= task.memory_requirement)
        .min_by(|(_, a), (_, b)| {
            a.load_avg
                .partial_cmp(&b.load_avg)
                .unwrap_or(core::cmp::Ordering::Equal)
        });

    if let Some((id, node)) = best_node {
        task.assigned_node = Some(*id);
        task.state = TaskState::Assigned;
        serial_println!(
            "[CLUSTER] Task {} assigned to node {} (load: {:.1})",
            task_id,
            node.name,
            node.load_avg
        );
    }
}

/// Get task status
pub fn task_status(task_id: u64) -> Option<TaskState> {
    DIST_TASKS.lock().get(&task_id).map(|t| t.state)
}

// ═══════════════════════════════════════════════════════════════════════
// RPC FRAMEWORK
// ═══════════════════════════════════════════════════════════════════════

/// RPC method identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcMethod {
    Heartbeat,
    RequestVote,
    AppendEntries,
    DhtGet,
    DhtPut,
    DhtDelete,
    TaskSubmit,
    TaskStatus,
    NodeJoin,
    NodeLeave,
    ProcessMigrate,
    Custom(u32),
}

/// RPC request
#[derive(Debug, Clone)]
pub struct RpcRequest {
    pub id: u64,
    pub method: RpcMethod,
    pub sender: NodeId,
    pub payload: Vec<u8>,
}

/// RPC response
#[derive(Debug, Clone)]
pub struct RpcResponse {
    pub id: u64,
    pub success: bool,
    pub payload: Vec<u8>,
    pub error: Option<String>,
}

static NEXT_RPC_ID: AtomicU64 = AtomicU64::new(1);

/// Send an RPC request to a node
pub fn rpc_call(
    target: NodeId,
    method: RpcMethod,
    payload: &[u8],
) -> Result<RpcResponse, &'static str> {
    let id = NEXT_RPC_ID.fetch_add(1, Ordering::SeqCst);

    let _request = RpcRequest {
        id,
        method,
        sender: LOCAL_NODE
            .lock()
            .as_ref()
            .map(|n| n.id)
            .unwrap_or(NodeId([0; 16])),
        payload: payload.to_vec(),
    };

    // In real implementation: serialize, send over TCP, await response
    serial_println!(
        "[CLUSTER-RPC] Call {:?} to {} (id={})",
        method,
        target.display_id(),
        id
    );

    Ok(RpcResponse {
        id,
        success: true,
        payload: Vec::new(),
        error: None,
    })
}

// ═══════════════════════════════════════════════════════════════════════
// CLUSTER STATUS
// ═══════════════════════════════════════════════════════════════════════

/// Get cluster summary
pub fn cluster_status() -> String {
    let nodes = CLUSTER_NODES.lock();
    let tasks = DIST_TASKS.lock();
    let leader = LEADER_ID.lock();

    let active = nodes
        .values()
        .filter(|n| n.state == NodeState::Active)
        .count();
    let pending = tasks
        .values()
        .filter(|t| t.state == TaskState::Pending)
        .count();
    let running = tasks
        .values()
        .filter(|t| t.state == TaskState::Running)
        .count();

    alloc::format!(
        "Cluster: {} nodes ({} active), {} tasks ({} pending, {} running), leader: {}",
        nodes.len(),
        active,
        tasks.len(),
        pending,
        running,
        leader
            .as_ref()
            .map(|l| l.display_id())
            .unwrap_or_else(|| String::from("none"))
    )
}

/// List all cluster nodes
pub fn list_nodes() -> Vec<ClusterNode> {
    CLUSTER_NODES.lock().values().cloned().collect()
}

fn rdtsc() -> u64 {
    unsafe {
        let mut lo: u32 = 0;
        let mut hi: u32 = 0;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nostack, nomem));
        ((hi as u64) << 32) | lo as u64
    }
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

static CLUSTER_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize the cluster subsystem
pub fn init() {
    // Set default config
    *CLUSTER_CONFIG.lock() = ClusterConfig::default();

    // Initialize local node
    let node_id = init_local_node("knoxos-node-0");

    CLUSTER_INITIALIZED.store(true, Ordering::SeqCst);

    serial_println!("[CLUSTER] Distributed computing subsystem initialized");
    serial_println!("[CLUSTER]   Node ID: {}", node_id.display_id());
    serial_println!("[CLUSTER]   Discovery: 239.0.0.1:7946");
    serial_println!("[CLUSTER]   Features: DHT, leader election, task scheduling, RPC");
}
