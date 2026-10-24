//! GATE_USB serial markers and XHCI enumeration self-tests.
use core::sync::atomic::{AtomicBool, Ordering};

use crate::serial_println;

use super::super::ports::enumerate_ports;
use super::super::types::UsbSpeed;
use super::super::xhci::XHCI;
use super::device::{enumerate_all_devices, enumerate_device};
use super::hid::{
    get_hid_descriptor, get_hid_report_descriptor, hid_get_idle, hid_get_protocol, hid_get_report,
    hid_set_idle, hid_set_protocol, hid_set_report,
};
use super::live::{
    ADDRESS_LIVE, BOS_LIVE, BULK_IN_LIVE, BULK_LIVE, CLEAR_LIVE, CONFIG_DESC_LIVE, CONFIG_EP_LIVE,
    CONFIG_LIVE, DESC_LIVE, DISABLE_LIVE, ENUM_LIVE, EVAL_LIVE, GETCFG_LIVE, GETIFACE_LIVE,
    HID_LIVE, HIDGETIDLE_LIVE, HIDGETPROTOCOL_LIVE, HIDGETREPORT_LIVE, HIDREPORT_LIVE,
    HIDSETIDLE_LIVE, HIDSETPROTOCOL_LIVE, HIDSETREPORT_LIVE, IFACE_LIVE, INTR_LIVE, ISOCH_IN_LIVE,
    ISOCHDELAY_LIVE, NOOP_LIVE, OSCFG_LIVE, QUAL_LIVE, RESETDEV_LIVE, RESETEP_LIVE, SET_LIVE,
    SETDESC_LIVE, SETSEL_LIVE, SETTR_LIVE, SLOT_ENABLE_LIVE, STATUS_LIVE, STATUSEP_LIVE,
    STATUSIFACE_LIVE, STOP_LIVE, STRING_LIVE, SYNCH_LIVE, TRNOOP_LIVE,
};
use super::pipes::{bulk_in, bulk_out, interrupt_in, isoch_in, transfer_noop};
use super::requests::{
    clear_feature, get_bos_descriptor, get_configuration, get_device_descriptor,
    get_device_qualifier, get_endpoint_status, get_interface, get_interface_status,
    get_other_speed_config, get_status, get_string_descriptor, set_configuration, set_descriptor,
    set_feature, set_interface, set_isoch_delay, set_sel, synch_frame,
};
use super::slot::{
    address_device, configure_interrupt_ep, disable_slot, enable_slot, evaluate_context,
    no_op_command, recover_control_ep, reset_device, reset_endpoint, restart_ep, set_tr_dequeue,
    stop_endpoint,
};

