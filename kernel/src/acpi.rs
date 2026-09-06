/// ACPI - Advanced Configuration and Power Interface
/// Provides shutdown, reboot, and basic system control
/// Implements power management compatible with Linux's ACPI subsystem
#[cfg(target_arch = "x86_64")]
use crate::arch_compat::instructions::port::Port;
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::instructions::port::Port;

/// ACPI shutdown using various methods
pub fn shutdown() -> ! {
    crate::serial_println!("[KnoxOS] Initiating system shutdown...");

    // Method 1: QEMU debug exit (for testing)
    unsafe {
        let mut port = Port::<u32>::new(0x604);
        port.write(0x2000);
    }

    // Method 2: Bochs/older QEMU
    unsafe {
        let mut port = Port::<u16>::new(0xB004);
        port.write(0x2000);
    }

    // Method 3: QEMU newer versions
    unsafe {
        let mut port = Port::<u16>::new(0x604);
        port.write(0x2000);
    }

    // Method 4: VirtualBox
    unsafe {
        let mut port = Port::<u16>::new(0x4004);
        port.write(0x3400);
    }

    // If all methods fail, halt
    crate::serial_println!("[KnoxOS] Shutdown failed, halting CPU");
    crate::hlt_loop();
}

/// System reboot using keyboard controller reset
pub fn reboot() -> ! {
    crate::serial_println!("[KnoxOS] Initiating system reboot...");

    unsafe {
        // Method 1: Keyboard controller reset (8042)
        let mut cmd_port = Port::<u8>::new(0x64);
        let mut data_port = Port::<u8>::new(0x60);

        // Wait for input buffer to be empty
        for _ in 0..10000 {
            let status = cmd_port.read();
            if status & 0x02 == 0 {
                break;
            }
        }

        // Send reset command
        cmd_port.write(0xFE);

        // Method 2: Triple fault (if keyboard controller reset fails)
        // Load a null IDT and trigger an interrupt
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "lidt [{}]",
            in(reg) &NULL_IDT_DESC as *const _ as u64,
            options(noreturn)
        );
    }

    // Fallback: loop forever on non-x86_64 or if the above didn't work
    loop {
        crate::arch_compat::instructions::interrupts::hlt();
    }
}

/// Null IDT descriptor for triple-fault reboot
#[repr(C, packed)]
struct IdtDescriptor {
    limit: u16,
    base: u64,
}

static NULL_IDT_DESC: IdtDescriptor = IdtDescriptor { limit: 0, base: 0 };

/// Power states (matching Linux ACPI S-states)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerState {
    S0Working,   // Normal operation
    S1Standby,   // CPU stops, RAM refreshed
    S3Suspend,   // Suspend to RAM
    S4Hibernate, // Suspend to disk
    S5SoftOff,   // Soft off (shutdown)
}

/// Current power state
static POWER_STATE: spin::Mutex<PowerState> = spin::Mutex::new(PowerState::S0Working);

/// Get current power state
pub fn current_state() -> PowerState {
    *POWER_STATE.lock()
}

/// Request a power state transition
pub fn request_state(state: PowerState) {
    match state {
        PowerState::S5SoftOff => shutdown(),
        PowerState::S0Working => { /* Already running */ }
        PowerState::S1Standby => {
            crate::serial_println!("[KnoxOS] Entering S1 standby...");
            // S1: CPU stops executing, caches flushed, RAM refreshed
            crate::arch_compat::instructions::interrupts::hlt();
            crate::serial_println!("[KnoxOS] Resumed from S1 standby");
        }
        PowerState::S3Suspend => {
            if let Err(e) = crate::suspend::suspend() {
                crate::serial_println!("[KnoxOS] S3 suspend failed: {}", e);
            }
        }
        PowerState::S4Hibernate => {
            if let Err(e) = crate::suspend::hibernate() {
                crate::serial_println!("[KnoxOS] S4 hibernate failed: {}", e);
            }
        }
    }
}

/// CPU halt (low-power idle)
pub fn cpu_idle() {
    crate::arch_compat::instructions::interrupts::hlt();
}

/// Read CPU temperature from the thermal monitoring subsystem
pub fn cpu_temperature() -> Option<u32> {
    // Read from real thermal zone (millidegrees → degrees)
    crate::thermal::get_temperature("cpu-package").map(|milli_c| (milli_c / 1000) as u32)
}

/// Battery info from ACPI battery monitor
#[derive(Debug, Clone)]
pub struct BatteryInfo {
    pub present: bool,
    pub charging: bool,
    pub percentage: u8,
    pub voltage_mv: u32,
}

pub fn battery_info() -> Option<BatteryInfo> {
    let info = crate::battery::get_info();
    match info.state {
        crate::battery::BatteryState::NotPresent => None,
        _ => Some(BatteryInfo {
            present: true,
            charging: info.state == crate::battery::BatteryState::Charging,
            percentage: info.charge_percent,
            voltage_mv: info.voltage_mv,
        }),
    }
}

/// Initialize ACPI subsystem
///
/// Note: Full ACPI table parsing (RSDP/RSDT/XSDT/MADT/FADT/HPET/MCFG)
/// is handled by `acpi_tables::init()` which is called during early boot
/// with the physical memory offset. This function initializes the power
/// management state machine and wires up to the thermal + battery monitors.
pub fn init() {
    *POWER_STATE.lock() = PowerState::S0Working;

    // Wire FADT PM registers into the suspend subsystem if available
    if let Some(info) = crate::acpi_tables::get_info() {
        if let Some(ref fadt) = info.fadt {
            crate::suspend::set_pm_registers(
                fadt.pm1a_control_block as u64,
                5, // SLP_TYPa for S3 (typical PIIX4/ICH value)
                6, // SLP_TYPa for S4
                7, // SLP_TYPa for S5
            );
        }
    }

    crate::serial_println!("[KnoxOS] ACPI power management initialized");
}
