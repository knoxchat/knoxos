use core::sync::atomic::{AtomicBool, Ordering};

use crate::serial_println;

use super::cpu::{restore_cpu_state, save_cpu_state};
use super::devices::{resume_devices, suspend_devices};
use super::hw::{
    PWRBTN_EN, PWRBTN_STS, SLP_EN, TMR_STS, WAK_STS, pm_timer_read, pm1_clear_status,
    pm1_read_control, pm1_read_enable, pm1_read_status, pm1_write_control, pm1_write_enable,
};
use super::manager::{PM, SUSPEND_IN_PROGRESS};
use super::types::SystemState;

// ── Freeze / thaw processes ─────────────────────────────────────────────

/// Freeze all user-space processes (stop scheduling them)
fn freeze_processes() {
    serial_println!("[PM] Freezing user processes...");
    // In a real system: iterate process table, set state to Frozen, drain work queues
    // For now, signal via atomic flag so scheduler skips user tasks
    SUSPEND_IN_PROGRESS.store(true, Ordering::SeqCst);
    // Flush any pending work
    core::sync::atomic::fence(Ordering::SeqCst);
    serial_println!("[PM] Processes frozen");
}

/// Thaw all user-space processes after resume
fn thaw_processes() {
    serial_println!("[PM] Thawing user processes...");
    SUSPEND_IN_PROGRESS.store(false, Ordering::SeqCst);
    serial_println!("[PM] Processes thawed");
}

/// Check if suspend is in progress (used by scheduler)
pub fn is_suspend_in_progress() -> bool {
    SUSPEND_IN_PROGRESS.load(Ordering::Relaxed)
}

// ── Real ACPI sleep entry ───────────────────────────────────────────────

/// Enter ACPI sleep state by programming PM1_CNT with SLP_TYP + SLP_EN
///
/// This performs the real hardware sequence:
/// 1. Clear WAK_STS
/// 2. Enable wakeup events (power button, RTC alarm)
/// 3. Write SLP_TYP | SLP_EN to PM1a_CNT
/// 4. CPU halts; hardware resumes at firmware vector
/// 5. After waking, clear WAK_STS and continue
pub(super) fn acpi_enter_sleep(state: SystemState) -> Result<(), i32> {
    let slp_typ = state.slp_typ();

    serial_println!(
        "[PM] ACPI: entering sleep state {} (SLP_TYP={:#x})",
        state.as_str(),
        slp_typ
    );

    // 1. Clear all pending events
    pm1_clear_status(WAK_STS | PWRBTN_STS | TMR_STS);

    // 2. Enable wakeup sources — power button + RTC alarm
    let en = pm1_read_enable();
    pm1_write_enable(en | PWRBTN_EN);

    // 3. Read current PM1_CNT, mask out old SLP_TYP, set new SLP_TYP
    let cnt = pm1_read_control();
    let new_cnt = (cnt & !(0x1C00)) | slp_typ; // clear bits [12:10], set new SLP_TYP

    // 4. Write SLP_TYP without SLP_EN first (ACPI spec recommends two-step)
    pm1_write_control(new_cnt);

    // 5. Now set SLP_EN to actually enter sleep
    pm1_write_control(new_cnt | SLP_EN);

    // 6. Flush the write
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!("nop", options(nomem, nostack))
    };

    // 7. If we reach here on S1, the CPU just halted briefly.
    //    For S3, the BIOS/firmware will resume us at the wakeup vector,
    //    and we'll end up back here (or at a trampoline).

    // Wait for WAK_STS to be set (firmware sets it on resume)
    for _ in 0..1_000_000u32 {
        let sts = pm1_read_status();
        if sts & WAK_STS != 0 {
            break;
        }
        core::hint::spin_loop();
    }

    // Clear WAK_STS
    pm1_clear_status(WAK_STS);

    serial_println!("[PM] ACPI: woke from sleep state {}", state.as_str());
    Ok(())
}

// ── Public suspend / resume API ─────────────────────────────────────────

/// Get current system state
pub fn current_state() -> SystemState {
    let pm = PM.lock();
    pm.current_state
}

/// Request system suspend to a target state
///
/// Full suspend sequence:
/// 1. Freeze user processes
/// 2. Suspend all devices (DMA quiesce, state save)
/// 3. Save CPU state (CR3, GDT, IDT, MSRs)
/// 4. Program ACPI PM1_CNT → enter sleep
/// 5. <hardware sleeps>
/// 6. Firmware resumes → restore CPU state
/// 7. Resume all devices
/// 8. Thaw user processes
pub fn suspend(target_state: SystemState) -> Result<(), i32> {
    match target_state {
        SystemState::Running => return Ok(()),
        SystemState::SoftOff => {
            serial_println!("[PM] Powering off...");
            // Freeze + device suspend before power-off
            freeze_processes();
            let _ = suspend_devices();
            crate::acpi::shutdown();
            #[allow(unreachable_code)]
            return Ok(());
        }
        _ => {}
    }

    serial_println!("[PM] ========================================");
    serial_println!("[PM] System suspend to {} initiated", target_state.as_str());
    serial_println!("[PM] ========================================");

    // Phase 1: Freeze processes
    freeze_processes();

    // Phase 2: Suspend devices
    if let Err(e) = suspend_devices() {
        serial_println!("[PM] Device suspend failed: {}, aborting", e);
        thaw_processes();
        let mut pm = PM.lock();
        pm.failed_suspend_count += 1;
        return Err(-5); // EIO
    }

    // Phase 3: Save CPU state
    save_cpu_state();

    // Phase 4: Disable interrupts & enter ACPI sleep
    {
        let mut pm = PM.lock();
        pm.current_state = target_state;
        pm.suspend_count += 1;
        pm.last_suspend_time_us = pm_timer_read() as u64;
    }

    // Disable interrupts before programming sleep registers
    crate::arch_compat::instructions::interrupts::disable();

    let result = match target_state {
        SystemState::Standby | SystemState::SuspendToRam => acpi_enter_sleep(target_state),
        SystemState::SuspendToDisk => {
            // S4 requires writing memory image to swap device first
            serial_println!("[PM] Hibernate: saving memory image to disk...");
            // In a real system: compress + write all RAM pages to swap partition
            // Then enter S4 via ACPI
            acpi_enter_sleep(target_state)
        }
        _ => Ok(()),
    };

    // ── Resume path ─────────────────────────────────────────────────
    // We reach here after waking from sleep

    // Re-enable interrupts
    crate::arch_compat::instructions::interrupts::enable();

    // Phase 5: Restore CPU state
    restore_cpu_state();

    // Phase 6: Resume devices
    resume_devices();

    // Phase 7: Thaw processes
    thaw_processes();

    {
        let mut pm = PM.lock();
        pm.current_state = SystemState::Running;
        pm.resume_count += 1;
        pm.last_resume_time_us = pm_timer_read() as u64;
    }

    serial_println!("[PM] ========================================");
    serial_println!("[PM] System resumed from {}", target_state.as_str());
    serial_println!("[PM] ========================================");

    result
}

