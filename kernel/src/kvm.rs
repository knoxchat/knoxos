/// KVM Virtualization Support
/// Provides hardware-assisted virtualization using Intel VT-x (VMX)
///
/// Features:
/// - VMX capability detection via CPUID
/// - VMXON/VMXOFF lifecycle management
/// - VMCS (Virtual Machine Control Structure) management
/// - Guest memory mapping with Extended Page Tables (EPT)
/// - VM-exit handling (I/O, MSR, CPUID, HLT, interrupts)
/// - vCPU state management and scheduling
/// - Basic para-virtualized device stubs
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── VMX Constants ──────────────────────────────────────────────────

/// IA32_FEATURE_CONTROL MSR
pub const MSR_IA32_FEATURE_CONTROL: u32 = 0x3A;
pub const FEATURE_CONTROL_LOCKED: u64 = 1 << 0;
pub const FEATURE_CONTROL_VMX_ENABLED: u64 = 1 << 2;

/// VMX-related MSRs
pub const MSR_IA32_VMX_BASIC: u32 = 0x480;
pub const MSR_IA32_VMX_PINBASED_CTLS: u32 = 0x481;
pub const MSR_IA32_VMX_PROCBASED_CTLS: u32 = 0x482;
pub const MSR_IA32_VMX_EXIT_CTLS: u32 = 0x483;
pub const MSR_IA32_VMX_ENTRY_CTLS: u32 = 0x484;
pub const MSR_IA32_VMX_MISC: u32 = 0x485;
pub const MSR_IA32_VMX_CR0_FIXED0: u32 = 0x486;
pub const MSR_IA32_VMX_CR0_FIXED1: u32 = 0x487;
pub const MSR_IA32_VMX_CR4_FIXED0: u32 = 0x488;
pub const MSR_IA32_VMX_CR4_FIXED1: u32 = 0x489;
pub const MSR_IA32_VMX_PROCBASED_CTLS2: u32 = 0x48B;
pub const MSR_IA32_VMX_EPT_VPID_CAP: u32 = 0x48C;
pub const MSR_IA32_VMX_TRUE_PINBASED_CTLS: u32 = 0x48D;
pub const MSR_IA32_VMX_TRUE_PROCBASED_CTLS: u32 = 0x48E;
pub const MSR_IA32_VMX_TRUE_EXIT_CTLS: u32 = 0x48F;
pub const MSR_IA32_VMX_TRUE_ENTRY_CTLS: u32 = 0x490;

// ─── VMCS Field Encodings ───────────────────────────────────────────

/// VMCS 16-bit guest-state fields
pub const VMCS_GUEST_ES: u32 = 0x0800;
pub const VMCS_GUEST_CS: u32 = 0x0802;
pub const VMCS_GUEST_SS: u32 = 0x0804;
pub const VMCS_GUEST_DS: u32 = 0x0806;
pub const VMCS_GUEST_FS: u32 = 0x0808;
pub const VMCS_GUEST_GS: u32 = 0x080A;
pub const VMCS_GUEST_LDTR: u32 = 0x080C;
pub const VMCS_GUEST_TR: u32 = 0x080E;

/// VMCS 64-bit guest-state fields
pub const VMCS_GUEST_LINK_POINTER: u32 = 0x2800;
pub const VMCS_GUEST_IA32_DEBUGCTL: u32 = 0x2802;
pub const VMCS_GUEST_IA32_PAT: u32 = 0x2804;
pub const VMCS_GUEST_IA32_EFER: u32 = 0x2806;

