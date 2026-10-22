//! Power management service (org.freedesktop.UPower)

use alloc::collections::BTreeMap;
use alloc::string::String;

use crate::serial_println;

use super::protocol::DbusValue;

pub fn get_display_device() -> BTreeMap<String, DbusValue> {
    let mut props = BTreeMap::new();
    props.insert(
        String::from("Type"),
        DbusValue::Uint32(2), // Battery
    );
    props.insert(
        String::from("State"),
        DbusValue::Uint32(2), // Discharging
    );
    props.insert(
        String::from("Percentage"),
        DbusValue::Double(100_u64), // 100% (as bits)
    );
    props.insert(String::from("IsPresent"), DbusValue::Boolean(true));
    props.insert(String::from("OnBattery"), DbusValue::Boolean(false));
    props
}

pub fn init() {
    serial_println!("[dbus] Registered service: org.freedesktop.UPower");
}
