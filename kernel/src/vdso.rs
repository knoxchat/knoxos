/// vDSO — Virtual Dynamic Shared Object
///
/// The vDSO is a small shared library that the kernel maps into every user process's
/// address space. It provides fast user-space implementations of certain syscalls
/// (like clock_gettime) that can be performed without a context switch.
///
/// Linux-compatible: maps at a high virtual address, exports symbols for:
///   - __vdso_clock_gettime
///   - __vdso_gettimeofday
///   - __vdso_time
///   - __vdso_getcpu
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::process::Pid;
use crate::serial_println;

// ─── vDSO Page Layout ─────────────────────────────────────────────────

/// Virtual address where vDSO is mapped in every user process
pub const VDSO_BASE: u64 = 0x0000_7FFF_F7FF_0000;
/// Size of the vDSO region (one page)
pub const VDSO_SIZE: u64 = 4096;

/// Shared data page (updated by kernel, read by user-space vDSO code)
pub const VVAR_BASE: u64 = VDSO_BASE - 4096;
pub const VVAR_SIZE: u64 = 4096;

/// vDSO data page — shared between kernel and user-space
/// Kernel writes timing data here; vDSO code reads it without syscall
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VdsoData {
    /// Sequence counter (even = stable, odd = being updated)
    pub seq: u64,
    /// CLOCK_REALTIME seconds
    pub realtime_sec: u64,
    /// CLOCK_REALTIME nanoseconds
    pub realtime_nsec: u64,
    /// CLOCK_MONOTONIC seconds
    pub monotonic_sec: u64,
    /// CLOCK_MONOTONIC nanoseconds
    pub monotonic_nsec: u64,
    /// TSC frequency (Hz) for user-space RDTSC interpolation
    pub tsc_frequency: u64,
    /// TSC value at last update
    pub tsc_at_update: u64,
    /// Boot time seconds (for CLOCK_BOOTTIME)
    pub boottime_sec: u64,
    /// System timezone offset in seconds from UTC
    pub tz_offset: i32,
    /// DST flag
    pub tz_dst: i32,
    /// Current CPU ID (for getcpu)
    pub cpu_id: u32,
    /// NUMA node
    pub numa_node: u32,
}

impl Default for VdsoData {
    fn default() -> Self {
        Self::new()
    }
}

impl VdsoData {
    pub const fn new() -> Self {
        Self {
            seq: 0,
            realtime_sec: 0,
            realtime_nsec: 0,
            monotonic_sec: 0,
            monotonic_nsec: 0,
            tsc_frequency: 0,
            tsc_at_update: 0,
            boottime_sec: 0,
            tz_offset: 0,
            tz_dst: 0,
            cpu_id: 0,
            numa_node: 0,
        }
    }
}

lazy_static::lazy_static! {
    /// Global vDSO data — kernel updates this, user-space reads it
    pub static ref VDSO_DATA: Mutex<VdsoData> = Mutex::new(VdsoData::new());
}

/// Physical frame holding the vDSO code page
static VDSO_CODE_FRAME: AtomicU64 = AtomicU64::new(0);
/// Physical frame holding the vvar data page
static VVAR_DATA_FRAME: AtomicU64 = AtomicU64::new(0);

