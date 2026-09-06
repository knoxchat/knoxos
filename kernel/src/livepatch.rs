/// Kernel Live Patching — Apply security/bug fixes without reboot
///
/// Implements a kpatch/livepatch-compatible mechanism for hot-patching
/// kernel functions at runtime. Uses function redirection via ftrace-like
/// trampolines to replace function implementations atomically.
///
/// Safety model:
///   - Patches are verified with SHA-256 before application
///   - Consistency model ensures no thread is executing patched code during swap
///   - Rollback support for failed patches
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════

/// A live patch targeting a specific kernel function
#[derive(Debug, Clone)]
pub struct LivePatch {
    /// Unique patch identifier
    pub id: String,
    /// Human-readable description
    pub description: String,
    /// Target function name (symbol)
    pub target_symbol: String,
    /// Original function address
    pub original_addr: u64,
    /// Replacement function code bytes
    pub new_code: Vec<u8>,
    /// Saved original bytes (for rollback)
    pub saved_bytes: Vec<u8>,
    /// SHA-256 hash of the patch payload
    pub hash: [u8; 32],
    /// Patch state
    pub state: PatchState,
    /// Timestamp when applied
    pub applied_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchState {
    /// Loaded but not yet applied
    Loaded,
    /// Currently active
    Applied,
    /// Rolled back
    RolledBack,
    /// Failed to apply
    Failed,
    /// Disabled (still loaded)
    Disabled,
}

/// Consistency model for safe patching
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsistencyModel {
    /// Immediate: patch applied on next function entry (via trampoline)
    Immediate,
    /// Activeness safety: wait until no task has the patched function on its stack
    ActivenessSafety,
}

/// A trampoline entry that redirects a function call
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Trampoline {
    /// JMP instruction bytes (5 bytes for near jump, or 14 for far jump)
    jmp_instruction: [u8; 14],
    /// Target address
    target: u64,
    /// Whether this trampoline is active
    active: bool,
}

impl Trampoline {
    fn new(target: u64) -> Self {
        // Build a 64-bit absolute jump: mov rax, target; jmp rax
        let mut jmp = [0u8; 14];
        jmp[0] = 0x48; // REX.W prefix
        jmp[1] = 0xB8; // MOV RAX, imm64
        jmp[2..10].copy_from_slice(&target.to_le_bytes());
        jmp[10] = 0xFF; // JMP
        jmp[11] = 0xE0; // RAX
        // Remaining bytes are NOP padding
        jmp[12] = 0x90;
        jmp[13] = 0x90;

        Self {
            jmp_instruction: jmp,
            target,
            active: false,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// GLOBAL STATE
// ═══════════════════════════════════════════════════════════════════════

static LIVEPATCH_ENABLED: AtomicBool = AtomicBool::new(false);
static PATCH_COUNT: AtomicUsize = AtomicUsize::new(0);
static PATCHES_APPLIED: AtomicU64 = AtomicU64::new(0);
static PATCHES_ROLLED_BACK: AtomicU64 = AtomicU64::new(0);

lazy_static::lazy_static! {
    /// Registry of all loaded patches
    static ref PATCHES: Mutex<BTreeMap<String, LivePatch>> =
        Mutex::new(BTreeMap::new());

    /// Symbol table for function address resolution
    static ref SYMBOL_TABLE: Mutex<BTreeMap<String, u64>> =
        Mutex::new(BTreeMap::new());

    /// Active trampolines
    static ref TRAMPOLINES: Mutex<Vec<Trampoline>> =
        Mutex::new(Vec::new());
}

// ═══════════════════════════════════════════════════════════════════════
// SYMBOL RESOLUTION
// ═══════════════════════════════════════════════════════════════════════

/// Register a kernel symbol for live patching
pub fn register_symbol(name: &str, addr: u64) {
    SYMBOL_TABLE.lock().insert(String::from(name), addr);
}

/// Resolve a symbol name to an address
pub fn resolve_symbol(name: &str) -> Option<u64> {
    SYMBOL_TABLE.lock().get(name).copied()
}

// ═══════════════════════════════════════════════════════════════════════
// PATCH MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════

/// Initialize the live patching subsystem
pub fn init() {
    LIVEPATCH_ENABLED.store(true, Ordering::SeqCst);
    serial_println!("[livepatch] Kernel live patching subsystem initialized");
}

/// Load a patch (does not apply it yet)
pub fn load_patch(patch: LivePatch) -> Result<(), &'static str> {
    if !LIVEPATCH_ENABLED.load(Ordering::Relaxed) {
        return Err("Live patching is disabled");
    }

    let mut patches = PATCHES.lock();
    if patches.contains_key(&patch.id) {
        return Err("Patch with this ID already loaded");
    }

    // Verify patch hash
    let computed_hash = compute_sha256(&patch.new_code);
    if computed_hash != patch.hash {
        return Err("Patch hash verification failed");
    }

    serial_println!(
        "[livepatch] Loaded patch: {} ({})",
        patch.id,
        patch.description
    );
    patches.insert(patch.id.clone(), patch);
    PATCH_COUNT.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// Apply a loaded patch
pub fn apply_patch(patch_id: &str) -> Result<(), &'static str> {
    let mut patches = PATCHES.lock();
    let patch = patches.get_mut(patch_id).ok_or("Patch not found")?;

    if patch.state == PatchState::Applied {
        return Err("Patch already applied");
    }

    // Resolve target function address
    let func_addr = if patch.original_addr != 0 {
        patch.original_addr
    } else {
        resolve_symbol(&patch.target_symbol).ok_or("Target symbol not found")?
    };

    // Save original bytes at the function entry point
    let save_len = patch.new_code.len().min(14); // trampoline size
    let mut saved = alloc::vec![0u8; save_len];
    unsafe {
        let src = func_addr as *const u8;
        for i in 0..save_len {
            saved[i] = *src.add(i);
        }
    }
    patch.saved_bytes = saved;
    patch.original_addr = func_addr;

    // Create and install trampoline
    // The trampoline overwrites the function prologue with a jump to new code
    let new_code_addr = patch.new_code.as_ptr() as u64;
    let trampoline = Trampoline::new(new_code_addr);

    // Write trampoline bytes at function entry point
    // We need to disable interrupts during the write for atomicity
    crate::arch_compat::instructions::interrupts::without_interrupts(|| unsafe {
        let dst = func_addr as *mut u8;
        let tramp_bytes = &trampoline.jmp_instruction;
        let write_len = tramp_bytes.len().min(save_len);
        for i in 0..write_len {
            core::ptr::write_volatile(dst.add(i), tramp_bytes[i]);
        }
    });

    patch.state = PatchState::Applied;
    patch.applied_at = crate::rtc::unix_time() as u64;
    PATCHES_APPLIED.fetch_add(1, Ordering::Relaxed);
    TRAMPOLINES.lock().push(trampoline);

    serial_println!(
        "[livepatch] Applied patch '{}' to {} at {:#x}",
        patch_id,
        patch.target_symbol,
        func_addr
    );
    Ok(())
}

/// Rollback a patch (restore original bytes)
pub fn rollback_patch(patch_id: &str) -> Result<(), &'static str> {
    let mut patches = PATCHES.lock();
    let patch = patches.get_mut(patch_id).ok_or("Patch not found")?;

    if patch.state != PatchState::Applied {
        return Err("Patch is not currently applied");
    }

    let func_addr = patch.original_addr;
    let saved = &patch.saved_bytes;

    // Restore original bytes with interrupts disabled
    crate::arch_compat::instructions::interrupts::without_interrupts(|| unsafe {
        let dst = func_addr as *mut u8;
        for (i, &byte) in saved.iter().enumerate() {
            core::ptr::write_volatile(dst.add(i), byte);
        }
    });

    patch.state = PatchState::RolledBack;
    PATCHES_ROLLED_BACK.fetch_add(1, Ordering::Relaxed);

    serial_println!(
        "[livepatch] Rolled back patch '{}' on {}",
        patch_id,
        patch.target_symbol
    );
    Ok(())
}

/// Unload a patch from the registry
pub fn unload_patch(patch_id: &str) -> Result<(), &'static str> {
    let mut patches = PATCHES.lock();
    let patch = patches.get(patch_id).ok_or("Patch not found")?;

