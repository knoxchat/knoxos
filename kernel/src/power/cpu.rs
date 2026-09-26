use spin::Mutex;

use crate::serial_println;

use super::manager::PM;
use super::types::CpuFreqGovernor;

// ── MSRs for CPU P-state control ────────────────────────────────────────

/// IA32_PERF_CTL — write desired P-state ratio
const MSR_IA32_PERF_CTL: u32 = 0x199;
/// IA32_PERF_STATUS — read current P-state
const MSR_IA32_PERF_STATUS: u32 = 0x198;
/// IA32_MISC_ENABLE
const MSR_IA32_MISC_ENABLE: u32 = 0x1A0;
/// IA32_PM_ENABLE (HWP)
const MSR_IA32_PM_ENABLE: u32 = 0x770;
/// IA32_HWP_REQUEST
const MSR_IA32_HWP_REQUEST: u32 = 0x774;
/// IA32_HWP_CAPABILITIES
const MSR_IA32_HWP_CAPABILITIES: u32 = 0x771;
/// IA32_MWAIT_LEAF
const CPUID_MWAIT_LEAF: u32 = 5;

// ── CPU save state for S3 resume ────────────────────────────────────────

/// Saved CPU state across S3 suspend (one per CPU)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuSaveState {
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
    /// GDT base
    pub gdtr_base: u64,
    pub gdtr_limit: u16,
    /// IDT base
    pub idtr_base: u64,
    pub idtr_limit: u16,
    /// IA32_EFER MSR
    pub efer: u64,
    /// IA32_PAT MSR
    pub pat: u64,
}

static CPU_SAVE_STATE: Mutex<CpuSaveState> = Mutex::new(CpuSaveState {
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
    rip: 0,
    rflags: 0,
    cr0: 0,
    cr3: 0,
    cr4: 0,
    gdtr_base: 0,
    gdtr_limit: 0,
    idtr_base: 0,
    idtr_limit: 0,
    efer: 0,
    pat: 0,
});

// ── CPU state save/restore for S3 ──────────────────────────────────────

/// Save CPU state before entering S3
pub(super) fn save_cpu_state() {
    let mut save = CPU_SAVE_STATE.lock();

    // SAFETY: reading control registers and descriptor table registers
    unsafe {
        // Read control registers
        let mut cr0: u64 = 0;
        let mut cr3: u64 = 0;
        let mut cr4: u64 = 0;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("mov {}, cr0", out(reg) cr0);
        core::arch::asm!("mov {}, cr3", out(reg) cr3);
        core::arch::asm!("mov {}, cr4", out(reg) cr4);
        save.cr0 = cr0;
        save.cr3 = cr3;
        save.cr4 = cr4;

        // Read RFLAGS
        let mut rflags: u64 = 0;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("pushfq; pop {}", out(reg) rflags);
        save.rflags = rflags;

        // Read GDT descriptor
        let mut gdtr = [0u8; 10];
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("sgdt [{}]", in(reg) gdtr.as_mut_ptr());
        save.gdtr_limit = u16::from_le_bytes([gdtr[0], gdtr[1]]);
        save.gdtr_base = u64::from_le_bytes([
            gdtr[2], gdtr[3], gdtr[4], gdtr[5], gdtr[6], gdtr[7], gdtr[8], gdtr[9],
        ]);

        // Read IDT descriptor
        let mut idtr = [0u8; 10];
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("sidt [{}]", in(reg) idtr.as_mut_ptr());
        save.idtr_limit = u16::from_le_bytes([idtr[0], idtr[1]]);
        save.idtr_base = u64::from_le_bytes([
            idtr[2], idtr[3], idtr[4], idtr[5], idtr[6], idtr[7], idtr[8], idtr[9],
        ]);

        // Read IA32_EFER
        let mut efer_lo: u32 = 0;
        let mut efer_hi: u32 = 0;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "rdmsr",
            in("ecx") 0xC000_0080u32,
            out("eax") efer_lo,
            out("edx") efer_hi,
        );
        save.efer = ((efer_hi as u64) << 32) | (efer_lo as u64);
    }

    serial_println!(
        "[PM] CPU state saved (CR3={:#x}, GDT base={:#x})",
        save.cr3,
        save.gdtr_base
    );
}

/// Restore CPU state after S3 resume
pub(super) fn restore_cpu_state() {
    let save = CPU_SAVE_STATE.lock();

    // SAFETY: restoring control registers and descriptor tables to previously-saved values
    unsafe {
        // Restore GDT
        let mut gdtr = [0u8; 10];
        gdtr[0..2].copy_from_slice(&save.gdtr_limit.to_le_bytes());
        gdtr[2..10].copy_from_slice(&save.gdtr_base.to_le_bytes());
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("lgdt [{}]", in(reg) gdtr.as_ptr());

        // Restore IDT
        let mut idtr = [0u8; 10];
        idtr[0..2].copy_from_slice(&save.idtr_limit.to_le_bytes());
        idtr[2..10].copy_from_slice(&save.idtr_base.to_le_bytes());
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("lidt [{}]", in(reg) idtr.as_ptr());

        // Restore control registers
        core::arch::asm!("mov cr0, {}", in(reg) save.cr0);
        core::arch::asm!("mov cr3, {}", in(reg) save.cr3);
        core::arch::asm!("mov cr4, {}", in(reg) save.cr4);

        // Restore IA32_EFER
        let efer_lo = save.efer as u32;
        let efer_hi = (save.efer >> 32) as u32;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "wrmsr",
            in("ecx") 0xC000_0080u32,
            in("eax") efer_lo,
            in("edx") efer_hi,
        );
    }

    serial_println!("[PM] CPU state restored");
}

