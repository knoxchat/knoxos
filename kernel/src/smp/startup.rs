use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::serial_println;

use super::idle::ap_idle_loop;
use super::lapic::{init_lapic, init_timer, lapic_write, pit_wait_10ms, wait_icr_idle};
use super::regs::*;
use super::state::{CPU_DATA, CPUS_STARTED, MAX_CPUS, PHYS_OFFSET, PerCpuData, online_cpus};

// ─── SMP — Application Processor Startup ────────────────────────────────

/// AP trampoline code location (must be < 1MB, page-aligned)
pub const AP_TRAMPOLINE_ADDR: u64 = 0x8000;

/// AP trampoline data area (parameters passed from BSP to AP)
pub const AP_DATA_ADDR: u64 = 0x7000;

/// AP trampoline stack base (each AP gets its own stack)
pub const AP_STACK_SIZE: usize = 16 * 1024; // 16KB stack per AP
pub const AP_STACK_BASE: u64 = 0x0000_0000_0080_0000; // 8MB mark

/// Flag set by AP to signal it's alive
static AP_ALIVE: AtomicBool = AtomicBool::new(false);
/// `str` value the first AP loaded (proves a distinct TSS, not the BSP's).
static AP_TSS_SEL: AtomicU32 = AtomicU32::new(0);

