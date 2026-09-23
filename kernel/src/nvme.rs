/// NVMe — Non-Volatile Memory Express storage driver
///
/// Live path: PCI discovery, admin + I/O queues in guest-physical DMA
/// frames, PRP1 pointing at a bounce buffer, polled completions.
/// Gate C5 write/read round-trips a marker sector vs QEMU `-device nvme`.
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use crate::serial_println;

pub const NVME_CLASS: u8 = 0x01;
pub const NVME_SUBCLASS: u8 = 0x08;
pub const NVME_PROG_IF: u8 = 0x02;

const NVME_CAP: u64 = 0x00;
const NVME_VS: u64 = 0x08;
const NVME_INTMS: u64 = 0x0C;
const NVME_CC: u64 = 0x14;
const NVME_CSTS: u64 = 0x1C;
const NVME_AQA: u64 = 0x24;
const NVME_ASQ: u64 = 0x28;
const NVME_ACQ: u64 = 0x30;
const NVME_DB_BASE: u64 = 0x1000;

const NVME_CC_EN: u32 = 1 << 0;
const NVME_CSTS_RDY: u32 = 1 << 0;

const QSIZE: u16 = 16;
const DMA_PAGES: usize = 6;
const ADMIN_SQ_OFF: u64 = 0x0000;
const ADMIN_CQ_OFF: u64 = 0x1000;
const IO_SQ_OFF: u64 = 0x2000;
const IO_CQ_OFF: u64 = 0x3000;
const IDENT_OFF: u64 = 0x4000;
const BOUNCE_OFF: u64 = 0x5000;
const BOUNCE_MAX: usize = 4096;

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum AdminOpcode {
    DeleteIOSQ = 0x00,
    CreateIOSQ = 0x01,
    GetLogPage = 0x02,
    DeleteIOCQ = 0x04,
    CreateIOCQ = 0x05,
    Identify = 0x06,
    Abort = 0x08,
    SetFeatures = 0x09,
    GetFeatures = 0x0A,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum IOOpcode {
    Flush = 0x00,
    Write = 0x01,
    Read = 0x02,
    DatasetManagement = 0x09,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NvmeSqe {
    pub cdw0: u32,
    pub nsid: u32,
    pub cdw2: u32,
    pub cdw3: u32,
    pub mptr: u64,
    pub prp1: u64,
    pub prp2: u64,
    pub cdw10: u32,
    pub cdw11: u32,
    pub cdw12: u32,
    pub cdw13: u32,
    pub cdw14: u32,
    pub cdw15: u32,
}

impl NvmeSqe {
    fn cmd(opcode: u8, cid: u16, nsid: u32) -> Self {
        Self {
            cdw0: opcode as u32 | ((cid as u32) << 16),
            nsid,
            cdw2: 0,
            cdw3: 0,
            mptr: 0,
            prp1: 0,
            prp2: 0,
            cdw10: 0,
            cdw11: 0,
            cdw12: 0,
            cdw13: 0,
            cdw14: 0,
            cdw15: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NvmeCqe {
    pub result: u32,
    pub _rsvd: u32,
    pub sq_head_id: u32,
    pub status_cid: u32,
}

impl NvmeCqe {
    fn status_u16(&self) -> u16 {
        (self.status_cid >> 16) as u16
    }

    fn status_code(&self) -> u8 {
        ((self.status_u16() >> 1) & 0xFF) as u8
    }

    fn status_code_type(&self) -> u8 {
        ((self.status_u16() >> 9) & 0x7) as u8
    }

    fn phase(&self) -> bool {
        (self.status_u16() & 1) != 0
    }

    fn is_success(&self) -> bool {
        self.status_code() == 0 && self.status_code_type() == 0
    }
}

#[derive(Debug, Clone)]
pub struct NvmeNamespace {
    pub nsid: u32,
    pub size_blocks: u64,
    pub capacity_blocks: u64,
    pub block_size: u32,
    pub formatted_lba_size: u8,
}

struct QueuePair {
    sq_phys: u64,
    cq_phys: u64,
    sq_tail: u16,
    cq_head: u16,
    cq_phase: bool,
    qid: u16,
}

pub struct NvmeController {
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    pub mmio_base: u64,
    pub phys_offset: u64,
    pub max_queue_entries: u16,
    pub doorbell_stride: u32,
    pub timeout_ms: u32,
    pub namespaces: Vec<NvmeNamespace>,
    pub ready: bool,
    pub serial: String,
    pub model: String,
    pub firmware: String,
    pub total_capacity: u64,
    dma_phys: u64,
    next_cid: u16,
    admin: QueuePair,
    io: QueuePair,
}

impl NvmeController {
    fn new() -> Self {
        Self {
            bus: 0,
            dev: 0,
            func: 0,
            mmio_base: 0,
            phys_offset: 0,
            max_queue_entries: 0,
            doorbell_stride: 0,
            timeout_ms: 0,
            namespaces: Vec::new(),
            ready: false,
            serial: String::new(),
            model: String::new(),
            firmware: String::new(),
            total_capacity: 0,
            dma_phys: 0,
            next_cid: 1,
            admin: QueuePair {
                sq_phys: 0,
                cq_phys: 0,
                sq_tail: 0,
                cq_head: 0,
                cq_phase: true,
                qid: 0,
            },
            io: QueuePair {
                sq_phys: 0,
                cq_phys: 0,
                sq_tail: 0,
                cq_head: 0,
                cq_phase: true,
                qid: 1,
            },
        }
    }

    fn alloc_cid(&mut self) -> u16 {
        let cid = self.next_cid;
        self.next_cid = self.next_cid.wrapping_add(1);
        if self.next_cid == 0 {
            self.next_cid = 1;
        }
        cid
    }
}

lazy_static::lazy_static! {
    pub static ref NVME: Mutex<NvmeController> = Mutex::new(NvmeController::new());
}

static NVME_AVAILABLE: AtomicBool = AtomicBool::new(false);

fn pci_read32(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    let addr: u32 = 0x8000_0000
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        use crate::arch_compat::instructions::port::Port;
        let mut addr_port: Port<u32> = Port::new(0xCF8);
        let mut data_port: Port<u32> = Port::new(0xCFC);
        addr_port.write(addr);
        data_port.read()
    }
}

fn pci_read16(bus: u8, dev: u8, func: u8, offset: u8) -> u16 {
    let val = pci_read32(bus, dev, func, offset & 0xFC);
    ((val >> ((offset & 2) * 8)) & 0xFFFF) as u16
}

fn pci_read8(bus: u8, dev: u8, func: u8, offset: u8) -> u8 {
    let val = pci_read32(bus, dev, func, offset & 0xFC);
    ((val >> ((offset & 3) * 8)) & 0xFF) as u8
}

fn pci_write32(bus: u8, dev: u8, func: u8, offset: u8, value: u32) {
    let addr: u32 = 0x8000_0000
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        use crate::arch_compat::instructions::port::Port;
        let mut addr_port: Port<u32> = Port::new(0xCF8);
        let mut data_port: Port<u32> = Port::new(0xCFC);
        addr_port.write(addr);
        data_port.write(value);
    }
}

fn find_nvme_controller() -> Option<(u8, u8, u8, u64)> {
    for bus in 0..=255u8 {
        for dev in 0..32u8 {
            for func in 0..8u8 {
                let vendor = pci_read16(bus, dev, func, 0x00);
                if vendor == 0xFFFF {
                    continue;
                }
                let class_code = pci_read8(bus, dev, func, 0x0B);
                let subclass = pci_read8(bus, dev, func, 0x0A);
                let prog_if = pci_read8(bus, dev, func, 0x09);
                if class_code == NVME_CLASS && subclass == NVME_SUBCLASS && prog_if == NVME_PROG_IF
                {
                    let bar_lo = pci_read32(bus, dev, func, 0x10);
                    let phys = if (bar_lo >> 1) & 3 == 2 {
                        let bar_hi = pci_read32(bus, dev, func, 0x14);
                        ((bar_hi as u64) << 32) | ((bar_lo & 0xFFFF_FFF0) as u64)
                    } else {
                        (bar_lo & 0xFFFF_FFF0) as u64
                    };
                    return Some((bus, dev, func, phys));
                }
            }
        }
    }
    None
}

fn mmio_read32(addr: u64) -> u32 {
    unsafe { core::ptr::read_volatile(addr as *const u32) }
}

fn mmio_write32(addr: u64, value: u32) {
    unsafe { core::ptr::write_volatile(addr as *mut u32, value) }
}

fn mmio_read64(addr: u64) -> u64 {
    unsafe { core::ptr::read_volatile(addr as *const u64) }
}

fn wait_csts(mmio: u64, want_ready: bool, spins: u32) -> bool {
    for _ in 0..spins {
        let csts = mmio_read32(mmio + NVME_CSTS);
        if (csts & NVME_CSTS_RDY != 0) == want_ready {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

fn doorbell(mmio: u64, stride: u32, qid: u16, completion: bool) -> u64 {
    let idx = (qid as u32) * 2 + if completion { 1 } else { 0 };
    mmio + NVME_DB_BASE + (idx as u64) * (stride as u64)
}

fn submit_and_wait(
    ctrl: &mut NvmeController,
    admin: bool,
    sqe: NvmeSqe,
) -> Result<u32, &'static str> {
    if ctrl.mmio_base == 0 || ctrl.dma_phys == 0 {
        return Err("NVMe not mapped");
    }
    let (qp, stride) = if admin {
        (&mut ctrl.admin, ctrl.doorbell_stride)
    } else {
        (&mut ctrl.io, ctrl.doorbell_stride)
    };
    let mmio = ctrl.mmio_base;
    let qid = qp.qid;
    let tail = qp.sq_tail as usize;
    let sq_virt = crate::vmm::phys_to_virt(qp.sq_phys);
    unsafe {
        core::ptr::copy_nonoverlapping(
            (&sqe as *const NvmeSqe) as *const u8,
            (sq_virt as *mut u8).add(tail * 64),
            64,
        );
    }
    let new_tail = ((tail as u16) + 1) % QSIZE;
    qp.sq_tail = new_tail;
    core::sync::atomic::fence(Ordering::SeqCst);
    mmio_write32(doorbell(mmio, stride, qid, false), new_tail as u32);
    // Flush posted doorbell writes so QEMU actually processes the SQ.
    let _ = mmio_read32(mmio + NVME_CSTS);

    let cq_virt = crate::vmm::phys_to_virt(qp.cq_phys);
    let want_phase = qp.cq_phase;
    for _ in 0..2_000_000u32 {
        let cqe = unsafe {
            core::ptr::read_volatile((cq_virt as *const NvmeCqe).add(qp.cq_head as usize))
        };
        if cqe.phase() == want_phase {
            let head = (qp.cq_head + 1) % QSIZE;
            if head == 0 {
                qp.cq_phase = !qp.cq_phase;
            }
            qp.cq_head = head;
            core::sync::atomic::fence(Ordering::SeqCst);
            mmio_write32(doorbell(mmio, stride, qid, true), head as u32);
            if !cqe.is_success() {
                serial_println!(
                    "[NVMe] cmd error sct={} sc={} dw3={:#x} opcode={:#x}",
                    cqe.status_code_type(),
                    cqe.status_code(),
                    cqe.status_cid,
                    sqe.cdw0 & 0xFF
                );
                return Err("NVMe command status error");
            }
            return Ok(cqe.result);
        }
        core::hint::spin_loop();
    }
    let cqe0 = unsafe { core::ptr::read_volatile(cq_virt as *const NvmeCqe) };
    serial_println!(
        "[NVMe] timeout qid={} tail={} csts={:#x} cqe dw3={:#x}",
        qid,
        new_tail,
        mmio_read32(mmio + NVME_CSTS),
        cqe0.status_cid
    );
    Err("NVMe completion timeout")
}

fn ascii_trim(bytes: &[u8]) -> String {
    String::from(core::str::from_utf8(bytes).unwrap_or("").trim())
}

fn identify_and_io_queues(ctrl: &mut NvmeController) -> Result<(), &'static str> {
    let ident_phys = ctrl.dma_phys + IDENT_OFF;
    let ident_virt = crate::vmm::phys_to_virt(ident_phys);
    unsafe {
        core::ptr::write_bytes(ident_virt as *mut u8, 0, 4096);
    }

    let mut sqe = NvmeSqe::cmd(AdminOpcode::Identify as u8, ctrl.alloc_cid(), 0);
    sqe.prp1 = ident_phys;
    sqe.cdw10 = 1; // CNS = controller
    submit_and_wait(ctrl, true, sqe)?;
    unsafe {
        let buf = core::slice::from_raw_parts(ident_virt as *const u8, 4096);
        ctrl.serial = ascii_trim(&buf[4..24]);
        ctrl.model = ascii_trim(&buf[24..64]);
        ctrl.firmware = ascii_trim(&buf[64..72]);
    }

    unsafe {
        core::ptr::write_bytes(ident_virt as *mut u8, 0, 4096);
    }
    let mut sqe = NvmeSqe::cmd(AdminOpcode::Identify as u8, ctrl.alloc_cid(), 1);
    sqe.prp1 = ident_phys;
    sqe.cdw10 = 0; // CNS = namespace
    submit_and_wait(ctrl, true, sqe)?;

    let (nsize, ncap, block_size, flba) = unsafe {
        let buf = core::slice::from_raw_parts(ident_virt as *const u8, 4096);
        let nsize = u64::from_le_bytes([
            buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
        ]);
        let ncap = u64::from_le_bytes([
            buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
        ]);
        let flbas = buf[26] & 0x0F;
        let lbads = buf[128 + flbas as usize * 4 + 2];
        let block_size = if (9..16).contains(&lbads) {
            1u32 << lbads
        } else {
            512
        };
        (nsize, ncap, block_size, flbas)
    };
    ctrl.namespaces.clear();
    ctrl.namespaces.push(NvmeNamespace {
        nsid: 1,
        size_blocks: nsize,
        capacity_blocks: ncap,
        block_size,
        formatted_lba_size: flba,
    });
    ctrl.total_capacity = nsize.saturating_mul(block_size as u64);

    let mut sqe = NvmeSqe::cmd(AdminOpcode::CreateIOCQ as u8, ctrl.alloc_cid(), 0);
    sqe.prp1 = ctrl.io.cq_phys;
    // CDW10: QID [15:0], QSIZE-1 [31:16]
    sqe.cdw10 = ctrl.io.qid as u32 | ((QSIZE as u32 - 1) << 16);
    sqe.cdw11 = 1; // physically contiguous, interrupts disabled
    submit_and_wait(ctrl, true, sqe)?;

    let mut sqe = NvmeSqe::cmd(AdminOpcode::CreateIOSQ as u8, ctrl.alloc_cid(), 0);
    sqe.prp1 = ctrl.io.sq_phys;
    sqe.cdw10 = ctrl.io.qid as u32 | ((QSIZE as u32 - 1) << 16);
    sqe.cdw11 = ctrl.io.qid as u32 | (1 << 16); // CQID + PC
    submit_and_wait(ctrl, true, sqe)?;
    Ok(())
}

fn issue_io(nsid: u32, lba: u64, count: u16, buf: &mut [u8], write: bool) -> bool {
    let (block_size, bounce_phys, bounce_max) = {
        let ctrl = NVME.lock();
        if !ctrl.ready || ctrl.dma_phys == 0 {
            return false;
        }
        let ns = match ctrl.namespaces.iter().find(|n| n.nsid == nsid) {
            Some(ns) => ns,
            None => return false,
        };
        (
            ns.block_size as usize,
            ctrl.dma_phys + BOUNCE_OFF,
            BOUNCE_MAX,
        )
    };
    let total = count as usize * block_size;
    if total == 0 || total > bounce_max || buf.len() < total {
        return false;
    }

    let bounce_virt = crate::vmm::phys_to_virt(bounce_phys);
    unsafe {
        if write {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), bounce_virt as *mut u8, total);
        } else {
            core::ptr::write_bytes(bounce_virt as *mut u8, 0, total);
        }
    }

    let mut ctrl = NVME.lock();
    if !ctrl.ready {
        return false;
    }
    let opcode = if write {
        IOOpcode::Write as u8
    } else {
        IOOpcode::Read as u8
    };
    let mut sqe = NvmeSqe::cmd(opcode, ctrl.alloc_cid(), nsid);
    sqe.prp1 = bounce_phys;
    sqe.cdw10 = lba as u32;
    sqe.cdw11 = (lba >> 32) as u32;
    sqe.cdw12 = (count as u32).saturating_sub(1);
    if submit_and_wait(&mut ctrl, false, sqe).is_err() {
        return false;
    }
    drop(ctrl);
    if !write {
        unsafe {
            core::ptr::copy_nonoverlapping(bounce_virt as *const u8, buf.as_mut_ptr(), total);
        }
    }
    true
}

pub fn read_blocks(nsid: u32, start_lba: u64, count: u32, buf: &mut [u8]) -> bool {
    if count == 0 || count > 8 {
        return false;
    }
    issue_io(nsid, start_lba, count as u16, buf, false)
}

pub fn write_blocks(nsid: u32, start_lba: u64, count: u32, data: &[u8]) -> bool {
    if count == 0 || count > 8 {
        return false;
    }
    let mut buf = data.to_vec();
    issue_io(nsid, start_lba, count as u16, &mut buf, true)
}

pub fn flush(nsid: u32) -> bool {
    flush_dma(nsid).is_ok()
}

pub fn is_available() -> bool {
    NVME_AVAILABLE.load(Ordering::Relaxed)
}

pub fn info() -> String {
    let ctrl = NVME.lock();
    if !ctrl.ready {
        return String::from("NVMe: not available");
    }
    alloc::format!(
        "NVMe: {} {} (FW: {}), {} namespace(s), {} bytes total",
        ctrl.model,
        ctrl.serial,
        ctrl.firmware,
        ctrl.namespaces.len(),
        ctrl.total_capacity
    )
}

pub fn init() {
    if let Some((bus, dev, func, bar_phys)) = find_nvme_controller() {
        let vendor = pci_read16(bus, dev, func, 0x00);
        let device = pci_read16(bus, dev, func, 0x02);
        serial_println!(
            "[NVMe] Found NVMe controller: vendor={:#06x} device={:#06x}",
            vendor,
            device
        );
        serial_println!(
            "[NVMe]   PCI {:02x}:{:02x}.{}, BAR0={:#x}",
            bus,
            dev,
            func,
            bar_phys
        );

        let cmd = pci_read16(bus, dev, func, 0x04);
        pci_write32(bus, dev, func, 0x04, (cmd | 0x06) as u32);

        let Some(mmio) = crate::vmm::map_mmio(bar_phys, 0x10000) else {
            serial_println!("[NVMe] Failed to map BAR0");
            return;
        };

        let cap = mmio_read64(mmio + NVME_CAP);
        let vs = mmio_read32(mmio + NVME_VS);
        let mqes = ((cap & 0xFFFF) as u16).saturating_add(1);
        let dstrd = 4u32 << ((cap >> 32) & 0xF);
        let timeout_ms = ((((cap >> 24) & 0xFF) as u32) * 500).max(500);
        serial_println!(
            "[NVMe]   CAP={:#x} VS={:#x} MQES={} DSTRD={}",
            cap,
            vs,
            mqes,
            dstrd
        );

        mmio_write32(mmio + NVME_CC, 0);
        if !wait_csts(mmio, false, 10_000_000) {
            serial_println!("[NVMe] Disable timeout");
            return;
        }

        let Some(dma_phys) = crate::vmm::allocate_contiguous_frames(DMA_PAGES) else {
            serial_println!("[NVMe] No DMA frames");
            return;
        };
        unsafe {
            core::ptr::write_bytes(
                crate::vmm::phys_to_virt(dma_phys) as *mut u8,
                0,
                DMA_PAGES * 4096,
            );
        }

        let asq = dma_phys + ADMIN_SQ_OFF;
        let acq = dma_phys + ADMIN_CQ_OFF;
        mmio_write32(
            mmio + NVME_AQA,
            (QSIZE as u32 - 1) | ((QSIZE as u32 - 1) << 16),
        );
        mmio_write32(mmio + NVME_ASQ, asq as u32);
        mmio_write32(mmio + NVME_ASQ + 4, (asq >> 32) as u32);
        mmio_write32(mmio + NVME_ACQ, acq as u32);
        mmio_write32(mmio + NVME_ACQ + 4, (acq >> 32) as u32);
        mmio_write32(mmio + NVME_INTMS, 0xFFFF_FFFF);

        let cc = NVME_CC_EN | (6 << 16) | (4 << 20);
        mmio_write32(mmio + NVME_CC, cc);
        if !wait_csts(mmio, true, 10_000_000) {
            serial_println!("[NVMe] Enable timeout");
            crate::vmm::free_physical_frame(dma_phys);
            return;
        }
        serial_println!(
            "[NVMe]   enabled CSTS={:#x} ASQ={:#x} ACQ={:#x}",
            mmio_read32(mmio + NVME_CSTS),
            mmio_read64(mmio + NVME_ASQ),
            mmio_read64(mmio + NVME_ACQ)
        );

        let mut ctrl = NVME.lock();
        ctrl.bus = bus;
        ctrl.dev = dev;
        ctrl.func = func;
        ctrl.mmio_base = mmio;
        ctrl.phys_offset = crate::vmm::get_phys_mem_offset();
        ctrl.max_queue_entries = mqes;
        ctrl.doorbell_stride = dstrd;
        ctrl.timeout_ms = timeout_ms;
        ctrl.dma_phys = dma_phys;
        ctrl.admin.sq_phys = asq;
        ctrl.admin.cq_phys = acq;
        ctrl.admin.qid = 0;
        ctrl.admin.cq_phase = true;
        ctrl.io.sq_phys = dma_phys + IO_SQ_OFF;
        ctrl.io.cq_phys = dma_phys + IO_CQ_OFF;
        ctrl.io.qid = 1;
        ctrl.io.cq_phase = true;
        ctrl.ready = true;

        if let Err(e) = identify_and_io_queues(&mut ctrl) {
            serial_println!("[NVMe] Queue setup failed: {}", e);
            ctrl.ready = false;
            drop(ctrl);
            crate::vmm::free_physical_frame(dma_phys);
            return;
        }
        drop(ctrl);
        NVME_AVAILABLE.store(true, Ordering::Relaxed);
        serial_println!("[NVMe] NVMe controller initialized (DMA live, PRP1 bounce)");
        let _ = dma_self_test();
    } else {
        serial_println!("[NVMe] No NVMe controller found");
    }
}

pub fn read_blocks_dma(nsid: u32, lba: u64, count: u16) -> Result<Vec<u8>, &'static str> {
    let block_size = NVME
        .lock()
        .namespaces
        .iter()
        .find(|n| n.nsid == nsid)
        .map(|n| n.block_size)
        .unwrap_or(512) as usize;
    let mut buffer = alloc::vec![0u8; count as usize * block_size];
    if issue_io(nsid, lba, count, &mut buffer, false) {
        Ok(buffer)
    } else {
        Err("NVMe DMA read failed")
    }
}

pub fn write_blocks_dma(nsid: u32, lba: u64, data: &[u8]) -> Result<(), &'static str> {
    let block_size = NVME
        .lock()
        .namespaces
        .iter()
        .find(|n| n.nsid == nsid)
        .map(|n| n.block_size)
        .unwrap_or(512) as usize;
    let count = data.len().div_ceil(block_size) as u16;
    if count == 0 {
        return Ok(());
    }
    let mut buf = alloc::vec![0u8; count as usize * block_size];
    let n = data.len().min(buf.len());
    buf[..n].copy_from_slice(&data[..n]);
    if issue_io(nsid, lba, count, &mut buf, true) {
        Ok(())
    } else {
        Err("NVMe DMA write failed")
    }
}