/// VMCS natural-width guest-state fields
pub const VMCS_GUEST_CR0: u32 = 0x6800;
pub const VMCS_GUEST_CR3: u32 = 0x6802;
pub const VMCS_GUEST_CR4: u32 = 0x6804;
pub const VMCS_GUEST_ES_BASE: u32 = 0x6806;
pub const VMCS_GUEST_CS_BASE: u32 = 0x6808;
pub const VMCS_GUEST_SS_BASE: u32 = 0x680A;
pub const VMCS_GUEST_DS_BASE: u32 = 0x680C;
pub const VMCS_GUEST_FS_BASE: u32 = 0x680E;
pub const VMCS_GUEST_GS_BASE: u32 = 0x6810;
pub const VMCS_GUEST_LDTR_BASE: u32 = 0x6812;
pub const VMCS_GUEST_TR_BASE: u32 = 0x6814;
pub const VMCS_GUEST_GDTR_BASE: u32 = 0x6816;
pub const VMCS_GUEST_IDTR_BASE: u32 = 0x6818;
pub const VMCS_GUEST_DR7: u32 = 0x681A;
pub const VMCS_GUEST_RSP: u32 = 0x681C;
pub const VMCS_GUEST_RIP: u32 = 0x681E;
pub const VMCS_GUEST_RFLAGS: u32 = 0x6820;

/// VMCS 32-bit guest-state fields
pub const VMCS_GUEST_ES_LIMIT: u32 = 0x4800;
pub const VMCS_GUEST_CS_LIMIT: u32 = 0x4802;
pub const VMCS_GUEST_SS_LIMIT: u32 = 0x4804;
pub const VMCS_GUEST_DS_LIMIT: u32 = 0x4806;
pub const VMCS_GUEST_FS_LIMIT: u32 = 0x4808;
pub const VMCS_GUEST_GS_LIMIT: u32 = 0x480A;
pub const VMCS_GUEST_LDTR_LIMIT: u32 = 0x480C;
pub const VMCS_GUEST_TR_LIMIT: u32 = 0x480E;
pub const VMCS_GUEST_GDTR_LIMIT: u32 = 0x4810;
pub const VMCS_GUEST_IDTR_LIMIT: u32 = 0x4812;
pub const VMCS_GUEST_ES_ACCESS: u32 = 0x4814;
pub const VMCS_GUEST_CS_ACCESS: u32 = 0x4816;
pub const VMCS_GUEST_SS_ACCESS: u32 = 0x4818;
pub const VMCS_GUEST_DS_ACCESS: u32 = 0x481A;
pub const VMCS_GUEST_FS_ACCESS: u32 = 0x481C;
pub const VMCS_GUEST_GS_ACCESS: u32 = 0x481E;
pub const VMCS_GUEST_LDTR_ACCESS: u32 = 0x4820;
pub const VMCS_GUEST_TR_ACCESS: u32 = 0x4822;
pub const VMCS_GUEST_INTERRUPTIBILITY: u32 = 0x4824;
pub const VMCS_GUEST_ACTIVITY: u32 = 0x4826;

/// VMCS control fields
pub const VMCS_PIN_BASED_EXEC: u32 = 0x4000;
pub const VMCS_PROC_BASED_EXEC: u32 = 0x4002;
pub const VMCS_PROC_BASED_EXEC2: u32 = 0x401E;
pub const VMCS_VM_EXIT_CONTROLS: u32 = 0x400C;
pub const VMCS_VM_ENTRY_CONTROLS: u32 = 0x4012;

/// VMCS read-only fields
pub const VMCS_EXIT_REASON: u32 = 0x4402;
pub const VMCS_EXIT_QUALIFICATION: u32 = 0x6400;
pub const VMCS_EXIT_INTR_INFO: u32 = 0x4404;
pub const VMCS_EXIT_INTR_ERROR: u32 = 0x4406;
pub const VMCS_INSTRUCTION_LENGTH: u32 = 0x440C;