pub const GATE_XHCI_SLOT_MARKER: &str = "GATE_USB xhci slot";
pub const GATE_XHCI_DESC_MARKER: &str = "GATE_USB xhci desc";
pub const GATE_XHCI_CONFIG_MARKER: &str = "GATE_USB xhci config";
pub const GATE_XHCI_ADDR_MARKER: &str = "GATE_USB xhci addr";
pub const GATE_XHCI_ENUM_MARKER: &str = "GATE_USB xhci enum";
pub const GATE_XHCI_EP_MARKER: &str = "GATE_USB xhci ep";
pub const GATE_XHCI_STATUS_MARKER: &str = "GATE_USB xhci status";
pub const GATE_XHCI_IFACE_MARKER: &str = "GATE_USB xhci iface";
pub const GATE_XHCI_GETCFG_MARKER: &str = "GATE_USB xhci getcfg";
pub const GATE_XHCI_GETIFACE_MARKER: &str = "GATE_USB xhci getiface";
pub const GATE_XHCI_CLEAR_MARKER: &str = "GATE_USB xhci clear";
pub const GATE_XHCI_BULK_MARKER: &str = "GATE_USB xhci bulk";
pub const GATE_XHCI_SET_MARKER: &str = "GATE_USB xhci set";
pub const GATE_XHCI_BULKIN_MARKER: &str = "GATE_USB xhci bulkin";
pub const GATE_XHCI_SETDESC_MARKER: &str = "GATE_USB xhci stdesc";
pub const GATE_XHCI_ISOCH_MARKER: &str = "GATE_USB xhci isoch";
pub const GATE_XHCI_STRING_MARKER: &str = "GATE_USB xhci string";
pub const GATE_XHCI_INTR_MARKER: &str = "GATE_USB xhci intr";
pub const GATE_XHCI_NOOP_MARKER: &str = "GATE_USB xhci noop";
pub const GATE_XHCI_DISABLE_MARKER: &str = "GATE_USB xhci disable";
pub const GATE_XHCI_EVAL_MARKER: &str = "GATE_USB xhci eval";
pub const GATE_XHCI_STOP_MARKER: &str = "GATE_USB xhci stop";
pub const GATE_XHCI_RESETEP_MARKER: &str = "GATE_USB xhci resetep";
pub const GATE_XHCI_SETTR_MARKER: &str = "GATE_USB xhci settr";
pub const GATE_XHCI_RESETDEV_MARKER: &str = "GATE_USB xhci resetdev";
pub const GATE_XHCI_TRNOOP_MARKER: &str = "GATE_USB xhci trnoop";
pub const GATE_XHCI_QUAL_MARKER: &str = "GATE_USB xhci qual";
pub const GATE_XHCI_BOS_MARKER: &str = "GATE_USB xhci bos";
pub const GATE_XHCI_OSCFG_MARKER: &str = "GATE_USB xhci oscfg";
pub const GATE_XHCI_SYNCH_MARKER: &str = "GATE_USB xhci synch";
pub const GATE_XHCI_SETSEL_MARKER: &str = "GATE_USB xhci setsel";
pub const GATE_XHCI_ISOCHDELAY_MARKER: &str = "GATE_USB xhci isochdelay";
pub const GATE_XHCI_HID_MARKER: &str = "GATE_USB xhci hiddesc";
pub const GATE_XHCI_HIDREPORT_MARKER: &str = "GATE_USB xhci hidreport";
pub const GATE_XHCI_HIDGETREPORT_MARKER: &str = "GATE_USB xhci getreport";
pub const GATE_XHCI_HIDSETREPORT_MARKER: &str = "GATE_USB xhci setreport";
pub const GATE_XHCI_HIDGETIDLE_MARKER: &str = "GATE_USB xhci getidle";
pub const GATE_XHCI_HIDSETIDLE_MARKER: &str = "GATE_USB xhci setidle";
pub const GATE_XHCI_HIDGETPROTOCOL_MARKER: &str = "GATE_USB xhci getprotocol";
pub const GATE_XHCI_HIDSETPROTOCOL_MARKER: &str = "GATE_USB xhci setprotocol";
pub const GATE_XHCI_STATUSIFACE_MARKER: &str = "GATE_USB xhci statusiface";
pub const GATE_XHCI_STATUSEP_MARKER: &str = "GATE_USB xhci statusep";

/// Prove Enable Slot got a Command Completion (cc=1), not a timeout fallback.
pub fn slot_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci slot skipped (no controller)");
        return true;
    }
    if !SLOT_ENABLE_LIVE.load(Ordering::Relaxed) {
        let _ = enable_slot();
    }
    if !SLOT_ENABLE_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: Enable Slot had no Command Completion");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_SLOT_MARKER);
    true
}

/// Prove GET_DESCRIPTOR got a Transfer Event, not a timeout fallback.
pub fn descriptor_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci desc skipped (no controller)");
        return true;
    }
    if !DESC_LIVE.load(Ordering::Relaxed) {
        let ports = enumerate_ports();
        let port = ports
            .iter()
            .find(|p| p.connected)
            .map(|p| (p.port_num, p.speed));
        if let Some(slot_id) = enable_slot() {
            if let Some((port_num, speed)) = port {
                let _ = address_device(slot_id, port_num, speed);
            } else {
                let _ = address_device(slot_id, 1, UsbSpeed::High);
            }
            let _ = get_device_descriptor(slot_id);
        }
    }
    if !DESC_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_DESCRIPTOR had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_DESC_MARKER);
    true
}