/// x86_64 machine code for vDSO functions
/// These are minimal implementations that read from the vvar page
///
/// __vdso_clock_gettime(clockid_t clock_id, struct timespec *tp):
///   Reads realtime or monotonic from vvar page
///
/// __vdso_gettimeofday(struct timeval *tv, struct timezone *tz):
///   Reads realtime from vvar page, converts to timeval
///
/// __vdso_time(time_t *tloc):
///   Returns realtime seconds
///
/// __vdso_getcpu(unsigned *cpu, unsigned *node, void *unused):
///   Returns CPU and NUMA node from vvar page
fn generate_vdso_code() -> [u8; 4096] {
    let mut code = [0u8; 4096];

    // The vvar page is mapped at VVAR_BASE (one page below VDSO_BASE).
    // vDSO functions read VdsoData fields using a seqcount protocol:
    //   1. Read seq (must be even = stable)
    //   2. Read data fields
    //   3. Re-read seq, retry if changed
    //
    // Memory layout of VdsoData (offset from vvar page start):
    //   0x00: seq          (u64)
    //   0x08: realtime_sec (u64)
    //   0x10: realtime_nsec(u64)
    //   0x18: monotonic_sec(u64)
    //   0x20: monotonic_nsec(u64)
    //   0x28: tsc_frequency(u64)
    //   0x30: tsc_at_update(u64)
    //   0x38: boottime_sec (u64)
    //   0x40: tz_offset    (i32)
    //   0x44: tz_dst       (i32)
    //   0x48: cpu_id       (u32)
    //   0x4C: numa_node    (u32)

    // The vvar page address relative to the vDSO code page
    // = VVAR_BASE.  We encode it as an absolute address since
    // both pages are at fixed virtual addresses.
    let vvar_lo = (VVAR_BASE & 0xFFFF_FFFF) as u32;
    let vvar_hi = (VVAR_BASE >> 32) as u32;

    // ──────────────────────────────────────────────────────────────
    // Offset 0x000: __vdso_clock_gettime(clock_id: rdi, tp: rsi)
    // Reads from vvar page with seqcount retry.
    // clock_id: 0 = REALTIME, 1 = MONOTONIC
    // tp->tv_sec at [rsi], tp->tv_nsec at [rsi+8]
    // ──────────────────────────────────────────────────────────────
    let clock_gettime: &[u8] = &[
        // mov r8, VVAR_BASE  (movabs)
        0x49,
        0xb8,
        vvar_lo as u8,
        (vvar_lo >> 8) as u8,
        (vvar_lo >> 16) as u8,
        (vvar_lo >> 24) as u8,
        vvar_hi as u8,
        (vvar_hi >> 8) as u8,
        (vvar_hi >> 16) as u8,
        (vvar_hi >> 24) as u8,
        // .Lretry:
        // mov rax, [r8+0]     ; seq
        0x49,
        0x8b,
        0x00,
        // test al, 1          ; odd = updating
        0xa8,
        0x01,
        // jnz .Lretry  (jump back 5 bytes to mov rax)
        0x75,
        0xf9,
        // mov rcx, rax        ; save seq
        0x48,
        0x89,
        0xc1,
        // cmp edi, 1          ; clock_id == MONOTONIC?
        0x83,
        0xff,
        0x01,
        // je .Lmono (skip 2 movs = 8 bytes ahead)
        0x74,
        0x08,
        // mov rdx, [r8+0x08]  ; realtime_sec
        0x49,
        0x8b,
        0x50,
        0x08,
        // mov r9,  [r8+0x10]  ; realtime_nsec
        0x4d,
        0x8b,
        0x48,
        0x10,
        // jmp .Ldone (skip mono = 8 bytes ahead)
        0xeb,
        0x08,
        // .Lmono:
        // mov rdx, [r8+0x18]  ; monotonic_sec
        0x49,
        0x8b,
        0x50,
        0x18,
        // mov r9,  [r8+0x20]  ; monotonic_nsec
        0x4d,
        0x8b,
        0x48,
        0x20,
        // .Ldone:
        // mov rax, [r8+0]     ; re-read seq
        0x49,
        0x8b,
        0x00,
        // cmp rax, rcx        ; changed?
        0x48,
        0x39,
        0xc8,
        // jne .Lretry (back to mov rax,[r8] = offset 10 from start)
        // (distance = current - target)  — use syscall fallback if loop
        // exceeds attempts
        0x75,
        0xd4,
        // mov [rsi], rdx      ; tp->tv_sec
        0x48,
        0x89,
        0x16,
        // mov [rsi+8], r9     ; tp->tv_nsec
        0x4c,
        0x89,
        0x4e,
        0x08,
        // xor eax, eax        ; return 0
        0x31,
        0xc0,
        // ret
        0xc3,
    ];
    code[..clock_gettime.len()].copy_from_slice(clock_gettime);

    // Offset 0x060: __vdso_gettimeofday  — reads realtime from vvar
    let gettimeofday: &[u8] = &[
        // mov r8, VVAR_BASE
        0x49,
        0xb8,
        vvar_lo as u8,
        (vvar_lo >> 8) as u8,
        (vvar_lo >> 16) as u8,
        (vvar_lo >> 24) as u8,
        vvar_hi as u8,
        (vvar_hi >> 8) as u8,
        (vvar_hi >> 16) as u8,
        (vvar_hi >> 24) as u8,
        // mov rax, [r8+0x08]  ; realtime_sec
        0x49,
        0x8b,
        0x40,
        0x08,
        // mov [rdi], rax      ; tv->tv_sec
        0x48,
        0x89,
        0x07,
        // mov rax, [r8+0x10]  ; realtime_nsec
        0x49,
        0x8b,
        0x40,
        0x10,
        // Convert nsec to usec: shr rax, 10  (approx /1024, close to /1000)
        // More accurate: we just store nsec/1000
        // mov rcx, 1000
        0x48,
        0xc7,
        0xc1,
        0xe8,
        0x03,
        0x00,
        0x00,
        // xor edx, edx
        0x31,
        0xd2,
        // div rcx
        0x48,
        0xf7,
        0xf1,
        // mov [rdi+8], rax    ; tv->tv_usec
        0x48,
        0x89,
        0x47,
        0x08,
        // xor eax, eax
        0x31,
        0xc0,
        // ret
        0xc3,
    ];
    code[0x060..0x060 + gettimeofday.len()].copy_from_slice(gettimeofday);

    // Offset 0x0A0: __vdso_time(time_t *tloc) — reads realtime_sec from vvar
    let time: &[u8] = &[
        // mov r8, VVAR_BASE
        0x49,
        0xb8,
        vvar_lo as u8,
        (vvar_lo >> 8) as u8,
        (vvar_lo >> 16) as u8,
        (vvar_lo >> 24) as u8,
        vvar_hi as u8,
        (vvar_hi >> 8) as u8,
        (vvar_hi >> 16) as u8,
        (vvar_hi >> 24) as u8,
        // mov rax, [r8+0x08]  ; realtime_sec
        0x49,
        0x8b,
        0x40,
        0x08,
        // test rdi, rdi       ; tloc == NULL?
        0x48,
        0x85,
        0xff,
        // jz .Lskip
        0x74,
        0x03,
        // mov [rdi], rax
        0x48,
        0x89,
        0x07,
        // .Lskip:
        // ret
        0xc3,
    ];
    code[0x0A0..0x0A0 + time.len()].copy_from_slice(time);

    // Offset 0x0D0: __vdso_getcpu(cpu*, node*, unused) — reads from vvar
    let getcpu: &[u8] = &[
        // mov r8, VVAR_BASE
        0x49,
        0xb8,
        vvar_lo as u8,
        (vvar_lo >> 8) as u8,
        (vvar_lo >> 16) as u8,
        (vvar_lo >> 24) as u8,
        vvar_hi as u8,
        (vvar_hi >> 8) as u8,
        (vvar_hi >> 16) as u8,
        (vvar_hi >> 24) as u8,
        // test rdi, rdi
        0x48,
        0x85,
        0xff,
        // jz .Lskip_cpu
        0x74,
        0x06,
        // mov eax, [r8+0x48]  ; cpu_id
        0x41,
        0x8b,
        0x40,
        0x48,
        // mov [rdi], eax
        0x89,
        0x07,
        // .Lskip_cpu:
        // test rsi, rsi
        0x48,
        0x85,
        0xf6,
        // jz .Lskip_node
        0x74,
        0x06,
        // mov eax, [r8+0x4C]  ; numa_node
        0x41,
        0x8b,
        0x40,
        0x4c,
        // mov [rsi], eax
        0x89,
        0x06,
        // .Lskip_node:
        // xor eax, eax
        0x31,
        0xc0,
        // ret
        0xc3,
    ];
    code[0x0D0..0x0D0 + getcpu.len()].copy_from_slice(getcpu);

    code
}

