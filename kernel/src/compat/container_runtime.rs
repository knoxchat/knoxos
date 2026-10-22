use crate::serial_println;
/// OCI Container Runtime
///
/// Pull, store, and run OCI-compatible container images.
/// Namespace isolation, overlay filesystem, cgroup resource limits.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Container image reference
#[derive(Debug, Clone)]
pub struct ImageRef {
    pub registry: String,
    pub repository: String,
    pub tag: String,
    pub digest: Option<String>,
}

/// Container state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContainerState {
    Created,
    Running,
    Paused,
    Stopped,
    Removing,
}

/// Namespace configuration
#[derive(Debug, Clone)]
pub struct Namespaces {
    pub pid: bool,
    pub net: bool,
    pub mount: bool,
    pub uts: bool,
    pub ipc: bool,
    pub user: bool,
}

/// Container resource limits
#[derive(Debug, Clone)]
pub struct ContainerLimits {
    pub cpu_shares: u32,
    pub memory_bytes: u64,
    pub pids_max: u32,
    pub readonly_rootfs: bool,
}

/// Mount point
#[derive(Debug, Clone)]
pub struct Mount {
    pub source: String,
    pub destination: String,
    pub readonly: bool,
}

/// Container instance
#[derive(Debug)]
pub struct Container {
    pub id: String,
    pub image: ImageRef,
    pub state: ContainerState,
    pub pid: Option<u32>,
    pub namespaces: Namespaces,
    pub limits: ContainerLimits,
    pub mounts: Vec<Mount>,
    pub env: Vec<(String, String)>,
    pub entrypoint: String,
    pub args: Vec<String>,
}

/// Container runtime
pub struct ContainerRuntime {
    pub containers: Vec<Container>,
    pub images_dir: String,
    pub next_id: u32,
}

lazy_static::lazy_static! {
    static ref RUNTIME: Mutex<ContainerRuntime> = Mutex::new(ContainerRuntime {
        containers: Vec::new(),
        images_dir: String::new(),
        next_id: 1,
    });
}

impl ContainerRuntime {
    /// Create a new container from image
    pub fn create(&mut self, image: ImageRef, entrypoint: &str) -> String {
        let id = alloc::format!("knoxos-{:08x}", self.next_id);
        self.next_id += 1;

        let container = Container {
            id: id.clone(),
            image,
            state: ContainerState::Created,
            pid: None,
            namespaces: Namespaces {
                pid: true,
                net: true,
                mount: true,
                uts: true,
                ipc: true,
                user: true,
            },
            limits: ContainerLimits {
                cpu_shares: 1024,
                memory_bytes: 512 * 1024 * 1024,
                pids_max: 256,
                readonly_rootfs: false,
            },
            mounts: Vec::new(),
            env: Vec::new(),
            entrypoint: String::from(entrypoint),
            args: Vec::new(),
        };

        serial_println!("[CONTAINER] Created: {}", id);
        self.containers.push(container);
        id
    }

    /// Start a container
    pub fn start(&mut self, id: &str) -> bool {
        if let Some(c) = self.containers.iter_mut().find(|c| c.id == id) {
            if c.state != ContainerState::Created && c.state != ContainerState::Stopped {
                return false;
            }
            // Would: setup namespaces, mount overlay fs, apply cgroups, fork+exec
            c.state = ContainerState::Running;
            c.pid = Some(self.next_id * 1000);
            serial_println!("[CONTAINER] Started: {} (PID {:?})", id, c.pid);
            true
        } else {
            false
        }
    }

    /// Stop a container
    pub fn stop(&mut self, id: &str) -> bool {
        if let Some(c) = self.containers.iter_mut().find(|c| c.id == id) {
            if c.state != ContainerState::Running {
                return false;
            }
            c.state = ContainerState::Stopped;
            c.pid = None;
            serial_println!("[CONTAINER] Stopped: {}", id);
            true
        } else {
            false
        }
    }

    /// Remove a container
    pub fn remove(&mut self, id: &str) -> bool {
        if let Some(pos) = self.containers.iter().position(|c| c.id == id) {
            let c = &self.containers[pos];
            if c.state == ContainerState::Running {
                return false;
            }
            serial_println!("[CONTAINER] Removed: {}", id);
            self.containers.remove(pos);
            true
        } else {
            false
        }
    }

    /// List all containers
    pub fn list(&self) -> Vec<(&str, ContainerState)> {
        self.containers
            .iter()
            .map(|c| (c.id.as_str(), c.state))
            .collect()
    }
}

pub fn init() {
    serial_println!("[CONTAINER] OCI container runtime initialized");
}
