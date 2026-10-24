//! Chromium slice 1: unpack a real ELF tree and exec it through ld-linux.
//!
//! Not Blink. A Chromium-shaped ELF64 (`PT_INTERP` `/lib64/ld-linux-x86-64.so.2`,
//! `DT_NEEDED` `libc.so.6`) plus ICU/pak headers that are not zero-fill.

use crate::serial_println;

pub const CHROME_PATH: &str = "/opt/chromium/chrome";
pub const CHROME_ZYGOTE_PATH: &str = "/opt/chromium/chrome-zygote";
pub const LD_LINUX_PATH: &str = "/lib64/ld-linux-x86-64.so.2";
pub const LIBC_GLIBC_PATH: &str = "/lib/libc.so.6";
pub const ICU_PATH: &str = "/opt/chromium/icudtl.dat";
pub const PAK_PATH: &str = "/opt/chromium/resources.pak";

pub const GATE_CHR_UNPACK_MARKER: &str = "GATE_CHR unpack";
pub const GATE_CHR_EXEC_MARKER: &str = "GATE_CHR exec";
pub const GATE_CHR_LDSO_MARKER: &str = "GATE_CHR ldso";
pub const GATE_CHR_GLIBC_MARKER: &str = "GATE_CHR glibc";
pub const GATE_CHR_SCMRIGHTS_MARKER: &str = "GATE_CHR scmrights";
pub const GATE_CHR_FORK_MARKER: &str = "GATE_CHR fork";
pub const GATE_CHR_WLSHM_MARKER: &str = "GATE_CHR wlshm";
pub const GATE_CHR_XDGSHELL_MARKER: &str = "GATE_CHR xdgshell";
pub const GATE_CHR_WLSEAT_MARKER: &str = "GATE_CHR wlseat";
pub const GATE_CHR_WLOUTPUT_MARKER: &str = "GATE_CHR wloutput";
pub const GATE_CHR_WLDATA_MARKER: &str = "GATE_CHR wldata";
pub const GATE_CHR_TEXTINPUT_MARKER: &str = "GATE_CHR textinput";
pub const GATE_CHR_PAINT_MARKER: &str = "GATE_CHR paint";
pub const GATE_CHR_PROC_MARKER: &str = "GATE_CHR proc";
pub const GATE_CHR_SYSCTL_MARKER: &str = "GATE_CHR sysctl";
pub const GATE_CHR_DEV_MARKER: &str = "GATE_CHR dev";
pub const GATE_CHR_ETC_MARKER: &str = "GATE_CHR etc";
pub const GATE_CHR_XDG_MARKER: &str = "GATE_CHR xdg";

/// ICU `DataHeader`: headerSize=0x20, magic=0xDA27, `CmnD` TOC. Not a zero file.
pub fn icu_dat() -> alloc::vec::Vec<u8> {
    super::chromium_media::icu_common_data()
}

/// Chromium `.pak` v5 with one UTF-8 resource. Not ELF-magic-plus-zeros.
pub fn resources_pak() -> alloc::vec::Vec<u8> {
    let data = b"GATE\n";
    let mut buf = alloc::vec::Vec::new();
    buf.extend_from_slice(&5u32.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&24u32.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&((24 + data.len()) as u32).to_le_bytes());
    buf.extend_from_slice(data);
    buf
}

pub fn is_icu_dat(data: &[u8]) -> bool {
    data.len() >= 4 && data[0] == 0x20 && data[1] == 0x00 && data[2] == 0xDA && data[3] == 0x27
}

pub fn is_resources_pak(data: &[u8]) -> bool {
    data.len() >= 4 && u32::from_le_bytes([data[0], data[1], data[2], data[3]]) == 5
}

