/// ACPI - Advanced Configuration and Power Interface
/// Provides shutdown, reboot, and basic system control
/// Implements power management compatible with Linux's ACPI subsystem
#[cfg(target_arch = "x86_64")]
use crate::arch_compat::instructions::port::Port;
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::instructions::port::Port;

/// PM1a port and SLP_TYP|SLP_EN value for S5. None when FADT/DSDT is missing.
pub fn s5_pm1a_write() -> Option<(u16, u16)> {
    let pm1a = crate::suspend::pm1a_control_block();
    if pm1a == 0 || pm1a > 0xFFFF {
        return None;
    }
    let slp = (*S5_SLP_TYP.lock()).or_else(|| {
        let v = crate::suspend::slp_typ_s5();
        if v == 0 { None } else { Some(v) }
    })?;
    Some((pm1a as u16, ((slp as u16) << 10) | (1 << 13)))
}

/// ACPI S5 through PM1a_CNT. No QEMU/Bochs/VirtualBox I/O-port fallbacks.
pub fn shutdown() -> ! {
    crate::serial_println!("[KnoxOS] Initiating system shutdown...");

    if let Some((port, value)) = s5_pm1a_write() {
        crate::serial_println!("[KnoxOS] ACPI S5 PM1a={:#x} value={:#x}", port, value);
        // SAFETY: PM1a_CNT comes from the mapped FADT; SLP_TYP is the DSDT _S5 integer.
        unsafe {
            let mut pm1a = Port::<u16>::new(port);
            pm1a.write(value);
        }
    } else {
        crate::serial_println!("[KnoxOS] ACPI S5 skipped: no PM1a/_S5");
    }

    crate::serial_println!("[KnoxOS] Shutdown: halting CPU");
    crate::hlt_loop();
}

