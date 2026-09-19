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
    // The GDT is built once and its TSS descriptor must keep pointing at a
    // stable address, so the TSS lives in a `static` rather than here.
}

/// Stacks used by exception/IST entry. Fixed addresses for the kernel's life.
mod stacks {
    pub const DOUBLE_FAULT_SIZE: usize = 4096 * 5;
    pub const SYSCALL_SIZE: usize = 4096 * 8;

    #[repr(C, align(16))]
    pub struct Aligned<const N: usize>(pub [u8; N]);

    pub static mut DOUBLE_FAULT: Aligned<DOUBLE_FAULT_SIZE> = Aligned([0; DOUBLE_FAULT_SIZE]);
    pub static mut SYSCALL: Aligned<SYSCALL_SIZE> = Aligned([0; SYSCALL_SIZE]);

    pub fn double_fault_top() -> u64 {
        // SAFETY: address-of only; the array is never read or written as data.
        unsafe { core::ptr::addr_of_mut!(DOUBLE_FAULT.0) as u64 + DOUBLE_FAULT_SIZE as u64 }
    }

    pub fn syscall_top() -> u64 {
        // SAFETY: address-of only; the array is never read or written as data.
        unsafe { core::ptr::addr_of_mut!(SYSCALL.0) as u64 + SYSCALL_SIZE as u64 }
    }
}

/// The TSS. `RSP0` is rewritten on every switch to a user task because it is
/// the stack the CPU selects for Ring 3 → Ring 0 transitions (hardware IRQs
/// and exceptions). `syscall` entry uses `gs:[8]`, kept in step by
/// [`crate::usermode::set_kernel_stack`].
struct TssStorage(core::cell::UnsafeCell<TaskStateSegment>);

// SAFETY: `RSP0` is written from kernel context before the owning task runs,
// and read by the CPU on privilege transitions. Every other field is written
// once by `init()` before `load_tss` and never mutated afterwards.
unsafe impl Sync for TssStorage {}

static TSS: TssStorage = TssStorage(core::cell::UnsafeCell::new(TaskStateSegment::new()));

/// Shared reference to the one TSS.
fn tss() -> &'static TaskStateSegment {
    // SAFETY: see the `Sync` impl above; callers only read, except through
    // `set_privilege_stack_top` which is the documented writer.
    unsafe { &*TSS.0.get() }
}

/// Install the exception/IST stacks. Must run before the TSS is loaded and
/// before any Ring 3 task exists.
fn init_ist_stacks() {
    let tss = unsafe { &mut *TSS.0.get() };
    tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] =
        VirtAddr::new(stacks::double_fault_top());
    tss.interrupt_stack_table[SYSCALL_IST_INDEX as usize] = VirtAddr::new(stacks::syscall_top());
    tss.privilege_stack_table[0] = VirtAddr::new(stacks::syscall_top());
}

/// RSP0 used on Ring 3 → Ring 0 privilege change (and the initial syscall stack).
pub fn privilege_stack_top() -> u64 {
    tss().privilege_stack_table[0].as_u64()
}

/// Point future Ring 3 → Ring 0 transitions at `top`.
///
/// # Safety
/// `top` must be the top of a mapped, writable stack owned by the task that is
/// about to run in Ring 3.
pub unsafe fn set_privilege_stack_top(top: u64) {
    let tss = &mut *TSS.0.get();
    tss.privilege_stack_table[0] = VirtAddr::new(top);
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
        let tss_selector = gdt.append(Descriptor::tss_segment(tss()));
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

    init_ist_stacks();

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
    crate::serial_println!(
        "[KnoxOS] TSS: RSP0={:#x} (rewritten per user task), DF-IST={:#x}",
        privilege_stack_top(),
        tss().interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize].as_u64()
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
