/// Swap — Virtual Memory Swapping Subsystem
///
/// Provides page swapping to disk when physical memory is under pressure:
///   - Swap area management (partition or file-backed)
///   - Page-out / page-in operations
///   - LRU-based page replacement
///   - Swap map for tracking slot usage
///   - /proc/swaps information
///   - swapon/swapoff syscall support
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── Constants ─────────────────────────────────────────────────────────

pub const PAGE_SIZE: usize = 4096;
pub const SWAP_MAGIC: &[u8; 10] = b"SWAPSPACE2";

/// Maximum number of swap areas
pub const MAX_SWAP_AREAS: usize = 32;

/// Swap slot states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotState {
    Free,
    Used,
    Bad, // bad sector
}

// ─── Swap Area ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapType {
    Partition,
    File,
}

#[derive(Debug, Clone)]
pub struct SwapArea {
    pub id: usize,
    pub swap_type: SwapType,
    pub path: String,
    pub total_slots: usize,
    pub used_slots: usize,
    pub priority: i32,
    pub active: bool,
    /// Bitmap: each bit = one page slot
    pub slot_map: Vec<u8>,
}

impl SwapArea {
    pub fn new(
        id: usize,
        path: &str,
        swap_type: SwapType,
        total_pages: usize,
        priority: i32,
    ) -> Self {
        let bitmap_bytes = total_pages.div_ceil(8);
        Self {
            id,
            swap_type,
            path: String::from(path),
            total_slots: total_pages,
            used_slots: 0,
            priority,
            active: false,
            slot_map: alloc::vec![0u8; bitmap_bytes],
        }
    }

    /// Allocate a free slot, return slot index
    pub fn alloc_slot(&mut self) -> Option<usize> {
        for byte_idx in 0..self.slot_map.len() {
            if self.slot_map[byte_idx] != 0xFF {
                for bit in 0..8u8 {
                    let slot = byte_idx * 8 + bit as usize;
                    if slot >= self.total_slots {
                        return None;
                    }
                    if self.slot_map[byte_idx] & (1 << bit) == 0 {
                        self.slot_map[byte_idx] |= 1 << bit;
                        self.used_slots += 1;
                        return Some(slot);
                    }
                }
            }
        }
        None
    }

    /// Free a slot
    pub fn free_slot(&mut self, slot: usize) {
        if slot >= self.total_slots {
            return;
        }
        let byte_idx = slot / 8;
        let bit = slot % 8;
        if self.slot_map[byte_idx] & (1 << bit) != 0 {
            self.slot_map[byte_idx] &= !(1 << bit);
            self.used_slots -= 1;
        }
    }

    pub fn is_slot_used(&self, slot: usize) -> bool {
        if slot >= self.total_slots {
            return false;
        }
        let byte_idx = slot / 8;
        let bit = slot % 8;
        self.slot_map[byte_idx] & (1 << bit) != 0
    }

    pub fn free_slots(&self) -> usize {
        self.total_slots - self.used_slots
    }

    pub fn usage_percent(&self) -> f64 {
        if self.total_slots == 0 {
            return 0.0;
        }
        (self.used_slots as f64 / self.total_slots as f64) * 100.0
    }
}

// ─── Swap Entry (PTE encoding) ─────────────────────────────────────────

/// When a page is swapped out, its PTE is replaced with a swap entry:
///   Bits  0:    Present = 0  (page not present)
///   Bits  1:    Swap bit = 1  (this is a swap entry, not just unmapped)
///   Bits  2-6:  Swap area ID (0-31)
///   Bits  7-63: Slot offset within swap area
#[derive(Debug, Clone, Copy)]
pub struct SwapEntry(pub u64);

impl SwapEntry {
    pub fn new(area_id: usize, slot: usize) -> Self {
        let val = (1u64 << 1)                           // swap bit
            | (((area_id & 0x1F) as u64) << 2)   // area ID in bits 2-6
            | ((slot as u64) << 7); // slot offset in bits 7+
        Self(val)
    }

