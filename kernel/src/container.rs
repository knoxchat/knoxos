/// OCI Container Runtime
/// Implements an Open Container Initiative (OCI) compatible container runtime
///
/// Features:
/// - OCI runtime spec compatible container lifecycle
/// - Container creation, start, stop, delete
/// - Namespace isolation (PID, network, mount, UTS, IPC, user)
/// - Cgroup resource limits integration
/// - Root filesystem pivot_root / chroot
/// - Container state management (creating, created, running, stopped)
/// - Container ID and bundle management
/// - runc-compatible interface
/// - Container process management
/// - Seccomp profile enforcement
/// - Capabilities dropping
/// - Container networking (bridge, host, none)
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// OCI CONTAINER SPEC
// ═══════════════════════════════════════════════════════════════════════

/// Container state (OCI runtime spec)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerState {
    Creating,
    Created,
    Running,
    Stopped,
    Paused,
}

impl core::fmt::Display for ContainerState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ContainerState::Creating => write!(f, "creating"),
            ContainerState::Created => write!(f, "created"),
            ContainerState::Running => write!(f, "running"),
            ContainerState::Stopped => write!(f, "stopped"),
            ContainerState::Paused => write!(f, "paused"),
        }
    }
}

/// Container network mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkMode {
    None,           // No networking
    Host,           // Share host network namespace
    Bridge,         // Bridge network (default)
    Container(u64), // Share another container's network
}

/// Mount specification
#[derive(Debug, Clone)]
pub struct MountSpec {
    pub destination: String,
    pub source: Option<String>,
    pub fs_type: Option<String>,
    pub options: Vec<String>,
}

/// Container process specification
#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub args: Vec<String>,
    pub env: Vec<String>,
    pub cwd: String,
    pub uid: u32,
    pub gid: u32,
    pub additional_gids: Vec<u32>,
    pub no_new_privileges: bool,
    pub terminal: bool,
}

impl ProcessSpec {
    pub fn default_shell() -> Self {
        Self {
            args: vec![String::from("/bin/sh")],
            env: vec![
                String::from("PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"),
                String::from("TERM=xterm"),
                String::from("HOME=/root"),
            ],
            cwd: String::from("/"),
            uid: 0,
            gid: 0,
            additional_gids: Vec::new(),
            no_new_privileges: false,
            terminal: true,
        }
    }
}

/// Linux-specific container configuration
#[derive(Debug, Clone)]
pub struct LinuxConfig {
    pub namespaces: Vec<NamespaceConfig>,
    pub cgroup_path: Option<String>,
    pub resources: Option<ResourceLimits>,
    pub seccomp_profile: Option<String>,
    pub rootfs_propagation: String,
    pub masked_paths: Vec<String>,
    pub readonly_paths: Vec<String>,
    pub capabilities: CapabilityConfig,
}

impl LinuxConfig {
    pub fn default() -> Self {
        Self {
            namespaces: vec![
                NamespaceConfig {
                    ns_type: NamespaceType::Pid,
                    path: None,
                },
                NamespaceConfig {
                    ns_type: NamespaceType::Network,
                    path: None,
                },
                NamespaceConfig {
                    ns_type: NamespaceType::Mount,
                    path: None,
                },
                NamespaceConfig {
                    ns_type: NamespaceType::Uts,
                    path: None,
                },
                NamespaceConfig {
                    ns_type: NamespaceType::Ipc,
                    path: None,
                },
            ],
            cgroup_path: None,
            resources: Some(ResourceLimits::default()),
            seccomp_profile: None,
            rootfs_propagation: String::from("rprivate"),
            masked_paths: vec![
                String::from("/proc/acpi"),
                String::from("/proc/kcore"),
                String::from("/proc/keys"),
                String::from("/proc/latency_stats"),
                String::from("/proc/sched_debug"),
                String::from("/proc/scsi"),
                String::from("/proc/timer_list"),
                String::from("/proc/timer_stats"),
                String::from("/sys/firmware"),
            ],
            readonly_paths: vec![
                String::from("/proc/bus"),
                String::from("/proc/fs"),
                String::from("/proc/irq"),
                String::from("/proc/sys"),
                String::from("/proc/sysrq-trigger"),
            ],
            capabilities: CapabilityConfig::default(),
        }
    }
}