/// Real-mode → protected-mode → long-mode AP trampoline code
/// This machine code is copied to AP_TRAMPOLINE_ADDR (below 1MB)
/// The AP starts executing in 16-bit real mode at this address.
///
/// Layout at AP_DATA_ADDR (0x7000):
///   0x7000: u32 — cr3 (page table base, from BSP, low 32 bits)
///   0x7004: u32 — reserved (zero)
///   0x7008: u64 — 64-bit entry point (ap_entry_64)
///   0x7010: u64 — stack top for this AP
///   0x7018: u32 — APIC ID of this AP
///   0x701C: u32 — reserved (zero)
///   0x7020: GDT pointer (6 bytes: limit + base)
///   0x7026: GDT entries (null, code32, data32, code64, data64)
///   0x704E: far pointer for 16→32 jump (6 bytes: offset32 + selector16)
///   0x7054: far pointer for 32→64 jump (6 bytes: offset32 + selector16)
///
/// Trampoline byte layout (base = 0x8000):
///   Offset 0x00: 16-bit real-mode entry
///   Offset 0x22: 32-bit protected-mode code
///   Offset 0x6E: 64-bit long-mode code
///
/// Debug markers output to serial port 0x3F8:
///   'A' = AP entered real mode
///   'B' = AP entered protected mode
///   'C' = AP about to enable paging+longmode
///   'D' = AP reached 64-bit mode
///
/// IMPORTANT: In 32-bit and 64-bit modes, `mov edx, imm` (opcode 0xBA)
/// takes a 32-bit immediate (5 bytes total), NOT 16-bit.  All serial port
/// address loads must use the full `BA F8 03 00 00` encoding.
static AP_TRAMPOLINE_CODE: &[u8] = &[
    // ═══ 16-bit real mode (offset 0x00) ═══
    // [0x00]
    0xFA, //  cli
    // [0x01]
    0x31, 0xC0, //  xor ax, ax  (in 16-bit mode this is ax)
    // [0x03]
    0x8E, 0xD8, //  mov ds, ax
    // [0x05]
    0x8E, 0xC0, //  mov es, ax
    // [0x07]
    0x8E, 0xD0, //  mov ss, ax
    // [0x09]  Output 'A' to serial port 0x3F8
    0xB0, 0x41, //  mov al, 'A'
    0xBA, 0xF8, 0x03, //  mov dx, 0x3F8  (16-bit: BA = mov dx, imm16)
    0xEE, //  out dx, al
    // [0x0F]  Load GDT from AP_DATA_ADDR + 0x20 = 0x7020
    0x0F, 0x01, 0x16, 0x20, 0x70, //  lgdt [0x7020]
    // [0x14]  Enable protected mode: CR0.PE = 1
    0x0F, 0x20, 0xC0, //  mov eax, cr0
    // [0x17]
    0x0C, 0x01, //  or al, 1
    // [0x19]
    0x0F, 0x22, 0xC0, //  mov cr0, eax
    // [0x1C]  Indirect far jump to 32-bit code via pointer at [0x704E]
    //         The far pointer (offset32 + selector16) is written by
    //         install_trampoline() — safer than inline 66 EA in real mode.
    0x66, 0xFF, 0x2E, 0x4E, 0x70, //  jmp far [0x704E]  (o32 in 16-bit mode)
    // [0x21]  nop padding (not reached)
    0x90,
    // ═══ 32-bit protected mode (offset 0x22) ═══
    // [0x22]  Load data segments
    0x66, 0xB8, 0x10, 0x00, //  mov ax, 0x10  (data32 segment selector)
    // [0x26]
    0x8E, 0xD8, //  mov ds, ax
    // [0x28]
    0x8E, 0xC0, //  mov es, ax
    // [0x2A]
    0x8E, 0xD0, //  mov ss, ax
    // [0x2C]  Output 'B' to serial port 0x3F8
    //         NOTE: In 32-bit mode, BA = mov edx, imm32 (5 bytes, NOT 3!)
    0xB0, 0x42, //  mov al, 'B'
    0xBA, 0xF8, 0x03, 0x00, 0x00, //  mov edx, 0x000003F8
    // [0x33]
    0xEE, //  out dx, al
    // [0x34]  Enable PAE (CR4 bit 5)
    0x0F, 0x20, 0xE0, //  mov eax, cr4
    // [0x37]
    0x0D, 0x20, 0x00, 0x00, 0x00, //  or eax, 0x20
    // [0x3C]
    0x0F, 0x22, 0xE0, //  mov cr4, eax
    // [0x3F]  Load CR3 from [0x7000] (page table base)
    0xA1, 0x00, 0x70, 0x00, 0x00, //  mov eax, [0x7000]
    // [0x44]
    0x0F, 0x22, 0xD8, //  mov cr3, eax
    // [0x47]  Enable long mode via MSR IA32_EFER (0xC0000080), bit 8 (LME)
    //         Also enable NXE (bit 11) so the NX bit in page tables is valid.
    //         Without NXE, bit 63 in PTEs is "reserved" and causes #PF.
    0xB9, 0x80, 0x00, 0x00, 0xC0, //  mov ecx, 0xC0000080
    // [0x4C]
    0x0F, 0x32, //  rdmsr
    // [0x4E]
    0x0D, 0x00, 0x09, 0x00, 0x00, //  or eax, 0x900  (LME=bit8 | NXE=bit11)
    // [0x53]
    0x0F, 0x30, //  wrmsr
    // [0x55]  Output 'C' to serial port
    0xB0, 0x43, //  mov al, 'C'
    0xBA, 0xF8, 0x03, 0x00, 0x00, //  mov edx, 0x000003F8
    // [0x5C]
    0xEE, //  out dx, al
    // [0x5D]  Enable paging: CR0.PG = bit 31
    0x0F, 0x20, 0xC0, //  mov eax, cr0
    // [0x60]
    0x0D, 0x00, 0x00, 0x00, 0x80, //  or eax, 0x80000000
    // [0x65]
    0x0F, 0x22, 0xC0, //  mov cr0, eax
    // [0x68]  Indirect far jump to 64-bit code via pointer at [0x7054]
    //         Far pointer written by install_trampoline()
    0xFF, 0x2D, 0x54, 0x70, 0x00, 0x00, //  jmp far [0x7054]  (32-bit indirect)
    // ═══ 64-bit long mode (offset 0x6E) ═══
    // [0x6E]  Set up 64-bit data segments
    0x66, 0xB8, 0x20, 0x00, //  mov ax, 0x20  (data64 segment selector)
    // [0x72]
    0x8E, 0xD8, //  mov ds, ax
    // [0x74]
    0x8E, 0xC0, //  mov es, ax
    // [0x76]
    0x8E, 0xD0, //  mov ss, ax
    // [0x78]  Output 'D' to serial port
    //         NOTE: In 64-bit mode, BA = mov edx, imm32 (5 bytes, NOT 3!)
    0xB0, 0x44, //  mov al, 'D'
    0xBA, 0xF8, 0x03, 0x00, 0x00, //  mov edx, 0x000003F8
    // [0x7F]
    0xEE, //  out dx, al
    // [0x80]  Load stack from [0x7010]
    0x48, 0x8B, 0x24, 0x25, 0x10, 0x70, 0x00, 0x00, //  mov rsp, qword [0x7010]
    // [0x88]  Load 64-bit entry point from [0x7008]
    0x48, 0x8B, 0x04, 0x25, 0x08, 0x70, 0x00, 0x00, //  mov rax, qword [0x7008]
    // [0x90]  Load APIC ID argument from [0x7018]
    0x8B, 0x3C, 0x25, 0x18, 0x70, 0x00, 0x00, //  mov edi, dword [0x7018]
    // [0x97]  Jump to 64-bit Rust entry point
    0xFF, 0xE0, //  jmp rax
    // [0x99]  Halt fallback
    0xF4, //  hlt
    0xEB, 0xFD, //  jmp $-1
];

