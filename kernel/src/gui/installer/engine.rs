use core::sync::atomic::Ordering;

use super::state::{INSTALL_COMPLETE, INSTALL_PROGRESS, INSTALLER_STATE, close_installer};
use super::types::{BootloaderType, FsType, TIMEZONES};

// ─── Installation Engine ─────────────────────────────────────────────

pub(super) fn begin_installation() {
    INSTALL_PROGRESS.store(1, Ordering::Relaxed);

    {
        INSTALLER_STATE
            .lock()
            .append_log("[1/8] Partitioning disk...");
    }
    partition_disk();
    INSTALL_PROGRESS.store(15, Ordering::Relaxed);

    {
        INSTALLER_STATE
            .lock()
            .append_log("[2/8] Formatting partitions...");
    }
    format_partitions();
    INSTALL_PROGRESS.store(25, Ordering::Relaxed);

    {
        INSTALLER_STATE
            .lock()
            .append_log("[3/8] Mounting target...");
    }
    mount_target();
    INSTALL_PROGRESS.store(30, Ordering::Relaxed);

    {
        INSTALLER_STATE
            .lock()
            .append_log("[4/8] Copying system files...");
    }
    copy_system_files();
    INSTALL_PROGRESS.store(65, Ordering::Relaxed);

    {
        INSTALLER_STATE
            .lock()
            .append_log("[5/8] Installing bootloader...");
    }
    install_bootloader();
    INSTALL_PROGRESS.store(78, Ordering::Relaxed);

    {
        INSTALLER_STATE
            .lock()
            .append_log("[6/8] Creating user account...");
    }
    create_user_account();
    INSTALL_PROGRESS.store(85, Ordering::Relaxed);

    {
        INSTALLER_STATE
            .lock()
            .append_log("[7/8] Setting timezone & locale...");
    }
    configure_system();
    INSTALL_PROGRESS.store(95, Ordering::Relaxed);

    {
        INSTALLER_STATE.lock().append_log("[8/8] Finalizing...");
    }
    finalize_install();
    INSTALL_PROGRESS.store(100, Ordering::Relaxed);
    INSTALL_COMPLETE.store(true, Ordering::Relaxed);

    {
        INSTALLER_STATE
            .lock()
            .append_log("Installation complete! You may now reboot.");
    }
}

fn partition_disk() {
    let _st = INSTALLER_STATE.lock();
    crate::vfs::ensure_directory("/tmp/install");
    crate::vfs::create_file_dispatch(
        "/tmp/install/partitions.conf",
        b"# GPT Partition Table\nESP: 512M FAT32\nSWAP: 4G\nROOT: remaining\n",
    );
}

fn format_partitions() {
    let fs = INSTALLER_STATE.lock().root_fs;
    let label = match fs {
        FsType::Ext4 => "mkfs.ext4 /dev/disk_root",
        FsType::Btrfs => "mkfs.btrfs /dev/disk_root",
        FsType::Xfs => "mkfs.xfs /dev/disk_root",
        FsType::Fat32 => "mkfs.fat -F32 /dev/disk_root",
    };
    crate::vfs::ensure_directory("/tmp/install");
    crate::vfs::create_file_dispatch("/tmp/install/format.log", label.as_bytes());
}

fn mount_target() {
    crate::vfs::ensure_directory("/mnt/target");
    crate::vfs::ensure_directory("/mnt/target/boot");
    crate::vfs::ensure_directory("/mnt/target/boot/efi");
    crate::vfs::ensure_directory("/mnt/target/etc");
    crate::vfs::ensure_directory("/mnt/target/home");
    crate::vfs::ensure_directory("/mnt/target/root");
    crate::vfs::ensure_directory("/mnt/target/usr");
    crate::vfs::ensure_directory("/mnt/target/var");
}

fn copy_system_files() {
    crate::vfs::create_file_dispatch("/mnt/target/boot/knoxos", b"[kernel binary]");

    let st = INSTALLER_STATE.lock();
    let hostname = &st.hostname[..st.hostname_len];
    crate::vfs::create_file_dispatch("/mnt/target/etc/hostname", hostname);

    let fstab = match st.root_fs {
        FsType::Ext4 => {
            "/dev/root  /  ext4  defaults  0  1\n/dev/efi  /boot/efi  vfat  defaults  0  2\n"
        }
        FsType::Btrfs => {
            "/dev/root  /  btrfs  defaults,compress=zstd  0  1\n/dev/efi  /boot/efi  vfat  defaults  0  2\n"
        }
        FsType::Xfs => {
            "/dev/root  /  xfs  defaults  0  1\n/dev/efi  /boot/efi  vfat  defaults  0  2\n"
        }
        FsType::Fat32 => "/dev/root  /  vfat  defaults  0  1\n",
    };
    drop(st);
    crate::vfs::create_file_dispatch("/mnt/target/etc/fstab", fstab.as_bytes());
}