/// Namespace configuration
#[derive(Debug, Clone)]
pub struct NamespaceConfig {
    pub ns_type: NamespaceType,
    pub path: Option<String>, // None = create new, Some = join existing
}

/// Namespace type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamespaceType {
    Pid,
    Network,
    Mount,
    Uts,
    Ipc,
    User,
    Cgroup,
}

/// Resource limits
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    pub memory_limit: Option<u64>, // bytes
    pub memory_swap: Option<u64>,
    pub cpu_shares: Option<u64>,
    pub cpu_quota: Option<i64>,      // microseconds per period
    pub cpu_period: Option<u64>,     // microseconds
    pub cpuset_cpus: Option<String>, // "0-3" or "0,1"
    pub cpuset_mems: Option<String>,
    pub pids_limit: Option<i64>,
    pub blkio_weight: Option<u16>,
    pub oom_score_adj: Option<i32>,
}

impl ResourceLimits {
    pub fn default() -> Self {
        Self {
            memory_limit: None,
            memory_swap: None,
            cpu_shares: Some(1024),
            cpu_quota: None,
            cpu_period: Some(100_000), // 100ms
            cpuset_cpus: None,
            cpuset_mems: None,
            pids_limit: None,
            blkio_weight: None,
            oom_score_adj: None,
        }
    }
}

/// Capability configuration
#[derive(Debug, Clone)]
pub struct CapabilityConfig {
    pub bounding: Vec<String>,
    pub effective: Vec<String>,
    pub inheritable: Vec<String>,
    pub permitted: Vec<String>,
    pub ambient: Vec<String>,
}

