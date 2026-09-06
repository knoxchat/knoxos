#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::structures::idt::{
    InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode,
};
/// IDT - Interrupt Descriptor Table & Hardware Interrupt Handling
/// Handles CPU exceptions, keyboard, mouse, timer interrupts

#[cfg(target_arch = "x86_64")]
use crate::{gdt, hlt_loop, serial_println};
#[cfg(target_arch = "x86_64")]
use lazy_static::lazy_static;
#[cfg(target_arch = "x86_64")]
use pic8259::ChainedPics;
#[cfg(target_arch = "x86_64")]
use spin::Mutex;
#[cfg(target_arch = "x86_64")]
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

#[cfg(target_arch = "x86_64")]
pub const PIC_1_OFFSET: u8 = 32;
#[cfg(target_arch = "x86_64")]
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

#[cfg(target_arch = "x86_64")]
pub static PICS: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

#[cfg(target_arch = "x86_64")]
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard,
    Cascade,
    COM2,
    COM1,
    LPT2,
    Floppy,
    LPT1,
    RTC,
    Free1,
    Free2,
    Free3,
    Mouse,
    FPU,
    PrimaryATA,
    SecondaryATA,
}

#[cfg(target_arch = "x86_64")]
impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }
}

#[cfg(target_arch = "x86_64")]
lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        // CPU Exception handlers
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }
        idt.page_fault.set_handler_fn(page_fault_handler);
        idt.general_protection_fault
            .set_handler_fn(general_protection_fault_handler);
        idt.invalid_opcode.set_handler_fn(invalid_opcode_handler);
        idt.stack_segment_fault
            .set_handler_fn(stack_segment_fault_handler);

        // Hardware interrupt handlers
        idt[InterruptIndex::Timer.as_u8()].set_handler_fn(timer_interrupt_handler);
        idt[InterruptIndex::Keyboard.as_u8()].set_handler_fn(keyboard_interrupt_handler);
        idt[InterruptIndex::Mouse.as_u8()].set_handler_fn(mouse_interrupt_handler);

        // Virtio device interrupts (IRQ 9, 10, 11 — shared PCI IRQ lines)
        idt[InterruptIndex::Free1.as_u8()].set_handler_fn(virtio_irq_handler);
        idt[InterruptIndex::Free2.as_u8()].set_handler_fn(virtio_irq_handler);
        idt[InterruptIndex::Free3.as_u8()].set_handler_fn(virtio_irq_handler);

        // ATA disk interrupts
        idt[InterruptIndex::PrimaryATA.as_u8()].set_handler_fn(ata_irq_handler);
        idt[InterruptIndex::SecondaryATA.as_u8()].set_handler_fn(ata_irq_handler);

        // APIC timer vector (0x40 = 64) — per-core preemption timer
        idt[crate::apic_timer::TIMER_VECTOR].set_handler_fn(apic_timer_interrupt_handler);

        idt
    };
}

#[cfg(target_arch = "x86_64")]
pub fn init_idt() {
    IDT.load();
}

#[cfg(not(target_arch = "x86_64"))]
pub fn init_idt() {
    // No IDT on non-x86_64 architectures
}

// ─── CPU Exception Handlers ─────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    serial_println!("[EXCEPTION] BREAKPOINT\n{:#?}", stack_frame);
}

#[cfg(target_arch = "x86_64")]
extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    serial_println!("[EXCEPTION] DOUBLE FAULT\n{:#?}", stack_frame);
    panic!("DOUBLE FAULT\n{:#?}", stack_frame);
}

