/// Architecture-abstracted display initialization for KnoxOS
///
/// Provides a unified display backend that works across x86_64, aarch64, and riscv64.
/// Each architecture has its own framebuffer discovery and mode-setting path:
///   - x86_64: Bochs VBE (BGA) + VESA LFB from bootloader
///   - aarch64: SimpleFB / VirtIO GPU (device tree)
///   - riscv64: SimpleFB / VirtIO GPU (device tree)
///
/// The common `DisplayBackend` trait abstracts over these differences so the
/// rest of the GUI stack never touches arch-specific registers.

/// Display backend abstraction — implemented per architecture
pub trait DisplayBackend {
    /// Try to set a display mode. Returns true on success.
    fn set_mode(&self, width: u16, height: u16) -> bool;
    /// Check if this backend is available on the current hardware.
    fn is_available(&self) -> bool;
    /// Get the name of this backend (for logging).
    fn name(&self) -> &'static str;
}

// ═══════════════════════════════════════════════════════════════════════
// x86_64: Bochs VBE (BGA) via I/O ports
// ═══════════════════════════════════════════════════════════════════════

#[cfg(target_arch = "x86_64")]
pub mod bga {
    use super::DisplayBackend;
    #[cfg(target_arch = "x86_64")]
    use crate::arch_compat::instructions::port::Port;
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::instructions::port::Port;

    const VBE_INDEX: u16 = 0x01CE;
    const VBE_DATA: u16 = 0x01CF;

    const VBE_DISPI_INDEX_ID: u16 = 0x00;
    const VBE_DISPI_INDEX_XRES: u16 = 0x01;
    const VBE_DISPI_INDEX_YRES: u16 = 0x02;
    const VBE_DISPI_INDEX_BPP: u16 = 0x03;
    const VBE_DISPI_INDEX_ENABLE: u16 = 0x04;

    const VBE_DISPI_DISABLED: u16 = 0x00;
    const VBE_DISPI_ENABLED: u16 = 0x01;
    const VBE_DISPI_LFB_ENABLED: u16 = 0x40;

    unsafe fn write_reg(index: u16, value: u16) {
        let mut idx_port = Port::<u16>::new(VBE_INDEX);
        let mut dat_port = Port::<u16>::new(VBE_DATA);
        idx_port.write(index);
        dat_port.write(value);
    }

    unsafe fn read_reg(index: u16) -> u16 {
        let mut idx_port = Port::<u16>::new(VBE_INDEX);
        let mut dat_port = Port::<u16>::new(VBE_DATA);
        idx_port.write(index);
        dat_port.read()
    }

    pub struct BgaBackend;

    impl DisplayBackend for BgaBackend {
        fn is_available(&self) -> bool {
            let id = unsafe { read_reg(VBE_DISPI_INDEX_ID) };
            (0xB0C0..=0xB0C5).contains(&id)
        }

        fn set_mode(&self, width: u16, height: u16) -> bool {
            if !self.is_available() {
                return false;
            }
            unsafe {
                write_reg(VBE_DISPI_INDEX_ENABLE, VBE_DISPI_DISABLED);
                write_reg(VBE_DISPI_INDEX_XRES, width);
                write_reg(VBE_DISPI_INDEX_YRES, height);
                write_reg(VBE_DISPI_INDEX_BPP, 32);
                write_reg(
                    VBE_DISPI_INDEX_ENABLE,
                    VBE_DISPI_ENABLED | VBE_DISPI_LFB_ENABLED,
                );
            }
            let actual_w = unsafe { read_reg(VBE_DISPI_INDEX_XRES) };
            let actual_h = unsafe { read_reg(VBE_DISPI_INDEX_YRES) };
            actual_w == width && actual_h == height
        }

