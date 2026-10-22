//! Ring 3 SHM / Wayland clients. Instruction bytes live in `userspace/apps/`.
use alloc::vec::Vec;

fn app_elf(bytes: &'static [u8]) -> Vec<u8> {
    bytes.to_vec()
}

pub fn display_client_elf_data() -> Vec<u8> {
    app_elf(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../userspace/apps/shm-clients/display.elf"
    )))
}

pub fn terminal_client_elf_data() -> Vec<u8> {
    app_elf(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../userspace/apps/shm-clients/terminal.elf"
    )))
}

pub fn launcher_client_elf_data() -> Vec<u8> {
    app_elf(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../userspace/apps/paint/paint.elf"
    )))
}

pub fn calculator_client_elf_data() -> Vec<u8> {
    app_elf(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../userspace/apps/calculator/calculator.elf"
    )))
}

pub fn files_client_elf_data() -> Vec<u8> {
    app_elf(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../userspace/apps/files/files.elf"
    )))
}

pub fn gpui_button_client_elf_data() -> Vec<u8> {
    app_elf(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../userspace/apps/shm-clients/gpui-button.elf"
    )))
}

pub fn gpui_calc_client_elf_data() -> Vec<u8> {
    app_elf(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../userspace/apps/shm-clients/gpui-calc.elf"
    )))
}

pub fn pty_terminal_client_elf_data() -> Vec<u8> {
    app_elf(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../userspace/apps/term/term.elf"
    )))
}

pub fn settings_client_elf_data() -> Vec<u8> {
    app_elf(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../userspace/apps/shm-clients/settings.elf"
    )))
}

pub fn taskmgr_client_elf_data() -> Vec<u8> {
    app_elf(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../userspace/apps/shm-clients/taskmgr.elf"
    )))
}

pub fn editor_client_elf_data() -> Vec<u8> {
    app_elf(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../userspace/apps/shm-clients/editor.elf"
    )))
}

pub fn browser_client_elf_data() -> Vec<u8> {
    app_elf(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../userspace/apps/shm-clients/browser.elf"
    )))
}

pub fn ai_client_elf_data() -> Vec<u8> {
    app_elf(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../userspace/apps/shm-clients/ai.elf"
    )))
}

pub fn wayland_unix_client_elf_data() -> Vec<u8> {
    app_elf(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../userspace/apps/shm-clients/wayland-unix.elf"
    )))
}

pub fn wayland_protocol_client_elf_data() -> Vec<u8> {
    app_elf(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../userspace/apps/shm-clients/wayland-protocol.elf"
    )))
}

pub fn wayland_server_elf_data() -> Vec<u8> {
    app_elf(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../userspace/apps/shm-clients/wayland-server.elf"
    )))
}

pub fn dbus_daemon_elf_data() -> Vec<u8> {
    app_elf(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../userspace/apps/shm-clients/dbus-daemon.elf"
    )))
}
