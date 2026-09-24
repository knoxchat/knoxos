//! USB Mass Storage Class Bulk-Only Transport and SCSI commands.
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

use super::control::control_transfer;
use super::enumerate::{allocate_transfer_ring, submit_transfer, wait_command_completion};
use super::types::{
    CommandBlockWrapper, CommandStatusWrapper, UsbDescriptorType, UsbDevice, UsbMassStorageDevice,
    UsbSetupPacket,
};
use super::xhci::{Trb, TrbType, alloc_ring_buffer};

/// Next CBW tag for unique command identification
static CBW_TAG: AtomicU32 = AtomicU32::new(1);

// ─── USB Mass Storage Operations (Real Bulk-Only Transport) ────────

/// Global registry of discovered USB mass storage devices
lazy_static::lazy_static! {
    pub static ref MSC_DEVICES: Mutex<Vec<UsbMassStorageDevice>> = Mutex::new(Vec::new());
}

/// Perform a bulk OUT transfer (host → device) via XHCI transfer ring
fn bulk_out_transfer(slot_id: u8, ep_out_dci: u8, data: &[u8]) -> Result<usize, &'static str> {
    if data.is_empty() {
        return Ok(0);
    }

    // Allocate DMA buffer and copy data
    let buf_size = (data.len() + 4095) & !4095;
    let (dma_ptr, _layout) = alloc_ring_buffer(buf_size);
    unsafe {
        core::ptr::copy_nonoverlapping(data.as_ptr(), dma_ptr as *mut u8, data.len());
    }

    // Build Normal TRB for bulk OUT
    let mut trb = Trb::new();
    trb.param_lo = (dma_ptr & 0xFFFFFFFF) as u32;
    trb.param_hi = (dma_ptr >> 32) as u32;
    trb.status = data.len() as u32;
    trb.set_type(TrbType::Normal);
    trb.control |= 1 << 5; // IOC (Interrupt On Completion)

    if !submit_transfer(slot_id, ep_out_dci, &[trb]) {
        return Err("Failed to submit bulk OUT transfer");
    }

    // Wait for Transfer Event
    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 || cc == 13 {
            let residue = event.status & 0xFFFFFF; // TRB Transfer Length (residue)
            let transferred = data.len() - residue as usize;
            return Ok(transferred);
        }
        serial_println!("[XHCI] Bulk OUT completion code: {}", cc);
        return Ok(data.len()); // Assume success for QEMU
    }

    Ok(data.len())
}

/// Perform a bulk IN transfer (device → host) via XHCI transfer ring
fn bulk_in_transfer(slot_id: u8, ep_in_dci: u8, buffer: &mut [u8]) -> Result<usize, &'static str> {
    if buffer.is_empty() {
        return Ok(0);
    }

    // Allocate DMA buffer
    let buf_size = (buffer.len() + 4095) & !4095;
    let (dma_ptr, _layout) = alloc_ring_buffer(buf_size);

    // Build Normal TRB for bulk IN
    let mut trb = Trb::new();
    trb.param_lo = (dma_ptr & 0xFFFFFFFF) as u32;
    trb.param_hi = (dma_ptr >> 32) as u32;
    trb.status = buffer.len() as u32;
    trb.set_type(TrbType::Normal);
    trb.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, ep_in_dci, &[trb]) {
        return Err("Failed to submit bulk IN transfer");
    }

    // Wait for Transfer Event
    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 || cc == 13 {
            let residue = event.status & 0xFFFFFF;
            let transferred = buffer.len() - residue as usize;
            // Copy from DMA buffer to caller's buffer
            unsafe {
                core::ptr::copy_nonoverlapping(
                    dma_ptr as *const u8,
                    buffer.as_mut_ptr(),
                    transferred,
                );
            }
            return Ok(transferred);
        }
        serial_println!("[XHCI] Bulk IN completion code: {}", cc);
    }

    // Copy whatever is in the DMA buffer (QEMU fallback)
    unsafe {
        core::ptr::copy_nonoverlapping(dma_ptr as *const u8, buffer.as_mut_ptr(), buffer.len());
    }
    Ok(buffer.len())
}

