//! Host-side parsers used by KnoxOS. These tests run on the build machine
//! without QEMU so ELF / IPv6 / cron / ext4 / packet parsing can fail in CI.

mod curve25519;
mod sha512;

pub fn elf_magic_ok(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0] == 0x7F && bytes[1] == b'E' && bytes[2] == b'L' && bytes[3] == b'F'
}

const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EM_X86_64: u16 = 62;
const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;
const ELF64_EHDR: usize = 64;
const ELF64_PHDR: usize = 56;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfValidateError {
    TooSmall,
    InvalidMagic,
    InvalidClass,
    InvalidEndian,
    InvalidMachine,
    InvalidType,
    TruncatedPhdr,
}

/// Host copy of `kernel/src/elf.rs::validate_elf` + program-header bounds.
pub fn validate_elf(data: &[u8]) -> Result<(), ElfValidateError> {
    if data.len() < ELF64_EHDR {
        return Err(ElfValidateError::TooSmall);
    }
    if !elf_magic_ok(data) {
        return Err(ElfValidateError::InvalidMagic);
    }
    if data[4] != ELFCLASS64 {
        return Err(ElfValidateError::InvalidClass);
    }
    if data[5] != ELFDATA2LSB {
        return Err(ElfValidateError::InvalidEndian);
    }
    let machine = u16::from_le_bytes([data[18], data[19]]);
    if machine != EM_X86_64 {
        return Err(ElfValidateError::InvalidMachine);
    }
    let ty = u16::from_le_bytes([data[16], data[17]]);
    if ty != ET_EXEC && ty != ET_DYN {
        return Err(ElfValidateError::InvalidType);
    }
    let phoff = u64::from_le_bytes(data[32..40].try_into().unwrap()) as usize;
    let phentsize = u16::from_le_bytes([data[54], data[55]]) as usize;
    let phnum = u16::from_le_bytes([data[56], data[57]]) as usize;
    if phentsize == 0 && phnum > 0 {
        return Err(ElfValidateError::TruncatedPhdr);
    }
    for i in 0..phnum {
        let offset = phoff.saturating_add(i.saturating_mul(phentsize));
        let need = if phentsize == 0 {
            ELF64_PHDR
        } else {
            phentsize
        };
        if offset.saturating_add(need) > data.len() {
            return Err(ElfValidateError::TruncatedPhdr);
        }
    }
    Ok(())
}

