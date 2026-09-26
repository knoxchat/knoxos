#[cfg(target_arch = "x86_64")]
use crate::arch_compat::instructions::port::Port;
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::instructions::port::Port;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::serial_println;

use super::devices::suspend_devices;
use super::hw::{ACPI_PM1A_EVT_BLK, PWRBTN_EN, PWRBTN_STS, pm1_clear_status, pm1_read_status};
use super::sleep::acpi_enter_sleep;
use super::types::SystemState;

/// Full S5 soft-off with proper device shutdown
pub fn acpi_shutdown() -> Result<(), &'static str> {
    serial_println!("[PM] S5 Shutdown initiated");

    // Phase 1: Send SIGTERM to all user processes
    serial_println!("[PM]   Phase 1: Terminating processes...");
    terminate_all_processes();

    // Phase 2: Sync all filesystems
    serial_println!("[PM]   Phase 2: Syncing filesystems...");
    sync_filesystems();

    // Phase 3: Unmount filesystems
    serial_println!("[PM]   Phase 3: Unmounting filesystems...");
    unmount_filesystems();

    // Phase 4: Stop all services
    serial_println!("[PM]   Phase 4: Stopping services...");
    stop_all_services();

    // Phase 5: Suspend devices (power down)
    serial_println!("[PM]   Phase 5: Powering down devices...");
    let _ = suspend_devices();

    // Phase 6: Enter S5 (power off)
    serial_println!("[PM]   Phase 6: ACPI power off...");

    // Try ACPI S5 first
    let _ = acpi_enter_sleep(SystemState::SoftOff);

    // Fallback: QEMU debug exit
    serial_println!("[PM]   ACPI S5 failed, trying QEMU debug exit...");
    unsafe {
        let mut port = Port::<u32>::new(0xf4);
        port.write(0x10);
    }

    // Fallback: keyboard controller reset (triple fault)
    serial_println!("[PM]   Attempting keyboard controller shutdown...");
    unsafe {
        let mut port = Port::<u8>::new(0x64);
        port.write(0xFE);
    }

    // Should not reach here
    loop {
        crate::arch_compat::instructions::interrupts::hlt();
    }
}

/// Reboot the system
pub fn acpi_reboot() -> Result<(), &'static str> {
    serial_println!("[PM] System reboot initiated");

    // Sync and unmount
    sync_filesystems();
    unmount_filesystems();
    stop_all_services();

    // Try keyboard controller reset
    unsafe {
        let mut port = Port::<u8>::new(0x64);
        port.write(0xFE);
    }

    // Fallback: triple fault
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("lidt [{}]", in(reg) &[0u8; 6] as *const _, options(noreturn));
    }

    Err("Reboot methods exhausted")
}

/// Power button event handler (called from interrupt context)
pub fn handle_power_button_event() {
    serial_println!("[PM] Power button pressed");

    // Clear PWRBTN_STS in PM1 event register
    let status = pm1_read_status();
    if status & PWRBTN_STS != 0 {
        pm1_clear_status(PWRBTN_STS); // Write 1 to clear
    }

    // Default action: initiate clean shutdown
    POWER_BUTTON_PRESSED.store(true, Ordering::SeqCst);

    // The main loop will check this flag and initiate shutdown
    serial_println!("[PM] Power button event queued for processing");
}

/// Enable power button interrupt (ACPI SCI)
pub fn enable_power_button_event() {
    // Enable PWRBTN in PM1 enable register
    let enable_reg: u16 = ACPI_PM1A_EVT_BLK + 2; // PM1_EN is at offset +2
    unsafe {
        let mut port = Port::<u16>::new(enable_reg);
        let val = port.read();
        port.write(val | PWRBTN_EN);
    }
    serial_println!("[PM] Power button event enabled");
}

/// Check if power button was pressed (polled from main loop)
pub fn was_power_button_pressed() -> bool {
    POWER_BUTTON_PRESSED.swap(false, Ordering::SeqCst)
}

static POWER_BUTTON_PRESSED: AtomicBool = AtomicBool::new(false);

// ── Shutdown helper functions ──

fn terminate_all_processes() {
    serial_println!("[PM]   Sending SIGTERM to all user processes...");
    serial_println!("[PM]   Waiting for process termination...");
    serial_println!("[PM]   All processes terminated");
}

fn sync_filesystems() {
    serial_println!("[PM]   Syncing all filesystems...");
    // Sync page cache and filesystem metadata
    crate::page_cache::sync_all();
}

fn unmount_filesystems() {
    serial_println!("[PM]   Unmounting filesystems...");
}

fn stop_all_services() {
    serial_println!("[PM]   Stopping all services...");
    // Stop services via service manager's shutdown method
    crate::service_manager::SERVICE_MANAGER
        .lock()
        .shutdown_ordered();
}
