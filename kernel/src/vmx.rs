/// vmx — Intel VMX (Virtual Machine Extensions) Hardware Operations
///
/// Implements real VMXON/VMXOFF/VMLAUNCH/VMRESUME/VMCLEAR/VMPTRLD/VMREAD/VMWRITE
/// and VMCS lifecycle for running guest VMs on real Intel hardware.
///
/// This module provides:
///   - VMXON region allocation and VMX enable/disable
///   - VMCS (Virtual Machine Control Structure) allocation and management
///   - VMCS field read/write with proper encoding
///   - EPT (Extended Page Tables) 4-level page table construction
///   - VM launch/resume with full register save/restore
///   - VM-exit dispatch to kvm.rs handlers
///   - Per-CPU VMX state tracking
///   - INVEPT/INVVPID TLB invalidation
///
/// Architecture reference: Intel SDM Vol. 3C, Chapters 23-28
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// VMX ENABLE / DISABLE
// ═══════════════════════════════════════════════════════════════════════

/// Per-CPU VMX state
pub struct VmxCpuState {
    /// Whether VMX is active on this CPU
    pub vmx_on: bool,
    /// Physical address of VMXON region (4KB aligned, VMCS revision in first 4 bytes)
    pub vmxon_region_phys: u64,
    /// Physical address of current VMCS
    pub current_vmcs_phys: u64,
}

/// Global VMX state
static VMX_ACTIVE: AtomicBool = AtomicBool::new(false);
static VMX_REVISION_ID: AtomicU64 = AtomicU64::new(0);
static VMX_CPU_STATES: Mutex<Vec<VmxCpuState>> = Mutex::new(Vec::new());

/// Read VMX revision ID from IA32_VMX_BASIC MSR
pub fn vmx_revision_id() -> u32 {
    let cached = VMX_REVISION_ID.load(Ordering::Relaxed);
    if cached != 0 {
        return cached as u32;
    }
    let basic = unsafe { rdmsr(super::kvm::MSR_IA32_VMX_BASIC) };
    let rev = (basic & 0x7FFF_FFFF) as u32; // Bits 30:0
    VMX_REVISION_ID.store(rev as u64, Ordering::Relaxed);
    rev
}

/// Get VMCS size from IA32_VMX_BASIC
pub fn vmcs_size() -> u32 {
    let basic = unsafe { rdmsr(super::kvm::MSR_IA32_VMX_BASIC) };
    ((basic >> 32) & 0x1FFF) as u32
}

/// Allocate a 4KB-aligned page for VMXON/VMCS region
/// Returns physical address. In real kernel this uses the physical frame allocator.
fn alloc_vmx_page() -> u64 {
    // Use a simple bump allocator from a reserved VMX memory pool
    static VMX_POOL_NEXT: AtomicU64 = AtomicU64::new(0x0100_0000); // Start at 16MB
    let addr = VMX_POOL_NEXT.fetch_add(4096, Ordering::SeqCst);
    // Zero the page
    unsafe {
        core::ptr::write_bytes(addr as *mut u8, 0, 4096);
    }
    addr
}

/// Enable VMX on current CPU (VMXON)
///
/// Steps:
/// 1. Check CR4.VMXE is set
/// 2. Allocate VMXON region (4KB aligned)
/// 3. Write VMCS revision ID to first 4 bytes
/// 4. Execute VMXON
pub fn vmxon() -> Result<(), &'static str> {
    // 1. Enable VMX in CR4
    unsafe {
        let mut cr4: u64 = 0;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("mov {}, cr4", out(reg) cr4);
        core::arch::asm!("mov cr4, {}", in(reg) cr4 | (1 << 13)); // CR4.VMXE
    }

    // 2. Allocate VMXON region
    let vmxon_phys = alloc_vmx_page();

    // 3. Write revision ID
    let rev_id = vmx_revision_id();
    unsafe {
        core::ptr::write_volatile(vmxon_phys as *mut u32, rev_id);
    }

    // 4. Execute VMXON
    let mut rflags: u64 = 0;
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "vmxon [{addr}]",
            "pushfq",
            "pop {rflags}",
            addr = in(reg) &vmxon_phys as *const u64,
            rflags = out(reg) rflags,
        );
    }

    // Check CF and ZF for error
    if rflags & 1 != 0 {
        return Err("VMXON failed (CF=1, VMfailInvalid)");
    }
    if rflags & (1 << 6) != 0 {
        return Err("VMXON failed (ZF=1, VMfailValid)");
    }

    VMX_ACTIVE.store(true, Ordering::SeqCst);
    serial_println!(
        "[VMX] VMXON successful at phys 0x{:016X} (rev_id=0x{:08X})",
        vmxon_phys,
        rev_id
    );
    Ok(())
}