fn install_bootloader() {
    let bl = INSTALLER_STATE.lock().bootloader;
    match bl {
        BootloaderType::Grub => {
            let cfg = "set timeout=5\nset default=0\n\nmenuentry 'KnoxOS' {\n    linux /boot/knoxos root=/dev/root\n}\n";
            crate::vfs::ensure_directory("/mnt/target/boot/grub");
            crate::vfs::create_file_dispatch("/mnt/target/boot/grub/grub.cfg", cfg.as_bytes());
        }
        BootloaderType::SystemdBoot => {
            crate::vfs::ensure_directory("/mnt/target/boot/loader/entries");
            crate::vfs::create_file_dispatch(
                "/mnt/target/boot/loader/loader.conf",
                b"default knoxos\ntimeout 5\n",
            );
            crate::vfs::create_file_dispatch(
                "/mnt/target/boot/loader/entries/knoxos.conf",
                b"title KnoxOS\nlinux /boot/knoxos\n",
            );
        }
    }
}

fn create_user_account() {
    let st = INSTALLER_STATE.lock();

    // Home directory
    let mut home_path = [0u8; 128];
    let prefix = b"/mnt/target/home/";
    home_path[..prefix.len()].copy_from_slice(prefix);
    home_path[prefix.len()..prefix.len() + st.username_len]
        .copy_from_slice(&st.username[..st.username_len]);
    let home_str = core::str::from_utf8(&home_path[..prefix.len() + st.username_len])
        .unwrap_or("/mnt/target/home/user");
    crate::vfs::ensure_directory(home_str);

    // /etc/passwd
    let mut line = [0u8; 256];
    let mut pos = 0usize;
    for &b in st.username[..st.username_len].iter() {
        line[pos] = b;
        pos += 1;
    }
    let mid = b":x:1000:1000:";
    line[pos..pos + mid.len()].copy_from_slice(mid);
    pos += mid.len();
    for &b in st.full_name[..st.full_name_len].iter() {
        if pos < line.len() {
            line[pos] = b;
            pos += 1;
        }
    }
    let home_s = b":/home/";
    if pos + home_s.len() < line.len() {
        line[pos..pos + home_s.len()].copy_from_slice(home_s);
        pos += home_s.len();
    }
    for &b in st.username[..st.username_len].iter() {
        if pos < line.len() {
            line[pos] = b;
            pos += 1;
        }
    }
    let shell = b":/bin/sh\n";
    if pos + shell.len() < line.len() {
        line[pos..pos + shell.len()].copy_from_slice(shell);
        pos += shell.len();
    }

    crate::vfs::create_file_dispatch("/mnt/target/etc/passwd", &line[..pos]);
}

fn configure_system() {
    let tz = TIMEZONES[INSTALLER_STATE.lock().timezone_idx];
    crate::vfs::create_file_dispatch("/mnt/target/etc/timezone", tz.as_bytes());
    crate::vfs::create_file_dispatch("/mnt/target/etc/locale.conf", b"LANG=en_US.UTF-8\n");
    crate::vfs::ensure_directory("/mnt/target/etc/kernel");
    crate::vfs::create_file_dispatch(
        "/mnt/target/etc/kernel/cmdline",
        b"root=/dev/root quiet splash\n",
    );
}

fn finalize_install() {
    crate::vfs::create_file_dispatch(
        "/mnt/target/etc/knoxos-release",
        b"NAME=\"KnoxOS\"\nVERSION=\"0.2.6\"\nID=knoxos\n",
    );
    let mut st = INSTALLER_STATE.lock();
    st.append_log("Syncing disk caches...");
    st.append_log("Unmounting target filesystems...");
}

pub(super) fn do_reboot() {
    close_installer();
    // ACPI reboot: pulse reset line via keyboard controller port 0x64
    #[cfg(target_arch = "x86_64")]
    // SAFETY: inline assembly uses the architecture instruction and listed clobbers; no unexpected memory is accessed.
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") 0x64u16,
            in("al") 0xFEu8,
            options(nomem, nostack)
        );
    }
}