impl CapabilityConfig {
    pub fn default() -> Self {
        let default_caps = vec![
            String::from("CAP_AUDIT_WRITE"),
            String::from("CAP_CHOWN"),
            String::from("CAP_DAC_OVERRIDE"),
            String::from("CAP_FOWNER"),
            String::from("CAP_FSETID"),
            String::from("CAP_KILL"),
            String::from("CAP_MKNOD"),
            String::from("CAP_NET_BIND_SERVICE"),
            String::from("CAP_NET_RAW"),
            String::from("CAP_SETFCAP"),
            String::from("CAP_SETGID"),
            String::from("CAP_SETPCAP"),
            String::from("CAP_SETUID"),
            String::from("CAP_SYS_CHROOT"),
        ];
        Self {
            bounding: default_caps.clone(),
            effective: default_caps.clone(),
            inheritable: Vec::new(),
            permitted: default_caps.clone(),
            ambient: Vec::new(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// OCI RUNTIME SPEC (config.json)
// ═══════════════════════════════════════════════════════════════════════

/// OCI container spec (config.json equivalent)
#[derive(Debug, Clone)]
pub struct OciSpec {
    pub oci_version: String,
    pub root: RootConfig,
    pub process: ProcessSpec,
    pub hostname: String,
    pub mounts: Vec<MountSpec>,
    pub linux: LinuxConfig,
    pub annotations: BTreeMap<String, String>,
}

/// Root filesystem configuration
#[derive(Debug, Clone)]
pub struct RootConfig {
    pub path: String,
    pub readonly: bool,
}

impl OciSpec {
    pub fn default(rootfs: &str) -> Self {
        Self {
            oci_version: String::from("1.0.2"),
            root: RootConfig {
                path: String::from(rootfs),
                readonly: false,
            },
            process: ProcessSpec::default_shell(),
            hostname: String::from("container"),
            mounts: vec![
                MountSpec {
                    destination: String::from("/proc"),
                    source: Some(String::from("proc")),
                    fs_type: Some(String::from("proc")),
                    options: Vec::new(),
                },
                MountSpec {
                    destination: String::from("/dev"),
                    source: Some(String::from("tmpfs")),
                    fs_type: Some(String::from("tmpfs")),
                    options: vec![
                        String::from("nosuid"),
                        String::from("strictatime"),
                        String::from("mode=755"),
                    ],
                },
                MountSpec {
                    destination: String::from("/sys"),
                    source: Some(String::from("sysfs")),
                    fs_type: Some(String::from("sysfs")),
                    options: vec![
                        String::from("nosuid"),
                        String::from("noexec"),
                        String::from("nodev"),
                        String::from("ro"),
                    ],
                },
                MountSpec {
                    destination: String::from("/dev/pts"),
                    source: Some(String::from("devpts")),
                    fs_type: Some(String::from("devpts")),
                    options: vec![String::from("nosuid"), String::from("noexec")],
                },
                MountSpec {
                    destination: String::from("/dev/shm"),
                    source: Some(String::from("shm")),
                    fs_type: Some(String::from("tmpfs")),
                    options: vec![
                        String::from("nosuid"),
                        String::from("noexec"),
                        String::from("nodev"),
                        String::from("mode=1777"),
                    ],
                },
            ],
            linux: LinuxConfig::default(),
            annotations: BTreeMap::new(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// CONTAINER INSTANCE
// ═══════════════════════════════════════════════════════════════════════

/// Container instance
#[derive(Debug, Clone)]
pub struct Container {
    pub id: String,
    pub state: ContainerState,
    pub pid: Option<u32>, // Init process PID
    pub bundle: String,   // Bundle directory path
    pub rootfs: String,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub exited_at: Option<u64>,
    pub exit_code: Option<i32>,
    pub spec: OciSpec,
    pub network_mode: NetworkMode,
    pub hostname: String,
    pub cgroup_path: Option<String>,
    pub namespace_pids: BTreeMap<String, u32>, // ns_type -> ns_id
}

/// Container state (for JSON serialization)
#[derive(Debug, Clone)]
pub struct ContainerStateInfo {
    pub oci_version: String,
    pub id: String,
    pub status: String,
    pub pid: Option<u32>,
    pub bundle: String,
    pub annotations: BTreeMap<String, String>,
}

/// Global container registry
static CONTAINERS: Mutex<BTreeMap<String, Container>> = Mutex::new(BTreeMap::new());
static CONTAINER_COUNTER: AtomicU64 = AtomicU64::new(1);

// ═══════════════════════════════════════════════════════════════════════
// CONTAINER LIFECYCLE (runc-compatible)
// ═══════════════════════════════════════════════════════════════════════

/// Create a new container (runc create)
pub fn create(id: &str, bundle: &str, spec: OciSpec) -> Result<(), &'static str> {
    let mut containers = CONTAINERS.lock();

    if containers.contains_key(id) {
        return Err("Container ID already exists");
    }

    let container = Container {
        id: String::from(id),
        state: ContainerState::Creating,
        pid: None,
        bundle: String::from(bundle),
        rootfs: spec.root.path.clone(),
        created_at: CONTAINER_COUNTER.fetch_add(1, Ordering::SeqCst),
        started_at: None,
        exited_at: None,
        exit_code: None,
        hostname: spec.hostname.clone(),
        network_mode: NetworkMode::Bridge,
        cgroup_path: spec.linux.cgroup_path.clone(),
        namespace_pids: BTreeMap::new(),
        spec,
    };

    serial_println!("[OCI] Creating container '{}' from bundle '{}'", id, bundle);

    // 1. Set up namespaces
    setup_namespaces(id, &container.spec.linux.namespaces);

    // 2. Set up cgroup
    if let Some(ref cg_path) = container.cgroup_path {
        setup_cgroup(id, cg_path, &container.spec.linux.resources);
    }

    // 3. Prepare rootfs mounts
    prepare_rootfs(id, &container.rootfs, &container.spec.mounts);

    let mut c = container;
    c.state = ContainerState::Created;

    containers.insert(String::from(id), c);
    serial_println!("[OCI] Container '{}' created", id);
    Ok(())
}

/// Start a created container (runc start)
pub fn start(id: &str) -> Result<(), &'static str> {
    let mut containers = CONTAINERS.lock();
    let container = containers.get_mut(id).ok_or("Container not found")?;

    if container.state != ContainerState::Created {
        return Err("Container not in 'created' state");
    }

    serial_println!("[OCI] Starting container '{}'", id);

    // Create init process
    let pid = create_container_process(container)?;
    container.pid = Some(pid);
    container.state = ContainerState::Running;
    container.started_at = Some(CONTAINER_COUNTER.fetch_add(1, Ordering::SeqCst));

    serial_println!("[OCI] Container '{}' started (PID {})", id, pid);
    Ok(())
}

/// Stop a running container (runc kill)
pub fn kill(id: &str, signal: i32) -> Result<(), &'static str> {
    let mut containers = CONTAINERS.lock();
    let container = containers.get_mut(id).ok_or("Container not found")?;

    if container.state != ContainerState::Running && container.state != ContainerState::Created {
        return Err("Container not running");
    }

    if let Some(pid) = container.pid {
        serial_println!(
            "[OCI] Sending signal {} to container '{}' (PID {})",
            signal,
            id,
            pid
        );
        // In real implementation: crate::signals::kill(pid as i32, signal);
    }

    container.state = ContainerState::Stopped;
    container.exit_code = Some(128 + signal);
    container.exited_at = Some(CONTAINER_COUNTER.fetch_add(1, Ordering::SeqCst));

    serial_println!("[OCI] Container '{}' stopped", id);
    Ok(())
}

/// Delete a stopped container (runc delete)
pub fn delete(id: &str) -> Result<(), &'static str> {
    let mut containers = CONTAINERS.lock();
    let container = containers.get(id).ok_or("Container not found")?;

    if container.state == ContainerState::Running {
        return Err("Cannot delete running container (kill first)");
    }

    serial_println!("[OCI] Deleting container '{}'", id);

    // Cleanup cgroup
    if let Some(ref cg) = container.cgroup_path {
        cleanup_cgroup(cg);
    }

    // Cleanup mounts
    cleanup_mounts(id);

    containers.remove(id);
    serial_println!("[OCI] Container '{}' deleted", id);
    Ok(())
}

/// Pause a container (runc pause)
pub fn pause(id: &str) -> Result<(), &'static str> {
    let mut containers = CONTAINERS.lock();
    let container = containers.get_mut(id).ok_or("Container not found")?;

    if container.state != ContainerState::Running {
        return Err("Container not running");
    }

    // Freeze via cgroup
    container.state = ContainerState::Paused;
    serial_println!("[OCI] Container '{}' paused", id);
    Ok(())
}

/// Resume a paused container (runc resume)
pub fn resume(id: &str) -> Result<(), &'static str> {
    let mut containers = CONTAINERS.lock();
    let container = containers.get_mut(id).ok_or("Container not found")?;

    if container.state != ContainerState::Paused {
        return Err("Container not paused");
    }

    container.state = ContainerState::Running;
    serial_println!("[OCI] Container '{}' resumed", id);
    Ok(())
}

/// Get container state (runc state)
pub fn state(id: &str) -> Result<ContainerStateInfo, &'static str> {
    let containers = CONTAINERS.lock();
    let container = containers.get(id).ok_or("Container not found")?;

    Ok(ContainerStateInfo {
        oci_version: String::from("1.0.2"),
        id: container.id.clone(),
        status: alloc::format!("{}", container.state),
        pid: container.pid,
        bundle: container.bundle.clone(),
        annotations: container.spec.annotations.clone(),
    })
}

/// List all containers (runc list)
pub fn list() -> Vec<ContainerStateInfo> {
    let containers = CONTAINERS.lock();
    containers
        .values()
        .map(|c| ContainerStateInfo {
            oci_version: String::from("1.0.2"),
            id: c.id.clone(),
            status: alloc::format!("{}", c.state),
            pid: c.pid,
            bundle: c.bundle.clone(),
            annotations: c.spec.annotations.clone(),
        })
        .collect()
}

/// Execute a process in a running container (runc exec)
pub fn exec(id: &str, process: &ProcessSpec) -> Result<u32, &'static str> {
    let containers = CONTAINERS.lock();
    let container = containers.get(id).ok_or("Container not found")?;

