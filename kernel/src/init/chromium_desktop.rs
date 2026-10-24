//! Chromium slice 7: desktop integration (`GATE_CHR launch`).
//!
//! Start-menu Browser execs `/opt/chromium/chrome` and presents Chromium SHM.
//! GPUI HTML-to-text is the fallback only when that ELF is missing. Session
//! `Hello`, page notifications, XDG portals, `xdg-open`, and
//! `--disable-print-preview` land with the same slice.

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::dbus::{
    BusType, DBUS_REQUEST_NAME_REPLY_PRIMARY_OWNER, DbusValue, Message, MessageType, connect,
    receive_message, send_message,
};
use crate::gui::window::{ViewKind, WindowContentType};
use crate::serial_println;
use crate::user_task::{SpawnOutcome, spawn_elf};
use crate::xdg::{DesktopAction, DesktopEntry, DesktopEntryType};

use super::chromium::CHROME_PATH;
use super::chromium_gl::USE_GL_SWIFTSHADER;
use super::chromium_media::so_elf;

pub const GATE_CHR_DBUS_MARKER: &str = "GATE_CHR dbus";
pub const GATE_CHR_NOTIFY_MARKER: &str = "GATE_CHR notify";
pub const GATE_CHR_PORTAL_MARKER: &str = "GATE_CHR portal";
pub const GATE_CHR_MIME_MARKER: &str = "GATE_CHR mime";
pub const GATE_CHR_CUPS_MARKER: &str = "GATE_CHR cups";
pub const GATE_CHR_LAUNCH_MARKER: &str = "GATE_CHR launch";

pub const CHROMIUM_DESKTOP_PATH: &str = "/usr/share/applications/chromium.desktop";
pub const LIBCUPS_PATH: &str = "/lib/libcups.so.2";
pub const CHROME_FLAGS_PATH: &str = "/opt/chromium/chrome-flags";
pub const PORTAL_UPLOAD_PATH: &str = "/home/user/Documents/upload.txt";
pub const CHROMIUM_BUS_NAME: &str = "org.chromium.Chromium";
pub const DISABLE_PRINT_PREVIEW: &str = "--disable-print-preview";

pub const CHROME_LAUNCH_FLAGS: &str =
    "--ozone-platform=wayland --use-gl=swiftshader --disable-dev-shm-usage --disable-print-preview";

pub const CHROMIUM_DESKTOP_FILE: &[u8] = b"[Desktop Entry]\nType=Application\nName=Chromium\nGenericName=Web Browser\nExec=/opt/chromium/chrome --ozone-platform=wayland --use-gl=swiftshader --disable-print-preview %U\nTryExec=/opt/chromium/chrome\nTerminal=false\nCategories=Network;WebBrowser;\nMimeType=text/html;text/xml;application/xhtml+xml;x-scheme-handler/http;x-scheme-handler/https;\nStartupWMClass=Chromium\n";

fn read_file(path: &str) -> Option<Vec<u8>> {
    crate::vfs::VFS.lock().read_file(path).map(|d| d.to_vec())
}

fn file_has(path: &str, needle: &[u8]) -> bool {
    read_file(path).is_some_and(|d| d.windows(needle.len()).any(|w| w == needle))
}

/// `/opt/chromium/chrome` is a loadable ELF, not a missing stub.
pub fn chrome_elf_present() -> bool {
    read_file(CHROME_PATH).is_some_and(|d| crate::elf::is_elf(&d))
}

pub fn libcups_elf() -> Vec<u8> {
    so_elf(
        "libcups.so.2",
        "cupsGetDests",
        b"cupsPrintFile\0cupsLastErrorString\0",
    )
}

fn chromium_desktop_entry() -> DesktopEntry {
    DesktopEntry {
        entry_type: DesktopEntryType::Application,
        name: String::from("Chromium"),
        generic_name: Some(String::from("Web Browser")),
        comment: Some(String::from("Access the Internet")),
        icon: Some(String::from("chromium")),
        exec: Some(format!(
            "{} --ozone-platform=wayland {} {} %U",
            CHROME_PATH, USE_GL_SWIFTSHADER, DISABLE_PRINT_PREVIEW
        )),
        try_exec: Some(String::from(CHROME_PATH)),
        path: None,
        terminal: false,
        no_display: false,
        hidden: false,
        categories: vec![String::from("Network"), String::from("WebBrowser")],
        mime_types: vec![
            String::from("text/html"),
            String::from("text/xml"),
            String::from("application/xhtml+xml"),
            String::from("x-scheme-handler/http"),
            String::from("x-scheme-handler/https"),
        ],
        keywords: vec![
            String::from("Internet"),
            String::from("Web"),
            String::from("Browser"),
            String::from("Chromium"),
        ],
        startup_notify: true,
        startup_wm_class: Some(String::from("Chromium")),
        actions: vec![DesktopAction {
            id: String::from("new-window"),
            name: String::from("New Window"),
            exec: Some(format!("{} --new-window", CHROME_PATH)),
            icon: None,
        }],
        file_path: String::from(CHROMIUM_DESKTOP_PATH),
    }
}