/// ICMPv6 checksum over the IPv6 pseudo-header (next header 58).
pub fn icmpv6_checksum(src: &[u8; 16], dst: &[u8; 16], icmp_data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    for i in (0..16).step_by(2) {
        sum += ((src[i] as u32) << 8) | src[i + 1] as u32;
    }
    for i in (0..16).step_by(2) {
        sum += ((dst[i] as u32) << 8) | dst[i + 1] as u32;
    }
    let len = icmp_data.len() as u32;
    sum += (len >> 16) & 0xFFFF;
    sum += len & 0xFFFF;
    sum += 58;
    let mut i = 0;
    while i + 1 < icmp_data.len() {
        sum += ((icmp_data[i] as u32) << 8) | icmp_data[i + 1] as u32;
        i += 2;
    }
    if i < icmp_data.len() {
        sum += (icmp_data[i] as u32) << 8;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CronField {
    Any,
    Value(u8),
    Range(u8, u8),
    List(Vec<u8>),
    Step(u8),
}

impl CronField {
    pub fn matches(&self, val: u8) -> bool {
        match self {
            CronField::Any => true,
            CronField::Value(v) => *v == val,
            CronField::Range(lo, hi) => val >= *lo && val <= *hi,
            CronField::List(vals) => vals.contains(&val),
            CronField::Step(step) => *step > 0 && val % *step == 0,
        }
    }

    pub fn parse(s: &str) -> Self {
        if s == "*" {
            return CronField::Any;
        }
        if let Some(rest) = s.strip_prefix("*/") {
            if let Ok(n) = rest.parse::<u8>() {
                return CronField::Step(n);
            }
        }
        if let Some(dash_pos) = s.find('-') {
            if let (Ok(lo), Ok(hi)) = (s[..dash_pos].parse::<u8>(), s[dash_pos + 1..].parse::<u8>())
            {
                return CronField::Range(lo, hi);
            }
        }
        if s.contains(',') {
            let vals: Vec<u8> = s.split(',').filter_map(|v| v.parse::<u8>().ok()).collect();
            if !vals.is_empty() {
                return CronField::List(vals);
            }
        }
        if let Ok(v) = s.parse::<u8>() {
            CronField::Value(v)
        } else {
            CronField::Any
        }
    }
}

/// NTFS boot OEM at offset 3 must be `NTFS`.
pub fn ntfs_oem_ok(sector: &[u8]) -> bool {
    sector.len() >= 11 && &sector[3..7] == b"NTFS"
}

/// MFT FILE record magic.
pub fn ntfs_mft_magic_ok(record: &[u8]) -> bool {
    record.len() >= 4 && record[0..4] == [b'F', b'I', b'L', b'E']
}

fn parse_aml_s5_package_at(aml: &[u8], mut p: usize) -> Option<u8> {
    if aml.get(p).copied() != Some(0x12) {
        return None;
    }
    p += 1;
    let pkg0 = *aml.get(p)?;
    let extra = (pkg0 >> 6) as usize;
    p += 1 + extra;
    let _n = *aml.get(p)?;
    p += 1;
    Some(match *aml.get(p)? {
        0x0A => *aml.get(p + 1).unwrap_or(&0),
        0x00 => 0,
        0x01 => 1,
        b => b,
    })
}

/// Scan AML for `Name(_S5, Package(){...})` and return SLP_TYPa.
pub fn parse_aml_s5_slp_typ(aml: &[u8]) -> Option<u8> {
    const NEEDLES: [&[u8]; 3] = [
        &[0x08, b'_', b'S', b'5', b'_'],
        &[0x08, 0x5C, b'_', b'S', b'5', b'_'],
        &[0x08, b'_', b'S', b'5', 0],
    ];
    for needle in NEEDLES {
        let mut i = 0;
        while i + needle.len() + 3 < aml.len() {
            if &aml[i..i + needle.len()] == needle {
                if let Some(typ) = parse_aml_s5_package_at(aml, i + needle.len()) {
                    return Some(typ);
                }
            }
            i += 1;
        }
    }
    None
}

/// ext4 superblock magic is at byte 0x38 of the superblock (LBA 2).
pub const EXT4_SUPER_MAGIC: u16 = 0xEF53;

pub fn ext4_magic_ok(superblock: &[u8]) -> bool {
    if superblock.len() < 0x3A {
        return false;
    }
    u16::from_le_bytes([superblock[0x38], superblock[0x39]]) == EXT4_SUPER_MAGIC
}

/// exFAT VBR signature is `"EXFAT   "` at offset 3.
pub fn exfat_magic_ok(boot: &[u8]) -> bool {
    boot.len() >= 11 && &boot[3..11] == b"EXFAT   "
}

/// XFS superblock magic is big-endian `"XFSB"`.
pub fn xfs_magic_ok(sb: &[u8]) -> bool {
    sb.len() >= 4 && sb[0..4] == [0x58, 0x46, 0x53, 0x42]
}

/// Btrfs superblock magic `"_BHRfS_M"` at offset 0x40.
pub fn btrfs_magic_ok(sb: &[u8]) -> bool {
    sb.len() >= 0x48 && &sb[0x40..0x48] == b"_BHRfS_M"
}

/// 802.1Q TPID is 0x8100 after the two MAC addresses.
pub fn vlan_tpid(frame: &[u8]) -> Option<u16> {
    if frame.len() < 16 {
        return None;
    }
    Some(u16::from_be_bytes([frame[12], frame[13]]))
}

/// 802.11 Beacon SSID IE (offset 36).
pub fn parse_beacon_ssid(frame: &[u8]) -> Option<&[u8]> {
    if frame.len() < 38 {
        return None;
    }
    let fc = u16::from_le_bytes([frame[0], frame[1]]);
    if (fc >> 2) & 0x03 != 0 || (fc >> 4) & 0x0F != 8 {
        return None;
    }
    let mut off = 36usize;
    while off + 2 <= frame.len() {
        let id = frame[off];
        let len = frame[off + 1] as usize;
        if off + 2 + len > frame.len() {
            return None;
        }
        if id == 0 {
            return Some(&frame[off + 2..off + 2 + len]);
        }
        off += 2 + len;
    }
    None
}

/// GNU .mo little-endian magic.
pub fn mo_magic_ok(data: &[u8]) -> bool {
    data.len() >= 4 && data[0..4] == [0xDE, 0x12, 0x04, 0x95]
}

/// USB 2.0 hub descriptor type 0x29; returns port count.
pub fn parse_hub_num_ports(data: &[u8]) -> Option<u8> {
    if data.len() < 7 || data[1] != 0x29 {
        return None;
    }
    Some(data[2])
}

/// USB 2.0 GetPortStatus: connected bit of wPortStatus.
pub fn hub_port_connected(data: &[u8]) -> Option<bool> {
    if data.len() < 4 {
        return None;
    }
    Some(u16::from_le_bytes([data[0], data[1]]) & 1 != 0)
}

/// Generic HID gamepad report: buttons in byte 0, left X in byte 2 (0x80 center).
pub fn hid_gamepad_a_pressed(report: &[u8]) -> Option<bool> {
    if report.len() < 8 {
        return None;
    }
    Some(report[0] & 1 != 0)
}

/// VirtIO input event: `le16 type, le16 code, le32 value`.
pub fn parse_virtio_input_event(b: &[u8]) -> Option<(u16, u16, u32)> {
    if b.len() < 8 {
        return None;
    }
    let typ = u16::from_le_bytes([b[0], b[1]]);
    let code = u16::from_le_bytes([b[2], b[3]]);
    let value = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    if typ == 0 && code == 0 && value == 0 {
        return None;
    }
    Some((typ, code, value))
}

/// HID digitizer / usb-tablet: `[buttons, x_lo, x_hi, y_lo, y_hi]`.
pub fn parse_hid_digitizer_xy(report: &[u8]) -> Option<(u16, u16, bool)> {
    if report.len() < 5 {
        return None;
    }
    Some((
        u16::from_le_bytes([report[1], report[2]]),
        u16::from_le_bytes([report[3], report[4]]),
        report[0] & 1 != 0,
    ))
}

/// HID multitouch contact count is byte 0; each contact is 6 bytes.
pub fn parse_mt_contact_count(report: &[u8]) -> Option<u8> {
    if report.is_empty() {
        return None;
    }
    let count = report[0];
    let need = 1usize.saturating_add((count as usize).saturating_mul(6));
    if count == 0 || count > 5 || report.len() < need {
        return None;
    }
    Some(count)
}

/// Wacom Intuos 10-byte HID: pressure at bytes 6..8.
pub fn parse_wacom_pressure(report: &[u8]) -> Option<u16> {
    if report.len() < 10 {
        return None;
    }
    Some(u16::from_le_bytes([report[6], report[7]]))
}

/// UVC payload HLE + BFH; bit 1 of BFH is EOF.
pub fn parse_uvc_eof(packet: &[u8]) -> Option<bool> {
    if packet.is_empty() {
        return None;
    }
    let hle = packet[0] as usize;
    if hle < 2 || hle > packet.len() {
        return None;
    }
    Some(packet[1] & 0x02 != 0)
}

/// UAC 1.0 SET_CUR sampling frequency is 3 bytes little-endian.
pub fn parse_uac_set_cur_freq(data: &[u8]) -> Option<u32> {
    if data.len() < 3 {
        return None;
    }
    Some(u32::from(data[0]) | (u32::from(data[1]) << 8) | (u32::from(data[2]) << 16))
}

/// CDC ECM bulk payload is a raw Ethernet frame; returns EtherType.
pub fn parse_cdc_ecm_ethertype(frame: &[u8]) -> Option<u16> {
    if frame.len() < 14 {
        return None;
    }
    Some(u16::from_be_bytes([frame[12], frame[13]]))
}

/// SMB2 header: `\xFESMB` + StructureSize 64 + Command at offset 12.
pub fn parse_smb2_command(pkt: &[u8]) -> Option<u16> {
    if pkt.len() < 64 {
        return None;
    }
    if pkt[0..4] != [0xFE, 0x53, 0x4D, 0x42] {
        return None;
    }
    if u16::from_le_bytes([pkt[4], pkt[5]]) != 64 {
        return None;
    }
    Some(u16::from_le_bytes([pkt[12], pkt[13]]))
}

/// RTL8139 RX status dword: bit0 ROK, bits 16..31 length including CRC.
pub fn parse_rtl8139_rx_header(header: u32) -> Option<usize> {
    let rok = (header & 0x01) != 0;
    let len = ((header >> 16) & 0xFFFF) as usize;
    if !rok || len < 4 || len > 1600 {
        return None;
    }
    Some(len)
}

/// FUSE entry payload: ino u64, size u64, mode u32.
pub fn parse_fuse_entry(data: &[u8]) -> Option<(u64, u64, u32)> {
    if data.len() < 20 {
        return None;
    }
    let ino = u64::from_le_bytes(data[0..8].try_into().ok()?);
    let size = u64::from_le_bytes(data[8..16].try_into().ok()?);
    let mode = u32::from_le_bytes(data[16..20].try_into().ok()?);
    Some((ino, size, mode))
}

/// AMDGPU firmware: `AMDGPUFW` + count + (ip, size, payload)*.
pub fn parse_amdgpu_firmware_ips(blob: &[u8]) -> Option<u32> {
    if blob.len() < 12 || blob[0..8] != *b"AMDGPUFW" {
        return None;
    }
    let count = u32::from_le_bytes(blob[8..12].try_into().ok()?) as usize;
    if count == 0 || count > 16 {
        return None;
    }
    let mut off: usize = 12;
    let mut seen = 0u32;
    for _ in 0..count {
        if off.saturating_add(8) > blob.len() {
            return None;
        }
        let ip = u32::from_le_bytes(blob[off..off + 4].try_into().ok()?);
        let size = u32::from_le_bytes(blob[off + 4..off + 8].try_into().ok()?) as usize;
        off += 8;
        if off.saturating_add(size) > blob.len() || ip == 0 || ip > 31 {
            return None;
        }
        seen |= 1 << ip;
        off += size;
    }
    Some(seen)
}

/// iwlwifi firmware: `IWLWIFI\0` + version + TLVs.
pub fn parse_iwl_firmware_sections(blob: &[u8]) -> Option<u32> {
    if blob.len() < 12 || blob[0..8] != *b"IWLWIFI\0" {
        return None;
    }
    let mut off: usize = 12;
    let mut sections = 0u32;
    while off.saturating_add(8) <= blob.len() {
        let ty = u32::from_le_bytes(blob[off..off + 4].try_into().ok()?);
        let len = u32::from_le_bytes(blob[off + 4..off + 8].try_into().ok()?) as usize;
        off += 8;
        if off.saturating_add(len) > blob.len() || ty == 0 || ty > 16 {
            return None;
        }
        sections += 1;
        off += len;
    }
    if sections == 0 {
        None
    } else {
        Some(sections)
    }
}

/// ZFS uberblock magic `0x00BAB10C` (big-endian).
pub fn parse_zfs_uberblock_magic(ub: &[u8]) -> Option<u32> {
    if ub.len() < 4 {
        return None;
    }
    let magic = u32::from_be_bytes(ub[0..4].try_into().ok()?);
    if magic != 0x00BA_B10C {
        return None;
    }
    Some(magic)
}

/// NFSv4 XDR opcode (LOOKUP is 15).
pub fn parse_nfs4_op(xdr: &[u8]) -> Option<u32> {
    if xdr.len() < 4 {
        return None;
    }
    let op = u32::from_be_bytes(xdr[0..4].try_into().ok()?);
    if op == 0 || op > 75 {
        return None;
    }
    Some(op)
}

/// TLS record + handshake type. Byte 0 is content type 22, byte 5 is HS type.
pub fn parse_tls_handshake_type(rec: &[u8]) -> Option<u8> {
    if rec.len() < 6 || rec[0] != 22 {
        return None;
    }
    Some(rec[5])
}

/// SAE commit group id (little-endian u16); group 19 is NIST P-256.
pub fn parse_sae_commit_group(msg: &[u8]) -> Option<u16> {
    if msg.len() < 2 {
        return None;
    }
    let g = u16::from_le_bytes([msg[0], msg[1]]);
    if g != 19 {
        return None;
    }
    Some(g)
}

/// EPT PTE R/W/X bits. Present requires the read bit.
pub fn parse_ept_pte(pte: u64) -> Option<(bool, bool, bool)> {
    let r = pte & 1 != 0;
    let w = pte & 2 != 0;
    let x = pte & 4 != 0;
    if !r {
        return None;
    }
    Some((r, w, x))
}

/// virtio-mmio magic `0x74726976` ("virt").
pub fn parse_virtio_mmio_magic(b: &[u8]) -> Option<u32> {
    if b.len() < 4 {
        return None;
    }
    let m = u32::from_le_bytes(b[0..4].try_into().ok()?);
    if m != 0x7472_6976 {
        return None;
    }
    Some(m)
}

/// Broadcom firmware: `BRCMFW` + version + TLVs.
pub fn parse_brcm_firmware_sections(blob: &[u8]) -> Option<u32> {
    if blob.len() < 10 || blob[0..6] != *b"BRCMFW" {
        return None;
    }
    let mut off: usize = 10;
    let mut sections = 0u32;
    while off.saturating_add(8) <= blob.len() {
        let ty = u32::from_le_bytes(blob[off..off + 4].try_into().ok()?);
        let len = u32::from_le_bytes(blob[off + 4..off + 8].try_into().ok()?) as usize;
        off += 8;
        if off.saturating_add(len) > blob.len() || ty == 0 || ty > 16 {
            return None;
        }
        sections += 1;
        off += len;
    }
    if sections == 0 {
        None
    } else {
        Some(sections)
    }
}

/// Thunderbolt route string (nonzero u64 LE).
pub fn parse_tb_route(bytes: &[u8]) -> Option<u64> {
    if bytes.len() < 8 {
        return None;
    }
    let r = u64::from_le_bytes(bytes[0..8].try_into().ok()?);
    if r == 0 {
        return None;
    }
    Some(r)
}

/// TrueType 26.6 round-to-grid.
pub fn round_26_6(v: i32) -> i32 {
    if v >= 0 {
        (v + 32) & !63
    } else {
        -(((-v) + 32) & !63)
    }
}

/// TIS ACCESS after REQUEST_USE: VALID | ACTIVE_LOCALITY.
pub fn tis_grant_locality(request_use: u8) -> u8 {
    if request_use & 0x02 != 0 {
        0xA0
    } else {
        0x81
    }
}

/// SCSI READ(10) opcode 0x28, LBA at bytes 2..6, count at 7..9.
pub fn scsi_read10_lba(cdb: &[u8]) -> Option<(u32, u16)> {
    if cdb.len() < 10 || cdb[0] != 0x28 {
        return None;
    }
    let lba = u32::from_be_bytes([cdb[2], cdb[3], cdb[4], cdb[5]]);
    let count = u16::from_be_bytes([cdb[7], cdb[8]]);
    Some((lba, count))
}

/// RFC 1071 IPv4 header checksum over a zeroed checksum field.
pub fn ipv4_header_checksum(hdr: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < hdr.len() {
        sum += u16::from_be_bytes([hdr[i], hdr[i + 1]]) as u32;
        i += 2;
    }
    if i < hdr.len() {
        sum += (hdr[i] as u32) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

/// Reject truncated or internally inconsistent ext4 superblocks.
pub fn parse_ext4_superblock(superblock: &[u8]) -> Result<u32, &'static str> {
    if superblock.len() < 0x5C {
        return Err("truncated superblock");
    }
    if !ext4_magic_ok(superblock) {
        return Err("bad magic");
    }
    let blocks_per_group = u32::from_le_bytes(superblock[0x20..0x24].try_into().unwrap());
    if blocks_per_group == 0 {
        return Err("blocks_per_group is 0");
    }
    let log_block_size = u32::from_le_bytes(superblock[0x18..0x1C].try_into().unwrap());
    if log_block_size > 6 {
        return Err("s_log_block_size too large");
    }
    Ok(1024u32 << log_block_size)
}

pub fn quota_kib(bytes: usize) -> u64 {
    (bytes as u64).div_ceil(1024).max(1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketKind {
    Arp,
    Ipv4,
    Ipv6,
}

/// Host copy of the Ethernet / IPv4 / IPv6 length checks in `net/packet.rs`.
pub fn parse_ethernet(frame: &[u8]) -> Result<PacketKind, &'static str> {
    if frame.len() < 14 {
        return Err("truncated ethernet");
    }
    let ether_type = u16::from_be_bytes([frame[12], frame[13]]);
    match ether_type {
        0x0806 => Ok(PacketKind::Arp),
        0x0800 => {
            parse_ipv4(&frame[14..])?;
            Ok(PacketKind::Ipv4)
        }
        0x86DD => {
            parse_ipv6(&frame[14..])?;
            Ok(PacketKind::Ipv6)
        }
        _ => Err("unknown ethertype"),
    }
}

pub fn parse_ipv4(data: &[u8]) -> Result<u8, &'static str> {
    if data.len() < 20 {
        return Err("truncated ipv4");
    }
    let version_ihl = data[0];
    if version_ihl >> 4 != 4 {
        return Err("not ipv4");
    }
    let ihl = ((version_ihl & 0x0F) as usize) * 4;
    if ihl < 20 || ihl > data.len() {
        return Err("bad ihl");
    }
    Ok(data[9])
}

pub fn parse_ipv6(data: &[u8]) -> Result<u8, &'static str> {
    if data.len() < 40 {
        return Err("truncated ipv6");
    }
    if data[0] >> 4 != 6 {
        return Err("not ipv6");
    }
    Ok(data[6])
}

/// Deterministic bit/byte flips over a seed. The parser must not panic.
pub fn fuzz_mutate(seed: &[u8], round: usize) -> Vec<u8> {
    let mut out = seed.to_vec();
    if out.is_empty() {
        return out;
    }
    let i = round % out.len();
    match round % 5 {
        0 => out[i] ^= 0xFF,
        1 => out[i] = out[i].wrapping_add(1),
        2 => out.truncate(i.max(1)),
        3 => out.extend_from_slice(&[0x00, 0xFF, round as u8]),
        _ => out[i] = 0,
    }
    out
}

#[cfg(test)]
fn minimal_elf() -> Vec<u8> {
    let mut e = vec![0u8; ELF64_EHDR];
    e[0..4].copy_from_slice(&[0x7F, b'E', b'L', b'F']);
    e[4] = ELFCLASS64;
    e[5] = ELFDATA2LSB;
    e[6] = 1;
    e[16..18].copy_from_slice(&ET_DYN.to_le_bytes());
    e[18..20].copy_from_slice(&EM_X86_64.to_le_bytes());
    e[52..54].copy_from_slice(&(ELF64_EHDR as u16).to_le_bytes());
    e[54..56].copy_from_slice(&(ELF64_PHDR as u16).to_le_bytes());
    e
}

#[cfg(test)]
fn minimal_ext4_sb() -> Vec<u8> {
    let mut sb = vec![0u8; 1024];
    sb[0x18..0x1C].copy_from_slice(&0u32.to_le_bytes());
    sb[0x20..0x24].copy_from_slice(&8192u32.to_le_bytes());
    sb[0x38] = 0x53;
    sb[0x39] = 0xEF;
    sb
}

#[cfg(test)]
fn minimal_ipv4_frame() -> Vec<u8> {
    let mut f = vec![0u8; 14 + 20];
    f[12..14].copy_from_slice(&0x0800u16.to_be_bytes());
    f[14] = 0x45;
    f[14 + 9] = 6;
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elf_magic_detects_header() {
        assert!(elf_magic_ok(&[0x7F, b'E', b'L', b'F', 2, 1]));
        assert!(!elf_magic_ok(&[0x7F, b'E', b'L']));
        assert!(!elf_magic_ok(b"MZ\0\0"));
    }

    #[test]
    fn validate_elf_rejects_malformed() {
        assert_eq!(validate_elf(&[]), Err(ElfValidateError::TooSmall));
        assert_eq!(
            validate_elf(&[0x7F, b'E', b'L', b'F']),
            Err(ElfValidateError::TooSmall)
        );
        let mut bad = minimal_elf();
        bad[0] = 0;
        assert_eq!(validate_elf(&bad), Err(ElfValidateError::InvalidMagic));
        let mut bad = minimal_elf();
        bad[4] = 1;
        assert_eq!(validate_elf(&bad), Err(ElfValidateError::InvalidClass));
        let mut bad = minimal_elf();
        bad[18] = 0;
        bad[19] = 0;
        assert_eq!(validate_elf(&bad), Err(ElfValidateError::InvalidMachine));
        let mut huge = minimal_elf();
        huge[32..40].copy_from_slice(&0xFFFF_FFF0u64.to_le_bytes());
        huge[56..58].copy_from_slice(&16u16.to_le_bytes());
        assert_eq!(validate_elf(&huge), Err(ElfValidateError::TruncatedPhdr));
        assert!(validate_elf(&minimal_elf()).is_ok());
    }

    #[test]
    fn icmpv6_echo_checksum_nonzero() {
        let src = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        let mut icmp = vec![128u8, 0, 0, 0, 0, 0, 0, 1, b'K', b'N', b'O', b'X'];
        let c = icmpv6_checksum(&src, &src, &icmp);
        icmp[2] = (c >> 8) as u8;
        icmp[3] = (c & 0xFF) as u8;
        assert_eq!(icmpv6_checksum(&src, &src, &icmp), 0);
    }

    #[test]
    fn cron_fields_match() {
        assert!(CronField::parse("*").matches(7));
        assert!(CronField::parse("*/5").matches(10));
        assert!(!CronField::parse("*/5").matches(11));
        assert!(CronField::parse("1-3").matches(2));
        assert!(CronField::parse("1,15,30").matches(15));
        assert_eq!(CronField::parse("42"), CronField::Value(42));
    }

    #[test]
    fn ext4_super_magic() {
        let mut sb = [0u8; 64];
        sb[0x38] = 0x53;
        sb[0x39] = 0xEF;
        assert!(ext4_magic_ok(&sb));
        sb[0x38] = 0;
        assert!(!ext4_magic_ok(&sb));
        assert!(parse_ext4_superblock(&minimal_ext4_sb()).is_ok());
        assert!(parse_ext4_superblock(&[0u8; 8]).is_err());
        let mut zpg = minimal_ext4_sb();
        zpg[0x20..0x24].copy_from_slice(&0u32.to_le_bytes());
        assert!(parse_ext4_superblock(&zpg).is_err());
    }

    #[test]
    fn ntfs_oem_and_mft_magic() {
        let mut boot = vec![0u8; 512];
        boot[3..11].copy_from_slice(b"NTFS    ");
        assert!(ntfs_oem_ok(&boot));
        boot[3] = b'X';
        assert!(!ntfs_oem_ok(&boot));
        assert!(ntfs_mft_magic_ok(b"FILE...."));
        assert!(!ntfs_mft_magic_ok(b"BAAD"));
    }

    #[test]
    fn poly1305_rfc8439() {
        fn mul_u128(a: u128, b: u128) -> (u128, u128) {
            let a0 = a as u64 as u128;
            let a1 = a >> 64;
            let b0 = b as u64 as u128;
            let b1 = b >> 64;
            let p00 = a0 * b0;
            let p01 = a0 * b1;
            let p10 = a1 * b0;
            let p11 = a1 * b1;
            let mut lo = p00;
            let (nlo, c1) = lo.overflowing_add(p01 << 64);
            lo = nlo;
            let mut hi = (p01 >> 64) + u128::from(c1);
            let (nlo, c2) = lo.overflowing_add(p10 << 64);
            lo = nlo;
            hi += (p10 >> 64) + u128::from(c2);
            hi += p11;
            (lo, hi)
        }
        fn poly1305_reduce(mut lo: u128, mut hi: u128) -> (u128, u128) {
            loop {
                let top = hi >> 2;
                if top == 0 {
                    break;
                }
                hi &= 3;
                let (nlo, c) = lo.overflowing_add(top.wrapping_mul(5));
                lo = nlo;
                hi += u128::from(c);
            }
            (lo, hi)
        }
        fn poly1305(key: &[u8; 32], msg: &[u8]) -> [u8; 16] {
            let mut rbytes = [0u8; 16];
            rbytes.copy_from_slice(&key[0..16]);
            rbytes[3] &= 15;
            rbytes[7] &= 15;
            rbytes[11] &= 15;
            rbytes[15] &= 15;
            rbytes[4] &= 252;
            rbytes[8] &= 252;
            rbytes[12] &= 252;
            let r = u128::from_le_bytes(rbytes);
            let mut sbytes = [0u8; 16];
            sbytes.copy_from_slice(&key[16..32]);
            let s = u128::from_le_bytes(sbytes);
            let mut h_lo = 0u128;
            let mut h_hi = 0u128;
            let mut offset = 0;
            while offset < msg.len() {
                let n = core::cmp::min(16, msg.len() - offset);
                let mut block = [0u8; 16];
                block[..n].copy_from_slice(&msg[offset..offset + n]);
                let n_lo = u128::from_le_bytes(block);
                let (sum, c) = h_lo.overflowing_add(n_lo);
                h_lo = sum;
                h_hi += u128::from(c);
                if n == 16 {
                    h_hi += 1;
                } else {
                    let pad = 1u128 << (8 * n);
                    let (sum2, c2) = h_lo.overflowing_add(pad);
                    h_lo = sum2;
                    h_hi += u128::from(c2);
                }
                let (p0, p1) = mul_u128(h_lo, r);
                let p_hi = p1.wrapping_add(h_hi.wrapping_mul(r));
                let (nlo, nhi) = poly1305_reduce(p0, p_hi);
                h_lo = nlo;
                h_hi = nhi;
                offset += 16;
            }
            if h_hi > 3 || (h_hi == 3 && h_lo >= u128::MAX - 4) {
                h_lo = h_lo.wrapping_add(5);
            }
            let (tag, _) = h_lo.overflowing_add(s);
            tag.to_le_bytes()
        }
        let mut pkey = [0u8; 32];
        pkey.copy_from_slice(&[
            0x85, 0xd6, 0xbe, 0x78, 0x57, 0x55, 0x6d, 0x33, 0x7f, 0x44, 0x52, 0xfe, 0x42, 0xd5,
            0x06, 0xa8, 0x01, 0x03, 0x80, 0x8a, 0xfb, 0x0d, 0xb2, 0xfd, 0x4a, 0xbf, 0xf6, 0xaf,
            0x41, 0x49, 0xf5, 0x1b,
        ]);
        let mac = poly1305(&pkey, b"Cryptographic Forum Research Group");
        let exp = [
            0xa8, 0x06, 0x1d, 0xc1, 0x30, 0x51, 0x36, 0xc6, 0xc2, 0x2b, 0x8b, 0xaf, 0x0c, 0x01,
            0x27, 0xa9,
        ];
        assert_eq!(mac, exp, "got {:02x?}", mac);
    }

    #[test]
    fn exfat_oem_magic() {
        let mut boot = vec![0u8; 512];
        boot[3..11].copy_from_slice(b"EXFAT   ");
        assert!(exfat_magic_ok(&boot));
        boot[3] = b'X';
        assert!(!exfat_magic_ok(&boot));
        assert!(!exfat_magic_ok(&[0u8; 4]));
    }

    #[test]
    fn xfs_sb_magic() {
        let mut sb = vec![0u8; 16];
        sb[0..4].copy_from_slice(&0x5846_5342u32.to_be_bytes());
        assert!(xfs_magic_ok(&sb));
        sb[0] = 0;
        assert!(!xfs_magic_ok(&sb));
        assert!(!xfs_magic_ok(&[0u8; 2]));
    }

    #[test]
    fn btrfs_vlan_beacon_parsers() {
        let mut sb = vec![0u8; 0x48];
        sb[0x40..0x48].copy_from_slice(b"_BHRfS_M");
        assert!(btrfs_magic_ok(&sb));
        sb[0x40] = 0;
        assert!(!btrfs_magic_ok(&sb));
        let tagged = [
            0xFFu8, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x52, 0x54, 0x00, 0x12, 0x34, 0x56, 0x81, 0x00,
            0x00, 0x64,
        ];
        assert_eq!(vlan_tpid(&tagged), Some(0x8100));
        assert!(vlan_tpid(&[0u8; 12]).is_none());
        let mut beacon = vec![0u8; 36];
        beacon[0] = 0x80;
        beacon.push(0);
        beacon.push(4);
        beacon.extend_from_slice(b"IEEE");
        assert_eq!(parse_beacon_ssid(&beacon), Some(&b"IEEE"[..]));
        assert!(parse_beacon_ssid(&[0u8; 10]).is_none());
    }

    #[test]
    fn mo_magic_and_scsi_read10() {
        assert!(mo_magic_ok(&[0xDE, 0x12, 0x04, 0x95, 0, 0, 0, 0]));
        assert!(!mo_magic_ok(&[0, 1, 2, 3]));
        let mut cdb = [0u8; 10];
        cdb[0] = 0x28;
        cdb[5] = 7;
        cdb[8] = 2;
        assert_eq!(scsi_read10_lba(&cdb), Some((7, 2)));
        assert!(scsi_read10_lba(&[0x00u8; 10]).is_none());
    }

    #[test]
    fn hub_gamepad_hint_tis_parsers() {
        let desc = [9u8, 0x29, 4, 0, 0, 50, 100];
        assert_eq!(parse_hub_num_ports(&desc), Some(4));
        assert!(parse_hub_num_ports(&[9u8, 0x00, 4]).is_none());
        assert_eq!(hub_port_connected(&[0x03, 0x01, 0, 0]), Some(true));
        assert_eq!(hub_port_connected(&[0x00, 0x00, 0, 0]), Some(false));
        assert!(hub_port_connected(&[0x01]).is_none());
        let report = [1u8, 8, 255, 128, 128, 128, 0, 0];
        assert_eq!(hid_gamepad_a_pressed(&report), Some(true));
        assert_eq!(hid_gamepad_a_pressed(&[0u8; 8]), Some(false));
        assert!(hid_gamepad_a_pressed(&[0u8; 2]).is_none());
        assert_eq!(round_26_6(33 * 64 + 20), 33 * 64);
        assert_eq!(round_26_6(10 * 64), 10 * 64);
        assert_eq!(tis_grant_locality(0x02), 0xA0);
        assert_eq!(tis_grant_locality(0x00), 0x81);
    }

    #[test]
    fn usb_hid_uvc_audio_ecm_parsers() {
        assert_eq!(
            parse_hid_digitizer_xy(&[0x01, 0xFF, 0x3F, 0x00, 0x00]),
            Some((0x3FFF, 0, true))
        );
        assert!(parse_hid_digitizer_xy(&[0x01, 0x00]).is_none());
        let mt = [
            2u8, 0, 1, 0x00, 0x04, 0xE8, 0x03, 1, 1, 0x00, 0x08, 0xE8, 0x03,
        ];
        assert_eq!(parse_mt_contact_count(&mt), Some(2));
        assert!(parse_mt_contact_count(&[0u8]).is_none());
        assert!(parse_mt_contact_count(&[2u8, 0, 1]).is_none());
        let wacom = [0x02u8, 0x03, 0xE8, 0x03, 0xD0, 0x07, 0x00, 0x10, 10, 0];
        assert_eq!(parse_wacom_pressure(&wacom), Some(4096));
        assert!(parse_wacom_pressure(&[0u8; 4]).is_none());
        assert_eq!(parse_uvc_eof(&[2u8, 0x03, 0x10, 0x80]), Some(true));
        assert!(parse_uvc_eof(&[8u8]).is_none());
        assert_eq!(parse_uac_set_cur_freq(&[0x80, 0xBB, 0x00]), Some(48000));
        assert!(parse_uac_set_cur_freq(&[0x80]).is_none());
        let mut frame = [0u8; 14];
        frame[12..14].copy_from_slice(&0x0800u16.to_be_bytes());
        assert_eq!(parse_cdc_ecm_ethertype(&frame), Some(0x0800));
        assert!(parse_cdc_ecm_ethertype(&[0u8; 13]).is_none());
    }

    #[test]
    fn smb2_rtl8139_fuse_fw_parsers() {
        let mut smb = vec![0u8; 64];
        smb[0..4].copy_from_slice(&[0xFE, 0x53, 0x4D, 0x42]);
        smb[4..6].copy_from_slice(&64u16.to_le_bytes());
        smb[12..14].copy_from_slice(&0u16.to_le_bytes());
        assert_eq!(parse_smb2_command(&smb), Some(0));
        smb[0] = 0;
        assert!(parse_smb2_command(&smb).is_none());
        assert!(parse_smb2_command(&[0u8; 8]).is_none());
        assert_eq!(parse_rtl8139_rx_header(0x0001 | (16 << 16)), Some(16));
        assert!(parse_rtl8139_rx_header(0).is_none());
        let mut entry = [0u8; 20];
        entry[0..8].copy_from_slice(&2u64.to_le_bytes());
        entry[8..16].copy_from_slice(&12u64.to_le_bytes());
        entry[16..20].copy_from_slice(&0o100644u32.to_le_bytes());
        assert_eq!(parse_fuse_entry(&entry), Some((2, 12, 0o100644)));
        assert!(parse_fuse_entry(&[0u8; 4]).is_none());
        let mut amd = b"AMDGPUFW".to_vec();
        amd.extend_from_slice(&1u32.to_le_bytes());
        amd.extend_from_slice(&1u32.to_le_bytes());
        amd.extend_from_slice(&3u32.to_le_bytes());
        amd.extend_from_slice(b"psp");
        assert_eq!(parse_amdgpu_firmware_ips(&amd), Some(1 << 1));
        assert!(parse_amdgpu_firmware_ips(b"NOPE").is_none());
        let mut iwl = b"IWLWIFI\0".to_vec();
        iwl.extend_from_slice(&1u32.to_le_bytes());
        iwl.extend_from_slice(&1u32.to_le_bytes());
        iwl.extend_from_slice(&3u32.to_le_bytes());
        iwl.extend_from_slice(b"mac");
        assert_eq!(parse_iwl_firmware_sections(&iwl), Some(1));
        assert!(parse_iwl_firmware_sections(b"NOPE").is_none());
    }

    #[test]
    fn later_list_parsers() {
        let mut ub = [0u8; 8];
        ub[0..4].copy_from_slice(&0x00BA_B10Cu32.to_be_bytes());
        assert_eq!(parse_zfs_uberblock_magic(&ub), Some(0x00BA_B10C));
        assert!(parse_zfs_uberblock_magic(&[0u8; 2]).is_none());
        assert_eq!(parse_nfs4_op(&15u32.to_be_bytes()), Some(15));
        assert!(parse_nfs4_op(&[0, 0, 0, 0]).is_none());
        let mut rec = [0u8; 6];
        rec[0] = 22;
        rec[5] = 1;
        assert_eq!(parse_tls_handshake_type(&rec), Some(1));
        rec[0] = 23;
        assert!(parse_tls_handshake_type(&rec).is_none());
        assert_eq!(parse_sae_commit_group(&19u16.to_le_bytes()), Some(19));
        assert!(parse_sae_commit_group(&[0, 0]).is_none());
        assert_eq!(parse_ept_pte(0b111), Some((true, true, true)));
        assert!(parse_ept_pte(0).is_none());
        assert_eq!(
            parse_virtio_mmio_magic(&0x7472_6976u32.to_le_bytes()),
            Some(0x7472_6976)
        );
        assert!(parse_virtio_mmio_magic(&[0, 0, 0, 0]).is_none());
        let mut brcm = b"BRCMFW".to_vec();
        brcm.extend_from_slice(&1u32.to_le_bytes());
        brcm.extend_from_slice(&1u32.to_le_bytes());
        brcm.extend_from_slice(&3u32.to_le_bytes());
        brcm.extend_from_slice(b"mac");
        assert_eq!(parse_brcm_firmware_sections(&brcm), Some(1));
        assert!(parse_brcm_firmware_sections(b"NOPE").is_none());
        assert_eq!(parse_tb_route(&1u64.to_le_bytes()), Some(1));
        assert!(parse_tb_route(&[0u8; 8]).is_none());
        let mut vin = [0u8; 8];
        vin[0] = 3; // EV_ABS
        vin[4] = 0xFF;
        vin[5] = 0x3F;
        assert_eq!(parse_virtio_input_event(&vin), Some((3, 0, 0x3FFF)));
        assert!(parse_virtio_input_event(&[0u8; 8]).is_none());
    }

    #[test]
    fn ipv4_checksum_zero_header() {
        let mut hdr = [0u8; 20];
        hdr[0] = 0x45;
        let c = ipv4_header_checksum(&hdr);
        hdr[10] = (c >> 8) as u8;
        hdr[11] = c as u8;
        let mut chk = hdr;
        chk[10] = 0;
        chk[11] = 0;
        assert_eq!(ipv4_header_checksum(&chk), c);
    }

    #[test]
    fn aml_s5_package() {
        let aml = [
            0x08, b'_', b'S', b'5', 0x00, 0x12, 0x06, 0x04, 0x0A, 0x07, 0x00, 0x00, 0x00,
        ];
        assert_eq!(parse_aml_s5_slp_typ(&aml), Some(7));
        // Firmware NameSeg `_S5_` and QEMU `Name(\_S5, Package(){Zero,...})`.
        let qemu = [
            0x08, 0x5C, b'_', b'S', b'5', b'_', 0x12, 0x06, 0x04, 0x00, 0x00, 0x00, 0x00,
        ];
        assert_eq!(parse_aml_s5_slp_typ(&qemu), Some(0));
        assert!(parse_aml_s5_slp_typ(&[0x00, 0x01]).is_none());
    }

    #[test]
    fn quota_rounds_up_to_kib() {
        assert_eq!(quota_kib(1), 1);
        assert_eq!(quota_kib(1024), 1);
        assert_eq!(quota_kib(2048), 2);
        assert!(quota_kib(2048) > 1);
    }

    #[test]
    fn packet_parsers_reject_truncated() {
        assert!(parse_ethernet(&[]).is_err());
        assert!(parse_ethernet(&[0u8; 13]).is_err());
        assert_eq!(parse_ethernet(&minimal_ipv4_frame()), Ok(PacketKind::Ipv4));
        assert!(parse_ipv4(&[]).is_err());
        assert!(parse_ipv4(&[0x45]).is_err());
        let mut ihl = vec![0x4Fu8; 20];
        assert!(parse_ipv4(&ihl).is_err());
        ihl[0] = 0x45;
        ihl[9] = 6;
        assert_eq!(parse_ipv4(&ihl), Ok(6));
        assert!(parse_ipv6(&[0u8; 39]).is_err());
        let mut v6 = vec![0x60u8; 40];
        assert_eq!(parse_ipv6(&v6), Ok(0x60));
        v6[0] = 0x40;
        assert!(parse_ipv6(&v6).is_err());
    }

    #[test]
    fn fuzz_elf_malformed_does_not_panic() {
        let seed = minimal_elf();
        for round in 0..256 {
            let mut buf = fuzz_mutate(&seed, round);
            let _ = validate_elf(&buf);
            buf.truncate(round % 17);
            let _ = validate_elf(&buf);
        }
    }

    #[test]
    fn fuzz_ext4_malformed_does_not_panic() {
        let seed = minimal_ext4_sb();
        for round in 0..256 {
            let buf = fuzz_mutate(&seed, round);
            let _ = parse_ext4_superblock(&buf);
            let _ = ext4_magic_ok(&buf);
        }
    }

    #[test]
    fn fuzz_packet_malformed_does_not_panic() {
        let seed = minimal_ipv4_frame();
        for round in 0..256 {
            let buf = fuzz_mutate(&seed, round);
            let _ = parse_ethernet(&buf);
            if buf.len() >= 14 {
                let _ = parse_ipv4(&buf[14.min(buf.len())..]);
            }
            let _ = parse_ipv6(&buf);
            let _ = parse_hid_digitizer_xy(&buf);
            let _ = parse_mt_contact_count(&buf);
            let _ = parse_wacom_pressure(&buf);
            let _ = parse_uvc_eof(&buf);
            let _ = parse_uac_set_cur_freq(&buf);
            let _ = parse_cdc_ecm_ethertype(&buf);
            let _ = parse_smb2_command(&buf);
            let _ = parse_fuse_entry(&buf);
            let _ = parse_amdgpu_firmware_ips(&buf);
            let _ = parse_iwl_firmware_sections(&buf);
            let _ = parse_zfs_uberblock_magic(&buf);
            let _ = parse_nfs4_op(&buf);
            let _ = parse_tls_handshake_type(&buf);
            let _ = parse_sae_commit_group(&buf);
            if buf.len() >= 8 {
                let mut pte = [0u8; 8];
                pte.copy_from_slice(&buf[..8]);
                let _ = parse_ept_pte(u64::from_le_bytes(pte));
            }
            let _ = parse_virtio_mmio_magic(&buf);
            let _ = parse_virtio_input_event(&buf);
            let _ = parse_brcm_firmware_sections(&buf);
            let _ = parse_tb_route(&buf);
        }
    }
}