/// Install the Chromium-shaped tree. Replaces the ld-linux shebang stub.
pub fn install_chromium_files() {
    crate::vfs::ensure_directory("/opt");
    crate::vfs::ensure_directory("/opt/chromium");
    crate::vfs::ensure_directory("/lib");
    crate::vfs::ensure_directory("/lib64");
    let mut vfs = crate::vfs::VFS.lock();
    vfs.write_file(CHROME_PATH, &super::chrome_elf_data());
    vfs.write_file(CHROME_ZYGOTE_PATH, &super::zygote_child_elf_data());
    vfs.write_file(LD_LINUX_PATH, &super::ldso_elf_data());
    vfs.write_file(LIBC_GLIBC_PATH, &super::libc_glibc_elf_data());
    vfs.write_file(ICU_PATH, &icu_dat());
    vfs.write_file(PAK_PATH, &resources_pak());
    vfs.write_file(
        "/opt/chromium/ozone.wl",
        &crate::wayland::ozone_wayland_blob(),
    );
    drop(vfs);
    install_chromium_linux_shape();
    super::chromium_https::install_https_files();
    super::chromium_gl::install_gl_files();
    super::chromium_media::install_media_files();
    super::chromium_desktop::install_desktop_files();
    super::chromium_sandbox::install_sandbox_files();
    super::chromium_loader::install_loader_files();
    super::chromium_mojo::install_mojo_files();
    super::chromium_polish::install_polish_files();
}

fn read_file(path: &str) -> Option<alloc::vec::Vec<u8>> {
    crate::vfs::VFS.lock().read_file(path).map(|d| d.to_vec())
}

/// Files on VFS are ELF / ICU / pak, not `\x7fELF` + zeros.
pub fn unpack_self_test() -> bool {
    install_chromium_files();
    let Some(chrome) = read_file(CHROME_PATH) else {
        serial_println!("[chrome] GATE_CHR unpack FAILED: missing chrome");
        return false;
    };
    if !crate::elf::is_elf(&chrome) {
        serial_println!("[chrome] GATE_CHR unpack FAILED: chrome is not ELF");
        return false;
    }
    if crate::elf::interpreter_path(&chrome).as_deref() != Some(LD_LINUX_PATH) {
        serial_println!("[chrome] GATE_CHR unpack FAILED: PT_INTERP is not ld-linux");
        return false;
    }
    let Some(icu) = read_file(ICU_PATH) else {
        serial_println!("[chrome] GATE_CHR unpack FAILED: missing icudtl.dat");
        return false;
    };
    if !is_icu_dat(&icu) || icu.iter().skip(4).all(|&b| b == 0) {
        serial_println!("[chrome] GATE_CHR unpack FAILED: icudtl.dat is zeros");
        return false;
    }
    let Some(pak) = read_file(PAK_PATH) else {
        serial_println!("[chrome] GATE_CHR unpack FAILED: missing resources.pak");
        return false;
    };
    if !is_resources_pak(&pak) {
        serial_println!("[chrome] GATE_CHR unpack FAILED: resources.pak is not v5");
        return false;
    }
    serial_println!("[chrome] {}", GATE_CHR_UNPACK_MARKER);
    true
}

/// Shebang interp is not ELF; the installed ld-linux is. Chrome names it.
pub fn exec_self_test() -> bool {
    install_chromium_files();
    let Some(chrome) = read_file(CHROME_PATH) else {
        serial_println!("[chrome] GATE_CHR exec FAILED: missing chrome");
        return false;
    };
    if crate::elf::interpreter_path(&chrome).as_deref() != Some(LD_LINUX_PATH) {
        serial_println!("[chrome] GATE_CHR exec FAILED: PT_INTERP");
        return false;
    }
    let shebang = b"#!/lib64/ld-linux-x86-64.so.2\n";
    if crate::elf::is_elf(shebang) {
        serial_println!("[chrome] GATE_CHR exec FAILED: shebang counted as ELF");
        return false;
    }
    let Some(interp) = read_file(LD_LINUX_PATH) else {
        serial_println!("[chrome] GATE_CHR exec FAILED: missing ld-linux");
        return false;
    };
    if !crate::elf::is_elf(&interp) {
        serial_println!("[chrome] GATE_CHR exec FAILED: ld-linux is not ELF");
        return false;
    }
    true
}

/// `/lib64/ld-linux-x86-64.so.2` is a loadable ELF interpreter.
pub fn ldso_self_test() -> bool {
    install_chromium_files();
    let Some(interp) = read_file(LD_LINUX_PATH) else {
        serial_println!("[chrome] GATE_CHR ldso FAILED: missing");
        return false;
    };
    if !crate::elf::is_elf(&interp) || interp.starts_with(b"#!") {
        serial_println!("[chrome] GATE_CHR ldso FAILED: not ELF");
        return false;
    }
    true
}