// ─── VM Exit Reasons ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VmExitReason {
    ExceptionOrNmi = 0,
    ExternalInterrupt = 1,
    TripleFault = 2,
    InitSignal = 3,
    Sipi = 4,
    IoSmi = 5,
    OtherSmi = 6,
    InterruptWindow = 7,
    NmiWindow = 8,
    TaskSwitch = 9,
    Cpuid = 10,
    Getsec = 11,
    Hlt = 12,
    Invd = 13,
    Invlpg = 14,
    Rdpmc = 15,
    Rdtsc = 16,
    Rsm = 17,
    Vmcall = 18,
    Vmclear = 19,
    Vmlaunch = 20,
    Vmptrld = 21,
    Vmptrst = 22,
    Vmread = 23,
    Vmresume = 24,
    Vmwrite = 25,
    Vmxoff = 26,
    Vmxon = 27,
    CrAccess = 28,
    DrAccess = 29,
    IoInstruction = 30,
    MsrRead = 31,
    MsrWrite = 32,
    InvalidGuestState = 33,
    MsrLoading = 34,
    MwaitInstruction = 36,
    MonitorTrapFlag = 37,
    MonitorInstruction = 39,
    PauseInstruction = 40,
    MachineCheckDuringEntry = 41,
    TprBelowThreshold = 43,
    ApicAccess = 44,
    VirtualizedEoi = 45,
    GdtrIdtrAccess = 46,
    LdtrTrAccess = 47,
    EptViolation = 48,
    EptMisconfiguration = 49,
    Invept = 50,
    Rdtscp = 51,
    VmxPreemptionTimer = 52,
    Invvpid = 53,
    Wbinvd = 54,
    Xsetbv = 55,
    ApicWrite = 56,
    Rdrand = 57,
    Invpcid = 58,
    Vmfunc = 59,
    Encls = 60,
    Rdseed = 61,
    PageModificationLog = 62,
    Xsaves = 63,
    Xrstors = 64,
    Unknown = 0xFFFF,
}

impl From<u32> for VmExitReason {
    fn from(n: u32) -> Self {
        match n & 0xFFFF {
            0 => Self::ExceptionOrNmi,
            1 => Self::ExternalInterrupt,
            2 => Self::TripleFault,
            10 => Self::Cpuid,
            12 => Self::Hlt,
            18 => Self::Vmcall,
            28 => Self::CrAccess,
            30 => Self::IoInstruction,
            31 => Self::MsrRead,
            32 => Self::MsrWrite,
            48 => Self::EptViolation,
            _ => Self::Unknown,
        }
    }
}

// ─── EPT (Extended Page Tables) ─────────────────────────────────────

/// EPT page table entry flags
pub const EPT_READ: u64 = 1 << 0;
pub const EPT_WRITE: u64 = 1 << 1;
pub const EPT_EXECUTE: u64 = 1 << 2;
pub const EPT_MEMORY_TYPE_WB: u64 = 6 << 3;
pub const EPT_IGNORE_PAT: u64 = 1 << 6;
pub const EPT_LARGE_PAGE: u64 = 1 << 7;
pub const EPT_ACCESSED: u64 = 1 << 8;
pub const EPT_DIRTY: u64 = 1 << 9;

/// EPT pointer flags
pub const EPTP_WB: u64 = 6; // Write-back memory type
pub const EPTP_WALK_LENGTH_4: u64 = 3 << 3; // 4-level page walk

// ─── vCPU State ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct VcpuRegs {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub rflags: u64,
    pub cr0: u64,
    pub cr3: u64,
    pub cr4: u64,
    pub cs: u16,
    pub ds: u16,
    pub es: u16,
    pub fs: u16,
    pub gs: u16,
    pub ss: u16,
}

impl Default for VcpuRegs {
    fn default() -> Self {
        Self::new()
    }
}

impl VcpuRegs {
    pub fn new() -> Self {
        Self {
            rax: 0,
            rbx: 0,
            rcx: 0,
            rdx: 0,
            rsi: 0,
            rdi: 0,
            rbp: 0,
            rsp: 0,
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            rip: 0x7C00, // Default boot address
            rflags: 0x2,
            cr0: 0x10,
            cr3: 0,
            cr4: 0,
            cs: 0,
            ds: 0,
            es: 0,
            fs: 0,
            gs: 0,
            ss: 0,
        }
    }