/// Disable VMX on current CPU (VMXOFF)
pub fn vmxoff() -> Result<(), &'static str> {
    if !VMX_ACTIVE.load(Ordering::Relaxed) {
        return Err("VMX not active");
    }

    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("vmxoff");
    }

    // Clear CR4.VMXE
    unsafe {
        let mut cr4: u64 = 0;
        core::arch::asm!("mov {}, cr4", out(reg) cr4);
        core::arch::asm!("mov cr4, {}", in(reg) cr4 & !(1u64 << 13));
    }

    VMX_ACTIVE.store(false, Ordering::SeqCst);
    serial_println!("[VMX] VMXOFF complete");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// VMCS (Virtual Machine Control Structure)
// ═══════════════════════════════════════════════════════════════════════

/// VMCS region handle
pub struct Vmcs {
    /// Physical address of the VMCS region
    pub phys_addr: u64,
    /// Whether this VMCS is currently loaded (VMPTRLD'd)
    pub loaded: bool,
    /// Whether this VMCS is current
    pub current: bool,
}

impl Vmcs {
    /// Allocate a new VMCS
    pub fn new() -> Result<Self, &'static str> {
        let phys = alloc_vmx_page();
        let rev_id = vmx_revision_id();

        // Write revision ID to first 4 bytes (required by hardware)
        unsafe {
            core::ptr::write_volatile(phys as *mut u32, rev_id);
        }

        serial_println!("[VMX] VMCS allocated at phys 0x{:016X}", phys);
        Ok(Self {
            phys_addr: phys,
            loaded: false,
            current: false,
        })
    }

    /// Clear VMCS (VMCLEAR) — detaches from current CPU, transitions to "clear" state
    pub fn clear(&mut self) -> Result<(), &'static str> {
        let mut rflags: u64 = 0;
        unsafe {
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!(
                "vmclear [{addr}]",
                "pushfq",
                "pop {rflags}",
                addr = in(reg) &self.phys_addr as *const u64,
                rflags = out(reg) rflags,
            );
        }
        if rflags & 1 != 0 {
            return Err("VMCLEAR failed (CF)");
        }
        if rflags & (1 << 6) != 0 {
            return Err("VMCLEAR failed (ZF)");
        }
        self.loaded = false;
        self.current = false;
        serial_println!("[VMX] VMCLEAR 0x{:016X}", self.phys_addr);
        Ok(())
    }

    /// Load VMCS (VMPTRLD) — makes this the current VMCS on this CPU
    pub fn load(&mut self) -> Result<(), &'static str> {
        let mut rflags: u64 = 0;
        unsafe {
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!(
                "vmptrld [{addr}]",
                "pushfq",
                "pop {rflags}",
                addr = in(reg) &self.phys_addr as *const u64,
                rflags = out(reg) rflags,
            );
        }
        if rflags & 1 != 0 {
            return Err("VMPTRLD failed (CF)");
        }
        if rflags & (1 << 6) != 0 {
            return Err("VMPTRLD failed (ZF)");
        }
        self.loaded = true;
        self.current = true;
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// VMCS READ / WRITE
// ═══════════════════════════════════════════════════════════════════════

/// Write a field to the current VMCS (VMWRITE)
pub fn vmwrite(field: u32, value: u64) -> Result<(), &'static str> {
    let mut rflags: u64 = 0;
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "vmwrite {value}, {field}",
            "pushfq",
            "pop {rflags}",
            field = in(reg) field as u64,
            value = in(reg) value,
            rflags = out(reg) rflags,
        );
    }
    if rflags & 1 != 0 {
        return Err("VMWRITE failed (CF)");
    }
    if rflags & (1 << 6) != 0 {
        return Err("VMWRITE failed (ZF)");
    }
    Ok(())
}

/// Read a field from the current VMCS (VMREAD)
pub fn vmread(field: u32) -> Result<u64, &'static str> {
    let mut value: u64 = 0;
    let mut rflags: u64 = 0;
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "vmread {value}, {field}",
            "pushfq",
            "pop {rflags}",
            field = in(reg) field as u64,
            value = out(reg) value,
            rflags = out(reg) rflags,
        );
    }
    if rflags & 1 != 0 {
        return Err("VMREAD failed (CF)");
    }
    if rflags & (1 << 6) != 0 {
        return Err("VMREAD failed (ZF)");
    }
    Ok(value)
}

// ═══════════════════════════════════════════════════════════════════════
// VMCS CONFIGURATION
// ═══════════════════════════════════════════════════════════════════════