// ── CPU P-state control ─────────────────────────────────────────────────

/// Read current P-state ratio from IA32_PERF_STATUS MSR
pub(super) fn read_perf_status() -> u8 {
    unsafe {
        let mut lo: u32 = 0;
        let mut _hi: u32 = 0;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "rdmsr",
            in("ecx") MSR_IA32_PERF_STATUS,
            out("eax") lo,
            out("edx") _hi,
        );
        // Current P-state ratio is bits [15:8]
        ((lo >> 8) & 0xFF) as u8
    }
}

/// Write desired P-state ratio to IA32_PERF_CTL MSR
fn write_perf_ctl(ratio: u8) {
    unsafe {
        let val = (ratio as u32) << 8;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "wrmsr",
            in("ecx") MSR_IA32_PERF_CTL,
            in("eax") val,
            in("edx") 0u32,
        );
    }
}

/// Check if HWP (Hardware P-states, Intel Speed Shift) is supported
fn hwp_supported() -> bool {
    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
    if let Some(ext) = cpuid.get_extended_feature_info() {
        // HWP is CPUID.06H:EAX[bit 7] — but raw_cpuid doesn't expose this directly
        // Check via thermal/power leaf
    }
    // Simplified: check CPUID leaf 6, EAX bit 7
    let mut result: u32 = 0;
    unsafe {
        // ebx is reserved by LLVM, so save/restore it manually
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "push rbx",
            "cpuid",
            "pop rbx",
            inout("eax") 6u32 => result,
            out("ecx") _,
            out("edx") _,
        );
    }
    (result >> 7) & 1 == 1
}

/// Enable HWP if available
pub(super) fn enable_hwp() -> bool {
    if !hwp_supported() {
        return false;
    }
    unsafe {
        // Write 1 to IA32_PM_ENABLE
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "wrmsr",
            in("ecx") MSR_IA32_PM_ENABLE,
            in("eax") 1u32,
            in("edx") 0u32,
        );
    }
    serial_println!("[PM] HWP (Hardware P-states) enabled");
    true
}

/// Set HWP request (min/max/desired performance)
fn set_hwp_request(min_ratio: u8, max_ratio: u8, desired: u8) {
    let val = (min_ratio as u32) | ((max_ratio as u32) << 8) | ((desired as u32) << 16);
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "wrmsr",
            in("ecx") MSR_IA32_HWP_REQUEST,
            in("eax") val,
            in("edx") 0u32,
        );
    }
}

/// Set CPU frequency governor
pub fn set_governor(governor: CpuFreqGovernor) -> Result<(), i32> {
    let mut pm = PM.lock();
    let old_gov = pm.cpufreq.governor;
    pm.cpufreq.governor = governor;

    // Apply governor policy via P-state control
    match governor {
        CpuFreqGovernor::Performance => {
            let max_ratio = pm.cpufreq.max_ratio;
            if pm.cpufreq.hwp_active {
                set_hwp_request(max_ratio, max_ratio, max_ratio);
            } else {
                write_perf_ctl(max_ratio);
            }
            pm.cpufreq.cur_freq = pm.cpufreq.max_freq;
            pm.cpufreq.cur_ratio = max_ratio;
        }
        CpuFreqGovernor::Powersave => {
            let min_ratio = pm.cpufreq.min_ratio;
            if pm.cpufreq.hwp_active {
                set_hwp_request(min_ratio, min_ratio, min_ratio);
            } else {
                write_perf_ctl(min_ratio);
            }
            pm.cpufreq.cur_freq = pm.cpufreq.min_freq;
            pm.cpufreq.cur_ratio = min_ratio;
        }
        CpuFreqGovernor::Ondemand | CpuFreqGovernor::Schedutil | CpuFreqGovernor::Conservative => {
            // Dynamic governors: set range and let HWP/scheduler decide
            if pm.cpufreq.hwp_active {
                set_hwp_request(pm.cpufreq.min_ratio, pm.cpufreq.max_ratio, 0);
            }
            // For non-HWP: governor tick in scheduler adjusts P-state based on load
        }
        CpuFreqGovernor::Userspace => {
            // No automatic change — user sets frequency explicitly
        }
    }

    serial_println!(
        "[PM] CPU frequency governor: {} → {}",
        old_gov.as_str(),
        governor.as_str()
    );
    Ok(())
}

/// Get CPU frequency governor
pub fn get_governor() -> CpuFreqGovernor {
    let pm = PM.lock();
    pm.cpufreq.governor
}