/// Set up the AP trampoline GDT at AP_DATA_ADDR + 0x20
/// GDT layout: null(0), code32(0x08), data32(0x10), code64(0x18), data64(0x20)
fn setup_trampoline_gdt(phys_offset: u64) {
    unsafe {
        let gdt_base = (phys_offset + AP_DATA_ADDR + 0x26) as *mut u64;
        let gdtr = (phys_offset + AP_DATA_ADDR + 0x20) as *mut u8;

        // GDT entries
        let gdt_entries: [u64; 5] = [
            0x0000_0000_0000_0000, // Null descriptor
            0x00CF_9A00_0000_FFFF, // 32-bit code segment (base=0, limit=4G, DPL=0, exec/read)
            0x00CF_9200_0000_FFFF, // 32-bit data segment (base=0, limit=4G, DPL=0, read/write)
            0x00AF_9A00_0000_FFFF, // 64-bit code segment (L=1, D=0, DPL=0, exec/read)
            0x00CF_9200_0000_FFFF, // 64-bit data segment (base=0, limit=4G, DPL=0, read/write)
        ];

        for (i, entry) in gdt_entries.iter().enumerate() {
            core::ptr::write_volatile(gdt_base.add(i), *entry);
        }

        // GDTR: 2-byte limit + 4-byte base (use 32-bit base for real/protected mode)
        let limit: u16 = (gdt_entries.len() * 8 - 1) as u16;
        core::ptr::write_volatile(gdtr as *mut u16, limit);
        let gdt_addr = (AP_DATA_ADDR + 0x26) as u32;
        core::ptr::write_volatile(gdtr.add(2) as *mut u32, gdt_addr);
    }
}