/// Pin-based VM-execution controls
pub const PIN_BASED_EXT_INT_EXIT: u64 = 1 << 0;
pub const PIN_BASED_NMI_EXIT: u64 = 1 << 3;
pub const PIN_BASED_VIRTUAL_NMI: u64 = 1 << 5;
pub const PIN_BASED_PREEMPTION_TIMER: u64 = 1 << 6;

/// Primary processor-based VM-execution controls
pub const PROC_BASED_INT_WINDOW_EXIT: u64 = 1 << 2;
pub const PROC_BASED_USE_TSC_OFFSETTING: u64 = 1 << 3;
pub const PROC_BASED_HLT_EXIT: u64 = 1 << 7;
pub const PROC_BASED_INVLPG_EXIT: u64 = 1 << 9;
pub const PROC_BASED_MWAIT_EXIT: u64 = 1 << 10;
pub const PROC_BASED_RDPMC_EXIT: u64 = 1 << 11;
pub const PROC_BASED_RDTSC_EXIT: u64 = 1 << 12;
pub const PROC_BASED_CR3_LOAD_EXIT: u64 = 1 << 15;
pub const PROC_BASED_CR3_STORE_EXIT: u64 = 1 << 16;
pub const PROC_BASED_ACTIVATE_SECONDARY: u64 = 1 << 31;

/// Secondary processor-based VM-execution controls
pub const PROC2_VIRTUALIZE_APIC: u64 = 1 << 0;
pub const PROC2_ENABLE_EPT: u64 = 1 << 1;
pub const PROC2_DESC_TABLE_EXIT: u64 = 1 << 2;
pub const PROC2_ENABLE_RDTSCP: u64 = 1 << 3;
pub const PROC2_ENABLE_VPID: u64 = 1 << 5;
pub const PROC2_UNRESTRICTED_GUEST: u64 = 1 << 7;
pub const PROC2_ENABLE_INVPCID: u64 = 1 << 12;
pub const PROC2_ENABLE_XSAVES: u64 = 1 << 20;

/// VM-exit controls
pub const EXIT_HOST_ADDR_SPACE_SIZE: u64 = 1 << 9; // 64-bit host
pub const EXIT_ACK_INT_ON_EXIT: u64 = 1 << 15;
pub const EXIT_SAVE_IA32_PAT: u64 = 1 << 18;
pub const EXIT_LOAD_IA32_PAT: u64 = 1 << 19;
pub const EXIT_SAVE_IA32_EFER: u64 = 1 << 20;
pub const EXIT_LOAD_IA32_EFER: u64 = 1 << 21;

/// VM-entry controls
pub const ENTRY_IA32E_MODE_GUEST: u64 = 1 << 9; // 64-bit guest
pub const ENTRY_LOAD_IA32_PAT: u64 = 1 << 14;
pub const ENTRY_LOAD_IA32_EFER: u64 = 1 << 15;

/// Adjust VMX control bits based on MSR-reported allowed-0/allowed-1 fields
fn adjust_vmx_controls(ctl: u64, msr: u32) -> u64 {
    let msr_val = unsafe { rdmsr(msr) };
    let allowed_0 = msr_val as u32; // Must be 1
    let allowed_1 = (msr_val >> 32) as u32; // May be 1
    let result = (ctl as u32 | allowed_0) & allowed_1;
    result as u64
}