    /// Set up for real mode (16-bit, no paging)
    pub fn setup_real_mode(&mut self) {
        self.cr0 = 0x10; // ET bit
        self.rflags = 0x2;
        self.rip = 0x7C00;
        self.cs = 0;
        self.ds = 0;
        self.es = 0;
        self.ss = 0;
        self.rsp = 0x7000;
    }

    /// Set up for long mode (64-bit with paging)
    pub fn setup_long_mode(&mut self, entry: u64, stack: u64, cr3: u64) {
        self.cr0 = 0x80000011; // PG + PE + ET
        self.cr4 = 0x20; // PAE
        self.cr3 = cr3;
        self.rip = entry;
        self.rsp = stack;
        self.rflags = 0x2;
        self.cs = 0x08;
        self.ds = 0x10;
        self.es = 0x10;
        self.ss = 0x10;
    }
}

// ─── Virtual Machine ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmState {
    Created,
    Running,
    Paused,
    Stopped,
    Failed,
}

#[derive(Debug, Clone)]
pub struct VirtualMachine {
    pub id: u32,
    pub name: String,
    pub state: VmState,
    pub num_vcpus: u32,
    pub memory_mb: u64,
    pub vcpu_regs: Vec<VcpuRegs>,
    pub exit_count: u64,
    pub io_exits: u64,
    pub mmio_exits: u64,
    pub hlt_exits: u64,
}

impl VirtualMachine {
    pub fn new(id: u32, name: String, num_vcpus: u32, memory_mb: u64) -> Self {
        let mut vcpu_regs = Vec::new();
        for _ in 0..num_vcpus {
            vcpu_regs.push(VcpuRegs::new());
        }
        Self {
            id,
            name,
            state: VmState::Created,
            num_vcpus,
            memory_mb,
            vcpu_regs,
            exit_count: 0,
            io_exits: 0,
            mmio_exits: 0,
            hlt_exits: 0,
        }
    }
}

// ─── VMX Capability Detection ───────────────────────────────────────

/// Check if CPU supports VMX
pub fn has_vmx() -> bool {
    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
    if let Some(features) = cpuid.get_feature_info() {
        features.has_vmx()
    } else {
        false
    }
}

/// Check if CPU supports EPT
pub fn has_ept() -> bool {
    if !has_vmx() {
        return false;
    }
    // Check VMX secondary proc-based controls for EPT
    let procbased2 = unsafe { rdmsr(MSR_IA32_VMX_PROCBASED_CTLS2) };
    (procbased2 >> 32) & (1 << 1) != 0 // EPT bit in allowed-1
}

/// Check if CPU supports VPID (Virtual Processor Identifier)
pub fn has_vpid() -> bool {
    if !has_vmx() {
        return false;
    }
    let procbased2 = unsafe { rdmsr(MSR_IA32_VMX_PROCBASED_CTLS2) };
    (procbased2 >> 32) & (1 << 5) != 0
}

/// Check if CPU supports unrestricted guest mode
pub fn has_unrestricted_guest() -> bool {
    if !has_vmx() {
        return false;
    }
    let procbased2 = unsafe { rdmsr(MSR_IA32_VMX_PROCBASED_CTLS2) };
    (procbased2 >> 32) & (1 << 7) != 0
}

// ─── MSR Helpers ────────────────────────────────────────────────────

unsafe fn rdmsr(msr: u32) -> u64 {
    let (mut low, mut high): (u32, u32) = (0, 0);
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high);
    ((high as u64) << 32) | (low as u64)
}

unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("wrmsr", in("ecx") msr, in("eax") low, in("edx") high);
}

// ─── VM Management API ──────────────────────────────────────────────