        fn name(&self) -> &'static str {
            "Bochs VBE (BGA)"
        }
    }

    /// Convenience: check if BGA is available
    pub fn is_available() -> bool {
        BgaBackend.is_available()
    }

    /// Convenience: set BGA mode
    pub fn set_mode(width: u16, height: u16) -> bool {
        BgaBackend.set_mode(width, height)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// aarch64: SimpleFB / VirtIO GPU
// ═══════════════════════════════════════════════════════════════════════

#[cfg(target_arch = "aarch64")]
pub mod simplefb {
    use super::DisplayBackend;
    use core::sync::atomic::{AtomicU64, Ordering};

    /// Address of the SimpleFB framebuffer, set during device tree parsing.
    static SIMPLEFB_ADDR: AtomicU64 = AtomicU64::new(0);

    pub struct SimpleFbBackend;

    impl DisplayBackend for SimpleFbBackend {
        fn is_available(&self) -> bool {
            // SimpleFB is set up by firmware/bootloader via device tree
            // We detect it during device tree parsing
            SIMPLEFB_ADDR.load(Ordering::Relaxed) != 0
        }

        fn set_mode(&self, _width: u16, _height: u16) -> bool {
            // SimpleFB does not support mode switching — the resolution is
            // fixed by the bootloader. Return false to indicate no runtime
            // mode change is possible.
            false
        }

        fn name(&self) -> &'static str {
            "SimpleFB (device tree)"
        }
    }

    pub fn is_available() -> bool {
        SimpleFbBackend.is_available()
    }
}

// ═══════════════════════════════════════════════════════════════════════
// riscv64: SimpleFB / VirtIO GPU
// ═══════════════════════════════════════════════════════════════════════

#[cfg(target_arch = "riscv64")]
pub mod simplefb_rv {
    use super::DisplayBackend;
    use core::sync::atomic::{AtomicU64, Ordering};

    /// Address of the SimpleFB framebuffer, set during device tree parsing.
    static SIMPLEFB_ADDR: AtomicU64 = AtomicU64::new(0);

    pub struct SimpleFbRvBackend;

    impl DisplayBackend for SimpleFbRvBackend {
        fn is_available(&self) -> bool {
            SIMPLEFB_ADDR.load(Ordering::Relaxed) != 0
        }

        fn set_mode(&self, _width: u16, _height: u16) -> bool {
            false
        }

        fn name(&self) -> &'static str {
            "SimpleFB (RISC-V device tree)"
        }
    }

    pub fn is_available() -> bool {
        SimpleFbRvBackend.is_available()
    }
}

// ═══════════════════════════════════════════════════════════════════════
// VirtIO GPU (works on all architectures)
// ═══════════════════════════════════════════════════════════════════════

pub mod virtio_gpu_display {
    use super::DisplayBackend;

    pub struct VirtioGpuBackend;

    impl DisplayBackend for VirtioGpuBackend {
        fn is_available(&self) -> bool {
            // VirtIO GPU presence is detected during PCI/MMIO device enumeration
            crate::virtio_gpu::is_available()
        }

        fn set_mode(&self, width: u16, height: u16) -> bool {
            let rect = crate::virtio_gpu::Rect {
                x: 0,
                y: 0,
                width: width as u32,
                height: height as u32,
            };
            crate::virtio_gpu::set_scanout(0, 1, rect).is_ok()
        }

