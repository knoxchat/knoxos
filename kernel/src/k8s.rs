/// Kubernetes-Compatible Container Orchestration
/// Provides pod scheduling, service discovery, networking policies,
/// and a control plane for managing containerized workloads.
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

// ─── Constants ──────────────────────────────────────────────────────

const MAX_PODS: usize = 1024;
const MAX_SERVICES: usize = 256;
const MAX_NODES: usize = 64;
const MAX_NAMESPACES: usize = 128;
const POD_CIDR_BASE: [u8; 4] = [10, 244, 0, 0]; // 10.244.0.0/16
const SERVICE_CIDR_BASE: [u8; 4] = [10, 96, 0, 0]; // 10.96.0.0/12
const DNS_CLUSTER_IP: [u8; 4] = [10, 96, 0, 10];

static NEXT_POD_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_SERVICE_ID: AtomicU64 = AtomicU64::new(1);
static ORCHESTRATOR_RUNNING: AtomicBool = AtomicBool::new(false);

// ─── Pod ────────────────────────────────────────────────────────────

/// Pod status lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PodPhase {
    Pending,
    Running,
    Succeeded,
    Failed,
    Unknown,
    Terminating,
}

/// Container restart policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartPolicy {
    Always,
    OnFailure,
    Never,
}

/// Container health probe type
#[derive(Debug, Clone)]
pub enum ProbeType {
    HttpGet { path: String, port: u16 },
    TcpSocket { port: u16 },
    Exec { command: Vec<String> },
}

/// Container health probe
#[derive(Debug, Clone)]
pub struct Probe {
    pub probe_type: ProbeType,
    pub initial_delay_secs: u32,
    pub period_secs: u32,
    pub timeout_secs: u32,
    pub success_threshold: u32,
    pub failure_threshold: u32,
}

/// Resource requirements
#[derive(Debug, Clone, Copy)]
pub struct ResourceRequirements {
    pub cpu_milli: u32,    // Millicores (1000 = 1 CPU)
    pub memory_bytes: u64, // Bytes
    pub ephemeral_storage: u64,
}

impl Default for ResourceRequirements {
    fn default() -> Self {
        Self {
            cpu_milli: 100,
            memory_bytes: 64 * 1024 * 1024, // 64 MiB
            ephemeral_storage: 0,
        }
    }
}

/// Container specification
#[derive(Debug, Clone)]
pub struct ContainerSpec {
    pub name: String,
    pub image: String,
    pub command: Vec<String>,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub ports: Vec<ContainerPort>,
    pub resources_requests: ResourceRequirements,
    pub resources_limits: ResourceRequirements,
    pub volume_mounts: Vec<VolumeMount>,
    pub liveness_probe: Option<Probe>,
    pub readiness_probe: Option<Probe>,
    pub startup_probe: Option<Probe>,
    pub image_pull_policy: ImagePullPolicy,
    pub security_context: Option<SecurityContext>,
}

/// Container port mapping
#[derive(Debug, Clone)]
pub struct ContainerPort {
    pub name: String,
    pub container_port: u16,
    pub protocol: Protocol,
    pub host_port: Option<u16>,
}

/// Volume mount
#[derive(Debug, Clone)]
pub struct VolumeMount {
    pub name: String,
    pub mount_path: String,
    pub read_only: bool,
    pub sub_path: Option<String>,
}

/// Image pull policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImagePullPolicy {
    Always,
    IfNotPresent,
    Never,
}

/// Container security context
#[derive(Debug, Clone)]
pub struct SecurityContext {
    pub run_as_user: Option<u32>,
    pub run_as_group: Option<u32>,
    pub run_as_non_root: bool,
    pub read_only_root_fs: bool,
    pub privileged: bool,
    pub allow_privilege_escalation: bool,
    pub capabilities_add: Vec<String>,
    pub capabilities_drop: Vec<String>,
}

/// Container runtime state
#[derive(Debug, Clone)]
pub struct ContainerStatus {
    pub name: String,
    pub ready: bool,
    pub restart_count: u32,
    pub state: ContainerState,
    pub last_state: Option<ContainerState>,
    pub image: String,
    pub container_id: u64,
}