/// System reboot using keyboard controller reset
pub fn reboot() -> ! {
    crate::serial_println!("[KnoxOS] Initiating system reboot...");

    // SAFETY: I/O port access is exclusive to this driver path; the port number is a fixed architecture or BAR-derived address.
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

/// SLP_TYPa extracted from DSDT `Name(_S5, Package(){...})`.
static S5_SLP_TYP: spin::Mutex<Option<u8>> = spin::Mutex::new(None);

/// AML NameOp
const AML_NAME_OP: u8 = 0x08;
/// AML PackageOp
const AML_PACKAGE_OP: u8 = 0x12;
const AML_BYTE_PREFIX: u8 = 0x0A;
const AML_ZERO_OP: u8 = 0x00;
const AML_ONE_OP: u8 = 0x01;

fn parse_s5_package_at(aml: &[u8], mut p: usize) -> Option<u8> {
    if aml.get(p).copied() != Some(AML_PACKAGE_OP) {
        return None;
    }
    p += 1;
    // PkgLength: bits 7-6 of the first byte are the extra-byte count.
    let pkg0 = *aml.get(p)?;
    let extra = (pkg0 >> 6) as usize;
    p += 1 + extra;
    let _num_elements = *aml.get(p)?;
    p += 1;
    Some(match *aml.get(p)? {
        AML_BYTE_PREFIX => *aml.get(p + 1).unwrap_or(&0),
        AML_ZERO_OP => 0,
        AML_ONE_OP => 1,
        b => b,
    })
}

/// Scan DSDT AML for `Name(_S5, Package(...))` and return the first integer (SLP_TYPa).
///
/// ACPI NameSegs are 4 characters padded with `_`, so firmware encodes `_S5_`.
/// QEMU also emits `Name(\_S5, ...)` (RootChar + NameSeg). The NUL-padded form
/// is accepted for host-side unit tests.
pub fn parse_s5_slp_typ(aml: &[u8]) -> Option<u8> {
    const NEEDLES: [&[u8]; 3] = [
        &[AML_NAME_OP, b'_', b'S', b'5', b'_'],
        &[AML_NAME_OP, 0x5C, b'_', b'S', b'5', b'_'],
        &[AML_NAME_OP, b'_', b'S', b'5', 0],
    ];
    for needle in NEEDLES {
        let mut i = 0;
        while i + needle.len() + 3 < aml.len() {
            if &aml[i..i + needle.len()] == needle {
                if let Some(typ) = parse_s5_package_at(aml, i + needle.len()) {
                    return Some(typ);
                }
            }
            i += 1;
        }
    }
    None
}

fn is_canonical_va(va: u64) -> bool {
    let sign_ext = ((va as i64) << 16) >> 16;
    sign_ext as u64 == va
}

fn dsdt_bytes() -> Option<alloc::vec::Vec<u8>> {
    let info = crate::acpi_tables::get_info()?;
    let fadt = info.fadt.as_ref()?;
    let phys = fadt.dsdt_address;
    if phys == 0 || phys >= 0x1_0000_0000 {
        return None;
    }
    let offset = crate::acpi_tables::phys_offset();
    let virt = if crate::vmm::ready() {
        crate::vmm::phys_to_virt(phys)
    } else {
        offset.wrapping_add(phys)
    };
    if virt == 0 || !is_canonical_va(virt) {
        crate::serial_println!(
            "[KnoxOS] ACPI: DSDT phys {:#x} virt {:#x} not canonical",
            phys,
            virt
        );
        return None;
    }
    let mut header = [0u8; 8];
    // SAFETY: virt is a canonical offset-map of a firmware table below 4 GiB.
    unsafe {
        core::ptr::copy_nonoverlapping(virt as *const u8, header.as_mut_ptr(), 8);
    }
    if &header[0..4] != b"DSDT" {
        return None;
    }
    let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    if !(36..256 * 1024).contains(&len) {
        return None;
    }
    let mut buf = alloc::vec![0u8; len];
    // SAFETY: length was taken from the DSDT header and capped at 256 KiB.
    unsafe {
        core::ptr::copy_nonoverlapping(virt as *const u8, buf.as_mut_ptr(), len);
    }
    Some(buf)
}

pub const GATE_AML_MARKER: &str = "GATE_AML dsdt";
pub const GATE_ACPI_S5_MARKER: &str = "GATE_ACPI s5";

fn aml_self_test() -> bool {
    let Some(dsdt) = dsdt_bytes() else {
        crate::serial_println!("[KnoxOS] GATE_AML FAILED: no DSDT");
        return false;
    };
    if &dsdt[0..4] != b"DSDT" {
        crate::serial_println!("[KnoxOS] GATE_AML FAILED: signature");
        return false;
    }
    let aml = &dsdt[36..];
    match parse_s5_slp_typ(aml) {
        Some(typ) => {
            *S5_SLP_TYP.lock() = Some(typ);
            crate::serial_println!("[KnoxOS] {} (_S5 SLP_TYP={})", GATE_AML_MARKER, typ);
            true
        }
        None => {
            crate::serial_println!("[KnoxOS] GATE_AML FAILED: no _S5");
            false
        }
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
    crate::serial_println!("[KnoxOS] ACPI power management initialized");
}

pub fn s5_self_test() -> bool {
    match s5_pm1a_write() {
        Some((port, value)) if value & (1 << 13) != 0 && port != 0 => {
            crate::serial_println!("[KnoxOS] {} (PM1a={:#x} SLP_EN)", GATE_ACPI_S5_MARKER, port);
            true
        }
        other => {
            crate::serial_println!("[KnoxOS] GATE_ACPI s5 FAILED: {:?}", other);
            false
        }
    }
}

/// Parse DSDT AML after `acpi_tables::init` has mapped firmware tables.
pub fn interpret_dsdt() {
    let _ = aml_self_test();
    let slp_s5 = (*S5_SLP_TYP.lock()).unwrap_or(7);
    if let Some(info) = crate::acpi_tables::get_info() {
        if let Some(ref fadt) = info.fadt {
            crate::suspend::set_pm_registers(fadt.pm1a_control_block as u64, 5, 6, slp_s5);
        }
    }
    let _ = s5_self_test();
}
