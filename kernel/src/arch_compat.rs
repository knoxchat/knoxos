/// Cross-architecture compatibility shim for non-x86_64 targets.
///
/// Provides stub types and no-op functions matching the `x86_64` crate's API
/// so that code compiles on aarch64/riscv64 without feature-gating every usage.
/// On x86_64, this module is empty — the real crate is used instead.

#[cfg(not(target_arch = "x86_64"))]
pub mod instructions {
    pub mod port {
        /// Stub Port type — all operations are no-ops on non-x86 targets.
        pub struct Port<T> {
            _port: u16,
            _phantom: core::marker::PhantomData<T>,
        }
        impl<T> Port<T> {
            pub const fn new(port: u16) -> Self {
                Self {
                    _port: port,
                    _phantom: core::marker::PhantomData,
                }
            }
            pub unsafe fn read(&mut self) -> T {
                unsafe { core::mem::zeroed() }
            }
            pub unsafe fn write(&mut self, _val: T) {}
        }

        pub struct PortReadOnly<T> {
            _port: u16,
            _phantom: core::marker::PhantomData<T>,
        }
        impl<T> PortReadOnly<T> {
            pub const fn new(port: u16) -> Self {
                Self {
                    _port: port,
                    _phantom: core::marker::PhantomData,
                }
            }
            pub unsafe fn read(&mut self) -> T {
                unsafe { core::mem::zeroed() }
            }
        }

        pub struct PortWriteOnly<T> {
            _port: u16,
            _phantom: core::marker::PhantomData<T>,
        }
        impl<T> PortWriteOnly<T> {
            pub const fn new(port: u16) -> Self {
                Self {
                    _port: port,
                    _phantom: core::marker::PhantomData,
                }
            }
            pub unsafe fn write(&mut self, _val: T) {}
        }
    }

    pub mod interrupts {
        #[inline]
        pub fn without_interrupts<F, R>(f: F) -> R
        where
            F: FnOnce() -> R,
        {
            f()
        }
        #[inline]
        pub fn enable() {}
        #[inline]
        pub fn disable() {}
        #[inline]
        pub fn are_enabled() -> bool {
            false
        }
        #[inline]
        pub fn hlt() {
            #[cfg(target_arch = "aarch64")]
            unsafe {
                core::arch::asm!("wfe")
            };
            #[cfg(target_arch = "riscv64")]
            unsafe {
                core::arch::asm!("wfi")
            };
        }
        #[inline]
        pub fn int3() {}
    }

    pub mod segmentation {
        pub trait Segment {
            fn set_reg(_sel: crate::arch_compat::structures::gdt::SegmentSelector) {}
        }
        pub struct CS;
        impl Segment for CS {}
        impl CS {
            pub fn set_reg(_sel: crate::arch_compat::structures::gdt::SegmentSelector) {}
            pub fn get_reg() -> crate::arch_compat::structures::gdt::SegmentSelector {
                crate::arch_compat::structures::gdt::SegmentSelector(0)
            }
        }
        pub struct DS;
        impl Segment for DS {}
        impl DS {
            pub fn set_reg(_sel: crate::arch_compat::structures::gdt::SegmentSelector) {}
        }
        pub struct SS;
        impl Segment for SS {}
        impl SS {
            pub fn set_reg(_sel: crate::arch_compat::structures::gdt::SegmentSelector) {}
        }
    }

    pub mod tables {
        pub fn load_tss(_sel: crate::arch_compat::structures::gdt::SegmentSelector) {}
    }

    pub mod tlb {
        pub fn flush(_addr: crate::arch_compat::structures::paging::VirtAddr) {}
        pub fn flush_all() {}
    }