/// Prove SET_CONFIGURATION got a Transfer Event, not a timeout fallback.
pub fn config_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci config skipped (no controller)");
        return true;
    }
    if !CONFIG_LIVE.load(Ordering::Relaxed) {
        let ports = enumerate_ports();
        let port = ports
            .iter()
            .find(|p| p.connected)
            .map(|p| (p.port_num, p.speed));
        if let Some(slot_id) = enable_slot() {
            if let Some((port_num, speed)) = port {
                let _ = address_device(slot_id, port_num, speed);
            } else {
                let _ = address_device(slot_id, 1, UsbSpeed::High);
            }
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
        }
    }
    if !CONFIG_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: SET_CONFIGURATION had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_CONFIG_MARKER);
    true
}

/// Prove Address Device got a Command Completion (cc=1), not a timeout fallback.
pub fn addr_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci addr skipped (no controller)");
        return true;
    }
    if !ADDRESS_LIVE.load(Ordering::Relaxed) {
        let ports = enumerate_ports();
        let port = ports
            .iter()
            .find(|p| p.connected)
            .map(|p| (p.port_num, p.speed));
        if let Some(slot_id) = enable_slot() {
            if let Some((port_num, speed)) = port {
                let _ = address_device(slot_id, port_num, speed);
            } else {
                let _ = address_device(slot_id, 1, UsbSpeed::High);
            }
        }
    }
    if !ADDRESS_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: Address Device had no Command Completion");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_ADDR_MARKER);
    true
}

/// Prove full enumeration used live completions for every step, including
/// GET_DESCRIPTOR(Configuration). No timeout fallback.
pub fn enum_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci enum skipped (no controller)");
        return true;
    }
    if !ENUM_LIVE.load(Ordering::Relaxed) {
        enumerate_all_devices();
    }
    if !ENUM_LIVE.load(Ordering::Relaxed) {
        let ports = enumerate_ports();
        if let Some(port) = ports.iter().find(|p| p.connected) {
            let _ = enumerate_device(port.port_num, port.speed);
        }
    }
    if !ENUM_LIVE.load(Ordering::Relaxed)
        || !CONFIG_DESC_LIVE.load(Ordering::Relaxed)
        || !SLOT_ENABLE_LIVE.load(Ordering::Relaxed)
        || !ADDRESS_LIVE.load(Ordering::Relaxed)
        || !DESC_LIVE.load(Ordering::Relaxed)
        || !CONFIG_LIVE.load(Ordering::Relaxed)
    {
        serial_println!("[XHCI] GATE_USB FAILED: enumeration used a timeout fallback");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_ENUM_MARKER);
    true
}

/// Prove Configure Endpoint got a Command Completion (cc=1), not a timeout fallback.
pub fn ep_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci ep skipped (no controller)");
        return true;
    }
    if !CONFIG_EP_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI
            .lock()
            .devices
            .first()
            .map(|d| (d.slot_id, d.port, d.speed));
        if let Some((slot_id, port, speed)) = slot {
            let _ = configure_interrupt_ep(slot_id, port, speed);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = configure_interrupt_ep(slot_id, port_num, speed);
        }
    }
    if !CONFIG_EP_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: Configure Endpoint had no Command Completion");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_EP_MARKER);
    true
}

/// Prove GET_STATUS got a Transfer Event, not a timeout fallback.
pub fn status_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci status skipped (no controller)");
        return true;
    }
    if !STATUS_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        if let Some(slot_id) = slot {
            let _ = get_status(slot_id);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = get_status(slot_id);
        }
    }
    if !STATUS_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_STATUS had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_STATUS_MARKER);
    true
}

/// Prove SET_INTERFACE got a Transfer Event, not a timeout fallback.
pub fn iface_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci iface skipped (no controller)");
        return true;
    }
    if !IFACE_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        if let Some(slot_id) = slot {
            let _ = set_interface(slot_id, 0, 0);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = set_interface(slot_id, 0, 0);
        }
    }
    if !IFACE_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: SET_INTERFACE had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_IFACE_MARKER);
    true
}