/// Get current CPU frequency (KHz)
pub fn get_cpu_freq() -> u32 {
    let pm = PM.lock();
    pm.cpufreq.cur_freq
}

/// Set CPU frequency (userspace governor only), via P-state ratio
pub fn set_cpu_freq(freq_khz: u32) -> Result<(), i32> {
    let mut pm = PM.lock();
    if pm.cpufreq.governor != CpuFreqGovernor::Userspace {
        return Err(-1); // EPERM — can only set in userspace governor
    }
    if freq_khz < pm.cpufreq.min_freq || freq_khz > pm.cpufreq.max_freq {
        return Err(-22); // EINVAL
    }
    // Convert frequency to P-state ratio
    // ratio = freq_mhz / bus_freq_mhz (assume 100 MHz bus)
    let ratio = ((freq_khz / 1000) / 100) as u8;
    let ratio = ratio.max(pm.cpufreq.min_ratio).min(pm.cpufreq.max_ratio);

    if pm.cpufreq.hwp_active {
        set_hwp_request(ratio, ratio, ratio);
    } else {
        write_perf_ctl(ratio);
    }
    pm.cpufreq.cur_freq = freq_khz;
    pm.cpufreq.cur_ratio = ratio;
    Ok(())
}

/// Governor tick — called from scheduler timer to adjust frequency dynamically
pub fn governor_tick(cpu_load_percent: u8) {
    let mut pm = PM.lock();
    match pm.cpufreq.governor {
        CpuFreqGovernor::Ondemand => {
            // Jump to max if load > 80%, else scale proportionally to min
            if cpu_load_percent > 80 {
                pm.cpufreq.cur_ratio = pm.cpufreq.max_ratio;
            } else {
                let range = (pm.cpufreq.max_ratio - pm.cpufreq.min_ratio) as u32;
                let target = pm.cpufreq.min_ratio as u32 + (range * cpu_load_percent as u32 / 100);
                pm.cpufreq.cur_ratio = target as u8;
            }
        }
        CpuFreqGovernor::Conservative => {
            // Gradual ramp: ±5% at a time
            let step =
                ((pm.cpufreq.max_ratio - pm.cpufreq.min_ratio) as u32 * 5 / 100).max(1) as u8;
            if cpu_load_percent > 75 && pm.cpufreq.cur_ratio < pm.cpufreq.max_ratio {
                pm.cpufreq.cur_ratio = pm
                    .cpufreq
                    .cur_ratio
                    .saturating_add(step)
                    .min(pm.cpufreq.max_ratio);
            } else if cpu_load_percent < 25 && pm.cpufreq.cur_ratio > pm.cpufreq.min_ratio {
                pm.cpufreq.cur_ratio = pm
                    .cpufreq
                    .cur_ratio
                    .saturating_sub(step)
                    .max(pm.cpufreq.min_ratio);
            }
        }
        CpuFreqGovernor::Schedutil => {
            // Linear scaling based on load
            let range = (pm.cpufreq.max_ratio - pm.cpufreq.min_ratio) as u32;
            let target = pm.cpufreq.min_ratio as u32 + (range * cpu_load_percent as u32 / 100);
            pm.cpufreq.cur_ratio = target as u8;
        }
        _ => return, // Performance, Powersave, Userspace don't auto-adjust
    }

    let ratio = pm.cpufreq.cur_ratio;
    pm.cpufreq.cur_freq = (ratio as u32) * 100 * 1000; // ratio × 100 MHz

    if !pm.cpufreq.hwp_active {
        write_perf_ctl(ratio);
    }
}

// ── C-state idle ────────────────────────────────────────────────────────

/// Enter the deepest available C-state via MWAIT
pub fn cpu_idle_enter(target_residency_us: u32) -> u32 {
    let pm = PM.lock();

    // Select deepest C-state whose latency is acceptable
    let mut chosen_idx = 0;
    let mut chosen_hint = 0u32;
    for (i, cs) in pm.idle_states.iter().enumerate() {
        if cs.disabled {
            continue;
        }
        if cs.latency_us <= target_residency_us {
            chosen_idx = i;
            chosen_hint = cs.mwait_hint;
        }
    }

    drop(pm);

    // Enter C-state via MWAIT
    if chosen_hint > 0 {
        unsafe {
            // MONITOR + MWAIT sequence
            // MONITOR: set up address range to monitor (use a dummy location)
            let mut dummy: u64 = 0;
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!(
                "monitor",
                in("rax") &dummy as *const u64 as u64,
                in("ecx") 0u32,
                in("edx") 0u32,
            );
            // MWAIT: enter C-state
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!(
                "mwait",
                in("eax") chosen_hint,
                in("ecx") 0u32, // no break on interrupt flag
            );
        }
    } else {
        // Fallback: HLT
        crate::arch_compat::instructions::interrupts::hlt();
    }

    // Update statistics
    let mut pm = PM.lock();
    if let Some(cs) = pm.idle_states.get_mut(chosen_idx) {
        cs.usage += 1;
    }

    chosen_idx as u32
}