    pub fn hlt() {
        interrupts::hlt();
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub mod structures {
    pub mod paging {
        use bitflags::bitflags;

        bitflags! {
            #[derive(Clone, Copy, Debug)]
            pub struct PageTableFlags: u64 {
                const PRESENT = 1 << 0;
                const WRITABLE = 1 << 1;
                const USER_ACCESSIBLE = 1 << 2;
                const WRITE_THROUGH = 1 << 3;
                const NO_CACHE = 1 << 4;
                const ACCESSED = 1 << 5;
                const DIRTY = 1 << 6;
                const HUGE_PAGE = 1 << 7;
                const GLOBAL = 1 << 8;
                const NO_EXECUTE = 1 << 63;
            }
        }

        #[repr(align(4096))]
        pub struct PageTable {
            entries: [PageTableEntry; 512],
        }
        impl PageTable {
            pub fn new() -> Self {
                Self {
                    entries: [PageTableEntry(0); 512],
                }
            }
            pub fn iter(&self) -> core::slice::Iter<PageTableEntry> {
                self.entries.iter()
            }
            pub fn iter_mut(&mut self) -> core::slice::IterMut<PageTableEntry> {
                self.entries.iter_mut()
            }
            pub fn zero(&mut self) {
                for e in self.entries.iter_mut() {
                    e.0 = 0;
                }
            }
        }
        impl core::ops::Index<usize> for PageTable {
            type Output = PageTableEntry;
            fn index(&self, i: usize) -> &PageTableEntry {
                &self.entries[i]
            }
        }
        impl core::ops::IndexMut<usize> for PageTable {
            fn index_mut(&mut self, i: usize) -> &mut PageTableEntry {
                &mut self.entries[i]
            }
        }

        #[derive(Clone, Copy)]
        pub struct PageTableEntry(u64);
        impl PageTableEntry {
            pub fn new() -> Self {
                Self(0)
            }
            pub fn is_unused(&self) -> bool {
                self.0 == 0
            }
            pub fn set_unused(&mut self) {
                self.0 = 0;
            }
            pub fn flags(&self) -> PageTableFlags {
                PageTableFlags::from_bits_truncate(self.0)
            }
            pub fn set_flags(&mut self, flags: PageTableFlags) {
                self.0 = (self.0 & !0xFFF_0000_0000_0FFF) | flags.bits();
            }
            pub fn addr(&self) -> PhysAddr {
                PhysAddr::new(self.0 & 0x000F_FFFF_FFFF_F000)
            }
            pub fn set_addr(&mut self, addr: PhysAddr, flags: PageTableFlags) {
                self.0 = addr.as_u64() | flags.bits();
            }
            pub fn frame(&self) -> Result<PhysFrame, ()> {
                if self.flags().contains(PageTableFlags::PRESENT) {
                    Ok(PhysFrame {
                        start_address: self.addr(),
                        _size: core::marker::PhantomData,
                    })
                } else {
                    Err(())
                }
            }
            pub fn set_frame(&mut self, frame: PhysFrame, flags: PageTableFlags) {
                self.0 = frame.start_address.as_u64() | flags.bits();
            }
        }

        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub struct VirtAddr(u64);
        impl VirtAddr {
            pub fn new(addr: u64) -> Self {
                Self(addr)
            }
            pub fn as_u64(self) -> u64 {
                self.0
            }
            pub fn as_ptr<T>(self) -> *const T {
                self.0 as *const T
            }
            pub fn as_mut_ptr<T>(self) -> *mut T {
                self.0 as *mut T
            }
            pub fn from_ptr<T>(ptr: *const T) -> Self {
                Self(ptr as u64)
            }
            pub fn is_null(self) -> bool {
                self.0 == 0
            }
            pub fn align_up(self, align: u64) -> Self {
                Self((self.0 + align - 1) & !(align - 1))
            }
            pub fn align_down(self, align: u64) -> Self {
                Self(self.0 & !(align - 1))
            }
            pub fn p4_index(self) -> PageTableIndex {
                PageTableIndex((self.0 >> 39 & 0x1FF) as u16)
            }
            pub fn p3_index(self) -> PageTableIndex {
                PageTableIndex((self.0 >> 30 & 0x1FF) as u16)
            }
            pub fn p2_index(self) -> PageTableIndex {
                PageTableIndex((self.0 >> 21 & 0x1FF) as u16)
            }
            pub fn p1_index(self) -> PageTableIndex {
                PageTableIndex((self.0 >> 12 & 0x1FF) as u16)
            }
            pub fn page_offset(self) -> PageOffset {
                PageOffset(self.0 as u16 & 0xFFF)
            }
        }
        impl core::ops::Add<u64> for VirtAddr {
            type Output = Self;
            fn add(self, rhs: u64) -> Self {
                Self(self.0 + rhs)
            }
        }
        impl core::ops::AddAssign<u64> for VirtAddr {
            fn add_assign(&mut self, rhs: u64) {
                self.0 += rhs;
            }
        }
        impl core::ops::Sub<u64> for VirtAddr {
            type Output = Self;
            fn sub(self, rhs: u64) -> Self {
                Self(self.0 - rhs)
            }
        }
        impl core::ops::Sub<VirtAddr> for VirtAddr {
            type Output = u64;
            fn sub(self, rhs: VirtAddr) -> u64 {
                self.0 - rhs.0
            }
        }
        impl core::fmt::Display for VirtAddr {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                write!(f, "0x{:016x}", self.0)
            }
        }

        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub struct PhysAddr(u64);
        impl PhysAddr {
            pub fn new(addr: u64) -> Self {
                Self(addr)
            }
            pub fn as_u64(self) -> u64 {
                self.0
            }
            pub fn is_null(self) -> bool {
                self.0 == 0
            }
            pub fn align_up(self, align: u64) -> Self {
                Self((self.0 + align - 1) & !(align - 1))
            }
            pub fn align_down(self, align: u64) -> Self {
                Self(self.0 & !(align - 1))
            }
        }
        impl core::ops::Add<u64> for PhysAddr {
            type Output = Self;
            fn add(self, rhs: u64) -> Self {
                Self(self.0 + rhs)
            }
        }
        impl core::ops::AddAssign<u64> for PhysAddr {
            fn add_assign(&mut self, rhs: u64) {
                self.0 += rhs;
            }
        }
        impl core::fmt::Display for PhysAddr {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                write!(f, "0x{:016x}", self.0)
            }
        }

        pub struct PageTableIndex(u16);
        impl PageTableIndex {
            pub fn new(index: u16) -> Self {
                Self(index % 512)
            }
        }
        impl From<PageTableIndex> for u16 {
            fn from(i: PageTableIndex) -> u16 {
                i.0
            }
        }
        impl From<PageTableIndex> for usize {
            fn from(i: PageTableIndex) -> usize {
                i.0 as usize
            }
        }
        pub struct PageOffset(u16);
        impl From<PageOffset> for u16 {
            fn from(o: PageOffset) -> u16 {
                o.0
            }
        }

        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Size4KiB {}
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Size2MiB {}
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Size1GiB {}

        #[derive(Clone, Copy, Debug)]
        pub struct Page<S = Size4KiB> {
            start_address: VirtAddr,
            _size: core::marker::PhantomData<S>,
        }
        impl<S> Page<S> {
            pub fn containing_address(addr: VirtAddr) -> Self {
                Self {
                    start_address: addr.align_down(4096),
                    _size: core::marker::PhantomData,
                }
            }
            pub fn start_address(&self) -> VirtAddr {
                self.start_address
            }
        }
        impl Page<Size4KiB> {
            pub const SIZE: u64 = 4096;
            pub fn range_inclusive(start: Self, end: Self) -> PageRangeInclusive {
                PageRangeInclusive { start, end }
            }
        }
        impl Page<Size2MiB> {
            pub const SIZE: u64 = 2 * 1024 * 1024;
        }

        pub mod mapper {
            pub use super::{MapToError, Mapper, MapperFlush, OffsetPageTable, UnmapError};
        }
        pub mod page_table {
            pub use super::{PageTable, PageTableEntry, PageTableFlags, PageTableIndex};
        }

        pub struct PageRangeInclusive<S = Size4KiB> {
            start: Page<S>,
            end: Page<S>,
        }
        impl Iterator for PageRangeInclusive<Size4KiB> {
            type Item = Page<Size4KiB>;
            fn next(&mut self) -> Option<Self::Item> {
                if self.start.start_address.as_u64() <= self.end.start_address.as_u64() {
                    let page = self.start;
                    self.start = Page {
                        start_address: VirtAddr::new(self.start.start_address.as_u64() + 4096),
                        _size: core::marker::PhantomData,
                    };
                    Some(page)
                } else {
                    None
                }
            }
        }

        #[derive(Clone, Copy, Debug)]
        pub struct PhysFrame<S = Size4KiB> {
            pub start_address: PhysAddr,
            pub _size: core::marker::PhantomData<S>,
        }
        impl<S> PhysFrame<S> {
            pub fn containing_address(addr: PhysAddr) -> Self {
                Self {
                    start_address: addr.align_down(4096),
                    _size: core::marker::PhantomData,
                }
            }
            pub fn start_address(&self) -> PhysAddr {
                self.start_address
            }
            pub fn from_start_address(addr: PhysAddr) -> Result<Self, ()> {
                Ok(Self {
                    start_address: addr,
                    _size: core::marker::PhantomData,
                })
            }
        }
        impl PhysFrame<Size4KiB> {
            pub const SIZE: u64 = 4096;
            pub fn range_inclusive(start: Self, end: Self) -> PhysFrameRangeInclusive {
                PhysFrameRangeInclusive { start, end }
            }
        }
        impl PhysFrame<Size2MiB> {
            pub const SIZE: u64 = 2 * 1024 * 1024;
        }
        pub struct PhysFrameRangeInclusive<S = Size4KiB> {
            pub start: PhysFrame<S>,
            pub end: PhysFrame<S>,
        }
        impl Iterator for PhysFrameRangeInclusive<Size4KiB> {
            type Item = PhysFrame<Size4KiB>;
            fn next(&mut self) -> Option<Self::Item> {
                if self.start.start_address.as_u64() <= self.end.start_address.as_u64() {
                    let frame = self.start;
                    self.start = PhysFrame {
                        start_address: PhysAddr::new(self.start.start_address.as_u64() + 4096),
                        _size: core::marker::PhantomData,
                    };
                    Some(frame)
                } else {
                    None
                }
            }
        }

        pub unsafe trait FrameAllocator<S> {
            fn allocate_frame(&mut self) -> Option<PhysFrame<S>>;
        }

        pub unsafe trait Mapper<S> {
            unsafe fn map_to(
                &mut self,
                page: Page<S>,
                frame: PhysFrame<S>,
                flags: PageTableFlags,
                frame_allocator: &mut impl FrameAllocator<Size4KiB>,
            ) -> Result<MapperFlush<S>, MapToError<S>>;
            fn unmap(
                &mut self,
                page: Page<S>,
            ) -> Result<(PhysFrame<S>, MapperFlush<S>), UnmapError>;
            fn translate_page(&self, page: Page<S>) -> Result<PhysFrame<S>, TranslateError>;
        }

        pub struct MapperFlush<S>(Page<S>);
        impl<S> MapperFlush<S> {
            pub fn flush(self) {}
            pub fn ignore(self) {}
        }

        pub enum MapToError<S> {
            FrameAllocationFailed,
            ParentEntryHugePage,
            PageAlreadyMapped(PhysFrame<S>),
        }
        impl<S> core::fmt::Debug for MapToError<S> {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                match self {
                    Self::FrameAllocationFailed => write!(f, "FrameAllocationFailed"),
                    Self::ParentEntryHugePage => write!(f, "ParentEntryHugePage"),
                    Self::PageAlreadyMapped(_) => write!(f, "PageAlreadyMapped"),
                }
            }
        }