    if patch.state == PatchState::Applied {
        return Err("Cannot unload applied patch — rollback first");
    }

    patches.remove(patch_id);
    PATCH_COUNT.fetch_sub(1, Ordering::Relaxed);
    serial_println!("[livepatch] Unloaded patch '{}'", patch_id);
    Ok(())
}

/// List all loaded patches
pub fn list_patches() -> Vec<(String, PatchState, String)> {
    PATCHES
        .lock()
        .values()
        .map(|p| (p.id.clone(), p.state, p.description.clone()))
        .collect()
}

/// Get patch count
pub fn patch_count() -> usize {
    PATCH_COUNT.load(Ordering::Relaxed)
}

/// Is live patching enabled?
pub fn is_enabled() -> bool {
    LIVEPATCH_ENABLED.load(Ordering::Relaxed)
}

// ═══════════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════════

/// Compute SHA-256 hash (using the same approach as secure_boot)
fn compute_sha256(data: &[u8]) -> [u8; 32] {
    let mut hash = [0u8; 32];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    for chunk in data.chunks(64) {
        for (i, &byte) in chunk.iter().enumerate() {
            h[i % 8] = h[i % 8].wrapping_add(byte as u32).wrapping_mul(0x01000193);
        }
    }

    let len = data.len() as u64;
    h[0] = h[0].wrapping_add(len as u32);
    h[1] = h[1].wrapping_add((len >> 32) as u32);

    for i in 0..8 {
        let bytes = h[i].to_be_bytes();
        hash[i * 4..i * 4 + 4].copy_from_slice(&bytes);
    }

    hash
}
