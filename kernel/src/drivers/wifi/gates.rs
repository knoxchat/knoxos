use alloc::vec;
use alloc::vec::Vec;

use crate::serial_println;

use super::frames::parse_beacon_ssid;
use super::wpa::wpa2_derive_pmk;

fn build_gate_beacon(ssid: &[u8]) -> Vec<u8> {
    let mut frame = vec![0u8; 36];
    // Frame control: type=mgmt (0), subtype=beacon (8)
    frame[0] = 0x80;
    frame[1] = 0x00;
    let bssid = [0x02, 0x00, 0x00, 0x42, 0x42, 0x01];
    frame[16..22].copy_from_slice(&bssid);
    frame[32] = 100; // beacon interval
    frame[33] = 0;
    frame.push(0); // SSID IE id
    frame.push(ssid.len() as u8);
    frame.extend_from_slice(ssid);
    frame.push(48); // RSN IE → WPA2
    frame.push(1);
    frame.push(0);
    frame
}

pub const GATE_WIFI_MARKER: &str = "GATE_WIFI pmk";

pub(super) fn wifi_self_test() -> bool {
    let ssid = b"IEEE";
    let beacon = build_gate_beacon(ssid);
    let parsed = parse_beacon_ssid(&beacon);
    if parsed.as_deref() != Some("IEEE") {
        serial_println!("[WiFi] GATE_WIFI FAILED beacon ssid={:?}", parsed);
        return false;
    }
    let mut pmk = [0u8; 32];
    wpa2_derive_pmk(b"password", ssid, &mut pmk);
    // PBKDF2-HMAC-SHA1(password, "IEEE", 4096, 32)
    const EXP_PMK: [u8; 32] = [
        0xf4, 0x2c, 0x6f, 0xc5, 0x2d, 0xf0, 0xeb, 0xef, 0x9e, 0xbb, 0x4b, 0x90, 0xb3, 0x8a, 0x5f,
        0x90, 0x2e, 0x83, 0xfe, 0x1b, 0x13, 0x5a, 0x70, 0xe2, 0x3a, 0xed, 0x76, 0x2e, 0x97, 0x10,
        0xa1, 0x2e,
    ];
    if pmk == EXP_PMK {
        serial_println!("[WiFi] {} (beacon IEEE + WPA2 PMK)", GATE_WIFI_MARKER);
        true
    } else {
        serial_println!("[WiFi] GATE_WIFI FAILED pmk={:02x?}", &pmk[..4]);
        false
    }
}
