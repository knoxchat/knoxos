use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::serial_println;

use super::bss::{BSS_LIST, BssEntry, BssSecurity};
use super::iface::{
    FrequencyBand, InterfaceMode, PowerSaveMode, WIFI_INTERFACES, WifiBand, WifiChannel, WifiState,
    WirelessInterface,
};
use super::security::{AkmSuite, CipherSuite, PmfMode, SecurityConfig};
use super::wpa::{wpa2_derive_pmk, wpa2_derive_ptk};

/// Register a wireless interface
pub fn register_interface(name: &str, mac: [u8; 6]) -> Result<(), &'static str> {
    let mut interfaces = WIFI_INTERFACES.lock();

    // Build default channel lists
    let band_2g = WifiBand {
        band: FrequencyBand::Band2Ghz,
        channels: (1..=14)
            .map(|ch| {
                let freq = if ch <= 13 { 2407 + ch as u32 * 5 } else { 2484 };
                WifiChannel {
                    number: ch,
                    frequency: freq,
                    max_power: 20,
                    flags: 0,
                    dfs: false,
                }
            })
            .collect(),
        ht_capable: true,
        vht_capable: false,
    };

    let band_5g = WifiBand {
        band: FrequencyBand::Band5Ghz,
        channels: vec![
            WifiChannel {
                number: 36,
                frequency: 5180,
                max_power: 23,
                flags: 0,
                dfs: false,
            },
            WifiChannel {
                number: 40,
                frequency: 5200,
                max_power: 23,
                flags: 0,
                dfs: false,
            },
            WifiChannel {
                number: 44,
                frequency: 5220,
                max_power: 23,
                flags: 0,
                dfs: false,
            },
            WifiChannel {
                number: 48,
                frequency: 5240,
                max_power: 23,
                flags: 0,
                dfs: false,
            },
            WifiChannel {
                number: 52,
                frequency: 5260,
                max_power: 23,
                flags: 0,
                dfs: true,
            },
            WifiChannel {
                number: 56,
                frequency: 5280,
                max_power: 23,
                flags: 0,
                dfs: true,
            },
            WifiChannel {
                number: 60,
                frequency: 5300,
                max_power: 23,
                flags: 0,
                dfs: true,
            },
            WifiChannel {
                number: 64,
                frequency: 5320,
                max_power: 23,
                flags: 0,
                dfs: true,
            },
            WifiChannel {
                number: 149,
                frequency: 5745,
                max_power: 30,
                flags: 0,
                dfs: false,
            },
            WifiChannel {
                number: 153,
                frequency: 5765,
                max_power: 30,
                flags: 0,
                dfs: false,
            },
            WifiChannel {
                number: 157,
                frequency: 5785,
                max_power: 30,
                flags: 0,
                dfs: false,
            },
            WifiChannel {
                number: 161,
                frequency: 5805,
                max_power: 30,
                flags: 0,
                dfs: false,
            },
            WifiChannel {
                number: 165,
                frequency: 5825,
                max_power: 30,
                flags: 0,
                dfs: false,
            },
        ],
        ht_capable: true,
        vht_capable: true,
    };

    let idx = interfaces.len() as u32;
    interfaces.insert(
        String::from(name),
        WirelessInterface {
            name: String::from(name),
            index: idx,
            mac,
            mode: InterfaceMode::Station,
            state: WifiState::Disconnected,
            phy_index: 0,
            channel: None,
            frequency: None,
            bandwidth: 20,
            tx_power: 20,
            signal: -100,
            ssid: None,
            bssid: None,
            security: None,
            bands: vec![band_2g, band_5g],
        },
    );

    serial_println!("[WiFi] Registered interface {}", name);
    Ok(())
}

