//! Built-in notification service (org.freedesktop.Notifications)
//! Vivaldi uses this for desktop notifications

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::serial_println;

use super::protocol::{DbusValue, Message};

const SERVICE_NAME: &str = "org.freedesktop.Notifications";
const OBJECT_PATH: &str = "/org/freedesktop/Notifications";

static NEXT_NOTIFICATION_ID: AtomicU32 = AtomicU32::new(1);

/// Notification urgency levels
#[derive(Debug, Clone, Copy)]
pub enum Urgency {
    Low = 0,
    Normal = 1,
    Critical = 2,
}

/// A desktop notification
#[derive(Debug, Clone)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub replaces_id: u32,
    pub icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<String>,
    pub urgency: Urgency,
    pub expire_timeout: i32, // ms, -1 = server default, 0 = never
}

/// Handle a Notify method call
pub fn handle_notify(msg: &Message) -> Message {
    let id = NEXT_NOTIFICATION_ID.fetch_add(1, Ordering::Relaxed);

    // Extract notification fields from body
    let app_name = match msg.body.first() {
        Some(DbusValue::String(s)) => s.clone(),
        _ => String::from("unknown"),
    };

    let summary = match msg.body.get(3) {
        Some(DbusValue::String(s)) => s.clone(),
        _ => String::from("Notification"),
    };

    let body = match msg.body.get(4) {
        Some(DbusValue::String(s)) => s.clone(),
        _ => String::new(),
    };

    serial_println!(
        "[dbus:notify] #{}: [{}] {} - {}",
        id,
        app_name,
        summary,
        body
    );

    crate::gui::notifications::app_info(&app_name, &summary, &body);

    // Return notification ID
    let mut reply = Message::method_return(msg.header.serial);
    reply.body = vec![DbusValue::Uint32(id)];
    reply.signature = Some(String::from("u"));
    reply
}

/// Get server capabilities
pub fn get_capabilities() -> Vec<String> {
    vec![
        String::from("body"),
        String::from("body-markup"),
        String::from("actions"),
        String::from("icon-static"),
        String::from("persistence"),
    ]
}

/// Get server information
pub fn get_server_info() -> (String, String, String, String) {
    (
        String::from("KnoxOS Notification Daemon"),
        String::from("KnoxOS"),
        String::from("0.1.0"),
        String::from("1.2"), // notification spec version
    )
}

pub fn init() {
    serial_println!(
        "[dbus] Registered service: {} at {}",
        SERVICE_NAME,
        OBJECT_PATH
    );
}
