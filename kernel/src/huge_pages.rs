/// Huge Pages — 2 MiB and 1 GiB page support for TLB performance
///
/// Provides huge page allocation and mapping for performance-critical
/// memory regions. Huge pages reduce TLB misses by covering more virtual
/// address space per TLB entry:
///   - 2 MiB pages: 512× fewer TLB entries than 4 KiB pages
///   - 1 GiB pages: 262144× fewer TLB entries
///
/// Used for:
///   - Framebuffer mappings (always 2 MiB aligned)
///   - Large anonymous mmap regions
///   - DMA buffers
///   - HugePages pool (like Linux hugetlbfs)
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

/// 2 MiB huge page size
pub const HUGE_PAGE_2M: u64 = 2 * 1024 * 1024;
/// 1 GiB huge page size
pub const HUGE_PAGE_1G: u64 = 1024 * 1024 * 1024;
/// Standard 4 KiB page
pub const PAGE_SIZE_4K: u64 = 4096;

/// Huge page sizes supported
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HugePageSize {
    /// 2 MiB (Level 2 page table entry with PS bit)
    Size2M,
    /// 1 GiB (Level 3 page table entry with PS bit)  
    Size1G,
}

impl HugePageSize {
    pub fn size_bytes(self) -> u64 {
        match self {
            HugePageSize::Size2M => HUGE_PAGE_2M,
            HugePageSize::Size1G => HUGE_PAGE_1G,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            HugePageSize::Size2M => "2MiB",
            HugePageSize::Size1G => "1GiB",
        }
    }
}

/// A reserved huge page
#[derive(Debug, Clone)]
pub struct HugePage {
    pub phys_addr: u64,
    pub size: HugePageSize,
    pub in_use: bool,
    pub owner_pid: Option<u32>,
}

/// Huge page pool state
struct HugePagePool {
    pages_2m: Vec<HugePage>,
    pages_1g: Vec<HugePage>,
    /// Whether 1GiB pages are supported by the CPU
    supports_1g: bool,
    /// Whether 2MiB pages are supported (always true on x86_64)
    supports_2m: bool,
}

impl HugePagePool {
    const fn new() -> Self {
        Self {
            pages_2m: Vec::new(),
            pages_1g: Vec::new(),
            supports_1g: false,
            supports_2m: true,
        }
    }
}

lazy_static::lazy_static! {
    static ref POOL: Mutex<HugePagePool> = Mutex::new(HugePagePool::new());
}

static HUGE_2M_ALLOCS: AtomicU64 = AtomicU64::new(0);
static HUGE_1G_ALLOCS: AtomicU64 = AtomicU64::new(0);
static HUGE_2M_FREES: AtomicU64 = AtomicU64::new(0);
static HUGE_1G_FREES: AtomicU64 = AtomicU64::new(0);

/// Check CPU support for huge pages using CPUID
fn detect_support() -> (bool, bool) {
    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
    let supports_2m = true; // All x86_64 CPUs support 2MiB pages
    let supports_1g = cpuid
        .get_extended_processor_and_feature_identifiers()
        .map(|ext| ext.has_1gib_pages())
        .unwrap_or(false);
    (supports_2m, supports_1g)
}

/// Allocate a 2 MiB huge page from the pool
pub fn alloc_2m(owner_pid: Option<u32>) -> Option<u64> {
    let mut pool = POOL.lock();
    if !pool.supports_2m {
        return None;
    }

    // Find a free 2MiB page
    for page in &mut pool.pages_2m {
        if !page.in_use {
            page.in_use = true;
            page.owner_pid = owner_pid;
            HUGE_2M_ALLOCS.fetch_add(1, Ordering::Relaxed);
            return Some(page.phys_addr);
        }
    }
    None
}

/// Free a 2 MiB huge page back to the pool
pub fn free_2m(phys_addr: u64) -> bool {
    let mut pool = POOL.lock();
    for page in &mut pool.pages_2m {
        if page.phys_addr == phys_addr && page.in_use {
            page.in_use = false;
            page.owner_pid = None;
            HUGE_2M_FREES.fetch_add(1, Ordering::Relaxed);
            return true;
        }
    }
    false
}