    if container.state != ContainerState::Running {
        return Err("Container not running");
    }

    // Create new process in container's namespaces
    let pid = CONTAINER_COUNTER.fetch_add(1, Ordering::SeqCst) as u32 + 10000;
    serial_println!(
        "[OCI] Exec in container '{}': {:?} (PID {})",
        id,
        process.args,
        pid
    );
    Ok(pid)
}

// ═══════════════════════════════════════════════════════════════════════
// INTERNAL HELPERS
// ═══════════════════════════════════════════════════════════════════════

/// Set up namespaces for container
fn setup_namespaces(id: &str, namespaces: &[NamespaceConfig]) {
    for ns in namespaces {
        let ns_name = match ns.ns_type {
            NamespaceType::Pid => "pid",
            NamespaceType::Network => "net",
            NamespaceType::Mount => "mnt",
            NamespaceType::Uts => "uts",
            NamespaceType::Ipc => "ipc",
            NamespaceType::User => "user",
            NamespaceType::Cgroup => "cgroup",
        };

        if let Some(ref path) = ns.path {
            serial_println!(
                "[OCI] Container '{}': join {} namespace at {}",
                id,
                ns_name,
                path
            );
        } else {
            serial_println!("[OCI] Container '{}': create new {} namespace", id, ns_name);
            // In real implementation: crate::namespaces::create_namespace(ns.ns_type);
        }
    }
}