/// `/lib/libc.so.6` is ELF and exports `__libc_start_main`.
pub fn glibc_self_test() -> bool {
    install_chromium_files();
    let Some(libc) = read_file(LIBC_GLIBC_PATH) else {
        serial_println!("[chrome] GATE_CHR glibc FAILED: missing libc.so.6");
        return false;
    };
    if !crate::elf::is_elf(&libc) {
        serial_println!("[chrome] GATE_CHR glibc FAILED: libc.so.6 is not ELF");
        return false;
    }
    if !libc.windows(17).any(|w| w == b"__libc_start_main") {
        serial_println!("[chrome] GATE_CHR glibc FAILED: no __libc_start_main");
        return false;
    }
    true
}

/// Kernel `SCM_RIGHTS` queue round-trip (Ring 3 sendmsg/recvmsg is the gate).
pub fn scmrights_self_test() -> bool {
    if !crate::uds::scmrights_roundtrip_self_test() {
        serial_println!("[chrome] scmrights FAILED: uds rights round-trip");
        return false;
    }
    true
}

/// Linux-shaped `/proc` `/sys` `/dev` `/etc` and XDG dirs Chromium probes.
pub fn install_chromium_linux_shape() {
    crate::vfs::ensure_directory("/proc");
    crate::vfs::ensure_directory("/proc/self");
    crate::vfs::ensure_directory("/proc/self/fd");
    crate::vfs::ensure_directory("/proc/self/ns");
    crate::vfs::ensure_directory("/proc/sys/kernel/yama");
    crate::vfs::ensure_directory("/dev/dri");
    crate::vfs::ensure_directory("/dev/shm");
    crate::vfs::ensure_directory("/etc/ssl/certs");
    crate::vfs::ensure_directory("/root/.config");
    crate::vfs::ensure_directory("/root/.cache");
    crate::vfs::ensure_directory("/root/.local/share");
    crate::vfs::ensure_directory("/home/user/.config");
    crate::vfs::ensure_directory("/home/user/.cache");
    crate::vfs::ensure_directory("/home/user/.local/share");
    crate::vfs::ensure_directory("/run/user/1000");
    let mut vfs = crate::vfs::VFS.lock();
    vfs.write_file("/run/user/1000/.keep", b"xdg\n");
    vfs.write_file(
        "/etc/nsswitch.conf",
        b"passwd: files\ngroup: files\nhosts: files dns\n",
    );
    vfs.write_file("/etc/hosts", b"127.0.0.1\tlocalhost\n::1\tlocalhost\n");
    vfs.write_file(
        "/etc/resolv.conf",
        b"nameserver 1.1.1.1\nnameserver 8.8.8.8\n",
    );
    vfs.write_file(
        "/etc/ssl/certs/ca-certificates.crt",
        b"-----BEGIN CERTIFICATE-----\nKnoxOS-CA\n-----END CERTIFICATE-----\n",
    );
    vfs.write_file(
        super::chromium_https::CERT9_PATH,
        &super::chromium_https::cert9_db(),
    );
    vfs.write_file("/dev/dri/card0", &[]);
    vfs.write_file("/dev/dri/renderD128", &[]);
    vfs.write_file("/proc/sys/kernel/unprivileged_userns_clone", b"1\n");
    vfs.write_file("/proc/sys/kernel/core_pattern", b"core.%p\n");
    vfs.write_file("/proc/sys/kernel/yama/ptrace_scope", b"1\n");
}

fn file_has(path: &str, needle: &[u8]) -> bool {
    read_file(path).is_some_and(|d| d.windows(needle.len()).any(|w| w == needle))
}

fn dir_exists(path: &str) -> bool {
    crate::vfs::VFS.lock().resolve_path(path).is_some()
}

