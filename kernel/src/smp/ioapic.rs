use core::sync::atomic::Ordering;

use crate::serial_println;

use super::lapic::get_apic_id;
use super::regs::*;
use super::state::{IOAPIC_GSI_BASE, IOAPIC_PHYS, PHYS_OFFSET};

// ─── I/O APIC ───────────────────────────────────────────────────────────

/// Disable the legacy 8259 PIC by masking all IRQ lines.
/// Called after I/O APIC is configured to take over interrupt routing.
pub fn disable_legacy_pic() {
    unsafe {
        #[cfg(target_arch = "x86_64")]
        use crate::arch_compat::instructions::port::Port;
        #[cfg(not(target_arch = "x86_64"))]
        use crate::arch_compat::instructions::port::Port;
        let mut pic1_data: Port<u8> = Port::new(0x21);
        let mut pic2_data: Port<u8> = Port::new(0xA1);
        // Mask all IRQs on both PICs
        pic1_data.write(0xFF);
        pic2_data.write(0xFF);
    }
    serial_println!("[APIC] Legacy 8259 PIC disabled (all IRQs masked)");
}

/// Read an I/O APIC register
unsafe fn ioapic_read(reg: u32) -> u32 {
    let phys_offset = PHYS_OFFSET.load(Ordering::Relaxed);
    let base = phys_offset + IOAPIC_PHYS.load(Ordering::Relaxed);
    let select = base as *mut u32;
    let data = (base + IOAPIC_REG_DATA as u64) as *mut u32;
    core::ptr::write_volatile(select, reg);
    core::ptr::read_volatile(data)
}

/// Write an I/O APIC register
unsafe fn ioapic_write(reg: u32, value: u32) {
    let phys_offset = PHYS_OFFSET.load(Ordering::Relaxed);
    let base = phys_offset + IOAPIC_PHYS.load(Ordering::Relaxed);
    let select = base as *mut u32;
    let data = (base + IOAPIC_REG_DATA as u64) as *mut u32;
    core::ptr::write_volatile(select, reg);
    core::ptr::write_volatile(data, value);
}

/// Set up an I/O APIC redirection entry
pub fn ioapic_route_irq(irq: u8, vector: u8, dest_apic_id: u8) {
    unsafe {
        let reg_lo = IOAPIC_RED_TABLE_BASE + (irq as u32) * 2;
        let reg_hi = reg_lo + 1;

        // Low 32 bits: vector, delivery mode, etc.
        let lo = vector as u32; // Fixed delivery, physical destination, active high, edge-triggered
        // High 32 bits: destination APIC ID
        let hi = (dest_apic_id as u32) << 24;

        ioapic_write(reg_lo, lo);
        ioapic_write(reg_hi, hi);
    }
}

/// Initialize the I/O APIC
pub fn init_ioapic() {
    unsafe {
        let version = ioapic_read(IOAPIC_REG_VERSION);
        let max_redir = ((version >> 16) & 0xFF) as u8;
        let id = (ioapic_read(IOAPIC_REG_ID) >> 24) & 0xFF;

        serial_println!(
            "[IOAPIC] ID={}, version={:#x}, max redirections={}",
            id,
            version & 0xFF,
            max_redir + 1
        );

        // Mask all interrupts first
        for i in 0..=max_redir {
            let reg = IOAPIC_RED_TABLE_BASE + (i as u32) * 2;
            ioapic_write(reg, 0x10000); // Masked
        }

        // Route standard IRQs to BSP (matching PIC vector assignments)
        let bsp_id = get_apic_id() as u8;

        // Timer (IRQ 0) -> Vector 0x20
        ioapic_route_irq(0, 0x20, bsp_id);
        // Keyboard (IRQ 1) -> Vector 0x21
        ioapic_route_irq(1, 0x21, bsp_id);
        // Cascade (IRQ 2) - not needed with I/O APIC, leave masked
        // COM2 (IRQ 3) -> Vector 0x23
        ioapic_route_irq(3, 0x23, bsp_id);
        // COM1 (IRQ 4) -> Vector 0x24
        ioapic_route_irq(4, 0x24, bsp_id);
        // RTC (IRQ 8) -> Vector 0x28
        ioapic_route_irq(8, 0x28, bsp_id);
        // Virtio / PCI IRQ lines (IRQ 9, 10, 11)
        ioapic_route_irq(9, 0x29, bsp_id);
        ioapic_route_irq(10, 0x2A, bsp_id);
        ioapic_route_irq(11, 0x2B, bsp_id);
        // Mouse (IRQ 12) -> Vector 0x2C
        ioapic_route_irq(12, 0x2C, bsp_id);
        // ATA Primary (IRQ 14) -> Vector 0x2E
        ioapic_route_irq(14, 0x2E, bsp_id);
        // ATA Secondary (IRQ 15) -> Vector 0x2F
        ioapic_route_irq(15, 0x2F, bsp_id);
    }

    serial_println!("[IOAPIC] I/O APIC initialized, IRQs routed to BSP");
}