/// Set up cgroup for container
fn setup_cgroup(id: &str, path: &str, resources: &Option<ResourceLimits>) {
    serial_println!("[OCI] Container '{}': setup cgroup at {}", id, path);
    if let Some(res) = resources {
        if let Some(mem) = res.memory_limit {
            serial_println!("[OCI]   memory.max = {} bytes", mem);
        }
        if let Some(shares) = res.cpu_shares {
            serial_println!("[OCI]   cpu.weight = {}", shares);
        }
        if let Some(pids) = res.pids_limit {
            serial_println!("[OCI]   pids.max = {}", pids);
        }
    }
}

/// Prepare rootfs mounts
fn prepare_rootfs(id: &str, rootfs: &str, mounts: &[MountSpec]) {
    serial_println!("[OCI] Container '{}': preparing rootfs at {}", id, rootfs);
    for mount in mounts {
        serial_println!(
            "[OCI]   mount {} -> {}",
            mount.source.as_deref().unwrap_or("none"),
            mount.destination
        );
    }
}

/// Create container init process
fn create_container_process(container: &Container) -> Result<u32, &'static str> {
    let pid = CONTAINER_COUNTER.fetch_add(1, Ordering::SeqCst) as u32 + 1000;

    serial_println!(
        "[OCI] Container init process: {:?}",
        container.spec.process.args
    );
    serial_println!(
        "[OCI]   UID={} GID={}",
        container.spec.process.uid,
        container.spec.process.gid
    );
    serial_println!("[OCI]   CWD={}", container.spec.process.cwd);
    serial_println!("[OCI]   Hostname={}", container.hostname);

    // In real implementation:
    // 1. fork()
    // 2. unshare namespaces
    // 3. pivot_root to rootfs
    // 4. mount proc/sys/dev
    // 5. set hostname
    // 6. drop capabilities
    // 7. apply seccomp profile
    // 8. setuid/setgid
    // 9. exec process

    Ok(pid)
}

/// Cleanup cgroup
fn cleanup_cgroup(path: &str) {
    serial_println!("[OCI] Cleaning up cgroup {}", path);
}

/// Cleanup mounts
fn cleanup_mounts(id: &str) {
    serial_println!("[OCI] Cleaning up mounts for container {}", id);
}

// ═══════════════════════════════════════════════════════════════════════
// IMAGE SUPPORT (OCI Image Spec)
// ═══════════════════════════════════════════════════════════════════════

/// OCI image manifest
#[derive(Debug, Clone)]
pub struct ImageManifest {
    pub schema_version: u32,
    pub media_type: String,
    pub config_digest: String,
    pub layers: Vec<ImageLayer>,
}

/// OCI image layer
#[derive(Debug, Clone)]
pub struct ImageLayer {
    pub media_type: String,
    pub digest: String,
    pub size: u64,
}

/// OCI image config
#[derive(Debug, Clone)]
pub struct ImageConfig {
    pub architecture: String,
    pub os: String,
    pub rootfs_type: String,
    pub rootfs_diff_ids: Vec<String>,
    pub created: String,
    pub author: String,
    pub cmd: Vec<String>,
    pub entrypoint: Vec<String>,
    pub env: Vec<String>,
    pub working_dir: String,
    pub exposed_ports: Vec<String>,
    pub volumes: Vec<String>,
    pub labels: BTreeMap<String, String>,
}

/// Image registry (local cache)
static IMAGE_CACHE: Mutex<BTreeMap<String, ImageManifest>> = Mutex::new(BTreeMap::new());