        pub enum UnmapError {
            ParentEntryHugePage,
            PageNotMapped,
            InvalidFrameAddress(PhysFrame),
        }

        pub enum TranslateError {
            NotMapped,
            InvalidFrameAddress(PhysFrame),
        }

        pub struct OffsetPageTable<'a> {
            _phantom: core::marker::PhantomData<&'a ()>,
        }
        impl<'a> OffsetPageTable<'a> {
            pub unsafe fn new(_table: &'a mut PageTable, _offset: VirtAddr) -> Self {
                Self {
                    _phantom: core::marker::PhantomData,
                }
            }
            pub fn level_4_table(&self) -> &PageTable {
                static EMPTY: PageTable = PageTable {
                    entries: [PageTableEntry(0); 512],
                };
                &EMPTY
            }
        }
        unsafe impl Mapper<Size4KiB> for OffsetPageTable<'_> {
            unsafe fn map_to(
                &mut self,
                page: Page<Size4KiB>,
                frame: PhysFrame<Size4KiB>,
                _flags: PageTableFlags,
                _alloc: &mut impl FrameAllocator<Size4KiB>,
            ) -> Result<MapperFlush<Size4KiB>, MapToError<Size4KiB>> {
                Ok(MapperFlush(page))
            }
            fn unmap(
                &mut self,
                page: Page<Size4KiB>,
            ) -> Result<(PhysFrame<Size4KiB>, MapperFlush<Size4KiB>), UnmapError> {
                Err(UnmapError::PageNotMapped)
            }
            fn translate_page(
                &self,
                _page: Page<Size4KiB>,
            ) -> Result<PhysFrame<Size4KiB>, TranslateError> {
                Err(TranslateError::NotMapped)
            }
        }

        pub trait Translate {
            fn translate_addr(&self, addr: VirtAddr) -> Option<PhysAddr>;
        }
        impl Translate for OffsetPageTable<'_> {
            fn translate_addr(&self, _addr: VirtAddr) -> Option<PhysAddr> {
                None
            }
        }
    }

    pub mod idt {
        #[derive(Clone)]
        pub struct InterruptDescriptorTable {
            pub breakpoint: Entry,
            pub double_fault: Entry,
            pub page_fault: Entry,
            pub general_protection_fault: Entry,
            pub segment_not_present: Entry,
            _entries: [Entry; 256],
        }
        impl InterruptDescriptorTable {
            pub fn new() -> Self {
                Self {
                    breakpoint: Entry::new(),
                    double_fault: Entry::new(),
                    page_fault: Entry::new(),
                    general_protection_fault: Entry::new(),
                    segment_not_present: Entry::new(),
                    _entries: [Entry::new(); 256],
                }
            }
            pub fn load(&'static self) {}
        }
        impl core::ops::Index<u8> for InterruptDescriptorTable {
            type Output = Entry;
            fn index(&self, _idx: u8) -> &Entry {
                &self._entries[0]
            }
        }
        impl core::ops::IndexMut<u8> for InterruptDescriptorTable {
            fn index_mut(&mut self, idx: u8) -> &mut Entry {
                &mut self._entries[idx as usize]
            }
        }

        #[derive(Clone, Copy)]
        pub struct Entry {
            _data: [u64; 2],
        }
        impl Entry {
            pub const fn new() -> Self {
                Self { _data: [0; 2] }
            }
            pub fn set_handler_fn(&mut self, _handler: usize) -> &mut EntryOptions {
                static mut OPTS: EntryOptions = EntryOptions(0);
                unsafe { &mut OPTS }
            }
        }

        pub struct EntryOptions(u16);
        impl EntryOptions {
            pub fn set_stack_index(&mut self, _index: u16) -> &mut Self {
                self
            }
            pub fn disable_interrupts(&mut self, _disable: bool) -> &mut Self {
                self
            }
        }

        #[repr(C)]
        pub struct InterruptStackFrame {
            _data: [u64; 5],
        }
        impl core::fmt::Debug for InterruptStackFrame {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                write!(f, "InterruptStackFrame {{ ... }}")
            }
        }
        impl core::fmt::Display for InterruptStackFrame {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                write!(f, "InterruptStackFrame {{ ... }}")
            }
        }

        pub struct PageFaultErrorCode(u64);
        impl PageFaultErrorCode {
            pub const PROTECTION_VIOLATION: Self = Self(1);
            pub const CAUSED_BY_WRITE: Self = Self(2);
            pub const USER_MODE: Self = Self(4);
            pub const INSTRUCTION_FETCH: Self = Self(16);
        }
        impl core::fmt::Debug for PageFaultErrorCode {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                write!(f, "PageFaultErrorCode(0x{:x})", self.0)
            }
        }
    }

    pub mod tss {
        pub struct TaskStateSegment {
            pub privilege_stack_table: [super::paging::VirtAddr; 3],
            pub interrupt_stack_table: [super::paging::VirtAddr; 7],
        }
        impl TaskStateSegment {
            pub fn new() -> Self {
                Self {
                    privilege_stack_table: [super::paging::VirtAddr::new(0); 3],
                    interrupt_stack_table: [super::paging::VirtAddr::new(0); 7],
                }
            }
        }
    }

    pub mod gdt {
        pub struct GlobalDescriptorTable {
            _entries: [u64; 8],
            _len: usize,
        }
        impl GlobalDescriptorTable {
            pub fn new() -> Self {
                Self {
                    _entries: [0; 8],
                    _len: 1,
                }
            }
            pub fn append(&mut self, _descriptor: Descriptor) -> SegmentSelector {
                SegmentSelector(0)
            }
            pub fn load(&'static self) {}
        }

        pub enum Descriptor {
            KernelCodeSegment,
            KernelDataSegment,
            UserCodeSegment,
            UserDataSegment,
            TssSegment(&'static super::tss::TaskStateSegment),
        }
        impl Descriptor {
            pub fn kernel_code_segment() -> Self {
                Self::KernelCodeSegment
            }
            pub fn kernel_data_segment() -> Self {
                Self::KernelDataSegment
            }
            pub fn user_code_segment() -> Self {
                Self::UserCodeSegment
            }
            pub fn user_data_segment() -> Self {
                Self::UserDataSegment
            }
            pub fn tss_segment(tss: &'static super::tss::TaskStateSegment) -> Self {
                Self::TssSegment(tss)
            }
        }

        #[derive(Clone, Copy)]
        pub struct SegmentSelector(pub u16);
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub mod registers {
    pub mod control {
        use super::super::structures::paging::{PhysAddr, PhysFrame, Size4KiB};

        pub struct Cr3;
        impl Cr3 {
            pub fn read() -> (PhysFrame, u16) {
                (
                    PhysFrame {
                        start_address: PhysAddr::new(0),
                        _size: core::marker::PhantomData,
                    },
                    0,
                )
            }
            pub fn read_raw() -> (u64, u16) {
                (0, 0)
            }
            pub unsafe fn write(_frame: PhysFrame, _flags: u16) {}
        }

        pub struct Cr2;
        impl Cr2 {
            pub fn read() -> super::super::structures::paging::VirtAddr {
                super::super::structures::paging::VirtAddr::new(0)
            }
            pub fn read_raw() -> u64 {
                0
            }
        }
    }

    pub mod model_specific {
        pub struct Msr(u32);
        impl Msr {
            pub const fn new(reg: u32) -> Self {
                Self(reg)
            }
            pub unsafe fn read(&self) -> u64 {
                0
            }
            pub unsafe fn write(&mut self, _val: u64) {}
        }

        pub struct GsBase;
        impl GsBase {
            pub fn read() -> crate::arch_compat::structures::paging::VirtAddr {
                crate::arch_compat::structures::paging::VirtAddr::new(0)
            }
            pub fn write(_addr: crate::arch_compat::structures::paging::VirtAddr) {}
        }

        pub struct FsBase;
        impl FsBase {
            pub fn read() -> crate::arch_compat::structures::paging::VirtAddr {
                crate::arch_compat::structures::paging::VirtAddr::new(0)
            }
            pub fn write(_addr: crate::arch_compat::structures::paging::VirtAddr) {}
        }

        pub struct Efer;
        impl Efer {
            pub fn read_raw() -> u64 {
                0
            }
            pub unsafe fn write_raw(_val: u64) {}
        }

        pub struct EferFlags;
        impl EferFlags {
            pub const SYSTEM_CALL_EXTENSIONS: u64 = 1;
            pub const NO_EXECUTE_ENABLE: u64 = 1 << 11;
        }

        pub struct Star;
        impl Star {
            pub unsafe fn write_raw(
                _sysret_cs: u16,
                _sysret_ss: u16,
                _syscall_cs: u16,
                _syscall_ss: u16,
            ) {
            }
        }

        pub struct LStar;
        impl LStar {
            pub unsafe fn write(_addr: crate::arch_compat::structures::paging::VirtAddr) {}
        }

        pub struct SFMask;
        impl SFMask {
            pub unsafe fn write(_flags: u64) {}
        }
    }
}

// Re-export VirtAddr and PhysAddr at the crate root for convenience
#[cfg(not(target_arch = "x86_64"))]
pub use structures::paging::{PhysAddr, VirtAddr};

// ─── raw_cpuid stubs for non-x86_64 ────────────────────────────────
#[cfg(not(target_arch = "x86_64"))]
pub mod raw_cpuid {
    /// Stub CpuId – every query returns None.
    pub struct CpuId;
    impl CpuId {
        pub fn new() -> Self {
            CpuId
        }
        pub fn get_vendor_info(&self) -> Option<VendorInfo> {
            None
        }
        pub fn get_processor_brand_string(&self) -> Option<ProcessorBrandString> {
            None
        }
        pub fn get_feature_info(&self) -> Option<FeatureInfo> {
            None
        }
        pub fn get_extended_feature_info(&self) -> Option<ExtendedFeatureInfo> {
            None
        }
        pub fn get_processor_frequency_info(&self) -> Option<ProcessorFrequencyInfo> {
            None
        }
        pub fn get_tsc_info(&self) -> Option<TscInfo> {
            None
        }
        pub fn get_extended_processor_and_feature_identifiers(
            &self,
        ) -> Option<ExtendedProcessorFeatureIdentifiers> {
            None
        }
        pub fn get_cache_parameters(&self) -> Option<CacheParametersIter> {
            None
        }
        pub fn get_extended_topology_info(&self) -> Option<ExtendedTopologyIter> {
            None
        }
        pub fn get_thermal_power_info(&self) -> Option<ThermalPowerInfo> {
            None
        }
        pub fn get_hypervisor_info(&self) -> Option<HypervisorInfo> {
            None
        }
        pub fn get_advanced_power_mgmt_info(&self) -> Option<AdvancedPmInfo> {
            None
        }
    }

    pub struct VendorInfo;
    impl VendorInfo {
        pub fn as_str(&self) -> &str {
            "Unknown"
        }
    }

    pub struct ProcessorBrandString;
    impl ProcessorBrandString {
        pub fn as_str(&self) -> &str {
            "Unknown CPU"
        }
    }

    pub struct FeatureInfo;
    impl FeatureInfo {
        pub fn family_id(&self) -> u8 {
            0
        }
        pub fn model_id(&self) -> u8 {
            0
        }
        pub fn stepping_id(&self) -> u8 {
            0
        }
        pub fn max_logical_processor_ids(&self) -> u8 {
            1
        }
        pub fn has_apic(&self) -> bool {
            false
        }
        pub fn has_sse(&self) -> bool {
            false
        }
        pub fn has_sse2(&self) -> bool {
            false
        }
        pub fn has_sse3(&self) -> bool {
            false
        }
        pub fn has_ssse3(&self) -> bool {
            false
        }
        pub fn has_sse41(&self) -> bool {
            false
        }
        pub fn has_sse42(&self) -> bool {
            false
        }
        pub fn has_fpu(&self) -> bool {
            false
        }
        pub fn has_pse(&self) -> bool {
            false
        }
        pub fn has_msr(&self) -> bool {
            false
        }
        pub fn has_pae(&self) -> bool {
            false
        }
        pub fn has_rdseed(&self) -> bool {
            false
        }
        pub fn has_fma(&self) -> bool {
            false
        }
        pub fn has_avx(&self) -> bool {
            false
        }
        pub fn has_rdrand(&self) -> bool {
            false
        }
        pub fn has_vmx(&self) -> bool {
            false
        }
        pub fn has_pat(&self) -> bool {
            false
        }
    }

    pub struct ExtendedFeatureInfo;
    impl ExtendedFeatureInfo {
        pub fn has_avx2(&self) -> bool {
            false
        }
        pub fn has_avx512f(&self) -> bool {
            false
        }
        pub fn has_bmi1(&self) -> bool {
            false
        }
        pub fn has_bmi2(&self) -> bool {
            false
        }
        pub fn has_smep(&self) -> bool {
            false
        }
        pub fn has_smap(&self) -> bool {
            false
        }
        pub fn has_umip(&self) -> bool {
            false
        }
        pub fn has_rdseed(&self) -> bool {
            false
        }
    }

    pub struct ProcessorFrequencyInfo;
    impl ProcessorFrequencyInfo {
        pub fn processor_base_frequency(&self) -> u16 {
            0
        }
        pub fn processor_max_frequency(&self) -> u16 {
            0
        }
        pub fn bus_frequency(&self) -> u16 {
            0
        }
    }

    pub struct TscInfo;
    impl TscInfo {
        pub fn tsc_frequency(&self) -> Option<u64> {
            None
        }
    }

    pub struct ExtendedProcessorFeatureIdentifiers;
    impl ExtendedProcessorFeatureIdentifiers {
        pub fn has_1gib_pages(&self) -> bool {
            false
        }
        pub fn has_execute_disable(&self) -> bool {
            false
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum CacheType {
        Null,
        Data,
        Instruction,
        Unified,
    }

    pub struct CacheParameter;
    impl CacheParameter {
        pub fn level(&self) -> u8 {
            0
        }
        pub fn cache_type(&self) -> CacheType {
            CacheType::Null
        }
        pub fn sets(&self) -> usize {
            0
        }
        pub fn coherency_line_size(&self) -> usize {
            0
        }
        pub fn physical_line_partitions(&self) -> usize {
            0
        }
        pub fn associativity(&self) -> usize {
            0
        }
    }

    pub struct CacheParametersIter;
    impl Iterator for CacheParametersIter {
        type Item = CacheParameter;
        fn next(&mut self) -> Option<Self::Item> {
            None
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum TopologyType {
        Invalid,
        SMT,
        Core,
    }
    pub struct ExtendedTopologyLevel;
    impl ExtendedTopologyLevel {
        pub fn level_type(&self) -> TopologyType {
            TopologyType::Invalid
        }
        pub fn processors(&self) -> u32 {
            0
        }
    }
    pub struct ExtendedTopologyIter;
    impl Iterator for ExtendedTopologyIter {
        type Item = ExtendedTopologyLevel;
        fn next(&mut self) -> Option<Self::Item> {
            None
        }
    }

    pub struct ThermalPowerInfo;
    impl ThermalPowerInfo {
        pub fn has_hwp(&self) -> bool {
            false
        }
    }
    pub struct HypervisorInfo;
    pub struct AdvancedPmInfo;
}

/// Portable timestamp counter read.
#[cfg(target_arch = "x86_64")]
pub fn read_tsc() -> u64 {
    unsafe { core::arch::x86_64::_rdtsc() }
}

#[cfg(target_arch = "aarch64")]
pub fn read_tsc() -> u64 {
    let val: u64;
    unsafe { core::arch::asm!("mrs {}, cntvct_el0", out(reg) val) };
    val
}

#[cfg(target_arch = "riscv64")]
pub fn read_tsc() -> u64 {
    let val: u64;
    unsafe { core::arch::asm!("rdtime {}", out(reg) val) };
    val
}

// On x86_64, re-export the real crate types so `crate::arch_compat::*` works on all targets.
#[cfg(target_arch = "x86_64")]
pub mod instructions {
    pub mod port {
        pub use x86_64::instructions::port::{Port, PortReadOnly, PortWriteOnly};
    }
    pub mod interrupts {
        pub use x86_64::instructions::hlt;
        pub use x86_64::instructions::interrupts::{
            are_enabled, disable, enable, int3, without_interrupts,
        };
    }
    pub mod segmentation {
        pub use x86_64::instructions::segmentation::*;
    }
    pub mod tables {
        pub use x86_64::instructions::tables::*;
    }
    pub mod tlb {
        pub use x86_64::instructions::tlb::*;
    }
}

#[cfg(target_arch = "x86_64")]
pub mod structures {
    pub mod paging {
        pub use x86_64::PhysAddr;
        pub use x86_64::VirtAddr;
        pub use x86_64::structures::paging::*;
    }
    pub mod idt {
        pub use x86_64::structures::idt::*;
    }
    pub mod tss {
        pub use x86_64::structures::tss::*;
    }
    pub mod gdt {
        pub use x86_64::structures::gdt::*;
    }
}

#[cfg(target_arch = "x86_64")]
pub mod registers {
    pub mod control {
        pub use x86_64::registers::control::*;
    }
    pub mod model_specific {
        pub use x86_64::registers::model_specific::*;
    }
}

// On x86_64, re-export the real raw_cpuid crate.
#[cfg(target_arch = "x86_64")]
pub mod raw_cpuid {
    pub use ::raw_cpuid::*;
}

// Merge extension modules into instructions on non-x86_64
// (Users should use crate::arch_compat::instructions::segmentation etc.)

// ════════════════════════════════════════════════════════════════════════════
// bootloader_api shim for non-x86_64 targets
// ════════════════════════════════════════════════════════════════════════════

#[cfg(not(target_arch = "x86_64"))]
pub mod bootloader_shim {
    //! Minimal stubs for `bootloader_api` types used in the kernel.
    //! On x86_64 the real `bootloader_api` crate is used.

    pub mod info {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum MemoryRegionKind {
            Usable,
            Bootloader,
            UnknownBios(u32),
            UnknownUefi(u32),
        }

        #[repr(C)]
        pub struct MemoryRegion {
            pub start: u64,
            pub end: u64,
            pub kind: MemoryRegionKind,
        }

        /// Stub for MemoryRegions — an empty slice.
        pub struct MemoryRegions {
            _priv: (),
        }
        impl MemoryRegions {
            pub fn iter(&self) -> core::slice::Iter<'_, MemoryRegion> {
                [].iter()
            }
            pub fn get(&self, _index: usize) -> Option<&MemoryRegion> {
                None
            }
            pub fn len(&self) -> usize {
                0
            }
            pub fn is_empty(&self) -> bool {
                true
            }
        }

        /// Stub for Optional<T> from bootloader_api.
        pub enum Optional<T> {
            Some(T),
            None,
        }
        impl<T: Copy> Copy for Optional<T> {}
        impl<T: Clone> Clone for Optional<T> {
            fn clone(&self) -> Self {
                match self {
                    Optional::Some(v) => Optional::Some(v.clone()),
                    Optional::None => Optional::None,
                }
            }
        }
        impl<T> Optional<T> {
            pub fn as_mut(&mut self) -> Option<&mut T> {
                match self {
                    Optional::Some(ref mut v) => Some(v),
                    Optional::None => None,
                }
            }
            pub fn as_ref(&self) -> Option<&T> {
                match self {
                    Optional::Some(ref v) => Some(v),
                    Optional::None => None,
                }
            }
            pub fn into_option(self) -> Option<T> {
                match self {
                    Optional::Some(v) => Some(v),
                    Optional::None => None,
                }
            }
        }

        /// Stub FrameBuffer info
        pub struct FrameBufferInfo {
            pub byte_len: usize,
            pub width: usize,
            pub height: usize,
            pub bytes_per_pixel: usize,
            pub stride: usize,
            pub pixel_format: PixelFormat,
        }

        #[derive(Debug, Clone, Copy)]
        pub enum PixelFormat {
            Rgb,
            Bgr,
            U8,
        }

        /// Stub FrameBuffer
        pub struct FrameBuffer {
            _info: FrameBufferInfo,
        }
        impl FrameBuffer {
            pub fn info(&self) -> FrameBufferInfo {
                FrameBufferInfo {
                    byte_len: 0,
                    width: 0,
                    height: 0,
                    bytes_per_pixel: 4,
                    stride: 0,
                    pixel_format: PixelFormat::Bgr,
                }
            }
            pub fn buffer_mut(&mut self) -> &mut [u8] {
                &mut []
            }
            pub fn buffer(&self) -> &[u8] {
                &[]
            }
        }
    }

    pub mod config {
        pub struct BootloaderConfig {
            pub mappings: Mappings,
            pub frame_buffer: FrameBuffer,
        }
        impl BootloaderConfig {
            pub const fn new_default() -> Self {
                Self {
                    mappings: Mappings {
                        physical_memory: None,
                    },
                    frame_buffer: FrameBuffer {
                        minimum_framebuffer_width: None,
                        minimum_framebuffer_height: None,
                    },
                }
            }
        }
        pub struct Mappings {
            pub physical_memory: Option<Mapping>,
        }
        impl Mappings {
            pub const fn new_default() -> Self {
                Self {
                    physical_memory: None,
                }
            }
        }
        #[derive(Clone, Copy)]
        pub enum Mapping {
            Dynamic,
            FixedAddress(u64),
        }
        pub struct FrameBuffer {
            pub minimum_framebuffer_width: Option<u16>,
            pub minimum_framebuffer_height: Option<u16>,
        }
        impl FrameBuffer {
            pub const fn new_default() -> Self {
                Self {
                    minimum_framebuffer_width: None,
                    minimum_framebuffer_height: None,
                }
            }
        }
    }

    /// Stub BootInfo for non-x86_64.
    pub struct BootInfo {
        pub physical_memory_offset: info::Optional<u64>,
        pub framebuffer: info::Optional<info::FrameBuffer>,
        pub memory_regions: info::MemoryRegions,
    }
}
