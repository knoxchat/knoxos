/// IPv4 address
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ipv4Address(pub [u8; 4]);

impl Ipv4Address {
    pub const BROADCAST: Ipv4Address = Ipv4Address([255, 255, 255, 255]);
    pub const UNSPECIFIED: Ipv4Address = Ipv4Address([0, 0, 0, 0]);
    pub const LOOPBACK: Ipv4Address = Ipv4Address([127, 0, 0, 1]);

    pub fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        Ipv4Address([a, b, c, d])
    }

    pub fn to_u32(&self) -> u32 {
        u32::from_be_bytes(self.0)
    }

    pub fn from_u32(v: u32) -> Self {
        Ipv4Address(v.to_be_bytes())
    }

    pub fn is_broadcast(&self) -> bool {
        *self == Self::BROADCAST
    }
    pub fn is_loopback(&self) -> bool {
        self.0[0] == 127
    }
    pub fn is_multicast(&self) -> bool {
        self.0[0] >= 224 && self.0[0] <= 239
    }
}

impl core::fmt::Display for Ipv4Address {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}.{}.{}.{}", self.0[0], self.0[1], self.0[2], self.0[3])
    }
}

/// IP protocol numbers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IpProtocol {
    ICMP = 1,
    TCP = 6,
    UDP = 17,
}

/// IPv4 header (simplified, no options)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ipv4Header {
    pub version_ihl: u8,     // Version (4) | IHL (4)
    pub tos: u8,             // Type of Service
    pub total_length: u16,   // Big-endian
    pub identification: u16, // Big-endian
    pub flags_fragment: u16, // Big-endian: Flags (3) | Fragment Offset (13)
    pub ttl: u8,
    pub protocol: u8,
    pub checksum: u16, // Big-endian
    pub src_addr: [u8; 4],
    pub dst_addr: [u8; 4],
}

impl Ipv4Header {
    pub fn new(src: Ipv4Address, dst: Ipv4Address, protocol: IpProtocol, payload_len: u16) -> Self {
        let total_length = 20 + payload_len;
        let mut hdr = Self {
            version_ihl: 0x45, // IPv4, IHL=5 (20 bytes)
            tos: 0,
            total_length: total_length.to_be(),
            identification: 0,
            flags_fragment: 0x4000u16.to_be(), // Don't fragment
            ttl: 64,
            protocol: protocol as u8,
            checksum: 0,
            src_addr: src.0,
            dst_addr: dst.0,
        };
        hdr.checksum = hdr.compute_checksum().to_be();
        hdr
    }

    pub fn ihl(&self) -> u8 {
        self.version_ihl & 0x0F
    }
    pub fn header_len(&self) -> usize {
        (self.ihl() as usize) * 4
    }

    pub fn compute_checksum(&self) -> u16 {
        let ptr = self as *const Self as *const u16;
        let words = self.header_len() / 2;
        let mut sum: u32 = 0;
        for i in 0..words {
            if i == 5 {
                continue;
            } // Skip checksum field
            sum += unsafe { *ptr.add(i) } as u32;
        }
        while sum >> 16 != 0 {
            sum = (sum & 0xFFFF) + (sum >> 16);
        }
        !(sum as u16)
    }
}