/// Map the vDSO into a process's address space
pub fn map_vdso_for_process(pid: Pid) -> bool {
    let code_frame = VDSO_CODE_FRAME.load(Ordering::Relaxed);
    let data_frame = VVAR_DATA_FRAME.load(Ordering::Relaxed);

    if code_frame == 0 || data_frame == 0 {
        return false;
    }

    let mut spaces = crate::vmm::ADDRESS_SPACES.lock();
    if let Some(addr_space) = spaces.get_mut(&pid) {
        // Map vvar data page (read-only for user)
        let vvar_flags = crate::arch_compat::structures::paging::PageTableFlags::PRESENT
            | crate::arch_compat::structures::paging::PageTableFlags::USER_ACCESSIBLE
            | crate::arch_compat::structures::paging::PageTableFlags::NO_EXECUTE;
        unsafe {
            crate::vmm::map_page_in_table_pub(addr_space.cr3, VVAR_BASE, data_frame, vvar_flags);
        }

        // Map vDSO code page (read + execute for user)
        let vdso_flags = crate::arch_compat::structures::paging::PageTableFlags::PRESENT
            | crate::arch_compat::structures::paging::PageTableFlags::USER_ACCESSIBLE;
        unsafe {
            crate::vmm::map_page_in_table_pub(addr_space.cr3, VDSO_BASE, code_frame, vdso_flags);
        }

        // Track VMAs
        addr_space.add_vma(crate::vmm::VirtualMemoryArea {
            start: VVAR_BASE,
            end: VVAR_BASE + VVAR_SIZE,
            prot: crate::vmm::ProtFlags::R,
            mapping_type: crate::vmm::MappingType::Shared,
            flags: crate::vmm::MmapFlags {
                shared: true,
                anonymous: false,
                fixed: true,
                populate: true,
            },
            file_path: Some(alloc::string::String::from("[vvar]")),
            file_offset: 0,
            cow: false,
            ref_count: 1,
        });

        addr_space.add_vma(crate::vmm::VirtualMemoryArea {
            start: VDSO_BASE,
            end: VDSO_BASE + VDSO_SIZE,
            prot: crate::vmm::ProtFlags::RX,
            mapping_type: crate::vmm::MappingType::Shared,
            flags: crate::vmm::MmapFlags {
                shared: true,
                anonymous: false,
                fixed: true,
                populate: true,
            },
            file_path: Some(alloc::string::String::from("[vdso]")),
            file_offset: 0,
            cow: false,
            ref_count: 1,
        });

        return true;
    }

    false
}