/// Container state
#[derive(Debug, Clone)]
pub enum ContainerState {
    Waiting {
        reason: String,
    },
    Running {
        started_at: u64,
    },
    Terminated {
        exit_code: i32,
        reason: String,
        finished_at: u64,
    },
}

/// Pod volume
#[derive(Debug, Clone)]
pub enum Volume {
    EmptyDir { medium: String, size_limit: u64 },
    HostPath { path: String, volume_type: String },
    ConfigMap { name: String },
    Secret { name: String },
    PersistentVolumeClaim { claim_name: String, read_only: bool },
}

/// Pod specification
#[derive(Debug, Clone)]
pub struct PodSpec {
    pub containers: Vec<ContainerSpec>,
    pub init_containers: Vec<ContainerSpec>,
    pub volumes: Vec<(String, Volume)>,
    pub restart_policy: RestartPolicy,
    pub termination_grace_period: u64,
    pub node_selector: BTreeMap<String, String>,
    pub tolerations: Vec<Toleration>,
    pub affinity: Option<Affinity>,
    pub service_account: String,
    pub hostname: Option<String>,
    pub subdomain: Option<String>,
    pub dns_policy: DnsPolicy,
    pub priority: i32,
    pub priority_class_name: Option<String>,
}

/// DNS policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsPolicy {
    ClusterFirst,
    Default,
    ClusterFirstWithHostNet,
    None,
}