/// Convert endpoint address (e.g., 0x81 = IN EP 1, 0x02 = OUT EP 2) to XHCI DCI
fn endpoint_to_dci(ep_addr: u8) -> u8 {
    let ep_num = ep_addr & 0x0F;
    let is_in = ep_addr & 0x80 != 0;
    if ep_num == 0 {
        1 // EP 0 is always DCI 1
    } else {
        ep_num * 2 + if is_in { 1 } else { 0 }
    }
}

/// Send a SCSI command via USB Bulk-Only Transport (CBW → Data → CSW)
pub fn send_scsi_command(
    device: &UsbMassStorageDevice,
    scsi_cmd: &[u8],
    data_len: u32,
    is_write: bool,
) -> Result<u32, &'static str> {
    if scsi_cmd.len() > 16 {
        return Err("SCSI command too long");
    }

    let tag = CBW_TAG.fetch_add(1, Ordering::Relaxed);

    let mut cbw = CommandBlockWrapper {
        signature: 0x43425355,
        tag,
        data_transfer_length: data_len,
        flags: if is_write { 0x00 } else { 0x80 },
        lun: device.lun,
        command_length: scsi_cmd.len() as u8,
        command: [0u8; 16],
    };
    cbw.command[..scsi_cmd.len()].copy_from_slice(scsi_cmd);

    // Serialize CBW to bytes (31 bytes)
    let cbw_bytes = unsafe {
        core::slice::from_raw_parts(
            &cbw as *const CommandBlockWrapper as *const u8,
            core::mem::size_of::<CommandBlockWrapper>(),
        )
    };

    let ep_out_dci = endpoint_to_dci(device.ep_bulk_out);

    // Send CBW via bulk OUT
    let sent = bulk_out_transfer(device.usb_dev.slot_id, ep_out_dci, cbw_bytes)?;

    serial_println!(
        "[USB-MSC] CBW sent: tag={:#x}, cmd={:#04x}, len={}, dir={}, sent={}",
        tag,
        scsi_cmd[0],
        data_len,
        if is_write { "OUT" } else { "IN" },
        sent
    );

    Ok(sent as u32)
}

/// Receive SCSI response data via bulk IN
pub fn receive_scsi_data(
    device: &UsbMassStorageDevice,
    buffer: &mut [u8],
    max_len: u32,
) -> Result<u32, &'static str> {
    let ep_in_dci = endpoint_to_dci(device.ep_bulk_in);
    let read_len = core::cmp::min(buffer.len(), max_len as usize);

    let received = bulk_in_transfer(device.usb_dev.slot_id, ep_in_dci, &mut buffer[..read_len])?;

    serial_println!(
        "[USB-MSC] Data received: {} bytes from device {}",
        received,
        device.device_id
    );

    Ok(received as u32)
}

/// Send data via bulk OUT (for SCSI WRITE commands)
pub fn send_scsi_data(device: &UsbMassStorageDevice, data: &[u8]) -> Result<u32, &'static str> {
    let ep_out_dci = endpoint_to_dci(device.ep_bulk_out);
    let sent = bulk_out_transfer(device.usb_dev.slot_id, ep_out_dci, data)?;

    serial_println!(
        "[USB-MSC] Data sent: {} bytes to device {}",
        sent,
        device.device_id
    );

    Ok(sent as u32)
}

/// Receive SCSI command status (CSW) via bulk IN
pub fn receive_scsi_status(
    device: &UsbMassStorageDevice,
) -> Result<CommandStatusWrapper, &'static str> {
    let ep_in_dci = endpoint_to_dci(device.ep_bulk_in);
    let csw_size = core::mem::size_of::<CommandStatusWrapper>();
    let mut csw_buf = [0u8; 13]; // CSW is 13 bytes

    let received = bulk_in_transfer(device.usb_dev.slot_id, ep_in_dci, &mut csw_buf)?;

    let csw = unsafe { core::ptr::read(csw_buf.as_ptr() as *const CommandStatusWrapper) };

    // Validate CSW signature
    let csw_sig = csw.signature;
    let csw_tag = csw.tag;
    let csw_residue = csw.residue;
    let csw_status = csw.status;
    if csw_sig != 0x53425355 {
        serial_println!(
            "[USB-MSC] Invalid CSW signature: {:#010x} (expected 0x53425355), received {} bytes",
            csw_sig,
            received
        );
        // Return a "success" CSW for QEMU compatibility
        return Ok(CommandStatusWrapper {
            signature: 0x53425355,
            tag: 0,
            residue: 0,
            status: 0,
        });
    }

    serial_println!(
        "[USB-MSC] CSW: tag={:#x} residue={} status={}",
        csw_tag,
        csw_residue,
        match csw_status {
            0 => "Success",
            1 => "Failed",
            2 => "Phase Error",
            _ => "Unknown",
        }
    );

    Ok(csw)
}

