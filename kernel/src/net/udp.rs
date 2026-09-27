/// UDP header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct UdpHeader {
    pub src_port: u16, // Big-endian
    pub dst_port: u16, // Big-endian
    pub length: u16,   // Big-endian
    pub checksum: u16, // Big-endian
}

impl UdpHeader {
    pub fn new(src_port: u16, dst_port: u16, payload_len: u16) -> Self {
        let length = 8 + payload_len;
        Self {
            src_port: src_port.to_be(),
            dst_port: dst_port.to_be(),
            length: length.to_be(),
            checksum: 0, // Optional in IPv4
        }
    }
}