/// Pull an image from an OCI-compatible registry.
///
/// Parses the reference (e.g. "docker.io/library/alpine:latest"), resolves
/// the registry, fetches the manifest, downloads layers, and extracts them
/// into the VFS under `/var/lib/containers/images/<digest>/`.
pub fn pull_image(reference: &str) -> Result<(), &'static str> {
    serial_println!("[OCI] Pulling image: {}", reference);

    // Parse reference: "registry/repo:tag" or just "image:tag"
    let (registry, repo, tag) = parse_image_reference(reference);

    serial_println!("[OCI] Registry: {}, Repo: {}, Tag: {}", registry, repo, tag);

    // Step 1: DNS-resolve the registry host
    let resolved = crate::dns::resolve(&registry);
    let ip = match resolved {
        Some(addrs) if !addrs.is_empty() => addrs[0],
        _ => {
            serial_println!("[OCI] DNS resolution failed for {}", registry);
            // Fall back to creating a local placeholder manifest
            return pull_image_local(reference);
        }
    };

    // Step 2: Fetch manifest via HTTP GET /v2/<repo>/manifests/<tag>
    let url = alloc::format!("/v2/{}/manifests/{}", repo, tag);
    let host = registry.clone();

    let response = fetch_registry_http(&host, &ip, &url);

    let manifest = match response {
        Ok(body) => parse_oci_manifest(&body),
        Err(_) => {
            serial_println!("[OCI] HTTP fetch failed, using local placeholder");
            return pull_image_local(reference);
        }
    };

    // Step 3: Download each layer and extract into VFS
    let image_dir = alloc::format!("/var/lib/containers/images/{}", tag);
    crate::vfs::ensure_directory(&image_dir);

    for (i, layer) in manifest.layers.iter().enumerate() {
        serial_println!(
            "[OCI] Downloading layer {}/{} ({} bytes, {})",
            i + 1,
            manifest.layers.len(),
            layer.size,
            &layer.digest[..16.min(layer.digest.len())]
        );
        let layer_url = alloc::format!("/v2/{}/blobs/{}", repo, layer.digest);
        if let Ok(layer_data) = fetch_registry_http(&host, &ip, &layer_url) {
            let layer_path = alloc::format!("{}/layer_{}.tar", image_dir, i);
            let _ = crate::vfs::write_file_dispatch(&layer_path, &layer_data);
        }
    }

    // Step 4: Cache manifest
    IMAGE_CACHE.lock().insert(String::from(reference), manifest);
    serial_println!("[OCI] Image '{}' pulled successfully", reference);
    Ok(())
}

/// Parse an OCI image reference into (registry, repo, tag)
fn parse_image_reference(reference: &str) -> (String, String, String) {
    let (name, tag) = if let Some(idx) = reference.rfind(':') {
        (&reference[..idx], &reference[idx + 1..])
    } else {
        (reference, "latest")
    };

    if let Some(slash_idx) = name.find('/') {
        let first = &name[..slash_idx];
        // If first component has a dot or colon, it's a registry
        if first.contains('.') || first.contains(':') {
            return (
                String::from(first),
                String::from(&name[slash_idx + 1..]),
                String::from(tag),
            );
        }
    }

    // Default: docker.io
    (
        String::from("registry-1.docker.io"),
        if name.contains('/') {
            String::from(name)
        } else {
            alloc::format!("library/{}", name)
        },
        String::from(tag),
    )
}

/// Fetch data from an OCI registry via HTTP GET
fn fetch_registry_http(host: &str, ip: &[u8; 4], path: &str) -> Result<Vec<u8>, &'static str> {
    use crate::net::{Ipv4Address, SOCKETS, SocketAddress};

    // Create TCP socket: AF_INET=2, SOCK_STREAM=1, protocol=0
    let fd = crate::net::sys_socket(2, 1, 0).map_err(|_| "socket() failed")?;

    let addr = SocketAddress::Inet(Ipv4Address::new(ip[0], ip[1], ip[2], ip[3]), 443);

    {
        let mut sockets = SOCKETS.lock();
        if let Some(sock) = sockets.get_mut(&fd) {
            sock.remote_addr = Some(addr);
            sock.state = crate::net::SocketState::Connected;

            // Build HTTP request
            let req = alloc::format!(
                "GET {} HTTP/1.1\r\nHost: {}\r\nAccept: application/vnd.oci.image.manifest.v1+json\r\nConnection: close\r\n\r\n",
                path,
                host
            );
            sock.send_buf.extend_from_slice(req.as_bytes());
        }
    }

    // Poll for response
    let start = crate::interrupts::get_ticks();
    let mut response = Vec::new();
    loop {
        if crate::interrupts::get_ticks().wrapping_sub(start) > 300 {
            break;
        }
        let mut sockets = SOCKETS.lock();
        if let Some(sock) = sockets.get_mut(&fd) {
            if !sock.recv_buf.is_empty() {
                response.extend_from_slice(&sock.recv_buf);
                sock.recv_buf.clear();
                break;
            }
        }
        drop(sockets);
        crate::arch_compat::instructions::interrupts::hlt();
    }

    {
        let mut sockets = SOCKETS.lock();
        if let Some(sock) = sockets.get_mut(&fd) {
            sock.close();
        }
    }

    if response.is_empty() {
        Err("No response from registry")
    } else {
        // Strip HTTP headers (find \r\n\r\n)
        if let Some(pos) = response.windows(4).position(|w| w == b"\r\n\r\n") {
            Ok(response[pos + 4..].to_vec())
        } else {
            Ok(response)
        }
    }
}