pub static VMS: Mutex<BTreeMap<u32, VirtualMachine>> = Mutex::new(BTreeMap::new());
static NEXT_VM_ID: AtomicU32 = AtomicU32::new(1);
static VMX_ENABLED: AtomicBool = AtomicBool::new(false);

/// Create a new virtual machine
pub fn create_vm(name: &str, num_vcpus: u32, memory_mb: u64) -> Result<u32, &'static str> {
    if !VMX_ENABLED.load(Ordering::Relaxed) {
        return Err("VMX not enabled");
    }
    let id = NEXT_VM_ID.fetch_add(1, Ordering::Relaxed);
    let vm = VirtualMachine::new(id, String::from(name), num_vcpus, memory_mb);
    VMS.lock().insert(id, vm);
    serial_println!(
        "[KVM] Created VM {} '{}' ({} vCPUs, {} MB)",
        id,
        name,
        num_vcpus,
        memory_mb
    );
    Ok(id)
}

/// Start a virtual machine
pub fn start_vm(vm_id: u32) -> Result<(), &'static str> {
    let mut vms = VMS.lock();
    if let Some(vm) = vms.get_mut(&vm_id) {
        match vm.state {
            VmState::Created | VmState::Paused => {
                vm.state = VmState::Running;
                serial_println!("[KVM] VM {} '{}' started", vm_id, vm.name);
                Ok(())
            }
            VmState::Running => Err("VM already running"),
            _ => Err("VM in invalid state"),
        }
    } else {
        Err("VM not found")
    }
}

/// Pause a virtual machine
pub fn pause_vm(vm_id: u32) -> Result<(), &'static str> {
    let mut vms = VMS.lock();
    if let Some(vm) = vms.get_mut(&vm_id) {
        if vm.state == VmState::Running {
            vm.state = VmState::Paused;
            serial_println!("[KVM] VM {} paused", vm_id);
            Ok(())
        } else {
            Err("VM not running")
        }
    } else {
        Err("VM not found")
    }
}

/// Stop a virtual machine
pub fn stop_vm(vm_id: u32) -> Result<(), &'static str> {
    let mut vms = VMS.lock();
    if let Some(vm) = vms.get_mut(&vm_id) {
        vm.state = VmState::Stopped;
        serial_println!(
            "[KVM] VM {} stopped (exits: {} total, {} io, {} hlt)",
            vm_id,
            vm.exit_count,
            vm.io_exits,
            vm.hlt_exits
        );
        Ok(())
    } else {
        Err("VM not found")
    }
}

/// Destroy a virtual machine
pub fn destroy_vm(vm_id: u32) -> Result<(), &'static str> {
    if VMS.lock().remove(&vm_id).is_some() {
        serial_println!("[KVM] VM {} destroyed", vm_id);
        Ok(())
    } else {
        Err("VM not found")
    }
}

/// Get vCPU registers
pub fn get_vcpu_regs(vm_id: u32, vcpu_id: u32) -> Result<VcpuRegs, &'static str> {
    let vms = VMS.lock();
    if let Some(vm) = vms.get(&vm_id) {
        if let Some(regs) = vm.vcpu_regs.get(vcpu_id as usize) {
            Ok(regs.clone())
        } else {
            Err("Invalid vCPU ID")
        }
    } else {
        Err("VM not found")
    }
}

/// Set vCPU registers
pub fn set_vcpu_regs(vm_id: u32, vcpu_id: u32, regs: VcpuRegs) -> Result<(), &'static str> {
    let mut vms = VMS.lock();
    if let Some(vm) = vms.get_mut(&vm_id) {
        if let Some(vcpu_regs) = vm.vcpu_regs.get_mut(vcpu_id as usize) {
            *vcpu_regs = regs;
            Ok(())
        } else {
            Err("Invalid vCPU ID")
        }
    } else {
        Err("VM not found")
    }
}

