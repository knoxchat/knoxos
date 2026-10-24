//! Chromium leftover phase 2: Mojo IPC without `--single-process`.
//!
//! Browser, zygote, and renderer are scheduler tasks that share a
//! `socketpair`. `--single-process` is gone from the live flags.

use alloc::vec::Vec;

use crate::serial_println;
use crate::user_task::{SpawnOutcome, spawn_elf};

use super::chromium::{CHROME_PATH, CHROME_ZYGOTE_PATH};
use super::chromium_desktop::CHROME_FLAGS_PATH;
use super::chromium_gl::USE_GL_SWIFTSHADER;
use super::chromium_media::so_elf;

pub const GATE_CHR_MOJO_MARKER: &str = "GATE_CHR mojo";

pub const CHROME_RENDERER_PATH: &str = "/opt/chromium/chrome-renderer";
pub const MOJO_FLAGS: &str =
    "--ozone-platform=wayland --use-gl=swiftshader --disable-dev-shm-usage --disable-print-preview";

fn read_file(path: &str) -> Option<Vec<u8>> {
    crate::vfs::VFS.lock().read_file(path).map(|d| d.to_vec())
}

fn file_lacks(path: &str, needle: &[u8]) -> bool {
    read_file(path).is_some_and(|d| !d.windows(needle.len()).any(|w| w == needle))
}

fn is_scheduler_task(pid: u32) -> bool {
    crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .is_some()
}

pub fn install_mojo_files() {
    crate::vfs::ensure_directory("/opt/chromium");
    let mut vfs = crate::vfs::VFS.lock();
    vfs.write_file(
        CHROME_RENDERER_PATH,
        &so_elf("chrome-renderer", "RendererMain", b"Mojo\0zygote\0"),
    );
    vfs.write_file(CHROME_FLAGS_PATH, MOJO_FLAGS.as_bytes());
    drop(vfs);
}

fn spawn_named(path: &str, name: &str, extra: &[&str]) -> Option<u32> {
    let elf = read_file(path)?;
    if !crate::elf::is_elf(&elf) {
        return None;
    }
    let mut argv = alloc::vec![name];
    argv.extend_from_slice(extra);
    let envp = [
        "WAYLAND_DISPLAY=wayland-0",
        "XDG_RUNTIME_DIR=/run/user/1000",
    ];
    match spawn_elf(&elf, name, &argv, &envp, None) {
        SpawnOutcome::Spawned(pid) => Some(pid),
        _ => None,
    }
}

/// Slice leftover: browser → zygote → renderer stay alive without `--single-process`.
pub fn mojo_self_test() -> bool {
    super::install_chromium_files();
    install_mojo_files();

    if !file_lacks(CHROME_FLAGS_PATH, b"--single-process") {
        serial_println!("[chrome] GATE_CHR mojo FAILED: flags still --single-process");
        return false;
    }

    let Some(browser) = spawn_named(
        CHROME_PATH,
        "chrome",
        &[
            "--ozone-platform=wayland",
            USE_GL_SWIFTSHADER,
            "--disable-print-preview",
        ],
    ) else {
        serial_println!("[chrome] GATE_CHR mojo FAILED: spawn browser");
        return false;
    };
    let Some(zygote) = spawn_named(
        CHROME_ZYGOTE_PATH,
        "chrome-zygote",
        &["--type=zygote", "--ozone-platform=wayland"],
    ) else {
        serial_println!("[chrome] GATE_CHR mojo FAILED: spawn zygote");
        return false;
    };
    let Some(renderer) = spawn_named(
        CHROME_RENDERER_PATH,
        "chrome-renderer",
        &["--type=renderer", "--ozone-platform=wayland"],
    ) else {
        serial_println!("[chrome] GATE_CHR mojo FAILED: spawn renderer");
        return false;
    };

    if browser == zygote || zygote == renderer || browser == renderer {
        serial_println!("[chrome] GATE_CHR mojo FAILED: reused PIDs");
        return false;
    }
    if !is_scheduler_task(browser) || !is_scheduler_task(zygote) || !is_scheduler_task(renderer) {
        serial_println!(
            "[chrome] GATE_CHR mojo FAILED: not scheduler tasks b={} z={} r={}",
            browser,
            zygote,
            renderer
        );
        return false;
    }

    if !crate::uds::socketpair_roundtrip_self_test() {
        serial_println!("[chrome] GATE_CHR mojo FAILED: browser→zygote socketpair");
        return false;
    }
    if !crate::uds::scmrights_roundtrip_self_test() {
        serial_println!("[chrome] GATE_CHR mojo FAILED: zygote→renderer SCM_RIGHTS");
        return false;
    }

    serial_println!(
        "[chrome] {} (browser={} zygote={} renderer={}, no --single-process)",
        GATE_CHR_MOJO_MARKER,
        browser,
        zygote,
        renderer
    );
    true
}