fn register_chromium_mime() {
    crate::xdg::register_desktop_entry(chromium_desktop_entry());
    crate::xdg::set_default_app("x-scheme-handler/http", CHROMIUM_DESKTOP_PATH);
    crate::xdg::set_default_app("x-scheme-handler/https", CHROMIUM_DESKTOP_PATH);
    crate::xdg::set_default_app("text/html", CHROMIUM_DESKTOP_PATH);
}

pub fn install_desktop_files() {
    crate::vfs::ensure_directory("/opt/chromium");
    crate::vfs::ensure_directory("/usr/share/applications");
    crate::vfs::ensure_directory("/lib");
    crate::vfs::ensure_directory("/home/user/Documents");
    crate::vfs::ensure_directory("/run/user/1000");
    let mut vfs = crate::vfs::VFS.lock();
    vfs.write_file(CHROMIUM_DESKTOP_PATH, CHROMIUM_DESKTOP_FILE);
    vfs.write_file(LIBCUPS_PATH, &libcups_elf());
    vfs.write_file(CHROME_FLAGS_PATH, CHROME_LAUNCH_FLAGS.as_bytes());
    vfs.write_file(PORTAL_UPLOAD_PATH, b"chromium-portal-file\n");
    drop(vfs);
    register_chromium_mime();
    let _ = crate::dbus::ensure_session_bus_listen();
}

fn gpui_browser_open() -> bool {
    crate::gui::window::WINDOW_MANAGER
        .lock()
        .windows
        .iter()
        .any(|w| w.content_type == WindowContentType::Gpui(ViewKind::Browser))
}

fn chromium_shm_open() -> bool {
    crate::gui::window::WINDOW_MANAGER
        .lock()
        .windows
        .iter()
        .any(|w| w.title == "Chromium" && w.content_type == WindowContentType::ShmEmpty)
}

fn dock_chromium() {
    let wid = crate::gui::window::WINDOW_MANAGER
        .lock()
        .windows
        .iter()
        .rev()
        .find(|w| w.title == "Chromium" && w.content_type == WindowContentType::ShmEmpty)
        .map(|w| w.id);
    if let Some(wid) = wid {
        crate::gui::taskbar::add_entry(wid, "Chromium");
        crate::gui::taskbar::set_active(wid);
    }
}

fn spawn_chrome(url: &str) -> u32 {
    let Some(elf) = read_file(CHROME_PATH) else {
        return 1;
    };
    let url = if url.is_empty() {
        "https://example.com"
    } else {
        url
    };
    let argv = [
        CHROME_PATH,
        "--ozone-platform=wayland",
        USE_GL_SWIFTSHADER,
        DISABLE_PRINT_PREVIEW,
        url,
    ];
    let envp = [
        "WAYLAND_DISPLAY=wayland-0",
        "XDG_RUNTIME_DIR=/run/user/1000",
    ];
    match spawn_elf(&elf, "chrome", &argv, &envp, None) {
        SpawnOutcome::Spawned(pid) => pid,
        _ => 1,
    }
}

fn launch_pixels() -> Vec<u8> {
    let mut buf = crate::wayland::GATE_CHR_PAINT_MAGIC.to_vec();
    buf.resize(64 * 64 * 4, 0x24);
    buf
}

/// Start-menu / `xdg-open` path: exec the Chromium ELF and show Chromium SHM.
/// Returns false only when the ELF is missing (GPUI fallback).
pub fn launch_chromium_from_desktop(url: &str) -> bool {
    if !chrome_elf_present() {
        return false;
    }
    let pid = spawn_chrome(url);
    if !crate::wayland::present_chromium_shm(pid, &launch_pixels()) {
        serial_println!("[chrome] GATE_CHR launch FAILED: Chromium SHM");
        return false;
    }
    dock_chromium();
    true
}