/// Identity-map the first 2 MiB of physical memory so the AP trampoline
/// code (running at physical addresses 0x7000-0x8FFF) can access itself
/// and its data area after paging is enabled with the BSP's CR3.
///
/// We walk the 4-level page table manually and insert a single 2 MiB
/// huge page entry (PDE with PS=1) mapping virtual 0..0x200000 →
/// physical 0..0x200000.
///
/// This is safe because:
///   - The first 2 MiB is typically unused in the virtual address space
///   - We only need it during AP startup
pub(crate) fn identity_map_low_memory(phys_offset: u64, cr3: u64) {
    unsafe {
        let pml4_phys = cr3 & !0xFFF; // strip flags
        let pml4 = (phys_offset + pml4_phys) as *mut u64;

        // PML4 entry 0 (covers virtual 0..512 GiB)
        let mut pml4e = core::ptr::read_volatile(pml4);
        let pdpt_phys;
        if pml4e & 1 != 0 {
            // Already present — use existing PDPT
            pdpt_phys = pml4e & 0x000F_FFFF_FFFF_F000;
        } else {
            // We need to allocate a PDPT frame. Use a fixed safe physical
            // address in low memory that we know is free (0x6000).
            // This is below our data area at 0x7000.
            pdpt_phys = 0x6000;
            // Zero the new page table
            let pdpt = (phys_offset + pdpt_phys) as *mut u8;
            core::ptr::write_bytes(pdpt, 0, 4096);
            // Present + Writable
            pml4e = pdpt_phys | 0x03;
            core::ptr::write_volatile(pml4, pml4e);
        }

        let pdpt = (phys_offset + pdpt_phys) as *mut u64;

        // PDPT entry 0 (covers virtual 0..1 GiB)
        let mut pdpte = core::ptr::read_volatile(pdpt);
        let pd_phys;
        if pdpte & 1 != 0 {
            if pdpte & 0x80 != 0 {
                // 1 GiB huge page already present — low memory is identity-mapped
                serial_println!("[SMP] Low memory already identity-mapped (1G page)");
                return;
            }
            pd_phys = pdpte & 0x000F_FFFF_FFFF_F000;
        } else {
            // Allocate PD at 0x5000
            pd_phys = 0x5000;
            let pd = (phys_offset + pd_phys) as *mut u8;
            core::ptr::write_bytes(pd, 0, 4096);
            pdpte = pd_phys | 0x03; // Present + Writable
            core::ptr::write_volatile(pdpt, pdpte);
        }

        let pd = (phys_offset + pd_phys) as *mut u64;

        // PD entry 0: map virtual 0..2 MiB → physical 0..2 MiB as a 2 MiB huge page
        let pde = core::ptr::read_volatile(pd);
        if pde & 0x83 == 0x83 {
            // Already a 2 MiB huge page at physical 0 — check it maps to phys 0
            let mapped_phys = pde & 0x000F_FFFF_FFE0_0000;
            if mapped_phys == 0 {
                serial_println!("[SMP] Low memory already identity-mapped (2M page)");
                return;
            }
        }

        // Either not present, or not a correct identity map.
        // (Over)write PDE entry 0 with a 2 MiB identity-map page.
        // 2 MiB page: Present(1) + Writable(2) + PageSize(0x80) = 0x83
        // Physical address = 0 (mapping physical 0..0x200000)
        core::ptr::write_volatile(pd, 0x0000_0000_0000_0083u64);

        // Flush TLB for the region
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("invlpg [{}]", in(reg) 0u64, options(nostack, preserves_flags));

        serial_println!("[SMP] Identity-mapped first 2 MiB for AP trampoline");
    }
}

/// Install AP trampoline code and data into low memory
pub fn install_trampoline(phys_offset: u64, cr3: u64) {
    unsafe {
        // Zero the entire data area first to avoid stale data
        let data_base_ptr = (phys_offset + AP_DATA_ADDR) as *mut u8;
        core::ptr::write_bytes(data_base_ptr, 0, 0x5E); // zero 0x7000..0x705D

        // Copy trampoline code to AP_TRAMPOLINE_ADDR
        let trampoline_dest = (phys_offset + AP_TRAMPOLINE_ADDR) as *mut u8;
        core::ptr::copy_nonoverlapping(
            AP_TRAMPOLINE_CODE.as_ptr(),
            trampoline_dest,
            AP_TRAMPOLINE_CODE.len(),
        );

        // Set up data area at AP_DATA_ADDR
        let data_base = phys_offset + AP_DATA_ADDR;
        // CR3 at offset 0x00 (32-bit — page table must be below 4GB)
        assert!(
            cr3 <= 0xFFFF_FFFF,
            "AP trampoline: CR3 {:#x} exceeds 32-bit range",
            cr3
        );
        core::ptr::write_volatile(data_base as *mut u32, cr3 as u32);
        // Entry point at offset 0x08 (set per-AP before SIPI)
        core::ptr::write_volatile(
            (data_base + 0x08) as *mut u64,
            ap_entry_64 as *const () as u64,
        );

        // Set up GDT
        setup_trampoline_gdt(phys_offset);

        // Set up far jump pointer for 16→32 bit transition at 0x704E
        // Format: offset32 (4 bytes) + selector16 (2 bytes)
        // Target: offset 0x22 within trampoline → physical 0x8022
        // Selector: 0x0008 (code32 GDT entry)
        core::ptr::write_volatile((data_base + 0x4E) as *mut u32, 0x0000_8022u32);
        core::ptr::write_volatile((data_base + 0x52) as *mut u16, 0x0008u16);

        // Set up far jump pointer for 32→64 bit transition at 0x7054
        // Format: offset32 (4 bytes) + selector16 (2 bytes)
        // Target: offset 0x6E within trampoline → physical 0x806E
        // Selector: 0x0018 (code64 GDT entry)
        core::ptr::write_volatile((data_base + 0x54) as *mut u32, 0x0000_806Eu32);
        core::ptr::write_volatile((data_base + 0x58) as *mut u16, 0x0018u16);

        // Ensure all writes are visible to other CPUs.
        // On x86, stores are ordered and caches are coherent (MESI), but
        // wbinvd guarantees write-back of all modified cache lines.  This
        // is cheap (we only touched ~256 bytes) and eliminates any doubt.
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("wbinvd", options(nomem, nostack));
    }

    serial_println!(
        "[SMP] AP trampoline installed at {:#x}, data at {:#x}",
        AP_TRAMPOLINE_ADDR,
        AP_DATA_ADDR
    );
}

