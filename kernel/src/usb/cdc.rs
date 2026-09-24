//! USB CDC ACM serial port probe and line-coding placeholders.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// USB CDC ACM Serial Driver
// ═══════════════════════════════════════════════════════════════════════

/// CDC ACM serial port
#[derive(Debug, Clone)]
pub struct CdcAcmPort {
    pub device_id: u8,
    pub name: String,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub stop_bits: u8,
    pub parity: u8,
    pub dtr: bool,
    pub rts: bool,
    pub rx_buf: Vec<u8>,
    pub tx_buf: Vec<u8>,
}

lazy_static::lazy_static! {
    static ref CDC_ACM_PORTS: Mutex<Vec<CdcAcmPort>> = Mutex::new(Vec::new());
}

/// Probe USB device for CDC ACM interface (class 0x02, subclass 0x02)
pub fn cdc_acm_probe(device_id: u8, name: &str) -> bool {
    let mut ports = CDC_ACM_PORTS.lock();
    ports.push(CdcAcmPort {
        device_id,
        name: String::from(name),
        baud_rate: 115200,
        data_bits: 8,
        stop_bits: 1,
        parity: 0,
        dtr: false,
        rts: false,
        rx_buf: Vec::new(),
        tx_buf: Vec::new(),
    });
    serial_println!("[USB-Serial] CDC ACM port '{}' probed", name);
    true
}

/// Set serial line coding (baud rate, data bits, etc.)
pub fn cdc_acm_set_line_coding(
    port_idx: usize,
    baud: u32,
    data_bits: u8,
    stop_bits: u8,
    parity: u8,
) -> bool {
    let mut ports = CDC_ACM_PORTS.lock();
    if let Some(port) = ports.get_mut(port_idx) {
        port.baud_rate = baud;
        port.data_bits = data_bits;
        port.stop_bits = stop_bits;
        port.parity = parity;
        true
    } else {
        false
    }
}

/// Write data to serial port
pub fn cdc_acm_write(port_idx: usize, data: &[u8]) -> usize {
    let mut ports = CDC_ACM_PORTS.lock();
    if let Some(port) = ports.get_mut(port_idx) {
        port.tx_buf.extend_from_slice(data);
        data.len()
    } else {
        0
    }
}

/// Read data from serial port receive buffer
pub fn cdc_acm_read(port_idx: usize, buf: &mut [u8]) -> usize {
    let mut ports = CDC_ACM_PORTS.lock();
    if let Some(port) = ports.get_mut(port_idx) {
        let n = buf.len().min(port.rx_buf.len());
        buf[..n].copy_from_slice(&port.rx_buf[..n]);
        port.rx_buf.drain(..n);
        n
    } else {
        0
    }
}