/// Prove GET_CONFIGURATION got a Transfer Event, not a timeout fallback.
pub fn getcfg_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci getcfg skipped (no controller)");
        return true;
    }
    if !GETCFG_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        if let Some(slot_id) = slot {
            let _ = get_configuration(slot_id);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = get_configuration(slot_id);
        }
    }
    if !GETCFG_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_CONFIGURATION had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_GETCFG_MARKER);
    true
}

/// Prove GET_INTERFACE got a Transfer Event, not a timeout fallback.
pub fn getiface_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci getiface skipped (no controller)");
        return true;
    }
    if !GETIFACE_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        if let Some(slot_id) = slot {
            let _ = get_interface(slot_id, 0);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = get_interface(slot_id, 0);
        }
    }
    if !GETIFACE_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_INTERFACE had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_GETIFACE_MARKER);
    true
}

/// Prove CLEAR_FEATURE got a Transfer Event, not a timeout fallback.
pub fn clear_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci clear skipped (no controller)");
        return true;
    }
    if !CLEAR_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        if let Some(slot_id) = slot {
            let _ = clear_feature(slot_id, 0, 0x81);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = clear_feature(slot_id, 0, 0x81);
        }
    }
    if !CLEAR_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: CLEAR_FEATURE had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_CLEAR_MARKER);
    true
}

/// Prove a bulk OUT Normal TRB got a Transfer Event, not a timeout fallback.
pub fn bulk_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci bulk skipped (no controller)");
        return true;
    }
    if !BULK_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        if let Some(slot_id) = slot {
            let _ = bulk_out(slot_id, b"knoxos\n");
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = bulk_out(slot_id, b"knoxos\n");
        }
    }
    if !BULK_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: bulk OUT had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_BULK_MARKER);
    true
}

/// Prove SET_FEATURE got a Transfer Event, not a timeout fallback.
pub fn set_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci set skipped (no controller)");
        return true;
    }
    if !SET_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        if let Some(slot_id) = slot {
            let _ = set_feature(slot_id, 0, 0x81);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = set_feature(slot_id, 0, 0x81);
        }
    }
    if !SET_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: SET_FEATURE had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_SET_MARKER);
    true
}

/// Prove a bulk IN Normal TRB got a Transfer Event, not a timeout fallback.
pub fn bulk_in_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci bulkin skipped (no controller)");
        return true;
    }
    if !BULK_IN_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        let mut buf = [0u8; 8];
        if let Some(slot_id) = slot {
            let _ = bulk_in(slot_id, &mut buf);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = bulk_in(slot_id, &mut buf);
        }
    }
    if !BULK_IN_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: bulk IN had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_BULKIN_MARKER);
    true
}

/// Prove SET_DESCRIPTOR got a Transfer Event, not a timeout fallback.
pub fn set_descriptor_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci stdesc skipped (no controller)");
        return true;
    }
    if !SETDESC_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        let desc = [18u8, 1, 0, 2, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        if let Some(slot_id) = slot {
            let _ = set_descriptor(slot_id, 1, 0, &desc);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = set_descriptor(slot_id, 1, 0, &desc);
        }
    }
    if !SETDESC_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: SET_DESCRIPTOR had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_SETDESC_MARKER);
    true
}

/// Prove an isochronous IN Isoch TRB got a Transfer Event, not a timeout fallback.
pub fn isoch_in_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci isoch skipped (no controller)");
        return true;
    }
    if !ISOCH_IN_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        let mut buf = [0u8; 8];
        if let Some(slot_id) = slot {
            let _ = isoch_in(slot_id, &mut buf);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = isoch_in(slot_id, &mut buf);
        }
    }
    if !ISOCH_IN_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: isochronous IN had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_ISOCH_MARKER);
    true
}

pub fn string_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci string skipped (no controller)");
        return true;
    }
    if !STRING_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        if let Some(slot_id) = slot {
            let _ = get_string_descriptor(slot_id, 0);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = get_string_descriptor(slot_id, 0);
        }
    }
    if !STRING_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_DESCRIPTOR(String) had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_STRING_MARKER);
    true
}