/// Update vDSO timing data (called from timer interrupt or clock update)
pub fn update_vdso_time() {
    let mut data = VDSO_DATA.lock();

    // Begin update (odd sequence = updating)
    data.seq = data.seq.wrapping_add(1);

    // Read current time from RTC and monotonic clock
    let rtc_secs = crate::rtc::unix_time();
    let tsc = crate::arch_compat::read_tsc();

    data.realtime_sec = rtc_secs as u64;
    data.realtime_nsec = 0; // Sub-second precision from TSC interpolation
    data.monotonic_sec = rtc_secs as u64; // Simplified: same as realtime for now
    data.monotonic_nsec = 0;
    data.tsc_at_update = tsc;

    // End update (even sequence = stable)
    data.seq = data.seq.wrapping_add(1);
}

/// Get the vDSO base address (for AT_SYSINFO_EHDR in auxv)
pub fn get_vdso_base() -> u64 {
    VDSO_BASE
}

/// Initialize the vDSO subsystem
pub fn init() {
    // Allocate physical frames for vDSO code and vvar data
    if let Some(code_frame) = crate::vmm::allocate_physical_frame() {
        VDSO_CODE_FRAME.store(code_frame, Ordering::Relaxed);

        // Write vDSO code into the frame
        let offset = crate::vmm::get_phys_mem_offset();
        if offset != 0 {
            let code = generate_vdso_code();
            unsafe {
                let ptr = (offset + code_frame) as *mut u8;
                core::ptr::copy_nonoverlapping(code.as_ptr(), ptr, 4096);
            }
        }
    }

    if let Some(data_frame) = crate::vmm::allocate_physical_frame() {
        VVAR_DATA_FRAME.store(data_frame, Ordering::Relaxed);

        // Zero the vvar page
        let offset = crate::vmm::get_phys_mem_offset();
        if offset != 0 {
            unsafe {
                let ptr = (offset + data_frame) as *mut u8;
                core::ptr::write_bytes(ptr, 0, 4096);
            }
        }
    }

    // Initial time update
    update_vdso_time();

    serial_println!("[vDSO] Virtual Dynamic Shared Object initialized");
    serial_println!("[vDSO]   vvar page: {:#x}", VVAR_BASE);
    serial_println!("[vDSO]   vDSO code: {:#x}", VDSO_BASE);
}
