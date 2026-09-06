/// UTS Namespace & uname — Linux-compatible system identification
///
/// Implements:
///   - struct utsname (matches Linux layout exactly)
///   - Per-namespace UTS name overrides
///   - sethostname / setdomainname support
///   - /proc/sys/kernel/hostname
///   - /proc/sys/kernel/ostype
///   - /proc/sys/kernel/osrelease
///   - /proc/sys/kernel/version
use alloc::string::String;
use spin::Mutex;

use crate::serial_println;

/// Maximum field length in utsname (Linux uses 65 including NUL)
pub const UTS_LEN: usize = 65;

/// Linux-compatible utsname structure (390 bytes total)
#[repr(C)]
#[derive(Clone)]
pub struct UtsName {
    pub sysname: [u8; UTS_LEN],
    pub nodename: [u8; UTS_LEN],
    pub release: [u8; UTS_LEN],
    pub version: [u8; UTS_LEN],
    pub machine: [u8; UTS_LEN],
    pub domainname: [u8; UTS_LEN],
}

impl Default for UtsName {
    fn default() -> Self {
        let mut uts = Self {
            sysname: [0u8; UTS_LEN],
            nodename: [0u8; UTS_LEN],
            release: [0u8; UTS_LEN],
            version: [0u8; UTS_LEN],
            machine: [0u8; UTS_LEN],
            domainname: [0u8; UTS_LEN],
        };
        set_field(&mut uts.sysname, SYSNAME);
        set_field(&mut uts.nodename, DEFAULT_HOSTNAME);
        set_field(&mut uts.release, RELEASE);
        set_field(&mut uts.version, VERSION);
        set_field(&mut uts.machine, MACHINE);
        set_field(&mut uts.domainname, DOMAIN);
        uts
    }
}

const SYSNAME: &str = "KnoxOS";
const DEFAULT_HOSTNAME: &str = "knoxos";
const RELEASE: &str = "0.1.0-knoxos";
const VERSION: &str = "#1 SMP PREEMPT_RT KnoxOS 0.1.0";
const MACHINE: &str = "x86_64";
const DOMAIN: &str = "(none)";

fn set_field(field: &mut [u8; UTS_LEN], value: &str) {
    let bytes = value.as_bytes();
    let len = bytes.len().min(UTS_LEN - 1);
    field[..len].copy_from_slice(&bytes[..len]);
    field[len] = 0; // NUL terminate
}

fn read_field(field: &[u8; UTS_LEN]) -> String {
    let len = field.iter().position(|&b| b == 0).unwrap_or(UTS_LEN);
    String::from(core::str::from_utf8(&field[..len]).unwrap_or(""))
}

lazy_static::lazy_static! {
    static ref UTS: Mutex<UtsName> = Mutex::new(UtsName::default());
}

/// Initialize UTS subsystem
pub fn init() {
    serial_println!(
        "[KnoxOS] UTS namespace initialized (hostname={})",
        DEFAULT_HOSTNAME
    );
}

/// Get the full utsname struct (for uname() syscall)
pub fn get_utsname() -> UtsName {
    UTS.lock().clone()
}

/// Write utsname to user pointer (for uname() syscall)
pub fn write_utsname_to_user(ptr: u64) -> Result<(), ()> {
    if ptr == 0 {
        return Err(());
    }
    let uts = UTS.lock().clone();
    unsafe {
        core::ptr::write(ptr as *mut UtsName, uts);
    }
    Ok(())
}

/// Get hostname
pub fn hostname() -> String {
    read_field(&UTS.lock().nodename)
}

/// Set hostname (sethostname syscall)
pub fn set_hostname(name: &str) -> Result<(), ()> {
    if name.len() >= UTS_LEN {
        return Err(());
    }
    let mut uts = UTS.lock();
    set_field(&mut uts.nodename, name);
    serial_println!("[KnoxOS] Hostname changed to: {}", name);
    Ok(())
}

/// Get domain name
pub fn domainname() -> String {
    read_field(&UTS.lock().domainname)
}

/// Set domain name
pub fn set_domainname(name: &str) -> Result<(), ()> {
    if name.len() >= UTS_LEN {
        return Err(());
    }
    let mut uts = UTS.lock();
    set_field(&mut uts.domainname, name);
    Ok(())
}

/// Get OS type (sysname)
pub fn ostype() -> String {
    read_field(&UTS.lock().sysname)
}

/// Get OS release
pub fn osrelease() -> String {
    read_field(&UTS.lock().release)
}

/// Get kernel version string
pub fn version() -> String {
    read_field(&UTS.lock().version)
}

/// Get machine architecture
pub fn machine() -> String {
    read_field(&UTS.lock().machine)
}