/// Set interface mode
pub fn set_mode(name: &str, mode: InterfaceMode) -> Result<(), &'static str> {
    let mut interfaces = WIFI_INTERFACES.lock();
    let iface = interfaces.get_mut(name).ok_or("Interface not found")?;

    if iface.state != WifiState::Disconnected {
        return Err("Must disconnect before changing mode");
    }

    iface.mode = mode;
    serial_println!("[WiFi] {} mode set to {:?}", name, mode);
    Ok(())
}

/// Start scanning for networks
pub fn scan(name: &str) -> Result<(), &'static str> {
    let mut interfaces = WIFI_INTERFACES.lock();
    let iface = interfaces.get_mut(name).ok_or("Interface not found")?;

    if iface.mode != InterfaceMode::Station {
        return Err("Scanning only in station mode");
    }

    iface.state = WifiState::Scanning;
    serial_println!("[WiFi] Started scan on {}", name);
    drop(interfaces);

    let mut bss_list = BSS_LIST.lock();
    bss_list.clear();

    // Try real scan via VirtIO WiFi driver first
    if crate::virtio_wifi::is_present() {
        serial_println!("[WiFi] Using VirtIO WiFi driver for scan");
        // Trigger hardware scan on all channels
        let _ = crate::virtio_wifi::scan(0, None);

        // Wait briefly for scan results
        for _ in 0..50_000u32 {
            core::hint::spin_loop();
        }

        // Collect scan results from VirtIO driver
        if let Some(status) = crate::virtio_wifi::get_status() {
            serial_println!(
                "[WiFi] VirtIO WiFi link_up={}, channel={}",
                status.link_up,
                status.channel
            );
        }

        // Process received beacon/probe response frames
        let rx_frames = crate::virtio_wifi::process_rx();
        for frame in &rx_frames {
            if frame.len() < 24 {
                continue;
            }
            // Parse 802.11 management frame (beacon/probe response)
            let frame_ctrl = u16::from_le_bytes([frame[0], frame[1]]);
            let frame_type = (frame_ctrl >> 2) & 0x03;
            let frame_subtype = (frame_ctrl >> 4) & 0x0F;

            // Type 0 = Management, Subtype 8 = Beacon, 5 = Probe Response
            if frame_type == 0 && (frame_subtype == 8 || frame_subtype == 5) {
                let mut bssid = [0u8; 6];
                bssid.copy_from_slice(&frame[16..22]);

                // Parse information elements starting at offset 36
                // (after fixed fields: timestamp, beacon_interval, capability)
                let mut ssid = String::new();
                let mut channel = 0u8;
                let mut security = BssSecurity::Open;
                let mut ht_capable = false;
                let mut supported_rates = Vec::new();

                let mut ie_offset = 36;
                while ie_offset + 2 <= frame.len() {
                    let ie_id = frame[ie_offset];
                    let ie_len = frame[ie_offset + 1] as usize;
                    if ie_offset + 2 + ie_len > frame.len() {
                        break;
                    }
                    let ie_data = &frame[ie_offset + 2..ie_offset + 2 + ie_len];

                    match ie_id {
                        0 => {
                            // SSID
                            ssid = String::from_utf8_lossy(ie_data).into_owned();
                        }
                        1 => {
                            // Supported Rates
                            for &rate in ie_data {
                                supported_rates.push(rate & 0x7F);
                            }
                        }
                        3
                            // DS Parameter Set (channel)
                            if !ie_data.is_empty() => {
                                channel = ie_data[0];
                            }
                        45 => {
                            // HT Capabilities
                            ht_capable = true;
                        }
                        48 => {
                            // RSN (WPA2)
                            security = BssSecurity::Wpa2Psk;
                        }
                        _ => {}
                    }
                    ie_offset += 2 + ie_len;
                }

                let beacon_interval = if frame.len() >= 34 {
                    u16::from_le_bytes([frame[32], frame[33]])
                } else {
                    100
                };
                let capability = if frame.len() >= 36 {
                    u16::from_le_bytes([frame[34], frame[35]])
                } else {
                    0
                };

                let freq = if channel <= 14 {
                    2407 + (channel as u32) * 5
                } else {
                    5000 + (channel as u32) * 5
                };

                bss_list.push(BssEntry {
                    bssid,
                    ssid,
                    channel,
                    frequency: freq,
                    rssi: -50, // TODO: extract from radiotap header
                    security,
                    beacon_interval,
                    capability,
                    supported_rates,
                    ht_capable,
                    vht_capable: false,
                    last_seen_tick: 0,
                });
            }
        }
    }

    // If no results from hardware, add virtual test entries for development
    if bss_list.is_empty() {
        bss_list.push(BssEntry {
            bssid: [0xAA, 0xBB, 0xCC, 0x11, 0x22, 0x33],
            ssid: String::from("KnoxOS-Net"),
            channel: 6,
            frequency: 2437,
            rssi: -45,
            security: BssSecurity::Wpa2Psk,
            beacon_interval: 100,
            capability: 0x0431,
            supported_rates: vec![2, 4, 11, 22, 12, 18, 24, 36, 48, 72, 96, 108],
            ht_capable: true,
            vht_capable: false,
            last_seen_tick: 0,
        });

        bss_list.push(BssEntry {
            bssid: [0xDD, 0xEE, 0xFF, 0x44, 0x55, 0x66],
            ssid: String::from("Guest-WiFi"),
            channel: 11,
            frequency: 2462,
            rssi: -72,
            security: BssSecurity::Open,
            beacon_interval: 100,
            capability: 0x0421,
            supported_rates: vec![2, 4, 11, 22, 12, 18, 24, 36],
            ht_capable: true,
            vht_capable: false,
            last_seen_tick: 0,
        });
    }

    drop(bss_list);

    let mut interfaces = WIFI_INTERFACES.lock();
    if let Some(iface) = interfaces.get_mut(name) {
        iface.state = WifiState::Disconnected; // Scan complete
    }

    serial_println!("[WiFi] Scan complete on {}", name);
    Ok(())
}

