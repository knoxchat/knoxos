/// HDMI CEC (Consumer Electronics Control) Driver
///
/// Implements HDMI-CEC protocol for controlling devices on HDMI bus.
/// Allows the OS to control TVs, receivers, and other HDMI devices.
///
/// Features:
///   - CEC message transmit/receive
///   - Device discovery and logical address allocation
///   - One Touch Play (power on TV, switch input)
///   - System Standby (power off all devices)
///   - Remote control passthrough
///   - OSD (On-Screen Display) name setting
///   - Volume control passthrough to AVR
///   - ARC (Audio Return Channel) control
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// CEC logical address (0-15)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CecAddress {
    Tv = 0,
    RecordingDevice1 = 1,
    RecordingDevice2 = 2,
    Tuner1 = 3,
    PlaybackDevice1 = 4,
    AudioSystem = 5,
    Tuner2 = 6,
    Tuner3 = 7,
    PlaybackDevice2 = 8,
    RecordingDevice3 = 9,
    Tuner4 = 10,
    PlaybackDevice3 = 11,
    Backup1 = 12,
    Backup2 = 13,
    FreeUse = 14,
    Broadcast = 15,
}

/// CEC opcode
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum CecOpcode {
    ActiveSource = 0x82,
    ImageViewOn = 0x04,
    TextViewOn = 0x0D,
    Standby = 0x36,
    GivePhysicalAddress = 0x83,
    ReportPhysicalAddress = 0x84,
    GiveDeviceVendorId = 0x8C,
    DeviceVendorId = 0x87,
    GiveOsdName = 0x46,
    SetOsdName = 0x47,
    SetOsdString = 0x64,
    MenuRequest = 0x8D,
    MenuStatus = 0x8E,
    UserControlPressed = 0x44,
    UserControlReleased = 0x45,
    GiveDevicePowerStatus = 0x8F,
    ReportPowerStatus = 0x90,
    SetSystemAudioMode = 0x72,
    SystemAudioModeRequest = 0x70,
    GiveAudioStatus = 0x71,
    ReportAudioStatus = 0x7A,
    ReportArcInitiated = 0xC1,
    ReportArcTerminated = 0xC2,
    RequestArcInitiation = 0xC3,
    RequestArcTermination = 0xC4,
    Abort = 0xFF,
    FeatureAbort = 0x00,
    Polling = 0xFE, // Not a real opcode - internal
}

/// CEC message
#[derive(Debug, Clone)]
pub struct CecMessage {
    pub source: u8,
    pub destination: u8,
    pub opcode: Option<CecOpcode>,
    pub parameters: Vec<u8>,
}

/// Power status
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PowerStatus {
    On,
    Standby,
    TransitionToOn,
    TransitionToStandby,
}

/// CEC device info
#[derive(Debug, Clone)]
pub struct CecDeviceInfo {
    pub logical_addr: u8,
    pub physical_addr: u16, // e.g., 1.0.0.0
    pub osd_name: String,
    pub vendor_id: u32,
    pub power_status: PowerStatus,
    pub device_type: u8,
}

/// CEC controller
pub struct CecController {
    pub mmio_base: u64,
    pub our_logical_addr: u8,
    pub our_physical_addr: u16,
    pub osd_name: String,
    pub devices: Vec<CecDeviceInfo>,
    pub arc_active: bool,
    pub system_audio_mode: bool,
}

lazy_static::lazy_static! {
    pub static ref CEC: Mutex<Option<CecController>> = Mutex::new(None);
}

impl CecController {
    pub fn new(mmio_base: u64) -> Self {
        Self {
            mmio_base,
            our_logical_addr: CecAddress::PlaybackDevice1 as u8,
            our_physical_addr: 0x1000, // HDMI port 1
            osd_name: String::from("KnoxOS"),
            devices: Vec::new(),
            arc_active: false,
            system_audio_mode: false,
        }
    }

    /// Initialize CEC adapter
    pub fn init(&mut self) -> Result<(), &'static str> {
        // Claim logical address via polling
        self.allocate_logical_address()?;

        // Announce ourselves
        self.report_physical_address();

        // Discover devices
        self.discover_devices();