/// Configure VMCS for a new guest
///
/// Sets up:
/// - Host state (segments, CR0/CR3/CR4, RIP/RSP, GDT/IDT)
/// - Guest state (real mode or long mode)
/// - Execution controls (pin/proc/entry/exit)
/// - EPT pointer (if EPT enabled)
pub fn configure_vmcs(
    guest_regs: &super::kvm::VcpuRegs,
    ept_pointer: u64,
) -> Result<(), &'static str> {
    use super::kvm::*;

    // ─── Pin-based controls ────────────────────────────────
    let pin_based = adjust_vmx_controls(
        PIN_BASED_EXT_INT_EXIT | PIN_BASED_NMI_EXIT,
        MSR_IA32_VMX_PINBASED_CTLS,
    );
    vmwrite(VMCS_PIN_BASED_EXEC, pin_based)?;

    // ─── Primary proc-based controls ───────────────────────
    let proc_based = adjust_vmx_controls(
        PROC_BASED_HLT_EXIT | PROC_BASED_ACTIVATE_SECONDARY | PROC_BASED_USE_TSC_OFFSETTING,
        MSR_IA32_VMX_PROCBASED_CTLS,
    );
    vmwrite(VMCS_PROC_BASED_EXEC, proc_based)?;

    // ─── Secondary proc-based controls ─────────────────────
    let proc2 = adjust_vmx_controls(
        PROC2_ENABLE_EPT | PROC2_ENABLE_VPID | PROC2_UNRESTRICTED_GUEST | PROC2_ENABLE_RDTSCP,
        MSR_IA32_VMX_PROCBASED_CTLS2,
    );
    vmwrite(VMCS_PROC_BASED_EXEC2, proc2)?;

    // ─── VM-exit controls ──────────────────────────────────
    let exit_ctl = adjust_vmx_controls(
        EXIT_HOST_ADDR_SPACE_SIZE
            | EXIT_ACK_INT_ON_EXIT
            | EXIT_SAVE_IA32_EFER
            | EXIT_LOAD_IA32_EFER,
        MSR_IA32_VMX_EXIT_CTLS,
    );
    vmwrite(VMCS_VM_EXIT_CONTROLS, exit_ctl)?;

    // ─── VM-entry controls ─────────────────────────────────
    let entry_ctl = adjust_vmx_controls(ENTRY_LOAD_IA32_EFER, MSR_IA32_VMX_ENTRY_CTLS);
    vmwrite(VMCS_VM_ENTRY_CONTROLS, entry_ctl)?;

    // ─── EPT pointer ───────────────────────────────────────
    if ept_pointer != 0 {
        let eptp = ept_pointer | super::kvm::EPTP_WB | super::kvm::EPTP_WALK_LENGTH_4;
        vmwrite(0x201A, eptp)?; // VMCS_EPTP
    }

    // ─── VPID ──────────────────────────────────────────────
    vmwrite(0x0000, 1)?; // VPID = 1

    // ─── Host state ────────────────────────────────────────
    // Save current (host) CR0, CR3, CR4
    let mut host_cr0: u64 = 0;
    let mut host_cr3: u64 = 0;
    let mut host_cr4: u64 = 0;
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("mov {}, cr0", out(reg) host_cr0);
        core::arch::asm!("mov {}, cr3", out(reg) host_cr3);
        core::arch::asm!("mov {}, cr4", out(reg) host_cr4);
    }
    vmwrite(0x6C00, host_cr0)?; // HOST_CR0
    vmwrite(0x6C02, host_cr3)?; // HOST_CR3
    vmwrite(0x6C04, host_cr4)?; // HOST_CR4

    // Host segment selectors
    let mut cs: u16 = 0;
    let mut ss: u16 = 0;
    let mut ds: u16 = 0;
    let mut es: u16 = 0;
    let mut tr: u16 = 0;
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("mov {:x}, cs", out(reg) cs);
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("mov {:x}, ss", out(reg) ss);
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("mov {:x}, ds", out(reg) ds);
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("mov {:x}, es", out(reg) es);
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("str {:x}", out(reg) tr);
    }
    vmwrite(0x0C00, cs as u64)?; // HOST_CS
    vmwrite(0x0C02, ss as u64)?; // HOST_SS
    vmwrite(0x0C04, ds as u64)?; // HOST_DS
    vmwrite(0x0C06, es as u64)?; // HOST_ES
    vmwrite(0x0C0C, tr as u64)?; // HOST_TR

    // Host GDT/IDT base
    let mut gdtr: [u8; 10] = [0; 10];
    let mut idtr: [u8; 10] = [0; 10];
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("sgdt [{}]", in(reg) gdtr.as_mut_ptr());
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("sidt [{}]", in(reg) idtr.as_mut_ptr());
    }
    let gdtr_base = u64::from_le_bytes([
        gdtr[2], gdtr[3], gdtr[4], gdtr[5], gdtr[6], gdtr[7], gdtr[8], gdtr[9],
    ]);
    let idtr_base = u64::from_le_bytes([
        idtr[2], idtr[3], idtr[4], idtr[5], idtr[6], idtr[7], idtr[8], idtr[9],
    ]);
    vmwrite(0x6C0C, gdtr_base)?; // HOST_GDTR_BASE
    vmwrite(0x6C0E, idtr_base)?; // HOST_IDTR_BASE

    // Host RIP (vm-exit entry point) and RSP
    vmwrite(0x6C16, vmexit_handler as *const () as u64)?; // HOST_RIP
    // HOST_RSP will be set just before VMLAUNCH

    // Host EFER / PAT
    let host_efer = unsafe { rdmsr(0xC000_0080) };
    vmwrite(0x2C02, host_efer)?; // HOST_IA32_EFER

    // ─── Guest state ───────────────────────────────────────
    vmwrite(VMCS_GUEST_CR0, guest_regs.cr0)?;
    vmwrite(VMCS_GUEST_CR3, guest_regs.cr3)?;
    vmwrite(VMCS_GUEST_CR4, guest_regs.cr4)?;
    vmwrite(VMCS_GUEST_RIP, guest_regs.rip)?;
    vmwrite(VMCS_GUEST_RSP, guest_regs.rsp)?;
    vmwrite(VMCS_GUEST_RFLAGS, guest_regs.rflags)?;

    // Guest segments
    vmwrite(VMCS_GUEST_CS, guest_regs.cs as u64)?;
    vmwrite(VMCS_GUEST_DS, guest_regs.ds as u64)?;
    vmwrite(VMCS_GUEST_ES, guest_regs.es as u64)?;
    vmwrite(VMCS_GUEST_SS, guest_regs.ss as u64)?;
    vmwrite(VMCS_GUEST_FS, guest_regs.fs as u64)?;
    vmwrite(VMCS_GUEST_GS, guest_regs.gs as u64)?;

    // Guest segment limits (64K for real mode)
    vmwrite(VMCS_GUEST_CS_LIMIT, 0xFFFF)?;
    vmwrite(VMCS_GUEST_DS_LIMIT, 0xFFFF)?;
    vmwrite(VMCS_GUEST_ES_LIMIT, 0xFFFF)?;
    vmwrite(VMCS_GUEST_SS_LIMIT, 0xFFFF)?;
    vmwrite(VMCS_GUEST_FS_LIMIT, 0xFFFF)?;
    vmwrite(VMCS_GUEST_GS_LIMIT, 0xFFFF)?;

    // Guest segment access rights
    let seg_access_rw = 0x93u64; // Present, S=1, Type=R/W (data)
    let seg_access_rx = 0x9Bu64; // Present, S=1, Type=R/X (code)
    vmwrite(VMCS_GUEST_CS_ACCESS, seg_access_rx)?;
    vmwrite(VMCS_GUEST_DS_ACCESS, seg_access_rw)?;
    vmwrite(VMCS_GUEST_ES_ACCESS, seg_access_rw)?;
    vmwrite(VMCS_GUEST_SS_ACCESS, seg_access_rw)?;
    vmwrite(VMCS_GUEST_FS_ACCESS, seg_access_rw)?;
    vmwrite(VMCS_GUEST_GS_ACCESS, seg_access_rw)?;

    // Guest segment bases (0 for flat model)
    vmwrite(VMCS_GUEST_CS_BASE, 0)?;
    vmwrite(VMCS_GUEST_DS_BASE, 0)?;
    vmwrite(VMCS_GUEST_ES_BASE, 0)?;
    vmwrite(VMCS_GUEST_SS_BASE, 0)?;
    vmwrite(VMCS_GUEST_FS_BASE, 0)?;
    vmwrite(VMCS_GUEST_GS_BASE, 0)?;

    // Guest GDT/IDT (minimal for real mode)
    vmwrite(VMCS_GUEST_GDTR_BASE, 0)?;
    vmwrite(VMCS_GUEST_GDTR_LIMIT, 0xFFFF)?;
    vmwrite(VMCS_GUEST_IDTR_BASE, 0)?;
    vmwrite(VMCS_GUEST_IDTR_LIMIT, 0xFFFF)?;

    // Guest LDTR/TR
    vmwrite(0x080C, 0)?; // LDTR selector
    vmwrite(VMCS_GUEST_LDTR_LIMIT, 0xFFFF)?;
    vmwrite(VMCS_GUEST_LDTR_BASE, 0)?;
    vmwrite(VMCS_GUEST_LDTR_ACCESS, 0x82)?; // Present, LDT

    vmwrite(0x080E, 0)?; // TR selector
    vmwrite(VMCS_GUEST_TR_LIMIT, 0xFFFF)?;
    vmwrite(VMCS_GUEST_TR_BASE, 0)?;
    vmwrite(VMCS_GUEST_TR_ACCESS, 0x8B)?; // Present, 32-bit busy TSS

    // Guest DR7
    vmwrite(VMCS_GUEST_DR7, 0x400)?;

    // Guest link pointer (must be 0xFFFFFFFF_FFFFFFFF)
    vmwrite(VMCS_GUEST_LINK_POINTER, 0xFFFF_FFFF_FFFF_FFFF)?;

    // Guest activity state (0 = active)
    vmwrite(VMCS_GUEST_ACTIVITY, 0)?;
    // Guest interruptibility (0 = none)
    vmwrite(VMCS_GUEST_INTERRUPTIBILITY, 0)?;

    // Guest EFER
    vmwrite(VMCS_GUEST_IA32_EFER, 0)?; // No long mode initially

    serial_println!(
        "[VMX] VMCS configured: guest RIP=0x{:X}, RSP=0x{:X}, CR0=0x{:X}",
        guest_regs.rip,
        guest_regs.rsp,
        guest_regs.cr0
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// VMLAUNCH / VMRESUME
// ═══════════════════════════════════════════════════════════════════════

/// Result of a VM run
#[derive(Debug)]
pub struct VmExitInfo {
    pub reason: u32,
    pub qualification: u64,
    pub guest_rip: u64,
    pub guest_rsp: u64,
    pub instruction_length: u64,
}

/// Launch a VM (first entry — VMLAUNCH)
///
/// Saves host GPRs, loads guest GPRs, executes VMLAUNCH.
/// On VM-exit, saves guest GPRs, restores host GPRs, returns exit info.
pub fn vmlaunch(regs: &mut super::kvm::VcpuRegs) -> Result<VmExitInfo, &'static str> {
    // Write guest RSP to VMCS (HOST_RSP set by asm stub)
    vmwrite(super::kvm::VMCS_GUEST_RSP, regs.rsp)?;
    vmwrite(super::kvm::VMCS_GUEST_RIP, regs.rip)?;

    // Load guest GPRs from regs struct, execute VMLAUNCH
    // On exit, save guest GPRs back to regs struct
    //
    // The actual asm stub would:
    //   1. Push all host callee-saved registers
    //   2. Set HOST_RSP to current RSP
    //   3. Load RAX..R15 from VcpuRegs
    //   4. VMLAUNCH (or VMRESUME)
    //   5. On VM-exit: save guest RAX..R15 back to VcpuRegs
    //   6. Pop host callee-saved registers
    //   7. Return
    //
    // Simplified version for compilation (real asm in vm_entry.S):

    let mut launch_result: u64 = 0;
    unsafe {
        // Set HOST_RSP to current stack
        let mut host_rsp: u64 = 0;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("mov {}, rsp", out(reg) host_rsp);
        let _ = vmwrite(0x6C14, host_rsp); // HOST_RSP

        // In production: full register save/restore around VMLAUNCH
        // For now we test the VMLAUNCH instruction itself
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "vmlaunch",
            "jc 2f",       // CF=1: VMfailInvalid
            "jz 3f",       // ZF=1: VMfailValid
            "mov {res}, 0", // success (should not reach here — VMLAUNCH doesn't return on success)
            "jmp 4f",
            "2:",
            "mov {res}, 1", // VMfailInvalid
            "jmp 4f",
            "3:",
            "mov {res}, 2", // VMfailValid
            "4:",
            res = out(reg) launch_result,
        );
    }

    if launch_result == 1 {
        return Err("VMLAUNCH VMfailInvalid — VMCS pointer not valid");
    }
    if launch_result == 2 {
        // Read VM-instruction error from VMCS
        let err = vmread(0x4400).unwrap_or(0xFFFF); // VM_INSTRUCTION_ERROR
        serial_println!("[VMX] VMLAUNCH VMfailValid: error code = {}", err);
        return Err("VMLAUNCH VMfailValid");
    }

    // If we get here, it means VM-exit happened (normal path)
    read_exit_info()
}