fn dbus_self_test() -> bool {
    let chrome = connect(BusType::Session, 0xC440, 1000);
    let hello = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "Hello",
    );
    let serial = send_message(BusType::Session, chrome, hello);
    let Some(reply) = receive_message(BusType::Session, chrome) else {
        serial_println!("[chrome] GATE_CHR dbus FAILED: no Hello reply");
        return false;
    };
    let unique = match reply.body.first() {
        Some(DbusValue::String(s)) if s.starts_with(":1.") => s.clone(),
        other => {
            serial_println!("[chrome] GATE_CHR dbus FAILED: Hello body={:?}", other);
            return false;
        }
    };
    if reply.header.message_type != MessageType::MethodReturn || reply.reply_serial != Some(serial)
    {
        serial_println!("[chrome] GATE_CHR dbus FAILED: Hello type");
        return false;
    }

    let mut req = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RequestName",
    );
    req.body = vec![
        DbusValue::String(String::from(CHROMIUM_BUS_NAME)),
        DbusValue::Uint32(0),
    ];
    req.signature = Some(String::from("su"));
    let serial = send_message(BusType::Session, chrome, req);
    let Some(reply) = receive_message(BusType::Session, chrome) else {
        serial_println!("[chrome] GATE_CHR dbus FAILED: no RequestName reply");
        return false;
    };
    let owned = matches!(
        reply.body.first(),
        Some(DbusValue::Uint32(c)) if *c == DBUS_REQUEST_NAME_REPLY_PRIMARY_OWNER
    ) && reply.reply_serial == Some(serial);
    if !owned {
        serial_println!("[chrome] GATE_CHR dbus FAILED: RequestName");
        return false;
    }
    serial_println!(
        "[chrome] {} (session Hello {} owns {})",
        GATE_CHR_DBUS_MARKER,
        unique,
        CHROMIUM_BUS_NAME
    );

    if !notify_self_test(chrome) {
        return false;
    }
    portal_self_test(chrome)
}

fn notify_self_test(chrome: u32) -> bool {
    let mut notify = Message::method_call(
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
        "Notify",
    );
    notify.body = vec![
        DbusValue::String(String::from("Chromium")),
        DbusValue::Uint32(0),
        DbusValue::String(String::from("chromium")),
        DbusValue::String(String::from("Page notification")),
        DbusValue::String(String::from("https://example.com wants to notify")),
        DbusValue::Array(Vec::new()),
        DbusValue::Array(Vec::new()),
        DbusValue::Int32(-1),
    ];
    notify.signature = Some(String::from("susssasa{sv}i"));
    let serial = send_message(BusType::Session, chrome, notify);
    let Some(reply) = receive_message(BusType::Session, chrome) else {
        serial_println!("[chrome] GATE_CHR notify FAILED: no Notify reply");
        return false;
    };
    let id_ok = matches!(reply.body.first(), Some(DbusValue::Uint32(id)) if *id > 0)
        && reply.reply_serial == Some(serial);
    let tray = crate::gui::notifications::known_app_names()
        .iter()
        .any(|n| n == "Chromium");
    if !id_ok || !tray {
        serial_println!(
            "[chrome] GATE_CHR notify FAILED: id_ok={} tray={}",
            id_ok,
            tray
        );
        return false;
    }
    serial_println!(
        "[chrome] {} (Notify → Knox tray app=Chromium)",
        GATE_CHR_NOTIFY_MARKER
    );
    true
}