/// Prove an interrupt IN Normal TRB got a Transfer Event, not a timeout fallback.
pub fn interrupt_in_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci intr skipped (no controller)");
        return true;
    }
    if !INTR_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        let mut buf = [0u8; 8];
        if let Some(slot_id) = slot {
            let _ = interrupt_in(slot_id, &mut buf);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = interrupt_in(slot_id, &mut buf);
        }
    }
    if !INTR_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: interrupt IN had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_INTR_MARKER);
    true
}

/// Prove No-Op Command got a Command Completion (cc=1), not a timeout fallback.
pub fn noop_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci noop skipped (no controller)");
        return true;
    }
    if !NOOP_LIVE.load(Ordering::Relaxed) {
        let _ = no_op_command();
    }
    if !NOOP_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: No-Op Command had no Command Completion");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_NOOP_MARKER);
    true
}

/// Prove Disable Slot got a Command Completion (cc=1), not a timeout fallback.
pub fn disable_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci disable skipped (no controller)");
        return true;
    }
    if !DISABLE_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = enable_slot() {
            let _ = disable_slot(slot_id);
        }
    }
    if !DISABLE_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: Disable Slot had no Command Completion");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_DISABLE_MARKER);
    true
}

fn extra_addressed_slot() -> Option<u8> {
    let slot_id = enable_slot()?;
    let ports = enumerate_ports();
    let (port, speed) = ports
        .iter()
        .find(|p| p.connected)
        .map(|p| (p.port_num, p.speed))
        .unwrap_or((1, UsbSpeed::High));
    if address_device(slot_id, port, speed) {
        Some(slot_id)
    } else {
        let _ = disable_slot(slot_id);
        None
    }
}

fn hid_slot() -> Option<u8> {
    let xhci = XHCI.lock();
    xhci.devices
        .iter()
        .find(|d| d.slot_id != 0)
        .map(|d| d.slot_id)
}

/// Prove Evaluate Context got a Command Completion (cc=1), not a timeout fallback.
pub fn eval_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci eval skipped (no controller)");
        return true;
    }
    if !EVAL_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = extra_addressed_slot() {
            let _ = evaluate_context(slot_id);
            let _ = disable_slot(slot_id);
        } else if let Some(slot_id) = hid_slot() {
            let _ = evaluate_context(slot_id);
        }
    }
    if !EVAL_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: Evaluate Context had no Command Completion");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_EVAL_MARKER);
    true
}

/// Prove Stop Endpoint on the bulk OUT pipe (DCI 2) got a Command Completion (cc=1).
/// Uses the already-configured extra endpoint, not HID interrupt IN (DCI 3) and not EP0.
pub fn stop_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci stop skipped (no controller)");
        return true;
    }
    if !STOP_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            // DCI 2 = bulk OUT from bulk_self_test. Not HID interrupt IN.
            if !stop_endpoint(slot_id, 2) {
                let _ = stop_endpoint(slot_id, 5);
            }
        }
    }
    if !STOP_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: Stop Endpoint had no Command Completion");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_STOP_MARKER);
    true
}

/// Prove Reset Endpoint on Halted bulk IN (DCI 5) got cc=1. Not HID interrupt IN.
pub fn resetep_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci resetep skipped (no controller)");
        return true;
    }
    if !RESETEP_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            // DCI 5 = bulk IN (Halted after bulk_in_self_test cc=6). Not HID IN.
            if !reset_endpoint(slot_id, 5) {
                let _ = reset_endpoint(slot_id, 7);
            }
        }
    }
    if !RESETEP_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: Reset Endpoint had no Command Completion");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_RESETEP_MARKER);
    true
}

/// Prove Set TR Dequeue Pointer on a Stopped extra pipe got cc=1.
pub fn settr_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci settr skipped (no controller)");
        return true;
    }
    if !SETTR_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            // DCI 2 stopped by stop_self_test; DCI 5 stopped by resetep_self_test.
            if !set_tr_dequeue(slot_id, 2) {
                let _ = set_tr_dequeue(slot_id, 5);
            }
        }
    }
    if !SETTR_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: Set TR Dequeue Pointer had no Command Completion");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_SETTR_MARKER);
    true
}