/// Get scan results
pub fn get_scan_results() -> Vec<BssEntry> {
    BSS_LIST.lock().clone()
}

/// Connect to a network
pub fn connect(name: &str, ssid: &str, passphrase: Option<&str>) -> Result<(), &'static str> {
    let bss = {
        let bss_list = BSS_LIST.lock();
        bss_list.iter().find(|b| b.ssid == ssid).cloned()
    };

    let bss = bss.ok_or("Network not found in scan results")?;

    // Security validation
    match bss.security {
        BssSecurity::Wpa2Psk | BssSecurity::Wpa3Sae | BssSecurity::WpaPsk
            if passphrase.is_none() =>
        {
            return Err("Passphrase required for secured network");
        }
        _ => {}
    }

    let mut interfaces = WIFI_INTERFACES.lock();
    let iface = interfaces.get_mut(name).ok_or("Interface not found")?;

    // State machine: Authenticating → Associating → 4-Way Handshake → Connected
    iface.state = WifiState::Authenticating;
    serial_println!(
        "[WiFi] Authenticating to '{}' ({:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X})",
        ssid,
        bss.bssid[0],
        bss.bssid[1],
        bss.bssid[2],
        bss.bssid[3],
        bss.bssid[4],
        bss.bssid[5]
    );

    iface.state = WifiState::Associating;
    serial_println!("[WiFi] Associating...");

    if bss.security != BssSecurity::Open {
        iface.state = WifiState::FourWayHandshake;
        serial_println!("[WiFi] 4-way handshake in progress...");

        // WPA2 4-Way Handshake (IEEE 802.11i)
        // Step 1: Derive PMK from passphrase + SSID using PBKDF2-SHA1
        let pmk = if let Some(pass) = passphrase {
            let mut key = [0u8; 32];
            wpa2_derive_pmk(pass.as_bytes(), ssid.as_bytes(), &mut key);
            serial_println!("[WiFi] PMK derived via PBKDF2-SHA1 (4096 iterations)");
            key
        } else {
            [0u8; 32]
        };

        // Step 2: Generate SNonce (supplicant nonce)
        let mut snonce = [0u8; 32];
        crate::random::fill_random(&mut snonce);

        // Step 3: The AP would send ANonce in EAPOL Message 1
        // Simulate receiving ANonce from AP
        let mut anonce = [0u8; 32];
        crate::random::fill_random(&mut anonce);

        // Step 4: Derive PTK from PMK, ANonce, SNonce, AP MAC, STA MAC
        // PTK = PRF-384(PMK, "Pairwise key expansion",
        //               min(AA,SA) || max(AA,SA) || min(ANonce,SNonce) || max(ANonce,SNonce))
        let ptk = wpa2_derive_ptk(&pmk, &anonce, &snonce, &bss.bssid, &iface.mac);
        serial_println!("[WiFi] PTK derived (KCK + KEK + TK = 48 bytes)");

        // Step 5: MIC verification would happen here using KCK (first 16 bytes of PTK)

        // Step 6: GTK is delivered in EAPOL Message 3, encrypted with KEK
        let mut gtk = [0u8; 32];
        crate::random::fill_random(&mut gtk);
        serial_println!("[WiFi] GTK installed");

        let security = SecurityConfig {
            pairwise_cipher: CipherSuite::Ccmp,
            group_cipher: CipherSuite::Ccmp,
            akm: if bss.security == BssSecurity::Wpa3Sae {
                AkmSuite::Sae
            } else {
                AkmSuite::Psk
            },
            pmf: if bss.security == BssSecurity::Wpa3Sae {
                PmfMode::Required
            } else {
                PmfMode::Optional
            },
            passphrase: passphrase.map(String::from),
            pmk: Some(pmk),
            ptk: Some(ptk),
            gtk: Some(gtk),
        };
        iface.security = Some(security);
    }

    iface.state = WifiState::Connected;
    iface.ssid = Some(String::from(ssid));
    iface.bssid = Some(bss.bssid);
    iface.channel = Some(bss.channel);
    iface.frequency = Some(bss.frequency);
    iface.signal = bss.rssi;

    serial_println!(
        "[WiFi] Connected to '{}' on channel {} ({} dBm)",
        ssid,
        bss.channel,
        bss.rssi
    );
    Ok(())
}

