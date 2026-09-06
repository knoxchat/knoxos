use crate::serial_println;
/// Kernel Live Patching
///
/// Apply security patches to running kernel without reboot.
/// Function-level redirection via ftrace-style trampolines.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Patch state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PatchState {
    Disabled,
    Enabling,
    Enabled,
    Disabling,
}

/// A single function patch
#[derive(Debug)]
pub struct FuncPatch {
    pub name: String,
    pub old_addr: u64,
    pub new_addr: u64,
    pub original_bytes: [u8; 16], // preserve original instruction prefix
    pub trampoline_addr: u64,
}

/// A livepatch module containing multiple function patches
pub struct LivePatch {
    pub id: String,
    pub version: String,
    pub description: String,
    pub patches: Vec<FuncPatch>,
    pub state: PatchState,
    pub applied_time: u64,
}

/// Livepatch manager
pub struct LivePatchManager {
    pub patches: Vec<LivePatch>,
    pub trampoline_pool_base: u64,
    pub trampoline_pool_size: usize,
    pub next_trampoline: u64,
}

lazy_static::lazy_static! {
    static ref LIVEPATCH: Mutex<LivePatchManager> = Mutex::new(LivePatchManager {
        patches: Vec::new(),
        trampoline_pool_base: 0xFFFF_FF00_0000_0000,
        trampoline_pool_size: 4096 * 16,
        next_trampoline: 0xFFFF_FF00_0000_0000,
    });
}

impl LivePatchManager {
    /// Allocate a trampoline slot
    fn alloc_trampoline(&mut self) -> Option<u64> {
        let addr = self.next_trampoline;
        if addr >= self.trampoline_pool_base + self.trampoline_pool_size as u64 {
            return None;
        }
        self.next_trampoline += 32; // Each trampoline is 32 bytes
        Some(addr)
    }

    /// Register and apply a livepatch
    pub fn apply(&mut self, mut patch: LivePatch) -> bool {
        serial_println!("[LIVEPATCH] Applying: {} v{}", patch.id, patch.version);

        for func in &mut patch.patches {
            // Allocate trampoline
            let tramp = match self.alloc_trampoline() {
                Some(a) => a,
                None => {
                    serial_println!("[LIVEPATCH] Trampoline pool exhausted");
                    return false;
                }
            };
            func.trampoline_addr = tramp;

            // Write trampoline: MOV RAX, new_addr; JMP RAX
            // Would be written to trampoline memory page
            serial_println!(
                "[LIVEPATCH]   Patching {} @ {:#x} → {:#x} (tramp {:#x})",
                func.name,
                func.old_addr,
                func.new_addr,
                tramp
            );
        }

        patch.state = PatchState::Enabled;
        self.patches.push(patch);
        true
    }

    /// Revert a livepatch
    pub fn revert(&mut self, id: &str) -> bool {
        if let Some(patch) = self.patches.iter_mut().find(|p| p.id == id) {
            if patch.state != PatchState::Enabled {
                return false;
            }

            for func in &patch.patches {
                // Restore original bytes at old_addr
                serial_println!(
                    "[LIVEPATCH]   Reverting {} @ {:#x}",
                    func.name,
                    func.old_addr
                );
            }

            patch.state = PatchState::Disabled;
            serial_println!("[LIVEPATCH] Reverted: {}", id);
            true
        } else {
            false
        }
    }

    /// List applied patches
    pub fn list_active(&self) -> Vec<(&str, &str)> {
        self.patches
            .iter()
            .filter(|p| p.state == PatchState::Enabled)
            .map(|p| (p.id.as_str(), p.version.as_str()))
            .collect()
    }
}

pub fn init() {
    serial_println!("[LIVEPATCH] Kernel live patching initialized");
}
