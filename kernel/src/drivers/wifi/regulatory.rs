use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Regulatory domain
#[derive(Debug, Clone)]
pub struct RegulatoryDomain {
    pub country_code: [u8; 2], // ISO 3166-1 alpha-2
    pub dfs_region: DfsRegion,
    pub rules: Vec<RegRule>,
}

/// DFS region
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DfsRegion {
    Unset,
    Fcc,
    Etsi,
    Japan,
}

/// Regulatory rule
#[derive(Debug, Clone)]
pub struct RegRule {
    pub start_freq_khz: u32,
    pub end_freq_khz: u32,
    pub max_bandwidth_khz: u32,
    pub max_antenna_gain_mbi: i32,
    pub max_eirp_mbm: i32,
    pub flags: u32,
}

static REGULATORY: Mutex<Option<RegulatoryDomain>> = Mutex::new(None);

/// Set regulatory domain
pub fn set_regdomain(country: &str) {
    let cc = if country.len() >= 2 {
        [country.as_bytes()[0], country.as_bytes()[1]]
    } else {
        [b'U', b'S']
    };

    let rules = match &cc {
        b"US" => vec![
            RegRule {
                start_freq_khz: 2402000,
                end_freq_khz: 2472000,
                max_bandwidth_khz: 40000,
                max_antenna_gain_mbi: 600,
                max_eirp_mbm: 3000,
                flags: 0,
            },
            RegRule {
                start_freq_khz: 5170000,
                end_freq_khz: 5250000,
                max_bandwidth_khz: 80000,
                max_antenna_gain_mbi: 600,
                max_eirp_mbm: 1700,
                flags: 0,
            },
            RegRule {
                start_freq_khz: 5250000,
                end_freq_khz: 5330000,
                max_bandwidth_khz: 80000,
                max_antenna_gain_mbi: 600,
                max_eirp_mbm: 2400,
                flags: 1,
            },
            RegRule {
                start_freq_khz: 5735000,
                end_freq_khz: 5835000,
                max_bandwidth_khz: 80000,
                max_antenna_gain_mbi: 600,
                max_eirp_mbm: 3000,
                flags: 0,
            },
        ],
        _ => vec![
            RegRule {
                start_freq_khz: 2402000,
                end_freq_khz: 2482000,
                max_bandwidth_khz: 40000,
                max_antenna_gain_mbi: 600,
                max_eirp_mbm: 2000,
                flags: 0,
            },
            RegRule {
                start_freq_khz: 5170000,
                end_freq_khz: 5330000,
                max_bandwidth_khz: 80000,
                max_antenna_gain_mbi: 600,
                max_eirp_mbm: 2000,
                flags: 1,
            },
            RegRule {
                start_freq_khz: 5490000,
                end_freq_khz: 5710000,
                max_bandwidth_khz: 80000,
                max_antenna_gain_mbi: 600,
                max_eirp_mbm: 2700,
                flags: 1,
            },
        ],
    };

    let dfs_region = match &cc {
        b"US" => DfsRegion::Fcc,
        b"JP" => DfsRegion::Japan,
        _ => DfsRegion::Etsi,
    };

    *REGULATORY.lock() = Some(RegulatoryDomain {
        country_code: cc,
        dfs_region,
        rules,
    });

    serial_println!(
        "[WiFi] Regulatory domain set to {}{}",
        cc[0] as char,
        cc[1] as char
    );
}
