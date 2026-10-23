use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;

use super::disks::probe_disks;
use super::engine::{begin_installation, do_reboot};
use super::nav::{go_back, go_next};
use super::state::{
    CURRENT_STEP, INSTALL_COMPLETE, INSTALL_PROGRESS, INSTALLER_ACTIVE, INSTALLER_STATE,
    InstallerState,
};
use super::types::{BootloaderType, FsType, InstallSnap, STEP_COUNT, STEP_NAMES, TIMEZONES};

/// Launch the graphical installer
pub fn launch_installer() {
    {
        let mut st = INSTALLER_STATE.lock();
        *st = InstallerState::new_const();
        probe_disks(&mut st);
    }
    CURRENT_STEP.store(0, Ordering::Relaxed);
    INSTALL_PROGRESS.store(0, Ordering::Relaxed);
    INSTALL_COMPLETE.store(false, Ordering::Relaxed);
    INSTALLER_ACTIVE.store(true, Ordering::Relaxed);
}

pub fn snapshot() -> InstallSnap {
    let cur = CURRENT_STEP.load(Ordering::Relaxed) as usize;
    let complete = INSTALL_COMPLETE.load(Ordering::Relaxed);
    let progress = INSTALL_PROGRESS.load(Ordering::Relaxed);
    let st = INSTALLER_STATE.lock();
    let (body, options, selected) = match cur {
        0 => (
            String::from("Welcome to the KnoxOS installer."),
            Vec::new(),
            None,
        ),
        1 => {
            let mut opts = Vec::new();
            for i in 0..st.disk_count {
                let n = core::str::from_utf8(&st.disks[i].name[..st.disks[i].name_len])
                    .unwrap_or("disk");
                opts.push(format!("{} ({} MB)", n, st.disks[i].size_mb));
            }
            if opts.is_empty() {
                opts.push(String::from("No disk"));
            }
            (String::from("Select a disk."), opts, Some(st.selected_disk))
        }
        2 => (
            String::from("Filesystem"),
            alloc::vec![
                String::from("ext4"),
                String::from("btrfs"),
                String::from("XFS")
            ],
            Some(match st.root_fs {
                FsType::Ext4 => 0,
                FsType::Btrfs => 1,
                _ => 2,
            }),
        ),
        3 => (
            String::from("Bootloader"),
            alloc::vec![String::from("GRUB 2"), String::from("systemd-boot")],
            Some(match st.bootloader {
                BootloaderType::Grub => 0,
                BootloaderType::SystemdBoot => 1,
            }),
        ),
        4 => (
            String::from("Create a user account (keyboard in the next installer)."),
            Vec::new(),
            None,
        ),
        5 => (
            String::from("Timezone"),
            TIMEZONES.iter().map(|t| String::from(*t)).collect(),
            Some(st.timezone_idx),
        ),
        _ => {
            let log = core::str::from_utf8(&st.install_log[..st.install_log_len]).unwrap_or("");
            (
                if complete {
                    String::from("Installation complete.")
                } else if log.is_empty() {
                    String::from("Ready to install.")
                } else {
                    String::from(log)
                },
                Vec::new(),
                None,
            )
        }
    };
    let next_label = if complete {
        String::from("Reboot")
    } else if cur == STEP_COUNT - 1 {
        String::from("Install")
    } else {
        String::from("Next")
    };
    InstallSnap {
        step: cur,
        title: String::from(STEP_NAMES[cur.min(STEP_COUNT - 1)]),
        body,
        options,
        selected,
        next_label,
        can_back: cur > 0 && !complete,
        progress,
    }
}

pub fn next_step() {
    let cur = CURRENT_STEP.load(Ordering::Relaxed) as usize;
    if INSTALL_COMPLETE.load(Ordering::Relaxed) {
        do_reboot();
        return;
    }
    if cur == STEP_COUNT - 1 {
        if INSTALL_PROGRESS.load(Ordering::Relaxed) == 0 {
            begin_installation();
        }
    } else {
        go_next();
    }
}

pub fn back_step() {
    go_back();
}

pub fn pick(i: usize) {
    let cur = CURRENT_STEP.load(Ordering::Relaxed);
    let mut st = INSTALLER_STATE.lock();
    match cur {
        1 if i < st.disk_count => st.selected_disk = i,
        2 => {
            st.root_fs = match i {
                1 => FsType::Btrfs,
                2 => FsType::Xfs,
                _ => FsType::Ext4,
            };
        }
        3 => {
            st.bootloader = if i == 1 {
                BootloaderType::SystemdBoot
            } else {
                BootloaderType::Grub
            };
        }
        5 if i < TIMEZONES.len() => st.timezone_idx = i,
        _ => {}
    }
}