/// Allocate a stack for an AP
///
/// The stack must be in a virtual address range that is mapped in the BSP's
/// page tables (which the AP shares).  The simplest way to guarantee this is
/// to allocate from the kernel heap, which is already mapped.
fn allocate_ap_stack(_cpu_index: u32) -> u64 {
    use alloc::alloc::{Layout, alloc};
    let layout = Layout::from_size_align(AP_STACK_SIZE, 16).unwrap();
    let ptr = unsafe { alloc(layout) };
    if ptr.is_null() {
        // Fallback to the old static scheme (may not be mapped)
        let stack_bottom = AP_STACK_BASE + (_cpu_index as u64) * AP_STACK_SIZE as u64;
        return stack_bottom + AP_STACK_SIZE as u64;
    }
    // Stack grows downward — return the TOP of the allocation
    (ptr as u64) + AP_STACK_SIZE as u64
}

/// Start an Application Processor
pub fn start_ap(apic_id: u32, cpu_index: u32) {
    serial_println!("[SMP] start_ap: APIC ID {}, index {}", apic_id, cpu_index);

    // Prepare AP-specific data
    let phys_offset = PHYS_OFFSET.load(Ordering::Relaxed);
    let stack_top = allocate_ap_stack(cpu_index);
    serial_println!("[SMP]   stack_top = {:#x}", stack_top);

    unsafe {
        let data_base = phys_offset + AP_DATA_ADDR;
        // Stack top at offset 0x10
        core::ptr::write_volatile((data_base + 0x10) as *mut u64, stack_top);
        // APIC ID at offset 0x18
        core::ptr::write_volatile((data_base + 0x18) as *mut u32, apic_id);
    }

    AP_ALIVE.store(false, Ordering::SeqCst);

    serial_println!("[SMP]   sending INIT IPI...");
    unsafe {
        // Send INIT IPI (level-triggered assert)
        // Bits: delivery mode INIT (0x500) | level assert (bit14=1, 0x4000)
        //       | trigger mode level (bit15=1, 0x8000) = 0xC500
        wait_icr_idle();
        lapic_write(LAPIC_ICR_HI, apic_id << 24);
        lapic_write(LAPIC_ICR_LO, 0x0000_C500); // INIT, level, assert

        // Short delay, then de-assert INIT
        for _ in 0..1_000u32 {
            core::hint::spin_loop();
        }
        wait_icr_idle();
        lapic_write(LAPIC_ICR_HI, apic_id << 24);
        lapic_write(LAPIC_ICR_LO, 0x0000_8500); // INIT, level, de-assert

        // Wait 10ms (Intel MP spec requirement after INIT)
        pit_wait_10ms();
        serial_println!(
            "[SMP]   sending SIPI (vector={:#x})...",
            AP_TRAMPOLINE_ADDR / 4096
        );

        // Send two STARTUP IPIs (SIPI)
        for _ in 0..2 {
            let sipi_vector = (AP_TRAMPOLINE_ADDR / 4096) as u32;
            wait_icr_idle();
            lapic_write(LAPIC_ICR_HI, apic_id << 24);
            lapic_write(LAPIC_ICR_LO, IPI_STARTUP | sipi_vector);

            // Wait ~200µs between SIPIs (Intel MP spec)
            for _ in 0..20_000u32 {
                core::hint::spin_loop();
            }
        }
    }

    serial_println!("[SMP]   waiting for AP alive signal...");
    // Wait for AP to signal it's alive (generous timeout for slower emulators)
    for _ in 0..5_000_000u32 {
        if AP_ALIVE.load(Ordering::SeqCst) {
            serial_println!(
                "[SMP] AP {} (APIC ID {}) started successfully",
                cpu_index,
                apic_id
            );
            return;
        }
        core::hint::spin_loop();
    }

    serial_println!(
        "[SMP] AP {} (APIC ID {}) did not respond (timeout, continuing)",
        cpu_index,
        apic_id
    );
}

