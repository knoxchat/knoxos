//! XDG Desktop Portal service (org.freedesktop.portal.Desktop)
//! Vivaldi uses this for file chooser, screen capture, settings

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

use super::protocol::{DbusValue, Message};
use super::state::ConnectionId;

const SERVICE_NAME: &str = "org.freedesktop.portal.Desktop";
const OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";

/// Portal request handle
#[derive(Debug, Clone)]
pub struct PortalRequest {
    pub handle: String,
    pub sender: ConnectionId,
    pub interface: String,
    pub completed: bool,
    pub response: u32, // 0 = success, 1 = cancelled, 2 = error
    pub results: BTreeMap<String, DbusValue>,
}

lazy_static::lazy_static! {
    static ref REQUESTS: Mutex<BTreeMap<String, PortalRequest>> =
        Mutex::new(BTreeMap::new());
    static ref LAST_FILE_URIS: Mutex<Vec<String>> = Mutex::new(Vec::new());
}

static NEXT_REQUEST: AtomicU32 = AtomicU32::new(1);

/// Handle FileChooser.OpenFile
pub fn open_file(msg: &Message) -> Message {
    let req_id = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed);
    let handle = format!("/org/freedesktop/portal/desktop/request/{}", req_id);
    let uri = String::from("file:///home/user/Documents/upload.txt");

    serial_println!("[dbus:portal] FileChooser.OpenFile -> {} ({})", handle, uri);

    let mut results = BTreeMap::new();
    results.insert(
        String::from("uris"),
        DbusValue::Array(vec![DbusValue::String(uri.clone())]),
    );
    REQUESTS.lock().insert(
        handle.clone(),
        PortalRequest {
            handle: handle.clone(),
            sender: 0,
            interface: String::from("org.freedesktop.portal.FileChooser"),
            completed: true,
            response: 0,
            results,
        },
    );
    LAST_FILE_URIS.lock().push(uri);

    let mut reply = Message::method_return(msg.header.serial);
    reply.body = vec![DbusValue::ObjectPath(handle)];
    reply.signature = Some(String::from("o"));
    reply
}

/// URIs from the last FileChooser.OpenFile (Chromium `<input type=file>`).
pub fn last_file_uris() -> Vec<String> {
    LAST_FILE_URIS.lock().clone()
}

/// Handle ScreenCast.CreateSession
pub fn create_session(msg: &Message) -> Message {
    let req_id = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed);
    let handle = format!("/org/freedesktop/portal/desktop/session/{}", req_id);
    serial_println!("[dbus:portal] ScreenCast.CreateSession -> {}", handle);
    let mut reply = Message::method_return(msg.header.serial);
    reply.body = vec![DbusValue::ObjectPath(handle)];
    reply.signature = Some(String::from("o"));
    reply
}

/// Handle Secret.RetrieveSecret
pub fn retrieve_secret(msg: &Message) -> Message {
    serial_println!("[dbus:portal] Secret.RetrieveSecret");
    let mut reply = Message::method_return(msg.header.serial);
    reply.body = vec![DbusValue::Variant(alloc::boxed::Box::new(
        DbusValue::String(String::from("knoxos-secret")),
    ))];
    reply.signature = Some(String::from("v"));
    reply
}

/// Handle FileChooser.SaveFile
pub fn save_file(msg: &Message) -> Message {
    let req_id = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed);
    let handle = format!("/org/freedesktop/portal/desktop/request/{}", req_id);

    serial_println!("[dbus:portal] FileChooser.SaveFile -> {}", handle);

    let mut reply = Message::method_return(msg.header.serial);
    reply.body = vec![DbusValue::ObjectPath(handle)];
    reply.signature = Some(String::from("o"));
    reply
}

/// Handle Settings.Read
pub fn read_setting(msg: &Message) -> Message {
    let namespace = match msg.body.first() {
        Some(DbusValue::String(s)) => s.as_str(),
        _ => "",
    };
    let key = match msg.body.get(1) {
        Some(DbusValue::String(s)) => s.as_str(),
        _ => "",
    };

    serial_println!("[dbus:portal] Settings.Read {}.{}", namespace, key);

    let value = match (namespace, key) {
        ("org.freedesktop.appearance", "color-scheme") => {
            // 0 = no preference, 1 = dark, 2 = light
            DbusValue::Variant(alloc::boxed::Box::new(DbusValue::Uint32(0)))
        }
        ("org.gnome.desktop.interface", "gtk-theme") => DbusValue::Variant(alloc::boxed::Box::new(
            DbusValue::String(String::from("KnoxOS-Default")),
        )),
        ("org.gnome.desktop.interface", "icon-theme") => DbusValue::Variant(
            alloc::boxed::Box::new(DbusValue::String(String::from("KnoxOS"))),
        ),
        ("org.gnome.desktop.interface", "cursor-theme") => DbusValue::Variant(
            alloc::boxed::Box::new(DbusValue::String(String::from("default"))),
        ),
        ("org.gnome.desktop.interface", "font-name") => DbusValue::Variant(alloc::boxed::Box::new(
            DbusValue::String(String::from("Cantarell 11")),
        )),
        _ => DbusValue::Variant(alloc::boxed::Box::new(DbusValue::String(String::new()))),
    };

    let mut reply = Message::method_return(msg.header.serial);
    reply.body = vec![value];
    reply.signature = Some(String::from("v"));
    reply
}

pub fn init() {
    serial_println!(
        "[dbus] Registered portal: {} at {}",
        SERVICE_NAME,
        OBJECT_PATH
    );
}
