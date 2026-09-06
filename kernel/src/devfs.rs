use alloc::format;
/// devfs — Device filesystem (/dev)
/// Linux-compatible device nodes and management
///
/// Creates standard Linux /dev entries:
///   /dev/null, /dev/zero, /dev/random, /dev/urandom
///   /dev/tty, /dev/console, /dev/ptmx
///   /dev/stdin, /dev/stdout, /dev/stderr
///   /dev/loop0..N (loop devices)
///   /dev/sda, /dev/vda (block devices)
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Device type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeviceType {
    CharDevice,
    BlockDevice,
}

/// Device entry
#[derive(Debug, Clone)]
pub struct DeviceEntry {
    pub name: String,
    pub dev_type: DeviceType,
    pub major: u32,
    pub minor: u32,
    pub mode: u32,
}

lazy_static::lazy_static! {
    static ref DEVICES: Mutex<Vec<DeviceEntry>> = Mutex::new(Vec::new());
}

/// Read from a device
pub fn read_device(name: &str, buf: &mut [u8]) -> Result<usize, i32> {
    match name {
        "null" => Ok(0), // Always EOF
        "zero" => {
            for b in buf.iter_mut() {
                *b = 0;
            }
            Ok(buf.len())
        }
        "random" | "urandom" => {
            // Fill with pseudo-random data
            let mut seed = crate::arch_compat::read_tsc();
            for b in buf.iter_mut() {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                *b = (seed >> 33) as u8;
            }
            Ok(buf.len())
        }
        "full" => {
            Err(-28) // ENOSPC on write, returns zeros on read
        }
        _ => Err(-19), // ENODEV
    }
}

/// Write to a device
pub fn write_device(name: &str, buf: &[u8]) -> Result<usize, i32> {
    match name {
        "null" => Ok(buf.len()),               // Discard everything
        "zero" => Ok(buf.len()),               // Discard everything
        "full" => Err(-28),                    // ENOSPC
        "random" | "urandom" => Ok(buf.len()), // Accept entropy
        "tty" | "console" => {
            // Write to VGA/serial
            let s = core::str::from_utf8(buf).unwrap_or("");
            crate::serial_println!("{}", s);
            Ok(buf.len())
        }
        _ => Err(-19), // ENODEV
    }
}

/// Register a device
pub fn register_device(name: &str, dev_type: DeviceType, major: u32, minor: u32, mode: u32) {
    let mut devices = DEVICES.lock();
    devices.push(DeviceEntry {
        name: String::from(name),
        dev_type,
        major,
        minor,
        mode,
    });
}

/// List all registered devices
pub fn list_devices() -> Vec<DeviceEntry> {
    DEVICES.lock().clone()
}

/// Check if a device exists
pub fn device_exists(name: &str) -> bool {
    DEVICES.lock().iter().any(|d| d.name == name)
}

/// Make a device node (mknod)
pub fn mknod(
    name: &str,
    dev_type: DeviceType,
    major: u32,
    minor: u32,
    mode: u32,
) -> Result<(), i32> {
    let uid = crate::users::get_current_uid();
    if uid != 0 {
        return Err(-1); // EPERM (only root can create device nodes)
    }

    if device_exists(name) {
        return Err(-17); // EEXIST
    }

    register_device(name, dev_type, major, minor, mode);

    let vfs_file_type = match dev_type {
        DeviceType::CharDevice => crate::vfs::FileType::CharDevice,
        DeviceType::BlockDevice => crate::vfs::FileType::BlockDevice,
    };

    // Create in VFS
    let path = format!("/dev/{}", name);
    let mut vfs = crate::vfs::VFS.lock();
    let _ = vfs.create_file_at_path(&path, vfs_file_type, &[], mode as u16);
    drop(vfs);

    Ok(())
}

/// Initialize /dev filesystem
pub fn init() {
    // Ensure /dev directory exists
    let mut vfs = crate::vfs::VFS.lock();
    let _ = vfs.mkdir("/dev/pts", 0o755);
    let _ = vfs.mkdir("/dev/shm", 0o1777);
    let _ = vfs.mkdir("/dev/mqueue", 0o1777);
    drop(vfs);

    // Register standard character devices
    register_device("null", DeviceType::CharDevice, 1, 3, 0o666);
    register_device("zero", DeviceType::CharDevice, 1, 5, 0o666);
    register_device("full", DeviceType::CharDevice, 1, 7, 0o666);
    register_device("random", DeviceType::CharDevice, 1, 8, 0o666);
    register_device("urandom", DeviceType::CharDevice, 1, 9, 0o666);
    register_device("tty", DeviceType::CharDevice, 5, 0, 0o666);
    register_device("console", DeviceType::CharDevice, 5, 1, 0o600);
    register_device("ptmx", DeviceType::CharDevice, 5, 2, 0o666);

    // FD symlinks
    register_device("stdin", DeviceType::CharDevice, 0, 0, 0o666);
    register_device("stdout", DeviceType::CharDevice, 0, 1, 0o666);
    register_device("stderr", DeviceType::CharDevice, 0, 2, 0o666);

    // Loop devices
    for i in 0..8 {
        register_device(&format!("loop{}", i), DeviceType::BlockDevice, 7, i, 0o660);
    }

    // Create device files in VFS
    let mut vfs = crate::vfs::VFS.lock();
    let devices = DEVICES.lock();
    for dev in devices.iter() {
        let path = format!("/dev/{}", dev.name);
        let ft = match dev.dev_type {
            DeviceType::CharDevice => crate::vfs::FileType::CharDevice,
            DeviceType::BlockDevice => crate::vfs::FileType::BlockDevice,
        };
        let _ = vfs.create_file_at_path(&path, ft, &[], dev.mode as u16);
    }
    drop(devices);
    drop(vfs);

    serial_println!("[KnoxOS] Device filesystem initialized (/dev)");
}