        fn name(&self) -> &'static str {
            "VirtIO GPU"
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Architecture-abstracted timestamp counter
// ═══════════════════════════════════════════════════════════════════════

/// Read a monotonic high-resolution timestamp.
/// - x86_64: TSC via RDTSC
/// - aarch64: CNTVCT_EL0 (virtual counter)
/// - riscv64: rdtime (mtime counter)
#[inline]
pub fn read_timestamp() -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        crate::arch_compat::read_tsc()
    }
    #[cfg(target_arch = "aarch64")]
    {
        let mut val: u64 = 0;
        unsafe {
            core::arch::asm!("mrs {}, cntvct_el0", out(reg) val);
        }
        val
    }
    #[cfg(target_arch = "riscv64")]
    {
        let mut val: u64 = 0;
        unsafe {
            core::arch::asm!("rdtime {}", out(reg) val);
        }
        val
    }
    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    )))]
    {
        0
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Architecture-abstracted page table walk (for framebuffer VA→PA)
// ═══════════════════════════════════════════════════════════════════════

/// Translate a virtual address to physical by walking the active page table.
/// Returns `Some(phys_addr)` on success, `None` if unmapped.
pub fn translate_vaddr(vaddr: u64, phys_offset: u64) -> Option<u64> {
    #[cfg(target_arch = "x86_64")]
    {
        translate_vaddr_x86_64(vaddr, phys_offset)
    }
    #[cfg(target_arch = "aarch64")]
    {
        translate_vaddr_aarch64(vaddr, phys_offset)
    }
    #[cfg(target_arch = "riscv64")]
    {
        translate_vaddr_riscv64(vaddr, phys_offset)
    }
    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    )))]
    {
        None
    }
}

#[cfg(target_arch = "x86_64")]
fn translate_vaddr_x86_64(vaddr: u64, phys_offset: u64) -> Option<u64> {
    #[cfg(target_arch = "x86_64")]
    use crate::arch_compat::registers::control::Cr3;
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::registers::control::Cr3;
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::structures::paging::page_table::{PageTable, PageTableFlags};
    #[cfg(target_arch = "x86_64")]
    use x86_64::structures::paging::page_table::{PageTable, PageTableFlags};

    let (l4_frame, _) = Cr3::read();
    let cr3 = l4_frame.start_address().as_u64();

    let l4_idx = ((vaddr >> 39) & 0x1FF) as usize;
    let l3_idx = ((vaddr >> 30) & 0x1FF) as usize;
    let l2_idx = ((vaddr >> 21) & 0x1FF) as usize;
    let l1_idx = ((vaddr >> 12) & 0x1FF) as usize;
    let page_off = vaddr & 0xFFF;

    unsafe {
        let l4 = &*((cr3 + phys_offset) as *const PageTable);
        let e4 = &l4[l4_idx];
        if e4.is_unused() {
            return None;
        }

        let l3 = &*((e4.addr().as_u64() + phys_offset) as *const PageTable);
        let e3 = &l3[l3_idx];
        if e3.is_unused() {
            return None;
        }
        if e3.flags().contains(PageTableFlags::HUGE_PAGE) {
            return Some(e3.addr().as_u64() + (vaddr & 0x3FFF_FFFF));
        }

        let l2 = &*((e3.addr().as_u64() + phys_offset) as *const PageTable);
        let e2 = &l2[l2_idx];
        if e2.is_unused() {
            return None;
        }
        if e2.flags().contains(PageTableFlags::HUGE_PAGE) {
            return Some(e2.addr().as_u64() + (vaddr & 0x1F_FFFF));
        }

        let l1 = &*((e2.addr().as_u64() + phys_offset) as *const PageTable);
        let e1 = &l1[l1_idx];
        if e1.is_unused() {
            return None;
        }

        Some(e1.addr().as_u64() + page_off)
    }
}