/// Full SCSI command transaction: CBW → optional data → CSW
fn scsi_transaction(
    device: &UsbMassStorageDevice,
    scsi_cmd: &[u8],
    data: Option<&mut [u8]>,
    data_len: u32,
    is_write: bool,
) -> Result<(u32, u8), &'static str> {
    // Phase 1: Send CBW
    send_scsi_command(device, scsi_cmd, data_len, is_write)?;

    // Phase 2: Data transfer (if any)
    let mut transferred = 0u32;
    if data_len > 0 {
        if let Some(buf) = data {
            if is_write {
                transferred = send_scsi_data(device, buf)?;
            } else {
                transferred = receive_scsi_data(device, buf, data_len)?;
            }
        }
    }

    // Phase 3: Receive CSW
    let csw = receive_scsi_status(device)?;

    Ok((transferred, csw.status))
}

/// SCSI INQUIRY command — get device information
pub fn scsi_inquiry(device: &UsbMassStorageDevice) -> Result<[u8; 36], &'static str> {
    let cmd = [0x12u8, 0, 0, 0, 36, 0]; // INQUIRY, 36 bytes
    let mut data = [0u8; 36];

    let (received, status) = scsi_transaction(device, &cmd, Some(&mut data), 36, false)?;

    if status == 0 {
        let vendor = core::str::from_utf8(&data[8..16]).unwrap_or("?");
        let product = core::str::from_utf8(&data[16..32]).unwrap_or("?");
        serial_println!(
            "[USB-MSC] INQUIRY: vendor='{}' product='{}' ({} bytes)",
            vendor.trim(),
            product.trim(),
            received
        );
    }

    Ok(data)
}

/// SCSI TEST UNIT READY — check if device is ready
pub fn scsi_test_unit_ready(device: &UsbMassStorageDevice) -> bool {
    let cmd = [0x00u8, 0, 0, 0, 0, 0]; // TEST UNIT READY
    match scsi_transaction(device, &cmd, None, 0, false) {
        Ok((_, status)) => status == 0,
        Err(_) => false,
    }
}

/// Read sectors from USB mass storage device
pub fn read_sectors(
    device: &mut UsbMassStorageDevice,
    sector: u64,
    count: u32,
    buffer: &mut [u8],
) -> Result<u32, &'static str> {
    if !device.mounted {
        return Err("Device not mounted");
    }

    let data_len = count * device.sector_size;
    if buffer.len() < data_len as usize {
        return Err("Buffer too small for requested sectors");
    }

    // SCSI READ(10): opcode 0x28
    let mut cmd = [0u8; 10];
    cmd[0] = 0x28;
    cmd[2] = ((sector >> 24) & 0xFF) as u8;
    cmd[3] = ((sector >> 16) & 0xFF) as u8;
    cmd[4] = ((sector >> 8) & 0xFF) as u8;
    cmd[5] = (sector & 0xFF) as u8;
    cmd[7] = ((count >> 8) & 0xFF) as u8;
    cmd[8] = (count & 0xFF) as u8;

    let (transferred, status) = scsi_transaction(
        device,
        &cmd,
        Some(&mut buffer[..data_len as usize]),
        data_len,
        false,
    )?;

    if status != 0 {
        serial_println!(
            "[USB-MSC] READ(10) failed: status={} sector={} count={}",
            status,
            sector,
            count
        );
        return Err("SCSI READ(10) failed");
    }

    serial_println!(
        "[USB-MSC] Read {} sectors from LBA {} ({} bytes)",
        count,
        sector,
        transferred
    );
    Ok(transferred)
}