/// Disconnect from current network
pub fn disconnect(name: &str) -> Result<(), &'static str> {
    let mut interfaces = WIFI_INTERFACES.lock();
    let iface = interfaces.get_mut(name).ok_or("Interface not found")?;

    if iface.state == WifiState::Disconnected {
        return Ok(());
    }

    let ssid = iface.ssid.clone().unwrap_or_default();
    iface.state = WifiState::Disconnecting;
    serial_println!("[WiFi] Disconnecting from '{}'...", ssid);

    iface.state = WifiState::Disconnected;
    iface.ssid = None;
    iface.bssid = None;
    iface.channel = None;
    iface.frequency = None;
    iface.signal = -100;
    iface.security = None;

    serial_println!("[WiFi] Disconnected");
    Ok(())
}

/// Get interface status
pub fn get_status(name: &str) -> Option<WirelessInterface> {
    let interfaces = WIFI_INTERFACES.lock();
    interfaces.get(name).cloned()
}

/// Initiate a Wi-Fi scan for available networks
pub fn scan_networks(name: &str, _specific_ssid: Option<&str>) -> Result<(), &'static str> {
    let mut interfaces = WIFI_INTERFACES.lock();
    let iface = interfaces.get_mut(name).ok_or("Interface not found")?;

    iface.state = WifiState::Scanning;
    serial_println!("[WiFi] {} scanning for networks...", name);

    // In real implementation, this would:
    // 1. Transmit probe requests on all channels
    // 2. Collect probe responses and beacons
    // 3. Build BSS list (handled by process_beacon/probe_response)

    Ok(())
}

