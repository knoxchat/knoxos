use alloc::collections::BTreeMap;
/// Kernel Modules - Dynamic module loading framework
/// Compatible with Linux's module system (insmod/rmmod/lsmod)
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Module state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleState {
    Live,
    Loading,
    Unloading,
    Builtin,
}

/// Module dependency type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepType {
    Hard, // Required dependency
    Soft, // Optional dependency
}

/// Kernel module information
#[derive(Debug, Clone)]
pub struct KernelModule {
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub license: String,
    pub state: ModuleState,
    pub size_bytes: usize,
    pub dependencies: Vec<(String, DepType)>,
    pub ref_count: u32,
    pub parameters: BTreeMap<String, String>,
}

/// Global module registry
lazy_static::lazy_static! {
    static ref MODULES: Mutex<BTreeMap<String, KernelModule>> = Mutex::new(BTreeMap::new());
}

/// Register a built-in module
fn register_builtin(name: &str, description: &str, deps: &[(&str, DepType)], size: usize) {
    let mut modules = MODULES.lock();
    modules.insert(
        String::from(name),
        KernelModule {
            name: String::from(name),
            description: String::from(description),
            author: String::from("KnoxOS Contributors"),
            version: String::from("0.1.0"),
            license: String::from("MIT"),
            state: ModuleState::Builtin,
            size_bytes: size,
            dependencies: deps.iter().map(|(n, t)| (String::from(*n), *t)).collect(),
            ref_count: 1,
            parameters: BTreeMap::new(),
        },
    );
}

/// Insert a module (like insmod)
pub fn insert_module(module: KernelModule) -> Result<(), &'static str> {
    let mut modules = MODULES.lock();

    if modules.contains_key(&module.name) {
        return Err("Module already loaded");
    }

    // Check dependencies
    for (dep, dep_type) in &module.dependencies {
        match modules.get(dep) {
            Some(dep_mod) => {
                if dep_mod.state != ModuleState::Live
                    && dep_mod.state != ModuleState::Builtin
                    && *dep_type == DepType::Hard
                {
                    return Err("Required dependency not available");
                }
            }
            None => {
                if *dep_type == DepType::Hard {
                    return Err("Required dependency not loaded");
                }
            }
        }
    }

    let name = module.name.clone();
    modules.insert(name.clone(), module);

    crate::serial_println!("[KnoxOS] Module loaded: {}", name);
    Ok(())
}

/// Remove a module (like rmmod)
pub fn remove_module(name: &str) -> Result<(), &'static str> {
    let mut modules = MODULES.lock();

    let module = modules.get(name).ok_or("Module not found")?;

    if module.state == ModuleState::Builtin {
        return Err("Cannot unload builtin module");
    }

    if module.ref_count > 0 {
        return Err("Module is in use");
    }

    // Check if any other module depends on this one
    let dependents: Vec<String> = modules
        .values()
        .filter(|m| m.dependencies.iter().any(|(d, _)| d == name))
        .map(|m| m.name.clone())
        .collect();

    if !dependents.is_empty() {
        return Err("Module is depended upon by other modules");
    }

    modules.remove(name);
    crate::serial_println!("[KnoxOS] Module unloaded: {}", name);
    Ok(())
}

/// List all modules (like lsmod)
pub fn list_modules() -> Vec<KernelModule> {
    MODULES.lock().values().cloned().collect()
}

/// Get module info (like modinfo)
pub fn module_info(name: &str) -> Option<KernelModule> {
    MODULES.lock().get(name).cloned()
}

/// Set a module parameter
pub fn set_param(module_name: &str, param: &str, value: &str) -> Result<(), &'static str> {
    let mut modules = MODULES.lock();
    let module = modules.get_mut(module_name).ok_or("Module not found")?;
    module
        .parameters
        .insert(String::from(param), String::from(value));
    Ok(())
}

/// Get a module parameter
pub fn get_param(module_name: &str, param: &str) -> Option<String> {
    MODULES
        .lock()
        .get(module_name)
        .and_then(|m| m.parameters.get(param).cloned())
}

/// Format module listing (like /proc/modules)
pub fn format_modules() -> String {
    let modules = MODULES.lock();
    let mut output = String::new();

    for module in modules.values() {
        let state_str = match module.state {
            ModuleState::Live => "Live",
            ModuleState::Loading => "Loading",
            ModuleState::Unloading => "Unloading",
            ModuleState::Builtin => "Builtin",
        };

        let deps: Vec<&str> = module
            .dependencies
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        let deps_str = if deps.is_empty() {
            String::from("-")
        } else {
            deps.join(",")
        };

        output.push_str(&alloc::format!(
            "{:<20} {:>8}  {} - {} [{}]\n",
            module.name,
            module.size_bytes,
            module.ref_count,
            deps_str,
            state_str
        ));
    }

    output
}

/// Initialize the module system with built-in modules
pub fn init() {
    // Register all built-in kernel subsystems as modules
    register_builtin("knoxos_core", "KnoxOS Core Kernel", &[], 65536);
    register_builtin(
        "gdt",
        "Global Descriptor Table",
        &[("knoxos_core", DepType::Hard)],
        4096,
    );
    register_builtin(
        "idt",
        "Interrupt Descriptor Table",
        &[("knoxos_core", DepType::Hard)],
        8192,
    );
    register_builtin(
        "memory",
        "Memory Management",
        &[("knoxos_core", DepType::Hard)],
        16384,
    );
    register_builtin(
        "scheduler",
        "Process Scheduler",
        &[("knoxos_core", DepType::Hard)],
        8192,
    );
    register_builtin(
        "vfs",
        "Virtual File System",
        &[("knoxos_core", DepType::Hard)],
        12288,
    );
    register_builtin("ext2", "Ext2 Filesystem", &[("vfs", DepType::Hard)], 16384);
    register_builtin(
        "block",
        "Block Device Layer",
        &[("knoxos_core", DepType::Hard)],
        12288,
    );
    register_builtin(
        "net_core",
        "Network Stack",
        &[("knoxos_core", DepType::Hard)],
        32768,
    );
    register_builtin("tcp", "TCP Protocol", &[("net_core", DepType::Hard)], 16384);
    register_builtin("udp", "UDP Protocol", &[("net_core", DepType::Hard)], 8192);
    register_builtin("ip", "IPv4 Protocol", &[("net_core", DepType::Hard)], 8192);
    register_builtin(
        "pcspkr",
        "PC Speaker Driver",
        &[("knoxos_core", DepType::Hard)],
        2048,
    );
    register_builtin(
        "ps2_kbd",
        "PS/2 Keyboard Driver",
        &[("knoxos_core", DepType::Hard)],
        4096,
    );
    register_builtin(
        "ps2_mouse",
        "PS/2 Mouse Driver",
        &[("knoxos_core", DepType::Hard)],
        4096,
    );
    register_builtin(
        "fb",
        "Framebuffer Driver",
        &[("knoxos_core", DepType::Hard)],
        16384,
    );
    register_builtin(
        "tty",
        "TTY Subsystem",
        &[("knoxos_core", DepType::Hard)],
        8192,
    );
    register_builtin(
        "security",
        "Security Module",
        &[("knoxos_core", DepType::Hard)],
        8192,
    );
    register_builtin(
        "ai_engine",
        "AI Inference Engine",
        &[("knoxos_core", DepType::Hard)],
        32768,
    );

    let count = MODULES.lock().len();
    crate::serial_println!(
        "[KnoxOS] Module system initialized ({} builtin modules)",
        count
    );
}