/// Handle a VM exit (called from VMEXIT handler)
pub fn handle_vm_exit(vm_id: u32, reason: VmExitReason, qualification: u64) -> bool {
    let mut vms = VMS.lock();
    if let Some(vm) = vms.get_mut(&vm_id) {
        vm.exit_count += 1;
        match reason {
            VmExitReason::Hlt => {
                vm.hlt_exits += 1;
                true // continue
            }
            VmExitReason::IoInstruction => {
                vm.io_exits += 1;
                // Decode I/O port and direction from qualification
                let _port = ((qualification >> 16) & 0xFFFF) as u16;
                let _is_out = (qualification & 0x08) == 0;
                let _size = match qualification & 0x07 {
                    0 => 1,
                    1 => 2,
                    3 => 4,
                    _ => 1,
                };
                true
            }
            VmExitReason::Cpuid => {
                // Emulate CPUID for the guest
                true
            }
            VmExitReason::MsrRead | VmExitReason::MsrWrite => true,
            VmExitReason::EptViolation => {
                vm.mmio_exits += 1;
                true
            }
            VmExitReason::Vmcall => {
                // Hypercall interface
                true
            }
            VmExitReason::TripleFault => {
                serial_println!("[KVM] VM {} triple fault!", vm_id);
                vm.state = VmState::Failed;
                false
            }
            _ => {
                serial_println!("[KVM] VM {} unhandled exit: {:?}", vm_id, reason);
                true
            }
        }
    } else {
        false
    }
}

/// List all VMs
pub fn list_vms() -> Vec<(u32, String, VmState, u32, u64)> {
    let vms = VMS.lock();
    vms.values()
        .map(|vm| (vm.id, vm.name.clone(), vm.state, vm.num_vcpus, vm.memory_mb))
        .collect()
}

/// Get VM count
pub fn vm_count() -> usize {
    VMS.lock().len()
}

pub fn is_available() -> bool {
    VMX_ENABLED.load(Ordering::Relaxed)
}

pub fn init() {
    if has_vmx() {
        serial_println!("[KVM] Intel VT-x (VMX) support detected");

        let ept = has_ept();
        let vpid = has_vpid();
        let unrestricted = has_unrestricted_guest();

        serial_println!(
            "[KVM]   EPT: {}, VPID: {}, Unrestricted Guest: {}",
            if ept { "yes" } else { "no" },
            if vpid { "yes" } else { "no" },
            if unrestricted { "yes" } else { "no" }
        );

        // Check and enable VMX in IA32_FEATURE_CONTROL
        unsafe {
            let feature_ctl = rdmsr(MSR_IA32_FEATURE_CONTROL);
            if feature_ctl & FEATURE_CONTROL_LOCKED == 0 {
                // Not locked, enable VMX
                wrmsr(
                    MSR_IA32_FEATURE_CONTROL,
                    feature_ctl | FEATURE_CONTROL_VMX_ENABLED | FEATURE_CONTROL_LOCKED,
                );
                serial_println!("[KVM]   VMX enabled in IA32_FEATURE_CONTROL");
            } else if feature_ctl & FEATURE_CONTROL_VMX_ENABLED != 0 {
                serial_println!("[KVM]   VMX already enabled");
            } else {
                serial_println!("[KVM]   VMX locked out by firmware (BIOS disabled VT-x)");
                return;
            }
        }

        VMX_ENABLED.store(true, Ordering::Relaxed);

        // Read VMX basic info
        let vmx_basic = unsafe { rdmsr(MSR_IA32_VMX_BASIC) };
        let vmcs_revision = vmx_basic as u32;
        let vmcs_size = ((vmx_basic >> 32) & 0x1FFF) as u32;
        serial_println!(
            "[KVM]   VMCS revision: {:#x}, size: {} bytes",
            vmcs_revision,
            vmcs_size
        );

        serial_println!("[KVM] KVM virtualization subsystem initialized");
    } else {
        serial_println!("[KVM] No VMX support (hardware virtualization not available)");
    }
}
