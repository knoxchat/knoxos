/// Kernel Object Model — sysfs backing infrastructure
///
/// Implements a simplified Linux kobject/kset/ktype hierarchy:
///   - Kobjects represent kernel entities visible in /sys
///   - Ksets group related kobjects
///   - Attributes expose read/write properties
///   - Uevent notification for hotplug
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

/// Unique kobject ID
pub type KobjId = u64;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Attribute read/write function type
pub type AttrReadFn = fn(kobj: &Kobject) -> String;
pub type AttrWriteFn = fn(kobj: &mut Kobject, data: &str) -> Result<(), i32>;

/// A kernel object attribute
#[derive(Clone)]
pub struct Attribute {
    pub name: String,
    pub mode: u16, // File permissions (0o444 = read-only, 0o644 = read-write)
    pub read: Option<AttrReadFn>,
    pub write: Option<AttrWriteFn>,
}

/// Uevent action type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UeventAction {
    Add,
    Remove,
    Change,
    Move,
    Online,
    Offline,
    Bind,
    Unbind,
}

impl UeventAction {
    pub fn as_str(&self) -> &str {
        match self {
            UeventAction::Add => "add",
            UeventAction::Remove => "remove",
            UeventAction::Change => "change",
            UeventAction::Move => "move",
            UeventAction::Online => "online",
            UeventAction::Offline => "offline",
            UeventAction::Bind => "bind",
            UeventAction::Unbind => "unbind",
        }
    }
}

/// A kernel object — represents an entity in the kernel object hierarchy
#[derive(Clone)]
pub struct Kobject {
    /// Unique ID
    pub id: KobjId,
    /// Name (appears in /sys path)
    pub name: String,
    /// Parent kobject ID (0 = root)
    pub parent: KobjId,
    /// Kset this belongs to
    pub kset: Option<String>,
    /// Subsystem (e.g., "block", "net", "pci")
    pub subsystem: String,
    /// Device path in sysfs
    pub path: String,
    /// Reference count
    pub refcount: u32,
    /// Attributes
    pub attributes: Vec<Attribute>,
    /// Key-value properties (for uevent environment)
    pub properties: BTreeMap<String, String>,
}

impl Kobject {
    pub fn new(name: &str, subsystem: &str, parent: KobjId) -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        Kobject {
            id,
            name: String::from(name),
            parent,
            kset: None,
            subsystem: String::from(subsystem),
            path: String::new(), // Set when registered
            refcount: 1,
            attributes: Vec::new(),
            properties: BTreeMap::new(),
        }
    }

    /// Add an attribute
    pub fn add_attribute(&mut self, attr: Attribute) {
        self.attributes.push(attr);
    }

    /// Set a property
    pub fn set_property(&mut self, key: &str, value: &str) {
        self.properties
            .insert(String::from(key), String::from(value));
    }

    /// Get a property
    pub fn get_property(&self, key: &str) -> Option<&String> {
        self.properties.get(key)
    }

    /// Get reference
    pub fn get(&mut self) {
        self.refcount += 1;
    }

    /// Put reference (returns true if refcount reached 0)
    pub fn put(&mut self) -> bool {
        if self.refcount > 0 {
            self.refcount -= 1;
        }
        self.refcount == 0
    }
}

/// Global kobject registry
struct KobjectRegistry {
    objects: BTreeMap<KobjId, Kobject>,
    /// Name→ID index for fast lookup
    name_index: BTreeMap<String, KobjId>,
    /// Pending uevents
    uevent_queue: Vec<(KobjId, UeventAction, BTreeMap<String, String>)>,
}

lazy_static::lazy_static! {
    static ref REGISTRY: Mutex<KobjectRegistry> = Mutex::new(KobjectRegistry {
        objects: BTreeMap::new(),
        name_index: BTreeMap::new(),
        uevent_queue: Vec::new(),
    });
}

/// Register a kobject
pub fn register(mut kobj: Kobject) -> KobjId {
    let id = kobj.id;
    let mut reg = REGISTRY.lock();

    // Compute path
    let parent_path = if kobj.parent != 0 {
        reg.objects
            .get(&kobj.parent)
            .map(|p| p.path.clone())
            .unwrap_or_default()
    } else {
        String::new()
    };
    kobj.path = if parent_path.is_empty() {
        alloc::format!("/sys/{}", kobj.name)
    } else {
        alloc::format!("{}/{}", parent_path, kobj.name)
    };

    let path = kobj.path.clone();
    reg.name_index.insert(path, id);
    reg.objects.insert(id, kobj);

    // Queue ADD uevent
    reg.uevent_queue
        .push((id, UeventAction::Add, BTreeMap::new()));

    id
}