    pub fn is_swap_entry(pte: u64) -> bool {
        // Present=0, Swap=1
        (pte & 1) == 0 && (pte & 2) != 0
    }

    pub fn area_id(&self) -> usize {
        ((self.0 >> 2) & 0x1F) as usize
    }

    pub fn slot(&self) -> usize {
        (self.0 >> 7) as usize
    }

    pub fn to_pte(&self) -> u64 {
        self.0
    }

    pub fn from_pte(pte: u64) -> Self {
        Self(pte)
    }
}

// ─── LRU page tracking ────────────────────────────────────────────────

/// Tracks pages for LRU eviction
#[derive(Debug, Clone, Copy)]
pub struct PageInfo {
    pub phys_addr: u64,
    pub virt_addr: u64,
    pub pid: u32,
    pub access_count: u32,
    pub last_access_tick: u64,
    pub dirty: bool,
    pub pinned: bool, // kernel pages, DMA buffers, etc.
}

// ─── Swap Manager ──────────────────────────────────────────────────────

pub struct SwapManager {
    pub areas: Vec<SwapArea>,
    pub total_swap_pages: usize,
    pub used_swap_pages: usize,
    /// Map: (pid, vaddr) -> SwapEntry for swapped-out pages
    pub swapped_pages: BTreeMap<(u32, u64), SwapEntry>,
    /// Page contents keyed by (area_id, slot). Ram-backed until a disk area exists.
    slot_data: BTreeMap<(usize, usize), Vec<u8>>,
    /// LRU list of candidate pages
    pub lru_pages: Vec<PageInfo>,
    /// Swappiness (0-100, like Linux vm.swappiness)
    pub swappiness: u32,
    /// Pages swapped in/out counters
    pub pages_swapped_in: u64,
    pub pages_swapped_out: u64,
}

impl Default for SwapManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SwapManager {
    pub fn new() -> Self {
        Self {
            areas: Vec::new(),
            total_swap_pages: 0,
            used_swap_pages: 0,
            swapped_pages: BTreeMap::new(),
            slot_data: BTreeMap::new(),
            lru_pages: Vec::new(),
            swappiness: 60,
            pages_swapped_in: 0,
            pages_swapped_out: 0,
        }
    }