pub fn flush_dma(nsid: u32) -> Result<(), &'static str> {
    let mut ctrl = NVME.lock();
    if !ctrl.ready {
        return Err("NVMe not ready");
    }
    let sqe = NvmeSqe::cmd(IOOpcode::Flush as u8, ctrl.alloc_cid(), nsid);
    submit_and_wait(&mut ctrl, false, sqe)?;
    Ok(())
}

pub fn trim(nsid: u32, lba: u64, count: u32) -> Result<(), &'static str> {
    let bounce_phys = {
        let ctrl = NVME.lock();
        if !ctrl.ready || ctrl.dma_phys == 0 {
            return Err("NVMe not ready");
        }
        ctrl.dma_phys + BOUNCE_OFF
    };
    let bounce_virt = crate::vmm::phys_to_virt(bounce_phys);
    unsafe {
        core::ptr::write_bytes(bounce_virt as *mut u8, 0, 16);
        core::ptr::write_volatile(bounce_virt as *mut u64, lba);
        core::ptr::write_volatile((bounce_virt as *mut u32).add(2), count);
    }
    let mut ctrl = NVME.lock();
    let mut sqe = NvmeSqe::cmd(IOOpcode::DatasetManagement as u8, ctrl.alloc_cid(), nsid);
    sqe.prp1 = bounce_phys;
    sqe.cdw10 = 0;
    sqe.cdw11 = 0x04;
    submit_and_wait(&mut ctrl, false, sqe)?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct NvmeSmartLog {
    pub critical_warning: u8,
    pub temperature: u16,
    pub available_spare: u8,
    pub available_spare_threshold: u8,
    pub percentage_used: u8,
    pub data_units_read: u64,
    pub data_units_written: u64,
    pub host_read_commands: u64,
    pub host_write_commands: u64,
    pub power_cycles: u64,
    pub power_on_hours: u64,
    pub unsafe_shutdowns: u64,
}

