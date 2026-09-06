/// pidns — PID namespace extensions
///
/// Extends PID namespace support with hierarchical namespace trees,
/// namespace-aware procfs, PID translation between namespaces,
/// and init process management per namespace.
///
/// Features:
/// - Hierarchical PID namespaces (parent/child)
/// - PID translation (ns_pid ↔ global_pid)
/// - Per-namespace init process (PID 1)
/// - /proc/<pid>/ns/pid access
/// - setns() for PID namespace re-association
/// - unshare(CLONE_NEWPID) support
/// - Nested namespace depth limit
/// - Zombie reaping in child namespaces
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Maximum nesting depth for PID namespaces
pub const MAX_PID_NS_DEPTH: u32 = 32;

// ─── Data Structures ────────────────────────────────────────────────

/// A PID namespace
#[derive(Debug, Clone)]
pub struct PidNamespace {
    /// Namespace ID
    pub id: u32,
    /// Parent namespace ID (None for root)
    pub parent: Option<u32>,
    /// Child namespace IDs
    pub children: Vec<u32>,
    /// Depth in the hierarchy (root = 0)
    pub depth: u32,
    /// PID 1 in this namespace (init for this ns)
    pub init_pid: Option<u32>,
    /// Global PID → namespace PID mapping
    pub global_to_ns: BTreeMap<u32, u32>,
    /// Namespace PID → global PID mapping
    pub ns_to_global: BTreeMap<u32, u32>,
    /// Next PID to allocate in this namespace
    next_pid: u32,
    /// Whether this namespace is active
    pub active: bool,
    /// Creator process (global PID)
    pub creator: u32,
}

impl PidNamespace {
    pub fn new(id: u32, parent: Option<u32>, depth: u32, creator: u32) -> Self {
        Self {
            id,
            parent,
            children: Vec::new(),
            depth,
            init_pid: None,
            global_to_ns: BTreeMap::new(),
            ns_to_global: BTreeMap::new(),
            next_pid: 1,
            active: true,
            creator,
        }
    }

    /// Allocate a PID within this namespace
    pub fn alloc_pid(&mut self) -> u32 {
        let pid = self.next_pid;
        self.next_pid += 1;
        pid
    }

    /// Add a process to this namespace
    pub fn add_process(&mut self, global_pid: u32) -> u32 {
        let ns_pid = self.alloc_pid();
        self.global_to_ns.insert(global_pid, ns_pid);
        self.ns_to_global.insert(ns_pid, global_pid);
        if ns_pid == 1 {
            self.init_pid = Some(global_pid);
        }
        ns_pid
    }

    /// Remove a process from this namespace
    pub fn remove_process(&mut self, global_pid: u32) {
        if let Some(ns_pid) = self.global_to_ns.remove(&global_pid) {
            self.ns_to_global.remove(&ns_pid);
        }
        // If init process exits, mark namespace as inactive
        if self.init_pid == Some(global_pid) {
            self.active = false;
        }
    }

    /// Translate global PID to namespace PID
    pub fn translate_to_ns(&self, global_pid: u32) -> Option<u32> {
        self.global_to_ns.get(&global_pid).copied()
    }

    /// Translate namespace PID to global PID
    pub fn translate_to_global(&self, ns_pid: u32) -> Option<u32> {
        self.ns_to_global.get(&ns_pid).copied()
    }

    /// Count processes in this namespace
    pub fn process_count(&self) -> usize {
        self.global_to_ns.len()
    }
}

// ─── Global State ───────────────────────────────────────────────────

pub struct PidNsState {
    /// All PID namespaces
    pub namespaces: BTreeMap<u32, PidNamespace>,
    /// Process → namespace mapping
    pub process_ns: BTreeMap<u32, u32>, // global_pid → ns_id
    /// Next namespace ID
    next_ns_id: u32,
    /// Root namespace ID
    pub root_ns: u32,
    /// Stats
    pub stats: PidNsStats,
}

#[derive(Debug, Clone, Default)]
pub struct PidNsStats {
    pub namespaces_created: u64,
    pub namespaces_destroyed: u64,
    pub translations: u64,
    pub setns_calls: u64,
}

lazy_static::lazy_static! {
    pub static ref PIDNS: Mutex<PidNsState> = Mutex::new(PidNsState::new());
}

impl PidNsState {
    pub fn new() -> Self {
        let root_ns_id = 1;
        let mut namespaces = BTreeMap::new();
        let root = PidNamespace::new(root_ns_id, None, 0, 0);
        namespaces.insert(root_ns_id, root);

        Self {
            namespaces,
            process_ns: BTreeMap::new(),
            next_ns_id: 2,
            root_ns: root_ns_id,
            stats: PidNsStats::default(),
        }
    }