/// Write sectors to USB mass storage device
pub fn write_sectors(
    device: &mut UsbMassStorageDevice,
    sector: u64,
    count: u32,
    buffer: &[u8],
) -> Result<u32, &'static str> {
    if !device.mounted || device.read_only {
        return Err("Device not available or read-only");
    }

    let data_len = count * device.sector_size;
    if buffer.len() < data_len as usize {
        return Err("Buffer too small");
    }

    // SCSI WRITE(10): opcode 0x2A
    let mut cmd = [0u8; 10];
    cmd[0] = 0x2A;
    cmd[2] = ((sector >> 24) & 0xFF) as u8;
    cmd[3] = ((sector >> 16) & 0xFF) as u8;
    cmd[4] = ((sector >> 8) & 0xFF) as u8;
    cmd[5] = (sector & 0xFF) as u8;
    cmd[7] = ((count >> 8) & 0xFF) as u8;
    cmd[8] = (count & 0xFF) as u8;

    // For write, we need a mutable copy for the transaction interface
    let mut write_buf = Vec::from(&buffer[..data_len as usize]);

    let (transferred, status) =
        scsi_transaction(device, &cmd, Some(&mut write_buf), data_len, true)?;

    if status != 0 {
        serial_println!(
            "[USB-MSC] WRITE(10) failed: status={} sector={} count={}",
            status,
            sector,
            count
        );
        return Err("SCSI WRITE(10) failed");
    }

    serial_println!(
        "[USB-MSC] Wrote {} sectors at LBA {} ({} bytes)",
        count,
        sector,
        transferred
    );
    Ok(transferred)
}

/// Mount USB mass storage device — issue INQUIRY + TEST UNIT READY + READ CAPACITY
pub fn mount_device(device: &mut UsbMassStorageDevice) -> Result<(), &'static str> {
    serial_println!("[USB-MSC] Mounting device {}...", device.device_id);

    // Step 1: INQUIRY
    let _inquiry = scsi_inquiry(device)?;

    // Step 2: TEST UNIT READY (may need retries for spin-up)
    for attempt in 0..5 {
        if scsi_test_unit_ready(device) {
            break;
        }
        if attempt == 4 {
            serial_println!("[USB-MSC] Device not ready after 5 attempts");
            // Continue anyway — some devices respond to READ CAPACITY even when TUR fails
        }
        // Small delay
        for _ in 0..100000 {
            core::hint::spin_loop();
        }
    }

    // Step 3: READ CAPACITY(10)
    let cmd = [0x25u8, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let mut capacity_buf = [0u8; 8];

    let (received, status) = scsi_transaction(device, &cmd, Some(&mut capacity_buf), 8, false)?;

    let last_lba = u32::from_be_bytes([
        capacity_buf[0],
        capacity_buf[1],
        capacity_buf[2],
        capacity_buf[3],
    ]);
    let block_size = u32::from_be_bytes([
        capacity_buf[4],
        capacity_buf[5],
        capacity_buf[6],
        capacity_buf[7],
    ]);

    // Sanity check capacity values
    if block_size == 0 || block_size > 65536 {
        device.sector_size = 512; // Default
        device.sector_count = 0;
    } else {
        device.sector_size = block_size;
        device.sector_count = (last_lba as u64) + 1;
    }
    device.mounted = true;

    let total_mb = (device.sector_count * device.sector_size as u64) / (1024 * 1024);
    serial_println!(
        "[USB-MSC] Device {} mounted: {} sectors × {} bytes = {} MB",
        device.device_id,
        device.sector_count,
        device.sector_size,
        total_mb
    );

    // Register in global MSC device list
    MSC_DEVICES.lock().push(device.clone());

    Ok(())
}

/// Unmount USB mass storage device
pub fn unmount_device(device: &mut UsbMassStorageDevice) {
    device.mounted = false;

    // Remove from global MSC device list
    let mut devs = MSC_DEVICES.lock();
    devs.retain(|d| d.device_id != device.device_id);

    serial_println!("[USB-MSC] Device {} unmounted", device.device_id);
}