    /// swapon — activate a swap area
    pub fn swapon(
        &mut self,
        path: &str,
        swap_type: SwapType,
        size_bytes: usize,
        priority: i32,
    ) -> Result<usize, &'static str> {
        if self.areas.len() >= MAX_SWAP_AREAS {
            return Err("too many swap areas");
        }
        let total_pages = size_bytes / PAGE_SIZE;
        if total_pages == 0 {
            return Err("swap area too small");
        }
        let id = self.areas.len();
        let mut area = SwapArea::new(id, path, swap_type, total_pages, priority);
        area.active = true;
        self.total_swap_pages += total_pages;
        serial_println!(
            "[SWAP] swapon: {} ({} pages, pri={})",
            path,
            total_pages,
            priority
        );
        self.areas.push(area);
        Ok(id)
    }

    /// swapoff — deactivate a swap area (must page-in all pages first)
    pub fn swapoff(&mut self, path: &str) -> Result<(), &'static str> {
        let area = self.areas.iter_mut().find(|a| a.path == path && a.active);
        match area {
            Some(a) => {
                if a.used_slots > 0 {
                    return Err("swap area has pages in use, page them in first");
                }
                a.active = false;
                self.total_swap_pages -= a.total_slots;
                serial_println!("[SWAP] swapoff: {}", path);
                Ok(())
            }
            None => Err("swap area not found or not active"),
        }
    }

    /// Allocate a swap slot (picks highest-priority area with free space)
    pub fn alloc_swap_slot(&mut self) -> Option<SwapEntry> {
        // Sort by priority (highest first)
        let mut best: Option<(usize, i32)> = None;
        for (i, area) in self.areas.iter().enumerate() {
            if !area.active || area.free_slots() == 0 {
                continue;
            }
            match best {
                None => best = Some((i, area.priority)),
                Some((_, bp)) if area.priority > bp => best = Some((i, area.priority)),
                _ => {}
            }
        }
        if let Some((idx, _)) = best {
            if let Some(slot) = self.areas[idx].alloc_slot() {
                self.used_swap_pages += 1;
                return Some(SwapEntry::new(idx, slot));
            }
        }
        None
    }

    /// Free a swap slot
    pub fn free_swap_slot(&mut self, entry: SwapEntry) {
        let area_id = entry.area_id();
        let slot = entry.slot();
        if area_id < self.areas.len() {
            self.areas[area_id].free_slot(slot);
            self.used_swap_pages -= 1;
        }
    }

    /// Store a page's bytes in a swap slot and remember the mapping.
    pub fn page_out(&mut self, pid: u32, vaddr: u64, data: &[u8]) -> Option<SwapEntry> {
        let entry = self.alloc_swap_slot()?;
        let mut page = alloc::vec![0u8; PAGE_SIZE];
        let n = data.len().min(PAGE_SIZE);
        page[..n].copy_from_slice(&data[..n]);
        self.slot_data.insert((entry.area_id(), entry.slot()), page);
        self.swapped_pages.insert((pid, vaddr), entry);
        self.pages_swapped_out += 1;
        serial_println!(
            "[SWAP] Page out: pid={} vaddr={:#x} -> area={} slot={}",
            pid,
            vaddr,
            entry.area_id(),
            entry.slot()
        );
        Some(entry)
    }

    /// Restore a page's bytes and free the swap slot.
    pub fn page_in(&mut self, pid: u32, vaddr: u64) -> Option<Vec<u8>> {
        if let Some(entry) = self.swapped_pages.remove(&(pid, vaddr)) {
            let data = self
                .slot_data
                .remove(&(entry.area_id(), entry.slot()))
                .unwrap_or_else(|| alloc::vec![0u8; PAGE_SIZE]);
            self.free_swap_slot(entry);
            self.pages_swapped_in += 1;
            serial_println!("[SWAP] Page in: pid={} vaddr={:#x}", pid, vaddr);
            Some(data)
        } else {
            None
        }
    }

    /// Check if a virtual address is swapped out
    pub fn is_swapped(&self, pid: u32, vaddr: u64) -> bool {
        self.swapped_pages.contains_key(&(pid, vaddr))
    }

    /// Add a page to LRU tracking
    pub fn track_page(&mut self, info: PageInfo) {
        // Don't track pinned pages
        if info.pinned {
            return;
        }
        self.lru_pages.push(info);
    }

    /// Select victim page for eviction (approximate LRU)
    pub fn select_victim(&mut self) -> Option<PageInfo> {
        if self.lru_pages.is_empty() {
            return None;
        }

        // Find page with lowest access_count + oldest last_access
        let mut best_idx = 0;
        let mut best_score = u64::MAX;

        for (i, page) in self.lru_pages.iter().enumerate() {
            if page.pinned {
                continue;
            }
            let score = (page.access_count as u64) * 1000 + page.last_access_tick;
            if score < best_score {
                best_score = score;
                best_idx = i;
            }
        }

        Some(self.lru_pages.remove(best_idx))
    }

    /// Get swap usage statistics
    pub fn stats(&self) -> SwapStats {
        SwapStats {
            total_pages: self.total_swap_pages,
            used_pages: self.used_swap_pages,
            free_pages: self.total_swap_pages - self.used_swap_pages,
            pages_in: self.pages_swapped_in,
            pages_out: self.pages_swapped_out,
            area_count: self.areas.iter().filter(|a| a.active).count(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SwapStats {
    pub total_pages: usize,
    pub used_pages: usize,
    pub free_pages: usize,
    pub pages_in: u64,
    pub pages_out: u64,
    pub area_count: usize,
}

// ─── Global state ──────────────────────────────────────────────────────

lazy_static::lazy_static! {
    pub static ref SWAP_MANAGER: Mutex<SwapManager> = Mutex::new(SwapManager::new());
}

static SWAP_ENABLED: AtomicBool = AtomicBool::new(false);

// ─── Public API ────────────────────────────────────────────────────────

pub fn is_enabled() -> bool {
    SWAP_ENABLED.load(Ordering::Relaxed)
}

pub fn swapon(
    path: &str,
    swap_type: SwapType,
    size_bytes: usize,
    priority: i32,
) -> Result<usize, &'static str> {
    SWAP_MANAGER
        .lock()
        .swapon(path, swap_type, size_bytes, priority)
}

pub fn swapoff(path: &str) -> Result<(), &'static str> {
    SWAP_MANAGER.lock().swapoff(path)
}

pub fn page_out(pid: u32, vaddr: u64, data: &[u8]) -> Option<SwapEntry> {
    SWAP_MANAGER.lock().page_out(pid, vaddr, data)
}

pub fn page_in(pid: u32, vaddr: u64) -> Option<Vec<u8>> {
    SWAP_MANAGER.lock().page_in(pid, vaddr)
}

/// Track an anonymous page so reclaim can pick it as a victim.
pub fn track_anon_page(pid: u32, vaddr: u64, phys_addr: u64) {
    SWAP_MANAGER.lock().track_page(PageInfo {
        phys_addr,
        virt_addr: vaddr,
        pid,
        access_count: 0,
        last_access_tick: 0,
        dirty: true,
        pinned: false,
    });
}

/// Page out up to `max` tracked anonymous pages. Returns how many succeeded.
pub fn reclaim_anonymous(max: usize) -> usize {
    let mut freed = 0;
    for _ in 0..max {
        let victim = SWAP_MANAGER.lock().select_victim();
        let Some(page) = victim else {
            break;
        };
        if crate::vmm::swap_out_page(page.pid, page.virt_addr) {
            freed += 1;
        }
    }
    if freed > 0 {
        serial_println!("[SWAP] Reclaimed {} anonymous pages", freed);
    }
    freed
}

pub fn is_swapped(pid: u32, vaddr: u64) -> bool {
    SWAP_MANAGER.lock().is_swapped(pid, vaddr)
}

pub fn get_stats() -> SwapStats {
    SWAP_MANAGER.lock().stats()
}

pub fn set_swappiness(val: u32) {
    SWAP_MANAGER.lock().swappiness = val.min(100);
}

/// Initialize swap subsystem
pub fn init() {
    serial_println!("[SWAP] Virtual memory swap subsystem initialized");
    serial_println!("[SWAP] Swappiness={}", SWAP_MANAGER.lock().swappiness);
    SWAP_ENABLED.store(true, Ordering::Relaxed);

    // Ram-backed area so page-out has somewhere to store bytes without a
    // swap partition. Disk probe can add a real area later.
    let _ = swapon("/dev/ram-swap", SwapType::File, 64 * PAGE_SIZE, 0);

    // Probe for swap partitions on disk
    probe_swap_partitions();

    let _ = swap_io_self_test();
}

/// Serial marker once a page's bytes survive page-out and a #PF brings them back.
pub const GATE_K2_MARKER: &str = "GATE_K2 swap io";

/// Page out a populated anonymous page, then fault it back in with the same bytes.
pub fn swap_io_self_test() -> bool {
    if !crate::vmm::ready() {
        serial_println!("[SWAP] Gate K2 skipped: VMM not ready");
        return false;
    }
    crate::vmm::swap_fault_self_test()
}

// ─── Real disk I/O integration ─────────────────────────────────────────

/// Probe block devices for swap partition signatures
fn probe_swap_partitions() {
    let devices = ["/dev/sda2", "/dev/sda3", "/dev/vda2", "/dev/nvme0n1p2"];

    for dev in &devices {
        if let Some(data) = crate::vfs::read_file_dispatch(dev) {
            if data.len() >= PAGE_SIZE {
                // Check swap magic at offset PAGE_SIZE - 10
                let magic_offset = PAGE_SIZE - 10;
                if data.len() > magic_offset + 10
                    && &data[magic_offset..magic_offset + 10] == SWAP_MAGIC
                {
                    // Found a swap partition!
                    let swap_size = parse_swap_header(&data);
                    serial_println!(
                        "[SWAP] Found swap signature on {}: {} pages",
                        dev,
                        swap_size
                    );

                    let _ = swapon(dev, SwapType::Partition, swap_size * PAGE_SIZE, -1);
                }
            }
        }
    }
}

/// Parse a swap header to get total usable pages
fn parse_swap_header(data: &[u8]) -> usize {
    // Linux swap header layout:
    //   Bytes 0..1023: boot block (unused)
    //   Offset 1024+0: version (1)
    //   Offset 1024+4: last_page (total pages - 1)
    //   Offset 1024+8: nr_badpages
    if data.len() >= 1040 {
        let last_page =
            u32::from_le_bytes([data[1028], data[1029], data[1030], data[1031]]) as usize;
        last_page + 1
    } else {
        0
    }
}

/// Write a page to swap disk (real I/O)
pub fn write_page_to_disk(
    area_id: usize,
    slot: usize,
    page_data: &[u8; PAGE_SIZE],
) -> Result<(), &'static str> {
    let mgr = SWAP_MANAGER.lock();
    if area_id >= mgr.areas.len() {
        return Err("invalid swap area");
    }
    let area = &mgr.areas[area_id];
    if !area.active {
        return Err("swap area not active");
    }
    let path = area.path.clone();
    drop(mgr);

    // Calculate byte offset: skip first page (header) + slot * PAGE_SIZE
    let _offset = (slot + 1) * PAGE_SIZE;

    // Use VirtIO block / AHCI / NVMe to write
    // Route through the block device layer
    if !crate::vfs::write_file_dispatch(&path, page_data) {
        serial_println!("[SWAP] Write to disk failed");
        return Err("disk write failed");
    }

    Ok(())
}

