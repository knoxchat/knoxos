use core::sync::atomic::Ordering;

use super::state::PHYS_OFFSET;

/// Detect the number of CPUs via CPUID
pub fn detect_cpus() -> u32 {
    // Try ACPI MADT first — it lists exactly the CPUs the firmware describes.
    // The MADT is populated by QEMU to match the `-smp N` setting.
    if let Some(count) = detect_cpus_from_madt() {
        if count >= 1 {
            return count;
        }
    }

    // Fallback: CPUID max_logical_processor_ids reports the maximum the
    // package *could* support, which may be larger than the actual number
    // of vCPUs.  On QEMU with -smp 1 this would report 2 for AMD K8,
    // causing us to try to boot a non-existent AP.  Use 1 as safe default.
    1
}

/// Scan the ACPI MADT (Multiple APIC Description Table) for Local APIC
/// entries to determine the true CPU count.
fn detect_cpus_from_madt() -> Option<u32> {
    let phys_offset = PHYS_OFFSET.load(Ordering::Relaxed);
    if phys_offset == 0 {
        return None;
    }

    // Search for the RSDP in the EBDA (0x040E pointer) and BIOS area (0xE0000-0xFFFFF)
    let rsdp_addr = find_rsdp(phys_offset)?;

    // Read RSDP to find RSDT
    let rsdp = unsafe { &*((phys_offset + rsdp_addr) as *const AcpiRsdp) };
    if &rsdp.signature != b"RSD PTR " {
        return None;
    }
    let rsdt_addr = rsdp.rsdt_address as u64;

    // Read RSDT header
    let rsdt = unsafe { &*((phys_offset + rsdt_addr) as *const AcpiSdtHeader) };
    if &rsdt.signature != b"RSDT" {
        return None;
    }

    // Iterate RSDT entries (array of u32 physical pointers after the header)
    let entry_count = (rsdt.length as usize - core::mem::size_of::<AcpiSdtHeader>()) / 4;
    let entries = unsafe {
        core::slice::from_raw_parts(
            ((phys_offset + rsdt_addr) as usize + core::mem::size_of::<AcpiSdtHeader>())
                as *const u32,
            entry_count,
        )
    };

    for &entry_phys in entries {
        let hdr = unsafe { &*((phys_offset + entry_phys as u64) as *const AcpiSdtHeader) };
        if &hdr.signature == b"APIC" {
            // Found the MADT
            return parse_madt(phys_offset, entry_phys as u64, hdr.length);
        }
    }

    None
}

/// Parse the MADT to count Local APIC entries with the Enabled flag set.
fn parse_madt(phys_offset: u64, madt_phys: u64, total_len: u32) -> Option<u32> {
    let base = (phys_offset + madt_phys) as usize;
    // MADT header: SDT header (36 bytes) + local APIC address (4) + flags (4) = 44 bytes
    let mut offset = 44usize;
    let mut cpu_count = 0u32;

    while offset + 2 <= total_len as usize {
        let entry_type = unsafe { *(base.wrapping_add(offset) as *const u8) };
        let entry_len = unsafe { *(base.wrapping_add(offset + 1) as *const u8) } as usize;
        if entry_len < 2 {
            break;
        }

        if entry_type == 0 && entry_len >= 8 {
            // Type 0 = Processor Local APIC
            // Byte 4 = APIC ID, Byte 8 (offset+4) = flags
            let flags = unsafe { *((base + offset + 4) as *const u32) };
            // Bit 0 = Processor Enabled, Bit 1 = Online Capable
            if flags & 0x01 != 0 {
                cpu_count += 1;
            }
        }

        offset += entry_len;
    }

    if cpu_count > 0 { Some(cpu_count) } else { None }
}

/// Search for the ACPI RSDP signature in standard locations
fn find_rsdp(phys_offset: u64) -> Option<u64> {
    // Check EBDA pointer at 0x040E
    let ebda_seg = unsafe { *((phys_offset + 0x040E) as *const u16) } as u64;
    let ebda_base = ebda_seg << 4;
    if ebda_base > 0 && ebda_base < 0xA0000 {
        for addr in (ebda_base..ebda_base + 1024).step_by(16) {
            let sig = unsafe { &*((phys_offset + addr) as *const [u8; 8]) };
            if sig == b"RSD PTR " {
                return Some(addr);
            }
        }
    }

    // Search BIOS read-only area 0xE0000 - 0xFFFFF
    for addr in (0xE0000u64..0x100000).step_by(16) {
        let sig = unsafe { &*((phys_offset + addr) as *const [u8; 8]) };
        if sig == b"RSD PTR " {
            return Some(addr);
        }
    }

    None
}

/// ACPI RSDP structure (v1)
#[repr(C, packed)]
struct AcpiRsdp {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_address: u32,
}

/// ACPI SDT header (common to all ACPI tables)
#[repr(C, packed)]
struct AcpiSdtHeader {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
}