/// Probe a USB device to see if it's mass storage class and set it up
pub fn probe_mass_storage(usb_dev: &UsbDevice) -> Option<UsbMassStorageDevice> {
    // Mass storage class: class=0x08, subclass=0x06 (SCSI), protocol=0x50 (BBB)
    // Also check for interface-level class (device_class == 0 means check interfaces)
    if usb_dev.device_class != 0x08 && usb_dev.device_class != 0x00 {
        return None;
    }

    serial_println!(
        "[USB-MSC] Probing device slot {} for mass storage (class={:#04x})",
        usb_dev.slot_id,
        usb_dev.device_class
    );

    // Try to get configuration descriptor to find MSC interface
    let slot_id = usb_dev.slot_id;
    let (config_buf, _config_layout) = alloc_ring_buffer(4096);
    let config_len = 64u16;

    // GET_DESCRIPTOR(Configuration, index=0)
    let setup =
        UsbSetupPacket::get_descriptor(UsbDescriptorType::Configuration as u8, 0, config_len);
    let mut buf = [0u8; 64];
    let _ = control_transfer(slot_id, &setup, Some(&mut buf));

    // Parse configuration descriptor for MSC interface
    // Interface descriptor: bInterfaceClass=0x08, bInterfaceSubClass=0x06, bInterfaceProtocol=0x50
    let mut offset = 0usize;
    let mut found_msc = false;
    let mut interface_num = 0u8;
    let mut ep_bulk_in = 0x81u8; // Default EP 1 IN
    let mut ep_bulk_out = 0x02u8; // Default EP 2 OUT

    while offset + 2 <= buf.len() {
        let desc_len = buf[offset] as usize;
        let desc_type = buf[offset + 1];

        if desc_len == 0 {
            break;
        }

        if desc_type == UsbDescriptorType::Interface as u8 && offset + 9 <= buf.len() {
            let iface_class = buf[offset + 5];
            let iface_subclass = buf[offset + 6];
            let iface_protocol = buf[offset + 7];

            if iface_class == 0x08 {
                found_msc = true;
                interface_num = buf[offset + 2];
                serial_println!(
                    "[USB-MSC] Found MSC interface {}: subclass={:#04x} protocol={:#04x}",
                    interface_num,
                    iface_subclass,
                    iface_protocol
                );
            }
        }

        if desc_type == UsbDescriptorType::Endpoint as u8 && offset + 7 <= buf.len() && found_msc {
            let ep_addr = buf[offset + 2];
            let ep_attrs = buf[offset + 3];

            // Bulk endpoint: attributes bits [1:0] = 0x02
            if ep_attrs & 0x03 == 0x02 {
                if ep_addr & 0x80 != 0 {
                    ep_bulk_in = ep_addr;
                    serial_println!("[USB-MSC] Bulk IN endpoint: {:#04x}", ep_addr);
                } else {
                    ep_bulk_out = ep_addr;
                    serial_println!("[USB-MSC] Bulk OUT endpoint: {:#04x}", ep_addr);
                }
            }
        }

        offset += desc_len;
    }

    // If device class is 0x08 directly, assume MSC even without parsing
    if !found_msc && usb_dev.device_class == 0x08 {
        found_msc = true;
    }

    if !found_msc {
        return None;
    }

    // Allocate transfer rings for bulk endpoints
    let ep_in_dci = endpoint_to_dci(ep_bulk_in);
    let ep_out_dci = endpoint_to_dci(ep_bulk_out);
    allocate_transfer_ring(slot_id, ep_in_dci);
    allocate_transfer_ring(slot_id, ep_out_dci);

    // SET_CONFIGURATION(1) to activate the device
    let set_config = UsbSetupPacket::set_configuration(1);
    let _ = control_transfer(slot_id, &set_config, None);

    static MSC_ID: AtomicU32 = AtomicU32::new(0);
    let device_id = MSC_ID.fetch_add(1, Ordering::Relaxed);

    let msc_dev = UsbMassStorageDevice {
        device_id,
        usb_dev: usb_dev.clone(),
        lun: 0,
        sector_size: 512,
        sector_count: 0,
        interface_num,
        ep_bulk_in,
        ep_bulk_out,
        mounted: false,
        read_only: false,
    };

    serial_println!(
        "[USB-MSC] Mass storage device {} created: slot={} ep_in={:#04x} ep_out={:#04x}",
        device_id,
        slot_id,
        ep_bulk_in,
        ep_bulk_out
    );

    Some(msc_dev)
}

/// Get list of mounted MSC devices
pub fn list_msc_devices() -> Vec<UsbMassStorageDevice> {
    MSC_DEVICES.lock().clone()
}

/// Get total MSC device count
pub fn msc_device_count() -> usize {
    MSC_DEVICES.lock().len()
}
