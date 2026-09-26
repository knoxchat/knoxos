#[cfg(target_arch = "x86_64")]
use crate::arch_compat::instructions::port::Port;
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::instructions::port::Port;
use core::sync::atomic::AtomicU64;

use super::manager::PM;

// ── ACPI Fixed Hardware Registers ──────────────────────────────────────────

/// PM1a Event Block — status bits (ACPI spec §4.8.3)
pub(super) const ACPI_PM1A_EVT_BLK: u16 = 0x600; // QEMU PIIX4: 0x600
/// PM1a Control Block — SLP_TYP + SLP_EN (ACPI spec §4.8.3.2)
pub(super) const ACPI_PM1A_CNT_BLK: u16 = 0x604;
/// PM Timer port (24-bit or 32-bit counter at 3.579545 MHz)
pub(super) const ACPI_PM_TMR_BLK: u16 = 0x608;

/// SLP_EN bit — write 1 to enter sleep state
pub(super) const SLP_EN: u16 = 1 << 13;
/// SCI_EN bit in PM1_CNT — ACPI mode enabled
pub(super) const SCI_EN: u16 = 1;
/// WAK_STS bit in PM1_STS
pub(super) const WAK_STS: u16 = 1 << 15;
/// PWRBTN_STS
pub(super) const PWRBTN_STS: u16 = 1 << 8;
/// PWRBTN_EN
pub(super) const PWRBTN_EN: u16 = 1 << 8;
/// TMR_STS
pub(super) const TMR_STS: u16 = 1;

/// SLP_TYP values for each S-state (QEMU PIIX4 DSDT defaults)
pub(super) const SLP_TYP_S1: u16 = 1 << 10; // S1: bits [12:10] = 001
pub(super) const SLP_TYP_S3: u16 = 5 << 10; // S3: bits [12:10] = 101
pub(super) const SLP_TYP_S4: u16 = 6 << 10; // S4: bits [12:10] = 110
pub(super) const SLP_TYP_S5: u16 = 7 << 10; // S5: bits [12:10] = 111 (=0, QEMU uses 0 sometimes)

/// PM timer ticks counter
pub(super) static PM_TIMER_TICKS: AtomicU64 = AtomicU64::new(0);

// ── ACPI PM timer ───────────────────────────────────────────────────────

/// Read the ACPI PM timer (3.579545 MHz, 24-bit or 32-bit)
pub fn pm_timer_read() -> u32 {
    let pm = PM.lock();
    let port_addr = pm.pm_tmr_blk;
    drop(pm);
    unsafe {
        let mut port = Port::<u32>::new(port_addr);
        port.read()
    }
}

/// Convert PM timer ticks to microseconds
pub fn pm_timer_ticks_to_us(ticks: u32) -> u64 {
    // Timer frequency is 3.579545 MHz → 1 tick ≈ 0.2794 µs
    // ticks * 1_000_000 / 3_579_545 ≈ ticks * 2794 / 10000
    (ticks as u64 * 2794) / 10000
}

/// Busy-wait using PM timer (more accurate than TSC for short waits)
pub fn pm_timer_delay_us(us: u64) {
    let ticks_needed = (us * 3_579_545) / 1_000_000;
    let start = pm_timer_read();
    loop {
        let now = pm_timer_read();
        let elapsed = now.wrapping_sub(start) & 0x00FF_FFFF; // 24-bit wrap
        if elapsed as u64 >= ticks_needed {
            break;
        }
        core::hint::spin_loop();
    }
}

// ── ACPI PM1 register access ────────────────────────────────────────────

/// Read PM1_STS register
pub(super) fn pm1_read_status() -> u16 {
    let pm = PM.lock();
    let addr = pm.pm1a_evt_blk;
    drop(pm);
    unsafe {
        let mut port = Port::<u16>::new(addr);
        port.read()
    }
}

/// Write PM1_STS register (write-1-to-clear)
pub(super) fn pm1_clear_status(bits: u16) {
    let pm = PM.lock();
    let addr = pm.pm1a_evt_blk;
    drop(pm);
    unsafe {
        let mut port = Port::<u16>::new(addr);
        port.write(bits);
    }
}

/// Read PM1_EN register (enable bits at EVT_BLK + 2)
pub(super) fn pm1_read_enable() -> u16 {
    let pm = PM.lock();
    let addr = pm.pm1a_evt_blk + 2;
    drop(pm);
    unsafe {
        let mut port = Port::<u16>::new(addr);
        port.read()
    }
}

/// Write PM1_EN register
pub(super) fn pm1_write_enable(bits: u16) {
    let pm = PM.lock();
    let addr = pm.pm1a_evt_blk + 2;
    drop(pm);
    unsafe {
        let mut port = Port::<u16>::new(addr);
        port.write(bits);
    }
}

/// Read PM1_CNT register
pub(super) fn pm1_read_control() -> u16 {
    let pm = PM.lock();
    let addr = pm.pm1a_cnt_blk;
    drop(pm);
    unsafe {
        let mut port = Port::<u16>::new(addr);
        port.read()
    }
}

/// Write PM1_CNT register
pub(super) fn pm1_write_control(val: u16) {
    let pm = PM.lock();
    let addr = pm.pm1a_cnt_blk;
    drop(pm);
    unsafe {
        let mut port = Port::<u16>::new(addr);
        port.write(val);
    }
}