/// Pod toleration for taints
#[derive(Debug, Clone)]
pub struct Toleration {
    pub key: String,
    pub operator: TolerationOperator,
    pub value: Option<String>,
    pub effect: TaintEffect,
    pub toleration_seconds: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TolerationOperator {
    Exists,
    Equal,
}

/// Pod affinity/anti-affinity
#[derive(Debug, Clone)]
pub struct Affinity {
    pub node_affinity: Option<NodeAffinity>,
    pub pod_affinity: Vec<PodAffinityTerm>,
    pub pod_anti_affinity: Vec<PodAffinityTerm>,
}

#[derive(Debug, Clone)]
pub struct NodeAffinity {
    pub required: Vec<NodeSelectorTerm>,
    pub preferred: Vec<(i32, NodeSelectorTerm)>, // weight, term
}

#[derive(Debug, Clone)]
pub struct NodeSelectorTerm {
    pub match_expressions: Vec<NodeSelectorRequirement>,
}

#[derive(Debug, Clone)]
pub struct NodeSelectorRequirement {
    pub key: String,
    pub operator: SelectorOperator,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorOperator {
    In,
    NotIn,
    Exists,
    DoesNotExist,
    Gt,
    Lt,
}

#[derive(Debug, Clone)]
pub struct PodAffinityTerm {
    pub label_selector: BTreeMap<String, String>,
    pub topology_key: String,
}

/// Pod
pub struct Pod {
    pub id: u64,
    pub name: String,
    pub namespace: String,
    pub labels: BTreeMap<String, String>,
    pub annotations: BTreeMap<String, String>,
    pub spec: PodSpec,
    pub phase: PodPhase,
    pub pod_ip: Option<[u8; 4]>,
    pub host_ip: Option<[u8; 4]>,
    pub node_name: Option<String>,
    pub start_time: u64,
    pub container_statuses: Vec<ContainerStatus>,
    pub conditions: Vec<PodCondition>,
    pub qos_class: QoSClass,
    pub uid: String,
}

/// Pod condition
#[derive(Debug, Clone)]
pub struct PodCondition {
    pub condition_type: PodConditionType,
    pub status: bool,
    pub reason: String,
    pub message: String,
    pub last_transition_time: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PodConditionType {
    PodScheduled,
    Initialized,
    ContainersReady,
    Ready,
}

/// QoS class
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QoSClass {
    Guaranteed,
    Burstable,
    BestEffort,
}

// ─── Service ────────────────────────────────────────────────────────

/// Protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    TCP,
    UDP,
    SCTP,
}

/// Service type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceType {
    ClusterIP,
    NodePort,
    LoadBalancer,
    ExternalName,
}

/// Service port
#[derive(Debug, Clone)]
pub struct ServicePort {
    pub name: String,
    pub protocol: Protocol,
    pub port: u16,
    pub target_port: u16,
    pub node_port: Option<u16>,
}

/// Service definition
pub struct Service {
    pub id: u64,
    pub name: String,
    pub namespace: String,
    pub service_type: ServiceType,
    pub cluster_ip: Option<[u8; 4]>,
    pub external_ips: Vec<[u8; 4]>,
    pub ports: Vec<ServicePort>,
    pub selector: BTreeMap<String, String>,
    pub labels: BTreeMap<String, String>,
    pub session_affinity: SessionAffinity,
    pub load_balancer_ip: Option<[u8; 4]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAffinity {
    None,
    ClientIP,
}

// ─── Endpoint ───────────────────────────────────────────────────────

/// Endpoint for service discovery
#[derive(Debug, Clone)]
pub struct Endpoint {
    pub ip: [u8; 4],
    pub port: u16,
    pub node_name: Option<String>,
    pub ready: bool,
    pub serving: bool,
    pub terminating: bool,
}

pub struct EndpointSlice {
    pub service_name: String,
    pub namespace: String,
    pub address_type: AddressType,
    pub endpoints: Vec<Endpoint>,
    pub ports: Vec<ServicePort>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressType {
    IPv4,
    IPv6,
    FQDN,
}

// ─── Node ───────────────────────────────────────────────────────────

/// Node (worker machine) in the cluster
pub struct Node {
    pub name: String,
    pub labels: BTreeMap<String, String>,
    pub annotations: BTreeMap<String, String>,
    pub taints: Vec<Taint>,
    pub capacity: NodeResources,
    pub allocatable: NodeResources,
    pub conditions: Vec<NodeCondition>,
    pub addresses: Vec<NodeAddress>,
    pub os_image: String,
    pub kernel_version: String,
    pub container_runtime: String,
    pub kubelet_version: String,
    pub unschedulable: bool,
}

#[derive(Debug, Clone)]
pub struct Taint {
    pub key: String,
    pub value: Option<String>,
    pub effect: TaintEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaintEffect {
    NoSchedule,
    PreferNoSchedule,
    NoExecute,
}

#[derive(Debug, Clone, Copy)]
pub struct NodeResources {
    pub cpu_milli: u32,
    pub memory_bytes: u64,
    pub pods: u32,
    pub ephemeral_storage: u64,
}

#[derive(Debug, Clone)]
pub struct NodeCondition {
    pub condition_type: NodeConditionType,
    pub status: bool,
    pub reason: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeConditionType {
    Ready,
    MemoryPressure,
    DiskPressure,
    PIDPressure,
    NetworkUnavailable,
}

#[derive(Debug, Clone)]
pub struct NodeAddress {
    pub address_type: NodeAddressType,
    pub address: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeAddressType {
    InternalIP,
    ExternalIP,
    Hostname,
}

// ─── Namespace ──────────────────────────────────────────────────────

pub struct Namespace {
    pub name: String,
    pub labels: BTreeMap<String, String>,
    pub annotations: BTreeMap<String, String>,
    pub phase: NamespacePhase,
    pub resource_quotas: Vec<ResourceQuota>,
    pub limit_ranges: Vec<LimitRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamespacePhase {
    Active,
    Terminating,
}

#[derive(Debug, Clone)]
pub struct ResourceQuota {
    pub name: String,
    pub hard: BTreeMap<String, u64>,
    pub used: BTreeMap<String, u64>,
}

#[derive(Debug, Clone)]
pub struct LimitRange {
    pub name: String,
    pub limit_type: LimitType,
    pub default_request: ResourceRequirements,
    pub default_limit: ResourceRequirements,
    pub min: ResourceRequirements,
    pub max: ResourceRequirements,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitType {
    Container,
    Pod,
    PersistentVolumeClaim,
}

// ─── Network Policy ─────────────────────────────────────────────────

/// Network policy for pod communication control
pub struct NetworkPolicy {
    pub name: String,
    pub namespace: String,
    pub pod_selector: BTreeMap<String, String>,
    pub policy_types: Vec<PolicyType>,
    pub ingress_rules: Vec<IngressRule>,
    pub egress_rules: Vec<EgressRule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyType {
    Ingress,
    Egress,
}

#[derive(Debug, Clone)]
pub struct IngressRule {
    pub from: Vec<NetworkPolicyPeer>,
    pub ports: Vec<NetworkPolicyPort>,
}

#[derive(Debug, Clone)]
pub struct EgressRule {
    pub to: Vec<NetworkPolicyPeer>,
    pub ports: Vec<NetworkPolicyPort>,
}

#[derive(Debug, Clone)]
pub struct NetworkPolicyPeer {
    pub pod_selector: Option<BTreeMap<String, String>>,
    pub namespace_selector: Option<BTreeMap<String, String>>,
    pub ip_block: Option<IpBlock>,
}

#[derive(Debug, Clone)]
pub struct IpBlock {
    pub cidr: String,
    pub except: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct NetworkPolicyPort {
    pub protocol: Protocol,
    pub port: u16,
    pub end_port: Option<u16>,
}

// ─── Deployment / ReplicaSet ────────────────────────────────────────

/// Deployment for declarative pod management
pub struct Deployment {
    pub name: String,
    pub namespace: String,
    pub labels: BTreeMap<String, String>,
    pub replicas: u32,
    pub selector: BTreeMap<String, String>,
    pub template: PodSpec,
    pub strategy: DeploymentStrategy,
    pub revision_history_limit: u32,
    pub progress_deadline_secs: u64,
    pub status: DeploymentStatus,
}

#[derive(Debug, Clone)]
pub enum DeploymentStrategy {
    RollingUpdate {
        max_unavailable: u32,
        max_surge: u32,
    },
    Recreate,
}

#[derive(Debug, Clone)]
pub struct DeploymentStatus {
    pub observed_generation: u64,
    pub replicas: u32,
    pub updated_replicas: u32,
    pub ready_replicas: u32,
    pub available_replicas: u32,
    pub unavailable_replicas: u32,
}

/// ReplicaSet
pub struct ReplicaSet {
    pub name: String,
    pub namespace: String,
    pub replicas: u32,
    pub selector: BTreeMap<String, String>,
    pub template: PodSpec,
    pub owner_deployment: Option<String>,
    pub status: ReplicaSetStatus,
}

#[derive(Debug, Clone)]
pub struct ReplicaSetStatus {
    pub replicas: u32,
    pub ready_replicas: u32,
    pub available_replicas: u32,
}

// ─── ConfigMap & Secret ─────────────────────────────────────────────

pub struct ConfigMap {
    pub name: String,
    pub namespace: String,
    pub data: BTreeMap<String, String>,
    pub binary_data: BTreeMap<String, Vec<u8>>,
    pub immutable: bool,
}

pub struct Secret {
    pub name: String,
    pub namespace: String,
    pub secret_type: SecretType,
    pub data: BTreeMap<String, Vec<u8>>,
    pub immutable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretType {
    Opaque,
    DockerConfigJson,
    BasicAuth,
    SshAuth,
    TlsCert,
    ServiceAccountToken,
    BootstrapToken,
}

// ─── Ingress ────────────────────────────────────────────────────────

/// Ingress for HTTP/HTTPS routing
pub struct Ingress {
    pub name: String,
    pub namespace: String,
    pub ingress_class: Option<String>,
    pub tls: Vec<IngressTls>,
    pub rules: Vec<HttpIngressRule>,
    pub default_backend: Option<IngressBackend>,
}

#[derive(Debug, Clone)]
pub struct IngressTls {
    pub hosts: Vec<String>,
    pub secret_name: String,
}

#[derive(Debug, Clone)]
pub struct HttpIngressRule {
    pub host: Option<String>,
    pub paths: Vec<HttpIngressPath>,
}

#[derive(Debug, Clone)]
pub struct HttpIngressPath {
    pub path: String,
    pub path_type: PathType,
    pub backend: IngressBackend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathType {
    Exact,
    Prefix,
    ImplementationSpecific,
}

#[derive(Debug, Clone)]
pub struct IngressBackend {
    pub service_name: String,
    pub service_port: u16,
}

// ─── DaemonSet / StatefulSet / Job / CronJob ────────────────────────

/// DaemonSet ensures a pod runs on every node
pub struct DaemonSet {
    pub name: String,
    pub namespace: String,
    pub selector: BTreeMap<String, String>,
    pub template: PodSpec,
    pub update_strategy: DaemonSetUpdateStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonSetUpdateStrategy {
    RollingUpdate,
    OnDelete,
}

/// StatefulSet for stateful applications
pub struct StatefulSet {
    pub name: String,
    pub namespace: String,
    pub replicas: u32,
    pub selector: BTreeMap<String, String>,
    pub template: PodSpec,
    pub volume_claim_templates: Vec<PersistentVolumeClaim>,
    pub service_name: String,
    pub pod_management_policy: PodManagementPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PodManagementPolicy {
    OrderedReady,
    Parallel,
}

/// Job for batch workloads
pub struct Job {
    pub name: String,
    pub namespace: String,
    pub parallelism: u32,
    pub completions: u32,
    pub backoff_limit: u32,
    pub active_deadline_secs: Option<u64>,
    pub template: PodSpec,
    pub status: JobStatus,
}

#[derive(Debug, Clone)]
pub struct JobStatus {
    pub active: u32,
    pub succeeded: u32,
    pub failed: u32,
    pub start_time: u64,
    pub completion_time: Option<u64>,
}

/// CronJob for scheduled workloads
pub struct CronJob {
    pub name: String,
    pub namespace: String,
    pub schedule: String, // Cron expression
    pub concurrency_policy: ConcurrencyPolicy,
    pub starting_deadline_secs: Option<u64>,
    pub successful_jobs_history_limit: u32,
    pub failed_jobs_history_limit: u32,
    pub job_template: Job,
    pub suspend: bool,
    pub last_schedule_time: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConcurrencyPolicy {
    Allow,
    Forbid,
    Replace,
}

// ─── PersistentVolume / PersistentVolumeClaim ───────────────────────

pub struct PersistentVolume {
    pub name: String,
    pub capacity_bytes: u64,
    pub access_modes: Vec<AccessMode>,
    pub reclaim_policy: ReclaimPolicy,
    pub storage_class: Option<String>,
    pub phase: PVPhase,
    pub claim_ref: Option<(String, String)>, // namespace, name
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    ReadWriteOnce,
    ReadOnlyMany,
    ReadWriteMany,
    ReadWriteOncePod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReclaimPolicy {
    Retain,
    Delete,
    Recycle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PVPhase {
    Available,
    Bound,
    Released,
    Failed,
}

#[derive(Debug, Clone)]
pub struct PersistentVolumeClaim {
    pub name: String,
    pub namespace: String,
    pub access_modes: Vec<AccessMode>,
    pub storage_class: Option<String>,
    pub requested_bytes: u64,
    pub phase: PVCPhase,
    pub volume_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PVCPhase {
    Pending,
    Bound,
    Lost,
}

// ─── RBAC ───────────────────────────────────────────────────────────

pub struct Role {
    pub name: String,
    pub namespace: String,
    pub rules: Vec<PolicyRule>,
}

pub struct ClusterRole {
    pub name: String,
    pub rules: Vec<PolicyRule>,
    pub aggregation_rule: Option<Vec<BTreeMap<String, String>>>,
}

#[derive(Debug, Clone)]
pub struct PolicyRule {
    pub api_groups: Vec<String>,
    pub resources: Vec<String>,
    pub verbs: Vec<String>,
    pub resource_names: Vec<String>,
}

pub struct RoleBinding {
    pub name: String,
    pub namespace: String,
    pub role_ref: RoleRef,
    pub subjects: Vec<Subject>,
}

pub struct ClusterRoleBinding {
    pub name: String,
    pub role_ref: RoleRef,
    pub subjects: Vec<Subject>,
}

#[derive(Debug, Clone)]
pub struct RoleRef {
    pub api_group: String,
    pub kind: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Subject {
    pub kind: SubjectKind,
    pub name: String,
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectKind {
    User,
    Group,
    ServiceAccount,
}

// ─── ServiceAccount ─────────────────────────────────────────────────

pub struct ServiceAccount {
    pub name: String,
    pub namespace: String,
    pub secrets: Vec<String>,
    pub image_pull_secrets: Vec<String>,
    pub automount_service_account_token: bool,
}

// ─── Scheduler ──────────────────────────────────────────────────────

/// Pod scheduler
pub struct KubeScheduler {
    pub scheduling_queue: Vec<u64>, // Pod IDs
}

impl KubeScheduler {
    pub fn new() -> Self {
        Self {
            scheduling_queue: Vec::new(),
        }
    }

    /// Score a node for a pod placement (higher = better)
    pub fn score_node(node: &Node, pod: &PodSpec) -> i64 {
        let mut score: i64 = 100;

        // Check resource availability
        let total_cpu_request: u32 = pod
            .containers
            .iter()
            .map(|c| c.resources_requests.cpu_milli)
            .sum();
        let total_mem_request: u64 = pod
            .containers
            .iter()
            .map(|c| c.resources_requests.memory_bytes)
            .sum();

        if total_cpu_request > node.allocatable.cpu_milli {
            return -1; // Insufficient CPU
        }
        if total_mem_request > node.allocatable.memory_bytes {
            return -1; // Insufficient memory
        }

        // Prefer nodes with more available resources (spread)
        let cpu_ratio = (node.allocatable.cpu_milli - total_cpu_request) as i64;
        let mem_ratio =
            ((node.allocatable.memory_bytes - total_mem_request) / (1024 * 1024)) as i64;
        score += cpu_ratio / 100 + mem_ratio / 100;

        // Check node selector
        for (key, val) in &pod.node_selector {
            if node.labels.get(key) != Some(val) {
                return -1; // Node selector mismatch
            }
        }

        // Check taints/tolerations
        for taint in &node.taints {
            let tolerated = pod
                .tolerations
                .iter()
                .any(|t| t.key == taint.key && t.effect == taint.effect);
            if !tolerated && taint.effect == TaintEffect::NoSchedule {
                return -1;
            }
        }

        // Unschedulable nodes are skipped
        if node.unschedulable {
            return -1;
        }

        score
    }

    /// Schedule a pod to the best node
    pub fn schedule_pod(pod: &PodSpec, nodes: &[Node]) -> Option<usize> {
        let mut best_score: i64 = -1;
        let mut best_node: Option<usize> = None;

        for (i, node) in nodes.iter().enumerate() {
            let score = Self::score_node(node, pod);
            if score > best_score {
                best_score = score;
                best_node = Some(i);
            }
        }

        best_node
    }
}

// ─── Controller Manager ─────────────────────────────────────────────

/// Controllers reconcile desired state with actual state
pub struct ControllerManager {
    pub reconcile_interval_secs: u64,
}

impl ControllerManager {
    pub fn new() -> Self {
        Self {
            reconcile_interval_secs: 10,
        }
    }

    /// Reconcile deployment: ensure correct number of replicas
    pub fn reconcile_deployment(deployment: &Deployment, current_pods: &[&Pod]) -> Vec<PodAction> {
        let mut actions = Vec::new();
        let desired = deployment.replicas as usize;
        let running: usize = current_pods
            .iter()
            .filter(|p| p.phase == PodPhase::Running || p.phase == PodPhase::Pending)
            .count();

        if running < desired {
            // Scale up
            for _ in 0..(desired - running) {
                actions.push(PodAction::Create);
            }
        } else if running > desired {
            // Scale down
            for _ in 0..(running - desired) {
                actions.push(PodAction::Delete);
            }
        }

        actions
    }

    /// Reconcile DaemonSet: ensure a pod on every eligible node
    pub fn reconcile_daemonset(
        ds: &DaemonSet,
        nodes: &[Node],
        pods: &[&Pod],
    ) -> Vec<(PodAction, Option<String>)> {
        let mut actions = Vec::new();

        for node in nodes {
            let has_pod = pods
                .iter()
                .any(|p| p.node_name.as_deref() == Some(&node.name));
            if !has_pod && !node.unschedulable {
                actions.push((PodAction::Create, Some(node.name.clone())));
            }
        }

        actions
    }
}

/// Pod action from controller reconciliation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PodAction {
    Create,
    Delete,
    Update,
}

// ─── Service Discovery (CoreDNS-compatible) ─────────────────────────

/// Cluster DNS for service discovery
pub struct ClusterDns {
    pub domain: String,
    pub records: BTreeMap<String, Vec<DnsRecord>>,
}

#[derive(Debug, Clone)]
pub enum DnsRecord {
    A(String, [u8; 4]),
    SRV {
        target: String,
        port: u16,
        priority: u16,
        weight: u16,
    },
    CNAME(String, String),
}

impl ClusterDns {
    pub fn new() -> Self {
        Self {
            domain: String::from("cluster.local"),
            records: BTreeMap::new(),
        }
    }

    /// Register a service in DNS
    pub fn register_service(
        &mut self,
        name: &str,
        namespace: &str,
        cluster_ip: [u8; 4],
        ports: &[ServicePort],
    ) {
        let fqdn = format!("{}.{}.svc.{}", name, namespace, self.domain);

        // A record
        let a_records = self.records.entry(fqdn.clone()).or_default();
        a_records.push(DnsRecord::A(fqdn.clone(), cluster_ip));

        // SRV records for each port
        for port in ports {
            let srv_name = format!(
                "_{}._{}.{}",
                port.name,
                match port.protocol {
                    Protocol::TCP => "tcp",
                    Protocol::UDP => "udp",
                    Protocol::SCTP => "sctp",
                },
                fqdn
            );
            let srv_records = self.records.entry(srv_name).or_default();
            srv_records.push(DnsRecord::SRV {
                target: fqdn.clone(),
                port: port.port,
                priority: 0,
                weight: 100,
            });
        }
    }

    /// Resolve a service name
    pub fn resolve(&self, name: &str) -> Option<&Vec<DnsRecord>> {
        self.records.get(name)
    }
}

// ─── HPA (Horizontal Pod Autoscaler) ────────────────────────────────

pub struct HorizontalPodAutoscaler {
    pub name: String,
    pub namespace: String,
    pub target_ref: String, // Deployment name
    pub min_replicas: u32,
    pub max_replicas: u32,
    pub metrics: Vec<MetricSpec>,
    pub current_replicas: u32,
    pub desired_replicas: u32,
}

#[derive(Debug, Clone)]
pub struct MetricSpec {
    pub metric_type: MetricType,
    pub target_value: u64,
    pub current_value: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricType {
    CpuUtilization,
    MemoryUtilization,
    CustomMetric,
}

impl HorizontalPodAutoscaler {
    /// Calculate desired replicas based on metrics
    pub fn calculate_desired_replicas(&self) -> u32 {
        let mut max_desired = self.current_replicas;

        for metric in &self.metrics {
            if metric.target_value == 0 {
                continue;
            }
            let ratio = (metric.current_value as f64) / (metric.target_value as f64);
            let scaled = (self.current_replicas as f64) * ratio;
            // Manual ceil: if fractional part > 0, round up
            let truncated = scaled as u32;
            let desired = if (truncated as f64) < scaled {
                truncated + 1
            } else {
                truncated
            };
            if desired > max_desired {
                max_desired = desired;
            }
        }

        // Clamp to min/max
        max_desired.max(self.min_replicas).min(self.max_replicas)
    }
}

// ─── Orchestrator State ─────────────────────────────────────────────

use spin::Mutex;

static PODS: Mutex<BTreeMap<u64, Pod>> = Mutex::new(BTreeMap::new());
static SERVICES: Mutex<BTreeMap<u64, Service>> = Mutex::new(BTreeMap::new());
static K8S_NAMESPACES: Mutex<Vec<Namespace>> = Mutex::new(Vec::new());
static CLUSTER_DNS: Mutex<Option<ClusterDns>> = Mutex::new(None);

/// Create a pod
pub fn create_pod(name: &str, namespace: &str, spec: PodSpec) -> u64 {
    let id = NEXT_POD_ID.fetch_add(1, Ordering::Relaxed);
    let pod = Pod {
        id,
        name: String::from(name),
        namespace: String::from(namespace),
        labels: BTreeMap::new(),
        annotations: BTreeMap::new(),
        spec,
        phase: PodPhase::Pending,
        pod_ip: None,
        host_ip: Some([10, 0, 2, 15]),
        node_name: None,
        start_time: 0,
        container_statuses: Vec::new(),
        conditions: Vec::new(),
        qos_class: QoSClass::BestEffort,
        uid: format!("pod-{}", id),
    };
    PODS.lock().insert(id, pod);
    id
}

/// Delete a pod
pub fn delete_pod(id: u64) -> bool {
    PODS.lock().remove(&id).is_some()
}

/// Get pod count
pub fn pod_count() -> usize {
    PODS.lock().len()
}

/// Create a service
pub fn create_service(
    name: &str,
    namespace: &str,
    service_type: ServiceType,
    ports: Vec<ServicePort>,
    selector: BTreeMap<String, String>,
) -> u64 {
    let id = NEXT_SERVICE_ID.fetch_add(1, Ordering::Relaxed);
    let cluster_ip = if service_type != ServiceType::ExternalName {
        let octet = (id as u8).wrapping_add(1);
        Some([SERVICE_CIDR_BASE[0], SERVICE_CIDR_BASE[1], 0, octet])
    } else {
        None
    };

    // Register in DNS
    if let Some(ip) = cluster_ip {
        if let Some(ref mut dns) = *CLUSTER_DNS.lock() {
            dns.register_service(name, namespace, ip, &ports);
        }
    }

    let svc = Service {
        id,
        name: String::from(name),
        namespace: String::from(namespace),
        service_type,
        cluster_ip,
        external_ips: Vec::new(),
        ports,
        selector,
        labels: BTreeMap::new(),
        session_affinity: SessionAffinity::None,
        load_balancer_ip: None,
    };
    SERVICES.lock().insert(id, svc);
    id
}

/// Create a namespace
pub fn create_namespace(name: &str) {
    let mut ns = K8S_NAMESPACES.lock();
    if !ns.iter().any(|n| n.name == name) {
        ns.push(Namespace {
            name: String::from(name),
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            phase: NamespacePhase::Active,
            resource_quotas: Vec::new(),
            limit_ranges: Vec::new(),
        });
    }
}

/// List namespaces
pub fn list_namespaces() -> Vec<String> {
    K8S_NAMESPACES
        .lock()
        .iter()
        .map(|n| n.name.clone())
        .collect()
}

// ─── Initialization ─────────────────────────────────────────────────

pub fn init() {
    // Create default namespaces
    create_namespace("default");
    create_namespace("kube-system");
    create_namespace("kube-public");
    create_namespace("kube-node-lease");

    // Initialize cluster DNS
    *CLUSTER_DNS.lock() = Some(ClusterDns::new());

    // Register built-in services
    let dns_ports = alloc::vec![ServicePort {
        name: String::from("dns"),
        protocol: Protocol::UDP,
        port: 53,
        target_port: 53,
        node_port: None,
    }];
    create_service(
        "kube-dns",
        "kube-system",
        ServiceType::ClusterIP,
        dns_ports,
        BTreeMap::new(),
    );

    let api_ports = alloc::vec![ServicePort {
        name: String::from("https"),
        protocol: Protocol::TCP,
        port: 443,
        target_port: 6443,
        node_port: None,
    }];
    create_service(
        "kubernetes",
        "default",
        ServiceType::ClusterIP,
        api_ports,
        BTreeMap::new(),
    );

    ORCHESTRATOR_RUNNING.store(true, Ordering::Release);

    crate::serial_println!("[KnoxOS] Container orchestration initialized (Kubernetes-compatible)");
    crate::serial_println!(
        "[KnoxOS]   Namespaces: default, kube-system, kube-public, kube-node-lease"
    );
    crate::serial_println!("[KnoxOS]   Pod CIDR: 10.244.0.0/16");
    crate::serial_println!("[KnoxOS]   Service CIDR: 10.96.0.0/12");
    crate::serial_println!("[KnoxOS]   Cluster DNS: 10.96.0.10 (cluster.local)");
    crate::serial_println!("[KnoxOS]   Built-in services: kube-dns, kubernetes");
}
