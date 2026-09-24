//! USB protocol types, descriptors, and device representations.
use alloc::string::String;

// ─── USB Constants ─────────────────────────────────────────────────────

/// USB speeds
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum UsbSpeed {
    Full = 1,      // 12 Mbps
    Low = 2,       // 1.5 Mbps
    High = 3,      // 480 Mbps
    Super = 4,     // 5 Gbps
    SuperPlus = 5, // 10 Gbps
}

/// USB device class codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum UsbClass {
    InterfaceDescriptor = 0x00,
    Audio = 0x01,
    CDC = 0x02,
    HID = 0x03,
    Physical = 0x05,
    Image = 0x06,
    Printer = 0x07,
    MassStorage = 0x08,
    Hub = 0x09,
    CDCData = 0x0A,
    SmartCard = 0x0B,
    ContentSecurity = 0x0D,
    Video = 0x0E,
    PersonalHealthcare = 0x0F,
    AudioVideo = 0x10,
    Billboard = 0x11,
    Wireless = 0xE0,
    Miscellaneous = 0xEF,
    ApplicationSpecific = 0xFE,
    VendorSpecific = 0xFF,
}

/// USB request types
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum UsbRequestType {
    GetStatus = 0,
    ClearFeature = 1,
    SetFeature = 3,
    SetAddress = 5,
    GetDescriptor = 6,
    SetDescriptor = 7,
    GetConfiguration = 8,
    SetConfiguration = 9,
}

/// USB descriptor types
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum UsbDescriptorType {
    Device = 1,
    Configuration = 2,
    String = 3,
    Interface = 4,
    Endpoint = 5,
    DeviceQualifier = 6,
    OtherSpeedConfig = 7,
    InterfacePower = 8,
    OTG = 9,
    Debug = 10,
    InterfaceAssociation = 11,
    HID = 0x21,
    HIDReport = 0x22,
    HIDPhysical = 0x23,
}

// ─── USB Device Descriptor ─────────────────────────────────────────────

/// USB device descriptor (18 bytes)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct UsbDeviceDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub bcd_usb: u16,
    pub b_device_class: u8,
    pub b_device_sub_class: u8,
    pub b_device_protocol: u8,
    pub b_max_packet_size0: u8,
    pub id_vendor: u16,
    pub id_product: u16,
    pub bcd_device: u16,
    pub i_manufacturer: u8,
    pub i_product: u8,
    pub i_serial_number: u8,
    pub b_num_configurations: u8,
}

/// USB configuration descriptor
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct UsbConfigDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub w_total_length: u16,
    pub b_num_interfaces: u8,
    pub b_configuration_value: u8,
    pub i_configuration: u8,
    pub bm_attributes: u8,
    pub b_max_power: u8,
}

/// USB endpoint descriptor
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct UsbEndpointDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub b_endpoint_address: u8,
    pub bm_attributes: u8,
    pub w_max_packet_size: u16,
    pub b_interval: u8,
}

/// USB Mass Storage specific device
#[derive(Debug, Clone)]
pub struct UsbMassStorageDevice {
    pub device_id: u32,
    pub usb_dev: UsbDevice,
    pub lun: u8,           // Logical Unit Number
    pub sector_size: u32,  // Bytes per sector
    pub sector_count: u64, // Total number of sectors
    pub interface_num: u8,
    pub ep_bulk_in: u8,  // Bulk IN endpoint
    pub ep_bulk_out: u8, // Bulk OUT endpoint
    pub mounted: bool,
    pub read_only: bool,
}

/// CBW (Command Block Wrapper) for SCSI commands
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CommandBlockWrapper {
    pub signature: u32,            // 0x43425355 = "USBC"
    pub tag: u32,                  // Command tag
    pub data_transfer_length: u32, // Transfer length
    pub flags: u8,                 // 0x00=out, 0x80=in
    pub lun: u8,                   // LUN
    pub command_length: u8,        // SCSI command length (6-16)
    pub command: [u8; 16],         // SCSI command
}

/// CSW (Command Status Wrapper) for command completion
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CommandStatusWrapper {
    pub signature: u32, // 0x53425355 = "USBS"
    pub tag: u32,       // Command tag
    pub residue: u32,   // Residue bytes
    pub status: u8,     // 0=success, 1=failure, 2=phase error
}

/// USB device representation
#[derive(Debug, Clone)]
pub struct UsbDevice {
    pub slot_id: u8,
    pub port: u8,
    pub speed: UsbSpeed,
    pub address: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_class: u8,
    pub device_subclass: u8,
    pub device_protocol: u8,
    pub manufacturer: String,
    pub product: String,
    pub serial: String,
    pub configured: bool,
}

/// USB Setup Packet (8 bytes)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct UsbSetupPacket {
    pub bm_request_type: u8,
    pub b_request: u8,
    pub w_value: u16,
    pub w_index: u16,
    pub w_length: u16,
}

impl UsbSetupPacket {
    pub fn get_descriptor(desc_type: u8, desc_index: u8, length: u16) -> Self {
        Self {
            bm_request_type: 0x80, // Device-to-host, standard, device
            b_request: UsbRequestType::GetDescriptor as u8,
            w_value: ((desc_type as u16) << 8) | desc_index as u16,
            w_index: 0,
            w_length: length,
        }
    }

    pub fn set_configuration(config: u8) -> Self {
        Self {
            bm_request_type: 0x00, // Host-to-device, standard, device
            b_request: UsbRequestType::SetConfiguration as u8,
            w_value: config as u16,
            w_index: 0,
            w_length: 0,
        }
    }

    pub fn set_address(addr: u8) -> Self {
        Self {
            bm_request_type: 0x00,
            b_request: UsbRequestType::SetAddress as u8,
            w_value: addr as u16,
            w_index: 0,
            w_length: 0,
        }
    }
}