#[cfg(target_arch = "aarch64")]
fn translate_vaddr_aarch64(vaddr: u64, phys_offset: u64) -> Option<u64> {
    // AArch64 4-level page table walk (4KB granule, 48-bit VA)
    let mut ttbr1: u64 = 0;
    unsafe {
        core::arch::asm!("mrs {}, ttbr1_el1", out(reg) ttbr1);
    }
    let base = ttbr1 & 0x0000_FFFF_FFFF_F000;

    let l0_idx = ((vaddr >> 39) & 0x1FF) as usize;
    let l1_idx = ((vaddr >> 30) & 0x1FF) as usize;
    let l2_idx = ((vaddr >> 21) & 0x1FF) as usize;
    let l3_idx = ((vaddr >> 12) & 0x1FF) as usize;
    let page_off = vaddr & 0xFFF;

    unsafe {
        let l0_table = (base + phys_offset) as *const u64;
        let l0_entry = *l0_table.add(l0_idx);
        if l0_entry & 1 == 0 {
            return None;
        }

        let l1_base = l0_entry & 0x0000_FFFF_FFFF_F000;
        let l1_table = (l1_base + phys_offset) as *const u64;
        let l1_entry = *l1_table.add(l1_idx);
        if l1_entry & 1 == 0 {
            return None;
        }
        // 1GB block?
        if l1_entry & 0x2 == 0 {
            return Some((l1_entry & 0x0000_FFFF_C000_0000) + (vaddr & 0x3FFF_FFFF));
        }

        let l2_base = l1_entry & 0x0000_FFFF_FFFF_F000;
        let l2_table = (l2_base + phys_offset) as *const u64;
        let l2_entry = *l2_table.add(l2_idx);
        if l2_entry & 1 == 0 {
            return None;
        }
        // 2MB block?
        if l2_entry & 0x2 == 0 {
            return Some((l2_entry & 0x0000_FFFF_FFE0_0000) + (vaddr & 0x1F_FFFF));
        }

        let l3_base = l2_entry & 0x0000_FFFF_FFFF_F000;
        let l3_table = (l3_base + phys_offset) as *const u64;
        let l3_entry = *l3_table.add(l3_idx);
        if l3_entry & 1 == 0 {
            return None;
        }

        Some((l3_entry & 0x0000_FFFF_FFFF_F000) + page_off)
    }
}

#[cfg(target_arch = "riscv64")]
fn translate_vaddr_riscv64(vaddr: u64, phys_offset: u64) -> Option<u64> {
    // Sv48 4-level page table walk
    let mut satp: u64 = 0;
    unsafe {
        core::arch::asm!("csrr {}, satp", out(reg) satp);
    }
    // Sv48: mode=9, PPN in bits [43:0]
    let root_ppn = satp & 0x0FFF_FFFF_FFFF;
    let root_pa = root_ppn << 12;

    let vpn = [
        ((vaddr >> 12) & 0x1FF) as usize,
        ((vaddr >> 21) & 0x1FF) as usize,
        ((vaddr >> 30) & 0x1FF) as usize,
        ((vaddr >> 39) & 0x1FF) as usize,
    ];
    let page_off = vaddr & 0xFFF;

    let mut pa = root_pa;

    unsafe {
        // Walk levels 3→0
        for level in (0..4).rev() {
            let table = (pa + phys_offset) as *const u64;
            let pte = *table.add(vpn[level]);
            if pte & 1 == 0 {
                return None; // Invalid
            }
            let ppn = (pte >> 10) & 0x0FFF_FFFF_FFFF;
            // Leaf page?
            if pte & 0xE != 0 {
                // Superpage alignment check
                let page_size = 1u64 << (12 + level * 9);
                return Some((ppn << 12) + (vaddr & (page_size - 1)));
            }
            pa = ppn << 12;
        }
    }

    None
}

/// Check if any display backend is available for the current architecture.
pub fn has_display_backend() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        bga::is_available() || virtio_gpu_display::VirtioGpuBackend.is_available()
    }
    #[cfg(target_arch = "aarch64")]
    {
        simplefb::is_available() || virtio_gpu_display::VirtioGpuBackend.is_available()
    }
    #[cfg(target_arch = "riscv64")]
    {
        simplefb_rv::is_available() || virtio_gpu_display::VirtioGpuBackend.is_available()
    }
    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    )))]
    {
        false
    }
}

/// Try to set display mode using the best available backend.
/// Returns true on success.
pub fn set_display_mode(width: u16, height: u16) -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        if bga::is_available() {
            return bga::set_mode(width, height);
        }
    }
    // All arches: try VirtIO GPU
    if virtio_gpu_display::VirtioGpuBackend.is_available() {
        return virtio_gpu_display::VirtioGpuBackend.set_mode(width, height);
    }
    false
}