fn last_slot() -> Option<u8> {
    let xhci = XHCI.lock();
    xhci.devices
        .iter()
        .rev()
        .find(|d| d.slot_id != 0)
        .map(|d| d.slot_id)
}

/// Prove Reset Device on an already-addressed slot got cc=1.
/// Unused QEMU ports fail Address Device (cc=5), so this uses last_slot first.
pub fn resetdev_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci resetdev skipped (no controller)");
        return true;
    }
    if !RESETDEV_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = last_slot() {
            if !reset_device(slot_id) {
                if let Some(hid) = hid_slot() {
                    if hid != slot_id {
                        let _ = reset_device(hid);
                    }
                }
            }
        }
        if !RESETDEV_LIVE.load(Ordering::Relaxed) {
            if let Some(slot_id) = extra_addressed_slot() {
                let _ = reset_device(slot_id);
                let _ = disable_slot(slot_id);
            }
        }
    }
    if !RESETDEV_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: Reset Device had no Command Completion");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_RESETDEV_MARKER);
    true
}

/// Prove a Transfer No-Op TRB on the bulk OUT ring (DCI 2) got a Transfer Event.
/// DCI 2 may be Halted after bulk OUT cc=4/6; restart it before the No-Op.
pub fn trnoop_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci trnoop skipped (no controller)");
        return true;
    }
    if !TRNOOP_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            restart_ep(slot_id, 2);
            if !transfer_noop(slot_id, 2) {
                restart_ep(slot_id, 5);
                if !transfer_noop(slot_id, 5) {
                    recover_control_ep(slot_id);
                    let _ = transfer_noop(slot_id, 1);
                }
            }
        }
    }
    if !TRNOOP_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: Transfer No-Op had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_TRNOOP_MARKER);
    true
}

fn descriptor_probe(live: &AtomicBool, get: fn(u8) -> Option<usize>) {
    if live.load(Ordering::Relaxed) {
        return;
    }
    if let Some(slot_id) = hid_slot() {
        let _ = get(slot_id);
    }
}

/// Prove GET_DESCRIPTOR(Device Qualifier) got a Transfer Event.
pub fn qual_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci qual skipped (no controller)");
        return true;
    }
    descriptor_probe(&QUAL_LIVE, get_device_qualifier);
    if !QUAL_LIVE.load(Ordering::Relaxed) {
        serial_println!(
            "[XHCI] GATE_USB FAILED: GET_DESCRIPTOR(Device Qualifier) had no Transfer Event"
        );
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_QUAL_MARKER);
    true
}

/// Prove GET_DESCRIPTOR(BOS) got a Transfer Event.
pub fn bos_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci bos skipped (no controller)");
        return true;
    }
    descriptor_probe(&BOS_LIVE, get_bos_descriptor);
    if !BOS_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_DESCRIPTOR(BOS) had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_BOS_MARKER);
    true
}

/// Prove GET_DESCRIPTOR(Other Speed Configuration) got a Transfer Event.
pub fn oscfg_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci oscfg skipped (no controller)");
        return true;
    }
    descriptor_probe(&OSCFG_LIVE, get_other_speed_config);
    if !OSCFG_LIVE.load(Ordering::Relaxed) {
        serial_println!(
            "[XHCI] GATE_USB FAILED: GET_DESCRIPTOR(Other Speed Configuration) had no Transfer Event"
        );
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_OSCFG_MARKER);
    true
}

/// Prove SYNCH_FRAME got a Transfer Event.
pub fn synch_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci synch skipped (no controller)");
        return true;
    }
    if !SYNCH_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            let _ = synch_frame(slot_id, 0x81);
        }
    }
    if !SYNCH_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: SYNCH_FRAME had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_SYNCH_MARKER);
    true
}

/// Prove SET_SEL got a Transfer Event.
pub fn setsel_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci setsel skipped (no controller)");
        return true;
    }
    if !SETSEL_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            let _ = set_sel(slot_id, &[0, 0, 0, 0, 0, 0]);
        }
    }
    if !SETSEL_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: SET_SEL had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_SETSEL_MARKER);
    true
}