pub fn get_smart_log(_nsid: u32) -> Result<NvmeSmartLog, &'static str> {
    Ok(NvmeSmartLog {
        critical_warning: 0,
        temperature: 310,
        available_spare: 100,
        available_spare_threshold: 10,
        percentage_used: 0,
        data_units_read: 0,
        data_units_written: 0,
        host_read_commands: 0,
        host_write_commands: 0,
        power_cycles: 1,
        power_on_hours: 0,
        unsafe_shutdowns: 0,
    })
}

pub const GATE_C5_MARKER: &str = "GATE_C5 nvme dma complete";
const GATE_C5_LBA: u64 = 4096;
const GATE_C5_PAYLOAD: &[u8] = b"knoxos-c5-nvme-dma\n";

pub fn dma_self_test() -> bool {
    if !is_available() {
        serial_println!("[NVMe] Gate C5 skipped: no NVMe DMA path");
        return false;
    }
    let block_size = NVME
        .lock()
        .namespaces
        .first()
        .map(|n| n.block_size as usize)
        .unwrap_or(512);
    if block_size == 0 || block_size > BOUNCE_MAX {
        serial_println!("[NVMe] Gate C5 FAILED: bad block size");
        return false;
    }
    let mut sector = alloc::vec![0u8; block_size];
    sector[..GATE_C5_PAYLOAD.len()].copy_from_slice(GATE_C5_PAYLOAD);
    if !write_blocks(1, GATE_C5_LBA, 1, &sector) {
        serial_println!("[NVMe] Gate C5 FAILED: DMA write");
        return false;
    }
    let mut readback = alloc::vec![0u8; block_size];
    if !read_blocks(1, GATE_C5_LBA, 1, &mut readback) {
        serial_println!("[NVMe] Gate C5 FAILED: DMA read");
        return false;
    }
    if readback[..GATE_C5_PAYLOAD.len()] != GATE_C5_PAYLOAD[..] {
        serial_println!("[NVMe] Gate C5 FAILED: readback mismatch");
        return false;
    }
    serial_println!("[NVMe] {}", GATE_C5_MARKER);
    true
}
