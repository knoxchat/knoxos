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

/// GNU .mo little-endian magic.
pub fn mo_magic_ok(data: &[u8]) -> bool {
    data.len() >= 4 && data[0..4] == [0xDE, 0x12, 0x04, 0x95]
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
        }
    }
}
