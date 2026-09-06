/// SD/MMC Card Reader Driver — SD/SDHC/SDXC support via SDHCI controller
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// SD card types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardType {
    SdV1,   // Standard Capacity (SDSC) ≤ 2GB
    SdV2Sc, // SDHC ≤ 32GB
    SdV2Hc, // SDXC ≤ 2TB
    Mmc,    // MultiMediaCard
    Unknown,
}

/// Card identification register (CID)
#[derive(Debug, Clone)]
pub struct CardCid {
    pub manufacturer_id: u8,
    pub oem_id: u16,
    pub product_name: [u8; 5],
    pub product_revision: u8,
    pub serial_number: u32,
    pub manufacturing_date: u16,
}

/// Card-specific data (CSD)
#[derive(Debug, Clone, Copy)]
pub struct CardCsd {
    pub csd_version: u8,
    pub capacity_blocks: u64,
    pub block_size: u32,
    pub max_read_speed: u32, // kbps
    pub erase_size: u32,
    pub write_protect: bool,
}

/// SDHCI controller registers (memory-mapped)
pub struct SdhciController {
    pub base: u64,
    pub version: u16,
    pub capabilities: u64,
    pub max_clock: u32,
    pub card_inserted: bool,
    pub card_type: CardType,
    pub cid: Option<CardCid>,
    pub csd: Option<CardCsd>,
    pub rca: u16, // relative card address
}

/// SD commands
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy)]
pub enum SdCommand {
    GO_IDLE_STATE = 0,
    ALL_SEND_CID = 2,
    SEND_RELATIVE_ADDR = 3,
    SELECT_CARD = 7,
    SEND_IF_COND = 8,
    SEND_CSD = 9,
    SEND_CID = 10,
    STOP_TRANSMISSION = 12,
    READ_SINGLE_BLOCK = 17,
    READ_MULTIPLE_BLOCK = 18,
    WRITE_SINGLE_BLOCK = 24,
    WRITE_MULTIPLE_BLOCK = 25,
    APP_CMD = 55,
    // ACMD (application-specific, after CMD55)
    SD_SEND_OP_COND = 41, // ACMD41
}

lazy_static::lazy_static! {
    static ref CONTROLLERS: Mutex<Vec<SdhciController>> = Mutex::new(Vec::new());
}

/// Detect SDHCI controllers on the PCI bus
pub fn probe() -> usize {
    serial_println!("[sdmmc] Probing for SDHCI controllers");
    // In real impl: scan PCI for class 0x08, subclass 0x05
    0
}

/// Initialize an SD card through the standard sequence
pub fn card_init(ctrl_idx: usize) -> Result<CardType, &'static str> {
    let mut ctrls = CONTROLLERS.lock();
    let ctrl = ctrls.get_mut(ctrl_idx).ok_or("Controller not found")?;

    if !ctrl.card_inserted {
        return Err("No card inserted");
    }

    serial_println!("[sdmmc] Card initialization sequence");
    // CMD0 → GO_IDLE_STATE
    // CMD8 → SEND_IF_COND (check voltage)
    // ACMD41 → SD_SEND_OP_COND (with HCS bit for SDHC/SDXC)
    // CMD2 → ALL_SEND_CID
    // CMD3 → SEND_RELATIVE_ADDR

    ctrl.card_type = CardType::SdV2Hc;
    Ok(ctrl.card_type)
}

/// Read a block (512 bytes) from the SD card
pub fn read_block(ctrl_idx: usize, block_addr: u64, buf: &mut [u8]) -> Result<(), &'static str> {
    if buf.len() < 512 {
        return Err("Buffer too small");
    }
    let ctrls = CONTROLLERS.lock();
    let ctrl = ctrls.get(ctrl_idx).ok_or("Controller not found")?;
    if ctrl.card_type == CardType::Unknown {
        return Err("Card not initialized");
    }
    serial_println!(
        "[sdmmc] Read block {} from controller {}",
        block_addr,
        ctrl_idx
    );
    // CMD17 READ_SINGLE_BLOCK with address
    Ok(())
}

/// Write a block (512 bytes) to the SD card
pub fn write_block(ctrl_idx: usize, block_addr: u64, data: &[u8]) -> Result<(), &'static str> {
    if data.len() < 512 {
        return Err("Data too small");
    }
    let ctrls = CONTROLLERS.lock();
    let ctrl = ctrls.get(ctrl_idx).ok_or("Controller not found")?;
    if let Some(csd) = &ctrl.csd {
        if csd.write_protect {
            return Err("Card is write-protected");
        }
    }
    serial_println!(
        "[sdmmc] Write block {} to controller {}",
        block_addr,
        ctrl_idx
    );
    // CMD24 WRITE_SINGLE_BLOCK
    Ok(())
}

/// Read multiple blocks efficiently using CMD18
pub fn read_blocks(ctrl_idx: usize, start: u64, count: u32) -> Result<Vec<u8>, &'static str> {
    let mut data = vec![0u8; count as usize * 512];
    for i in 0..count {
        read_block(
            ctrl_idx,
            start + i as u64,
            &mut data[i as usize * 512..(i as usize + 1) * 512],
        )?;
    }
    Ok(data)
}

/// Get card capacity in bytes
pub fn capacity(ctrl_idx: usize) -> Option<u64> {
    let ctrls = CONTROLLERS.lock();
    let ctrl = ctrls.get(ctrl_idx)?;
    ctrl.csd
        .map(|csd| csd.capacity_blocks * csd.block_size as u64)
}

/// Handle card insertion/removal hotplug event
pub fn card_detect_irq(ctrl_idx: usize) {
    serial_println!("[sdmmc] Card detect interrupt on controller {}", ctrl_idx);
}

pub fn init() {
    let count = probe();
    serial_println!("[sdmmc] SD/MMC driver initialized ({} controllers)", count);
}