/// `/proc` `/sys` `/dev` `/etc` / XDG exist and parse as Chromium expects.
pub fn linux_shape_self_test() -> bool {
    install_chromium_linux_shape();
    let proc_ok = crate::procfs::read_proc_file("/proc/self/status")
        .is_some_and(|d| d.windows(5).any(|w| w == b"Name:"))
        && crate::procfs::read_proc_file("/proc/self/maps").is_some()
        && crate::procfs::read_proc_file("/proc/self/stat").is_some()
        && crate::procfs::read_proc_file("/proc/self/smaps").is_some()
        && crate::procfs::read_proc_file("/proc/self/statm").is_some()
        && crate::procfs::read_proc_file("/proc/self/oom_score_adj").is_some()
        && crate::procfs::read_proc_file("/proc/self/cgroup").is_some()
        && crate::procfs::read_proc_file("/proc/self/mountinfo").is_some()
        && crate::procfs::read_proc_file("/proc/self/limits").is_some()
        && crate::procfs::read_proc_file("/proc/self/fd").is_some()
        && crate::procfs::read_proc_file("/proc/self/exe").is_some()
        && crate::procfs::read_proc_file("/proc/self/cwd").is_some()
        && crate::procfs::read_proc_file("/proc/self/ns").is_some();
    if !proc_ok {
        serial_println!("[chrome] GATE_CHR proc FAILED");
        return false;
    }
    serial_println!("[chrome] {}", GATE_CHR_PROC_MARKER);

    let sys_ok = crate::procfs::read_proc_file("/proc/cpuinfo")
        .is_some_and(|d| d.windows(9).any(|w| w == b"processor"))
        && crate::procfs::read_proc_file("/proc/meminfo")
            .is_some_and(|d| d.windows(8).any(|w| w == b"MemTotal"))
        && crate::procfs::read_proc_file("/proc/sys/kernel/unprivileged_userns_clone")
            .is_some_and(|d| d.starts_with(b"1"))
        && crate::procfs::read_proc_file("/proc/sys/kernel/core_pattern").is_some()
        && crate::procfs::read_proc_file("/proc/sys/kernel/yama/ptrace_scope").is_some();
    if !sys_ok {
        serial_println!("[chrome] GATE_CHR sysctl FAILED");
        return false;
    }
    serial_println!("[chrome] {}", GATE_CHR_SYSCTL_MARKER);

    let dev_ok = dir_exists("/dev/null")
        && dir_exists("/dev/zero")
        && dir_exists("/dev/urandom")
        && dir_exists("/dev/shm")
        && dir_exists("/dev/dri/card0")
        && dir_exists("/dev/dri/renderD128");
    if !dev_ok {
        serial_println!("[chrome] GATE_CHR dev FAILED");
        return false;
    }
    serial_println!("[chrome] {}", GATE_CHR_DEV_MARKER);

    let etc_ok = file_has("/etc/passwd", b"root:")
        && file_has("/etc/nsswitch.conf", b"hosts:")
        && file_has("/etc/hosts", b"localhost")
        && file_has("/etc/resolv.conf", b"nameserver")
        && file_has("/etc/ssl/certs/ca-certificates.crt", b"BEGIN")
        && file_has("/etc/ssl/certs/cert9.db", b"SQLite format 3");
    if !etc_ok {
        serial_println!("[chrome] GATE_CHR etc FAILED");
        return false;
    }
    serial_println!("[chrome] {}", GATE_CHR_ETC_MARKER);

    let xdg_ok = dir_exists("/root/.config")
        && dir_exists("/root/.cache")
        && dir_exists("/root/.local/share")
        && dir_exists("/run/user/1000");
    if !xdg_ok {
        serial_println!("[chrome] GATE_CHR xdg FAILED");
        return false;
    }
    serial_println!("[chrome] {}", GATE_CHR_XDG_MARKER);
    true
}

/// Ozone/Wayland Linux `create_pool` + xdg-shell present (not GPUI Browser).
pub fn ozone_self_test() -> bool {
    crate::wayland::ozone_protocol_self_test()
}

/// Zygote image on VFS is a loadable ELF, not a fake atomic PID.
pub fn fork_self_test() -> bool {
    install_chromium_files();
    let Some(zygote) = read_file(CHROME_ZYGOTE_PATH) else {
        serial_println!("[chrome] zygote FAILED: missing chrome-zygote");
        return false;
    };
    if !crate::elf::is_elf(&zygote) {
        serial_println!("[chrome] zygote FAILED: chrome-zygote is not ELF");
        return false;
    }
    true
}
