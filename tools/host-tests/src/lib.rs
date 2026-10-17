//! Host-side parsers used by KnoxOS. These tests run on the build machine
//! without QEMU so ELF / IPv6 / cron / ext4 magic can fail in CI.

pub fn elf_magic_ok(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0] == 0x7F && bytes[1] == b'E' && bytes[2] == b'L' && bytes[3] == b'F'
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

/// ext4 superblock magic is at byte 0x38 of the superblock (LBA 2).
pub const EXT4_SUPER_MAGIC: u16 = 0xEF53;

pub fn ext4_magic_ok(superblock: &[u8]) -> bool {
    if superblock.len() < 0x3A {
        return false;
    }
    u16::from_le_bytes([superblock[0x38], superblock[0x39]]) == EXT4_SUPER_MAGIC
}

pub fn quota_kib(bytes: usize) -> u64 {
    (bytes as u64).div_ceil(1024).max(1)
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
    }

    #[test]
    fn quota_rounds_up_to_kib() {
        assert_eq!(quota_kib(1), 1);
        assert_eq!(quota_kib(1024), 1);
        assert_eq!(quota_kib(2048), 2);
        assert!(quota_kib(2048) > 1);
    }
}