/// Allocate a 1 GiB huge page from the pool
pub fn alloc_1g(owner_pid: Option<u32>) -> Option<u64> {
    let mut pool = POOL.lock();
    if !pool.supports_1g {
        return None;
    }

    for page in &mut pool.pages_1g {
        if !page.in_use {
            page.in_use = true;
            page.owner_pid = owner_pid;
            HUGE_1G_ALLOCS.fetch_add(1, Ordering::Relaxed);
            return Some(page.phys_addr);
        }
    }
    None
}

/// Free a 1 GiB huge page
pub fn free_1g(phys_addr: u64) -> bool {
    let mut pool = POOL.lock();
    for page in &mut pool.pages_1g {
        if page.phys_addr == phys_addr && page.in_use {
            page.in_use = false;
            page.owner_pid = None;
            HUGE_1G_FREES.fetch_add(1, Ordering::Relaxed);
            return true;
        }
    }
    false
}

/// Reserve physical memory for the huge page pool
/// Called during early boot to carve out contiguous physical regions
pub fn reserve_pool(num_2m: usize, num_1g: usize) {
    let mut pool = POOL.lock();

    // In a real implementation, we'd reserve contiguous physical frames
    // For now, create placeholder entries that track the state
    for i in 0..num_2m {
        pool.pages_2m.push(HugePage {
            phys_addr: 0x1_0000_0000 + (i as u64) * HUGE_PAGE_2M, // Placeholder
            size: HugePageSize::Size2M,
            in_use: false,
            owner_pid: None,
        });
    }

    for i in 0..num_1g {
        pool.pages_1g.push(HugePage {
            phys_addr: 0x10_0000_0000 + (i as u64) * HUGE_PAGE_1G, // Placeholder
            size: HugePageSize::Size1G,
            in_use: false,
            owner_pid: None,
        });
    }

    serial_println!(
        "[HugePages] Reserved {} × 2MiB + {} × 1GiB pages",
        num_2m,
        num_1g
    );
}

/// Get huge page statistics
pub fn stats() -> HugePageStats {
    let pool = POOL.lock();
    let free_2m = pool.pages_2m.iter().filter(|p| !p.in_use).count();
    let free_1g = pool.pages_1g.iter().filter(|p| !p.in_use).count();
    HugePageStats {
        total_2m: pool.pages_2m.len(),
        free_2m,
        total_1g: pool.pages_1g.len(),
        free_1g,
        supports_1g: pool.supports_1g,
        allocs_2m: HUGE_2M_ALLOCS.load(Ordering::Relaxed),
        frees_2m: HUGE_2M_FREES.load(Ordering::Relaxed),
        allocs_1g: HUGE_1G_ALLOCS.load(Ordering::Relaxed),
        frees_1g: HUGE_1G_FREES.load(Ordering::Relaxed),
    }
}

pub struct HugePageStats {
    pub total_2m: usize,
    pub free_2m: usize,
    pub total_1g: usize,
    pub free_1g: usize,
    pub supports_1g: bool,
    pub allocs_2m: u64,
    pub frees_2m: u64,
    pub allocs_1g: u64,
    pub frees_1g: u64,
}

/// Map a virtual address range using 2 MiB pages
/// Returns true on success
pub fn map_2m_page(
    mapper: &mut impl crate::arch_compat::structures::paging::Mapper<
        crate::arch_compat::structures::paging::Size2MiB,
    >,
    virt_addr: u64,
    phys_addr: u64,
    flags: crate::arch_compat::structures::paging::PageTableFlags,
    frame_allocator: &mut impl crate::arch_compat::structures::paging::FrameAllocator<
        crate::arch_compat::structures::paging::Size4KiB,
    >,
) -> bool {
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::structures::paging::{Page, PhysFrame, Size2MiB};
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::{PhysAddr, VirtAddr};
    #[cfg(target_arch = "x86_64")]
    use x86_64::structures::paging::{Page, PhysFrame, Size2MiB};
    #[cfg(target_arch = "x86_64")]
    use x86_64::{PhysAddr, VirtAddr};

    let page = Page::<Size2MiB>::containing_address(VirtAddr::new(virt_addr));
    let frame = PhysFrame::<Size2MiB>::containing_address(PhysAddr::new(phys_addr));

    unsafe {
        match mapper.map_to(page, frame, flags, frame_allocator) {
            Ok(flush) => {
                flush.flush();
                true
            }
            Err(_) => false,
        }
    }
}