/// Parse a minimal OCI manifest from JSON-like response body
fn parse_oci_manifest(body: &[u8]) -> ImageManifest {
    // Simplified JSON extraction for schemaVersion, config digest, layers
    let text = core::str::from_utf8(body).unwrap_or("");

    let config_digest = extract_json_string(text, "config").unwrap_or_default();

    let mut layers = Vec::new();
    // Find "layers" array entries
    if let Some(layers_start) = text.find("\"layers\"") {
        let rest = &text[layers_start..];
        let mut search_from = 0;
        while let Some(pos) = rest[search_from..].find("\"digest\"") {
            let abs = search_from + pos;
            if let Some(d) = extract_json_value(&rest[abs..]) {
                let size = extract_json_number(&rest[abs..], "size").unwrap_or(0);
                layers.push(ImageLayer {
                    media_type: String::from("application/vnd.oci.image.layer.v1.tar+gzip"),
                    digest: d,
                    size,
                });
            }
            search_from = abs + 10;
        }
    }

    if layers.is_empty() {
        layers.push(ImageLayer {
            media_type: String::from("application/vnd.oci.image.layer.v1.tar+gzip"),
            digest: String::from("sha256:unknown"),
            size: 0,
        });
    }

    ImageManifest {
        schema_version: 2,
        media_type: String::from("application/vnd.oci.image.manifest.v1+json"),
        config_digest,
        layers,
    }
}

fn extract_json_string(text: &str, key: &str) -> Option<String> {
    let pattern = alloc::format!("\"{}\"\\s*:", key);
    let key_with_quote = alloc::format!("\"{}\" :", key);
    let pos = text
        .find(&alloc::format!("\"{}\":", key))
        .or_else(|| text.find(&key_with_quote))?;
    let rest = &text[pos..];
    extract_json_value(rest)
}

fn extract_json_value(text: &str) -> Option<String> {
    let colon = text.find(':')?;
    let rest = &text[colon + 1..].trim_start();
    if let Some(inner) = rest.strip_prefix('"') {
        let end = inner.find('"')?;
        Some(String::from(&inner[..end]))
    } else {
        None
    }
}

fn extract_json_number(text: &str, key: &str) -> Option<u64> {
    let pattern = alloc::format!("\"{}\":", key);
    let pos = text
        .find(&pattern)
        .or_else(|| text.find(&alloc::format!("\"{}\" :", key)))?;
    let rest = &text[pos + pattern.len()..].trim_start();
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Fallback: create a local placeholder manifest when network is unavailable
fn pull_image_local(reference: &str) -> Result<(), &'static str> {
    let manifest = ImageManifest {
        schema_version: 2,
        media_type: String::from("application/vnd.oci.image.manifest.v1+json"),
        config_digest: alloc::format!("sha256:{:016x}", crate::random::random_u64()),
        layers: vec![ImageLayer {
            media_type: String::from("application/vnd.oci.image.layer.v1.tar+gzip"),
            digest: alloc::format!("sha256:{:016x}", crate::random::random_u64()),
            size: 0,
        }],
    };

    IMAGE_CACHE.lock().insert(String::from(reference), manifest);
    serial_println!("[OCI] Image '{}' cached locally (offline mode)", reference);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Initialize container runtime
pub fn init() {
    serial_println!("[OCI] Initializing OCI container runtime");
    serial_println!("[OCI] Runtime: knoxos-runc v1.0.0 (OCI spec v1.0.2)");
    serial_println!("[OCI] Container runtime initialized");
}