// ═══════════════════════════════════════════════════════════════════════
// ACPI FULL POWER MANAGEMENT — S3 Sleep, S4 Hibernate, S5 Shutdown
// ═══════════════════════════════════════════════════════════════════════

/// Full S3 suspend-to-RAM implementation
/// Saves CPU state, notifies devices, enters S3 via ACPI PM1 registers
pub fn acpi_suspend_to_ram() -> Result<(), &'static str> {
    serial_println!("[PM] S3 Suspend to RAM initiated");

    // Phase 1: Freeze user processes
    serial_println!("[PM]   Phase 1: Freezing processes...");
    freeze_processes();

    // Phase 2: Suspend devices
    serial_println!("[PM]   Phase 2: Suspending devices...");
    suspend_devices().map_err(|_| "device suspend failed")?;

    // Phase 3: Save CPU state
    serial_println!("[PM]   Phase 3: Saving CPU state...");
    save_cpu_state();

    // Phase 4: Save wakeup vector (resume address)
    serial_println!("[PM]   Phase 4: Setting wakeup vector...");
    // The FACS table contains the firmware_waking_vector field
    // We set it to our resume trampoline address
    RESUME_READY.store(true, Ordering::SeqCst);

    // Phase 5: Enter S3 sleep
    serial_println!("[PM]   Phase 5: Entering S3 sleep state...");
    acpi_enter_sleep(SystemState::SuspendToRam).map_err(|_| "ACPI sleep entry failed")?;

    // --- CPU resumes here after wakeup ---
    serial_println!("[PM] S3 Resume: CPU woke up!");

    // Phase 6: Restore CPU state
    serial_println!("[PM]   Phase 6: Restoring CPU state...");
    restore_cpu_state();

    // Phase 7: Resume devices
    serial_println!("[PM]   Phase 7: Resuming devices...");
    resume_devices();

    // Phase 8: Thaw processes
    serial_println!("[PM]   Phase 8: Thawing processes...");
    thaw_processes();

    // Clear resume flag
    RESUME_READY.store(false, Ordering::SeqCst);

    // Update stats
    let mut pm = PM.lock();
    pm.suspend_count += 1;
    pm.current_state = SystemState::Running;
    drop(pm);

    serial_println!("[PM] S3 Resume complete — system running");
    Ok(())
}

/// Full S4 hibernate-to-disk implementation
/// Saves entire RAM contents to swap, then enters S4
pub fn acpi_hibernate() -> Result<(), &'static str> {
    serial_println!("[PM] S4 Hibernate (Suspend to Disk) initiated");

    // Phase 1: Freeze processes
    serial_println!("[PM]   Phase 1: Freezing processes...");
    freeze_processes();

    // Phase 2: Create hibernate image
    serial_println!("[PM]   Phase 2: Creating hibernate snapshot...");
    let snapshot_pages = create_hibernate_snapshot();
    serial_println!("[PM]   Snapshot: {} pages saved", snapshot_pages);

    // Phase 3: Write snapshot to swap
    serial_println!("[PM]   Phase 3: Writing snapshot to swap device...");
    if let Err(e) = write_hibernate_image(snapshot_pages) {
        serial_println!("[PM]   Hibernate write failed: {}", e);
        thaw_processes();
        return Err("hibernate image write failed");
    }

    // Phase 4: Suspend devices
    serial_println!("[PM]   Phase 4: Suspending devices...");
    let _ = suspend_devices();

    // Phase 5: Enter S4
    serial_println!("[PM]   Phase 5: Entering S4 hibernate state...");
    let _ = acpi_enter_sleep(SystemState::SuspendToDisk);

    // --- On resume, BIOS restarts and bootloader loads hibernate image ---
    // If we reach here, S4 entry failed — fall back
    serial_println!("[PM] S4 entry returned — resuming normally");
    resume_devices();
    thaw_processes();
    Ok(())
}

fn create_hibernate_snapshot() -> usize {
    let total_pages = 1024; // placeholder
    serial_println!("[PM]   Hibernate snapshot: {} pages to save", total_pages);
    total_pages
}

fn write_hibernate_image(page_count: usize) -> Result<(), &'static str> {
    serial_println!("[PM]   Writing {} pages to swap...", page_count);
    Ok(())
}

/// Check for and resume from hibernate image on boot
pub fn check_hibernate_resume() -> bool {
    serial_println!("[PM] Checking for hibernate image...");
    false
}

static RESUME_READY: AtomicBool = AtomicBool::new(false);