/// Process beacon frame to discover networks
pub fn process_beacon(
    bssid: [u8; 6],
    ssid: &str,
    frequency: u32,
    channel: u8,
    rssi: i32,
    security: BssSecurity,
) {
    let mut list = BSS_LIST.lock();

    // Check if BSSID already in list
    if let Some(entry) = list.iter_mut().find(|b| b.bssid == bssid) {
        entry.rssi = rssi;
        entry.last_seen_tick = 0; // Would use system tick in real implementation
        return;
    }

    // Add new BSS entry
    let entry = BssEntry {
        bssid,
        ssid: String::from(ssid),
        frequency,
        channel,
        rssi,
        security,
        beacon_interval: 100,
        capability: 0x0411, // ESS, Privacy by default
        supported_rates: alloc::vec![0x8c, 0x12, 0x98, 0x24], // 6, 9, 12, 18 Mbps
        ht_capable: true,
        vht_capable: false,
        last_seen_tick: 0,
    };

    list.push(entry);
    serial_println!(
        "[WiFi] Beacon from {} '{}' on channel {} ({} MHz, {} dBm)",
        alloc::format!(
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            bssid[0],
            bssid[1],
            bssid[2],
            bssid[3],
            bssid[4],
            bssid[5]
        ),
        ssid,
        channel,
        frequency,
        rssi
    );
}

/// Set transmit power level
pub fn set_tx_power(name: &str, power_dbm: i32) -> Result<(), &'static str> {
    let mut interfaces = WIFI_INTERFACES.lock();
    let iface = interfaces.get_mut(name).ok_or("Interface not found")?;
    iface.tx_power = power_dbm.clamp(-20, 30); // Clamp -20..+30 dBm
    serial_println!("[WiFi] {} TX power set to {} dBm", name, iface.tx_power);
    Ok(())
}

/// Get signal strength of current connection
pub fn get_signal_strength(name: &str) -> Result<i32, &'static str> {
    let interfaces = WIFI_INTERFACES.lock();
    let iface = interfaces.get(name).ok_or("Interface not found")?;
    Ok(iface.signal)
}

/// Set power save mode
pub fn set_power_save(name: &str, mode: PowerSaveMode) -> Result<(), &'static str> {
    let mut interfaces = WIFI_INTERFACES.lock();
    let _iface = interfaces.get_mut(name).ok_or("Interface not found")?;
    serial_println!("[WiFi] {} power save: {:?}", name, mode);
    Ok(())
}

/// Get wireless info for iwconfig-style display
pub fn iwconfig_info(name: &str) -> String {
    let interfaces = WIFI_INTERFACES.lock();
    let iface = match interfaces.get(name) {
        Some(i) => i,
        None => return alloc::format!("{}: No such device\n", name),
    };

    let mut out = String::new();
    out.push_str(&alloc::format!("{}  IEEE 802.11  ", name));
    if let Some(ref ssid) = iface.ssid {
        out.push_str(&alloc::format!("ESSID:\"{}\"  \n", ssid));
    } else {
        out.push_str("ESSID:off/any  \n");
    }
    out.push_str(&alloc::format!("          Mode:{:?}  ", iface.mode));
    if let Some(freq) = iface.frequency {
        out.push_str(&alloc::format!(
            "Frequency:{}.{} GHz  ",
            freq / 1000,
            (freq % 1000) / 100
        ));
    }
    if let Some(ref bssid) = iface.bssid {
        out.push_str(&alloc::format!(
            "Access Point: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            bssid[0],
            bssid[1],
            bssid[2],
            bssid[3],
            bssid[4],
            bssid[5]
        ));
    }
    out.push('\n');
    out.push_str(&alloc::format!(
        "          Tx-Power={} dBm  ",
        iface.tx_power
    ));
    out.push_str(&alloc::format!("Signal level={} dBm\n", iface.signal));
    out
}
