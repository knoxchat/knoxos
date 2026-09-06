#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::VirtAddr;
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
#[cfg(target_arch = "x86_64")]
use crate::arch_compat::structures::paging::VirtAddr;
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::structures::tss::TaskStateSegment;
/// GDT - Global Descriptor Table
/// Sets up segmentation and TSS for interrupt handling
/// Includes kernel (ring 0) and user (ring 3) segments for syscall/sysret
use lazy_static::lazy_static;
#[cfg(target_arch = "x86_64")]
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
#[cfg(target_arch = "x86_64")]
use x86_64::structures::tss::TaskStateSegment;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;
pub const SYSCALL_IST_INDEX: u16 = 1;

lazy_static! {
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();
        // Double fault stack (IST index 0)
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            const STACK_SIZE: usize = 4096 * 5;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
            let stack_start = VirtAddr::from_ptr(&raw const STACK);
            stack_start + STACK_SIZE as u64
        };
        // Syscall handler stack (IST index 1) - used for kernel entry from ring 3
        tss.interrupt_stack_table[SYSCALL_IST_INDEX as usize] = {
            const STACK_SIZE: usize = 4096 * 8;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
            let stack_start = VirtAddr::from_ptr(&raw const STACK);
            stack_start + STACK_SIZE as u64
        };
        // Privilege stack table - RSP0 is used when transitioning from ring 3 to ring 0
        tss.privilege_stack_table[0] = {
            const STACK_SIZE: usize = 4096 * 8;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
            let stack_start = VirtAddr::from_ptr(&raw const STACK);
            stack_start + STACK_SIZE as u64
        };
        tss
    };
}

/// RSP0 used on Ring 3 → Ring 0 privilege change (and as syscall kernel stack).
pub fn privilege_stack_top() -> u64 {
    TSS.privilege_stack_table[0].as_u64()
}

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();
        // Segment order matters for syscall/sysret:
        // Index 1: Kernel Code (0x08) - Ring 0
        let code_selector = gdt.append(Descriptor::kernel_code_segment());
        // Index 2: Kernel Data (0x10) - Ring 0
        let data_selector = gdt.append(Descriptor::kernel_data_segment());
        // Index 3: User Data (0x1B) - Ring 3 (must come before user code for sysret)
        let user_data_selector = gdt.append(Descriptor::user_data_segment());
        // Index 4: User Code (0x23) - Ring 3
        let user_code_selector = gdt.append(Descriptor::user_code_segment());
        // Index 5-6: TSS (takes 2 entries for 64-bit TSS)
        let tss_selector = gdt.append(Descriptor::tss_segment(&TSS));
        (
            gdt,
            Selectors {
                code_selector,
                data_selector,
                user_code_selector,
                user_data_selector,
                tss_selector,
            },
        )
    };
}

pub struct Selectors {
    pub code_selector: SegmentSelector,
    pub data_selector: SegmentSelector,
    pub user_code_selector: SegmentSelector,
    pub user_data_selector: SegmentSelector,
    pub tss_selector: SegmentSelector,
}

/// Get kernel code segment selector
pub fn kernel_code_selector() -> SegmentSelector {
    GDT.1.code_selector
}

/// Get kernel data segment selector
pub fn kernel_data_selector() -> SegmentSelector {
    GDT.1.data_selector
}

/// Get user code segment selector (ring 3)
pub fn user_code_selector() -> SegmentSelector {
    GDT.1.user_code_selector
}

/// Get user data segment selector (ring 3)
pub fn user_data_selector() -> SegmentSelector {
    GDT.1.user_data_selector
}

pub fn init() {
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::instructions::segmentation::{CS, DS, Segment};
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::instructions::tables::load_tss;
    #[cfg(target_arch = "x86_64")]
    use x86_64::instructions::segmentation::{CS, DS, Segment};
    #[cfg(target_arch = "x86_64")]
    use x86_64::instructions::tables::load_tss;

    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.code_selector);
        DS::set_reg(GDT.1.data_selector);
        load_tss(GDT.1.tss_selector);
    }
    crate::serial_println!(
        "[KnoxOS] GDT: kernel CS={:#x}, DS={:#x}, user CS={:#x}, DS={:#x}",
        GDT.1.code_selector.0,
        GDT.1.data_selector.0,
        GDT.1.user_code_selector.0,
        GDT.1.user_data_selector.0
    );
}

/// Initialize GDT on an Application Processor.
/// Loads the same GDT and sets CS/DS, but does NOT load the TSS
/// (TSS is per-CPU; the shared TSS has its Busy bit set by the BSP).
pub fn init_ap() {
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::instructions::segmentation::{CS, DS, Segment};
    #[cfg(target_arch = "x86_64")]
    use x86_64::instructions::segmentation::{CS, DS, Segment};

    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.code_selector);
        DS::set_reg(GDT.1.data_selector);
        // Note: TSS not loaded — AP uses the kernel segments only.
        // A per-AP TSS should be created for IST-based exception handling.
    }
}