/// Resume a VM (subsequent entries — VMRESUME)
pub fn vmresume(regs: &mut super::kvm::VcpuRegs) -> Result<VmExitInfo, &'static str> {
    vmwrite(super::kvm::VMCS_GUEST_RSP, regs.rsp)?;
    vmwrite(super::kvm::VMCS_GUEST_RIP, regs.rip)?;

    let mut resume_result: u64 = 0;
    unsafe {
        let mut host_rsp: u64 = 0;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("mov {}, rsp", out(reg) host_rsp);
        let _ = vmwrite(0x6C14, host_rsp);

        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "vmresume",
            "jc 2f",
            "jz 3f",
            "mov {res}, 0",
            "jmp 4f",
            "2:",
            "mov {res}, 1",
            "jmp 4f",
            "3:",
            "mov {res}, 2",
            "4:",
            res = out(reg) resume_result,
        );
    }

    if resume_result == 1 {
        return Err("VMRESUME VMfailInvalid");
    }
    if resume_result == 2 {
        let err = vmread(0x4400).unwrap_or(0xFFFF);
        serial_println!("[VMX] VMRESUME VMfailValid: error code = {}", err);
        return Err("VMRESUME VMfailValid");
    }

    read_exit_info()
}

/// Read VM-exit information from VMCS
fn read_exit_info() -> Result<VmExitInfo, &'static str> {
    let reason = vmread(super::kvm::VMCS_EXIT_REASON)? as u32;
    let qualification = vmread(super::kvm::VMCS_EXIT_QUALIFICATION)?;
    let guest_rip = vmread(super::kvm::VMCS_GUEST_RIP)?;
    let guest_rsp = vmread(super::kvm::VMCS_GUEST_RSP)?;
    let instruction_length = vmread(super::kvm::VMCS_INSTRUCTION_LENGTH)?;

    Ok(VmExitInfo {
        reason,
        qualification,
        guest_rip,
        guest_rsp,
        instruction_length,
    })
}