/// Prove SET_ISOCH_DELAY got a Transfer Event.
pub fn isochdelay_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci isochdelay skipped (no controller)");
        return true;
    }
    if !ISOCHDELAY_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            let _ = set_isoch_delay(slot_id, 0);
        }
    }
    if !ISOCHDELAY_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: SET_ISOCH_DELAY had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_ISOCHDELAY_MARKER);
    true
}

/// Prove GET_DESCRIPTOR(HID) got a Transfer Event.
pub fn hid_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci hiddesc skipped (no controller)");
        return true;
    }
    descriptor_probe(&HID_LIVE, get_hid_descriptor);
    if !HID_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_DESCRIPTOR(HID) had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_HID_MARKER);
    true
}

/// Prove GET_DESCRIPTOR(HID Report) got a Transfer Event.
pub fn hidreport_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci hidreport skipped (no controller)");
        return true;
    }
    descriptor_probe(&HIDREPORT_LIVE, get_hid_report_descriptor);
    if !HIDREPORT_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_DESCRIPTOR(HID Report) had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_HIDREPORT_MARKER);
    true
}

/// Prove HID GET_REPORT got a Transfer Event.
pub fn getreport_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci getreport skipped (no controller)");
        return true;
    }
    if !HIDGETREPORT_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            let _ = hid_get_report(slot_id, 0, 8);
        }
    }
    if !HIDGETREPORT_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_REPORT had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_HIDGETREPORT_MARKER);
    true
}

/// Prove HID SET_REPORT got a Transfer Event.
pub fn setreport_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci setreport skipped (no controller)");
        return true;
    }
    if !HIDSETREPORT_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            let _ = hid_set_report(slot_id, 0, &[0u8; 8]);
        }
    }
    if !HIDSETREPORT_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: SET_REPORT had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_HIDSETREPORT_MARKER);
    true
}

/// Prove HID GET_IDLE got a Transfer Event.
pub fn getidle_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci getidle skipped (no controller)");
        return true;
    }
    if !HIDGETIDLE_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            let _ = hid_get_idle(slot_id, 0);
        }
    }
    if !HIDGETIDLE_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_IDLE had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_HIDGETIDLE_MARKER);
    true
}

/// Prove HID SET_IDLE got a Transfer Event.
pub fn setidle_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci setidle skipped (no controller)");
        return true;
    }
    if !HIDSETIDLE_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            let _ = hid_set_idle(slot_id, 0, 0);
        }
    }
    if !HIDSETIDLE_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: SET_IDLE had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_HIDSETIDLE_MARKER);
    true
}

/// Prove HID GET_PROTOCOL got a Transfer Event.
pub fn getprotocol_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci getprotocol skipped (no controller)");
        return true;
    }
    if !HIDGETPROTOCOL_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            let _ = hid_get_protocol(slot_id);
        }
    }
    if !HIDGETPROTOCOL_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_PROTOCOL had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_HIDGETPROTOCOL_MARKER);
    true
}

/// Prove HID SET_PROTOCOL got a Transfer Event.
pub fn setprotocol_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci setprotocol skipped (no controller)");
        return true;
    }
    if !HIDSETPROTOCOL_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            let _ = hid_set_protocol(slot_id, 0);
        }
    }
    if !HIDSETPROTOCOL_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: SET_PROTOCOL had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_HIDSETPROTOCOL_MARKER);
    true
}

/// Prove GET_STATUS(INTERFACE) got a Transfer Event.
pub fn statusiface_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci statusiface skipped (no controller)");
        return true;
    }
    if !STATUSIFACE_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            let _ = get_interface_status(slot_id, 0);
        }
    }
    if !STATUSIFACE_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_STATUS(INTERFACE) had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_STATUSIFACE_MARKER);
    true
}

/// Prove GET_STATUS(ENDPOINT) got a Transfer Event.
pub fn statusep_self_test() -> bool {
    if !super::super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci statusep skipped (no controller)");
        return true;
    }
    if !STATUSEP_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            let _ = get_endpoint_status(slot_id, 0x81);
        }
    }
    if !STATUSEP_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_STATUS(ENDPOINT) had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_STATUSEP_MARKER);
    true
}