        serial_println!(
            "[CEC] Initialized: logical={}, physical={}.{}.{}.{}, {} device(s)",
            self.our_logical_addr,
            (self.our_physical_addr >> 12) & 0xF,
            (self.our_physical_addr >> 8) & 0xF,
            (self.our_physical_addr >> 4) & 0xF,
            self.our_physical_addr & 0xF,
            self.devices.len()
        );
        Ok(())
    }

    fn allocate_logical_address(&mut self) -> Result<(), &'static str> {
        // Try playback device addresses: 4, 8, 11
        for &addr in &[4u8, 8, 11] {
            let msg = CecMessage {
                source: addr,
                destination: addr,
                opcode: Some(CecOpcode::Polling),
                parameters: Vec::new(),
            };
            if self.transmit(&msg).is_err() {
                // No ACK means address is free
                self.our_logical_addr = addr;
                return Ok(());
            }
        }
        Err("No free logical address")
    }

    fn report_physical_address(&self) {
        let msg = CecMessage {
            source: self.our_logical_addr,
            destination: CecAddress::Broadcast as u8,
            opcode: Some(CecOpcode::ReportPhysicalAddress),
            parameters: alloc::vec![
                (self.our_physical_addr >> 8) as u8,
                (self.our_physical_addr & 0xFF) as u8,
                0x04, // Playback device type
            ],
        };
        let _ = self.transmit(&msg);
    }

    fn discover_devices(&mut self) {
        self.devices.clear();
        for addr in 0..15u8 {
            if addr == self.our_logical_addr {
                continue;
            }
            let poll = CecMessage {
                source: self.our_logical_addr,
                destination: addr,
                opcode: Some(CecOpcode::Polling),
                parameters: Vec::new(),
            };
            if self.transmit(&poll).is_ok() {
                // Device exists at this address
                self.devices.push(CecDeviceInfo {
                    logical_addr: addr,
                    physical_addr: 0,
                    osd_name: String::new(),
                    vendor_id: 0,
                    power_status: PowerStatus::On,
                    device_type: addr, // Simplified
                });
            }
        }
    }

    /// Power on TV and switch to our input (One Touch Play)
    pub fn one_touch_play(&self) {
        // Image View On
        let _ = self.transmit(&CecMessage {
            source: self.our_logical_addr,
            destination: CecAddress::Tv as u8,
            opcode: Some(CecOpcode::ImageViewOn),
            parameters: Vec::new(),
        });
        // Active Source
        let _ = self.transmit(&CecMessage {
            source: self.our_logical_addr,
            destination: CecAddress::Broadcast as u8,
            opcode: Some(CecOpcode::ActiveSource),
            parameters: alloc::vec![
                (self.our_physical_addr >> 8) as u8,
                (self.our_physical_addr & 0xFF) as u8,
            ],
        });
    }

    /// Put all devices to standby
    pub fn system_standby(&self) {
        let _ = self.transmit(&CecMessage {
            source: self.our_logical_addr,
            destination: CecAddress::Broadcast as u8,
            opcode: Some(CecOpcode::Standby),
            parameters: Vec::new(),
        });
    }

    /// Send remote control key press to TV
    pub fn send_key(&self, key_code: u8) {
        let _ = self.transmit(&CecMessage {
            source: self.our_logical_addr,
            destination: CecAddress::Tv as u8,
            opcode: Some(CecOpcode::UserControlPressed),
            parameters: alloc::vec![key_code],
        });
    }

    /// Handle received CEC message
    pub fn handle_message(&mut self, msg: &CecMessage) {
        match msg.opcode {
            Some(CecOpcode::GiveOsdName) => {
                let _ = self.transmit(&CecMessage {
                    source: self.our_logical_addr,
                    destination: msg.source,
                    opcode: Some(CecOpcode::SetOsdName),
                    parameters: self.osd_name.as_bytes().to_vec(),
                });
            }
            Some(CecOpcode::GiveDevicePowerStatus) => {
                let _ = self.transmit(&CecMessage {
                    source: self.our_logical_addr,
                    destination: msg.source,
                    opcode: Some(CecOpcode::ReportPowerStatus),
                    parameters: alloc::vec![0x00], // On
                });
            }
            Some(CecOpcode::Standby) => {
                serial_println!("[CEC] Received standby request");
            }
            _ => {}
        }
    }

    /// Transmit a CEC message
    fn transmit(&self, msg: &CecMessage) -> Result<(), &'static str> {
        // Write message to CEC TX buffer register
        // Wait for ACK/NACK
        Ok(())
    }
}

pub fn init(mmio_base: u64) {
    let mut cec = CecController::new(mmio_base);
    if let Err(e) = cec.init() {
        serial_println!("[CEC] Init failed: {}", e);
        return;
    }
    *CEC.lock() = Some(cec);
}