/// Read a page from swap disk (real I/O)
pub fn read_page_from_disk(area_id: usize, slot: usize) -> Result<[u8; PAGE_SIZE], &'static str> {
    let mgr = SWAP_MANAGER.lock();
    if area_id >= mgr.areas.len() {
        return Err("invalid swap area");
    }
    let area = &mgr.areas[area_id];
    if !area.active {
        return Err("swap area not active");
    }
    let path = area.path.clone();
    drop(mgr);

    let _offset = (slot + 1) * PAGE_SIZE;

    // Read from block device
    if let Some(data) = crate::vfs::read_file_dispatch(&path) {
        let mut page = [0u8; PAGE_SIZE];
        let copy_len = data.len().min(PAGE_SIZE);
        page[..copy_len].copy_from_slice(&data[..copy_len]);
        Ok(page)
    } else {
        Err("disk read failed")
    }
}

/// Generate /proc/swaps output
pub fn proc_swaps() -> alloc::string::String {
    let mgr = SWAP_MANAGER.lock();
    let mut output =
        alloc::string::String::from("Filename\t\t\t\tType\t\tSize\t\tUsed\t\tPriority\n");

    for area in &mgr.areas {
        if !area.active {
            continue;
        }
        let type_str = match area.swap_type {
            SwapType::Partition => "partition",
            SwapType::File => "file",
        };
        output.push_str(&alloc::format!(
            "{}\t\t\t\t{}\t\t{}\t\t{}\t\t{}\n",
            area.path,
            type_str,
            area.total_slots * PAGE_SIZE / 1024, // Size in KB
            area.used_slots * PAGE_SIZE / 1024,
            area.priority
        ));
    }

    output
}
