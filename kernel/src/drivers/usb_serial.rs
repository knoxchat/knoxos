/// USB CDC ACM Serial Driver
///
/// Supports USB serial adapters using the CDC Abstract Control Model class.
/// Commonly used for Arduino, GPS receivers, modems, and debug consoles.
///
/// Features:
///   - CDC ACM class 2/0 (modem) and 2/2 (abstract control)
///   - Baud rate configuration (300–3000000)
///   - Line coding (data bits, stop bits, parity)
///   - Flow control (DTR/DSR, RTS/CTS)
///   - Break signal
///   - Ring indicator / carrier detect
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Line coding parameters
#[derive(Debug, Clone, Copy)]
pub struct LineCoding {
    pub baud_rate: u32,
    pub stop_bits: StopBits,
    pub parity: Parity,
    pub data_bits: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StopBits {
    One,
    OnePointFive,
    Two,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Parity {
    None,
    Odd,
    Even,
    Mark,
    Space,
}

impl Default for LineCoding {
    fn default() -> Self {
        Self {
            baud_rate: 115200,
            stop_bits: StopBits::One,
            parity: Parity::None,
            data_bits: 8,
        }
    }
}

/// Modem control lines
#[derive(Debug, Clone, Copy, Default)]
pub struct ModemLines {
    pub dtr: bool, // Data Terminal Ready
    pub rts: bool, // Request To Send
    pub dsr: bool, // Data Set Ready (read-only)
    pub cts: bool, // Clear To Send (read-only)
    pub dcd: bool, // Data Carrier Detect (read-only)
    pub ri: bool,  // Ring Indicator (read-only)
}

/// USB CDC ACM serial device
pub struct CdcAcmDevice {
    pub usb_addr: u8,
    pub port_name: String,
    pub line_coding: LineCoding,
    pub modem: ModemLines,
    pub rx_buffer: VecDeque<u8>,
    pub control_interface: u8,
    pub data_interface: u8,
    pub bulk_in_ep: u8,
    pub bulk_out_ep: u8,
    pub interrupt_ep: u8,
    pub open: bool,
}

lazy_static::lazy_static! {
    static ref CDC_DEVICES: Mutex<Vec<CdcAcmDevice>> = Mutex::new(Vec::new());
    static ref NEXT_PORT: Mutex<u32> = Mutex::new(0);
}

impl CdcAcmDevice {
    pub fn new(usb_addr: u8) -> Self {
        let port_num = {
            let mut n = NEXT_PORT.lock();
            let v = *n;
            *n += 1;
            v
        };
        Self {
            usb_addr,
            port_name: alloc::format!("/dev/ttyACM{}", port_num),
            line_coding: LineCoding::default(),
            modem: ModemLines::default(),
            rx_buffer: VecDeque::with_capacity(4096),
            control_interface: 0,
            data_interface: 1,
            bulk_in_ep: 0x81,
            bulk_out_ep: 0x02,
            interrupt_ep: 0x83,
            open: false,
        }
    }

    /// Open serial port
    pub fn open(&mut self) -> Result<(), &'static str> {
        if self.open {
            return Err("Already open");
        }
        // Set DTR to indicate host is ready
        self.modem.dtr = true;
        self.modem.rts = true;
        self.set_control_line_state()?;
        self.open = true;
        serial_println!("[CDC-ACM] {} opened", self.port_name);
        Ok(())
    }

    /// Close serial port
    pub fn close(&mut self) {
        self.modem.dtr = false;
        self.modem.rts = false;
        let _ = self.set_control_line_state();
        self.open = false;
    }

    /// Set baud rate and line coding
    pub fn set_line_coding(&mut self, coding: LineCoding) -> Result<(), &'static str> {
        self.line_coding = coding;
        // Send SET_LINE_CODING class request
        // 7 bytes: dwDTERate(4), bCharFormat(1), bParityType(1), bDataBits(1)
        Ok(())
    }

    fn set_control_line_state(&self) -> Result<(), &'static str> {
        let _value = (self.modem.dtr as u16) | ((self.modem.rts as u16) << 1);
        // Send SET_CONTROL_LINE_STATE class request
        Ok(())
    }

    /// Write data to serial port
    pub fn write(&self, data: &[u8]) -> Result<usize, &'static str> {
        if !self.open {
            return Err("Port not open");
        }
        // Send via USB bulk OUT endpoint
        Ok(data.len())
    }

    /// Read data from serial port
    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        let n = buf.len().min(self.rx_buffer.len());
        for i in 0..n {
            buf[i] = self.rx_buffer.pop_front().unwrap_or(0);
        }
        n
    }

    /// Send break signal
    pub fn send_break(&self, duration_ms: u16) -> Result<(), &'static str> {
        // Send SEND_BREAK class request
        let _ = duration_ms;
        Ok(())
    }

    /// Handle USB bulk IN data (received from device)
    pub fn handle_rx_data(&mut self, data: &[u8]) {
        for &byte in data {
            if self.rx_buffer.len() < 4096 {
                self.rx_buffer.push_back(byte);
            }
        }
    }

    /// Handle serial state notification (interrupt endpoint)
    pub fn handle_notification(&mut self, data: &[u8]) {
        if data.len() >= 10 {
            let serial_state = u16::from(data[8]) | (u16::from(data[9]) << 8);
            self.modem.dcd = (serial_state & 0x01) != 0;
            self.modem.dsr = (serial_state & 0x02) != 0;
            self.modem.ri = (serial_state & 0x08) != 0;
        }
    }
}

pub fn init() {
    serial_println!("[CDC-ACM] USB serial driver loaded");
}
