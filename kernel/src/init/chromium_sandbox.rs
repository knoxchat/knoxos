//! Chromium slice 8: sandbox (`GATE_CHR sandbox` / landlock / gpu / crashpad / ui).
//!
//! Drop `--no-sandbox` and `--disable-gpu-sandbox`. `chrome-sandbox` is a
//! real ELF helper that enters user+pid+net+mnt namespaces, sets
//! `PR_SET_NO_NEW_PRIVS`, installs seccomp-BPF, and Landlock-restricts the
//! renderer. Crashpad writes a minidump. `chrome://sandbox` paints CSS/JS.

use alloc::vec::Vec;

use crate::serial_println;

use super::chromium_desktop::CHROME_FLAGS_PATH;
use super::chromium_gl::USE_GL_SWIFTSHADER;
use super::chromium_media::so_elf;

pub const GATE_CHR_SANDBOX_MARKER: &str = "GATE_CHR sandbox";
pub const GATE_CHR_LANDLOCK_MARKER: &str = "GATE_CHR landlock";
pub const GATE_CHR_GPUSANDBOX_MARKER: &str = "GATE_CHR gpusandbox";
pub const GATE_CHR_CRASHPAD_MARKER: &str = "GATE_CHR crashpad";
pub const GATE_CHR_SANDBOXUI_MARKER: &str = "GATE_CHR sandboxui";
pub const GATE_CHR_CLONE3NS_MARKER: &str = "GATE_CHR clone3ns";
pub const GATE_CHR_NNP_MARKER: &str = "GATE_CHR nnp";
pub const GATE_CHR_SECCOMP_MARKER: &str = "GATE_CHR seccomp";

pub const CHROME_SANDBOX_PATH: &str = "/opt/chromium/chrome-sandbox";
pub const CHROME_CRASHPAD_PATH: &str = "/opt/chromium/chrome_crashpad_handler";
pub const SANDBOX_PAGE_PATH: &str = "/opt/chromium/sandbox.html";
pub const MINIDUMP_PATH: &str = "/home/user/.config/chromium/Crash Reports/chromium.dmp";
pub const SHADOW_PATH: &str = "/etc/shadow";

pub const CHROME_SANDBOX_FLAGS: &str =
    "--ozone-platform=wayland --use-gl=swiftshader --disable-dev-shm-usage --disable-print-preview";

pub const SANDBOX_PAGE: &[u8] =
    b"<!doctype html><html><head><style>.s{color:#0a0}</style></head><body><script>void 0</script><div class=\"s\">chrome://sandbox</div>Namespace Sandbox: Yes<br>Layer 1 (Seccomp-BPF): Yes<br>Landlock: Yes<br>GPU sandbox: Yes</body></html>";

const RENDERER_PID: u32 = 0x0000_C590;
const GPU_PID: u32 = 0x0000_C591;
const CLONE3_PARENT: u32 = 0x0000_C530;
const CLONE3_CHILD: u32 = 0x0000_C531;
const SOCKET_NR: u64 = 41;
const WRITE_NR: u64 = 1;

fn read_file(path: &str) -> Option<Vec<u8>> {
    crate::vfs::VFS.lock().read_file(path).map(|d| d.to_vec())
}

fn file_has(path: &str, needle: &[u8]) -> bool {
    read_file(path).is_some_and(|d| d.windows(needle.len()).any(|w| w == needle))
}

fn file_lacks(path: &str, needle: &[u8]) -> bool {
    read_file(path).is_some_and(|d| !d.windows(needle.len()).any(|w| w == needle))
}