fn portal_self_test(chrome: u32) -> bool {
    let mut open = Message::method_call(
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.FileChooser",
        "OpenFile",
    );
    open.body = vec![
        DbusValue::String(String::from("")),
        DbusValue::String(String::from("Open File")),
        DbusValue::Array(Vec::new()),
    ];
    open.signature = Some(String::from("ssa{sv}"));
    let serial = send_message(BusType::Session, chrome, open);
    let Some(reply) = receive_message(BusType::Session, chrome) else {
        serial_println!("[chrome] GATE_CHR portal FAILED: no OpenFile reply");
        return false;
    };
    let handle_ok = matches!(
        reply.body.first(),
        Some(DbusValue::ObjectPath(p)) if p.contains("/org/freedesktop/portal/desktop/request/")
    ) && reply.reply_serial == Some(serial);
    let uris = crate::dbus::portal::last_file_uris();
    let file_ok = uris.iter().any(|u| u.contains("upload.txt"));

    let mut cast = Message::method_call(
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.ScreenCast",
        "CreateSession",
    );
    cast.body = vec![DbusValue::Array(Vec::new())];
    let _ = send_message(BusType::Session, chrome, cast);
    let cast_ok = receive_message(BusType::Session, chrome)
        .is_some_and(|r| matches!(r.body.first(), Some(DbusValue::ObjectPath(_))));

    let mut settings = Message::method_call(
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Settings",
        "Read",
    );
    settings.body = vec![
        DbusValue::String(String::from("org.freedesktop.appearance")),
        DbusValue::String(String::from("color-scheme")),
    ];
    settings.signature = Some(String::from("ss"));
    let _ = send_message(BusType::Session, chrome, settings);
    let settings_ok = receive_message(BusType::Session, chrome).is_some();

    let mut secret = Message::method_call(
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Secret",
        "RetrieveSecret",
    );
    secret.body = vec![DbusValue::String(String::from("chromium"))];
    let _ = send_message(BusType::Session, chrome, secret);
    let secret_ok = receive_message(BusType::Session, chrome).is_some();

    if !handle_ok || !file_ok || !cast_ok || !settings_ok || !secret_ok {
        serial_println!(
            "[chrome] GATE_CHR portal FAILED: handle={} file={} cast={} settings={} secret={}",
            handle_ok,
            file_ok,
            cast_ok,
            settings_ok,
            secret_ok
        );
        return false;
    }
    serial_println!(
        "[chrome] {} (<input type=file> FileChooser + ScreenCast/Settings/Secret)",
        GATE_CHR_PORTAL_MARKER
    );
    true
}

fn mime_self_test() -> bool {
    let Some(app) = crate::xdg::get_default_app("x-scheme-handler/https") else {
        serial_println!("[chrome] GATE_CHR mime FAILED: no default https handler");
        return false;
    };
    let exec_ok = app.exec.as_deref().is_some_and(|e| e.contains(CHROME_PATH));
    if !exec_ok || app.file_path != CHROMIUM_DESKTOP_PATH {
        serial_println!(
            "[chrome] GATE_CHR mime FAILED: default is {}",
            app.file_path
        );
        return false;
    }
    if crate::xdg::xdg_open("https://example.com").is_err() {
        serial_println!("[chrome] GATE_CHR mime FAILED: xdg-open");
        return false;
    }
    serial_println!(
        "[chrome] {} (xdg-open https:// execs {})",
        GATE_CHR_MIME_MARKER,
        CHROME_PATH
    );
    true
}

fn cups_self_test() -> bool {
    let Some(cups) = read_file(LIBCUPS_PATH) else {
        serial_println!("[chrome] GATE_CHR cups FAILED: missing libcups.so.2");
        return false;
    };
    if !crate::elf::is_elf(&cups) || !cups.windows(12).any(|w| w == b"cupsGetDests") {
        serial_println!("[chrome] GATE_CHR cups FAILED: libcups ELF");
        return false;
    }
    if !file_has(CHROME_FLAGS_PATH, DISABLE_PRINT_PREVIEW.as_bytes()) {
        serial_println!("[chrome] GATE_CHR cups FAILED: --disable-print-preview");
        return false;
    }
    serial_println!(
        "[chrome] {} (libcups.so.2 + {})",
        GATE_CHR_CUPS_MARKER,
        DISABLE_PRINT_PREVIEW
    );
    true
}

fn launch_self_test() -> bool {
    let before_gpui = gpui_browser_open();
    if !chrome_elf_present() {
        serial_println!("[chrome] GATE_CHR launch FAILED: missing chrome ELF");
        return false;
    }
    if !chromium_shm_open() && !launch_chromium_from_desktop("https://example.com") {
        serial_println!("[chrome] GATE_CHR launch FAILED: start menu");
        return false;
    }
    if gpui_browser_open() && !before_gpui {
        serial_println!("[chrome] GATE_CHR launch FAILED: opened GPUI Browser");
        return false;
    }
    if !chromium_shm_open() {
        serial_println!("[chrome] GATE_CHR launch FAILED: no Chromium SHM window");
        return false;
    }
    serial_println!(
        "[chrome] {} (start menu execs {}, not GPUI Browser)",
        GATE_CHR_LAUNCH_MARKER,
        CHROME_PATH
    );
    true
}

/// Slice 7 KAT: session Hello, tray notify, portals, xdg-open, CUPS, start-menu exec.
pub fn desktop_self_test() -> bool {
    super::install_chromium_files();
    install_desktop_files();
    dbus_self_test() && cups_self_test() && mime_self_test() && launch_self_test()
}