#[cfg(target_arch = "x86_64")]
extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    #[cfg(target_arch = "x86_64")]
    use crate::arch_compat::registers::control::Cr2;
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::registers::control::Cr2;
    let fault_addr =
        Cr2::read().unwrap_or(crate::arch_compat::structures::paging::VirtAddr::zero());
    let is_write = error_code.contains(PageFaultErrorCode::CAUSED_BY_WRITE);
    let is_user = error_code.contains(PageFaultErrorCode::USER_MODE);

    // Check stack guard page first — immediately report stack overflow
    if crate::stack_guard::is_guard_page_fault(fault_addr.as_u64()) {
        crate::stack_guard::handle_stack_overflow(fault_addr.as_u64());
        // If it was a user process, the handler will have killed it;
        // for kernel stacks, handle_stack_overflow panics.
        return;
    }

    // Try VMM CoW / guard-page recovery for processes with address spaces
    let pid = crate::context::current_pid();
    let has_as = crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .map(|p| p.has_address_space)
        .unwrap_or(false);

    if has_as && crate::vmm::handle_page_fault(pid, fault_addr.as_u64(), is_write) {
        // VMM handled it (CoW, guard page, etc.) — resume
        return;
    }

    // VMM could not handle — fatal fault
    serial_println!("[EXCEPTION] PAGE FAULT (unrecoverable)");
    serial_println!("  Address: {:?}", fault_addr);
    serial_println!(
        "  Error Code: {:?} (write={}, user={})",
        error_code,
        is_write,
        is_user
    );
    serial_println!("  PID: {}", pid);
    serial_println!("{:#?}", stack_frame);

    if is_user && pid > 2 {
        // Kill the user process instead of halting the kernel
        serial_println!(
            "[page_fault] Killing PID {} due to segfault at {:?}",
            pid,
            fault_addr
        );
        let _ = crate::signals::kill(pid, crate::signals::Signal::SIGSEGV, 0);
        crate::process::destroy_process(pid);
    } else {
        hlt_loop();
    }
}

#[cfg(target_arch = "x86_64")]
extern "x86-interrupt" fn general_protection_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    serial_println!(
        "[EXCEPTION] GENERAL PROTECTION FAULT (error: {})\n{:#?}",
        error_code,
        stack_frame
    );

    // If this was a user-mode process, kill it gracefully instead of halting
    let pid = crate::context::current_pid();
    if pid > 2 {
        // Check if it's a user process
        let is_user = crate::process::PROCESS_TABLE
            .lock()
            .get_process(pid)
            .map(|p| p.has_address_space)
            .unwrap_or(false);
        if is_user {
            serial_println!(
                "[GPF] Killing user PID {} due to GPF (error={})",
                pid,
                error_code
            );
            let _ = crate::signals::kill(pid, crate::signals::Signal::SIGSEGV, 0);
            crate::process::destroy_process(pid);
            return;
        }
    }
    hlt_loop();
}

#[cfg(target_arch = "x86_64")]
extern "x86-interrupt" fn invalid_opcode_handler(stack_frame: InterruptStackFrame) {
    serial_println!("[EXCEPTION] INVALID OPCODE\n{:#?}", stack_frame);

    // Kill user processes gracefully
    let pid = crate::context::current_pid();
    if pid > 2 {
        let is_user = crate::process::PROCESS_TABLE
            .lock()
            .get_process(pid)
            .map(|p| p.has_address_space)
            .unwrap_or(false);
        if is_user {
            serial_println!("[UD] Killing user PID {} due to invalid opcode", pid);
            let _ = crate::signals::kill(pid, crate::signals::Signal::SIGILL, 0);
            crate::process::destroy_process(pid);
            return;
        }
    }
    hlt_loop();
}

#[cfg(target_arch = "x86_64")]
extern "x86-interrupt" fn stack_segment_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    serial_println!(
        "[EXCEPTION] STACK SEGMENT FAULT (error: {})\n{:#?}",
        error_code,
        stack_frame
    );

    // Kill user processes gracefully
    let pid = crate::context::current_pid();
    if pid > 2 {
        let is_user = crate::process::PROCESS_TABLE
            .lock()
            .get_process(pid)
            .map(|p| p.has_address_space)
            .unwrap_or(false);
        if is_user {
            serial_println!("[SS] Killing user PID {} due to stack segment fault", pid);
            let _ = crate::signals::kill(pid, crate::signals::Signal::SIGSEGV, 0);
            crate::process::destroy_process(pid);
            return;
        }
    }
    hlt_loop();
}

// ─── Hardware Interrupt Handlers ─────────────────────────────────────

/// Timer tick counter for scheduling
static TICK_COUNT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Get current tick count (lock-free)
pub fn get_ticks() -> u64 {
    TICK_COUNT.load(core::sync::atomic::Ordering::Relaxed)
}

/// Increment the global tick counter (called from APIC timer handler
/// when the PIT is no longer delivering IRQ0).
pub fn increment_ticks() {
    TICK_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
}