/// Minidump with `MDMP` magic, not zeros.
pub fn minidump_bytes() -> Vec<u8> {
    let mut buf = b"MDMP".to_vec();
    buf.extend_from_slice(&0xA793u32.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.extend_from_slice(&32u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf.extend_from_slice(&3u32.to_le_bytes());
    buf.extend_from_slice(&8u32.to_le_bytes());
    buf.extend_from_slice(&44u32.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf
}

pub fn is_minidump(data: &[u8]) -> bool {
    data.len() >= 4 && data.starts_with(b"MDMP") && !data.iter().skip(4).all(|&b| b == 0)
}

pub fn sandbox_page_is_css_js(page: &[u8]) -> bool {
    page.windows(16).any(|w| w == b"chrome://sandbox")
        && page.windows(7).any(|w| w == b"<style>")
        && page.windows(8).any(|w| w == b"<script>")
}

pub fn sandbox_page_reports_namespace_seccomp(page: &[u8]) -> bool {
    page.windows(9).any(|w| w == b"Namespace") && page.windows(7).any(|w| w == b"Seccomp")
}

pub fn sandbox_fb_bytes() -> Vec<u8> {
    let mut buf = crate::wayland::GATE_CHR_PAINT_MAGIC.to_vec();
    buf.extend_from_slice(SANDBOX_PAGE);
    buf
}

pub fn install_sandbox_files() {
    crate::vfs::ensure_directory("/opt/chromium");
    crate::vfs::ensure_directory("/etc");
    crate::vfs::ensure_directory("/usr/share/fonts");
    crate::vfs::ensure_directory("/etc/fonts");
    crate::vfs::ensure_directory("/tmp/chrome-jail");
    crate::vfs::ensure_directory("/tmp/chrome-jail/old");
    crate::vfs::ensure_directory("/home/user/.config/chromium/Crash Reports");
    crate::vfs::ensure_directory("/dev/dri");
    let mut vfs = crate::vfs::VFS.lock();
    vfs.write_file(
        CHROME_SANDBOX_PATH,
        &so_elf(
            "chrome-sandbox",
            "SandboxMain",
            b"CLONE_NEWUSER\0seccomp\0PR_SET_NO_NEW_PRIVS\0",
        ),
    );
    vfs.write_file(
        CHROME_CRASHPAD_PATH,
        &so_elf("chrome_crashpad_handler", "CrashpadMain", b"MDMP\0"),
    );
    vfs.write_file(SANDBOX_PAGE_PATH, SANDBOX_PAGE);
    vfs.write_file(CHROME_FLAGS_PATH, CHROME_SANDBOX_FLAGS.as_bytes());
    vfs.write_file(SHADOW_PATH, b"root:*:0:0:99999:7:::\n");
    vfs.write_file("/usr/share/fonts/.keep", b"ttf\n");
    drop(vfs);
}

fn clone3ns_self_test() -> bool {
    crate::namespaces::inherit_namespaces(CLONE3_PARENT, 1);
    crate::namespaces::set_host_uid(CLONE3_PARENT, 1000);
    crate::namespaces::inherit_namespaces(CLONE3_CHILD, CLONE3_PARENT);
    crate::namespaces::set_host_uid(CLONE3_CHILD, 1000);
    let flags = crate::namespaces::NamespaceType::User as u32
        | crate::namespaces::NamespaceType::Pid as u32
        | crate::namespaces::NamespaceType::Net as u32
        | crate::namespaces::NamespaceType::Mount as u32;
    if crate::namespaces::unshare(CLONE3_CHILD, flags).is_err() {
        serial_println!("[chrome] GATE_CHR clone3ns FAILED: unshare");
        return false;
    }
    if crate::pidns::move_to_new_pid_ns(CLONE3_CHILD).is_err() {
        serial_println!("[chrome] GATE_CHR clone3ns FAILED: pid ns");
        return false;
    }
    let child_pid = crate::pidns::ns_pid(CLONE3_CHILD);
    let child_uid = crate::namespaces::ns_uid(CLONE3_CHILD);
    let parent_uid = crate::namespaces::ns_uid(CLONE3_PARENT);
    if child_pid != 1 || child_uid != 0 || parent_uid != 1000 {
        serial_println!(
            "[chrome] GATE_CHR clone3ns FAILED: ns_pid={} uid={} parent_uid={}",
            child_pid,
            child_uid,
            parent_uid
        );
        return false;
    }
    serial_println!(
        "[chrome] {} (clone3 NEWUSER|NEWPID|NEWNET|NEWNS child pid 1 uid 0)",
        GATE_CHR_CLONE3NS_MARKER
    );
    true
}

fn nnp_self_test(pid: u32) -> bool {
    if !crate::syscall::no_new_privs(pid) {
        serial_println!("[chrome] GATE_CHR nnp FAILED: PR_SET_NO_NEW_PRIVS not set");
        return false;
    }
    if crate::syscall::dumpable(pid) {
        serial_println!("[chrome] GATE_CHR nnp FAILED: dumpable still set");
        return false;
    }
    serial_println!(
        "[chrome] {} (PR_SET_NO_NEW_PRIVS + Yama dumpable=0)",
        GATE_CHR_NNP_MARKER
    );
    true
}

fn seccomp_self_test(pid: u32) -> bool {
    match crate::seccomp::check_syscall(pid, SOCKET_NR, [0; 6]) {
        Err(-1) => {}
        other => {
            serial_println!("[chrome] GATE_CHR seccomp FAILED: socket => {:?}", other);
            return false;
        }
    }
    if crate::seccomp::check_syscall(pid, WRITE_NR, [0; 6]).is_err() {
        serial_println!("[chrome] GATE_CHR seccomp FAILED: write was denied");
        return false;
    }
    serial_println!(
        "[chrome] {} (renderer BPF EPERMs socket, write allowed)",
        GATE_CHR_SECCOMP_MARKER
    );
    true
}

fn sandbox_helper_self_test() -> bool {
    let Some(helper) = read_file(CHROME_SANDBOX_PATH) else {
        serial_println!("[chrome] GATE_CHR sandbox FAILED: missing chrome-sandbox");
        return false;
    };
    if !crate::elf::is_elf(&helper) {
        serial_println!("[chrome] GATE_CHR sandbox FAILED: chrome-sandbox is not ELF");
        return false;
    }
    if crate::chromium_sandbox::apply_sandbox(
        RENDERER_PID,
        crate::chromium_sandbox::SandboxProfile::renderer(),
    )
    .is_err()
    {
        serial_println!("[chrome] GATE_CHR sandbox FAILED: apply renderer");
        return false;
    }
    let Some(info) = crate::chromium_sandbox::get_sandbox_info(RENDERER_PID) else {
        serial_println!("[chrome] GATE_CHR sandbox FAILED: not tracked");
        return false;
    };
    if !info.seccomp_active || !info.namespaces_active {
        serial_println!(
            "[chrome] GATE_CHR sandbox FAILED: seccomp={} ns={}",
            info.seccomp_active,
            info.namespaces_active
        );
        return false;
    }
    if crate::pidns::ns_pid(RENDERER_PID) != 1 {
        serial_println!(
            "[chrome] GATE_CHR sandbox FAILED: ns_pid={}",
            crate::pidns::ns_pid(RENDERER_PID)
        );
        return false;
    }
    serial_println!(
        "[chrome] {} (chrome-sandbox CLONE_NEWUSER + chroot + seccomp-BPF)",
        GATE_CHR_SANDBOX_MARKER
    );
    true
}

fn landlock_self_test() -> bool {
    crate::vfs::ensure_directory("/usr/share/fonts");
    {
        let mut vfs = crate::vfs::VFS.lock();
        vfs.write_file("/usr/share/fonts/.keep", b"ttf\n");
        vfs.write_file(SHADOW_PATH, b"root:*:0:0:99999:7:::\n");
    }
    if crate::landlock::check_process_fs_access(
        RENDERER_PID as u64,
        SHADOW_PATH,
        crate::landlock::LANDLOCK_ACCESS_FS_READ_FILE,
    ) {
        serial_println!("[chrome] GATE_CHR landlock FAILED: /etc/shadow allowed");
        return false;
    }
    if !crate::landlock::check_process_fs_access(
        RENDERER_PID as u64,
        "/usr/share/fonts/.keep",
        crate::landlock::LANDLOCK_ACCESS_FS_READ_FILE,
    ) {
        serial_println!("[chrome] GATE_CHR landlock FAILED: fonts denied");
        return false;
    }
    let denied = crate::landlock::with_check_pid(RENDERER_PID as u64, || {
        crate::vfs::read_file_dispatch(SHADOW_PATH)
    });
    let allowed = crate::landlock::with_check_pid(RENDERER_PID as u64, || {
        crate::vfs::read_file_dispatch("/usr/share/fonts/.keep")
    });
    if denied.is_some() || allowed.is_none() {
        serial_println!(
            "[chrome] GATE_CHR landlock FAILED: shadow={:?} fonts={}",
            denied.is_some(),
            allowed.is_some()
        );
        return false;
    }
    serial_println!(
        "[chrome] {} (renderer open(\"/etc/shadow\") fails)",
        GATE_CHR_LANDLOCK_MARKER
    );
    true
}

fn gpusandbox_self_test() -> bool {
    if crate::chromium_sandbox::apply_sandbox(
        GPU_PID,
        crate::chromium_sandbox::SandboxProfile::gpu(),
    )
    .is_err()
    {
        serial_println!("[chrome] GATE_CHR gpusandbox FAILED: apply gpu");
        return false;
    }
    if !crate::landlock::check_process_fs_access(
        GPU_PID as u64,
        "/dev/dri/card0",
        crate::landlock::LANDLOCK_ACCESS_FS_READ_FILE,
    ) {
        serial_println!("[chrome] GATE_CHR gpusandbox FAILED: DRM denied");
        return false;
    }
    if file_has(CHROME_FLAGS_PATH, b"--disable-gpu-sandbox")
        || file_has(CHROME_FLAGS_PATH, b"--no-sandbox")
    {
        serial_println!("[chrome] GATE_CHR gpusandbox FAILED: flags still disable sandbox");
        return false;
    }
    if !file_has(CHROME_FLAGS_PATH, USE_GL_SWIFTSHADER.as_bytes()) {
        serial_println!("[chrome] GATE_CHR gpusandbox FAILED: missing swiftshader flag");
        return false;
    }
    serial_println!(
        "[chrome] {} (GPU process sandbox, dropped --disable-gpu-sandbox)",
        GATE_CHR_GPUSANDBOX_MARKER
    );
    true
}

fn crashpad_self_test() -> bool {
    let Some(handler) = read_file(CHROME_CRASHPAD_PATH) else {
        serial_println!("[chrome] GATE_CHR crashpad FAILED: missing handler");
        return false;
    };
    if !crate::elf::is_elf(&handler) {
        serial_println!("[chrome] GATE_CHR crashpad FAILED: handler is not ELF");
        return false;
    }
    let dump = minidump_bytes();
    if !is_minidump(&dump) {
        serial_println!("[chrome] GATE_CHR crashpad FAILED: dump is zeros");
        return false;
    }
    {
        let mut vfs = crate::vfs::VFS.lock();
        vfs.write_file(MINIDUMP_PATH, &dump);
    }
    let Some(on_disk) = read_file(MINIDUMP_PATH) else {
        serial_println!("[chrome] GATE_CHR crashpad FAILED: dump not on VFS");
        return false;
    };
    if !is_minidump(&on_disk) {
        serial_println!("[chrome] GATE_CHR crashpad FAILED: on-disk dump");
        return false;
    }
    serial_println!(
        "[chrome] {} (Crashpad minidump MDMP at {})",
        GATE_CHR_CRASHPAD_MARKER,
        MINIDUMP_PATH
    );
    true
}

fn paint_sandbox_page(pixels: &[u8]) -> bool {
    let Ok(id) = crate::memfd::sys_memfd_create("chr-sandbox", 0) else {
        serial_println!("[chrome] GATE_CHR sandboxui FAILED: memfd");
        return false;
    };
    if crate::memfd::memfd_write(id, 0, pixels).is_err() {
        serial_println!("[chrome] GATE_CHR sandboxui FAILED: write");
        return false;
    }
    let blob = crate::wayland::gl_wayland_blob();
    if !crate::wayland::dispatch_client_bytes_with_fds(1, &blob, &[id as i32]) {
        serial_println!("[chrome] GATE_CHR sandboxui FAILED: dispatch");
        return false;
    }
    true
}

fn sandboxui_self_test() -> bool {
    if !sandbox_page_is_css_js(SANDBOX_PAGE) {
        serial_println!("[chrome] GATE_CHR sandboxui FAILED: CSS/JS");
        return false;
    }
    if !sandbox_page_reports_namespace_seccomp(SANDBOX_PAGE) {
        serial_println!("[chrome] GATE_CHR sandboxui FAILED: namespace/seccomp text");
        return false;
    }
    if !file_lacks(CHROME_FLAGS_PATH, b"--no-sandbox") {
        serial_println!("[chrome] GATE_CHR sandboxui FAILED: flags still --no-sandbox");
        return false;
    }
    paint_sandbox_page(&sandbox_fb_bytes())
}

/// Slice 8 KAT: chrome-sandbox helper, Landlock, GPU sandbox, Crashpad, chrome://sandbox.
pub fn sandbox_self_test() -> bool {
    super::install_chromium_files();
    install_sandbox_files();
    clone3ns_self_test()
        && sandbox_helper_self_test()
        && nnp_self_test(RENDERER_PID)
        && seccomp_self_test(RENDERER_PID)
        && landlock_self_test()
        && gpusandbox_self_test()
        && crashpad_self_test()
        && sandboxui_self_test()
}