/// Apply MADT I/O APIC address and interrupt-source overrides.
/// Called after `acpi_tables::init` so IRQ0→GSI 2 (typical QEMU) is honored.
pub fn apply_madt_ioapic() {
    let Some(info) = crate::acpi_tables::get_info() else {
        serial_println!("[IOAPIC] No ACPI MADT; keeping default {:#x}", IOAPIC_BASE);
        return;
    };

    if let Some(io) = info.io_apics.first() {
        IOAPIC_PHYS.store(io.address as u64, Ordering::Relaxed);
        IOAPIC_GSI_BASE.store(io.gsi_base, Ordering::Relaxed);
        serial_println!(
            "[IOAPIC] MADT address={:#x} gsi_base={} ({} override(s))",
            io.address,
            io.gsi_base,
            info.interrupt_overrides.len()
        );
    }

    for ov in &info.interrupt_overrides {
        serial_println!(
            "[IOAPIC] ISO bus={} IRQ{} -> GSI {} flags={:#x}",
            ov.bus,
            ov.irq_source,
            ov.gsi,
            ov.flags
        );
    }

    unsafe {
        let version = ioapic_read(IOAPIC_REG_VERSION);
        let max_redir = ((version >> 16) & 0xFF) as u8;
        for i in 0..=max_redir {
            let reg = IOAPIC_RED_TABLE_BASE + (i as u32) * 2;
            ioapic_write(reg, 0x10000);
        }

        let bsp_id = get_apic_id() as u8;
        let gsi_base = IOAPIC_GSI_BASE.load(Ordering::Relaxed);
        let pin = |irq: u8| -> u8 {
            let gsi = info
                .interrupt_overrides
                .iter()
                .find(|o| o.irq_source == irq)
                .map(|o| o.gsi)
                .unwrap_or(irq as u32);
            gsi.saturating_sub(gsi_base) as u8
        };

        ioapic_route_irq(pin(0), 0x20, bsp_id);
        ioapic_route_irq(pin(1), 0x21, bsp_id);
        ioapic_route_irq(pin(3), 0x23, bsp_id);
        ioapic_route_irq(pin(4), 0x24, bsp_id);
        ioapic_route_irq(pin(8), 0x28, bsp_id);
        ioapic_route_irq(pin(9), 0x29, bsp_id);
        ioapic_route_irq(pin(10), 0x2A, bsp_id);
        ioapic_route_irq(pin(11), 0x2B, bsp_id);
        ioapic_route_irq(pin(12), 0x2C, bsp_id);
        ioapic_route_irq(pin(14), 0x2E, bsp_id);
        ioapic_route_irq(pin(15), 0x2F, bsp_id);
    }

    serial_println!("[IOAPIC] MADT redirection table programmed");
}
