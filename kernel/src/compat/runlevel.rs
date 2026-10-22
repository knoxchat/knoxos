use crate::serial_println;
/// Runlevel / Target Management
///
/// Systemd-style targets: multi-user, graphical, rescue, emergency.
/// Manages transitions between run states.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    Rescue,    // Single-user, minimal
    MultiUser, // Full services, no GUI
    Graphical, // Full services + desktop
    Emergency, // Root shell only
    Reboot,
    Poweroff,
}

#[derive(Debug, Clone)]
pub struct TargetDep {
    pub target: Target,
    pub requires: Vec<String>, // service names
    pub wants: Vec<String>,    // optional services
}

pub struct TargetManager {
    pub current: Target,
    pub default: Target,
    pub definitions: Vec<TargetDep>,
}

lazy_static::lazy_static! {
    static ref TARGETS: Mutex<TargetManager> = Mutex::new(TargetManager {
        current: Target::Graphical,
        default: Target::Graphical,
        definitions: Vec::new(),
    });
}

impl TargetManager {
    pub fn define_target(&mut self, def: TargetDep) {
        serial_println!(
            "[RUNLEVEL] Defined target {:?} ({} requires, {} wants)",
            def.target,
            def.requires.len(),
            def.wants.len()
        );
        self.definitions.push(def);
    }

    pub fn switch_to(&mut self, target: Target) {
        serial_println!("[RUNLEVEL] Switching {:?} → {:?}", self.current, target);
        // Stop services not needed by new target, start services that are
        self.current = target;
    }

    pub fn set_default(&mut self, target: Target) {
        self.default = target;
        serial_println!("[RUNLEVEL] Default target: {:?}", target);
    }

    pub fn current(&self) -> Target {
        self.current
    }
}

pub fn init() {
    let mut mgr = TARGETS.lock();
    mgr.define_target(TargetDep {
        target: Target::Rescue,
        requires: alloc::vec![String::from("syslog")],
        wants: Vec::new(),
    });
    mgr.define_target(TargetDep {
        target: Target::MultiUser,
        requires: alloc::vec![String::from("syslog"), String::from("network")],
        wants: alloc::vec![String::from("cron"), String::from("sshd")],
    });
    mgr.define_target(TargetDep {
        target: Target::Graphical,
        requires: alloc::vec![
            String::from("syslog"),
            String::from("network"),
            String::from("display-manager")
        ],
        wants: alloc::vec![String::from("cron"), String::from("sshd")],
    });
    serial_println!("[RUNLEVEL] Target manager initialized (default: Graphical)");
}