#[cfg(target_arch = "x86_64")]
extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // When the APIC timer is running (100Hz), it handles TICK_COUNT,
    // RTC ticks, cursor redraws, and scheduler ticks. The PIT handler
    // must NOT duplicate that work — just send EOI and return.
    if crate::apic_timer::is_initialized() {
        if crate::smp::is_apic_mode() {
            crate::smp::eoi();
        } else {
            unsafe {
                PICS.lock()
                    .notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
            }
        }
        return;
    }

    let ticks = TICK_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed) + 1;

    // Update RTC monotonic counter (atomic, no locks)
    crate::rtc::tick();

    if ticks % 6 == 0 {
        crate::gui::request_cursor_redraw();
    }

    crate::scheduler::isr_timer_tick();

    if crate::smp::is_apic_mode() {
        crate::smp::eoi();
    } else {
        unsafe {
            PICS.lock()
                .notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
        }
    }
}

#[cfg(target_arch = "x86_64")]
extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    #[cfg(target_arch = "x86_64")]
    use crate::arch_compat::instructions::port::Port;
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::instructions::port::Port;

    let mut port = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };

    crate::task::keyboard::add_scancode(scancode);

    if crate::smp::is_apic_mode() {
        crate::smp::eoi();
    } else {
        unsafe {
            PICS.lock()
                .notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
        }
    }
}

#[cfg(target_arch = "x86_64")]
extern "x86-interrupt" fn mouse_interrupt_handler(_stack_frame: InterruptStackFrame) {
    #[cfg(target_arch = "x86_64")]
    use crate::arch_compat::instructions::port::Port;
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::instructions::port::Port;

    let mut port = Port::new(0x60);
    let data: u8 = unsafe { port.read() };

    crate::gui::input::add_mouse_byte(data);

    if crate::smp::is_apic_mode() {
        crate::smp::eoi();
    } else {
        unsafe {
            PICS.lock()
                .notify_end_of_interrupt(InterruptIndex::Mouse.as_u8());
        }
    }
}

// ─── Virtio IRQ Handler (IRQ 9 / 10 / 11 — shared PCI lines) ───────

/// Global counter of virtio interrupts serviced (for diagnostics)
static VIRTIO_IRQ_COUNT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

pub fn virtio_irq_count() -> u64 {
    VIRTIO_IRQ_COUNT.load(core::sync::atomic::Ordering::Relaxed)
}

#[cfg(target_arch = "x86_64")]
extern "x86-interrupt" fn virtio_irq_handler(_stack_frame: InterruptStackFrame) {
    VIRTIO_IRQ_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

    // Notify both virtio-blk and virtio-net — on shared PCI IRQ lines
    // we can't know which device fired, so we signal both and let them
    // check their own ISR registers.
    crate::virtio_blk::handle_interrupt();
    crate::virtio_net::handle_interrupt();

    // EOI — all three vectors (Free1/2/3) map to PIC2 so any of their
    // vector numbers works for the EOI.
    if crate::smp::is_apic_mode() {
        crate::smp::eoi();
    } else {
        unsafe {
            PICS.lock()
                .notify_end_of_interrupt(InterruptIndex::Free1.as_u8());
        }
    }
}

// ─── APIC Timer IRQ Handler (vector 0x40) ────────────────────────────

#[cfg(target_arch = "x86_64")]
extern "x86-interrupt" fn apic_timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    crate::apic_timer::handle_interrupt();
}

// ─── ATA Disk IRQ Handler (IRQ 14 / 15) ─────────────────────────────

static ATA_IRQ_COUNT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

pub fn ata_irq_count() -> u64 {
    ATA_IRQ_COUNT.load(core::sync::atomic::Ordering::Relaxed)
}

#[cfg(target_arch = "x86_64")]
extern "x86-interrupt" fn ata_irq_handler(_stack_frame: InterruptStackFrame) {
    ATA_IRQ_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

    // Read the ATA status register to acknowledge the interrupt
    // Primary: 0x1F7, Secondary: 0x177
    unsafe {
        #[cfg(target_arch = "x86_64")]
        use crate::arch_compat::instructions::port::Port;
        #[cfg(not(target_arch = "x86_64"))]
        use crate::arch_compat::instructions::port::Port;
        let mut primary_status: Port<u8> = Port::new(0x1F7);
        let _ = primary_status.read();
        let mut secondary_status: Port<u8> = Port::new(0x177);
        let _ = secondary_status.read();
    }

    // Signal any waiters in the AHCI / block layer
    crate::ahci::handle_ata_interrupt();

    if crate::smp::is_apic_mode() {
        crate::smp::eoi();
    } else {
        unsafe {
            PICS.lock()
                .notify_end_of_interrupt(InterruptIndex::PrimaryATA.as_u8());
        }
    }
}