/// 64-bit AP entry point — called from trampoline after mode switch
/// This runs on the AP's own stack in long mode.
///
/// All AP initialization is done here (not in a separate function) to
/// avoid issues with function prologues when the GDT/IDT haven't been
/// loaded yet.
extern "C" fn ap_entry_64(apic_id: u32) {
    // Claim a CPU index first so we load the matching per-CPU TSS.
    let cpu_index = CPUS_STARTED.fetch_add(1, Ordering::Relaxed);

    crate::gdt::init_ap(cpu_index);
    crate::interrupts::init_idt();
    init_lapic();

    let mut kernel_rsp: u64 = 0;
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!("mov {}, rsp", out(reg) kernel_rsp, options(nostack));
    }
    crate::usermode::program_ap_gs(cpu_index, apic_id, kernel_rsp);

    let mut tr: u16 = 0;
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!("str {0:x}", out(reg) tr, options(nostack, nomem));
    }
    AP_TSS_SEL.store(tr as u32, Ordering::SeqCst);

    {
        let mut cpus = CPU_DATA.lock();
        if (cpu_index as usize) < MAX_CPUS {
            cpus[cpu_index as usize] = PerCpuData {
                apic_id,
                cpu_index,
                is_bsp: false,
                online: true,
                current_pid: 0,
                context_switches: 0,
                timer_ticks: 0,
                idle_ticks: 0,
            };
        }
    }

    // Signal BSP that we're alive — BEFORE timer calibration so the BSP
    // doesn't time out during the 10ms calibration spin.
    AP_ALIVE.store(true, Ordering::SeqCst);

    // Initialize APIC timer on this AP
    init_timer(100); // 100 Hz

    serial_println!("[SMP] AP {} online (APIC ID {})", cpu_index, apic_id);

    // Real idle: wait for the BSP to calibrate `apic_timer`, then STI and
    // run any Ring 3 task queued on this CPU (Gate I3). Never HLT with IF=0.
    ap_idle_loop();
}

/// AP entry point (called when an AP starts up in 64-bit mode)
/// Kept as a public API entry point; delegates to ap_entry_64's logic.
pub fn ap_entry(_apic_id: u32) {
    // All logic is now in ap_entry_64 directly.
    // This stub is kept for any external callers.
}

pub const GATE_I1_MARKER: &str = "GATE_I1 smp online";

/// Prove GS is per-CPU, the BSP TSS is loaded, and at least one AP came up
/// with its own TSS (QEMU `-smp 2`).
pub fn smp_self_test() -> bool {
    let gs = crate::usermode::gs_base();
    let expected_gs = crate::usermode::cpu_local_ptr(0) as u64;
    let cpu = crate::usermode::current_cpu_index();
    let mut tr: u16 = 0;
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!("str {0:x}", out(reg) tr, options(nostack, nomem));
    }
    let bsp_sel = crate::gdt::tss_selector(0).0;
    let online = online_cpus();
    let ap_tr = AP_TSS_SEL.load(Ordering::SeqCst) as u16;
    let ap_sel = crate::gdt::tss_selector(1).0;

    let gs_ok = gs == expected_gs && cpu == 0;
    let tss_ok = tr == bsp_sel && bsp_sel != 0;
    let ap_ok = online >= 2 && ap_tr == ap_sel && ap_sel != 0 && ap_sel != bsp_sel;

    if gs_ok && tss_ok && ap_ok {
        serial_println!(
            "[SMP] {} (online={} gs={:#x} tr={:#x} ap_tr={:#x})",
            GATE_I1_MARKER,
            online,
            gs,
            tr,
            ap_tr
        );
        true
    } else {
        serial_println!(
            "[SMP] Gate I1 FAILED: online={} gs={:#x}/{:#x} cpu={} tr={:#x}/{:#x} ap_tr={:#x}/{:#x}",
            online,
            gs,
            expected_gs,
            cpu,
            tr,
            bsp_sel,
            ap_tr,
            ap_sel
        );
        false
    }
}