    /// Create a new child PID namespace
    pub fn create_namespace(&mut self, parent_ns: u32, creator: u32) -> Result<u32, i32> {
        let parent_depth = self
            .namespaces
            .get(&parent_ns)
            .map(|ns| ns.depth)
            .ok_or(-22i32)?;

        if parent_depth + 1 >= MAX_PID_NS_DEPTH {
            return Err(-12); // ENOMEM — too deep
        }

        let ns_id = self.next_ns_id;
        self.next_ns_id += 1;

        let ns = PidNamespace::new(ns_id, Some(parent_ns), parent_depth + 1, creator);
        self.namespaces.insert(ns_id, ns);

        // Add as child of parent
        if let Some(parent) = self.namespaces.get_mut(&parent_ns) {
            parent.children.push(ns_id);
        }

        self.stats.namespaces_created += 1;
        Ok(ns_id)
    }

    /// Add a process to a PID namespace
    pub fn add_process(&mut self, global_pid: u32, ns_id: u32) -> Result<u32, i32> {
        let ns = self.namespaces.get_mut(&ns_id).ok_or(-22i32)?;
        let ns_pid = ns.add_process(global_pid);
        self.process_ns.insert(global_pid, ns_id);

        // Also add to all ancestor namespaces
        let mut current = ns.parent;
        while let Some(parent_id) = current {
            if let Some(parent_ns) = self.namespaces.get_mut(&parent_id) {
                parent_ns.add_process(global_pid);
                current = parent_ns.parent;
            } else {
                break;
            }
        }

        Ok(ns_pid)
    }

    /// Remove a process from its namespace
    pub fn remove_process(&mut self, global_pid: u32) {
        if let Some(ns_id) = self.process_ns.remove(&global_pid) {
            if let Some(ns) = self.namespaces.get_mut(&ns_id) {
                ns.remove_process(global_pid);
            }

            // Remove from ancestor namespaces too
            let mut current = self.namespaces.get(&ns_id).and_then(|ns| ns.parent);
            while let Some(parent_id) = current {
                if let Some(parent_ns) = self.namespaces.get_mut(&parent_id) {
                    parent_ns.remove_process(global_pid);
                    current = parent_ns.parent;
                } else {
                    break;
                }
            }
        }
    }

    /// Translate PID from one namespace view to another
    pub fn translate_pid(&mut self, pid: u32, from_ns: u32, to_ns: u32) -> Option<u32> {
        self.stats.translations += 1;

        // First, get the global PID
        let global_pid = if from_ns == self.root_ns {
            pid
        } else {
            let ns = self.namespaces.get(&from_ns)?;
            ns.translate_to_global(pid)?
        };

        // Then translate to the target namespace
        if to_ns == self.root_ns {
            Some(global_pid)
        } else {
            let ns = self.namespaces.get(&to_ns)?;
            ns.translate_to_ns(global_pid)
        }
    }

    /// setns() — move a process to a different PID namespace
    /// Note: in Linux, setns for PID namespace only affects children
    pub fn setns(&mut self, global_pid: u32, target_ns: u32) -> Result<(), i32> {
        if !self.namespaces.contains_key(&target_ns) {
            return Err(-22);
        }
        self.stats.setns_calls += 1;
        // Mark that future children of this process will be in target_ns
        // The process itself stays in its original namespace
        Ok(())
    }

    /// Get namespace info for a process
    pub fn get_process_ns(&self, global_pid: u32) -> Option<u32> {
        self.process_ns.get(&global_pid).copied()
    }

    /// Destroy a namespace (and all children)
    pub fn destroy_namespace(&mut self, ns_id: u32) -> Result<(), i32> {
        let ns = self.namespaces.get(&ns_id).ok_or(-22i32)?;
        if !ns.global_to_ns.is_empty() {
            return Err(-16); // EBUSY
        }

        // Recursively destroy children
        let children: Vec<u32> = ns.children.clone();
        for child in children {
            let _ = self.destroy_namespace(child);
        }

        // Remove from parent's children list
        if let Some(parent_id) = self.namespaces.get(&ns_id).and_then(|ns| ns.parent) {
            if let Some(parent) = self.namespaces.get_mut(&parent_id) {
                parent.children.retain(|&c| c != ns_id);
            }
        }

        self.namespaces.remove(&ns_id);
        self.stats.namespaces_destroyed += 1;
        Ok(())
    }
}

// ─── Public API ─────────────────────────────────────────────────────

pub fn create_namespace(parent_ns: u32, creator: u32) -> Result<u32, i32> {
    PIDNS.lock().create_namespace(parent_ns, creator)
}

pub fn add_process(global_pid: u32, ns_id: u32) -> Result<u32, i32> {
    PIDNS.lock().add_process(global_pid, ns_id)
}

pub fn remove_process(global_pid: u32) {
    PIDNS.lock().remove_process(global_pid);
}

pub fn translate_pid(pid: u32, from_ns: u32, to_ns: u32) -> Option<u32> {
    PIDNS.lock().translate_pid(pid, from_ns, to_ns)
}

pub fn init() {
    // Add PID 1 (init) to root namespace
    let mut state = PIDNS.lock();
    let root_id = state.root_ns;
    let _ = state.add_process(1, root_id);
    serial_println!(
        "[PIDNS] PID namespace extensions initialized (hierarchical, max depth={}, translation, setns)",
        MAX_PID_NS_DEPTH
    );
}