/// Initialize huge page support
pub fn init() {
    let (supports_2m, supports_1g) = detect_support();

    {
        let mut pool = POOL.lock();
        pool.supports_2m = supports_2m;
        pool.supports_1g = supports_1g;
    }

    // Reserve a small default pool
    reserve_pool(16, if supports_1g { 1 } else { 0 });

    // Probe real hardware TLB capabilities
    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
    // Note: raw-cpuid doesn't expose TLB info directly; skip TLB probe
    serial_println!(
        "[HugePages] CPUID TLB probing skipped (use CPUID leaf 0x18 manually if needed)"
    );

    // Check if PAT (Page Attribute Table) is available for write-combining
    let has_pat = cpuid
        .get_feature_info()
        .map(|f| f.has_pat())
        .unwrap_or(false);

    serial_println!(
        "[KnoxOS] Huge pages initialized: 2MiB={}, 1GiB={}, PAT={}",
        if supports_2m { "yes" } else { "no" },
        if supports_1g { "yes" } else { "no" },
        if has_pat { "yes" } else { "no" },
    );

    // Report pool status
    let s = stats();
    serial_println!(
        "[HugePages] Pool: {} × 2MiB ({} free), {} × 1GiB ({} free)",
        s.total_2m,
        s.free_2m,
        s.total_1g,
        s.free_1g
    );
}

/// Map a framebuffer region using 2 MiB pages for optimal TLB performance
pub fn map_framebuffer_2m(
    mapper: &mut impl crate::arch_compat::structures::paging::Mapper<
        crate::arch_compat::structures::paging::Size2MiB,
    >,
    virt_start: u64,
    phys_start: u64,
    size: u64,
    frame_allocator: &mut impl crate::arch_compat::structures::paging::FrameAllocator<
        crate::arch_compat::structures::paging::Size4KiB,
    >,
) -> usize {
    #[cfg(target_arch = "x86_64")]
    use crate::arch_compat::structures::paging::PageTableFlags;
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::structures::paging::PageTableFlags;

    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE;

    let pages_needed = size.div_ceil(HUGE_PAGE_2M) as usize;
    let mut mapped = 0;

    for i in 0..pages_needed {
        let virt = virt_start + (i as u64) * HUGE_PAGE_2M;
        let phys = phys_start + (i as u64) * HUGE_PAGE_2M;
        if map_2m_page(mapper, virt, phys, flags, frame_allocator) {
            mapped += 1;
        }
    }

    serial_println!(
        "[HugePages] Mapped framebuffer: {} × 2MiB pages ({} MiB total)",
        mapped,
        mapped * 2
    );

    mapped
}

/// Map a DMA buffer using huge pages for reduced TLB pressure
pub fn map_dma_buffer_2m(
    mapper: &mut impl crate::arch_compat::structures::paging::Mapper<
        crate::arch_compat::structures::paging::Size2MiB,
    >,
    virt_start: u64,
    phys_start: u64,
    frame_allocator: &mut impl crate::arch_compat::structures::paging::FrameAllocator<
        crate::arch_compat::structures::paging::Size4KiB,
    >,
) -> bool {
    #[cfg(target_arch = "x86_64")]
    use crate::arch_compat::structures::paging::PageTableFlags;
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::structures::paging::PageTableFlags;

    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE;

    map_2m_page(mapper, virt_start, phys_start, flags, frame_allocator)
}

/// Get TLB statistics summary
pub fn tlb_info() -> &'static str {
    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
    let supports_1g = cpuid
        .get_extended_processor_and_feature_identifiers()
        .map(|ext| ext.has_1gib_pages())
        .unwrap_or(false);

    if supports_1g {
        "4KiB + 2MiB + 1GiB pages supported"
    } else {
        "4KiB + 2MiB pages supported"
    }
}