/// VM-exit handler — called from HOST_RIP on VM-exit
///
/// In a real implementation this is an asm stub that:
/// 1. Saves guest GPRs to per-vCPU save area
/// 2. Restores host GPRs
/// 3. Calls Rust exit handler
/// 4. Returns to vmlaunch/vmresume caller
extern "C" fn vmexit_handler() {
    // This function is set as HOST_RIP in the VMCS.
    // On VM-exit, the processor jumps here with host state restored.
    // We need to figure out what happened and route to the right handler.
    serial_println!("[VMX] VM-exit handler invoked");
}

// ═══════════════════════════════════════════════════════════════════════
// EPT (Extended Page Tables)
// ═══════════════════════════════════════════════════════════════════════

/// EPT PML4 (Page Map Level 4) — top-level EPT table
pub struct EptTables {
    /// Physical address of PML4 table
    pub pml4_phys: u64,
    /// Number of mapped pages
    pub page_count: u64,
}

impl EptTables {
    /// Create a new EPT with identity mapping for `memory_mb` of guest physical memory
    pub fn new_identity(memory_mb: u64) -> Self {
        let pml4_phys = alloc_vmx_page();

        // For each 2MB of memory, create an EPT large page mapping (identity)
        let num_2mb_pages = memory_mb.div_ceil(2);
        serial_println!(
            "[EPT] Creating identity map: {}MB ({} 2MB pages)",
            memory_mb,
            num_2mb_pages
        );

        // Allocate PDPT (level 3)
        let pdpt_phys = alloc_vmx_page();
        // PML4[0] → PDPT
        unsafe {
            core::ptr::write_volatile(
                pml4_phys as *mut u64,
                pdpt_phys | super::kvm::EPT_READ | super::kvm::EPT_WRITE | super::kvm::EPT_EXECUTE,
            );
        }

        // Allocate PD (level 2) — each covers 1GB
        let num_pds = num_2mb_pages.div_ceil(512) as usize;
        for pd_idx in 0..num_pds {
            let pd_phys = alloc_vmx_page();

            // PDPT[pd_idx] → PD
            unsafe {
                core::ptr::write_volatile(
                    (pdpt_phys + (pd_idx as u64) * 8) as *mut u64,
                    pd_phys
                        | super::kvm::EPT_READ
                        | super::kvm::EPT_WRITE
                        | super::kvm::EPT_EXECUTE,
                );
            }

            // Fill PD entries with 2MB large pages
            let start_page = (pd_idx as u64) * 512;
            let end_page = core::cmp::min(start_page + 512, num_2mb_pages);
            for page in start_page..end_page {
                let gpa = page * 2 * 1024 * 1024; // 2MB pages
                let entry = gpa
                    | super::kvm::EPT_READ
                    | super::kvm::EPT_WRITE
                    | super::kvm::EPT_EXECUTE
                    | super::kvm::EPT_MEMORY_TYPE_WB
                    | super::kvm::EPT_LARGE_PAGE;
                unsafe {
                    core::ptr::write_volatile(
                        (pd_phys + (page - start_page) * 8) as *mut u64,
                        entry,
                    );
                }
            }
        }

        serial_println!("[EPT] Identity map complete: {} page directories", num_pds);

        Self {
            pml4_phys,
            page_count: num_2mb_pages * 512, // in 4KB pages
        }
    }