/// Unregister a kobject
pub fn unregister(id: KobjId) {
    let mut reg = REGISTRY.lock();

    // Queue REMOVE uevent before removing
    reg.uevent_queue
        .push((id, UeventAction::Remove, BTreeMap::new()));

    if let Some(kobj) = reg.objects.remove(&id) {
        reg.name_index.remove(&kobj.path);
    }
}

/// Find a kobject by path
pub fn find_by_path(path: &str) -> Option<Kobject> {
    let reg = REGISTRY.lock();
    reg.name_index
        .get(path)
        .and_then(|id| reg.objects.get(id))
        .cloned()
}

/// Find a kobject by ID
pub fn find_by_id(id: KobjId) -> Option<Kobject> {
    let reg = REGISTRY.lock();
    reg.objects.get(&id).cloned()
}

/// List children of a kobject
pub fn list_children(parent_id: KobjId) -> Vec<Kobject> {
    let reg = REGISTRY.lock();
    reg.objects
        .values()
        .filter(|k| k.parent == parent_id)
        .cloned()
        .collect()
}

/// Read an attribute value
pub fn read_attribute(kobj_id: KobjId, attr_name: &str) -> Result<String, i32> {
    let reg = REGISTRY.lock();
    let kobj = reg.objects.get(&kobj_id).ok_or(-2i32)?; // ENOENT
    let attr = kobj
        .attributes
        .iter()
        .find(|a| a.name == attr_name)
        .ok_or(-2i32)?;
    let read_fn = attr.read.ok_or(-1i32)?; // EPERM
    Ok(read_fn(kobj))
}

/// Write an attribute value
pub fn write_attribute(kobj_id: KobjId, attr_name: &str, data: &str) -> Result<(), i32> {
    let mut reg = REGISTRY.lock();
    let kobj = reg.objects.get_mut(&kobj_id).ok_or(-2i32)?;
    let attr = kobj
        .attributes
        .iter()
        .find(|a| a.name == attr_name)
        .ok_or(-2i32)?;
    let write_fn = attr.write.ok_or(-1i32)?; // EPERM / EACCES
    write_fn(kobj, data)
}

/// Send a uevent for a kobject
pub fn send_uevent(id: KobjId, action: UeventAction, env: BTreeMap<String, String>) {
    let mut reg = REGISTRY.lock();
    reg.uevent_queue.push((id, action, env));
}

/// Process pending uevents
pub fn process_uevents() {
    let events: Vec<_> = {
        let mut reg = REGISTRY.lock();
        core::mem::take(&mut reg.uevent_queue)
    };

    for (id, action, env) in events {
        let reg = REGISTRY.lock();
        if let Some(kobj) = reg.objects.get(&id) {
            serial_println!(
                "[uevent] {}@{} SUBSYSTEM={}",
                action.as_str(),
                kobj.path,
                kobj.subsystem
            );
            for (k, v) in &env {
                serial_println!("[uevent]   {}={}", k, v);
            }
        }
    }
}

/// Get all registered kobject paths (for debugging)
pub fn list_all_paths() -> Vec<String> {
    let reg = REGISTRY.lock();
    reg.name_index.keys().cloned().collect()
}

/// Total number of registered kobjects
pub fn count() -> usize {
    let reg = REGISTRY.lock();
    reg.objects.len()
}

/// Initialize kobject subsystem with root objects
pub fn init() {
    // Create root kobjects for major subsystems
    let kernel = Kobject::new("kernel", "kernel", 0);
    let kernel_id = register(kernel);

    let devices = Kobject::new("devices", "devices", 0);
    let devices_id = register(devices);

    let bus = Kobject::new("bus", "bus", 0);
    let bus_id = register(bus);

    let class_kobj = Kobject::new("class", "class", 0);
    let class_id = register(class_kobj);

    let module = Kobject::new("module", "module", 0);
    let _module_id = register(module);

    let firmware = Kobject::new("firmware", "firmware", 0);
    let _firmware_id = register(firmware);

    let fs = Kobject::new("fs", "fs", 0);
    let _fs_id = register(fs);

    let power = Kobject::new("power", "power", 0);
    let _power_id = register(power);

    // Create bus subtypes
    let pci_bus = Kobject::new("pci", "bus", bus_id);
    register(pci_bus);

    let platform_bus = Kobject::new("platform", "bus", bus_id);
    register(platform_bus);

    // Create device classes
    let net_class = Kobject::new("net", "class", class_id);
    register(net_class);

    let block_class = Kobject::new("block", "class", class_id);
    register(block_class);

    let tty_class = Kobject::new("tty", "class", class_id);
    register(tty_class);

    let input_class = Kobject::new("input", "class", class_id);
    register(input_class);

    serial_println!(
        "[KnoxOS] Kobject subsystem initialized ({} objects)",
        count()
    );
}
