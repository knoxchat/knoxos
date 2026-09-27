use alloc::vec::Vec;

/// Read raw blocks from underlying vdev
pub fn read_vdev_block(device: &str, offset: u64, size: usize) -> Result<Vec<u8>, &'static str> {
    let total_bytes = size.div_ceil(512) * 512;
    let mut buf = alloc::vec![0u8; total_bytes];
    if device.starts_with("/dev/nvme") {
        let lba = offset / 512;
        let count = size.div_ceil(512) as u32;
        if crate::nvme::read_blocks(1, lba, count, &mut buf) {
            buf.truncate(size);
            Ok(buf)
        } else {
            Err("NVMe read failed")
        }
    } else if device.starts_with("/dev/sd") {
        let port = device.chars().nth(7).map(|c| (c as u8) - b'a').unwrap_or(0);
        let lba = offset / 512;
        let count = size.div_ceil(512) as u16;
        if crate::ahci::read_sectors(port, lba, count, &mut buf) {
            buf.truncate(size);
            Ok(buf)
        } else {
            Err("AHCI read failed")
        }
    } else {
        crate::vfs::read_file_dispatch(device).ok_or("device not found")
    }
}

/// Write raw blocks to underlying vdev
pub fn write_vdev_block(device: &str, offset: u64, data: &[u8]) -> Result<(), &'static str> {
    if device.starts_with("/dev/nvme") {
        let lba = offset / 512;
        let count = data.len().div_ceil(512) as u32;
        if crate::nvme::write_blocks(1, lba, count, data) {
            Ok(())
        } else {
            Err("NVMe write failed")
        }
    } else if device.starts_with("/dev/sd") {
        let port = device.chars().nth(7).map(|c| (c as u8) - b'a').unwrap_or(0);
        let lba = offset / 512;
        let count = data.len().div_ceil(512) as u16;
        if crate::ahci::write_sectors(port, lba, count, data) {
            Ok(())
        } else {
            Err("AHCI write failed")
        }
    } else {
        let _ = crate::vfs::write_file_dispatch(device, data);
        Ok(())
    }
}

/// Read and validate ZFS label from a block device
/// ZFS has 4 label copies: L0 at 0, L1 at 256K, L2 at end-256K, L3 at end
pub fn read_zfs_label(device: &str) -> Result<Vec<u8>, &'static str> {
    // Label 0 starts at offset 0, first 16K is blank, then uberblock array
    let data = read_vdev_block(device, 0, 256 * 1024)?;
    if data.len() < 256 * 1024 {
        return Err("read too short for ZFS label");
    }

    // Check for nvpair format at offset 16K (name-value pair list)
    // The nvlist header starts with encoding (0x01) and endianness (0x01 for native)
    if data.len() > 16384 + 8 && data[16384] == 0x01 {
        crate::serial_println!("[ZFS] Found valid ZFS label on {}", device);
        Ok(data)
    } else {
        Err("no valid ZFS label found")
    }
}