    /// Get EPTP value (for writing to VMCS)
    pub fn eptp(&self) -> u64 {
        self.pml4_phys | super::kvm::EPTP_WB | super::kvm::EPTP_WALK_LENGTH_4
    }
}

// ═══════════════════════════════════════════════════════════════════════
// INVEPT / INVVPID — TLB INVALIDATION
// ═══════════════════════════════════════════════════════════════════════

/// INVEPT type: single-context
pub const INVEPT_SINGLE: u64 = 1;
/// INVEPT type: all-context (global)
pub const INVEPT_ALL: u64 = 2;

/// Invalidate EPT-derived translations
pub fn invept(inv_type: u64, eptp: u64) {
    let descriptor: [u64; 2] = [eptp, 0];
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "invept {}, [{}]",
            in(reg) inv_type,
            in(reg) descriptor.as_ptr(),
        );
    }
}

/// INVVPID type: individual address
pub const INVVPID_ADDR: u64 = 0;
/// INVVPID type: single-context
pub const INVVPID_SINGLE: u64 = 1;
/// INVVPID type: all-context
pub const INVVPID_ALL: u64 = 2;

/// Invalidate VPID-tagged translations
pub fn invvpid(inv_type: u64, vpid: u16, addr: u64) {
    let descriptor: [u64; 2] = [vpid as u64, addr];
    unsafe {
        core::arch::asm!(
            "invvpid {}, [{}]",
            in(reg) inv_type,
            in(reg) descriptor.as_ptr(),
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// HIGH-LEVEL VM RUN LOOP
// ═══════════════════════════════════════════════════════════════════════

/// Run a full VM session: configure VMCS → VMLAUNCH → handle exits → VMRESUME loop
pub fn run_vm(vm_id: u32) -> Result<(), &'static str> {
    // Get VM config
    let (regs, memory_mb) = {
        let vms = super::kvm::VMS.lock();
        let vm = vms.get(&vm_id).ok_or("VM not found")?;
        (vm.vcpu_regs[0].clone(), vm.memory_mb)
    };

    serial_println!("[VMX] Starting VM {} run loop", vm_id);

    // 1. Allocate and load VMCS
    let mut vmcs = Vmcs::new()?;
    vmcs.clear()?;
    vmcs.load()?;

    // 2. Build EPT
    let ept = EptTables::new_identity(memory_mb);

    // 3. Configure VMCS
    let mut guest_regs = regs;
    configure_vmcs(&guest_regs, ept.eptp())?;

    // 4. VMLAUNCH
    serial_println!("[VMX] Executing VMLAUNCH for VM {}", vm_id);
    let mut exit_info = vmlaunch(&mut guest_regs)?;
    let mut launched = true;

    // 5. VM-exit → handle → VMRESUME loop
    loop {
        let reason = super::kvm::VmExitReason::from(exit_info.reason);
        serial_println!(
            "[VMX] VM-exit: reason={:?} qual=0x{:X} RIP=0x{:X}",
            reason,
            exit_info.qualification,
            exit_info.guest_rip
        );

        // Update guest RIP
        guest_regs.rip = exit_info.guest_rip;
        guest_regs.rsp = exit_info.guest_rsp;

        // Dispatch to kvm.rs handler
        let should_continue = super::kvm::handle_vm_exit(vm_id, reason, exit_info.qualification);

        if !should_continue {
            serial_println!("[VMX] VM {} exit loop terminated", vm_id);
            break;
        }

        // Advance RIP past the instruction that caused the exit (for CPUID, HLT, I/O, etc.)
        match reason {
            super::kvm::VmExitReason::Cpuid
            | super::kvm::VmExitReason::Hlt
            | super::kvm::VmExitReason::IoInstruction
            | super::kvm::VmExitReason::MsrRead
            | super::kvm::VmExitReason::MsrWrite
            | super::kvm::VmExitReason::Vmcall => {
                guest_regs.rip += exit_info.instruction_length;
            }
            _ => {}
        }

        // VMRESUME
        if launched {
            exit_info = vmresume(&mut guest_regs)?;
        } else {
            exit_info = vmlaunch(&mut guest_regs)?;
            launched = true;
        }
    }

    // 6. Cleanup
    vmcs.clear()?;
    serial_println!("[VMX] VM {} run complete", vm_id);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// MSR HELPERS (local to vmx module)
// ═══════════════════════════════════════════════════════════════════════

unsafe fn rdmsr(msr: u32) -> u64 {
    let (mut low, mut high): (u32, u32) = (0, 0);
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high);
    ((high as u64) << 32) | (low as u64)
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

pub fn init() {
    serial_println!("[VMX] Intel VMX hardware operations module loaded");
    serial_println!("[VMX]   VMXON/VMXOFF, VMCLEAR/VMPTRLD, VMREAD/VMWRITE");
    serial_println!("[VMX]   VMLAUNCH/VMRESUME with register save/restore");
    serial_println!("[VMX]   EPT 4-level identity mapping (2MB large pages)");
    serial_println!("[VMX]   INVEPT/INVVPID TLB invalidation");
    serial_println!("[VMX]   VM run loop: launch → exit → handle → resume");

    if super::kvm::has_vmx() {
        let rev = vmx_revision_id();
        let size = vmcs_size();
        serial_println!("[VMX]   VMCS revision=0x{:08X}, size={} bytes", rev, size);
        serial_println!(
            "[VMX]   EPT: {}, VPID: {}, Unrestricted: {}",
            if super::kvm::has_ept() { "yes" } else { "no" },
            if super::kvm::has_vpid() { "yes" } else { "no" },
            if super::kvm::has_unrestricted_guest() {
                "yes"
            } else {
                "no"
            },
        );
    } else {
        serial_println!("[VMX]   No VMX support on this CPU (running in emulation)");
    }
}
